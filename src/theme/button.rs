//! The glass button (specification §8.1): sizes, frames, shapes, colours
//! and the press motion.
//!
//! Everything here is a pure function of the specification's tables, so
//! the widget theme ([`super::Glass`]) and the standalone verification
//! scenes build a button from the same decisions.

use waterui::animation::Animation;
use waterui_controls::ControlSize;

use crate::material::geometry::{CornerCurve, Rect, Shape};
use crate::material::recipe::Appearance;

use super::palette::Rgba;

/// Which of the two button kinds is drawn (§8.1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// The glass body with the label drawn on it.
    Glass,
    /// The glass body filled with the tint, with a white label.
    Prominent,
}

/// The button's border shape (§8.1, Geometry).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BorderShape {
    /// A capsule, the default.
    Capsule,
    /// A rounded rectangle; `None` takes the size group's default radius.
    RoundedRect(Option<f32>),
    /// A circle of diameter `min(width, height)`.
    Circle,
}

/// The size groups of §8.1: mini and small are identical, and so are large
/// and extra large.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SizeGroup {
    /// Mini and small.
    Compact,
    /// Regular.
    Regular,
    /// Large and extra large.
    Large,
}

impl SizeGroup {
    /// The group a control size belongs to.
    ///
    /// # Panics
    /// On a control size the specification gives no button metrics for.
    #[must_use]
    pub fn of(size: ControlSize) -> Self {
        match size {
            ControlSize::ExtraSmall | ControlSize::Small => Self::Compact,
            ControlSize::Medium => Self::Regular,
            ControlSize::Large | ControlSize::ExtraLarge => Self::Large,
            other => panic!("the glass button has no metrics for control size {other:?}"),
        }
    }

    /// The group's metrics.
    #[must_use]
    pub const fn metrics(self) -> SizeMetrics {
        match self {
            Self::Compact => SizeMetrics {
                text_size: 15.0,
                line_height: 18.0,
                inset_x: 10.0,
                inset_y: 5.0,
                symbol_width: 19.333_334,
                rounded_radius: 4.8,
            },
            Self::Regular => SizeMetrics {
                text_size: 17.0,
                line_height: 20.33,
                inset_x: 12.0,
                inset_y: 7.0,
                symbol_width: 22.0,
                rounded_radius: 6.9,
            },
            Self::Large => SizeMetrics {
                text_size: 17.0,
                line_height: 20.33,
                inset_x: 20.0,
                inset_y: 15.0,
                symbol_width: 22.0,
                rounded_radius: 8.9,
            },
        }
    }
}

/// One size group's measured values (§8.1, Geometry). **[measured]**
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SizeMetrics {
    /// Label text size, pt; the weight is regular in every group.
    pub text_size: f32,
    /// Label line height, pt.
    pub line_height: f32,
    /// Horizontal inset, pt.
    pub inset_x: f32,
    /// Vertical inset, pt.
    pub inset_y: f32,
    /// Layout width of a symbol drawn at the label's text size, pt
    /// **[fitted]**.
    pub symbol_width: f32,
    /// Corner radius of a rounded-rectangle shape given no radius, pt.
    pub rounded_radius: f32,
}

/// Gap between a symbol and the title of an icon-and-title label, pt
/// **[fitted]**.
pub const ICON_TITLE_GAP: f32 = 8.0;

/// Layout width of an icon-and-title label: the symbol, the gap, the
/// title.
#[must_use]
pub const fn icon_title_label_width(group: SizeGroup, title_width: f32) -> f32 {
    group.metrics().symbol_width + ICON_TITLE_GAP + title_width
}

/// The button's frame size: the label's layout size plus the insets of its
/// size group. A circle shape replaces the horizontal inset by the vertical
/// one.
#[must_use]
pub fn frame_size(group: SizeGroup, label: [f32; 2], shape: BorderShape) -> [f32; 2] {
    let m = group.metrics();
    let inset_x = if shape == BorderShape::Circle {
        m.inset_y
    } else {
        m.inset_x
    };
    [
        inset_x.mul_add(2.0, label[0]),
        m.inset_y.mul_add(2.0, label[1]),
    ]
}

/// The silhouette of a button laid out at `frame`: continuous capsule and
/// rounded-rectangle corners, a circle for [`BorderShape::Circle`].
#[must_use]
pub fn shape(frame: Rect, border: BorderShape, group: SizeGroup) -> Shape {
    match border {
        BorderShape::Capsule => Shape::rounded_rect(frame, frame.h * 0.5, CornerCurve::Continuous),
        BorderShape::RoundedRect(radius) => Shape::rounded_rect(
            frame,
            radius.unwrap_or_else(|| group.metrics().rounded_radius),
            CornerCurve::Continuous,
        ),
        BorderShape::Circle => {
            let [cx, cy] = frame.center();
            Shape::circle(cx, cy, frame.m() * 0.5)
        }
    }
}

/// Smallest side of the touch target, pt **[chosen]**.
pub const MIN_HIT_SIDE: f32 = 44.0;

/// The hit region: the frame, extended to at least 44 × 44 pt about its
/// centre **[chosen]**.
#[must_use]
pub const fn hit_region(frame: Rect) -> Rect {
    let [cx, cy] = frame.center();
    let w = frame.w.max(MIN_HIT_SIDE);
    let h = frame.h.max(MIN_HIT_SIDE);
    Rect::new(w.mul_add(-0.5, cx), h.mul_add(-0.5, cy), w, h)
}

/// How far outside the hit region a touch may stray before the press
/// cancels, pt **[chosen]**.
pub const CANCEL_DISTANCE: f32 = 70.0;

/// Whether a touch `distance` pt outside the hit region cancels the press:
/// within [`CANCEL_DISTANCE`] the press stays active and a release fires
/// the action.
#[must_use]
pub fn cancels_press(distance: f32) -> bool {
    distance > CANCEL_DISTANCE
}

/// Growth of the whole button at full press: `s = 1 + 0.245·p`
/// **[measured]**.
pub const GROWTH_PER_PRESS: f32 = 0.245;

/// The uniform scale of a button at press progress `p`.
#[must_use]
pub const fn growth(p: f32) -> f32 {
    GROWTH_PER_PRESS.mul_add(p, 1.0)
}

/// Peak alpha of the press wash at full press **[measured]**.
pub const WASH_ALPHA: f32 = 0.30;
/// How much the wash fades toward the ends of the button
/// (`1 − 0.37·u²`) **[measured]**.
pub const WASH_EDGE_FADE: f32 = 0.37;

/// Alpha of the white press wash at press progress `p`, `u` being the
/// pixel's horizontal distance from the touch over half the grown width,
/// clamped to 1. Zero while `p` is negative.
#[must_use]
pub fn wash_alpha(p: f32, u: f32) -> f32 {
    let u = u.min(1.0);
    p.max(0.0) * WASH_ALPHA * (-WASH_EDGE_FADE * u).mul_add(u, 1.0)
}

/// A unit-mass damped spring that drives the press progress toward its
/// target (§8.1, Press motion). **[fitted]**
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Spring {
    /// Natural frequency ω0, rad/s.
    pub omega0: f32,
    /// Damping ratio ζ.
    pub zeta: f32,
}

impl Spring {
    /// The spring a touch-down drives `p` toward 1 with.
    pub const PRESS: Self = Self {
        omega0: 28.2,
        zeta: 0.65,
    };
    /// The spring a release or cancellation drives `p` toward 0 with.
    pub const RELEASE: Self = Self {
        omega0: 15.8,
        zeta: 0.385,
    };

    /// The framework animation that plays this spring from the current
    /// value toward its target.
    ///
    /// `Animation::spring` is normalised over its own fixed duration, so
    /// the stiffness and damping handed to it are the spring's in that
    /// time unit: `ω = ω0·T`, stiffness `ω²`, damping `2ζω`.
    #[must_use]
    pub fn animation(self) -> Animation {
        let omega = self.omega0 * Animation::spring(1.0, 0.0).duration().as_secs_f32();
        Animation::spring(omega * omega, 2.0 * self.zeta * omega)
    }
}

/// The default tint (the accent colour): (0, 145, 255) in dark appearance,
/// (0, 136, 255) in light **[measured]**.
#[must_use]
pub const fn default_tint(appearance: Appearance) -> Rgba {
    match appearance {
        Appearance::Dark => [0.0, 145.0 / 255.0, 1.0, 1.0],
        Appearance::Light => [0.0, 136.0 / 255.0, 1.0, 1.0],
    }
}

/// The grey of a disabled prominent fill and how far the body moves toward
/// it, `mix(body, g, k)` **[fitted]**.
#[must_use]
pub const fn disabled_prominent_fill(appearance: Appearance) -> (f32, f32) {
    match appearance {
        Appearance::Dark => (0.60, 0.20),
        Appearance::Light => (0.55, 0.25),
    }
}

/// The label colour dimmed for a disabled button: the colour beneath it
/// plus 50 levels per channel in dark appearance and minus 75 in light,
/// clamped **[measured]**.
#[must_use]
pub fn dimmed_label(beneath: [f32; 3], appearance: Appearance) -> [f32; 3] {
    let offset = match appearance {
        Appearance::Dark => 50.0 / 255.0,
        Appearance::Light => -75.0 / 255.0,
    };
    beneath.map(|c| (c + offset).clamp(0.0, 1.0))
}

#[cfg(test)]
#[allow(
    clippy::suboptimal_flops,
    reason = "tests spell out the specification's arithmetic"
)]
mod tests {
    use core::time::Duration;

    use super::*;

    fn near(a: f32, b: f32, tol: f32) {
        assert!((a - b).abs() <= tol, "{a} vs {b}");
    }

    fn frame_of(group: SizeGroup, label: [f32; 2], border: BorderShape) -> [f32; 2] {
        frame_size(group, label, border)
    }

    #[test]
    fn control_sizes_map_onto_the_three_groups() {
        assert_eq!(SizeGroup::of(ControlSize::ExtraSmall), SizeGroup::Compact);
        assert_eq!(SizeGroup::of(ControlSize::Small), SizeGroup::Compact);
        assert_eq!(SizeGroup::of(ControlSize::Medium), SizeGroup::Regular);
        assert_eq!(SizeGroup::of(ControlSize::Large), SizeGroup::Large);
        assert_eq!(SizeGroup::of(ControlSize::ExtraLarge), SizeGroup::Large);
    }

    #[test]
    fn frames_reproduce_the_observed_table() {
        // (group, label layout size, observed frame)
        let rows = [
            (SizeGroup::Compact, [37.67, 18.0], [57.67, 28.0]),
            (SizeGroup::Regular, [42.0, 20.33], [66.0, 34.33]),
            (SizeGroup::Large, [42.0, 20.33], [82.0, 50.33]),
            (SizeGroup::Regular, [68.67, 20.33], [92.67, 34.33]),
            // a filled star symbol
            (SizeGroup::Regular, [22.0, 20.67], [46.0, 34.67]),
            (SizeGroup::Large, [22.0, 20.67], [62.0, 50.67]),
            (SizeGroup::Compact, [19.33, 18.33], [39.33, 28.33]),
            // two lines
            (SizeGroup::Regular, [62.33, 42.33], [86.33, 56.33]),
        ];
        for (group, label, frame) in rows {
            let got = frame_of(group, label, BorderShape::Capsule);
            near(got[0], frame[0], 0.01);
            near(got[1], frame[1], 0.01);
        }
    }

    #[test]
    fn the_icon_and_title_label_adds_the_symbol_and_the_gap() {
        near(icon_title_label_width(SizeGroup::Regular, 42.0), 72.0, 1e-4);
        near(
            icon_title_label_width(SizeGroup::Compact, 37.67),
            19.33 + 8.0 + 37.67,
            1e-2,
        );
    }

    #[test]
    fn a_circle_replaces_the_horizontal_inset_by_the_vertical_one() {
        let frame = frame_of(SizeGroup::Regular, [22.0, 20.67], BorderShape::Circle);
        near(frame[0], 36.0, 1e-4);
        near(frame[1], 34.67, 1e-4);
        let s = shape(
            Rect::new(0.0, 0.0, frame[0], frame[1]),
            BorderShape::Circle,
            SizeGroup::Regular,
        );
        near(s.rect.w, 34.67, 1e-3);
        near(s.rect.h, 34.67, 1e-3);
        near(s.radii[0], 34.67 / 2.0, 1e-3);
        near(s.rect.center()[0], 18.0, 1e-3);
    }

    #[test]
    fn a_capsule_takes_half_the_height_with_continuous_corners() {
        let s = shape(
            Rect::new(0.0, 0.0, 86.33, 56.33),
            BorderShape::Capsule,
            SizeGroup::Regular,
        );
        assert_eq!(s.radii, [56.33 * 0.5; 4]);
        assert_eq!(s.curve, CornerCurve::Continuous);
    }

    #[test]
    fn a_rounded_rectangle_takes_its_default_or_the_given_radius() {
        let frame = Rect::new(0.0, 0.0, 100.0, 40.0);
        for (group, radius) in [
            (SizeGroup::Compact, 4.8),
            (SizeGroup::Regular, 6.9),
            (SizeGroup::Large, 8.9),
        ] {
            let s = shape(frame, BorderShape::RoundedRect(None), group);
            assert_eq!(s.radii, [radius; 4]);
        }
        let s = shape(
            frame,
            BorderShape::RoundedRect(Some(4.0)),
            SizeGroup::Regular,
        );
        assert_eq!(s.radii, [4.0; 4]);
    }

    #[test]
    fn the_hit_region_reaches_44_about_the_centre() {
        let h = hit_region(Rect::new(10.0, 10.0, 57.67, 28.0));
        near(h.w, 57.67, 1e-4);
        near(h.h, 44.0, 1e-4);
        near(h.center()[1], 24.0, 1e-4);
        let h = hit_region(Rect::new(0.0, 0.0, 30.0, 28.0));
        near(h.w, 44.0, 1e-4);
        near(h.center()[0], 15.0, 1e-4);
    }

    #[test]
    fn a_press_cancels_beyond_seventy_points() {
        assert!(!cancels_press(39.0));
        assert!(!cancels_press(70.0));
        assert!(cancels_press(70.5));
    }

    #[test]
    fn growth_is_one_plus_245_thousandths_of_the_press() {
        near(growth(0.0), 1.0, 1e-6);
        near(growth(1.0), 1.245, 1e-6);
        // The press spring's peak of 1.07 gives the observed 1.263.
        near(growth(1.07), 1.262_15, 1e-4);
        near(growth(-0.27), 0.933_85, 1e-4);
    }

    #[test]
    fn the_wash_is_brightest_at_the_touch_and_absent_below_rest() {
        near(wash_alpha(1.0, 0.0), 0.30, 1e-6);
        near(wash_alpha(1.0, 1.0), 0.30 * 0.63, 1e-6);
        near(wash_alpha(1.0, 3.0), 0.30 * 0.63, 1e-6);
        assert_eq!(wash_alpha(-0.2, 0.0), 0.0);
        near(wash_alpha(0.5, 0.0), 0.15, 1e-6);
    }

    #[test]
    fn the_springs_reproduce_the_fitted_motion() {
        let press = Spring::PRESS.animation();
        // From rest: peak 1.07 at 0.147 s, within 2 % of 1 after 0.23 s.
        near(press.progress(Duration::from_millis(147)), 1.07, 0.01);
        near(press.progress(Duration::from_millis(230)), 1.0, 0.02);
        let release = Spring::RELEASE.animation();
        // From a settled press the release runs 1 -> 0: -0.27 at 0.215 s,
        // +0.07 at 0.43 s.
        near(
            1.0 - release.progress(Duration::from_millis(215)),
            -0.27,
            0.01,
        );
        near(
            1.0 - release.progress(Duration::from_millis(430)),
            0.07,
            0.01,
        );
    }

    #[test]
    fn the_dimmed_label_offsets_the_colour_beneath() {
        let d = dimmed_label([32.0 / 255.0; 3], Appearance::Dark);
        near(d[0] * 255.0, 82.0, 1e-3);
        let d = dimmed_label([184.0 / 255.0; 3], Appearance::Dark);
        near(d[1] * 255.0, 234.0, 1e-3);
        let d = dimmed_label([132.0 / 255.0; 3], Appearance::Light);
        near(d[2] * 255.0, 57.0, 1e-3);
        let d = dimmed_label([30.0 / 255.0; 3], Appearance::Light);
        assert_eq!(d, [0.0; 3]);
    }

    #[test]
    fn the_disabled_fill_reproduces_the_fitted_values() {
        let (g, k) = disabled_prominent_fill(Appearance::Dark);
        near(32.0f32.mul_add(1.0 - k, 255.0 * g * k), 56.0, 0.5);
        near(184.0f32.mul_add(1.0 - k, 255.0 * g * k), 178.0, 0.5);
        let (g, k) = disabled_prominent_fill(Appearance::Light);
        near(132.0f32.mul_add(1.0 - k, 255.0 * g * k), 134.0, 0.5);
        near(252.0f32.mul_add(1.0 - k, 255.0 * g * k), 224.0, 0.5);
    }
}
