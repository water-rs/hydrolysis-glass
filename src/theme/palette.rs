//! Colour roles the theme draws its non-glass chrome with.
//!
//! The glass material itself has no palette: its colour comes from the
//! backdrop it refracts. These roles cover everything around it — labels,
//! toggles, tracks, dividers, menus — and the tint a prominent capsule folds
//! into its body.

use cherenkov::WorkingColor;
use waterui::color::{ColorScheme, Srgb};

use crate::material::recipe::Appearance;

/// sRGB-encoded colour with straight alpha, `0..=1` per channel.
pub type Rgba = [f32; 4];

/// One complete set of colour roles.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Palette {
    /// The material appearance this palette pairs with.
    pub appearance: Appearance,
    /// Window background.
    pub background: Rgba,
    /// Opaque surface for menus and popups, which are not glass.
    pub surface: Rgba,
    /// A slightly raised opaque surface (table headers, alternate rows).
    pub surface_variant: Rgba,
    /// Hairlines and outlines.
    pub border: Rgba,
    /// Primary text.
    pub foreground: Rgba,
    /// Secondary text and placeholders.
    pub muted_foreground: Rgba,
    /// Accent for selection, progress, prominent capsules.
    pub accent: Rgba,
    /// Text drawn on the accent.
    pub accent_foreground: Rgba,
    /// Destructive actions.
    pub error: Rgba,
    /// Text drawn on the error colour.
    pub error_foreground: Rgba,
    /// Translucent state layer laid over pressed or hovered chrome.
    pub state_layer: Rgba,
    /// Track colour behind toggles, sliders and progress.
    pub track: Rgba,
    /// The knob of a toggle switch and the handle of a slider.
    pub knob: Rgba,
}

impl Palette {
    /// Roles for the light appearance.
    pub const LIGHT: Self = Self {
        appearance: Appearance::Light,
        background: [0.949, 0.949, 0.969, 1.0],
        surface: [1.0, 1.0, 1.0, 1.0],
        surface_variant: [0.949, 0.949, 0.969, 1.0],
        border: [0.235, 0.235, 0.263, 0.29],
        foreground: [0.0, 0.0, 0.0, 1.0],
        muted_foreground: [0.235, 0.235, 0.263, 0.6],
        accent: [0.0, 0.478, 1.0, 1.0],
        accent_foreground: [1.0, 1.0, 1.0, 1.0],
        error: [1.0, 0.231, 0.188, 1.0],
        error_foreground: [1.0, 1.0, 1.0, 1.0],
        state_layer: [0.0, 0.0, 0.0, 1.0],
        track: [0.471, 0.471, 0.502, 0.16],
        knob: [1.0, 1.0, 1.0, 1.0],
    };

    /// Roles for the dark appearance.
    pub const DARK: Self = Self {
        appearance: Appearance::Dark,
        background: [0.0, 0.0, 0.0, 1.0],
        surface: [0.11, 0.11, 0.118, 1.0],
        surface_variant: [0.173, 0.173, 0.18, 1.0],
        border: [0.329, 0.329, 0.345, 0.6],
        foreground: [1.0, 1.0, 1.0, 1.0],
        muted_foreground: [0.922, 0.922, 0.961, 0.6],
        accent: [0.039, 0.518, 1.0, 1.0],
        accent_foreground: [1.0, 1.0, 1.0, 1.0],
        error: [1.0, 0.271, 0.227, 1.0],
        error_foreground: [1.0, 1.0, 1.0, 1.0],
        state_layer: [1.0, 1.0, 1.0, 1.0],
        track: [0.471, 0.471, 0.502, 0.32],
        knob: [1.0, 1.0, 1.0, 1.0],
    };

    /// The palette for a `WaterUI` colour scheme.
    #[must_use]
    pub const fn for_scheme(scheme: ColorScheme) -> Self {
        match scheme {
            ColorScheme::Light => Self::LIGHT,
            ColorScheme::Dark => Self::DARK,
        }
    }

    /// The `WaterUI` colour scheme this palette belongs to.
    #[must_use]
    pub const fn scheme(&self) -> ColorScheme {
        match self.appearance {
            Appearance::Light => ColorScheme::Light,
            Appearance::Dark => ColorScheme::Dark,
        }
    }
}

/// Convert a palette colour into the engine's working space.
#[must_use]
pub fn working(c: Rgba) -> WorkingColor {
    WorkingColor::from(Srgb::new(c[0], c[1], c[2])).with_alpha(c[3])
}

/// The same colour at another alpha.
#[must_use]
pub const fn alpha(c: Rgba, a: f32) -> Rgba {
    [c[0], c[1], c[2], a]
}

/// `c` composited over `under`, both straight alpha; the result is opaque
/// when `under` is.
#[must_use]
pub fn over(c: Rgba, under: Rgba) -> Rgba {
    let a = c[3];
    let out_a = under[3].mul_add(1.0 - a, a);
    if out_a <= 0.0 {
        return [0.0; 4];
    }
    let ch = |i: usize| (under[i] * under[3]).mul_add(1.0 - a, c[i] * a) / out_a;
    [ch(0), ch(1), ch(2), out_a]
}

/// Convert a palette colour into a resolved `WaterUI` colour.
#[must_use]
pub fn resolved(c: Rgba) -> WorkingColor {
    working(c)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn over_opaque_stays_opaque() {
        let out = over([1.0, 0.0, 0.0, 0.5], [0.0, 0.0, 1.0, 1.0]);
        assert!((out[3] - 1.0).abs() < 1e-6);
        assert!((out[0] - 0.5).abs() < 1e-6);
        assert!((out[2] - 0.5).abs() < 1e-6);
    }

    #[test]
    fn schemes_round_trip() {
        assert_eq!(
            Palette::for_scheme(ColorScheme::Dark).scheme(),
            ColorScheme::Dark
        );
        assert_eq!(
            Palette::for_scheme(ColorScheme::Light).scheme(),
            ColorScheme::Light
        );
    }
}
