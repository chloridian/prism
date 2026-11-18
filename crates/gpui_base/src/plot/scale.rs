mod band;
mod linear;
mod ordinal;
mod point;
mod sealed;

pub use band::ScaleBand;
pub use linear::ScaleLinear;
pub use ordinal::ScaleOrdinal;
pub use point::ScalePoint;

use std::ops::{Add, Div, Mul, Rem, Sub};

/// A number a [`ScaleLinear`] and the charts built on it can place: `f32`
/// and `f64`.
///
/// Sealed: integer types are left out on purpose, because a linear scale
/// divides values and integer division would truncate every position.
pub trait PlotValue:
    sealed::Sealed
    + Copy
    + PartialOrd
    + Add<Output = Self>
    + Sub<Output = Self>
    + Mul<Output = Self>
    + Div<Output = Self>
    + Rem<Output = Self>
    + 'static
{
    fn zero() -> Self;

    fn is_zero(&self) -> bool;

    fn to_f32(&self) -> Option<f32>;

    fn to_f64(&self) -> Option<f64>;
}

impl PlotValue for f32 {
    fn zero() -> Self {
        0.
    }

    fn is_zero(&self) -> bool {
        *self == 0.
    }

    fn to_f32(&self) -> Option<f32> {
        Some(*self)
    }

    fn to_f64(&self) -> Option<f64> {
        Some(f64::from(*self))
    }
}

impl PlotValue for f64 {
    fn zero() -> Self {
        0.
    }

    fn is_zero(&self) -> bool {
        *self == 0.
    }

    fn to_f32(&self) -> Option<f32> {
        Some(*self as f32)
    }

    fn to_f64(&self) -> Option<f64> {
        Some(*self)
    }
}

pub trait Scale<T> {
    /// Get the tick of the scale.
    fn tick(&self, value: &T) -> Option<f32>;

    /// The index of the domain value whose position is nearest `tick`; `0` for
    /// an empty domain or a scale without discrete positions.
    fn nearest_index(&self, _tick: f32) -> usize {
        0
    }
}
