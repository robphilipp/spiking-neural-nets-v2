//! A scripted "live session", standing in for a UI: a LIF network runs in real time on its own thread while this
//! thread reads spikes at 60 frames per second, changes the clock speed, pauses, queries a neuron, steps,
//! resumes, injects live input, adds connections, and prints the system events. A second subscriber never reads
//! anything, to show that a stalled consumer can't slow the simulation down.
//!
//! ```sh
//! cargo run --release -p tempo-testkit --example live
//! ```

use std::time::{Duration, Instant};

use tempo_runtime::{EdgeIdx, Pacing, RouteEdge, Runtime, RuntimeConfig, SimDuration, SimTime, TopologyOp};
use tempo_testkit::models::lif::{self, LifMsg, LifNeuron};

const FRAME: Duration = Duration::from_micros(16_667);

fn main() {
    let network = lif::LifConfig {
        neurons: 10_000,
        synapses_per_neuron: 20,
        sources: 200,
        input_until: SimTime::MAX,
        ..lif::LifConfig::default()
    };
    let mut rt = Runtime::new(RuntimeConfig {
        seed: 7,
        shards: 2,
        pacing: Pacing::RealTime { time_factor: 1.0 },
        min_remote_delay: Some(network.input_delay),
        journal: true,
        ..RuntimeConfig::default()
    });
    let net = lif::build(&mut rt, &network);
    let spikes = rt.subscribe(4_096);
    let stalled = rt.subscribe(4); // never read
    let notices = rt.system_events();
    let control = rt.control();
    let clock = rt.clock();
    println!(
        "LIF network: {} neurons × {} synapses on 2 shards, real time",
        network.neurons, network.synapses_per_neuron
    );
    println!(
        "{:>6} {:>10} {:>8} {:>14} {:>12}  event",
        "wall s", "sim ms", "speed", "spikes/frame", "frame ms"
    );

    std::thread::scope(|s| {
        let runner = s.spawn(|| {
            rt.run();
            rt
        });

        let start = Instant::now();
        let mut next_frame = start;
        let mut frames = 0u64;
        let mut spikes_this_second = 0usize;
        let mut worst_frame = Duration::ZERO;
        let mut script = vec![
            (1.0, "slow down to 4× slower"),
            (2.0, "speed up to 2× faster"),
            (3.0, "pause; query; step 5 ms"),
            (3.5, "resume at 1×; live stimulus to 100 neurons"),
            (4.0, "connect 10 new edges"),
            (5.0, "stop"),
        ];
        script.reverse();

        loop {
            // One frame: take whatever spikes have arrived.
            let frame_start = Instant::now();
            let batch = spikes.drain();
            spikes_this_second += batch.len();
            worst_frame = worst_frame.max(frame_start.elapsed());
            frames += 1;

            let wall = start.elapsed().as_secs_f64();
            if frames.is_multiple_of(30) {
                println!(
                    "{:>6.2} {:>10.1} {:>8} {:>14.0} {:>12.3}",
                    wall,
                    clock.sim_time().since_start().as_millis_f64(),
                    clock.time_factor().map_or("max".into(), |f| format!("{:.2}", 1.0 / f)),
                    spikes_this_second as f64 / 30.0,
                    worst_frame.as_secs_f64() * 1e3
                );
                spikes_this_second = 0;
                worst_frame = Duration::ZERO;
            }

            if let Some(&(at, what)) = script.last()
                && wall >= at
            {
                script.pop();
                println!(
                    "{wall:>6.2} {:>10.1} {:>8} {:>14} {:>12}  >> {what}",
                    clock.sim_time().since_start().as_millis_f64(),
                    "",
                    "",
                    ""
                );
                match what {
                    "slow down to 4× slower" => clock.set_time_factor(4.0),
                    "speed up to 2× faster" => clock.set_time_factor(0.5),
                    "pause; query; step 5 ms" => {
                        clock.pause();
                        let before = clock.sim_time();
                        let n = control
                            .query(net.neurons[0], |n: &LifNeuron| (n.spikes, n.u_mv))
                            .wait()
                            .unwrap()
                            .unwrap();
                        println!(
                            "{:>6} {:>10} {:>8} {:>14} {:>12}     neuron 0: {} spikes, u = {:.2} mV (paused at {before})",
                            "", "", "", "", "", n.0, n.1
                        );
                        clock.step(SimDuration::from_millis(5));
                    }
                    "resume at 1×; live stimulus to 100 neurons" => {
                        clock.set_time_factor(1.0);
                        clock.resume();
                        let replies: Vec<_> = net.neurons[..100]
                            .iter()
                            .map(|&n| control.inject_live(n, LifMsg::Input(12.0)))
                            .collect();
                        let at = replies.into_iter().map(|r| r.wait().unwrap()).max().unwrap();
                        println!(
                            "{:>6} {:>10} {:>8} {:>14} {:>12}     stimulus delivered at {at}",
                            "", "", "", "", ""
                        );
                    }
                    "connect 10 new edges" => {
                        for i in 0..10 {
                            let route = control
                                .query(net.neurons[i], |n: &LifNeuron| n.out)
                                .wait()
                                .unwrap()
                                .flatten()
                                .unwrap();
                            let edge = RouteEdge {
                                target: net.neurons[i + 5_000],
                                delay: SimDuration::from_millis(2),
                                edge: EdgeIdx(0),
                            };
                            control.change_route(route, TopologyOp::Connect(edge));
                        }
                    }
                    _ => {
                        clock.stop();
                        break;
                    }
                }
            }

            next_frame += FRAME;
            std::thread::sleep(next_frame.saturating_duration_since(Instant::now()));
        }

        let mut rt = runner.join().unwrap();
        let wall = start.elapsed();
        println!();
        println!("simulated {} in {:.2} s wall", rt.now(), wall.as_secs_f64());
        println!("events delivered: {}", rt.stats().delivered);
        println!("fast subscriber dropped:    {} events", spikes.dropped());
        println!(
            "stalled subscriber dropped: {} events (the simulation never waited for it)",
            stalled.dropped()
        );
        println!("journal: {} live inputs recorded for replay", rt.take_journal().len());
        println!("system events:");
        for notice in notices.drain() {
            println!(
                "  {:>10}  {:?}  {:?}",
                notice.sim_time.to_string(),
                notice.severity,
                notice.event
            );
        }
    });
}
