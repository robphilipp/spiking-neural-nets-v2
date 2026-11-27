//! Input signals (app plan 05): the formula language shared with the UI's designer, seeded noise, and the encoders
//! that turn samples into neuron input (used by sensors and by signal sources).

pub mod encode;
pub mod formula;
pub mod noise;

pub use encode::{Channel, SampleEncoding};
pub use formula::{At, Formula, FormulaError};
