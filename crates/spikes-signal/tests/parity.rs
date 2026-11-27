//! Writes samples of formulas for the UI's test (`apps/spikes-ui/test/signal.test.ts`), which evaluates the same
//! formulas with `signal.ts` and compares: the designer's preview and the engine must agree, noise included. A change
//! shows up in `git diff`.

use std::path::Path;

use spikes_signal::{At, Formula, noise};

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
    let fixture = serde_json::json!({
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
