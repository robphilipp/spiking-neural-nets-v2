//! Physical quantities and their units. Every value is converted to the model's base unit for its quantity;
//! a value without a unit is taken to be in that base unit (as in v1's `dna-defs.md`).

use std::f64::consts::PI;

/// What a parameter measures, and therefore which units it accepts.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Quantity {
    /// Base unit mV.
    Potential,
    /// Base unit ms.
    Time,
    /// Base unit µm.
    Length,
    /// Base unit m/s.
    Speed,
    /// Base unit Hz.
    Frequency,
    /// Base unit µWb (= mV·ms).
    Flux,
    /// Base unit radians.
    Angle,
    /// No unit allowed.
    Dimensionless,
}

impl Quantity {
    /// The base unit's name, for messages.
    pub fn base_unit(self) -> &'static str {
        match self {
            Self::Potential => "mV",
            Self::Time => "ms",
            Self::Length => "µm",
            Self::Speed => "m/s",
            Self::Frequency => "Hz",
            Self::Flux => "µWb",
            Self::Angle => "rad",
            Self::Dimensionless => "(none)",
        }
    }

    /// The factor that converts `unit` to the base unit, or `None` if `unit` doesn't measure this quantity.
    pub fn factor(self, unit: &str) -> Option<f64> {
        // Accept "u" and the Greek mu (U+03BC) as well as the micro sign (U+00B5).
        let unit = unit.replace(['u', 'μ'], "µ");
        let f = match (self, unit.as_str()) {
            (Self::Potential, "V") => 1e3,
            (Self::Potential, "mV") => 1.0,
            (Self::Potential, "µV") => 1e-3,
            (Self::Potential, "nV") => 1e-6,
            (Self::Time, "h") => 3.6e6,
            (Self::Time, "min") => 6e4,
            (Self::Time, "s") => 1e3,
            (Self::Time, "ms") => 1.0,
            (Self::Time, "µs") => 1e-3,
            (Self::Time, "ns") => 1e-6,
            (Self::Length, "m") => 1e6,
            (Self::Length, "cm") => 1e4,
            (Self::Length, "mm") => 1e3,
            (Self::Length, "µm") => 1.0,
            (Self::Length, "nm") => 1e-3,
            (Self::Speed, "m/s") => 1.0,
            (Self::Speed, "cm/s") => 1e-2,
            (Self::Speed, "mm/s") => 1e-3,
            (Self::Speed, "µm/s") => 1e-6,
            (Self::Frequency, "kHz") => 1e3,
            (Self::Frequency, "Hz") => 1.0,
            (Self::Frequency, "mHz") => 1e-3,
            (Self::Flux, "Wb") => 1e6,
            (Self::Flux, "mWb") => 1e3,
            (Self::Flux, "µWb") => 1.0,
            (Self::Flux, "nWb") => 1e-3,
            (Self::Angle, "rad") => 1.0,
            (Self::Angle, "deg" | "°") => PI / 180.0,
            _ => return None,
        };
        Some(f)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conversions() {
        assert_eq!(Quantity::Time.factor("s"), Some(1e3));
        assert_eq!(Quantity::Time.factor("h"), Some(3.6e6));
        assert_eq!(Quantity::Length.factor("µm"), Some(1.0));
        assert_eq!(Quantity::Length.factor("um"), Some(1.0));
        assert_eq!(Quantity::Potential.factor("μV"), Some(1e-3));
        assert_eq!(Quantity::Flux.factor("µWb"), Some(1.0));
        assert_eq!(Quantity::Time.factor("mV"), None);
    }
}
