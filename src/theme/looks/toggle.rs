//! The glass switch (specification §8.2) and the checkbox looks.

use cherenkov_record::kurbo::{BezPath, Point, Rect, RoundedRect, RoundedRectRadii, Size, Stroke};
use cherenkov_record::{
    BlendSpace, Draw as _, Fixed, Group, Live, MaterialEffect, Paint, ShapeData, WorkingColor,
};
use hydrolysis::theme::{
    Backdrop, CheckboxLook, Custom, Driven, HitRegion, Inputs, Motion, SwitchDrag, SwitchLook,
    ToggleBackdrop, ToggleControl, ToggleSpec, Transition, ValueMotion,
};
use waterui::animation::Animation;
use waterui::interaction::InteractionState;
use waterui::theme::Tint;
use waterui_core::Environment;

use super::super::Glass;
use super::super::button::Spring;
use super::super::chrome::{FOCUS_RING_GAP, FOCUS_RING_WIDTH};
use super::super::materials::{self, GLASS_SHADER};
use super::super::palette::{alpha, working};
use super::super::switch::{
    self, DISABLED_OPACITY, FADE_OFF_OMEGA, FADE_ON_OMEGA, FRAME, KNOB_FILL, TOGGLE_SPRING, TRAVEL,
};
use super::{is_disabled, paint, state_layer_opacity};

/// The toggle motion of §8.2: the toggle spring moves a value and eases
/// hover, focus and drag; the press and release springs of §8.1 hold the
/// press.
fn motion() -> Motion {
    let toggle = Transition::symmetric(TOGGLE_SPRING.physical());
    Motion {
        hover: toggle.clone(),
        focus: toggle.clone(),
        press: Transition::new(Spring::PRESS.physical(), Spring::RELEASE.physical()),
        drag: toggle,
        value: ValueMotion::Own(TOGGLE_SPRING.physical()),
    }
}

/// A critically damped spring of natural frequency `omega`.
fn critical(omega: f32) -> Animation {
    Spring {
        omega0: omega,
        zeta: 1.0,
    }
    .physical()
}

/// The switch's touch target: 44 pt at least, a 10 pt slop for a waiting
/// touch and a 70 pt cancel distance (§8.2).
const fn switch_hit() -> HitRegion {
    HitRegion::NONE
        .slop(10.0)
        .cancel_distance(70.0)
        .minimum(Size::new(44.0, 44.0))
}

/// The look of a switch in `spec`: a flat track and knob at rest, the knob a
/// glass lens while a touch holds it, the whole switch at half opacity while
/// disabled.
pub(in super::super) fn switch(glass: &Glass, spec: &ToggleSpec, env: &Environment) -> SwitchLook {
    let direction = spec.direction();
    let drag = SwitchDrag::new(TRAVEL, direction);
    let (appearance, translucency) = (glass.palette.appearance, glass.translucency);
    let tinted = Tint::inherited(env).is_some();
    let tint = spec.tint().resolve(env);
    let default_tint = working(switch::default_tint(appearance));

    // The on fill is drawn over the off fill at the track fade's opacity,
    // which turns on by a slower spring than it turns off by.
    let (fade_in, fade_out) = (
        ValueMotion::Own(critical(FADE_ON_OMEGA)),
        ValueMotion::Own(critical(FADE_OFF_OMEGA)),
    );
    let on = Driven::follow(
        tint,
        move |tint: &WorkingColor, inputs: &Inputs<'_, bool>| {
            let tint = if tinted { *tint } else { default_tint };
            #[expect(
                clippy::cast_possible_truncation,
                reason = "the progress is within 0..=1"
            )]
            let progress = drag.progress(*inputs.value, inputs.drag) as f32;
            Paint::from(tint.with_alpha(tint.components[3] * progress))
        },
    )
    .value_motion(move |on: &bool| {
        if *on {
            fade_in.clone()
        } else {
            fade_out.clone()
        }
    });
    let off = Driven::<Paint, bool>::constant(paint(switch::off_fill(appearance)));
    let track = Custom::<bool, _>::new((off, on), move |recorder, frame, (off, on)| {
        let track = switch::capsule(switch::track_rect(frame, direction));
        recorder.group(
            Group::new().blend_space(BlendSpace::SrgbEncoded),
            |recorder| {
                recorder.fill(Fixed(track.clone()), off);
                recorder.fill(Fixed(track), on);
            },
        );
    });

    // The knob at rest on the track, growing about its centre while held.
    let knob = move || {
        Driven::new(move |inputs: &Inputs<'_, bool>| {
            let q = drag.progress(*inputs.value, inputs.drag);
            let held = f64::from(u8::from(inputs.state.contains(InteractionState::PRESSED)));
            let track = switch::track_rect(inputs.frame, direction);
            switch::capsule(switch::knob_rect(track, direction, q, held))
        })
    };
    let lens = Driven::new(move |inputs: &Inputs<'_, ()>| -> MaterialEffect {
        switch::lens_effect(
            appearance,
            translucency,
            inputs.state.contains(InteractionState::PRESSED),
        )
    });
    let backdrop = Backdrop::new(
        GLASS_SHADER,
        materials::capture_class(false, switch::lens_capture_scale(appearance, translucency)),
        lens,
    );
    // The knob's white is opaque at rest and clears as the lens takes over.
    let white = Driven::new(|inputs: &Inputs<'_, bool>| {
        let held = f32::from(u8::from(inputs.state.contains(InteractionState::PRESSED)));
        paint(alpha(KNOB_FILL, 1.0 - held))
    });
    let fill = Custom::<bool, _>::new(
        (knob(), white),
        |recorder, _frame, (shape, white): (Live<ShapeData>, Live<Paint>)| {
            recorder.group(
                Group::new().blend_space(BlendSpace::SrgbEncoded),
                |recorder| {
                    recorder.fill(shape, white);
                },
            );
        },
    );

    let control = ToggleControl::new(Size::new(FRAME[0], FRAME[1]), motion())
        .reach(3.0)
        .hit(switch_hit())
        .group_opacity(Driven::new(|inputs: &Inputs<'_, bool>| {
            if is_disabled(inputs.state) {
                DISABLED_OPACITY
            } else {
                1.0
            }
        }))
        .part(track)
        .backdrop(ToggleBackdrop::new(knob(), backdrop))
        .part(fill);
    SwitchLook::new(control, drag)
}

/// The checkbox's side, pt.
const CHECKBOX_SIDE: f64 = 22.0;
/// The gap between a checkbox and its label, pt.
const CHECKBOX_LABEL_SPACING: f64 = 10.0;
/// A checkbox's corner radius, in shares of its side.
const CHECKBOX_RADIUS: f64 = 0.3;
/// The check mark's stroke width, in shares of the checkbox's side.
const CHECK_STROKE: f64 = 0.12;

/// The look of a checkbox in `spec`: an outline, an accent fill that fades
/// in as the box checks with its check mark, and a state layer with a focus
/// ring.
pub(in super::super) fn checkbox(
    glass: &Glass,
    spec: &ToggleSpec,
    env: &Environment,
) -> CheckboxLook {
    let palette = glass.palette;
    let tint = spec.tint().resolve(env);
    let outline = Driven::new(move |inputs: &Inputs<'_, bool>| {
        paint(if is_disabled(inputs.state) {
            alpha(palette.border, palette.border[3] * 0.5)
        } else {
            palette.border
        })
    });
    let fill = Driven::follow(tint, |tint: &WorkingColor, inputs: &Inputs<'_, bool>| {
        let checked = f32::from(u8::from(*inputs.value));
        let dimmed = if is_disabled(inputs.state) { 0.5 } else { 1.0 };
        Paint::from(tint.with_alpha(tint.components[3] * checked * dimmed))
    });
    // The check mark fades in with the fill it sits on.
    let check = Driven::new(move |inputs: &Inputs<'_, bool>| {
        let checked = f32::from(u8::from(*inputs.value));
        paint(alpha(
            palette.accent_foreground,
            palette.accent_foreground[3] * checked,
        ))
    });
    let layer = Driven::new(move |inputs: &Inputs<'_, bool>| {
        paint(alpha(
            palette.state_layer,
            state_layer_opacity(inputs.state),
        ))
    });
    let ring = Driven::new(move |inputs: &Inputs<'_, bool>| {
        let focused =
            inputs.state.contains(InteractionState::FOCUSED) && !is_disabled(inputs.state);
        paint(alpha(palette.accent, f32::from(u8::from(focused))))
    });
    let body = Custom::<bool, _>::new(
        (outline, fill, check, layer, ring),
        |recorder, frame, (outline, fill, check, layer, ring)| {
            let radii = RoundedRectRadii::from_single_radius(
                frame.width().min(frame.height()) * CHECKBOX_RADIUS,
            );
            let rounded = |rect: Rect, grow: f64| {
                RoundedRect::from_rect(
                    rect,
                    RoundedRectRadii::new(
                        radii.top_left + grow,
                        radii.top_right + grow,
                        radii.bottom_right + grow,
                        radii.bottom_left + grow,
                    ),
                )
            };
            recorder.stroke(rounded(frame.inset(-0.75), 0.0), Stroke::new(1.5), outline);
            recorder.fill(rounded(frame, 0.0), fill);
            let (w, h) = (frame.width(), frame.height());
            let at = |x: f64, y: f64| Point::new(x.mul_add(w, frame.x0), y.mul_add(h, frame.y0));
            let mut mark = BezPath::new();
            mark.move_to(at(0.22, 0.52));
            mark.line_to(at(0.42, 0.72));
            mark.line_to(at(0.78, 0.30));
            recorder.stroke(mark, Stroke::new(w * CHECK_STROKE), check);
            recorder.fill(rounded(frame, 0.0), layer);
            let ring_inset = FOCUS_RING_WIDTH.mul_add(0.5, FOCUS_RING_GAP);
            recorder.stroke(
                rounded(frame.inset(-ring_inset), ring_inset),
                Stroke::new(FOCUS_RING_WIDTH),
                ring,
            );
        },
    );
    CheckboxLook::new(
        ToggleControl::new(Size::new(CHECKBOX_SIDE, CHECKBOX_SIDE), motion())
            .label_spacing(CHECKBOX_LABEL_SPACING)
            .hit(switch_hit())
            .part(body),
    )
}
