//! The Spikes network description language (`.boo` files): parser and description model.
//!
//! [`parse`] reads a file into a [`NetworkDescription`] with units converted to the model's base units (mV, ms,
//! µm, m/s, Hz), neuron and synapse types applied (with each neuron's and connection's overrides), populations
//! placed, sets and connection ends resolved, and every reference checked. All problems are reported, each with its
//! line and column. The language is described in `docs/plans/dna/README.md`; [`schema`] has every key, and is what
//! the editor's completions and hover help come from.
//!
//! [`migrate`] converts a file in the older (v1-derived) form to this one.

mod convert;
pub mod description;
mod migrate;
mod neurons;
pub mod schema;
mod schema_details;
mod syntax;
pub mod units;

pub use description::{
    ConnectionDescription, DEFAULT_RELEASE, ModulatorDescription, NetworkDescription, NeuronDescription, Pattern,
    PopulationDescription, SetDescription, SynapseSpec, WeightSpec,
};
pub use migrate::migrate;
pub use neurons::{NeuronNames, NeuronView, Neurons};
pub use syntax::{ParseError, Span};

/// Parses a `.boo` network description.
///
/// # Errors
/// Every problem found, in source order: syntax errors stop at the first; otherwise all unknown keys, bad units,
/// missing values and unresolved references are reported together.
pub fn parse(text: &str) -> Result<NetworkDescription, Vec<ParseError>> {
    let root = syntax::parse(text).map_err(|e| vec![e])?;
    convert::convert(&root)
}
