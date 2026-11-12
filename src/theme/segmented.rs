//! The glass segmented control (specification §8.4): geometry, colours,
//! the label fonts, the selection spring and the press lens.
//!
//! Everything here is a pure function of the specification's tables; the
//! segmented look ([`super::looks`]) builds the control from the same
//! decisions. The drag, its settle and the right-to-left mirror are
//! Hydrolysis's: the look names the drag and the renderer executes it. At
//! rest no part is glass: the container and the thumb are flat fills blended
//! in encoded space. Glass appears only on the thumb while a touch presses
//! it.

use cherenkov_record::MaterialEffect;
use cherenkov_record::kurbo::{Point, Rect};
use waterui::text::font::FontWeight;
use waterui_controls::ControlSize;
use waterui_core::contract_violation;

use crate::material::geometry::{CornerCurve, Element, Rect as ElementRect, Shape};
use crate::material::recipe::{Appearance, Family, Recipe, Translucency};

use super::button::Spring;
use super::materials;
use super::palette::Rgba;

/// The size groups of §8.4: mini, small and regular are identical, and so
/// are large and extra large.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SizeGroup {
    /// Mini, small and regular.
    Regular,
    /// Large and extra large.
    Large,
}

impl SizeGroup {
    /// The group a control size belongs to.
    ///
    /// A control size the specification gives no segmented metrics for is a
    /// contract violation, reported, and the control takes the regular
    /// group's.
    #[must_use]
    pub fn of(size: ControlSize) -> Self {
        match size {
            ControlSize::ExtraSmall | ControlSize::Small | ControlSize::Medium => Self::Regular,
            ControlSize::Large | ControlSize::ExtraLarge => Self::Large,
            other => {
                contract_violation!(
                    "the glass segmented control has no metrics for control size {other:?}; \
                     it takes the regular size group's"
                );
                Self::Regular
            }
        }
    }

    /// The control's frame height, pt **[measured: simulator]**.
    #[must_use]
    pub const fn frame_height(self) -> f64 {
        match self {
            Self::Regular => 31.0,
            Self::Large => 48.0,
        }
    }
}

/// How far the container overhangs the frame's bottom edge, pt: it is 1 pt
/// taller than the frame and takes no layout space **[measured:
/// simulator]**.
pub const CONTAINER_OVERHANG: f64 = 1.0;
/// The thumb's inset from its segment on every side of the container, pt
/// **[measured: simulator]**.
pub const THUMB_INSET: f64 = 2.0;
/// Opacity of the whole control while it is disabled **[fitted:
/// simulator]**.
pub const DISABLED_OPACITY: f32 = 0.5;
/// Size of a label's text at every size group, pt **[measured: simulator]**.
pub const LABEL_SIZE: f32 = 13.0;

/// A label's weight: medium on the selected segment, regular elsewhere
/// **[measured: simulator]**.
#[must_use]
pub const fn label_weight(selected: bool) -> FontWeight {
    if selected {
        FontWeight::Medium
    } else {
        FontWeight::Normal
    }
}

/// The container of a control whose row is `row`: the row's width, its top
/// edge, and 1 pt taller than the row.
#[must_use]
pub fn container_rect(row: Rect) -> Rect {
    Rect::new(row.x0, row.y0, row.x1, row.y1 + CONTAINER_OVERHANG)
}

/// The width of each of `count` equal segments of `row`, whatever their
/// labels.
#[must_use]
#[allow(clippy::cast_precision_loss, reason = "a segment count is a handful")]
pub fn segment_width(row: Rect, count: usize) -> f64 {
    row.width() / count as f64
}

/// The thumb at display position `s` in a control of `count` segments laid
/// out on `row`, scaled about its centre by `scale`.
///
/// `s` is the selected index as a real number. At rest the thumb's leading
/// edge is at `s · W / n + 2`, and it is `W / n − 4` wide and the
/// container's height less 4.
#[must_use]
pub fn thumb_rect(row: Rect, count: usize, s: f64, scale: f64) -> Rect {
    let width = segment_width(row, count);
    let container = container_rect(row);
    let x0 = s.mul_add(width, row.x0) + THUMB_INSET;
    let rest = Rect::new(
        x0,
        container.y0 + THUMB_INSET,
        x0 + THUMB_INSET.mul_add(-2.0, width),
        container.y1 - THUMB_INSET,
    );
    Rect::from_center_size(rest.center(), rest.size() * scale)
}

/// The container's fill, straight alpha over the backdrop **[chosen]**.
#[must_use]
pub const fn container_fill(appearance: Appearance) -> Rgba {
    match appearance {
        Appearance::Dark => [117.0 / 255.0, 117.0 / 255.0, 130.0 / 255.0, 0.239],
        Appearance::Light => [115.0 / 255.0, 115.0 / 255.0, 123.0 / 255.0, 0.122],
    }
}

/// The thumb's fill, straight alpha over the container **[chosen]**.
#[must_use]
pub const fn thumb_fill(appearance: Appearance) -> Rgba {
    match appearance {
        Appearance::Dark => [235.0 / 255.0, 235.0 / 255.0, 245.0 / 255.0, 0.30],
        Appearance::Light => [1.0, 1.0, 1.0, 1.0],
    }
}

/// The selection spring **[fitted: simulator]**.
///
/// The display position `s` moves to the new index by it on every selection
/// change, whether or not the change was made with an animation. It reuses
/// the button's spring primitive.
pub const SELECTION: Spring = Spring {
    omega0: 15.7,
    zeta: 0.85,
};

/// How far a touch may move and still be a tap, pt **[chosen]**.
pub const TAP_SLOP: f64 = 10.0;

/// Smallest height of a segment's touch target, pt **[chosen]**.
pub const MIN_HIT_HEIGHT: f64 = 44.0;

/// Growth of a pressed thumb: `1 + 0.15·p` about its centre **[chosen]**.
pub const LENS_GROWTH_PER_PRESS: f64 = 0.15;

/// The thumb's scale at press progress `p`.
#[must_use]
pub const fn lens_scale(p: f64) -> f64 {
    LENS_GROWTH_PER_PRESS.mul_add(p, 1.0)
}

/// The clear-family recipe of the lens a control's thumb becomes: the idle
/// thumb's, `w` by `h` pt, so the press keeps its `m` (§5.5).
fn lens_recipe(w: f64, h: f64, appearance: Appearance, translucency: Translucency) -> Recipe {
    #[expect(
        clippy::cast_possible_truncation,
        reason = "a thumb is tens of points across, far inside f32"
    )]
    Recipe::for_element(w as f32, h as f32, Family::Clear, appearance, translucency)
}

/// The material the pressed thumb of a control laid out on `row` with
/// `count` segments is: the clear family's recipe at the idle thumb's size.
///
/// # Panics
/// If the clear recipe bends its normal toward the thumb's ellipse, which
/// needs the lens's position: the effect is stated for the idle thumb at the
/// origin, wherever the thumb is.
#[must_use]
pub fn lens_effect(
    row: Rect,
    count: usize,
    appearance: Appearance,
    translucency: Translucency,
) -> MaterialEffect {
    let idle = thumb_rect(row, count, 0.0, 1.0);
    let recipe = lens_recipe(idle.width(), idle.height(), appearance, translucency);
    assert!(
        recipe.ellipse_bend == 0.0,
        "the lens effect carries no position, so its recipe cannot bend toward the thumb's ellipse"
    );
    #[expect(
        clippy::cast_possible_truncation,
        reason = "a thumb is tens of points across, far inside f32"
    )]
    let (w, h) = (idle.width() as f32, idle.height() as f32);
    let shape = Shape::rounded_rect(
        ElementRect::new(0.0, 0.0, w, h),
        h * 0.5,
        CornerCurve::Continuous,
    );
    materials::effect(&Element { shape, recipe })
}

/// The recipe's capture scale of a `group` control's lens. The clear
/// family's capture scale does not depend on the element's size, so the
/// thumb's height stands for its short side.
#[must_use]
pub fn lens_capture_scale(
    group: SizeGroup,
    appearance: Appearance,
    translucency: Translucency,
) -> f32 {
    let h = 2.0f64.mul_add(-THUMB_INSET, group.frame_height() + CONTAINER_OVERHANG);
    lens_recipe(h, h, appearance, translucency).capture_scale
}

/// The point the pressed thumb's lens collapses to while no touch presses
/// it: the thumb's centre.
#[must_use]
pub fn lens_centre(row: Rect, count: usize, s: f64) -> Point {
    thumb_rect(row, count, s, 1.0).center()
}

#[cfg(test)]
#[allow(
    clippy::suboptimal_flops,
    reason = "tests spell out the specification's arithmetic"
)]
mod tests {
    use core::time::Duration;

    use super::*;
    use crate::theme::palette::over;

    fn near(a: f64, b: f64, tol: f64) {
        assert!((a - b).abs() <= tol, "{a} vs {b}");
    }

    #[test]
    fn control_sizes_map_onto_the_two_groups() {
        for size in [
            ControlSize::ExtraSmall,
            ControlSize::Small,
            ControlSize::Medium,
        ] {
            assert_eq!(SizeGroup::of(size), SizeGroup::Regular);
        }
        for size in [ControlSize::Large, ControlSize::ExtraLarge] {
            assert_eq!(SizeGroup::of(size), SizeGroup::Large);
        }
    }

    #[test]
    fn heights_follow_the_size_group() {
        for (group, frame, container, thumb) in [
            (SizeGroup::Regular, 31.0, 32.0, 28.0),
            (SizeGroup::Large, 48.0, 49.0, 45.0),
        ] {
            let row = Rect::new(0.0, 0.0, 300.0, group.frame_height());
            assert_eq!(group.frame_height(), frame);
            assert_eq!(container_rect(row).height(), container);
            assert_eq!(thumb_rect(row, 3, 0.0, 1.0).height(), thumb);
        }
    }

    #[test]
    fn the_first_of_three_segments_has_a_96_by_28_thumb_at_x_2() {
        let thumb = thumb_rect(Rect::new(0.0, 0.0, 300.0, 31.0), 3, 0.0, 1.0);
        assert_eq!(thumb, Rect::new(2.0, 2.0, 98.0, 30.0));
        // The leading edge is `s · W / n + 2` at every display position.
        let between = thumb_rect(Rect::new(10.0, 0.0, 310.0, 31.0), 3, 1.25, 1.0);
        near(between.x0, 10.0 + 1.25 * 100.0 + 2.0, 1e-9);
        near(between.width(), 96.0, 1e-9);
    }

    #[test]
    fn the_container_overhangs_the_frame_by_one_point() {
        let row = Rect::new(0.0, 10.0, 100.0, 41.0);
        assert_eq!(container_rect(row), Rect::new(0.0, 10.0, 100.0, 42.0));
    }

    #[test]
    fn labels_are_medium_when_selected_and_regular_otherwise() {
        assert_eq!(label_weight(true), FontWeight::Medium);
        assert_eq!(label_weight(false), FontWeight::Normal);
        assert_eq!(LABEL_SIZE, 13.0);
    }

    /// The red channel of the fills over a backdrop, 8-bit: the values of
    /// §8.4 Colours.
    fn red(c: Rgba) -> f32 {
        (c[0] * 255.0).round()
    }

    #[test]
    fn the_container_and_thumb_reproduce_the_rendered_colours() {
        for (appearance, container, thumb) in [
            (Appearance::Dark, [28.0, 125.0, 222.0], [90.0, 158.0, 226.0]),
            (
                Appearance::Light,
                [14.0, 126.0, 238.0],
                [255.0, 255.0, 255.0],
            ),
        ] {
            for ((backdrop, c), t) in [0.0, 0.5, 1.0].into_iter().zip(container).zip(thumb) {
                let under = [backdrop, backdrop, backdrop, 1.0];
                let c_fill = over(container_fill(appearance), under);
                assert!((red(c_fill) - c).abs() <= 1.0, "{} vs {c}", red(c_fill));
                let t_fill = red(over(thumb_fill(appearance), c_fill));
                assert!((t_fill - t).abs() <= 1.0, "{t_fill} vs {t}");
            }
        }
    }

    /// The display position `elapsed` after the selection moved from index
    /// `from` to index `to` with the thumb at rest.
    fn display_position(from: f64, to: f64, elapsed: Duration) -> f64 {
        (to - from).mul_add(SELECTION.physical().progress(elapsed), from)
    }

    #[test]
    fn the_selection_spring_moves_the_thumb_as_observed() {
        // §7, scene 13: the thumb's centre in a 300 pt control, first to
        // third segment.
        let centre = |t_ms| {
            let s = display_position(0.0, 2.0, Duration::from_millis(t_ms));
            lens_centre(Rect::new(0.0, 0.0, 300.0, 31.0), 3, s).x
        };
        near(centre(0), 50.0, 1e-3);
        near(centre(50), 89.5, 1.0);
        near(centre(100), 152.0, 1.0);
        near(centre(200), 229.0, 1.0);
        near(centre(380), 251.3, 0.5);
        near(centre(600), 250.0, 0.3);
    }

    #[test]
    fn the_lens_grows_by_fifteen_hundredths_of_the_press() {
        near(lens_scale(0.0), 1.0, 1e-9);
        near(lens_scale(1.0), 1.15, 1e-9);
        // About the container's height at the regular size, and 7.2 pt past
        // the idle thumb's edge on each side.
        let row = Rect::new(0.0, 0.0, 300.0, 31.0);
        let idle = thumb_rect(row, 3, 0.0, 1.0);
        let grown = thumb_rect(row, 3, 0.0, lens_scale(1.0));
        near(grown.height(), 32.2, 1e-9);
        near(grown.x0, idle.x0 - 7.2, 1e-9);
        near(grown.center().x, idle.center().x, 1e-9);
        near(grown.center().y, idle.center().y, 1e-9);
    }

    #[test]
    fn the_lens_is_the_clear_family_at_the_idle_thumbs_size() {
        let row = Rect::new(0.0, 0.0, 300.0, 31.0);
        let t = Translucency::default();
        let shape = Shape::rounded_rect(
            ElementRect::new(0.0, 0.0, 96.0, 28.0),
            14.0,
            CornerCurve::Continuous,
        );
        let recipe = Recipe::for_element(96.0, 28.0, Family::Clear, Appearance::Light, t);
        assert_eq!(
            lens_effect(row, 3, Appearance::Light, t),
            materials::effect(&Element { shape, recipe })
        );
        assert_eq!(
            lens_capture_scale(SizeGroup::Regular, Appearance::Light, t),
            recipe.capture_scale
        );
    }
}
