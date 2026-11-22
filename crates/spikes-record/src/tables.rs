//! Table schemas and the streaming table writers.

use std::collections::HashMap;
use std::fs::File;
use std::io::BufWriter;
use std::path::Path;
use std::sync::Arc;
use std::time::UNIX_EPOCH;

use arrow::array::{
    ArrayRef, BooleanBuilder, Float32Builder, Int64Builder, RecordBatch, StringBuilder, UInt32Builder, UInt64Builder,
};
use arrow::datatypes::{DataType, Field, Schema, SchemaRef};
use arrow::ipc::writer::StreamWriter;
use spikes_model::Transition;
use spikes_network::Event;
use tempo_runtime::{Emitted, SimTime, SystemNotice};

use crate::Result;

/// The recording format version, stored in every table's schema metadata.
pub(crate) const FORMAT_VERSION: &str = "1";

pub(crate) fn schema(fields: Vec<Field>) -> SchemaRef {
    let metadata = HashMap::from([
        ("spikes.format".to_string(), FORMAT_VERSION.to_string()),
        (
            "spikes.units".to_string(),
            "t_ns: simulation time in ns; mv: millivolts; x/y/z: micrometres".to_string(),
        ),
    ]);
    Arc::new(Schema::new_with_metadata(fields, metadata))
}

fn col(name: &str, data_type: DataType) -> Field {
    Field::new(name, data_type, false)
}

pub(crate) fn t_ns(t: SimTime) -> u64 {
    t.since_start().as_nanos()
}

/// One streaming table: an Arrow IPC stream file plus column builders that are flushed every `batch_rows` rows.
struct Stream<B> {
    writer: StreamWriter<BufWriter<File>>,
    schema: SchemaRef,
    builders: B,
    rows: usize,
    total: u64,
}

trait Columns: Default {
    fn finish(&mut self) -> Vec<ArrayRef>;
}

impl<B: Columns> Stream<B> {
    fn create(dir: &Path, name: &str, schema: SchemaRef) -> Result<Self> {
        let file = BufWriter::new(File::create(dir.join(format!("{name}.arrows")))?);
        Ok(Self {
            writer: StreamWriter::try_new(file, &schema)?,
            schema,
            builders: B::default(),
            rows: 0,
            total: 0,
        })
    }

    fn row_added(&mut self, batch_rows: usize) -> Result<()> {
        self.rows += 1;
        self.total += 1;
        if self.rows >= batch_rows {
            self.flush()?;
        }
        Ok(())
    }

    fn flush(&mut self) -> Result<()> {
        if self.rows > 0 {
            let batch = RecordBatch::try_new(Arc::clone(&self.schema), self.builders.finish())?;
            self.writer.write(&batch)?;
            self.writer.flush()?;
            self.rows = 0;
        }
        Ok(())
    }

    fn finish(mut self) -> Result<u64> {
        self.flush()?;
        self.writer.finish()?;
        Ok(self.total)
    }
}

#[derive(Default)]
struct Spikes {
    t_ns: UInt64Builder,
    neuron: UInt64Builder,
}

impl Columns for Spikes {
    fn finish(&mut self) -> Vec<ArrayRef> {
        vec![Arc::new(self.t_ns.finish()), Arc::new(self.neuron.finish())]
    }
}

#[derive(Default)]
struct Weights {
    t_ns: UInt64Builder,
    post: UInt64Builder,
    synapse: UInt32Builder,
    pre: UInt64Builder,
    before: Float32Builder,
    after: Float32Builder,
}

impl Columns for Weights {
    fn finish(&mut self) -> Vec<ArrayRef> {
        vec![
            Arc::new(self.t_ns.finish()),
            Arc::new(self.post.finish()),
            Arc::new(self.synapse.finish()),
            Arc::new(self.pre.finish()),
            Arc::new(self.before.finish()),
            Arc::new(self.after.finish()),
        ]
    }
}

#[derive(Default)]
struct Membrane {
    t_ns: UInt64Builder,
    neuron: UInt64Builder,
    mv: Float32Builder,
}

impl Columns for Membrane {
    fn finish(&mut self) -> Vec<ArrayRef> {
        vec![
            Arc::new(self.t_ns.finish()),
            Arc::new(self.neuron.finish()),
            Arc::new(self.mv.finish()),
        ]
    }
}

#[derive(Default)]
struct Transitions {
    t_ns: UInt64Builder,
    neuron: UInt64Builder,
    tonic: BooleanBuilder,
}

impl Columns for Transitions {
    fn finish(&mut self) -> Vec<ArrayRef> {
        vec![
            Arc::new(self.t_ns.finish()),
            Arc::new(self.neuron.finish()),
            Arc::new(self.tonic.finish()),
        ]
    }
}

#[derive(Default)]
struct System {
    t_ns: UInt64Builder,
    wall_unix_ns: Int64Builder,
    severity: StringBuilder,
    kind: StringBuilder,
    detail: StringBuilder,
}

impl Columns for System {
    fn finish(&mut self) -> Vec<ArrayRef> {
        vec![
            Arc::new(self.t_ns.finish()),
            Arc::new(self.wall_unix_ns.finish()),
            Arc::new(self.severity.finish()),
            Arc::new(self.kind.finish()),
            Arc::new(self.detail.finish()),
        ]
    }
}

/// Row counts per streaming table.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct Counts {
    /// Rows in `spikes`.
    pub spikes: u64,
    /// Rows in `weights`.
    pub weights: u64,
    /// Rows in `membrane`.
    pub membrane: u64,
    /// Rows in `transitions`.
    pub transitions: u64,
    /// Rows in `system`.
    pub system: u64,
}

/// All streaming tables of a run.
pub(crate) struct Streams {
    spikes: Stream<Spikes>,
    weights: Stream<Weights>,
    membrane: Stream<Membrane>,
    transitions: Stream<Transitions>,
    system: Stream<System>,
    batch_rows: usize,
}

impl Streams {
    pub(crate) fn create(dir: &Path, batch_rows: usize) -> Result<Self> {
        use DataType::{Boolean, Float32, Int64, UInt32, UInt64, Utf8};
        Ok(Self {
            spikes: Stream::create(dir, "spikes", schema(vec![col("t_ns", UInt64), col("neuron", UInt64)]))?,
            weights: Stream::create(
                dir,
                "weights",
                schema(vec![
                    col("t_ns", UInt64),
                    col("post", UInt64),
                    col("synapse", UInt32),
                    col("pre", UInt64),
                    col("before", Float32),
                    col("after", Float32),
                ]),
            )?,
            membrane: Stream::create(
                dir,
                "membrane",
                schema(vec![col("t_ns", UInt64), col("neuron", UInt64), col("mv", Float32)]),
            )?,
            transitions: Stream::create(
                dir,
                "transitions",
                schema(vec![col("t_ns", UInt64), col("neuron", UInt64), col("tonic", Boolean)]),
            )?,
            system: Stream::create(
                dir,
                "system",
                schema(vec![
                    col("t_ns", UInt64),
                    col("wall_unix_ns", Int64),
                    col("severity", Utf8),
                    col("kind", Utf8),
                    col("detail", Utf8),
                ]),
            )?,
            batch_rows: batch_rows.max(1),
        })
    }

    pub(crate) fn event(&mut self, e: &Emitted<Event>) -> Result<()> {
        let neuron = e.src.raw();
        match e.event {
            Event::Spike { at } => {
                let b = &mut self.spikes.builders;
                b.t_ns.append_value(t_ns(at));
                b.neuron.append_value(neuron);
                self.spikes.row_added(self.batch_rows)
            }
            Event::Weight {
                synapse,
                pre,
                before,
                after,
            } => {
                let b = &mut self.weights.builders;
                b.t_ns.append_value(t_ns(e.at));
                b.post.append_value(neuron);
                b.synapse.append_value(synapse);
                b.pre.append_value(pre.raw());
                b.before.append_value(before);
                b.after.append_value(after);
                self.weights.row_added(self.batch_rows)
            }
            Event::Membrane { mv } => {
                let b = &mut self.membrane.builders;
                b.t_ns.append_value(t_ns(e.at));
                b.neuron.append_value(neuron);
                b.mv.append_value(mv);
                self.membrane.row_added(self.batch_rows)
            }
            Event::Transition(transition) => {
                let b = &mut self.transitions.builders;
                b.t_ns.append_value(t_ns(e.at));
                b.neuron.append_value(neuron);
                b.tonic.append_value(transition == Transition::StartedTonic);
                self.transitions.row_added(self.batch_rows)
            }
        }
    }

    pub(crate) fn notice(&mut self, n: &SystemNotice) -> Result<()> {
        let b = &mut self.system.builders;
        b.t_ns.append_value(t_ns(n.sim_time));
        let wall = n
            .wall_time
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| i64::try_from(d.as_nanos()).unwrap_or(i64::MAX));
        b.wall_unix_ns.append_value(wall);
        b.severity.append_value(format!("{:?}", n.severity).to_lowercase());
        let debug = format!("{:?}", n.event);
        let kind_end = debug.find([' ', '{', '(']).unwrap_or(debug.len());
        b.kind.append_value(&debug[..kind_end]);
        b.detail.append_value(debug[kind_end..].trim());
        self.system.row_added(self.batch_rows)
    }

    pub(crate) fn finish(self) -> Result<Counts> {
        Ok(Counts {
            spikes: self.spikes.finish()?,
            weights: self.weights.finish()?,
            membrane: self.membrane.finish()?,
            transitions: self.transitions.finish()?,
            system: self.system.finish()?,
        })
    }
}
