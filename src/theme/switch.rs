//! The glass switch (specification §8.2): geometry, colours, the toggle
//! motion and the glass knob a touch holds.
//!
//! Everything here is a pure function of the specification's tables; the
//! switch look ([`super::looks`]) builds a switch from the same decisions.

use cherenkov_record::kurbo::{Rect, RoundedRectRadii};
use cherenkov_record::{ContinuousRect, MaterialEffect, ShapeData};
use waterui_core::layout::LayoutDirection;

use crate::material::geometry::{CornerCurve, Element, Rect as ElementRect, Shape};
use crate::material::recipe::{Appearance, Family, Recipe, Translucency};

use super::button::Spring;
use super::materials::{self, CONTINUOUS_SMOOTHING};
use super::palette::Rgba;

/// The switch's layout frame, pt **[measured]**.
pub const FRAME: [f64; 2] = [61.0, 28.0];
/// The track, pt: 2 pt longer than the frame, overhanging its trailing
/// edge, and taking no layout space **[measured]**.
pub const TRACK: [f64; 2] = [63.0, 28.0];
/// The knob at rest, pt **[measured]**.
pub const KNOB: [f32; 2] = [37.0, 24.0];
/// Gap between the knob and the track's edges, pt **[measured]**.
pub const KNOB_INSET: f64 = 2.0;
/// How far the knob travels from off to on, pt **[measured]**.
pub const TRAVEL: f32 = 22.0;

/// The track's rectangle for a switch laid out at `frame` in `direction`:
/// its leading edge on the frame's leading edge, overhanging the trailing
/// one, and the frame's height.
#[must_use]
pub fn track_rect(frame: Rect, direction: LayoutDirection) -> Rect {
    let x0 = match direction {
        LayoutDirection::LeftToRight => frame.x0,
        LayoutDirection::RightToLeft => frame.x1 - TRACK[0],
    };
    Rect::new(x0, frame.y0, x0 + TRACK[0], frame.y0 + TRACK[1])
}

/// The knob's rectangle on `track` at toggle progress `q` (0 off, 1 on),
/// grown about its centre by [`LENS_GROWTH`] times the press progress `p`.
#[must_use]
pub fn knob_rect(track: Rect, direction: LayoutDirection, q: f64, p: f64) -> Rect {
    let travel = f64::from(TRAVEL);
    let knob_w = f64::from(KNOB[0]);
    let knob_h = f64::from(KNOB[1]);
    let x0 = match direction {
        LayoutDirection::LeftToRight => travel.mul_add(q, track.x0 + KNOB_INSET),
        LayoutDirection::RightToLeft => (-q).mul_add(travel, track.x1 - KNOB_INSET - knob_w),
    };
    let rest = Rect::new(
        x0,
        track.y0 + KNOB_INSET,
        x0 + knob_w,
        track.y0 + KNOB_INSET + knob_h,
    );
    Rect::from_center_size(rest.center(), rest.size() * LENS_GROWTH.mul_add(p, 1.0))
}

/// How much a held knob grows: the lens is the knob scaled by `1 + 0.60·p`
/// about its centre (§1.4: `59.2 × 38.4 pt`).
pub const LENS_GROWTH: f64 = 0.60;

/// The capsule of `rect`, with the continuous corners every glass capsule
/// has.
#[must_use]
pub fn capsule(rect: Rect) -> ShapeData {
    ShapeData::Continuous(
        ContinuousRect::new(
            rect,
            RoundedRectRadii::from_single_radius(rect.height() / 2.0),
        )
        .with_smoothing(CONTINUOUS_SMOOTHING),
    )
}

/// The off track's fill: the secondary label colour at 30 % alpha, so it
/// reads as the backdrop shifted toward grey **[measured]**.
#[must_use]
pub const fn off_fill(appearance: Appearance) -> Rgba {
    match appearance {
        Appearance::Dark => [235.0 / 255.0, 235.0 / 255.0, 245.0 / 255.0, 0.30],
        Appearance::Light => [60.0 / 255.0, 60.0 / 255.0, 67.0 / 255.0, 0.30],
    }
}

/// The default tint: the theme's Display P3 accent colour **[measured]**.
#[must_use]
pub const fn default_tint(appearance: Appearance) -> Rgba {
    super::button::default_tint(appearance)
}

/// Opacity of a disabled switch, track and knob together **[fitted]**.
pub const DISABLED_OPACITY: f32 = 0.5;

/// The knob's fill, white and opaque in both appearances.
pub const KNOB_FILL: Rgba = [1.0; 4];

/// The toggle spring moving the knob: critically damped (`T_r = 0.299 s, ζ = 1.000`).
pub const TOGGLE_SPRING: Spring = Spring {
    omega0: 21.014,
    zeta: 1.0,
};

/// ω0 of the track fade turning on (`T_r = 0.561 s, ζ = 1.000`), rad/s.
pub const FADE_ON_OMEGA: f32 = 11.199_97;
/// ω0 of the track fade turning off (`T_r = 0.311 s, ζ = 1.000`), rad/s.
pub const FADE_OFF_OMEGA: f32 = 20.203_17;

fn build_lens_effect(w: f32, h: f32, recipe: &Recipe) -> MaterialEffect {
    assert!(
        recipe.ellipse_bend == 0.0,
        "the lens effect carries no position, so its recipe cannot bend toward the knob's ellipse"
    );
    let shape = Shape::rounded_rect(
        ElementRect::new(0.0, 0.0, w, h),
        h * 0.5,
        CornerCurve::Continuous,
    );
    materials::effect(&Element {
        shape,
        recipe: *recipe,
    })
}

/// The material the held knob is: the clear family's recipe at the lens's
/// size, `pressed` or not. The knob is opaque white while at rest, so the
/// lens only shows once a touch holds it.
///
/// # Panics
/// If the clear recipe bends its normal toward the knob's ellipse, which
/// needs the lens's position: the effect a look declares sees no toggle
/// value, so it cannot say where the knob is.
#[must_use]
pub fn lens_effect(
    appearance: Appearance,
    translucency: Translucency,
    pressed: bool,
) -> MaterialEffect {
    let (w, h) = lens_size(pressed);
    let recipe = Recipe::for_element(w, h, Family::Clear, appearance, translucency);
    build_lens_effect(w, h, &recipe)
}

/// Clear refractive lens tuned for the switch knob over its 28 pt track.
///
/// While pressed, the outer lip reflects the track beneath it and the bevel
/// refracts outward past the 28 pt track into dark exterior meniscus bands.
///
/// # Panics
/// If the clear recipe bends its normal toward the knob's ellipse.
#[must_use]
pub fn toggle_lens_effect(
    appearance: Appearance,
    translucency: Translucency,
    pressed: bool,
) -> MaterialEffect {
    let (w, h) = lens_size(pressed);
    let mut recipe = Recipe::for_element(w, h, Family::Clear, appearance, translucency);
    if pressed {
        recipe.primary_shift = 16.0;
        recipe.primary_depth = 9.0;
        recipe.rim_shift = -26.0;
        recipe.rim_depth = 3.0;
        recipe.rim_ramp = [-2.0, 0.0];
        recipe.rim_mix = 0.85;
    }
    build_lens_effect(w, h, &recipe)
}

/// The recipe's capture scale of the lens, which is the same held or not.
#[must_use]
pub fn lens_capture_scale(appearance: Appearance, translucency: Translucency) -> f32 {
    let (w, h) = lens_size(false);
    Recipe::for_element(w, h, Family::Clear, appearance, translucency).capture_scale
}

#[allow(
    clippy::cast_possible_truncation,
    reason = "the lens is tens of points wide, far inside f32"
)]
fn lens_size(pressed: bool) -> (f32, f32) {
    let growth = LENS_GROWTH.mul_add(f64::from(u8::from(pressed)), 1.0);
    (
        (f64::from(KNOB[0]) * growth) as f32,
        (f64::from(KNOB[1]) * growth) as f32,
    )
}

#[cfg(test)]
mod tests {
    use cherenkov_record::Animation as EngineAnimation;
    use waterui_graphics::animation::engine_animation;

    use super::*;

    fn near(a: f64, b: f64, tol: f64) {
        assert!((a - b).abs() <= tol, "{a} vs {b}");
    }

    const LTR: LayoutDirection = LayoutDirection::LeftToRight;

    /// The step response, at `t` seconds, of the critically damped spring
    /// the engine integrates for `spring`.
    fn step(spring: Spring, t: f64) -> f64 {
        let EngineAnimation::Spring(engine) = engine_animation(&spring.physical()) else {
            panic!("a physical spring is a spring in the engine");
        };
        assert!(
            (engine.damping - 1.0).abs() < 1e-6,
            "{engine:?} is not critical"
        );
        let s = core::f64::consts::TAU / engine.response * t;
        (1.0 + s).mul_add(-(-s).exp(), 1.0)
    }

    #[test]
    fn geometry_reproduces_the_observed_table() {
        let frame = Rect::new(10.0, 20.0, 10.0 + FRAME[0], 20.0 + FRAME[1]);
        let track = track_rect(frame, LTR);
        // 63 x 28 from the frame's leading edge, 2 pt past its trailing one.
        assert_eq!(
            (track.x0, track.y0, track.width(), track.height()),
            (10.0, 20.0, 63.0, 28.0)
        );
        assert_eq!(track.x1 - frame.x1, 2.0);
        let ShapeData::Continuous(capsule) = capsule(track) else {
            panic!("a glass capsule has continuous corners");
        };
        assert_eq!(capsule.radii, RoundedRectRadii::from_single_radius(14.0));
        let off = knob_rect(track, LTR, 0.0, 0.0);
        assert_eq!(
            (
                off.x0 - track.x0,
                off.y0 - track.y0,
                off.width(),
                off.height()
            ),
            (2.0, 2.0, 37.0, 24.0)
        );
        let on = knob_rect(track, LTR, 1.0, 0.0);
        // 2 pt from the trailing end, 22 pt of travel.
        near(track.x1 - on.x1, 2.0, 1e-9);
        near(on.x0 - off.x0, 22.0, 1e-9);
        near(on.height() / 2.0, 12.0, 1e-9);
    }

    #[test]
    fn a_right_to_left_switch_mirrors_the_track_and_the_knob() {
        let frame = Rect::new(10.0, 20.0, 10.0 + FRAME[0], 20.0 + FRAME[1]);
        let track = track_rect(frame, LayoutDirection::RightToLeft);
        // Begins at the frame's right edge, overhanging the left one.
        assert_eq!(track.x1, frame.x1);
        assert_eq!(frame.x0 - track.x0, 2.0);
        let off = knob_rect(track, LayoutDirection::RightToLeft, 0.0, 0.0);
        let on = knob_rect(track, LayoutDirection::RightToLeft, 1.0, 0.0);
        near(track.x1 - off.x1, 2.0, 1e-9);
        near(off.x0 - on.x0, 22.0, 1e-9);
    }

    #[test]
    fn a_held_knob_is_a_lens_59_by_38_overhanging_the_track() {
        let track = track_rect(Rect::new(0.0, 0.0, FRAME[0], FRAME[1]), LTR);
        let held = knob_rect(track, LTR, 0.0, 1.0);
        near(held.width(), 59.2, 0.01);
        near(held.height(), 38.4, 1e-3);
        near(track.y0 - held.y0, 5.2, 1e-3);
        near(
            held.center().x,
            knob_rect(track, LTR, 0.0, 0.0).center().x,
            1e-9,
        );
    }

    #[test]
    fn the_off_track_reproduces_the_rendered_fills() {
        let over = |c: Rgba, backdrop: f32| (c[3] * c[0]).mul_add(1.0, (1.0 - c[3]) * backdrop);
        let near = |a: f32, b: f32, tol: f32| assert!((a - b).abs() <= tol, "{a} vs {b}");
        let dark = off_fill(Appearance::Dark);
        near(over(dark, 0.0) * 255.0, 70.0, 1.0);
        near(over(dark, 0.5) * 255.0, 160.0, 1.0);
        near(over(dark, 1.0) * 255.0, 249.0, 1.0);
        let light = off_fill(Appearance::Light);
        near(over(light, 0.0) * 255.0, 18.0, 1.0);
        near(over(light, 0.5) * 255.0, 108.0, 1.0);
        near(over(light, 1.0) * 255.0, 197.0, 1.0);
    }

    #[test]
    fn the_toggle_spring_moves_the_knob_as_fitted() {
        // 28 % after 0.05 s, 62 % after 0.1 s, 99 % after 0.3 s, no overshoot.
        near(step(TOGGLE_SPRING, 0.05), 0.28, 0.01);
        near(step(TOGGLE_SPRING, 0.1), 0.62, 0.01);
        near(step(TOGGLE_SPRING, 0.3), 0.99, 0.01);
        for ms in (0..1000).step_by(10) {
            assert!(step(TOGGLE_SPRING, f64::from(ms) / 1000.0) <= 1.0 + 1e-6);
        }
    }

    #[test]
    fn the_knob_follows_the_observed_toggle_sequence() {
        let track = track_rect(Rect::new(0.0, 0.0, FRAME[0], FRAME[1]), LTR);
        // Leading edge from the track's leading edge: 8.2, 15.7, 20.1, 23.7 pt.
        for (ms, edge) in [(50, 8.2), (100, 15.7), (150, 20.1), (300, 23.7)] {
            let q = step(TOGGLE_SPRING, f64::from(ms) / 1000.0);
            near(knob_rect(track, LTR, q, 0.0).x0 - track.x0, edge, 0.15);
        }
    }

    #[test]
    fn the_track_fade_springs_reproduce_the_fitted_fractions() {
        let fade = |omega: f32, t: f64| {
            step(
                Spring {
                    omega0: omega,
                    zeta: 1.0,
                },
                t,
            )
        };
        // Turning on: 0.31 after 0.1 s, 0.85 after 0.3 s.
        near(fade(FADE_ON_OMEGA, 0.1), 0.31, 0.01);
        near(fade(FADE_ON_OMEGA, 0.3), 0.85, 0.01);
        // Turning off: 0.91 of the way after 0.2 s, so 0.09 left.
        near(1.0 - fade(FADE_OFF_OMEGA, 0.2), 0.09, 0.01);
    }

    #[test]
    fn a_held_lens_is_the_clear_family_at_the_grown_size() {
        let held = lens_effect(Appearance::Dark, Translucency::default(), true);
        let rest = lens_effect(Appearance::Dark, Translucency::default(), false);
        assert_ne!(held, rest);
    }
}
