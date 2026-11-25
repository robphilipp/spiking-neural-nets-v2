//! Recording a run and reading it back.

use std::path::{Path, PathBuf};

use arrow::array::{Array, AsArray, RecordBatch};
use arrow::datatypes::{Float32Type, UInt32Type, UInt64Type};
use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;
use spikes_network::{BuildOptions, Event, PoissonInput, Runtime, build};
use spikes_record::{RecordOptions, Recorder, RunInfo, read_table, to_parquet, to_parquet_in_parts};
use tempo_runtime::{Emitted, RuntimeConfig, SimDuration, SimTime};

const SEED: u64 = 7;

fn source() -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test-data/v1/stdp-test.boo");
    std::fs::read_to_string(path).unwrap()
}

fn out_dir(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

/// Records 2 s of the STDP test network driven by Poisson input, with the output neuron observed. Returns the
/// directory and the events the runtime buffered, for comparison.
fn record(name: &str, shards: usize, batch_rows: usize) -> (PathBuf, Vec<Emitted<Event>>) {
    let text = source();
    let description = spikes_dna::parse(&text).unwrap();
    let mut rt = Runtime::new(RuntimeConfig {
        seed: SEED,
        shards,
        min_remote_delay: Some(SimDuration::from_millis(1)),
        ..RuntimeConfig::default()
    });
    rt.buffer_events();
    let net = build(
        &mut rt,
        &description,
        &BuildOptions {
            record_weights: true,
            ..BuildOptions::default()
        },
    );
    rt.observe(net.neuron("output-1").unwrap().id(), true);
    rt.spawn(PoissonInput {
        rate_hz: 40.0,
        targets: net.matching(|n| n.starts_with("input")),
        mv: 3.0,
        delay: SimDuration::from_millis(1),
        start: SimTime::ZERO,
        until: SimTime::from_start(SimDuration::from_secs(2)),
    });
    let dir = out_dir(name);
    let recorder = Recorder::start(
        &dir,
        &rt,
        &net,
        &description,
        RunInfo {
            seed: SEED,
            description: &text,
        },
        RecordOptions { batch_rows },
    )
    .unwrap();
    rt.run_until(SimTime::from_start(SimDuration::from_secs(2)));
    let summary = recorder.finish().unwrap();
    assert_eq!((summary.neurons, summary.synapses), (3, 2));
    (dir, rt.take_events())
}

fn u64s(batch: &RecordBatch, column: &str) -> Vec<u64> {
    batch
        .column_by_name(column)
        .unwrap()
        .as_primitive::<UInt64Type>()
        .values()
        .to_vec()
}

#[test]
fn every_event_is_recorded() {
    let (dir, events) = record("every_event", 1, 7); // tiny batches: many record batches per stream
    let count = |f: fn(&Event) -> bool| events.iter().filter(|e| f(&e.event)).count();
    let spikes = read_table(&dir.join("spikes.arrows")).unwrap();
    let weights = read_table(&dir.join("weights.arrows")).unwrap();
    let membrane = read_table(&dir.join("membrane.arrows")).unwrap();
    assert!(spikes.num_rows() > 50, "{}", spikes.num_rows());
    assert_eq!(spikes.num_rows(), count(|e| matches!(e, Event::Spike { .. })));
    assert_eq!(weights.num_rows(), count(|e| matches!(e, Event::Weight { .. })));
    assert_eq!(membrane.num_rows(), count(|e| matches!(e, Event::Membrane { .. })));
    assert!(weights.num_rows() > 0 && membrane.num_rows() > 0);

    // Values survive: the spike times and the last weight change of each synapse.
    let mut expected: Vec<(u64, u64)> = events
        .iter()
        .filter_map(|e| match e.event {
            Event::Spike { at } => Some((at.since_start().as_nanos(), e.src.raw())),
            _ => None,
        })
        .collect();
    let mut got: Vec<(u64, u64)> = u64s(&spikes, "t_ns").into_iter().zip(u64s(&spikes, "neuron")).collect();
    expected.sort_unstable();
    got.sort_unstable();
    assert_eq!(got, expected);

    // On one shard rows are in emission order, so the weight changes match one for one.
    let after = weights.column_by_name("after").unwrap().as_primitive::<Float32Type>();
    let synapse = weights.column_by_name("synapse").unwrap().as_primitive::<UInt32Type>();
    let expected: Vec<(u32, f32)> = events
        .iter()
        .filter_map(|e| match e.event {
            Event::Weight { synapse, after, .. } => Some((synapse, after)),
            _ => None,
        })
        .collect();
    let got: Vec<(u32, f32)> = synapse
        .values()
        .iter()
        .copied()
        .zip(after.values().iter().copied())
        .collect();
    assert_eq!(got, expected);
}

#[test]
fn static_tables_describe_the_network() {
    let (dir, _) = record("static_tables", 1, 1024);
    let neurons = read_table(&dir.join("neurons.arrow")).unwrap();
    let names = neurons.column_by_name("name").unwrap().as_string::<i32>();
    let names: Vec<&str> = (0..names.len()).map(|i| names.value(i)).collect();
    assert_eq!(names, ["input-1", "input-2", "output-1"]);
    let x = neurons
        .column_by_name("x")
        .unwrap()
        .as_primitive::<arrow::datatypes::Float64Type>();
    assert_eq!(x.value(0), 1000.0, "1 mm in µm");
    let kinds = neurons.column_by_name("kind").unwrap().as_string::<i32>();
    assert_eq!(kinds.value(2), "monostable_integrator");

    let synapses = read_table(&dir.join("synapses.arrow")).unwrap();
    let ids = u64s(&neurons, "neuron");
    assert_eq!(u64s(&synapses, "post"), [ids[2], ids[2]]);
    assert_eq!(u64s(&synapses, "pre"), [ids[0], ids[1]]);

    let run = read_table(&dir.join("run.arrow")).unwrap();
    assert_eq!(u64s(&run, "seed"), [SEED]);
    assert!(
        run.column_by_name("description")
            .unwrap()
            .as_string::<i32>()
            .value(0)
            .contains("output-1")
    );
    assert_eq!(
        run.schema().metadata().get("spikes.format").map(String::as_str),
        Some("1")
    );
}

fn parquet_table(path: &Path) -> RecordBatch {
    let reader = ParquetRecordBatchReaderBuilder::try_new(std::fs::File::open(path).unwrap())
        .unwrap()
        .build()
        .unwrap();
    let batches: Vec<RecordBatch> = reader.map(Result::unwrap).collect();
    arrow::compute::concat_batches(&batches[0].schema(), &batches).unwrap()
}

#[test]
fn parquet_is_sorted_and_independent_of_shard_count() {
    let mut tables = Vec::new();
    for shards in [1, 3] {
        let (dir, _) = record(&format!("parquet_{shards}"), shards, 1024);
        let written = to_parquet(&dir).unwrap();
        assert_eq!(written.len(), 8, "{written:?}");
        let spikes = parquet_table(&dir.join("spikes.parquet"));
        let t = u64s(&spikes, "t_ns");
        assert!(t.is_sorted());
        let weights = parquet_table(&dir.join("weights.parquet"));
        tables.push((spikes, weights));
    }
    assert_eq!(tables[0], tables[1], "same spikes and weights on 1 and 3 shards");
}

#[test]
fn large_tables_become_partitioned_datasets_with_the_same_rows() {
    // Three shards: rows arrive in time order only per shard, so some parts may get late rows.
    let (dir, _) = record("parquet_parts", 3, 16);
    to_parquet(&dir).unwrap();
    let whole = parquet_table(&dir.join("spikes.parquet"));
    assert!(whole.num_rows() > 60, "{} spikes", whole.num_rows());

    // In parts of ~20 rows: the single file gives way to a directory of sorted parts, in time order.
    let written = to_parquet_in_parts(&dir, 20).unwrap();
    assert!(!dir.join("spikes.parquet").exists(), "replaced by the dataset");
    let mut parts: Vec<PathBuf> = written
        .iter()
        .filter(|p| p.parent() == Some(dir.join("spikes").as_path()))
        .cloned()
        .collect();
    assert!(parts.len() >= 3, "{parts:?}");
    parts.sort();
    let mut tables = Vec::new();
    let mut previous_main_end = None;
    for part in &parts {
        let table = parquet_table(part);
        let t = u64s(&table, "t_ns");
        assert!(t.is_sorted(), "{} is sorted", part.display());
        // `part-00003.parquet` is a span's main file; `part-00003-1.parquet` holds its late rows.
        let main = part.file_name().unwrap().len() == "part-00000.parquet".len();
        if main {
            if let Some(end) = previous_main_end {
                assert!(t[0] >= end, "spans are in time order");
            }
            previous_main_end = t.last().copied();
        }
        tables.push(table);
    }
    let all = arrow::compute::concat_batches(&whole.schema(), &tables).unwrap();
    let mut rows: Vec<(u64, u64)> = u64s(&all, "t_ns").into_iter().zip(u64s(&all, "neuron")).collect();
    rows.sort_unstable();
    let expected: Vec<(u64, u64)> = u64s(&whole, "t_ns").into_iter().zip(u64s(&whole, "neuron")).collect();
    assert_eq!(rows, expected, "the parts hold exactly the table's rows");
    // Small tables stay single files; a new export replaces the dataset again.
    assert!(dir.join("neurons.parquet").exists());
    to_parquet(&dir).unwrap();
    assert!(dir.join("spikes.parquet").exists() && !dir.join("spikes").exists());
}
