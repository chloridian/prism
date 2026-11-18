//! Upstream-shaped accessors over gpui-ce's palette colour types, so code
//! written against `gpui-kit` ports with small edits.

use anyhow::{Result, anyhow};
use gpui::{Hsla, Rgba, hsla};
use palette::IntoColor as _;

/// HSLA accessors with the units upstream used: hue, saturation, lightness
/// and alpha are all fractions in `0.0..=1.0`.
pub trait HslaExt: Sized {
    fn h(&self) -> f32;
    fn s(&self) -> f32;
    fn l(&self) -> f32;
    fn a(&self) -> f32;

    /// Replaces the alpha.
    fn alpha(&self, alpha: f32) -> Self;

    fn to_rgb(&self) -> Rgba;
}

impl HslaExt for Hsla {
    fn h(&self) -> f32 {
        self.color.hue.into_positive_degrees() / 360.
    }

    fn s(&self) -> f32 {
        self.color.saturation
    }

    fn l(&self) -> f32 {
        self.color.lightness
    }

    fn a(&self) -> f32 {
        self.alpha
    }

    fn alpha(&self, alpha: f32) -> Self {
        hsla(self.h(), self.s(), self.l(), alpha)
    }

    fn to_rgb(&self) -> Rgba {
        (*self).into_color()
    }
}

/// RGBA accessors with the names upstream's fields had.
pub trait RgbaExt {
    fn r(&self) -> f32;
    fn g(&self) -> f32;
    fn b(&self) -> f32;
    fn a(&self) -> f32;
}

impl RgbaExt for Rgba {
    fn r(&self) -> f32 {
        self.color.red
    }

    fn g(&self) -> f32 {
        self.color.green
    }

    fn b(&self) -> f32 {
        self.color.blue
    }

    fn a(&self) -> f32 {
        self.alpha
    }
}

/// Parses `#rgb`, `#rgba`, `#rrggbb` and `#rrggbbaa`.
pub fn rgba_from_hex(hex: &str) -> Result<Rgba> {
    let digits = hex
        .strip_prefix('#')
        .ok_or_else(|| anyhow!("invalid hex color `{hex}`: missing `#`"))?;
    let nibble = |c: char| {
        c.to_digit(16)
            .ok_or_else(|| anyhow!("invalid hex color `{hex}`"))
    };
    let chars: Vec<char> = digits.chars().collect();
    let channels: Vec<u32> = match chars.len() {
        3 | 4 => chars
            .iter()
            .map(|c| nibble(*c).map(|n| n * 17))
            .collect::<Result<_>>()?,
        6 | 8 => chars
            .chunks(2)
            .map(|pair| Ok(nibble(pair[0])? * 16 + nibble(pair[1])?))
            .collect::<Result<_>>()?,
        _ => return Err(anyhow!("invalid hex color `{hex}`: bad length")),
    };
    let channel = |i: usize| channels.get(i).map_or(1., |v| *v as f32 / 255.);
    Ok(Rgba::new(channel(0), channel(1), channel(2), channel(3)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hsla_accessors_round_trip_upstream_units() {
        let color = hsla(0.25, 0.5, 0.75, 0.5);
        assert!((color.h() - 0.25).abs() < 1e-5);
        assert_eq!((color.s(), color.l(), color.a()), (0.5, 0.75, 0.5));
        assert_eq!(color.alpha(1.).a(), 1.);
    }

    #[test]
    fn hex_parsing_accepts_every_length_and_rejects_garbage() {
        let rgb = rgba_from_hex("#ff8000").unwrap();
        assert_eq!((rgb.r(), rgb.a()), (1., 1.));
        assert!((rgb.g() - 128. / 255.).abs() < 1e-5);
        assert_eq!(rgba_from_hex("#f00").unwrap().r(), 1.);
        assert!((rgba_from_hex("#ff000080").unwrap().a() - 128. / 255.).abs() < 1e-5);
        assert!((rgba_from_hex("#f008").unwrap().a() - 136. / 255.).abs() < 1e-5);
        assert!(rgba_from_hex("ff0000").is_err());
        assert!(rgba_from_hex("#ff00").is_ok());
        assert!(rgba_from_hex("#ff000").is_err());
        assert!(rgba_from_hex("#gg0000").is_err());
    }
}
