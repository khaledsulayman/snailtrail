use crate::pag;
use crate::pag::PagEdge;

// use timely::dataflow::ProbeHandle;
// use timely::dataflow::operators::probe::Probe;
use timely::dataflow::operators::inspect::Inspect;
use timely::dataflow::operators::map::Map;
use timely::dataflow::operators::filter::Filter;
use timely::dataflow::channels::pact::Pipeline;
// use timely::dataflow::operators::generic::OutputHandle;
use timely::dataflow::operators::generic::operator::Operator;
use timely::dataflow::Scope;
use timely::dataflow::Stream;
// use timely::Data;
use timely::dataflow::operators::exchange::Exchange;
use timely::dataflow::operators::aggregation::aggregate::Aggregate;
use timely::dataflow::operators::concat::Concat;

use std::time::Duration;
// use std::time::Instant;
use std::rc::Rc;
use std::collections::{VecDeque, HashMap};
use std::hash::Hash;
use std::collections::hash_map::DefaultHasher;
use std::hash::Hasher;

use st2_logformat::pair::Pair;
use st2_logformat::ActivityType;

use tdiag_connect::receive as connect;
use tdiag_connect::receive::ReplaySource;

use crate::STError;

/// Runs graph algorithms on ST2.
pub fn run(
    timely_configuration: timely::Configuration,
    replay_source: ReplaySource,
    with_waiting: bool) -> Result<(), STError> {

    timely::execute(timely_configuration, move |worker| {
        let index = worker.index();

        // read replayers from file (offline) or TCP stream (online)
        let readers = connect::make_readers(replay_source.clone(), worker.index(), worker.peers()).expect("couldn't create readers");

        worker.dataflow(|scope| {
            let pag: Stream<_, (PagEdge, Pair<u64, Duration>, isize)>  = pag::create_pag(scope, readers, index, 1);

            pag
                .cpmetric(with_waiting)
                .filter(|(_, c)| c > &1)
                .inspect_time(|t, (x, c)| println!("CP {} | ep {} | w{}@{:?} -> w{}@{:?}",
                                                   c, t.first - 1,
                                                   x.source.worker_id,
                                                   x.source.timestamp,
                                                   x.destination.worker_id,
                                                   x.destination.timestamp));
        });
    })
        .map_err(|x| STError(format!("error in the timely computation: {}", x)))?;

    Ok(())
}

/// Hashes an arbitrary value.
pub fn hash_code<T: Hash>(value: &T) -> u64 {
    let mut hasher = DefaultHasher::new();
    value.hash(&mut hasher);
    hasher.finish()
}

/// Trait describing the operation `self * other * multiplicand`.
pub trait ScaleReduce {
    /// Reduce the value of `self` with `other` and scale it with `multiplicand`.
    fn scale_reduce(self, other: Self, multiplicand: usize) -> Self;
}

impl ScaleReduce for u64 {
    fn scale_reduce(self, other: u64, mult: usize) -> Self {
        self * other * mult as u64
    }
}

/// Run CP metric on provided `Stream`.
pub trait CPMetric<S: Scope<Timestamp = Pair<u64, Duration>>> {
    /// Compute CP Metric
    fn cpmetric(&self, with_waiting: bool) -> Stream<S, (PagEdge, u64)>;
}

impl<S: Scope<Timestamp = Pair<u64, Duration>>> CPMetric<S> for Stream<S, (PagEdge, S::Timestamp, isize)>{
    fn cpmetric(&self, with_waiting: bool) -> Stream<S, (PagEdge, u64)> {
        // We do not want to traverse Waiting edges so remove them from the PAG
        let edges = self
            .map(|(edge, _t, _diff)| edge)
            .filter(move |edge| with_waiting || edge.edge_type != ActivityType::Waiting)
            .exchange(|edge| edge.source.epoch);

        let forward = edges
            .filter(|edge| edge.source.start)
            .map(|edge| (edge, From::from(1u64)));

        let backward = edges
            .filter(|edge| edge.destination.end)
            .map(|edge| (edge, From::from(1u64)));

        let output = edges.group_explore(&forward, "CP Forward", true);
        let output2 = edges.group_explore(&backward, "CP Backward", false);
        let combined = output.concat(&output2);
            // .inspect(|x| println!("explore: {:?}", x));

        // Compute betweeness centrality
        combined.aggregate::<_,Vec<u64>,_,_,_>(
            |_key, val, agg| agg.push(val),
            |key, mut agg| {
                agg.sort_by(|a, b| a.partial_cmp(b).unwrap());
                match agg.len() {
                    1 => (key, agg[0]),
                    n => {
                        // Check for even number of edges.
                        // This is only partially correct and won't detect when there's an even number of edges from one side only!
                        // For this to work, we would need to know the direction of the edge.
                        // Idea: Map the output to (edge, (direction, count)) -MH
                        // assert_eq!(0, n & 1, "Wrong number of output tuples, n={}, agg={:?}, key={:?}!", n, agg, key);
                        (key, (agg[0].scale_reduce(agg[n/2], n / 2)))
                    },
                }
            },
            |key| hash_code(key))
    }
}

#[derive(Debug)]
struct NodeInfo<D, DO> {
    in_degree: u32,
    in_sum: DO,
    outgoing: Vec<D>,
}

impl<'a, D, DO: Default> NodeInfo<D, DO> {
    fn new() -> Self {
        NodeInfo {
            outgoing: vec![],
            in_degree: 0,
            in_sum: Default::default(),
        }
    }
}

/// A trait defining an interface to explore a graph.
pub trait GroupExplore<S: Scope<Timestamp = Pair<u64, Duration>>> {
    /// Explores a graph iteratively based on a frontier stream. The graph is exepcted to be
    /// provided by the implementation. Each edge in the input graph is joined with the frontier.
    fn group_explore(&self,
                     frontier_stream: &Stream<S, (PagEdge, u64)>,
                     name: &str,
                     forward: bool) -> Stream<S, (PagEdge, u64)>;
}


impl<S: Scope<Timestamp = Pair<u64, Duration>>> GroupExplore<S> for Stream<S, PagEdge>{
    fn group_explore(&self,
                     frontier_stream: &Stream<S, (PagEdge, u64)>,
                     name: &str,
                     forward:bool) -> Stream<S, (PagEdge, u64)> {
        let join = Rc::new(move |e: &PagEdge| if forward {
            e.destination
        } else {
            e.source
        });
        let group = Rc::new(move |e: &PagEdge| if forward {
            e.source
        } else {
            e.destination
        });

        let mut node_data = HashMap::new();
        let mut frontier_stash = HashMap::new();

        let mut graph_vector = Vec::new();
        let mut frontier_vector = Vec::new();

        self.binary_notify(frontier_stream, Pipeline, Pipeline, name, vec![], move |graph, frontier, output, notificator| {
            graph.for_each(|time, data| {
                data.swap(&mut graph_vector);

                for datum in graph_vector.drain(..) {
                    // source
                    let node_info = node_data
                        .entry(datum.source.epoch)
                        .or_insert_with(HashMap::new)
                        .entry(group(&datum))
                        .or_insert_with(NodeInfo::new);
                    node_info.outgoing.push(datum.clone());

                    // destination
                    let node_info = node_data
                        .entry(datum.source.epoch)
                        .or_insert_with(HashMap::new)
                        .entry(join(&datum))
                        .or_insert_with(NodeInfo::new);
                    node_info.in_degree += 1;
                }

                let time = time.delayed(&Pair::new(time.time().first + 1, Default::default()));
                notificator.notify_at(time.clone());
            });

            frontier.for_each(|time, data| {
                data.swap(&mut frontier_vector);
                for datum in frontier_vector.drain(..) {
                    frontier_stash
                        .entry(datum.0.source.epoch)
                        .or_insert_with(VecDeque::new)
                        .push_back(datum);
                }

                notificator.notify_at(time.delayed(&Pair::new(time.time().first + 1, Default::default())));
            });

            notificator.for_each(|time, _, _| {
                let mut session = output.session(&time);

                let mut frontier_edges = frontier_stash.remove(&(time.first - 1)).unwrap_or_default();

                if let Some(data) = node_data.get_mut(&(time.first - 1)) {
                    while let Some(datum) = frontier_edges.pop_front() {
                        // destination
                        if let Some(node_info) = data.get_mut(&join(&datum.0)) {
                            // assert!(node_info.in_degree > 0,
                            //         "node_info: {:?} datum: {:?}",
                            //         node_info,
                            //         datum);
                            node_info.in_sum += datum.1;
                            if node_info.in_degree > 0 {
                                node_info.in_degree -= 1;
                            }

                            if node_info.in_degree == 0 {

                                for o in node_info.outgoing.clone() {
                                    session.give((o.clone(), node_info.in_sum));
                                    frontier_edges.push_back((o, node_info.in_sum));
                                }
                            }
                        }
                    }
                }

                // cleanup
                node_data.remove(&(time.first - 1));
            });
        })
    }
}
