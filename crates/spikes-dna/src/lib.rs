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
//! | `syd=…` | neuron, connection | synaptic delay (default 0.5 ms; on a neuron, for its outgoing connections; on a connection, overriding it). A connection's delay is `syd + distance / csp` |
//! | `nam=…` | learning or stickiness function | a name, so several of one type can coexist (default: its `fnc`) |
//! | `MOD=[(nam=…, clr=…, rad=…, spd=…, lat=…, len=…)]` | top level | neuromodulators: clearance time, reach, spreading speed, latency, spatial falloff |
//! | `RCP=[(mod=…, exg=…, ing=…, thr=…, pla=…, elg=…, rat=…, dir=…)]` | neuron | receptors: excitatory/inhibitory gain, threshold, plasticity, three-factor learning (eligibility time, rate), direct potential |
//! | `REL=(mod=…, amt=…)` | neuron | releases a modulator when the neuron spikes |

mod convert;
pub mod description;
mod syntax;
pub mod units;

pub use description::{
    ConnectionDescription, GroupDescription, ModulatorDescription, NetworkDescription, NeuronDescription,
    SynapseDefaults, WeightSpec,
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
