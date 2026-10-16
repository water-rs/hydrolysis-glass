//! The glass switch (specification §8.2): geometry, colours, the toggle
//! motion and the glass knob a touch holds.
//!
//! Everything here is a pure function of the specification's tables except
//! [`record`], which draws a switch from them; the widget theme
//! ([`super::Glass`]) and the verification scenes build a switch from the
//! same decisions.

use cherenkov::{BlendSpace, Draw as _, Fixed, Group, Recorder};

use crate::material::geometry::{CornerCurve, Element, Rect, Shape};
use crate::material::recipe::{Appearance, Family, Recipe, Translucency};

use super::button::Spring;
use super::materials::{self, shape_data};
use super::palette::{Rgba, alpha, working};

/// The switch's layout frame, pt **[measured]**.
pub const FRAME: [f32; 2] = [61.0, 28.0];
/// The track, pt: 2 pt longer than the frame, overhanging its trailing
/// edge, and taking no layout space **[measured]**.
pub const TRACK: [f32; 2] = [63.0, 28.0];
/// The knob at rest, pt **[measured]**.
pub const KNOB: [f32; 2] = [37.0, 24.0];
/// Gap between the knob and the track's edges, pt **[measured]**.
pub const KNOB_INSET: f32 = 2.0;
/// How far the knob travels from off to on, pt **[measured]**.
pub const TRAVEL: f32 = 22.0;

/// The track's rectangle for a switch laid out at `frame`: its leading edge
/// on the frame's leading edge, centred vertically.
#[must_use]
pub fn track_rect(frame: Rect) -> Rect {
    Rect::new(
        frame.x,
        (frame.h - TRACK[1]).mul_add(0.5, frame.y),
        TRACK[0],
        TRACK[1],
    )
}

/// The knob's rectangle on `track` at toggle progress `q` (0 off, 1 on)
/// and press progress `p`: [`KNOB`] at `2 + 22·q` pt from the track's
/// leading end, scaled about its centre by `1 + 0.35·p` while held
/// **[chosen]**.
#[must_use]
pub fn knob_rect(track: Rect, q: f32, p: f32) -> Rect {
    Rect::new(
        TRAVEL.mul_add(q, track.x + KNOB_INSET),
        track.y + KNOB_INSET,
        KNOB[0],
        KNOB[1],
    )
    .scaled_about_center(LENS_GROWTH.mul_add(p, 1.0))
}

/// Growth of the held knob, `1 + 0.35·p` **[chosen]**.
pub const LENS_GROWTH: f32 = 0.35;

/// The capsule silhouette of a track or knob: radius half the height,
/// continuous corners.
#[must_use]
pub fn capsule(rect: Rect) -> Shape {
    Shape::rounded_rect(rect, rect.h * 0.5, CornerCurve::Continuous)
}

/// The off track's fill: (235, 235, 245) in dark appearance and
/// (60, 60, 67) in light, both at alpha 0.30, blended in encoded space
/// **[chosen]**.
#[must_use]
pub const fn off_fill(appearance: Appearance) -> Rgba {
    match appearance {
        Appearance::Dark => [235.0 / 255.0, 235.0 / 255.0, 245.0 / 255.0, 0.30],
        Appearance::Light => [60.0 / 255.0, 60.0 / 255.0, 67.0 / 255.0, 0.30],
    }
}

/// The default tint: green, (48, 209, 88) in dark appearance and
/// (52, 199, 89) in light **[chosen]**.
#[must_use]
pub const fn default_tint(appearance: Appearance) -> Rgba {
    match appearance {
        Appearance::Dark => [48.0 / 255.0, 209.0 / 255.0, 88.0 / 255.0, 1.0],
        Appearance::Light => [52.0 / 255.0, 199.0 / 255.0, 89.0 / 255.0, 1.0],
    }
}

/// Opacity of a disabled switch, track and knob together **[fitted]**.
pub const DISABLED_OPACITY: f32 = 0.5;

/// The knob's fill, white and opaque in both appearances.
pub const KNOB_FILL: Rgba = [1.0; 4];

/// The toggle spring moving the knob: critically damped, ω0 21 rad/s
/// **[fitted]**.
pub const TOGGLE_SPRING: Spring = Spring {
    omega0: 21.0,
    zeta: 1.0,
};

/// ω0 of the track fade turning on, rad/s **[fitted]**.
pub const FADE_ON_OMEGA: f32 = 11.2;
/// ω0 of the track fade turning off, rad/s **[fitted]**.
pub const FADE_OFF_OMEGA: f32 = 20.2;

/// What a critically damped spring from rest has left to travel after
/// `ω0·t = s`: `(1 + s)·e^{-s}`.
fn remaining(s: f32) -> f32 {
    (1.0 + s) * (-s).exp()
}

/// The `s` with `remaining(s) = left`, `0 < left ≤ 1`; `remaining` falls
/// monotonically, so bisection finds it.
fn elapsed(left: f32) -> f32 {
    let (mut lo, mut hi) = (0.0_f32, 40.0_f32);
    for _ in 0..40 {
        let mid = f32::midpoint(lo, hi);
        if remaining(mid) > left {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    f32::midpoint(lo, hi)
}

/// The opacity `c` of the on fill over the off fill, from the knob's
/// toggle progress `q` and the value `selected` it is heading to.
///
/// The fade is its own critically damped spring (ω0 [`FADE_ON_OMEGA`]
/// turning on, [`FADE_OFF_OMEGA`] turning off) started with the knob. The
/// widget contract hands the theme `q` and the target, not the fade's
/// state, so the time since the change is read back from `q` — the toggle
/// spring's remaining travel — and the fade evaluated at that time. That
/// is exact for a change from rest; a change made during a motion
/// continues from the current `q` as if it had started at rest from there.
#[must_use]
pub fn fade(q: f32, selected: bool) -> f32 {
    let q = q.clamp(0.0, 1.0);
    let left = if selected { 1.0 - q } else { q };
    if left <= 0.0 {
        return f32::from(u8::from(selected));
    }
    let omega = if selected {
        FADE_ON_OMEGA
    } else {
        FADE_OFF_OMEGA
    };
    let left_fade = remaining(elapsed(left) * omega / TOGGLE_SPRING.omega0);
    if selected { 1.0 - left_fade } else { left_fade }
}

/// What a switch is drawn from.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Switch {
    /// Appearance of the colours.
    pub appearance: Appearance,
    /// The on fill; `None` takes [`default_tint`].
    pub tint: Option<Rgba>,
    /// Knob position: 0 off, 1 on.
    pub progress: f32,
    /// Whether the value the knob is heading to is on.
    pub selected: bool,
    /// Press progress `p`: 0 at rest, 1 held; the springs overshoot it.
    pub press: f32,
    /// Whether the switch responds to touches; a disabled one is drawn at
    /// half opacity.
    pub enabled: bool,
}

impl Switch {
    /// The glass knob lens of a held switch, or `None` at rest and when
    /// disabled **[chosen]**: a clear-family element (§5.2) at the grown
    /// knob's size.
    #[must_use]
    pub fn lens(&self, frame: Rect, translucency: Translucency) -> Option<Element> {
        if !self.enabled || self.press <= 0.0 {
            return None;
        }
        let shape = capsule(knob_rect(track_rect(frame), self.progress, self.press));
        let recipe = Recipe::for_element(
            shape.rect.w,
            shape.rect.h,
            Family::Clear,
            self.appearance,
            translucency,
        );
        Some(Element { shape, recipe })
    }
}

/// Records the switch laid out at `frame`.
///
/// The off fill, the on fill at [`fade`] over it, and the knob, composited
/// in encoded space; a disabled switch as one group at
/// [`DISABLED_OPACITY`]. While held, the knob is the glass lens of
/// [`Switch::lens`] over the track with its white fill at `1 − p`.
///
/// # Panics
/// While held: as [`materials::record`] does, outside a layered recording
/// or inside a clip, transform or group scope.
pub fn record(recorder: &mut Recorder, frame: Rect, switch: &Switch, translucency: Translucency) {
    let track = capsule(track_rect(frame));
    let on = switch
        .tint
        .unwrap_or_else(|| default_tint(switch.appearance));
    let opacity = if switch.enabled {
        1.0
    } else {
        DISABLED_OPACITY
    };
    let lens = switch.lens(frame, translucency);
    let encoded = Group::new().blend_space(BlendSpace::SrgbEncoded);
    recorder.group(encoded.opacity(opacity), |recorder| {
        recorder.fill(
            Fixed(shape_data(&track)),
            working(off_fill(switch.appearance)),
        );
        recorder.fill(
            Fixed(shape_data(&track)),
            working(alpha(on, fade(switch.progress, switch.selected))),
        );
        if lens.is_none() {
            let knob = capsule(knob_rect(track.rect, switch.progress, 0.0));
            recorder.fill(Fixed(shape_data(&knob)), working(KNOB_FILL));
        }
    });
    if let Some(lens) = lens {
        materials::record(recorder, &lens, false);
        recorder.group(encoded, |recorder| {
            recorder.fill(
                Fixed(shape_data(&lens.shape)),
                working(alpha(KNOB_FILL, 1.0 - switch.press)),
            );
        });
    }
}

#[cfg(test)]
mod tests {
    use core::time::Duration;

    use super::*;

    fn near(a: f32, b: f32, tol: f32) {
        assert!((a - b).abs() <= tol, "{a} vs {b}");
    }

    #[test]
    fn geometry_reproduces_the_observed_table() {
        let frame = Rect::new(10.0, 20.0, FRAME[0], FRAME[1]);
        let track = track_rect(frame);
        // 63 x 28 from the frame's leading edge, 2 pt past its trailing one.
        assert_eq!(
            (track.x, track.y, track.w, track.h),
            (10.0, 20.0, 63.0, 28.0)
        );
        assert_eq!(track.x + track.w - (frame.x + frame.w), 2.0);
        let capsule = capsule(track);
        assert_eq!(capsule.radii, [14.0; 4]);
        assert_eq!(capsule.curve, CornerCurve::Continuous);
        let off = knob_rect(track, 0.0, 0.0);
        assert_eq!(
            (off.x - track.x, off.y - track.y, off.w, off.h),
            (2.0, 2.0, 37.0, 24.0)
        );
        let on = knob_rect(track, 1.0, 0.0);
        // 2 pt from the trailing end, 22 pt of travel.
        near(track.x + track.w - (on.x + on.w), 2.0, 1e-4);
        near(on.x - off.x, 22.0, 1e-4);
        assert_eq!(capsule_radius(&on), 12.0);
    }

    fn capsule_radius(rect: &Rect) -> f32 {
        capsule(*rect).radii[0]
    }

    #[test]
    fn a_held_knob_is_a_lens_50_by_32_overhanging_the_track() {
        let track = track_rect(Rect::new(0.0, 0.0, FRAME[0], FRAME[1]));
        let held = knob_rect(track, 0.0, 1.0);
        near(held.w, 49.95, 0.01);
        near(held.h, 32.4, 1e-3);
        near(track.y - held.y, 2.2, 1e-3);
        near(
            held.center()[0],
            knob_rect(track, 0.0, 0.0).center()[0],
            1e-4,
        );
    }

    #[test]
    fn the_off_track_reproduces_the_rendered_fills() {
        let over = |c: Rgba, backdrop: f32| (c[3] * c[0]).mul_add(1.0, (1.0 - c[3]) * backdrop);
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
        let a = TOGGLE_SPRING.animation();
        // 28 % after 0.05 s, 62 % after 0.1 s, 99 % after 0.3 s, no overshoot.
        near(a.progress(Duration::from_millis(50)), 0.28, 0.01);
        near(a.progress(Duration::from_millis(100)), 0.62, 0.01);
        near(a.progress(Duration::from_millis(300)), 0.99, 0.01);
        for ms in (0..1000).step_by(10) {
            assert!(a.progress(Duration::from_millis(ms)) <= 1.0 + 1e-4);
        }
    }

    #[test]
    fn the_knob_follows_the_observed_toggle_sequence() {
        let track = track_rect(Rect::new(0.0, 0.0, FRAME[0], FRAME[1]));
        let a = TOGGLE_SPRING.animation();
        // Leading edge from the track's leading edge: 8.2, 15.7, 20.1, 23.7 pt.
        for (ms, edge) in [(50, 8.2), (100, 15.7), (150, 20.1), (300, 23.7)] {
            let q = a.progress(Duration::from_millis(ms));
            near(knob_rect(track, q, 0.0).x - track.x, edge, 0.15);
        }
    }

    #[test]
    fn the_track_fade_reproduces_the_fitted_fractions() {
        let a = TOGGLE_SPRING.animation();
        let q = |ms| a.progress(Duration::from_millis(ms));
        // Turning on: 0.31 after 0.1 s, 0.85 after 0.3 s.
        near(fade(q(100), true), 0.31, 0.01);
        near(fade(q(300), true), 0.85, 0.01);
        // Turning off: 0.91 of the way after 0.2 s, so 0.09 left.
        near(fade(1.0 - q(200), false), 0.09, 0.01);
        // At rest.
        assert_eq!(fade(0.0, false), 0.0);
        assert_eq!(fade(1.0, true), 1.0);
        assert_eq!(fade(0.0, true), 0.0);
        assert_eq!(fade(1.0, false), 1.0);
    }

    #[test]
    fn a_lens_exists_only_while_an_enabled_switch_is_held() {
        let frame = Rect::new(0.0, 0.0, FRAME[0], FRAME[1]);
        let mut s = Switch {
            appearance: Appearance::Dark,
            tint: None,
            progress: 0.0,
            selected: false,
            press: 0.0,
            enabled: true,
        };
        assert!(s.lens(frame, Translucency::default()).is_none());
        s.press = 1.0;
        let lens = s.lens(frame, Translucency::default()).expect("held");
        near(lens.shape.rect.h, 32.4, 1e-3);
        s.enabled = false;
        assert!(s.lens(frame, Translucency::default()).is_none());
    }
}
