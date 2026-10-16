//! The glass slider (specification §8.3): geometry, colours, the knob's
//! shadow, the value spring and the drag laws.
//!
//! Everything here is a pure function of the specification's tables, so the
//! widget theme ([`super::Glass`]) and the verification scenes build a
//! slider from the same decisions.

use cherenkov::kurbo::{Point, Rect};
use waterui_backend_core::widget::SliderMetrics;

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
/// Smallest track width the layout asks for, pt: the knob's travel.
pub const MIN_TRAVEL: f64 = 120.0;
/// Space between the slider's own label and its row, pt.
pub const LABEL_SPACING: f64 = 8.0;

/// The layout metrics the slider asks the renderer for.
///
/// The renderer lays the knob's *centre* along the rectangle it hands the
/// theme as the track, so that rectangle is the knob's travel: the full
/// track inset by half the knob at each end. The horizontal inset is that
/// half knob, and a symbol's gap to the full track is the spec's
/// [`SYMBOL_GAP`] beyond it.
#[must_use]
pub const fn metrics() -> SliderMetrics {
    SliderMetrics::new(
        KNOB_WIDTH * 0.5,
        SYMBOL_GAP + KNOB_WIDTH * 0.5,
        LABEL_SPACING,
        MIN_TRAVEL,
        TRACK_HEIGHT,
        KNOB_WIDTH,
        HEIGHT,
    )
}

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

/// A slider as the renderer hands it to the theme, with what the theme
/// reads from the interaction state.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Slider {
    /// The full track: [`TRACK_HEIGHT`] tall across the slider's width.
    pub track: Rect,
    /// The knob's centre, which is also where the fill ends.
    pub knob_center: Point,
    /// The appearance.
    pub appearance: Appearance,
    /// The fill colour.
    pub tint: Rgba,
    /// Whether the slider responds to touches.
    pub enabled: bool,
    /// The press progress `p`, 0 at rest and 1 pressed; the springs
    /// overshoot it. Zero while disabled.
    pub press: f32,
}

impl Slider {
    /// The slider whose knob travels `travel` (the rectangle the renderer
    /// calls the track) and whose fill ends at `fill`'s trailing edge.
    #[must_use]
    pub const fn new(
        travel: Rect,
        fill: Rect,
        appearance: Appearance,
        tint: Rgba,
        enabled: bool,
        press: f32,
    ) -> Self {
        let y = travel.center().y;
        Self {
            track: Rect::new(
                KNOB_WIDTH.mul_add(-0.5, travel.x0),
                TRACK_HEIGHT.mul_add(-0.5, y),
                KNOB_WIDTH.mul_add(0.5, travel.x1),
                TRACK_HEIGHT.mul_add(0.5, y),
            ),
            knob_center: Point::new(fill.x1, y),
            appearance,
            tint,
            enabled,
            press: if enabled { press } else { 0.0 },
        }
    }

    /// The part of the track before the knob's centre, under the knob.
    #[must_use]
    pub const fn fill(&self) -> Rect {
        Rect::new(
            self.track.x0,
            self.track.y0,
            self.knob_center.x,
            self.track.y1,
        )
    }

    /// The knob: 37 × 24 pt, scaled about its centre by `1 + 0.35·p`.
    #[must_use]
    pub fn knob(&self) -> Rect {
        let s = LENS_GROWTH.mul_add(f64::from(self.press), 1.0);
        Rect::from_center_size(self.knob_center, (KNOB_WIDTH * s, KNOB_HEIGHT * s))
    }

    /// The knob at rest: the size the lens's recipe keeps.
    #[must_use]
    pub fn idle_knob(&self) -> Rect {
        Rect::from_center_size(self.knob_center, (KNOB_WIDTH, KNOB_HEIGHT))
    }

    /// The touch target: the knob, extended to at least 44 × 44 pt about
    /// its centre **[chosen]**.
    #[must_use]
    pub fn hit_region(&self) -> Rect {
        let side = f64::from(button::MIN_HIT_SIDE);
        Rect::from_center_size(
            self.knob_center,
            (KNOB_WIDTH.max(side), KNOB_HEIGHT.max(side)),
        )
    }

    /// Whether the knob is a glass lens: pressed and enabled.
    #[must_use]
    pub fn is_lens(&self) -> bool {
        self.enabled && self.press > 0.0
    }

    /// The opacity of the knob's white fill and its shadow over the lens:
    /// `1 − max(p, 0)`, clamped to [0, 1] **[chosen]**.
    #[must_use]
    pub fn flat_opacity(&self) -> f32 {
        (1.0 - self.press.max(0.0)).clamp(0.0, 1.0)
    }

    /// The opacity of the whole slider: half while disabled.
    #[must_use]
    pub const fn opacity(&self) -> f32 {
        if self.enabled { 1.0 } else { DISABLED_OPACITY }
    }
}

/// A value rounded to the nearest multiple of `step` from the range's
/// start, kept inside the range **[chosen]**.
#[must_use]
pub fn round_to_step(value: f64, step: f64, range: &core::ops::RangeInclusive<f64>) -> f64 {
    let start = *range.start();
    let steps = ((value - start) / step).round();
    steps.mul_add(step, start).clamp(start, *range.end())
}

/// The value after a drag of `offset` pt from `start` on a slider `width`
/// pt wide.
///
/// It moves by `offset / (W − 37)` of the range, clamped to the range, then
/// rounded to the step if there is one. `mirrored` flips the direction in a
/// right-to-left layout **[chosen]**.
#[must_use]
pub fn dragged_value(
    start: f64,
    offset: f64,
    width: f64,
    range: &core::ops::RangeInclusive<f64>,
    step: Option<f64>,
    mirrored: bool,
) -> f64 {
    let span = *range.end() - *range.start();
    let offset = if mirrored { -offset } else { offset };
    let moved = (offset / (width - KNOB_WIDTH)).mul_add(span, start);
    let clamped = moved.clamp(*range.start(), *range.end());
    step.map_or(clamped, |step| round_to_step(clamped, step, range))
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

    fn near(a: f64, b: f64, tol: f64) {
        assert!((a - b).abs() <= tol, "{a} vs {b}");
    }

    /// A slider 300 pt wide at value `v`, whose frame starts at x = 0.
    fn slider_at(v: f64, press: f32) -> Slider {
        let travel = Rect::new(
            KNOB_WIDTH * 0.5,
            12.5,
            300.0 - KNOB_WIDTH * 0.5,
            12.5 + TRACK_HEIGHT,
        );
        let end = travel.x0 + v * travel.width();
        let fill = Rect::new(travel.x0, travel.y0, end, travel.y1);
        Slider::new(
            travel,
            fill,
            Appearance::Dark,
            default_tint(Appearance::Dark),
            true,
            press,
        )
    }

    #[test]
    fn the_metrics_make_the_knobs_centre_travel_the_inset_track() {
        let m = metrics();
        assert_eq!(m.horizontal_inset, 18.5);
        assert_eq!(m.handle_overhang(), 18.5);
        assert_eq!(m.horizontal_spacing - m.handle_overhang(), SYMBOL_GAP);
        assert_eq!(m.track_height, TRACK_HEIGHT);
        // The row's height is the frame's.
        assert_eq!(m.handle_height, HEIGHT);
    }

    #[test]
    fn the_track_spans_the_width_and_the_knob_sits_on_it() {
        for v in [0.0, 0.3, 0.5, 1.0] {
            let s = slider_at(v, 0.0);
            assert_eq!((s.track.x0, s.track.x1), (0.0, 300.0));
            assert_eq!(s.track.height(), 6.0);
            assert_eq!(s.track.center().y, s.knob().center().y);
            near(s.knob().x0, knob_leading(v, 300.0), 1e-9);
            assert_eq!((s.knob().width(), s.knob().height()), (37.0, 24.0));
            assert_eq!(s.fill().x1, s.knob_center.x);
        }
        near(knob_leading(0.3, 300.0), 78.9, 1e-9);
        near(slider_at(0.5, 0.0).knob().center().x, 150.0, 1e-9);
        near(slider_at(1.0, 0.0).knob().x1, 300.0, 1e-9);
    }

    #[test]
    fn a_press_grows_the_knob_about_its_centre_to_the_lens() {
        let rest = slider_at(0.5, 0.0);
        let held = slider_at(0.5, 1.0);
        near(held.knob().width(), 49.95, 1e-9);
        near(held.knob().height(), 32.4, 1e-9);
        assert_eq!(held.knob().center(), rest.knob().center());
        assert!(held.is_lens() && !rest.is_lens());
        assert_eq!(held.flat_opacity(), 0.0);
        // The springs overshoot p past 1; the opacity stays a coverage.
        assert_eq!(slider_at(0.5, 1.07).flat_opacity(), 0.0);
        near(f64::from(slider_at(0.5, 0.25).flat_opacity()), 0.75, 1e-6);
    }

    #[test]
    fn a_disabled_slider_is_at_half_opacity_and_ignores_a_press() {
        let travel = Rect::new(18.5, 0.0, 281.5, 6.0);
        let off = Slider::new(
            travel,
            travel,
            Appearance::Light,
            default_tint(Appearance::Light),
            false,
            1.0,
        );
        assert_eq!(off.opacity(), 0.5);
        assert_eq!(off.press, 0.0);
        assert!(!off.is_lens());
        assert_eq!(slider_at(0.5, 0.0).opacity(), 1.0);
    }

    #[test]
    fn the_hit_region_is_at_least_44_by_44_about_the_knob() {
        let hit = slider_at(0.5, 0.0).hit_region();
        assert_eq!((hit.width(), hit.height()), (44.0, 44.0));
        assert_eq!(hit.center().x, 150.0);
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

    #[test]
    fn values_round_to_the_nearest_step() {
        let range = 0.0..=1.0;
        near(round_to_step(0.37, 0.25, &range), 0.25, 1e-12);
        near(round_to_step(0.38, 0.25, &range), 0.5, 1e-12);
        near(round_to_step(2.0, 0.25, &range), 1.0, 1e-12);
        let range = 10.0..=20.0;
        near(round_to_step(13.0, 5.0, &range), 15.0, 1e-12);
    }

    #[test]
    fn a_drag_moves_the_value_by_the_offset_over_the_travel() {
        let range = 0.0..=1.0;
        // The travel of a 300 pt slider is 263 pt.
        near(
            dragged_value(0.2, 52.6, 300.0, &range, None, false),
            0.4,
            1e-9,
        );
        // Clamped to the range wherever the touch goes.
        assert_eq!(dragged_value(0.9, 500.0, 300.0, &range, None, false), 1.0);
        assert_eq!(dragged_value(0.1, -500.0, 300.0, &range, None, false), 0.0);
        // Rounded to the step.
        near(
            dragged_value(0.2, 52.6, 300.0, &range, Some(0.25), false),
            0.5,
            1e-12,
        );
        // Mirrored in a right-to-left layout.
        near(
            dragged_value(0.5, 26.3, 300.0, &range, None, true),
            0.4,
            1e-9,
        );
    }
}
