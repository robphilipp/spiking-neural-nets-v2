//! External sensors.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use arrow::array::AsArray;
use arrow::datatypes::UInt64Type;
use rand::{RngExt, SeedableRng};
use rand_xoshiro::Xoshiro256PlusPlus;
use spikes_record::read_table;
use spikes_session::{
    BuildSettings, Encoding, Engine, OnPause, RunSettings, SampleBatch, Sensor, SensorSpec, Session, Speed, SpikeBatch,
};
use tempo_runtime::SimDuration;

/// `n` unconnected neurons with threshold `mst` mV.
fn neurons(n: usize, mst: f64) -> String {
    let list: Vec<String> = (0..n)
        .map(|i| format!("(nid=n{i}, mst={mst} mV, mpn=0 mV, LOC=(cst=ct, px1={i}, px2=0, px3=0))"))
        .collect();
    format!("(NRN=[{}])", list.join(", "))
}

fn session(engine: &Engine, n: usize, mst: f64) -> Session {
    let session = engine.create("sensors", neurons(n, mst)).unwrap();
    session.build(BuildSettings::default()).unwrap();
    session
}

fn out_dir(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

/// Streams `batches` batches of 10 samples (1 kHz, `channels` channels, all `value`) from another thread at
/// ~10 ms intervals with ±5 ms jitter, like a real device over a network.
fn stream(sensor: &Sensor, channels: usize, batches: u64, value: f32) {
    let mut rng = Xoshiro256PlusPlus::seed_from_u64(3);
    let start = Instant::now();
    for b in 0..batches {
        let due = Duration::from_millis(10 * b) + Duration::from_micros(rng.random_range(0..10_000));
        if let Some(wait) = due.checked_sub(start.elapsed()) {
            std::thread::sleep(wait.saturating_sub(Duration::from_millis(5)));
        }
        sensor
            .send_samples(&SampleBatch {
                t0_ns: 7_000_000_000 + b * 10_000_000, // the sensor's own clock, unrelated to the simulation's
                dt_ns: 1_000_000,
                values: vec![value; 10 * channels],
            })
            .unwrap();
    }
}

#[test]
fn jittered_batches_keep_exact_sample_spacing() {
    let engine = Engine::new();
    let session = session(&engine, 4, 5.0);
    session.observe(&[0, 1, 2, 3], true).unwrap();
    let sensor = session
        .connect_sensor(SensorSpec {
            encoding: Encoding::Current { gain_mv: 0.1 },
            ..SensorSpec::new("probe", vec![vec![0], vec![1], vec![2], vec![3]])
        })
        .unwrap();
    let dir = out_dir("sensor-spacing");
    session
        .start(RunSettings {
            speed: Speed::Factor(1.0),
            duration: Some(SimDuration::from_millis(400)),
            record: Some(dir.clone()),
        })
        .unwrap();
    stream(&sensor, 4, 30, 1.0);
    assert!(session.wait_for_run(Duration::from_secs(5)));
    let stats = sensor.stats();
    assert_eq!(
        (stats.batches, stats.samples, stats.inputs, stats.late_batches),
        (30, 300, 1200, 0)
    );

    // Every input reached its neuron exactly 1 ms after the previous one.
    let membrane = read_table(&dir.join("membrane.arrows")).unwrap();
    let neuron = membrane
        .column_by_name("neuron")
        .unwrap()
        .as_primitive::<UInt64Type>()
        .values()
        .to_vec();
    let t = membrane
        .column_by_name("t_ns")
        .unwrap()
        .as_primitive::<UInt64Type>()
        .values()
        .to_vec();
    let first = session.network().id(0).unwrap().raw();
    let times: Vec<u64> = neuron
        .iter()
        .zip(&t)
        .filter(|(n, _)| **n == first)
        .map(|(_, t)| *t)
        .collect();
    assert_eq!(times.len(), 300);
    assert!(times.windows(2).all(|w| w[1] - w[0] == 1_000_000), "spacing kept");
    // The recording has the samples and the inputs.
    assert_eq!(read_table(&dir.join("sensor_samples.arrows")).unwrap().num_rows(), 1200);
    assert_eq!(read_table(&dir.join("sensor_inputs.arrows")).unwrap().num_rows(), 1200);
}

fn spikes(dir: &Path) -> Vec<(u64, u64)> {
    let t = read_table(&dir.join("spikes.arrows")).unwrap();
    let ns = t
        .column_by_name("t_ns")
        .unwrap()
        .as_primitive::<UInt64Type>()
        .values()
        .to_vec();
    let n = t
        .column_by_name("neuron")
        .unwrap()
        .as_primitive::<UInt64Type>()
        .values()
        .to_vec();
    let mut s: Vec<(u64, u64)> = ns.into_iter().zip(n).collect();
    s.sort_unstable();
    s
}

#[test]
fn replaying_recorded_sensor_inputs_reproduces_the_run() {
    let engine = Engine::new();
    let live = session(&engine, 4, 1.0);
    let sensor = live
        .connect_sensor(SensorSpec {
            encoding: Encoding::Rate { max_hz: 200.0, mv: 2.0 },
            ..SensorSpec::new("cochlea", vec![vec![0], vec![1], vec![2], vec![3]])
        })
        .unwrap();
    let live_dir = out_dir("sensor-live");
    live.start(RunSettings {
        speed: Speed::Factor(1.0),
        duration: Some(SimDuration::from_millis(300)),
        record: Some(live_dir.clone()),
    })
    .unwrap();
    stream(&sensor, 4, 25, 0.8);
    assert!(live.wait_for_run(Duration::from_secs(5)));
    let original = spikes(&live_dir);
    assert!(original.len() > 20, "{}", original.len());

    let replay = session(&engine, 4, 1.0);
    let replay_dir = out_dir("sensor-replay");
    assert_eq!(
        replay.replay_sensor_inputs(&live_dir).unwrap() as u64,
        sensor.stats().inputs
    );
    replay
        .start(RunSettings {
            speed: Speed::AsFastAsPossible,
            duration: Some(SimDuration::from_millis(300)),
            record: Some(replay_dir.clone()),
        })
        .unwrap();
    assert!(replay.wait_for_run(Duration::from_secs(5)));
    assert_eq!(spikes(&replay_dir), original);
}

fn buffered(session: &Session, encoding: Encoding, targets: Vec<Vec<u32>>) -> Sensor {
    session
        .connect_sensor(SensorSpec {
            encoding,
            on_pause: OnPause::Buffer, // not running counts as paused
            ..SensorSpec::new("s", targets)
        })
        .unwrap()
}

#[test]
fn encodings() {
    let engine = Engine::new();
    let s = session(&engine, 3, 1000.0);

    // Rate: 1 s at full rate, 1 kHz max: about 1000 events (σ ≈ 32).
    let rate = buffered(
        &s,
        Encoding::Rate {
            max_hz: 1000.0,
            mv: 1.0,
        },
        vec![vec![0]],
    );
    rate.send_samples(&SampleBatch {
        t0_ns: 0,
        dt_ns: 1_000_000,
        values: vec![1.0; 1000],
    })
    .unwrap();
    let n = rate.stats().inputs;
    assert!((870..1130).contains(&n), "{n}");

    // Threshold: 0 → 1 is four steps of 0.25 up, 1 → 0.25 three down.
    let delta = buffered(&s, Encoding::Threshold { level: 0.25, mv: 1.0 }, vec![vec![1]]);
    delta
        .send_samples(&SampleBatch {
            t0_ns: 0,
            dt_ns: 1_000_000,
            values: vec![0.0, 0.5, 1.0, 0.25],
        })
        .unwrap();
    assert_eq!(delta.stats().inputs, 7);

    // Current: zero samples send nothing; each channel reaches all its targets.
    let current = buffered(&s, Encoding::Current { gain_mv: 1.0 }, vec![vec![0, 1], vec![2]]);
    current
        .send_samples(&SampleBatch {
            t0_ns: 0,
            dt_ns: 1_000_000,
            values: vec![1.0, 0.0, 1.0, 1.0],
        })
        .unwrap();
    assert_eq!(current.stats().inputs, 2 + 2 + 1);
    assert!(
        current
            .send_samples(&SampleBatch {
                t0_ns: 0,
                dt_ns: 1,
                values: vec![1.0; 3]
            })
            .is_err()
    );

    // Spikes: passed through to each target.
    let events = buffered(&s, Encoding::Spikes { mv: 1.0 }, vec![vec![0, 1, 2]]);
    events
        .send_spikes(&SpikeBatch {
            t0_ns: 0,
            events: vec![(0, 10), (0, 20)],
        })
        .unwrap();
    assert_eq!(events.stats().inputs, 6);
    assert!(
        events
            .send_spikes(&SpikeBatch {
                t0_ns: 0,
                events: vec![(1, 10)]
            })
            .is_err()
    );

    // Bad targets are rejected when connecting.
    assert!(s.connect_sensor(SensorSpec::new("bad", vec![vec![9]])).is_err());
}

#[test]
fn live_sensors_drop_samples_while_paused() {
    let engine = Engine::new();
    let s = session(&engine, 1, 5.0);
    let sensor = s.connect_sensor(SensorSpec::new("mic", vec![vec![0]])).unwrap();
    let batch = SampleBatch {
        t0_ns: 0,
        dt_ns: 1_000_000,
        values: vec![0.1; 10],
    };
    sensor.send_samples(&batch).unwrap(); // not running: dropped
    s.start(RunSettings::default()).unwrap();
    sensor
        .send_samples(&SampleBatch {
            t0_ns: 10_000_000,
            ..batch.clone()
        })
        .unwrap();
    s.pause().unwrap();
    sensor
        .send_samples(&SampleBatch {
            t0_ns: 20_000_000,
            ..batch.clone()
        })
        .unwrap();
    s.stop().unwrap();
    let stats = sensor.stats();
    assert_eq!((stats.samples, stats.dropped, stats.inputs), (30, 20, 10));
}

#[test]
fn a_session_lists_its_sensors_until_they_disconnect() {
    let engine = Engine::new();
    let session = session(&engine, 4, 10.0);
    let notices = session.notices();
    let sensor = session
        .connect_sensor(SensorSpec {
            encoding: Encoding::Current { gain_mv: 1.0 },
            ..SensorSpec::new("mic", vec![vec![0, 1], vec![2]])
        })
        .unwrap();
    let copy = sensor.clone();
    let listed = session.sensors();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].spec.name, "mic");
    assert_eq!(listed[0].spec.targets, vec![vec![0, 1], vec![2]]);
    session
        .start(RunSettings {
            speed: Speed::Factor(1.0),
            duration: Some(SimDuration::from_millis(200)),
            record: None,
        })
        .unwrap();
    stream(&sensor, 2, 5, 3.0);
    assert!(session.sensors()[0].stats.samples >= 50, "its counters are listed");
    session.stop().unwrap();
    drop(sensor);
    assert_eq!(session.sensors().len(), 1, "a clone keeps it connected");
    drop(copy);
    assert!(session.sensors().is_empty());
    let changes = notices
        .try_iter()
        .filter(|n| matches!(n, spikes_session::SessionNotice::SensorsChanged))
        .count();
    assert_eq!(changes, 2, "connected, then disconnected");
}
