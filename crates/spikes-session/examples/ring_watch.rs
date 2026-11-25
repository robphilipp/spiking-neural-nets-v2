//! Watches a ring network after one stimulus: spikes per group and the weights between groups over time, to see
//! why activity stops.
//!
//! ```text
//! cargo run --release -p spikes-session --example ring_watch -- [file] [seconds] [stimulated prefix] [mV]
//! ```

use spikes_session::{BuildSettings, Engine, RunSettings, Speed};
use tempo_runtime::SimDuration;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let file = args
        .get(1)
        .map_or("test-data/networks/ring-even-spaced.boo", String::as_str);
    let seconds: u64 = args.get(2).map_or(400, |s| s.parse().expect("seconds"));
    let prefix = args.get(3).map_or("hidden-1", String::as_str);
    let mv: f32 = args.get(4).map_or(10.0, |s| s.parse().expect("mV"));

    let engine = Engine::new();
    let session = engine
        .create("ring", std::fs::read_to_string(file).expect("file"))
        .expect("parses");
    let network = session.build(BuildSettings::default()).expect("builds");
    let names: Vec<String> = network.neurons.iter().map(|n| n.name.clone()).collect();
    let group = |i: usize| names[i].trim_start_matches("hidden-").chars().next().unwrap();
    let targets: Vec<u32> = (0..names.len() as u32)
        .filter(|&i| names[i as usize].starts_with(prefix))
        .collect();
    session.stimulate(&targets, mv).expect("stimulate");

    // PROBE=<neuron>@<from s>: run to then, and print that neuron's state every 0.5 ms for 40 ms.
    if let Ok(probe) = std::env::var("PROBE") {
        let (name, from) = probe.split_once('@').expect("name@seconds");
        let i = names.iter().position(|n| n == name).expect("neuron") as u32;
        let run = |ms: f64| {
            session
                .start(RunSettings {
                    speed: Speed::AsFastAsPossible,
                    duration: Some(SimDuration::from_millis_f64(ms)),
                    record: None,
                })
                .expect("start");
            assert!(session.wait_for_run(std::time::Duration::from_secs(600)));
        };
        run(from.parse::<f64>().expect("seconds") * 1000.0);
        for _ in 0..80 {
            run(0.5);
            let s = session.query_neuron(i).unwrap();
            let mut syn: Vec<_> = s.synapses.iter().map(|x| (x.0, x.1, x.2)).collect();
            syn.sort_by_key(|x| x.0);
            let w: Vec<String> = syn.iter().map(|x| format!("{:.2}", x.1)).collect();
            println!(
                "t {:9.1} ms  u {:6.3} mV at {:9.1}  last spike {:9.1}  weights {}",
                session.sim_time().as_nanos() as f64 / 1e6,
                s.membrane_mv,
                s.last_event.as_nanos() as f64 / 1e6,
                s.last_spike.map_or(0.0, |t| t.as_nanos() as f64 / 1e6),
                w.join(" ")
            );
        }
        return;
    }
    // WATCH_FROM=<s>: run to then, and print each lap (250 ms) from there: which neurons fired, and when.
    if let Ok(from) = std::env::var("WATCH_FROM") {
        let from: f64 = from.parse().expect("WATCH_FROM");
        let run = |ms: f64| {
            session
                .start(RunSettings {
                    speed: Speed::AsFastAsPossible,
                    duration: Some(SimDuration::from_millis_f64(ms)),
                    record: None,
                })
                .expect("start");
            assert!(session.wait_for_run(std::time::Duration::from_secs(600)));
        };
        run(from * 1000.0);
        let mut last: Vec<_> = (0..names.len() as u32)
            .map(|i| session.query_neuron(i).unwrap().last_spike)
            .collect();
        for lap in 0..40 {
            run(250.0);
            let mut fired = Vec::new();
            for i in 0..names.len() {
                let s = session.query_neuron(i as u32).unwrap();
                if s.last_spike != last[i] {
                    let at = s.last_spike.unwrap().as_nanos() as f64 / 1e6;
                    fired.push(format!(
                        "{}@{:.1}",
                        &names[i][7..],
                        at - from * 1000.0 - 250.0 * lap as f64
                    ));
                }
                last[i] = s.last_spike;
            }
            println!("lap {lap:2}: {} fired: {}", fired.len(), fired.join(" "));
            if fired.is_empty() {
                break;
            }
        }
        return;
    }
    let mut before = vec![0u64; names.len()];
    println!("   t   spikes per group (1..5)        weights into each group from the one before: min/mean/max");
    for t in (10..=seconds).step_by(10) {
        session
            .start(RunSettings {
                speed: Speed::AsFastAsPossible,
                duration: Some(SimDuration::from_secs(10)),
                record: None,
            })
            .expect("start");
        assert!(session.wait_for_run(std::time::Duration::from_secs(600)));
        let states: Vec<_> = (0..names.len() as u32)
            .map(|i| session.query_neuron(i).expect("query"))
            .collect();
        let mut per_group = [0u64; 5];
        let mut weights: [Vec<f32>; 5] = Default::default();
        for (i, s) in states.iter().enumerate() {
            let g = (group(i) as u8 - b'1') as usize;
            per_group[g] += s.spikes - before[i];
            before[i] = s.spikes;
            weights[g].extend(s.synapses.iter().map(|syn| syn.1));
        }
        let w: Vec<String> = weights
            .iter()
            .map(|w| {
                let (min, max) = w.iter().fold((f32::MAX, f32::MIN), |(a, b), &x| (a.min(x), b.max(x)));
                let mean = w.iter().sum::<f32>() / w.len().max(1) as f32;
                format!("{min:.2}/{mean:.2}/{max:.2}")
            })
            .collect();
        println!("{t:4}   {:<30} {}", format!("{per_group:?}"), w.join("  "));
        if per_group.iter().all(|&c| c == 0) {
            println!(
                "stopped. Each neuron's incoming weights (from the previous group's neurons 1..5), and their sum:"
            );
            for (i, s) in states.iter().enumerate() {
                let mut syn: Vec<_> = s.synapses.iter().map(|x| (x.0, x.1)).collect();
                syn.sort_by_key(|x| x.0);
                let w: Vec<String> = syn.iter().map(|x| format!("{:4.2}", x.1)).collect();
                let sum: f32 = syn.iter().map(|x| x.1).sum();
                println!("  {}  {}   sum {sum:5.2}", names[i], w.join(" "));
            }
            break;
        }
    }
}
