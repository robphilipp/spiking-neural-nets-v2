//! The winner-takes-all example learns: driven one input at a time (as the Inputs page does), out-1 comes to follow
//! in-1 and out-2 in-2.

use std::path::Path;
use std::time::Duration;

use spikes_session::{BuildSettings, Encoding, Engine, OnPause, RunSettings, SampleBatch, SensorSpec, Speed};
use tempo_runtime::SimDuration;

#[test]
fn each_output_learns_to_follow_its_input() {
    let text = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test-data/examples/wta-learning.boo"),
    )
    .unwrap();
    let engine = Engine::new();
    let session = engine.create("wta", text).unwrap();
    let network = session.build(BuildSettings::default()).unwrap();
    let ix = |name: &str| network.by_name(name).unwrap();
    let (in1, in2, out1, out2) = (ix("in-1"), ix("in-2"), ix("out-1"), ix("out-2"));

    // The Inputs page's signal: one input at a time, 300 ms blocks, rate encoding at 100 Hz, 10 mV per event.
    let mut spec = SensorSpec::new("designed signal", vec![vec![in1], vec![in2]]);
    spec.encoding = Encoding::Rate {
        max_hz: 100.0,
        mv: 10.0,
    };
    spec.on_pause = OnPause::Buffer;
    spec.ingress_delay = SimDuration::from_millis(1);
    let sensor = session.connect_sensor(spec).unwrap();
    let active = |t: f64| ((((t / 300.0).floor() * 12.9898).sin() * 43758.5453).abs().floor() as usize) % 2;
    let values: Vec<f32> = (0..20_000)
        .flat_map(|t| (0..2).map(move |c| if active(f64::from(t)) == c { 1.0 } else { 0.0 }))
        .collect();
    sensor
        .send_samples(&SampleBatch {
            t0_ns: 0,
            dt_ns: 1_000_000,
            values,
        })
        .unwrap();
    session
        .start(RunSettings {
            speed: Speed::AsFastAsPossible,
            duration: Some(SimDuration::from_secs(20)),
            record: None,
        })
        .unwrap();
    assert!(session.wait_for_run(Duration::from_secs(60)));

    let weight = |post: u32, pre: u32| {
        session
            .query_neuron(post)
            .unwrap()
            .synapses
            .iter()
            .find(|s| s.0 == pre)
            .unwrap()
            .1
    };
    assert!(
        weight(out1, in1) > 2.5 && weight(out2, in2) > 2.5,
        "the matching synapses grew"
    );
    assert!(weight(out1, in2) < 0.3 && weight(out2, in1) < 0.3, "the others faded");
}
