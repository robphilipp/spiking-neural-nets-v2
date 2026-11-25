//! Streams synthetic "cochlea" input into a server: creates a session with 64 input neurons, starts it in real
//! time, and sends 64 channels of slowly sweeping tone envelopes at 1 kHz in 10 ms batches for a few seconds.
//!
//! ```text
//! spikes-server &   # in another terminal
//! cargo run -p spikes-sensor --example tones -- ws://127.0.0.1:7070 [seconds]
//! ```

use std::time::{Duration, Instant};

use spikes_api::protocol::{Encoding, Request, Response, SensorSpec};
use spikes_sensor::SensorClient;

const CHANNELS: u16 = 64;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    let server = args.get(1).map_or("ws://127.0.0.1:7070", String::as_str);
    let seconds: f64 = args.get(2).map_or(Ok(3.0), |s| s.parse())?;

    let neurons: Vec<String> = (0..CHANNELS)
        .map(|i| {
            format!(
                "(name=cochlea-{i}, threshold=1 mV, membrane_noise=0 mV, position=[{}, 0, 0])",
                i * 10
            )
        })
        .collect();
    let description = format!("(neurons=[{}])", neurons.join(", "));
    let mut client = SensorClient::connect(server, None)?;
    let Response::Session { session } = client.request(Request::CreateSession {
        name: "tones".into(),
        description,
    })?
    else {
        return Err("expected a session".into());
    };
    client.request(Request::Build {
        session: session.id,
        settings: Default::default(),
    })?;
    let targets = client.neurons_named(session.id, "cochlea-")?;
    let sensor = client.connect_sensor(
        session.id,
        SensorSpec {
            name: "tones".into(),
            targets: targets.into_iter().map(|n| vec![n]).collect(),
            encoding: Encoding::Rate { max_hz: 200.0, mv: 2.0 },
            ingress_delay_ms: None,
            buffer_while_paused: false,
        },
    )?;
    client.request(Request::Start {
        session: session.id,
        speed: Some(1.0),
        duration_ms: None,
        record: false,
        lockstep: None,
    })?;

    let start = Instant::now();
    let mut batch = 0u64;
    while start.elapsed().as_secs_f64() < seconds {
        // Ten 1 ms samples; a band of loudness sweeping up the channels once a second.
        let mut values = Vec::with_capacity(10 * usize::from(CHANNELS));
        for s in 0..10u64 {
            let t = (batch * 10 + s) as f64 / 1000.0;
            let centre = (t % 1.0) * f64::from(CHANNELS);
            for c in 0..CHANNELS {
                let d = (f64::from(c) - centre) / 4.0;
                values.push((-d * d).exp() as f32);
            }
        }
        client.send_samples(sensor, batch * 10_000_000, 1_000_000, CHANNELS, &values)?;
        batch += 1;
        let due = Duration::from_millis(10 * batch);
        if let Some(wait) = due.checked_sub(start.elapsed()) {
            std::thread::sleep(wait);
        }
    }
    let Response::Neuron { neuron } = client.request(Request::QueryNeuron {
        session: session.id,
        neuron: 32,
    })?
    else {
        return Err("expected a neuron".into());
    };
    client.request(Request::Stop { session: session.id })?;
    println!("sent {batch} batches; neuron cochlea-32 fired {} times", neuron.spikes);
    client.close()?;
    Ok(())
}
