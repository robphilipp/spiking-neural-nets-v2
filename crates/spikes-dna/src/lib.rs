//! The Spikes network description language (`.boo` files): parser and description model.
//!
//! [`parse`] reads a file into a [`NetworkDescription`] with units converted to the model's base units (mV, ms,
//! µm, m/s, Hz), id sets expanded (`input-1{1:7:1}` → `input-11` … `input-17`), and every reference checked. All
//! problems are reported, each with its line and column.
//!
//! The language is v1's (see `notes/03-network-description-language.md` and v1's `dna-defs.md`), read leniently:
//! keys an older file lacks get v1's defaults, and older names (`fnc=stdp`, `SNT`, `uwb`) are accepted. v2 adds:
//!
//! | Key | Where | Meaning |
//! |-----|-------|---------|
//! | `nty=mr`, `nty=br` | neuron | monostable / bistable resonator |
//! | `RES=(frq=…, dmp=…, lcr=…, sep=…)` | resonator neuron | resonant frequency, damping time constant; for `br` the limit-cycle and separatrix radii |
//! | `HIP=(tgt=…, str=…, win=…, lwb=…, upb=…)` | neuron | homeostatic intrinsic plasticity: target rate, strength (mV per Hz·s), rate window, bounds |
//! | `typ=exc` / `typ=inh` | connection | the connection's sign (default: from the sending neuron's `inh`) |
//! | `nam=…` | learning or stickiness function | a name, so several of one type can coexist (default: its `fnc`) |

mod convert;
pub mod description;
mod syntax;
pub mod units;

pub use description::{
    ConnectionDescription, GroupDescription, NetworkDescription, NeuronDescription, SynapseDefaults, WeightSpec,
};
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
