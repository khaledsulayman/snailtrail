use timely::dataflow::InputHandle;
use timely::dataflow::operators::{Map, Input, Exchange, Inspect, Probe};
use timely::dataflow::operators::filter::Filter;

use st2_timely::connect::Adapter;

fn main() {
    timely::execute_from_args(std::env::args(), |worker| {
        // (A) Create SnailTrail adapter at the beginning of the worker closure
        let adapter = Adapter::attach(worker);

        // Some computation
        let mut input = InputHandle::new();
        let probe = worker.dataflow(|scope|
            scope.input_from(&mut input)
                 .filter(|x| x % 2 == 0)
                 // .inspect_batch(|_,_| std::thread::sleep_ms(500))
                 .exchange(|_| 0) // skewed
                 .map(|x| x + 1)
                 .probe()
        );

        for round in 0..10 {
        // for round in 0..1000 {
            for x in 0 .. 2000 {
                input.send(x);
            }

            input.advance_to(round + 1);
            while probe.less_than(input.time()) {
                worker.step_or_park(None);
            }

            // (B) Communicate epoch completion
            adapter.tick_epoch();
        }
    }).unwrap();
}
