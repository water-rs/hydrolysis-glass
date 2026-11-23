//! The glass slider look (specification §8.3): a flat capsule track filled
//! to the value, a flat white knob with a soft shadow, and the knob a glass
//! lens while a touch drags it.
//!
//! The geometry, colours and springs are [`super::super::slider`]'s tables;
//! this builds the [`SliderLook`] from them. The look draws left to right:
//! Hydrolysis mirrors it in a right-to-left layout.

use cherenkov_record::kurbo::{RoundedRect, RoundedRectRadii, Size, Stroke, Vec2};
use cherenkov_record::{
    BlendSpace, Draw as _, Group, Live, MaterialEffect, Paint, Shadow, ShapeData, WorkingColor,
};
use hydrolysis_style::theme::{
    Custom, DragMapping, Driven, DrivenSet, HitRegion, Inputs, Knob, Motion, SliderLayout,
    SliderLook, SliderSpec, Transition, ValueIndicator, ValueMotion,
};
use waterui::animation::Animation;
use waterui::interaction::InteractionState;
use waterui::text::font::{Font, Footnote};
use waterui::theme::Tint;
use waterui_core::Environment;

use super::super::Glass;
use super::super::PRESS_DURATION;
use super::super::button::MIN_HIT_SIDE;
use super::super::chrome::{
    FOCUS_RING_GAP, FOCUS_RING_WIDTH, HAIRLINE, MENU_CORNER_RADIUS, POPUP_SHADOW_ALPHA,
    POPUP_SHADOW_BLUR, POPUP_SHADOW_OFFSET,
};
use super::super::materials::{self, GLASS_SHADER};
use super::super::palette::{alpha, working};
use super::super::slider::{
    self, DISABLED_OPACITY, HEIGHT, KNOB_HEIGHT, KNOB_WIDTH, LABEL_GAP, LABEL_SPACING, MIN_TRAVEL,
    SHADOW_ALPHA, SHADOW_OFFSET, SHADOW_SIGMA, TRAVEL_INSET,
};
use super::super::switch::{self, KNOB_FILL};
use super::{focus_ring_paint, held, is_disabled, paint, spring_animation, stretch_along_axis};
use crate::dynamics;

/// The slider's motion: eased hover and focus, button press/release springs
/// for the lens, jelly drag release, and `SLIDER_VALUE` for external value
/// changes (§6).
fn motion() -> Motion {
    let eased = Transition::symmetric(Animation::ease_out(PRESS_DURATION));
    Motion {
        hover: eased.clone(),
        focus: eased,
        press: Transition::new(
            spring_animation(dynamics::Spring::BUTTON_PRESS),
            spring_animation(dynamics::Spring::BUTTON_RELEASE),
        ),
        drag: Transition::new(
            spring_animation(dynamics::Spring::JELLY_DRAG_ACTIVE),
            spring_animation(dynamics::Spring::JELLY_DRAG_RELEASE),
        ),
        value: ValueMotion::Own(spring_animation(dynamics::Spring::SLIDER_VALUE)),
    }
}

/// The press progress a state drives toward: 1 held, 0 otherwise.
fn press_target(state: InteractionState) -> f32 {
    f32::from(u8::from(held(state)))
}

/// The look of a slider in `spec`.
pub(in super::super) fn look(glass: &Glass, spec: &SliderSpec, env: &Environment) -> SliderLook {
    let layout = SliderLayout {
        row_height: HEIGHT,
        track_inset: TRAVEL_INSET,
        label_inset: 0.0,
        label_gap: LABEL_GAP,
        minimum_track: MIN_TRAVEL,
        label_spacing: Some(LABEL_SPACING),
    };
    // The touch target is the knob extended to 44 pt each way, and a drag
    // never cancels: it follows the touch wherever it goes (§8.3).
    let hit = HitRegion::NONE.minimum(Size::new(f64::from(MIN_HIT_SIDE), f64::from(MIN_HIT_SIDE)));
    let knob = Knob {
        size: Size::new(KNOB_WIDTH, KNOB_HEIGHT),
        hit,
    };
    // A touch drags only from the knob and moves the value by its offset
    // from where it began (§8.3, Touch).
    let drag = DragMapping::Relative { knob_only: true };
    // A disabled slider is the enabled one composited at opacity 0.5 in
    // encoded space (§8.3, States).
    let mut look = SliderLook::new(layout, knob, drag, motion())
        .group_opacity(Driven::new(|inputs: &Inputs<'_, ()>| {
            if is_disabled(inputs.state) {
                DISABLED_OPACITY
            } else {
                1.0
            }
        }))
        .group_blend_space(BlendSpace::SrgbEncoded)
        .part(track(glass, spec, env))
        .part(knob_lens(glass))
        .part(knob_fill())
        .part(focus_ring(glass));
    if spec.has_value_indicator() {
        look = look.value_indicator(value_indicator(glass));
    }
    look
}

/// The knob at the slider's value, grown about its centre by the press and
/// stretched horizontally with drag velocity.
fn knob_shape() -> Driven<ShapeData, f64> {
    Driven::new(|inputs: &Inputs<'_, f64>| {
        let rect = slider::knob_rect(
            inputs.frame,
            *inputs.value,
            f64::from(press_target(inputs.state)),
        );
        switch::capsule(stretch_along_axis(rect, inputs.velocity, false))
    })
}

/// The track, the part the value has filled and the knob's shadow, which are
/// one layer composited in encoded space (§8.3, Colours).
fn track(
    glass: &Glass,
    spec: &SliderSpec,
    env: &Environment,
) -> Custom<f64, impl DrivenSet<f64> + use<>> {
    let appearance = glass.palette.appearance;
    let tinted = Tint::inherited(env).is_some();
    let tint = spec.tint().resolve(env);
    let default_tint = working(glass.palette.accent);

    let whole =
        Driven::new(|inputs: &Inputs<'_, f64>| switch::capsule(slider::track_rect(inputs.frame)));
    let unfilled = Driven::<Paint, f64>::constant(paint(slider::unfilled_track(appearance)));
    let filled = Driven::new(|inputs: &Inputs<'_, f64>| {
        switch::capsule(slider::fill_rect(inputs.frame, *inputs.value))
    });
    let fill = Driven::follow(
        tint,
        move |tint: &WorkingColor, _inputs: &Inputs<'_, f64>| {
            Paint::from(if tinted { *tint } else { default_tint })
        },
    );
    // The shadow fades along smoothstep(0.05, 0.55) as the lens takes over.
    let shadow = Driven::new(|inputs: &Inputs<'_, f64>| {
        let flat = 1.0 - press_target(inputs.state);
        Shadow::new(SHADOW_SIGMA, working([0.0, 0.0, 0.0, SHADOW_ALPHA * flat]))
            .offset(Vec2::new(0.0, SHADOW_OFFSET))
    })
    .smoothstep_window(0.05, 0.55);
    Custom::new(
        (whole, unfilled, filled, fill, knob_shape(), shadow),
        |recorder, _frame, (whole, unfilled, filled, fill, knob, shadow)| {
            recorder.group(
                Group::new().blend_space(BlendSpace::SrgbEncoded),
                |recorder| {
                    recorder.fill(whole, unfilled);
                    recorder.fill(filled, fill);
                    recorder.shadow(knob, shadow);
                },
            );
        },
    )
}

/// The knob: the glass lens a touch makes of it, over which the white fill
/// is drawn at the opacity the lens leaves it (§8.3, Touch).
///
/// The lens is the switch's (§8.2): the clear family's recipe at the knob's
/// size, which both controls give the same numbers.
fn knob_lens(glass: &Glass) -> Custom<f64, impl DrivenSet<f64> + use<>> {
    let (appearance, translucency) = (glass.palette.appearance, glass.translucency);
    let capture =
        materials::capture_class(false, switch::lens_capture_scale(appearance, translucency));
    let effect = Driven::new(move |inputs: &Inputs<'_, f64>| -> MaterialEffect {
        switch::lens_effect(appearance, translucency, held(inputs.state))
    });
    Custom::new(
        (knob_shape(), effect),
        move |recorder, _frame, (shape, effect): (Live<ShapeData>, Live<MaterialEffect>)| {
            recorder.backdrop_material(shape, GLASS_SHADER, capture, effect);
        },
    )
    .opacity(
        Driven::new(|inputs: &Inputs<'_, f64>| press_target(inputs.state))
            .smoothstep_window(0.05, 0.55),
    )
}

/// The knob's white fill, which the lens clears along smoothstep(0.05, 0.55).
fn knob_fill() -> Custom<f64, impl DrivenSet<f64> + use<>> {
    let white = Driven::new(|inputs: &Inputs<'_, f64>| {
        paint(alpha(KNOB_FILL, 1.0 - press_target(inputs.state)))
    })
    .smoothstep_window(0.05, 0.55);
    Custom::new((knob_shape(), white), |recorder, _frame, (shape, white)| {
        recorder.group(Group::new().blend_space(BlendSpace::Linear), |recorder| {
            recorder.fill(shape, white);
        });
    })
}

/// The focus ring around the knob, visible while the slider has keyboard
/// focus. The specification leaves focus unobserved (§8.3, States); the ring
/// is the theme's, as every control's.
fn focus_ring(glass: &Glass) -> Custom<f64, impl DrivenSet<f64> + use<>> {
    let accent = glass.palette.accent;
    let reach = FOCUS_RING_WIDTH.mul_add(0.5, FOCUS_RING_GAP);
    let ring = Driven::new(move |inputs: &Inputs<'_, f64>| {
        let knob = stretch_along_axis(
            slider::knob_rect(
                inputs.frame,
                *inputs.value,
                f64::from(press_target(inputs.state)),
            ),
            inputs.velocity,
            false,
        );
        switch::capsule(knob.inflate(reach, reach))
    });
    Custom::new(
        (ring, focus_ring_paint(accent)),
        |recorder, _frame, (ring, ring_paint)| {
            recorder.stroke(ring, Stroke::new(FOCUS_RING_WIDTH), ring_paint);
        },
    )
}

/// The bubble a drag shows above the knob: the popup panel's surface, with
/// the foreground label in the footnote font.
fn value_indicator(glass: &Glass) -> ValueIndicator {
    let palette = glass.palette;
    let shadow = Driven::<Shadow, f64>::constant(
        Shadow::new(
            POPUP_SHADOW_BLUR,
            working([0.0, 0.0, 0.0, POPUP_SHADOW_ALPHA]),
        )
        .offset(Vec2::new(0.0, POPUP_SHADOW_OFFSET)),
    );
    let fill = Driven::<Paint, f64>::constant(paint(palette.surface));
    let outline = Driven::<Paint, f64>::constant(paint(palette.border));
    ValueIndicator::new(
        Font::from(Footnote),
        Glass::color(palette.foreground),
        Custom::<f64, _>::new(
            (shadow, fill, outline),
            |recorder, frame, (shadow, fill, outline)| {
                let radii = RoundedRectRadii::from_single_radius(MENU_CORNER_RADIUS);
                recorder.shadow(RoundedRect::from_rect(frame, radii), shadow);
                recorder.fill(RoundedRect::from_rect(frame, radii), fill);
                recorder.stroke(
                    RoundedRect::from_rect(frame.inset(-0.25), radii),
                    Stroke::new(HAIRLINE),
                    outline,
                );
            },
        ),
    )
    .padding(10.0, 6.0)
    .minimum_size(Size::new(36.0, 28.0))
    .gap(8.0)
}
