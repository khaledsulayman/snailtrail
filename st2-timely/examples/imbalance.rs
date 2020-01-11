use timely::dataflow::InputHandle;
use timely::dataflow::operators::{Map, Input, Exchange, Inspect, Probe};
use timely::dataflow::operators::filter::Filter;

use st2_timely::connect::Adapter;

fn main() {
    // snag a filename to use for the input graph.
    let mut args = std::env::args();

    let _ = args.next(); // bin name
    let computation = args.next().expect("no computation given").parse::<usize>().unwrap();
    let _ = args.next(); // --

    let args = args.collect::<Vec<_>>();

    match computation {
        1 => asym_hardware(args),
        2 => skewed_load(args),
        3 => msg_flood(args),
        _ => panic!("unknown computation provided")
    };
}

// processing skew /straggler
fn asym_hardware(args: Vec<String>) {
    timely::execute_from_args(args.clone().into_iter(), |worker| {
        // (A) Create SnailTrail adapter at the beginning of the worker closure
        let adapter = Adapter::attach(worker);

        let idx = worker.index();

        // Some computation
        let mut input = InputHandle::new();
        let probe = worker.dataflow(|scope| {
            scope
                .input_from(&mut input)
                .exchange(|x| *x)
                .inspect_batch(move |_,_| {
                    if idx == 0 {
                        std::thread::sleep_ms(500)
                    }
                })
                .map(|x| x + 1)
                .probe()
        });

        for round in 0..10 {
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


fn skewed_load(args: Vec<String>) {
    timely::execute_from_args(args.clone().into_iter(), |worker| {
        // (A) Create SnailTrail adapter at the beginning of the worker closure
        let adapter = Adapter::attach(worker);

        let idx = worker.index();

        // Some computation
        let mut input = InputHandle::new();
        let probe = worker.dataflow(|scope| {
            scope
                .input_from(&mut input)
                .exchange(|x| *x)
                .inspect_batch(move |_,_| {})
                .map(|x| x + 1)
                .probe()
        });

        for round in 0..10 {
            if worker.index() == 0 {
                for x in 0 .. 10000 {
                    input.send(x);
                }
            } else {
                for x in 0 .. 2000 {
                    input.send(x);
                }
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


// @TODO: Not sure how to model this
fn msg_flood(args: Vec<String>) {
    // timely::execute_from_args(args.clone().into_iter(), |worker| {
    //     // (A) Create SnailTrail adapter at the beginning of the worker closure
    //     let adapter = Adapter::attach(worker);

    //     let idx = worker.index();

    //     // Some computation
    //     let mut input = InputHandle::new();
    //     let probe = worker.dataflow(|scope| {
    //         scope
    //             .input_from(&mut input)
    //             .exchange(|x| *x)
    //             .inspect_batch(move |_,_| {})
    //             .map(|x| x + 1)
    //             .probe()
    //     });

    //     for round in 0..10 {
    //         for x in 0 .. 2000 {
    //             input.send(x);
    //         }

    //         input.advance_to(round + 1);
    //         while probe.less_than(input.time()) {
    //             worker.step_or_park(None);
    //         }

    //         // (B) Communicate epoch completion
    //         adapter.tick_epoch();
    //     }
    // }).unwrap();
}
