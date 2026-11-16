//! The glass segmented control look (specification §8.4): a flat capsule
//! container, a flat capsule thumb under the selected segment's label, and
//! the thumb a glass lens over its neighbours and the labels while a touch
//! presses it.
//!
//! The geometry, colours and springs are [`super::super::segmented`]'s
//! tables; this builds the [`SegmentedLook`] from them. The look draws left
//! to right: Hydrolysis mirrors it in a right-to-left layout.

use cherenkov_record::kurbo::{Size, Stroke, Vec2};
use cherenkov_record::{BlendSpace, Draw as _, Group, Live, MaterialEffect, Paint, ShapeData};
use hydrolysis::theme::{
    Custom, Driven, DrivenSet, HitRegion, Inputs, Motion, Segment, SegmentLabel, SegmentedDrag,
    SegmentedLayout, SegmentedLook, SegmentedSpec, SegmentedValue, Transition, ValueMotion,
};
use waterui::animation::Animation;
use waterui::interaction::InteractionState;
use waterui::text::font::Font;
use waterui_core::Environment;

use super::super::Glass;
use super::super::chrome::{FOCUS_RING_GAP, FOCUS_RING_WIDTH};
use super::super::materials::{self, GLASS_SHADER};
use super::super::segmented::{
    self, CONTAINER_OVERHANG, DISABLED_OPACITY, LABEL_SIZE, MIN_HIT_HEIGHT, SELECTION, SizeGroup,
    TAP_SLOP,
};
use super::super::switch;
use super::super::{DRAG_DURATION, PRESS_DURATION};
use super::{faded, focus_ring_paint, held, is_disabled, paint};
use crate::theme::button::Spring;

/// Space between the picker's own label above the row and the row, pt: the
/// theme's picker label spacing.
const LABEL_SPACING: f64 = 8.0;
/// Room on each side of a label inside its segment, pt: the theme's picker
/// horizontal inset. The label is centred on both axes (§8.4), so the
/// padding only bounds how wide it may lay out.
const LABEL_PADDING_X: f64 = 12.0;
/// The narrowest a segment is measured, pt: the theme's picker segment
/// minimum.
const MINIMUM_SEGMENT_WIDTH: f64 = 64.0;
/// The narrowest the control is measured, pt: the theme's picker minimum.
const MINIMUM_WIDTH: f64 = 120.0;

/// The control's motion: eased hover, focus and drag, the press and release
/// springs of §8.1 for the press that makes the lens, and the selection
/// spring for every selection change, one made with animation disabled
/// included (§8.4, Selection motion).
fn motion() -> Motion {
    let eased = Transition::symmetric(Animation::ease_out(PRESS_DURATION));
    Motion {
        hover: eased.clone(),
        focus: eased,
        press: Transition::new(Spring::PRESS.physical(), Spring::RELEASE.physical()),
        drag: Transition::symmetric(Animation::ease_out(DRAG_DURATION)),
        value: ValueMotion::Always(SELECTION.physical()),
    }
}

/// The look of a segmented control in `spec`. The tint has no effect
/// (§8.4, States).
pub(in super::super) fn look(
    glass: &Glass,
    spec: &SegmentedSpec,
    _env: &Environment,
) -> SegmentedLook {
    let group = SizeGroup::of(spec.size());
    let count = spec.segment_count();
    let layout = SegmentedLayout {
        row_height: group.frame_height(),
        label_padding: Vec2::new(LABEL_PADDING_X, 0.0),
        minimum_segment_width: MINIMUM_SEGMENT_WIDTH,
        minimum_width: MINIMUM_WIDTH,
        label_spacing: LABEL_SPACING,
        // The container's overhang is all that draws past the row while
        // the control is disabled, the one state that fades it as a group:
        // a disabled control shows neither the lens nor the focus ring.
        reach: CONTAINER_OVERHANG,
    };
    // White in dark appearance, black in light, selected or not; 13 pt,
    // medium on the selected segment (§8.4, Geometry and Colours).
    let label = SegmentLabel::new(
        Driven::new(|inputs: &Inputs<'_, Segment>| {
            let selected = inputs.state.contains(InteractionState::SELECTED);
            Font::default()
                .size(LABEL_SIZE)
                .weight(segmented::label_weight(selected))
        }),
        Driven::<_, Segment>::constant(Glass::color(glass.palette.foreground)),
    );
    // Each segment extended vertically to 44 pt about its centre, a touch
    // that moves no more than 10 pt staying a tap (§8.4, Touch).
    let hit = HitRegion::NONE
        .minimum(Size::new(0.0, MIN_HIT_HEIGHT))
        .slop(TAP_SLOP);
    SegmentedLook::new(layout, label, motion())
        .hit(hit)
        .drag(SegmentedDrag::new(spec.direction()))
        // A disabled control is the enabled one composited at opacity 0.5 in
        // encoded space, labels included (§8.4, States).
        .group_opacity(Driven::new(|inputs: &Inputs<'_, ()>| {
            if is_disabled(inputs.state) {
                DISABLED_OPACITY
            } else {
                1.0
            }
        }))
        .group_blend_space(BlendSpace::SrgbEncoded)
        .part(body(glass, count))
        .part(focus_ring(glass, count))
        .over_labels(lens(glass, group, count))
}

/// The press progress the thumb's state drives toward: 1 held, 0 otherwise.
fn press(state: InteractionState) -> f64 {
    f64::from(u8::from(held(state)))
}

/// The thumb at the control's display position, grown about its centre by
/// the press.
fn thumb(count: usize, inputs: &Inputs<'_, SegmentedValue>) -> cherenkov_record::kurbo::Rect {
    segmented::thumb_rect(
        inputs.frame,
        count,
        inputs.value.position,
        segmented::lens_scale(press(inputs.state)),
    )
}

/// The container and the thumb's fill, one layer composited in encoded
/// space (§8.4, Colours). The fill fades as the press makes the thumb a
/// lens, which captures it with the labels over it.
fn body(
    glass: &Glass,
    count: usize,
) -> Custom<SegmentedValue, impl DrivenSet<SegmentedValue> + use<>> {
    let appearance = glass.palette.appearance;
    let container = Driven::new(|inputs: &Inputs<'_, SegmentedValue>| {
        switch::capsule(segmented::container_rect(inputs.frame))
    });
    let container_fill =
        Driven::<Paint, SegmentedValue>::constant(paint(segmented::container_fill(appearance)));
    let thumb_shape = Driven::new(move |inputs: &Inputs<'_, SegmentedValue>| {
        switch::capsule(thumb(count, inputs))
    });
    let thumb_fill = Driven::new(move |inputs: &Inputs<'_, SegmentedValue>| {
        paint(faded(
            segmented::thumb_fill(appearance),
            #[expect(
                clippy::cast_possible_truncation,
                reason = "the press target is 0 or 1"
            )]
            {
                1.0 - press(inputs.state) as f32
            },
        ))
    });
    Custom::new(
        (container, container_fill, thumb_shape, thumb_fill),
        |recorder, _frame, (container, container_fill, thumb, thumb_fill)| {
            recorder.group(
                Group::new().blend_space(BlendSpace::SrgbEncoded),
                |recorder| {
                    recorder.fill(container, container_fill);
                    recorder.fill(thumb, thumb_fill);
                },
            );
        },
    )
}

/// The lens a press makes of the thumb: a clear-family glass element over
/// the container, the neighbouring segments and the labels, the thumb scaled
/// about its centre by `1 + 0.15·p` and drawn at opacity `max(p, 0)`, so it
/// fades in and out with the press while the fill fades the other way
/// (§8.4, Touch). At rest it is at opacity 0, which the engine skips with
/// its capture: no part is glass.
///
/// The part's opacity runs with the control's press and release springs;
/// the release spring's undershoot below 0 takes the layer to an opacity at
/// or below 0, which shows nothing, so the opacity is `max(p, 0)`.
fn lens(
    glass: &Glass,
    group: SizeGroup,
    count: usize,
) -> Custom<SegmentedValue, impl DrivenSet<SegmentedValue> + use<>> {
    let (appearance, translucency) = (glass.palette.appearance, glass.translucency);
    let capture = materials::capture_class(
        false,
        segmented::lens_capture_scale(group, appearance, translucency),
    );
    let shape = Driven::new(move |inputs: &Inputs<'_, SegmentedValue>| {
        switch::capsule(thumb(count, inputs))
    });
    let effect = Driven::new(
        move |inputs: &Inputs<'_, SegmentedValue>| -> MaterialEffect {
            segmented::lens_effect(inputs.frame, count, appearance, translucency)
        },
    );
    Custom::new(
        (shape, effect),
        move |recorder, _frame, (shape, effect): (Live<ShapeData>, Live<MaterialEffect>)| {
            recorder.backdrop_material(shape, GLASS_SHADER, capture, effect);
        },
    )
    .opacity(Driven::new(|inputs: &Inputs<'_, SegmentedValue>| {
        f32::from(u8::from(held(inputs.state)))
    }))
}

/// The focus ring around the thumb, visible while the control has keyboard
/// focus. The specification leaves focus unobserved (§8.4, States); the ring
/// is the theme's, as every control's.
fn focus_ring(
    glass: &Glass,
    count: usize,
) -> Custom<SegmentedValue, impl DrivenSet<SegmentedValue> + use<>> {
    let reach = FOCUS_RING_WIDTH.mul_add(0.5, FOCUS_RING_GAP);
    let ring = Driven::new(move |inputs: &Inputs<'_, SegmentedValue>| {
        switch::capsule(thumb(count, inputs).inflate(reach, reach))
    });
    Custom::new(
        (ring, focus_ring_paint(glass.palette.accent)),
        |recorder, _frame, (ring, ring_paint)| {
            recorder.stroke(ring, Stroke::new(FOCUS_RING_WIDTH), ring_paint);
        },
    )
}
