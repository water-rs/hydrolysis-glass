//! The glass slider (specification §8.3): geometry, colours, the knob's
//! shadow and the value spring.
//!
//! Everything here is a pure function of the specification's tables; the
//! slider look ([`super::looks`]) builds a slider from the same decisions.
//! The drag, its step rounding and the value's range are Hydrolysis's: the
//! look names the drag mapping (§8.3, Touch) and the renderer executes it.

use cherenkov_record::kurbo::{Point, Rect};

use crate::material::recipe::Appearance;

use super::button::{self, Spring};
use super::palette::Rgba;

/// Height of the slider's layout frame, pt **[measured: simulator]**.
pub const HEIGHT: f64 = 31.0;
/// Height of the track, pt **[measured: simulator]**.
pub const TRACK_HEIGHT: f64 = 6.0;
/// Width of the knob at rest, pt **[measured: simulator]**.
pub const KNOB_WIDTH: f64 = 37.0;
/// Height of the knob at rest, pt **[measured: simulator]**.
pub const KNOB_HEIGHT: f64 = 24.0;
/// Space between an end symbol's layout box and the track, pt
/// **[chosen]**.
pub const SYMBOL_GAP: f64 = 12.0;
/// Smallest width of the knob's travel the layout asks for, pt.
pub const MIN_TRAVEL: f64 = 120.0;
/// Space between the slider's own label and its row, pt.
pub const LABEL_SPACING: f64 = 8.0;

/// How far the track runs past each end of the knob's travel, pt: half the
/// knob, whose centre travels the track inset by it.
///
/// The renderer lays the knob's *centre* along the row it hands the look as
/// the frame, so that row is the knob's travel and the full track is the
/// travel grown by this at each end.
pub const TRAVEL_INSET: f64 = KNOB_WIDTH * 0.5;
/// Space between an end symbol's layout box and the knob's travel, pt: the
/// spec's [`SYMBOL_GAP`] to the full track, beyond [`TRAVEL_INSET`].
pub const LABEL_GAP: f64 = SYMBOL_GAP + TRAVEL_INSET;

/// Alpha of the unfilled track's white (dark) or black (light) fill
/// **[chosen]**.
pub const UNFILLED_ALPHA: f32 = 0.098;
/// Alpha of the knob's shadow **[chosen]**.
pub const SHADOW_ALPHA: f32 = 0.137;
/// Standard deviation of the knob's shadow, pt **[fitted: simulator]**.
pub const SHADOW_SIGMA: f64 = 5.2;
/// Offset of the knob's shadow below the knob, pt **[fitted: simulator]**.
pub const SHADOW_OFFSET: f64 = 1.2;
/// Opacity of a disabled slider **[fitted: simulator]**.
pub const DISABLED_OPACITY: f32 = 0.5;
/// Growth of the knob's lens at full press: `1 + 0.35·p` **[chosen]**.
pub const LENS_GROWTH: f64 = 0.35;

/// The default tint (the accent colour) **[measured: simulator]**.
#[must_use]
pub const fn default_tint(appearance: Appearance) -> Rgba {
    button::default_tint(appearance)
}

/// The rendered red tint **[chosen: simulator rendering]**.
#[must_use]
pub const fn red_tint(appearance: Appearance) -> Rgba {
    match appearance {
        Appearance::Dark => [1.0, 66.0 / 255.0, 69.0 / 255.0, 1.0],
        Appearance::Light => [1.0, 56.0 / 255.0, 60.0 / 255.0, 1.0],
    }
}

/// The unfilled track: white in dark appearance and black in light, at
/// alpha 0.098 over the backdrop, blended in encoded space **[chosen]**.
#[must_use]
pub const fn unfilled_track(appearance: Appearance) -> Rgba {
    let c = match appearance {
        Appearance::Dark => 1.0,
        Appearance::Light => 0.0,
    };
    [c, c, c, UNFILLED_ALPHA]
}

/// The press progress `p` that drives the knob's lens uses the button's
/// press and release springs **[chosen]**, so the controls respond alike.
pub const PRESS_SPRING: Spring = Spring::PRESS;
/// See [`PRESS_SPRING`].
pub const RELEASE_SPRING: Spring = Spring::RELEASE;

/// The value spring that moves the display value `u` to a value changed
/// from outside the slider: ω0 15.6 rad/s, ζ 0.79 on a unit mass
/// **[fitted: simulator]**.
pub const VALUE_SPRING: Spring = Spring {
    omega0: 15.6,
    zeta: 0.79,
};

/// Leading edge of the knob from the track's leading end, for the value
/// `v` mapped to [0, 1] on a slider `width` pt wide: `v · (W − 37)`.
#[must_use]
pub fn knob_leading(v: f64, width: f64) -> f64 {
    v * (width - KNOB_WIDTH)
}

/// The full track of a slider whose knob's centre travels `travel`: the
/// travel grown by [`TRAVEL_INSET`] at each end, [`TRACK_HEIGHT`] tall and
/// centred on it.
#[must_use]
pub fn track_rect(travel: Rect) -> Rect {
    let y = travel.center().y;
    Rect::new(
        travel.x0 - TRAVEL_INSET,
        TRACK_HEIGHT.mul_add(-0.5, y),
        travel.x1 + TRAVEL_INSET,
        TRACK_HEIGHT.mul_add(0.5, y),
    )
}

/// The knob's centre for the value `v` mapped to [0, 1], on `travel`.
#[must_use]
pub const fn knob_center(travel: Rect, v: f64) -> Point {
    Point::new(v.mul_add(travel.width(), travel.x0), travel.center().y)
}

/// The knob for the value `v` on `travel`: [`KNOB_WIDTH`] by
/// [`KNOB_HEIGHT`], scaled about its centre by `1 + 0.35·p` for the press
/// progress `p`.
#[must_use]
pub fn knob_rect(travel: Rect, v: f64, p: f64) -> Rect {
    let growth = LENS_GROWTH.mul_add(p, 1.0);
    Rect::from_center_size(
        knob_center(travel, v),
        (KNOB_WIDTH * growth, KNOB_HEIGHT * growth),
    )
}

/// The part of the track before the knob's centre, under the knob.
#[must_use]
pub fn fill_rect(travel: Rect, v: f64) -> Rect {
    let track = track_rect(travel);
    Rect::new(track.x0, track.y0, knob_center(travel, v).x, track.y1)
}

#[cfg(test)]
#[allow(
    clippy::suboptimal_flops,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "tests spell out the specification's arithmetic and 8-bit levels"
)]
mod tests {
    use super::*;
    use crate::theme::switch;

    fn near(a: f64, b: f64, tol: f64) {
        assert!((a - b).abs() <= tol, "{a} vs {b}");
    }

    /// The knob's travel on a 300 pt slider whose frame starts at x = 0:
    /// the track inset by half the knob at each end.
    fn travel() -> Rect {
        Rect::new(TRAVEL_INSET, 12.5, 300.0 - TRAVEL_INSET, 18.5)
    }

    #[test]
    fn the_layout_makes_the_knobs_centre_travel_the_inset_track() {
        assert_eq!(TRAVEL_INSET, 18.5);
        // A symbol's gap to the full track is the spec's.
        assert_eq!(LABEL_GAP - TRAVEL_INSET, SYMBOL_GAP);
        assert_eq!(MIN_TRAVEL, 120.0);
        assert_eq!(HEIGHT, 31.0);
    }

    #[test]
    fn the_track_spans_the_width_and_the_knob_sits_on_it() {
        for v in [0.0, 0.3, 0.5, 1.0] {
            let track = track_rect(travel());
            assert_eq!((track.x0, track.x1), (0.0, 300.0));
            assert_eq!(track.height(), 6.0);
            let knob = knob_rect(travel(), v, 0.0);
            assert_eq!(track.center().y, knob.center().y);
            near(knob.x0, knob_leading(v, 300.0), 1e-9);
            assert_eq!((knob.width(), knob.height()), (37.0, 24.0));
            assert_eq!(fill_rect(travel(), v).x1, knob.center().x);
            assert_eq!(fill_rect(travel(), v).x0, track.x0);
        }
        near(knob_leading(0.3, 300.0), 78.9, 1e-9);
        near(knob_rect(travel(), 0.5, 0.0).center().x, 150.0, 1e-9);
        near(knob_rect(travel(), 1.0, 0.0).x1, 300.0, 1e-9);
    }

    #[test]
    fn a_press_grows_the_knob_about_its_centre_to_the_lens() {
        let rest = knob_rect(travel(), 0.5, 0.0);
        let held = knob_rect(travel(), 0.5, 1.0);
        near(held.width(), 49.95, 1e-9);
        near(held.height(), 32.4, 1e-9);
        assert_eq!(held.center(), rest.center());
    }

    #[test]
    fn the_lens_is_the_switchs_knob_at_the_same_growth() {
        // The slider's lens takes the switch's recipe (`switch::lens_effect`)
        // because the two controls' knobs and growth are the same numbers.
        assert_eq!([KNOB_WIDTH, KNOB_HEIGHT], switch::KNOB);
        assert_eq!(LENS_GROWTH, switch::LENS_GROWTH);
    }

    /// Encoded 8-bit result of `fg` over a grey backdrop in encoded space.
    fn over_grey(fg: Rgba, backdrop: f32) -> u8 {
        let v = fg[0].mul_add(fg[3], backdrop * (1.0 - fg[3]));
        (v * 255.0).round() as u8
    }

    #[test]
    fn the_unfilled_track_reproduces_the_rendered_levels() {
        let dark = unfilled_track(Appearance::Dark);
        assert_eq!([0.0, 0.5, 1.0].map(|b| over_grey(dark, b)), [25, 140, 255]);
        let light = unfilled_track(Appearance::Light);
        assert_eq!([0.0, 0.5, 1.0].map(|b| over_grey(light, b)), [0, 115, 230]);
    }

    #[test]
    fn the_tints_are_the_rendered_colours() {
        let c = |t: Rgba| t.map(|v| (v * 255.0).round() as u8);
        assert_eq!(c(default_tint(Appearance::Dark)), [0, 145, 255, 255]);
        assert_eq!(c(default_tint(Appearance::Light)), [0, 136, 255, 255]);
        assert_eq!(c(red_tint(Appearance::Dark)), [255, 66, 69, 255]);
        assert_eq!(c(red_tint(Appearance::Light)), [255, 56, 60, 255]);
    }

    /// The unit-step response of the value spring from rest, the
    /// closed-form solution the spec's table is read from.
    fn step_response(t: f64) -> f64 {
        let w0 = f64::from(VALUE_SPRING.omega0);
        let z = f64::from(VALUE_SPRING.zeta);
        let wd = w0 * z.mul_add(-z, 1.0).sqrt();
        let decay = (-z * w0 * t).exp();
        1.0 - decay * ((wd * t).cos() + z * w0 / wd * (wd * t).sin())
    }

    #[test]
    fn the_value_spring_covers_the_observed_fractions() {
        near(step_response(0.05), 0.20, 0.01);
        near(step_response(0.1), 0.53, 0.01);
        near(step_response(0.2), 0.93, 0.01);
        near(step_response(0.33), 1.018, 0.002);
        near(step_response(0.5), 1.0, 0.005);
        // Stiffness 243 and damping 24.6 on a unit mass.
        near(f64::from(VALUE_SPRING.omega0).powi(2), 243.0, 0.5);
        near(
            2.0 * f64::from(VALUE_SPRING.zeta * VALUE_SPRING.omega0),
            24.6,
            0.05,
        );
    }
}
