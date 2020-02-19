//! Replays event streams from Timely / Differential
//! and constructs a stream of LogRecords from them.
#![deny(missing_docs)]

#[macro_use]
extern crate log;

pub mod connect;
use crate::connect::{Replayer, CompEvent};
pub mod replay_throttled;
use crate::replay_throttled::ReplayThrottled;

use st2_logformat::{ActivityType, EventType, LogRecord};
use st2_logformat::pair::Pair;

use std::io::Read;
use std::time::Duration;
use std::collections::HashMap;

use timely::{
    dataflow::{
        channels::pact::Pipeline,
        operators::generic::operator::Operator,
        Scope, Stream,
    },
    logging::{
        StartStop,
        TimelyEvent::{Messages, Operates, Progress, Schedule},
    },
};

/// Returns a `Stream` of `LogRecord`s that can be used for PAG construction.
pub fn create_lrs<S, R>(
    scope: &mut S,
    replayers: Vec<Replayer<S::Timestamp, R>>,
    index: usize,
    throttle: u64,
) -> Stream<S, LogRecord>
where
    S: Scope<Timestamp = Pair<u64, Duration>>,
    R: Read + 'static,
{
    replayers
        .replay_throttled_into(index, scope, None, throttle)
        .construct_lrs(index)
}

/// Operator that converts a Stream of TimelyEvents to their LogRecord representation
pub trait ConstructLRs<S: Scope<Timestamp = Pair<u64, Duration>>> {
    /// Constructs a stream of log records to be used in PAG construction from an event stream.
    fn construct_lrs(&self, index: usize) -> Stream<S, LogRecord>;
    /// Strips an event `Stream` of encompassing operators
    /// (e.g. the dataflow operator for every direct child,
    /// the surrounding iterate operators for loops).
    fn peel_ops(&self, index: usize) -> Stream<S, CompEvent>;
    /// Makes a stream of log records from an event stream.
    fn make_lrs(&self, index: usize) -> Stream<S, LogRecord>;
    /// Builds a log record at differential time `time` from the supplied computation event.
    fn build_lr(comp_event: CompEvent, prev_ts: Option<Duration>, start: bool, end: bool) -> Option<LogRecord>;
}

impl<S: Scope<Timestamp = Pair<u64, Duration>>> ConstructLRs<S> for Stream<S, CompEvent>
{
    fn construct_lrs(&self, index: usize) -> Stream<S, LogRecord> {
        self.peel_ops(index)
            .make_lrs(index)
    }

    fn peel_ops(&self, _index: usize) -> Stream<S, CompEvent> {
        let mut vector = Vec::new();
        let mut outer_operates = std::collections::BTreeSet::new();
        let mut ids_to_addrs = std::collections::HashMap::new();

        self.unary(Pipeline, "Peel", move |_, _| { move |input, output| {
            input.for_each(|cap, data| {
                data.swap(&mut vector);
                for (epoch, seq_no, length, (t, wid, x)) in vector.drain(..) {
                    match x {
                        Operates(e) => {
                            if wid == 0 {
                                // Dataflow structure logging
                                info!("{:?}", e);
                            }
                            let mut addr = e.addr.clone();
                            addr.pop();
                            outer_operates.insert(addr);

                            ids_to_addrs.insert(e.id, e.addr);
                        }
                        Schedule(ref e) => {
                            assert!(cap.time() > &Pair::new(0, Default::default()));

                            // @TODO: For LBF > 1, we might not have seen all `Operates` events
                            // at all workers, so this fails.
                            let addr = ids_to_addrs.get(&e.id).expect("operates went wrong");
                            if !outer_operates.contains(addr) {
                                output.session(&cap).give((epoch, seq_no, length, (t, wid, x)));
                            }
                        }
                        _ => {
                            assert!(cap.time() > &Pair::new(0, Default::default()));

                            output.session(&cap).give((epoch, seq_no, length, (t, wid, x)));
                        }
                    }
                }
            });
        }})
    }

    fn make_lrs(&self, _index: usize) -> Stream<S, LogRecord> {
        let mut curr_epoch = HashMap::new();
        let mut at_first = HashMap::new();
        let mut vector = Vec::new();
        let mut at_last = false;

        self.unary(Pipeline, "LogRecordConstruct", move |_, _| { move |input, output| {
            input.for_each(|cap, data| {
                data.swap(&mut vector);

                let mut vec_iter = vector.drain(..).peekable();
                loop {
                    if let Some(x) = vec_iter.next() {
                        let wid = (x.3).1;

                        let ep = curr_epoch.entry(wid as usize).or_insert(0);
                        if x.0 > *ep {
                            *ep = x.0;
                            at_first.insert(wid, true);
                        } else {
                            let next = vec_iter.peek();
                            if let Some(n) = next {
                                // assert!((n.3).1 == wid);

                                if ((n.3).1 == wid) && x.0 < n.0 {
                                    at_last = true;
                                }
                            } else {
                                at_last = true;
                            }
                        }

                        let at_f = at_first.entry(wid as usize).or_insert(true);

                        // Might need to reorder data message receivals
                        // TODO this enforces -w == -s
                        if let Some(next) = vec_iter.peek() {
                            if let Messages(event) = &(next.3).2 {
                                if !event.is_send {
                                    let lr = if *at_f {
                                        *at_f = false;
                                        Self::build_lr(next.clone(), Some((x.3).0), true, false).into_iter()
                                    } else {
                                        if at_last {
                                            at_last = false;
                                            Self::build_lr(next.clone(), Some((x.3).0), false, true).into_iter()
                                        } else {
                                            Self::build_lr(next.clone(), Some((x.3).0), false, false).into_iter()
                                        }
                                    };

                                    output.session(&cap).give_iterator(lr);

                                    // Skip next - we've already built it
                                    vec_iter.next();
                                }
                            }
                        }

                        let lr = if *at_f {
                            *at_f = false;
                            Self::build_lr(x, None, true, false).into_iter()
                        } else {
                            if at_last {
                                at_last = false;
                                Self::build_lr(x, None, false, true).into_iter()
                            } else {
                                Self::build_lr(x, None, false, false).into_iter()
                            }
                        };
                        output.session(&cap).give_iterator(lr);

                    } else {
                        break
                    }
                };
            });
        }})
    }

    fn build_lr(comp_event: CompEvent, prev_ts: Option<Duration>, start: bool, end: bool) -> Option<LogRecord> {
        let (epoch, seq_no, length, (timestamp, wid, x)) = comp_event;
        let local_worker = wid as u64;

        match x {
            // Scheduling & Processing
            Schedule(event) => {
                let event_type = if event.start_stop == StartStop::Start {
                    EventType::Start
                } else {
                    EventType::End
                };

                Some(LogRecord {
                    seq_no,
                    epoch,
                    timestamp,
                    local_worker,
                    activity_type: ActivityType::Scheduling,
                    event_type,
                    remote_worker: None,
                    operator_id: Some(event.id as u64),
                    channel_id: None,
                    correlator_id: None,
                    length,
                    start,
                    end
                })
            }
            // remote data messages
            Messages(event) => {
                assert!(length.unwrap() == event.length);

                let remote_worker = if event.is_send {
                    Some(event.target as u64)
                } else {
                    Some(event.source as u64)
                };

                let event_type = if event.is_send {
                    EventType::Sent
                } else {
                    EventType::Received
                };


                let timestamp = if event.is_send {
                        timestamp
                } else {
                    if let Some(ts) = prev_ts {
                        ts
                    } else {
                        timestamp
                    }
                };

                Some(LogRecord {
                    seq_no,
                    epoch,
                    timestamp,
                    local_worker,
                    activity_type: ActivityType::DataMessage,
                    event_type,
                    remote_worker,
                    operator_id: None,
                    channel_id: Some(event.channel as u64),
                    correlator_id: Some(event.seq_no as u64),
                    length,
                    start,
                    end
                })
            }
            // Control Messages
            Progress(event) => {
                let event_type = if event.is_send {
                    EventType::Sent
                } else {
                    EventType::Received
                };

                let remote_worker = if event.is_send {
                    // Outgoing progress messages are broadcasts, so we don't know
                    // where they'll end up.
                    None
                } else {
                    Some(event.source as u64)
                };

                Some(LogRecord {
                    seq_no,
                    epoch,
                    timestamp,
                    local_worker,
                    activity_type: ActivityType::ControlMessage,
                    event_type,
                    remote_worker,
                    operator_id: None,
                    channel_id: Some(event.channel as u64),
                    correlator_id: Some(event.seq_no as u64),
                    length: None,
                    start,
                    end,
                })
            }
            // Channels / Operates events
            _ => None
        }
    }
}

// let mut vector = Vec::new();
// .inner
// .unary_frontier(Pipeline, "Logger", move |_, _| { move |input, output| {
//     input.for_each(|cap, data| {
//         data.swap(&mut vector);
//         output.session(&cap).give_iterator(vector.drain(..));
//     });
//     // println!("ggg: {} --- {:?}", index, input.frontier().frontier().to_vec());
// }})
// .as_collection()
