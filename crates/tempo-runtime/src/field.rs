//! Shard-local fields: shared state that every actor on a shard can read, updated by timed broadcasts.
//!
//! A field is the framework's tool for tick-free, region-wide effects such as neuromodulator concentrations
//! (see `docs/plans/actors/09-neuromodulation-and-synapse-signs.md`). Every shard holds its own copy. An update,
//! for example "dopamine released at time t, position x, amount A", is broadcast to every shard and applied at
//! its simulation time, in event order. Actors read the field *lazily*, when they handle their next event, by
//! evaluating it at their own position and time: no clock ticks and no message per actor.
//!
//! ```
//! use tempo_runtime::{Field, SimTime};
//!
//! /// Releases of one modulator; concentration decays exponentially after each release.
//! #[derive(Clone, Default)]
//! struct Dopamine { releases: Vec<(SimTime, f64)> }
//!
//! impl Field for Dopamine {
//!     type Update = f64; // amount released
//!     fn apply(&mut self, at: SimTime, amount: f64) { self.releases.push((at, amount)); }
//! }
//!
//! impl Dopamine {
//!     fn concentration(&self, now: SimTime, tau_ms: f64) -> f64 {
//!         self.releases.iter()
//!             .filter(|(t, _)| *t <= now)
//!             .map(|(t, a)| a * (-(now - *t).as_millis_f64() / tau_ms).exp())
//!             .sum()
//!     }
//! }
//! ```

use std::any::Any;

use tempo_core::SimTime;

/// Shard-local state updated by timed broadcasts. See the module documentation.
pub trait Field: Clone + Send + 'static {
    /// What a broadcast carries.
    type Update: Clone + Send + 'static;

    /// Applies an update at simulation time `at`. Updates arrive in event order.
    fn apply(&mut self, at: SimTime, update: Self::Update);
}

pub(crate) trait AnyField: Send {
    fn apply_any(&mut self, at: SimTime, update: Box<dyn Any + Send>);
    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;
}

impl<F: Field> AnyField for F {
    fn apply_any(&mut self, at: SimTime, update: Box<dyn Any + Send>) {
        let update = update
            .downcast::<F::Update>()
            .expect("field update matches its field type");
        self.apply(at, *update);
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}
