//! Simulation time.
//!
//! Simulation time is logical. It is carried on every message and is never read from the wall clock inside an
//! actor. Both types are integer nanoseconds, so arithmetic is exact and there is no floating-point drift.
//!
//! - [`SimTime`] is a point in simulation time: nanoseconds since the simulation started.
//! - [`SimDuration`] is a span of simulation time, such as a synaptic delay or a timer interval.
//!
//! Model code usually computes delays as `f64` (for example `distance / conductance_speed`). Convert them once, at
//! connect time, with [`SimDuration::from_micros_f64`] and friends, which round to the nearest nanosecond.

use core::fmt;
use core::iter::Sum;
use core::ops::{Add, AddAssign, Mul, Sub, SubAssign};

const NANOS_PER_MICRO: u64 = 1_000;
const NANOS_PER_MILLI: u64 = 1_000_000;
const NANOS_PER_SEC: u64 = 1_000_000_000;

/// A span of simulation time, in integer nanoseconds.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize), serde(transparent))]
pub struct SimDuration(u64);

/// A point in simulation time: nanoseconds since the simulation started.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize), serde(transparent))]
pub struct SimTime(u64);

/// Why a floating-point value could not be converted to a [`SimDuration`].
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum FromFloatError {
    /// The value was NaN or infinite.
    NotFinite,
    /// The value was negative. Durations can't be negative.
    Negative,
    /// The value was too large to fit in a `u64` of nanoseconds (about 584 years).
    Overflow,
}

impl fmt::Display for FromFloatError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::NotFinite => "duration is not a finite number",
            Self::Negative => "duration is negative",
            Self::Overflow => "duration is too large for SimDuration",
        })
    }
}

impl std::error::Error for FromFloatError {}

/// Multiplies at compile time or panics with a clear message (used by the integer constructors).
const fn scale(value: u64, factor: u64) -> u64 {
    match value.checked_mul(factor) {
        Some(nanos) => nanos,
        None => panic!("SimDuration overflow"),
    }
}

impl SimDuration {
    /// Zero duration.
    pub const ZERO: Self = Self(0);
    /// The largest representable duration (about 584 years).
    pub const MAX: Self = Self(u64::MAX);
    /// One nanosecond, the resolution of simulation time.
    pub const NANOSECOND: Self = Self(1);
    /// One microsecond.
    pub const MICROSECOND: Self = Self(NANOS_PER_MICRO);
    /// One millisecond.
    pub const MILLISECOND: Self = Self(NANOS_PER_MILLI);
    /// One second.
    pub const SECOND: Self = Self(NANOS_PER_SEC);

    /// Creates a duration from whole nanoseconds.
    pub const fn from_nanos(nanos: u64) -> Self {
        Self(nanos)
    }

    /// Creates a duration from whole microseconds.
    ///
    /// # Panics
    /// If the result overflows.
    pub const fn from_micros(micros: u64) -> Self {
        Self(scale(micros, NANOS_PER_MICRO))
    }

    /// Creates a duration from whole milliseconds.
    ///
    /// # Panics
    /// If the result overflows.
    pub const fn from_millis(millis: u64) -> Self {
        Self(scale(millis, NANOS_PER_MILLI))
    }

    /// Creates a duration from whole seconds.
    ///
    /// # Panics
    /// If the result overflows.
    pub const fn from_secs(secs: u64) -> Self {
        Self(scale(secs, NANOS_PER_SEC))
    }

    /// Converts fractional nanoseconds, rounding to the nearest nanosecond.
    ///
    /// # Errors
    /// If `nanos` is NaN, infinite, negative, or too large.
    pub fn try_from_nanos_f64(nanos: f64) -> Result<Self, FromFloatError> {
        if !nanos.is_finite() {
            return Err(FromFloatError::NotFinite);
        }
        if nanos < 0.0 {
            return Err(FromFloatError::Negative);
        }
        let rounded = nanos.round();
        // u64::MAX as f64 rounds up to exactly 2^64, so anything at or above it doesn't fit.
        if rounded >= u64::MAX as f64 {
            return Err(FromFloatError::Overflow);
        }
        Ok(Self(rounded as u64))
    }

    /// Converts fractional microseconds, rounding to the nearest nanosecond.
    ///
    /// # Errors
    /// If `micros` is NaN, infinite, negative, or too large.
    pub fn try_from_micros_f64(micros: f64) -> Result<Self, FromFloatError> {
        Self::try_from_nanos_f64(micros * NANOS_PER_MICRO as f64)
    }

    /// Converts fractional milliseconds, rounding to the nearest nanosecond.
    ///
    /// # Errors
    /// If `millis` is NaN, infinite, negative, or too large.
    pub fn try_from_millis_f64(millis: f64) -> Result<Self, FromFloatError> {
        Self::try_from_nanos_f64(millis * NANOS_PER_MILLI as f64)
    }

    /// Converts fractional seconds, rounding to the nearest nanosecond.
    ///
    /// # Errors
    /// If `secs` is NaN, infinite, negative, or too large.
    pub fn try_from_secs_f64(secs: f64) -> Result<Self, FromFloatError> {
        Self::try_from_nanos_f64(secs * NANOS_PER_SEC as f64)
    }

    /// Converts fractional microseconds, rounding to the nearest nanosecond.
    ///
    /// # Panics
    /// If `micros` is NaN, infinite, negative, or too large. Use [`Self::try_from_micros_f64`] to handle that.
    pub fn from_micros_f64(micros: f64) -> Self {
        Self::try_from_micros_f64(micros).unwrap_or_else(|e| panic!("invalid duration {micros} µs: {e}"))
    }

    /// Converts fractional milliseconds, rounding to the nearest nanosecond.
    ///
    /// # Panics
    /// If `millis` is NaN, infinite, negative, or too large. Use [`Self::try_from_millis_f64`] to handle that.
    pub fn from_millis_f64(millis: f64) -> Self {
        Self::try_from_millis_f64(millis).unwrap_or_else(|e| panic!("invalid duration {millis} ms: {e}"))
    }

    /// Converts fractional seconds, rounding to the nearest nanosecond.
    ///
    /// # Panics
    /// If `secs` is NaN, infinite, negative, or too large. Use [`Self::try_from_secs_f64`] to handle that.
    pub fn from_secs_f64(secs: f64) -> Self {
        Self::try_from_secs_f64(secs).unwrap_or_else(|e| panic!("invalid duration {secs} s: {e}"))
    }

    /// Whole nanoseconds.
    pub const fn as_nanos(self) -> u64 {
        self.0
    }

    /// Fractional microseconds.
    pub fn as_micros_f64(self) -> f64 {
        self.0 as f64 / NANOS_PER_MICRO as f64
    }

    /// Fractional milliseconds.
    pub fn as_millis_f64(self) -> f64 {
        self.0 as f64 / NANOS_PER_MILLI as f64
    }

    /// Fractional seconds.
    pub fn as_secs_f64(self) -> f64 {
        self.0 as f64 / NANOS_PER_SEC as f64
    }

    /// `true` for a zero-length duration.
    pub const fn is_zero(self) -> bool {
        self.0 == 0
    }

    /// `self + rhs`, or `None` on overflow.
    pub const fn checked_add(self, rhs: Self) -> Option<Self> {
        match self.0.checked_add(rhs.0) {
            Some(n) => Some(Self(n)),
            None => None,
        }
    }

    /// `self - rhs`, or `None` if `rhs` is larger.
    pub const fn checked_sub(self, rhs: Self) -> Option<Self> {
        match self.0.checked_sub(rhs.0) {
            Some(n) => Some(Self(n)),
            None => None,
        }
    }

    /// `self * rhs`, or `None` on overflow.
    pub const fn checked_mul(self, rhs: u64) -> Option<Self> {
        match self.0.checked_mul(rhs) {
            Some(n) => Some(Self(n)),
            None => None,
        }
    }

    /// `self + rhs`, clamped to [`Self::MAX`].
    pub const fn saturating_add(self, rhs: Self) -> Self {
        Self(self.0.saturating_add(rhs.0))
    }

    /// `self - rhs`, clamped to zero.
    pub const fn saturating_sub(self, rhs: Self) -> Self {
        Self(self.0.saturating_sub(rhs.0))
    }
}

impl SimTime {
    /// The start of the simulation.
    pub const ZERO: Self = Self(0);
    /// The latest representable time (about 584 years after the start).
    pub const MAX: Self = Self(u64::MAX);

    /// Creates a time from nanoseconds since the start.
    pub const fn from_nanos(nanos: u64) -> Self {
        Self(nanos)
    }

    /// The time at `elapsed` after the start.
    pub const fn from_start(elapsed: SimDuration) -> Self {
        Self(elapsed.0)
    }

    /// Nanoseconds since the start.
    pub const fn as_nanos(self) -> u64 {
        self.0
    }

    /// Time elapsed since the start.
    pub const fn since_start(self) -> SimDuration {
        SimDuration(self.0)
    }

    /// `self + d`, or `None` on overflow.
    pub const fn checked_add(self, d: SimDuration) -> Option<Self> {
        match self.0.checked_add(d.0) {
            Some(n) => Some(Self(n)),
            None => None,
        }
    }

    /// `self - d`, or `None` if that would be before the start.
    pub const fn checked_sub(self, d: SimDuration) -> Option<Self> {
        match self.0.checked_sub(d.0) {
            Some(n) => Some(Self(n)),
            None => None,
        }
    }

    /// `self + d`, clamped to [`Self::MAX`].
    pub const fn saturating_add(self, d: SimDuration) -> Self {
        Self(self.0.saturating_add(d.0))
    }

    /// Time from `earlier` to `self`, or `None` if `earlier` is later than `self`.
    pub const fn checked_duration_since(self, earlier: Self) -> Option<SimDuration> {
        match self.0.checked_sub(earlier.0) {
            Some(n) => Some(SimDuration(n)),
            None => None,
        }
    }

    /// Time from `earlier` to `self`, or zero if `earlier` is later than `self`.
    pub const fn saturating_duration_since(self, earlier: Self) -> SimDuration {
        SimDuration(self.0.saturating_sub(earlier.0))
    }
}

impl Add for SimDuration {
    type Output = Self;
    fn add(self, rhs: Self) -> Self {
        self.checked_add(rhs).expect("SimDuration overflow")
    }
}

impl AddAssign for SimDuration {
    fn add_assign(&mut self, rhs: Self) {
        *self = *self + rhs;
    }
}

impl Sub for SimDuration {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self {
        self.checked_sub(rhs)
            .expect("SimDuration underflow: subtracted a longer duration")
    }
}

impl SubAssign for SimDuration {
    fn sub_assign(&mut self, rhs: Self) {
        *self = *self - rhs;
    }
}

impl Mul<u64> for SimDuration {
    type Output = Self;
    fn mul(self, rhs: u64) -> Self {
        self.checked_mul(rhs).expect("SimDuration overflow")
    }
}

impl Sum for SimDuration {
    fn sum<I: Iterator<Item = Self>>(iter: I) -> Self {
        iter.fold(Self::ZERO, Add::add)
    }
}

impl Add<SimDuration> for SimTime {
    type Output = Self;
    fn add(self, rhs: SimDuration) -> Self {
        self.checked_add(rhs).expect("SimTime overflow")
    }
}

impl AddAssign<SimDuration> for SimTime {
    fn add_assign(&mut self, rhs: SimDuration) {
        *self = *self + rhs;
    }
}

impl Sub<SimDuration> for SimTime {
    type Output = Self;
    fn sub(self, rhs: SimDuration) -> Self {
        self.checked_sub(rhs)
            .expect("SimTime underflow: before the start of the simulation")
    }
}

impl SubAssign<SimDuration> for SimTime {
    fn sub_assign(&mut self, rhs: SimDuration) {
        *self = *self - rhs;
    }
}

impl Sub for SimTime {
    type Output = SimDuration;
    /// # Panics
    /// If `rhs` is later than `self`. Use [`SimTime::checked_duration_since`] when that can happen.
    fn sub(self, rhs: Self) -> SimDuration {
        self.checked_duration_since(rhs)
            .expect("SimTime subtraction: rhs is later than self")
    }
}

impl From<SimDuration> for std::time::Duration {
    fn from(d: SimDuration) -> Self {
        std::time::Duration::from_nanos(d.0)
    }
}

impl TryFrom<std::time::Duration> for SimDuration {
    type Error = core::num::TryFromIntError;
    fn try_from(d: std::time::Duration) -> Result<Self, Self::Error> {
        u64::try_from(d.as_nanos()).map(Self)
    }
}

/// Formats nanoseconds with the largest unit that is at most the value, keeping exact decimals,
/// e.g. `0 ns`, `750 ns`, `12.5 µs`, `3.000001 ms`, `100 s`.
fn fmt_nanos(nanos: u64, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    let (divisor, digits, unit) = if nanos >= NANOS_PER_SEC {
        (NANOS_PER_SEC, 9, "s")
    } else if nanos >= NANOS_PER_MILLI {
        (NANOS_PER_MILLI, 6, "ms")
    } else if nanos >= NANOS_PER_MICRO {
        (NANOS_PER_MICRO, 3, "µs")
    } else {
        return write!(f, "{nanos} ns");
    };
    let whole = nanos / divisor;
    let frac = nanos % divisor;
    if frac == 0 {
        write!(f, "{whole} {unit}")
    } else {
        let frac = format!("{frac:0digits$}");
        write!(f, "{whole}.{} {unit}", frac.trim_end_matches('0'))
    }
}

impl fmt::Display for SimDuration {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt_nanos(self.0, f)
    }
}

impl fmt::Display for SimTime {
    /// Displays the time since the start, e.g. `t=12.5 ms`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("t=")?;
        fmt_nanos(self.0, f)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn integer_constructors() {
        assert_eq!(SimDuration::from_micros(1).as_nanos(), 1_000);
        assert_eq!(SimDuration::from_millis(1).as_nanos(), 1_000_000);
        assert_eq!(SimDuration::from_secs(1).as_nanos(), 1_000_000_000);
        assert_eq!(SimDuration::from_secs(3600), SimDuration::from_millis(3_600_000));
    }

    #[test]
    #[should_panic(expected = "SimDuration overflow")]
    fn integer_constructor_overflow_panics() {
        let _ = SimDuration::from_secs(u64::MAX);
    }

    #[test]
    fn float_conversion_rounds_to_nearest_nanosecond() {
        assert_eq!(SimDuration::from_micros_f64(12.5).as_nanos(), 12_500);
        assert_eq!(SimDuration::from_micros_f64(0.0004).as_nanos(), 0);
        assert_eq!(SimDuration::from_micros_f64(0.0006).as_nanos(), 1);
        assert_eq!(SimDuration::from_millis_f64(2.5).as_nanos(), 2_500_000);
        // 10 µm at 1 m/s = 10 µs: the typical synaptic delay that v1 rounded away.
        let delay_ms = 10e-6 / 1.0 * 1e3;
        assert_eq!(SimDuration::from_millis_f64(delay_ms), SimDuration::from_micros(10));
    }

    #[test]
    fn float_conversion_rejects_invalid_values() {
        assert_eq!(
            SimDuration::try_from_micros_f64(f64::NAN),
            Err(FromFloatError::NotFinite)
        );
        assert_eq!(
            SimDuration::try_from_micros_f64(f64::INFINITY),
            Err(FromFloatError::NotFinite)
        );
        assert_eq!(SimDuration::try_from_micros_f64(-1.0), Err(FromFloatError::Negative));
        assert_eq!(SimDuration::try_from_secs_f64(1e12), Err(FromFloatError::Overflow));
        assert_eq!(SimDuration::try_from_micros_f64(-0.0), Ok(SimDuration::ZERO));
    }

    #[test]
    fn time_arithmetic() {
        let t = SimTime::ZERO + SimDuration::from_millis(5);
        let later = t + SimDuration::from_micros(10);
        assert_eq!(later - t, SimDuration::from_micros(10));
        assert_eq!(later - SimDuration::from_micros(10), t);
        assert_eq!(t.checked_duration_since(later), None);
        assert_eq!(t.saturating_duration_since(later), SimDuration::ZERO);
        assert_eq!(SimTime::MAX.checked_add(SimDuration::NANOSECOND), None);
        assert_eq!(SimTime::MAX.saturating_add(SimDuration::SECOND), SimTime::MAX);
    }

    #[test]
    #[should_panic(expected = "rhs is later than self")]
    fn time_subtraction_underflow_panics() {
        let _ = SimTime::ZERO - SimTime::from_nanos(1);
    }

    #[test]
    fn duration_sum_and_mul() {
        let total: SimDuration = [1, 2, 3].into_iter().map(SimDuration::from_micros).sum();
        assert_eq!(total, SimDuration::from_micros(6));
        assert_eq!(SimDuration::from_micros(7) * 3, SimDuration::from_micros(21));
    }

    #[test]
    fn std_duration_round_trip() {
        let d = SimDuration::from_nanos(123_456_789);
        let std: std::time::Duration = d.into();
        assert_eq!(SimDuration::try_from(std), Ok(d));
        assert!(SimDuration::try_from(std::time::Duration::MAX).is_err());
    }

    #[test]
    fn display_uses_readable_units() {
        assert_eq!(SimDuration::ZERO.to_string(), "0 ns");
        assert_eq!(SimDuration::from_nanos(750).to_string(), "750 ns");
        assert_eq!(SimDuration::from_nanos(12_500).to_string(), "12.5 µs");
        assert_eq!(SimDuration::from_nanos(3_000_001).to_string(), "3.000001 ms");
        assert_eq!(SimDuration::from_secs(100).to_string(), "100 s");
        assert_eq!(SimTime::from_nanos(12_500_000).to_string(), "t=12.5 ms");
    }
}
