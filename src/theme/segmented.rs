//! The glass segmented control (specification §8.4): geometry, colours,
//! the selection motion, the touch laws and the chrome a segment records.
//!
//! The laws are pure functions of the specification's tables, so the widget
//! theme ([`super::Glass`]) and the verification scenes build the control
//! from the same decisions. At rest no part is glass: the container and the
//! thumb are flat fills blended in encoded space. Glass appears only on the
//! thumb while a touch presses it.

use core::time::Duration;

use cherenkov::{BlendSpace, Draw as _, Fixed, Group as LayerGroup, Recorder};
use waterui_controls::ControlSize;

use crate::material::geometry::{CornerCurve, Element, Rect, Shape};
use crate::material::recipe::{Appearance, Family, Recipe, Translucency};

use super::button::Spring;
use super::materials;
use super::palette::{Palette, Rgba, working};

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
    /// # Panics
    /// On a control size the specification gives no segmented metrics for.
    #[must_use]
    pub fn of(size: ControlSize) -> Self {
        match size {
            ControlSize::ExtraSmall | ControlSize::Small | ControlSize::Medium => Self::Regular,
            ControlSize::Large | ControlSize::ExtraLarge => Self::Large,
            other => {
                panic!("the glass segmented control has no metrics for control size {other:?}")
            }
        }
    }

    /// The control's frame height, pt **[measured: simulator]**.
    #[must_use]
    pub const fn frame_height(self) -> f32 {
        match self {
            Self::Regular => 31.0,
            Self::Large => 48.0,
        }
    }
}

/// How far the container overhangs the frame's bottom edge, pt: it is 1 pt
/// taller than the frame and takes no layout space **[measured:
/// simulator]**.
pub const CONTAINER_OVERHANG: f32 = 1.0;
/// The thumb's inset from its segment on every side, pt **[measured:
/// simulator]**.
pub const THUMB_INSET: f32 = 2.0;
/// Alpha of the whole control while it is disabled **[fitted: simulator]**.
pub const DISABLED_OPACITY: f32 = 0.5;

/// The container's height for a control framed `frame_height` tall, pt.
#[must_use]
pub fn container_height(frame_height: f32) -> f32 {
    frame_height + CONTAINER_OVERHANG
}

/// The thumb's height for a control framed `frame_height` tall: the
/// container's minus the inset on both sides, pt.
#[must_use]
pub fn thumb_height(frame_height: f32) -> f32 {
    THUMB_INSET.mul_add(-2.0, container_height(frame_height))
}

/// The thumb under `segment`: inset 2 pt from it on every side of the
/// container, so `W / n − 4` wide.
#[must_use]
pub fn segment_thumb(segment: Rect) -> Rect {
    Rect::new(
        segment.x + THUMB_INSET,
        segment.y + THUMB_INSET,
        THUMB_INSET.mul_add(-2.0, segment.w),
        thumb_height(segment.h),
    )
}

/// The thumb drawn at display position `s` (the selected index as a real
/// number) in a control framed by `frame` with `count` equal segments.
#[must_use]
#[allow(clippy::cast_precision_loss, reason = "a segment count is a handful")]
pub fn display_thumb(frame: Rect, count: usize, s: f32) -> Rect {
    let width = frame.w / count as f32;
    segment_thumb(Rect::new(
        s.mul_add(width, frame.x),
        frame.y,
        width,
        frame.h,
    ))
}

/// Where a segment lies in the control.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Position {
    /// The only segment.
    Only,
    /// The first of several.
    First,
    /// Neither the first nor the last.
    Middle,
    /// The last of several.
    Last,
}

impl Position {
    /// The position of a segment the picker reports as first and/or last.
    #[must_use]
    pub const fn of(is_first: bool, is_last: bool) -> Self {
        match (is_first, is_last) {
            (true, true) => Self::Only,
            (true, false) => Self::First,
            (false, true) => Self::Last,
            (false, false) => Self::Middle,
        }
    }
}

/// The container's part over `segment`.
///
/// A continuous capsule's slice, 1 pt taller than the segment, rounded on
/// the container's ends only: the first segment rounds its leading side,
/// the last its trailing side, and a control of one segment is the whole
/// capsule.
#[must_use]
pub fn container_slice(segment: Rect, position: Position) -> Shape {
    let rect = Rect::new(segment.x, segment.y, segment.w, container_height(segment.h));
    let r = rect.h * 0.5;
    let start = if matches!(position, Position::Only | Position::First) {
        r
    } else {
        0.0
    };
    let end = if matches!(position, Position::Only | Position::Last) {
        r
    } else {
        0.0
    };
    Shape::rounded_rect(rect, r, CornerCurve::Continuous).with_radii([start, end, end, start])
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

/// The display position `elapsed` after the selection moved from index
/// `from` to index `to` with the thumb at rest.
#[must_use]
pub fn display_position(from: f32, to: f32, elapsed: Duration) -> f32 {
    (to - from).mul_add(SELECTION.animation().progress(elapsed), from)
}

/// How far a touch may move and still be a tap, pt **[chosen]**.
pub const TAP_SLOP: f32 = 10.0;

/// The display position while a touch drags the thumb: the selected index
/// plus the touch's horizontal offset over the segment width, clamped to
/// the segments **[chosen]**.
#[must_use]
#[allow(clippy::cast_precision_loss, reason = "a segment count is a handful")]
pub fn drag_position(selected: usize, offset: f32, segment_width: f32, count: usize) -> f32 {
    (offset / segment_width + selected as f32).clamp(0.0, (count - 1) as f32)
}

/// The segment a release at display position `s` selects: the nearest one
/// **[chosen]**.
#[must_use]
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    reason = "the position is clamped to the segment indices first, a handful"
)]
pub const fn nearest_segment(s: f32, count: usize) -> usize {
    s.round().clamp(0.0, (count - 1) as f32) as usize
}

/// Smallest height of a segment's touch target, pt **[chosen]**.
pub const MIN_HIT_HEIGHT: f32 = 44.0;

/// A segment's hit region: the segment, extended vertically to at least
/// 44 pt about the container's centre **[chosen]**.
#[must_use]
pub fn hit_region(segment: Rect) -> Rect {
    let container = Rect::new(segment.x, segment.y, segment.w, container_height(segment.h));
    let [_, cy] = container.center();
    let h = container.h.max(MIN_HIT_HEIGHT);
    Rect::new(segment.x, h.mul_add(-0.5, cy), segment.w, h)
}

/// Growth of a pressed thumb: `1 + 0.15·p` about its centre **[chosen]**.
pub const LENS_GROWTH_PER_PRESS: f32 = 0.15;

/// The thumb's scale at press progress `p`.
#[must_use]
pub const fn lens_scale(p: f32) -> f32 {
    LENS_GROWTH_PER_PRESS.mul_add(p, 1.0)
}

/// What one segment of the control is, and what a touch is doing to it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SegmentSpec {
    /// The segment's bounds: its share of the frame.
    pub bounds: Rect,
    /// Where it lies in the control.
    pub position: Position,
    /// Whether it is the selected one.
    pub selected: bool,
    /// Whether the control responds to touches.
    pub enabled: bool,
    /// The press progress `p` of a touch on this segment; the springs of
    /// §8.1 overshoot it.
    pub press: f32,
}

/// The thumb of a segment: a flat fill, and the clear glass it lies over
/// while pressed.
#[derive(Clone, Debug, PartialEq)]
pub struct Thumb {
    /// The thumb's silhouette, grown by the press.
    pub shape: Shape,
    /// The fill, straight alpha.
    pub fill: Rgba,
    /// The opacity the fill is drawn at: `1 − max(p, 0)`.
    pub fill_opacity: f32,
    /// The clear-family glass under the fill while pressed.
    pub lens: Option<Element>,
}

/// What a segment records: its slice of the container and, when it is
/// selected, the thumb.
#[derive(Clone, Debug, PartialEq)]
pub struct Chrome {
    /// The container's part over the segment.
    pub container: Shape,
    /// The container's fill, straight alpha.
    pub container_fill: Rgba,
    /// The selected segment's thumb.
    pub thumb: Option<Thumb>,
    /// The opacity the whole control is composited at.
    pub opacity: f32,
}

/// The chrome of one segment (§8.4).
///
/// A pressed thumb scales about its centre and becomes a clear-family glass
/// element whose recipe keeps the idle thumb's `m` (§5.5), under its fill
/// drawn at `1 − max(p, 0)`. A disabled control is the plain one at half
/// opacity and does not respond to a touch.
#[must_use]
pub fn chrome(palette: &Palette, spec: &SegmentSpec, translucency: Translucency) -> Chrome {
    let appearance = palette.appearance;
    let p = if spec.enabled { spec.press } else { 0.0 };
    let thumb = spec.selected.then(|| {
        let idle = segment_thumb(spec.bounds);
        let scale = lens_scale(p);
        let rect = idle.scaled_about_center(scale);
        let shape = Shape::rounded_rect(rect, rect.h * 0.5, CornerCurve::Continuous);
        let lens = (p > 0.0).then(|| Element {
            shape,
            recipe: Recipe::for_element(idle.w, idle.h, Family::Clear, appearance, translucency),
        });
        Thumb {
            shape,
            fill: thumb_fill(appearance),
            fill_opacity: 1.0 - p.max(0.0),
            lens,
        }
    });
    Chrome {
        container: container_slice(spec.bounds, spec.position),
        container_fill: container_fill(appearance),
        thumb,
        opacity: if spec.enabled { 1.0 } else { DISABLED_OPACITY },
    }
}

/// Records a segment's chrome.
///
/// The fills blend in encoded space, the control as one group at its
/// opacity. A pressed thumb splits the group around the lens: the
/// container's slice below the glass, the thumb's fill above it.
///
/// # Panics
/// As `Recorder::backdrop_material` does when a lens is recorded: outside a
/// `Content::record_layered` recording, or inside a clip, transform or
/// group scope.
pub fn record(recorder: &mut Recorder, chrome: &Chrome) {
    let group = LayerGroup::new()
        .blend_space(BlendSpace::SrgbEncoded)
        .opacity(chrome.opacity);
    let fill_container = |recorder: &mut Recorder| {
        recorder.fill(
            Fixed(materials::shape_data(&chrome.container)),
            working(chrome.container_fill),
        );
    };
    match &chrome.thumb {
        None => recorder.group(group, fill_container),
        Some(thumb) => {
            let fill_thumb = |recorder: &mut Recorder| {
                recorder.fill(
                    Fixed(materials::shape_data(&thumb.shape)),
                    working(thumb.fill),
                );
            };
            match &thumb.lens {
                None => recorder.group(group, |recorder| {
                    fill_container(recorder);
                    fill_thumb(recorder);
                }),
                Some(lens) => {
                    recorder.group(group, fill_container);
                    materials::record(recorder, lens, false);
                    recorder.group(
                        group.opacity(thumb.fill_opacity * chrome.opacity),
                        fill_thumb,
                    );
                }
            }
        }
    }
}

#[cfg(test)]
#[allow(
    clippy::suboptimal_flops,
    reason = "tests spell out the specification's arithmetic"
)]
mod tests {
    use super::*;
    use crate::theme::palette::over;

    fn near(a: f32, b: f32, tol: f32) {
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
            assert_eq!(group.frame_height(), frame);
            assert_eq!(container_height(frame), container);
            assert_eq!(thumb_height(frame), thumb);
        }
    }

    #[test]
    fn the_first_of_three_segments_has_a_96_by_28_thumb_at_x_2() {
        let thumb = display_thumb(Rect::new(0.0, 0.0, 300.0, 31.0), 3, 0.0);
        assert_eq!(thumb, Rect::new(2.0, 2.0, 96.0, 28.0));
        // The segment's own thumb is the same inset.
        assert_eq!(
            segment_thumb(Rect::new(0.0, 0.0, 100.0, 31.0)),
            Rect::new(2.0, 2.0, 96.0, 28.0)
        );
    }

    #[test]
    fn the_container_overhangs_the_frame_by_one_point() {
        let slice = container_slice(Rect::new(0.0, 10.0, 100.0, 31.0), Position::Only);
        assert_eq!(slice.rect, Rect::new(0.0, 10.0, 100.0, 32.0));
        assert_eq!(slice.radii, [16.0; 4]);
        assert_eq!(slice.curve, CornerCurve::Continuous);
    }

    #[test]
    fn only_the_ends_of_the_container_are_rounded() {
        let seg = Rect::new(0.0, 0.0, 100.0, 31.0);
        assert_eq!(
            container_slice(seg, Position::First).radii,
            [16.0, 0.0, 0.0, 16.0]
        );
        assert_eq!(
            container_slice(seg, Position::Last).radii,
            [0.0, 16.0, 16.0, 0.0]
        );
        assert_eq!(container_slice(seg, Position::Middle).radii, [0.0; 4]);
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
                near(red(c_fill), c, 1.0);
                near(red(over(thumb_fill(appearance), c_fill)), t, 1.0);
            }
        }
    }

    #[test]
    fn the_selection_spring_moves_the_thumb_as_observed() {
        // §7, scene 13: the thumb's centre in a 300 pt control, first to
        // third segment.
        let centre = |t_ms| {
            let s = display_position(0.0, 2.0, Duration::from_millis(t_ms));
            let thumb = display_thumb(Rect::new(0.0, 0.0, 300.0, 31.0), 3, s);
            thumb.center()[0]
        };
        near(centre(0), 50.0, 1e-3);
        near(centre(50), 89.5, 1.0);
        near(centre(100), 152.0, 1.0);
        near(centre(200), 229.0, 1.0);
        near(centre(380), 251.3, 0.5);
        near(centre(600), 250.0, 0.3);
    }

    #[test]
    fn a_drag_follows_the_touch_and_releases_on_the_nearest_segment() {
        near(drag_position(1, 30.0, 100.0, 3), 1.3, 1e-5);
        assert_eq!(drag_position(1, -500.0, 100.0, 3), 0.0);
        assert_eq!(drag_position(1, 500.0, 100.0, 3), 2.0);
        assert_eq!(nearest_segment(1.3, 3), 1);
        assert_eq!(nearest_segment(1.6, 3), 2);
        assert_eq!(nearest_segment(2.0, 3), 2);
    }

    #[test]
    fn the_hit_region_reaches_44_about_the_container_centre() {
        let seg = Rect::new(10.0, 100.0, 100.0, 31.0);
        let hit = hit_region(seg);
        assert_eq!((hit.x, hit.w, hit.h), (10.0, 100.0, 44.0));
        near(hit.center()[1], 116.0, 1e-4);
        let tall = hit_region(Rect::new(0.0, 0.0, 100.0, 48.0));
        assert_eq!(tall.h, 49.0);
    }

    #[test]
    fn the_lens_grows_by_fifteen_hundredths_of_the_press() {
        near(lens_scale(0.0), 1.0, 1e-6);
        near(lens_scale(1.0), 1.15, 1e-6);
        // About the container's height at the regular size, and 7.2 pt past
        // the idle thumb's edge on each side.
        let idle = segment_thumb(Rect::new(0.0, 0.0, 100.0, 31.0));
        let grown = idle.scaled_about_center(lens_scale(1.0));
        near(grown.h, 32.2, 1e-3);
        near(grown.x, idle.x - 7.2, 1e-3);
    }

    fn spec(selected: bool, enabled: bool, press: f32) -> SegmentSpec {
        SegmentSpec {
            bounds: Rect::new(0.0, 0.0, 100.0, 31.0),
            position: Position::First,
            selected,
            enabled,
            press,
        }
    }

    #[test]
    fn only_the_selected_segment_draws_a_thumb() {
        let p = Palette::DARK;
        let t = Translucency::default();
        assert!(chrome(&p, &spec(false, true, 0.0), t).thumb.is_none());
        let selected = chrome(&p, &spec(true, true, 0.0), t);
        let thumb = selected.thumb.expect("a thumb");
        assert!(thumb.lens.is_none());
        assert_eq!(thumb.fill_opacity, 1.0);
        assert_eq!(selected.opacity, 1.0);
    }

    #[test]
    fn a_pressed_thumb_is_a_clear_glass_lens_under_a_fading_fill() {
        let chrome = chrome(
            &Palette::LIGHT,
            &spec(true, true, 0.5),
            Translucency::default(),
        );
        let thumb = chrome.thumb.expect("a thumb");
        near(thumb.fill_opacity, 0.5, 1e-6);
        near(thumb.shape.rect.h, 28.0 * 1.075, 1e-3);
        let lens = thumb.lens.expect("a lens");
        assert_eq!(lens.shape, thumb.shape);
        assert_eq!(
            lens.recipe,
            Recipe::for_element(
                96.0,
                28.0,
                Family::Clear,
                Appearance::Light,
                Translucency::default()
            )
        );
    }

    #[test]
    fn a_disabled_control_is_the_plain_one_at_half_opacity_and_ignores_touches() {
        let chrome = chrome(
            &Palette::DARK,
            &spec(true, false, 1.0),
            Translucency::default(),
        );
        assert_eq!(chrome.opacity, DISABLED_OPACITY);
        let thumb = chrome.thumb.expect("a thumb");
        assert!(thumb.lens.is_none());
        assert_eq!(
            thumb.shape.rect,
            segment_thumb(Rect::new(0.0, 0.0, 100.0, 31.0))
        );
    }
}
