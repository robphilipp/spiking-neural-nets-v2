//! The recorder: static tables at the start, then a thread that streams events to disk.

use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use arrow::array::{
    ArrayRef, BooleanArray, Float32Array, Float64Array, Int64Array, RecordBatch, StringArray, UInt32Array, UInt64Array,
};
use arrow::datatypes::{DataType, Field};
use arrow::ipc::writer::FileWriter;
use spikes_dna::NetworkDescription;
use spikes_model::{NeuronKind, Sign};
use spikes_network::{Network, NeuronActor, Runtime};

use crate::tables::{Counts, Streams, schema};
use crate::{RecordError, Result};

/// Recording settings.
#[derive(Copy, Clone, Debug)]
pub struct RecordOptions {
    /// Rows per Arrow record batch in the streaming tables. Larger batches compress and load better; smaller ones
    /// reach the disk sooner.
    pub batch_rows: usize,
}

impl Default for RecordOptions {
    fn default() -> Self {
        Self { batch_rows: 65_536 }
    }
}

/// What the run is, for `run.arrow`.
#[derive(Copy, Clone, Debug)]
pub struct RunInfo<'a> {
    /// The runtime's seed.
    pub seed: u64,
    /// The network description's source text (`.boo`).
    pub description: &'a str,
}

/// What a finished recording holds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Summary {
    /// The run directory.
    pub dir: PathBuf,
    /// Neurons in `neurons.arrow`.
    pub neurons: u64,
    /// Synapses in `synapses.arrow`.
    pub synapses: u64,
    /// Rows in each streaming table.
    pub rows: Counts,
}

/// Records a run: create it after building the network and before running, then call [`Recorder::finish`] after
/// the run.
#[derive(Debug)]
pub struct Recorder {
    dir: PathBuf,
    neurons: u64,
    synapses: u64,
    stop: Arc<AtomicBool>,
    thread: JoinHandle<Result<Counts>>,
}

impl Recorder {
    /// Creates `dir` (and its parents), writes the static tables, and starts recording events.
    pub fn start(
        dir: impl AsRef<Path>,
        rt: &Runtime,
        network: &Network,
        description: &NetworkDescription,
        run: RunInfo<'_>,
        options: RecordOptions,
    ) -> Result<Self> {
        let dir = dir.as_ref().to_path_buf();
        std::fs::create_dir_all(&dir)?;
        write_run(&dir, rt, run)?;
        let neurons = write_neurons(&dir, network, description)?;
        let synapses = write_synapses(&dir, rt, network)?;

        let mut streams = Streams::create(&dir, options.batch_rows)?;
        let events = rt.subscribe_lossless();
        let notices = rt.system_events();
        let stop = Arc::new(AtomicBool::new(false));
        let stopping = Arc::clone(&stop);
        let thread = std::thread::Builder::new()
            .name("spikes-record".into())
            .spawn(move || -> Result<Counts> {
                loop {
                    // Read the flag first: everything sent before `finish` set it is then already queued.
                    let done = stopping.load(Ordering::Acquire);
                    let batches = if done {
                        vec![events.drain()]
                    } else {
                        events
                            .next_batch_timeout(Duration::from_millis(20))
                            .into_iter()
                            .collect()
                    };
                    for batch in &batches {
                        for e in batch {
                            streams.event(e)?;
                        }
                    }
                    for notice in notices.drain() {
                        streams.notice(&notice)?;
                    }
                    if done {
                        return streams.finish();
                    }
                }
            })?;
        Ok(Self {
            dir,
            neurons,
            synapses,
            stop,
            thread,
        })
    }

    /// The run directory.
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Writes everything recorded so far and closes the files. Call it after the run (e.g. after
    /// `Runtime::run_until` returns, or after the control plane has stopped the run).
    pub fn finish(self) -> Result<Summary> {
        self.stop.store(true, Ordering::Release);
        let rows = self.thread.join().map_err(|_| RecordError::WriterPanicked)??;
        Ok(Summary {
            dir: self.dir,
            neurons: self.neurons,
            synapses: self.synapses,
            rows,
        })
    }
}

fn write_table(dir: &Path, name: &str, fields: Vec<Field>, columns: Vec<ArrayRef>) -> Result<()> {
    let schema = schema(fields);
    let batch = RecordBatch::try_new(schema.clone(), columns)?;
    let mut writer = FileWriter::try_new(File::create(dir.join(format!("{name}.arrow")))?, &schema)?;
    writer.write(&batch)?;
    writer.finish()?;
    Ok(())
}

fn col(name: &str, data_type: DataType) -> Field {
    Field::new(name, data_type, false)
}

fn write_run(dir: &Path, rt: &Runtime, run: RunInfo<'_>) -> Result<()> {
    let started = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_nanos()).unwrap_or(i64::MAX));
    write_table(
        dir,
        "run",
        vec![
            col("seed", DataType::UInt64),
            col("shards", DataType::UInt32),
            col("started_unix_ns", DataType::Int64),
            col("recorder", DataType::Utf8),
            col("description", DataType::Utf8),
        ],
        vec![
            Arc::new(UInt64Array::from(vec![run.seed])),
            Arc::new(UInt32Array::from(vec![rt.shards() as u32])),
            Arc::new(Int64Array::from(vec![started])),
            Arc::new(StringArray::from(vec![concat!(
                "spikes-record ",
                env!("CARGO_PKG_VERSION")
            )])),
            Arc::new(StringArray::from(vec![run.description])),
        ],
    )
}

fn kind_name(kind: &NeuronKind) -> &'static str {
    match kind {
        NeuronKind::MonostableIntegrator => "monostable_integrator",
        NeuronKind::BistableIntegrator { .. } => "bistable_integrator",
        NeuronKind::MonostableResonator(_) => "monostable_resonator",
        NeuronKind::BistableResonator { .. } => "bistable_resonator",
    }
}

fn write_neurons(dir: &Path, network: &Network, description: &NetworkDescription) -> Result<u64> {
    let n = description.neurons.len().min(network.neurons.len());
    let neurons = &description.neurons[..n];
    let position = |axis: usize| Float64Array::from_iter_values(neurons.iter().map(|d| d.location_um[axis]));
    write_table(
        dir,
        "neurons",
        vec![
            col("neuron", DataType::UInt64),
            col("name", DataType::Utf8),
            col("group", DataType::Utf8),
            col("kind", DataType::Utf8),
            col("inhibitory", DataType::Boolean),
            col("x", DataType::Float64),
            col("y", DataType::Float64),
            col("z", DataType::Float64),
        ],
        vec![
            Arc::new(UInt64Array::from_iter_values(
                network.neurons[..n].iter().map(|r| r.id().raw()),
            )),
            Arc::new(StringArray::from_iter_values(neurons.iter().map(|d| d.id.as_str()))),
            Arc::new(StringArray::from_iter_values(neurons.iter().map(|d| d.group.as_str()))),
            Arc::new(StringArray::from_iter_values(
                neurons.iter().map(|d| kind_name(&d.params.kind)),
            )),
            Arc::new(BooleanArray::from_iter(neurons.iter().map(|d| Some(d.inhibitory)))),
            Arc::new(position(0)),
            Arc::new(position(1)),
            Arc::new(position(2)),
        ],
    )?;
    Ok(n as u64)
}

fn write_synapses(dir: &Path, rt: &Runtime, network: &Network) -> Result<u64> {
    let (mut post, mut index, mut pre, mut weight, mut equilibrium, mut inhibitory) =
        (Vec::new(), Vec::new(), Vec::new(), Vec::new(), Vec::new(), Vec::new());
    for neuron in &network.neurons {
        let Some(actor) = rt.actor::<NeuronActor>(neuron.id()) else {
            continue;
        };
        for (i, (syn, from)) in actor.synapses.iter().zip(&actor.pre).enumerate() {
            post.push(neuron.id().raw());
            index.push(i as u32);
            pre.push(from.raw());
            weight.push(syn.weight);
            equilibrium.push(syn.equilibrium);
            inhibitory.push(Some(syn.sign == Sign::Inhibitory));
        }
    }
    let rows = post.len() as u64;
    write_table(
        dir,
        "synapses",
        vec![
            col("post", DataType::UInt64),
            col("synapse", DataType::UInt32),
            col("pre", DataType::UInt64),
            col("weight", DataType::Float32),
            col("equilibrium", DataType::Float32),
            col("inhibitory", DataType::Boolean),
        ],
        vec![
            Arc::new(UInt64Array::from(post)),
            Arc::new(UInt32Array::from(index)),
            Arc::new(UInt64Array::from(pre)),
            Arc::new(Float32Array::from(weight)),
            Arc::new(Float32Array::from(equilibrium)),
            Arc::new(BooleanArray::from(inhibitory)),
        ],
    )?;
    Ok(rows)
}
