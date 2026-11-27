//! Writes samples of formulas for the UI's test (`apps/spikes-ui/test/signal.test.ts`), which evaluates the same
//! formulas with `signal.ts` and compares: the designer's preview and the engine must agree, noise included. A change
//! shows up in `git diff`.

use std::path::Path;

use spikes_signal::data::wav_16;
use spikes_signal::{At, DataFile, DataSets, Formula, noise};

/// Formulas over every operator, function and noise source.
const FORMULAS: &[&str] = &[
    "floor(t / 100) % 7 == c % 7",
    "0.5 + 0.5 * sin(2 * pi * t / 250 + c)",
    "pulse(t - 30, 100, 20) * (c + 1) / n",
    "t < 50 ? c : n - c",
    "-2^2 + 2^-1 + 2^3^2 / 512",
    "exp(-t / 100) * cos(t / 10) + log(1 + t) - sqrt(t) / 10",
    "abs(c - n / 2) + ceil(t / 33) - round(t / 20 - 2.5) + tan(t / 1000)",
    "min(max(c, 2), 5) + pow(c, 2) / 10 + clamp(t / 100 - 1, 0, 1) + step(t - 40)",
    "-7 % 3 + t % 13 - c % -3",
    "rand()",
    "gauss()",
    "rand() - rand() + 0.1 * gauss()",
    "clamp(0.5 + 0.3 * gauss(), 0, 1) * (rand() < 0.8)",
    "1 / 0 + 0 / 0",
];

/// Formulas that look up data files `tbl` (a CSV) and `wav` (a WAV), also out of range and by a missing name.
const DATA_FORMULAS: &[&str] = &[
    "data(\"tbl\", t, c)",
    "data(\"tbl\", t) * 2",
    "data(\"wav\", t / 2, c) + data(\"tbl\", t, 1)",
    "data(\"wav\", t, c + 1) - data(\"wav\", 20 - t)",
    "data(\"tbl\", t - 10, 0.5) + data(\"tbl\", t, -1) + data(\"missing\", t)",
    "pulse(t, 40, 10) * data(\"wav\", t * 0.7, c % 2)",
];

const CSV: &str = "t,a,b\n0,0,1\n7.5,1,0.5\n20,-1,2\n21,0.25,0\n55,3,3\n80,0,-2\n";

fn base64(bytes: &[u8]) -> String {
    const A: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let n = chunk
            .iter()
            .enumerate()
            .fold(0u32, |a, (i, &b)| a | (u32::from(b) << (16 - 8 * i)));
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(A[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

#[test]
fn write_formula_samples_for_the_ui() {
    let key = noise::key(42, "parity");
    let (channels, samples, dt_ms) = (5u32, 40u64, 2.5);
    let cases: Vec<serde_json::Value> = FORMULAS
        .iter()
        .map(|source| {
            let f = Formula::compile(source).unwrap();
            let values: Vec<f64> = (0..samples)
                .flat_map(|s| {
                    let f = &f;
                    (0..channels).map(move |c| f.eval(s as f64 * dt_ms, c, channels, At { sample: s, key }))
                })
                .collect();
            serde_json::json!({ "formula": source, "values": values })
        })
        .collect();
    let wav = wav_16(
        1000,
        &[
            (0..60).map(|i| ((i as f64 * 0.3).sin() * 20000.0) as i16).collect(),
            (0..60).map(|i| (i * 500 - 15000) as i16).collect(),
        ],
    );
    let files = [
        DataFile {
            name: "tbl".into(),
            bytes: CSV.as_bytes().to_vec(),
        },
        DataFile {
            name: "wav".into(),
            bytes: wav,
        },
    ];
    let data = DataSets::load(&files).unwrap();
    let data_cases: Vec<serde_json::Value> = DATA_FORMULAS
        .iter()
        .map(|source| {
            let f = Formula::compile(source).unwrap();
            let values: Vec<f64> = (0..samples)
                .flat_map(|s| {
                    let (f, data) = (&f, &data);
                    (0..channels).map(move |c| f.eval_with(s as f64 * dt_ms, c, channels, At { sample: s, key }, data))
                })
                .collect();
            serde_json::json!({ "formula": source, "values": values })
        })
        .collect();
    let data_files: Vec<serde_json::Value> = files
        .iter()
        .map(|f| serde_json::json!({ "name": f.name, "base64": base64(&f.bytes) }))
        .collect();
    let fixture = serde_json::json!({
        "dataFiles": data_files,
        "dataCases": data_cases,
        "seed": 42,
        "source": "parity",
        "key": key.to_string(),
        "channels": channels,
        "samples": samples,
        "dtMs": dt_ms,
        "cases": cases,
    });
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../apps/spikes-ui/test/fixtures/signal-parity.json");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, serde_json::to_string_pretty(&fixture).unwrap() + "\n").unwrap();
}
