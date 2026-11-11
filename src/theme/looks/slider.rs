//! The glass slider look: a capsule track filled to the value, a white
//! thumb with a shadow, its state layer and focus ring, and the value
//! bubble a drag shows.

use cherenkov_record::kurbo::{
    Circle, Point, Rect, RoundedRect, RoundedRectRadii, Size, Stroke, Vec2,
};
use cherenkov_record::{Draw as _, Paint, Shadow, ShapeData, WorkingColor};
use hydrolysis::theme::{
    Custom, DragMapping, Driven, DrivenSet, Inputs, Knob, SliderLayout, SliderLook, SliderSpec,
    ValueIndicator,
};
use waterui::interaction::InteractionState;
use waterui::text::font::{Font, Footnote};
use waterui_core::Environment;

use super::super::Glass;
use super::super::chrome::{
    FOCUS_RING_GAP, FOCUS_RING_WIDTH, HAIRLINE, MENU_CORNER_RADIUS, POPUP_SHADOW_ALPHA,
    POPUP_SHADOW_BLUR, POPUP_SHADOW_OFFSET,
};
use super::super::palette::{alpha, over, working};
use super::{control_motion, faded, hit_region, is_disabled, paint, state_layer_opacity};

/// The track's thickness, pt.
const TRACK_HEIGHT: f64 = 4.0;
/// The thumb's diameter, pt.
const THUMB: f64 = 28.0;
/// How much larger than the thumb its state layer is.
const STATE_LAYER_SCALE: f64 = 1.4;

/// The look of a slider in `spec`.
pub(in super::super) fn look(glass: &Glass, spec: &SliderSpec, env: &Environment) -> SliderLook {
    let rtl = spec.direction().is_right_to_left();
    let layout = SliderLayout {
        row_height: THUMB,
        track_inset: 0.0,
        label_inset: 0.0,
        label_gap: 8.0,
        minimum_track: 120.0,
        label_spacing: Some(8.0),
    };
    let knob = Knob {
        size: Size::new(THUMB, THUMB),
        hit: hit_region(),
    };
    let mut look = SliderLook::new(layout, knob, DragMapping::Absolute, control_motion())
        .part(track(glass, spec, env, rtl))
        .part(thumb(glass, rtl))
        .part(state_layer(glass, rtl));
    if spec.has_value_indicator() {
        look = look.value_indicator(value_indicator(glass));
    }
    look
}

/// The x of the normalized `value` across `frame`: from the leading edge.
fn x_at(frame: Rect, value: f64, rtl: bool) -> f64 {
    let travelled = value.mul_add(frame.width(), 0.0);
    if rtl {
        frame.x1 - travelled
    } else {
        frame.x0 + travelled
    }
}

/// The thumb's centre for the normalized `value` on `frame`.
fn thumb_center(frame: Rect, value: f64, rtl: bool) -> Point {
    Point::new(x_at(frame, value, rtl), frame.center().y)
}

/// A capsule `[x0, x1]` wide and the track's thick, centred on `frame`.
fn capsule(frame: Rect, x0: f64, x1: f64) -> ShapeData {
    let cy = frame.center().y;
    let rect = Rect::new(
        x0.min(x1),
        cy - TRACK_HEIGHT / 2.0,
        x0.max(x1),
        cy + TRACK_HEIGHT / 2.0,
    );
    ShapeData::of(&RoundedRect::from_rect(
        rect,
        RoundedRectRadii::from_single_radius(TRACK_HEIGHT / 2.0),
    ))
}

/// The track and the part of it the value has filled, which is the tint.
fn track(
    glass: &Glass,
    spec: &SliderSpec,
    env: &Environment,
    rtl: bool,
) -> Custom<f64, impl DrivenSet<f64> + use<>> {
    let palette = glass.palette;
    let dim = |state: InteractionState| if is_disabled(state) { 0.5 } else { 1.0 };
    let whole = Driven::new(move |inputs: &Inputs<'_, f64>| {
        capsule(inputs.frame, inputs.frame.x0, inputs.frame.x1)
    });
    let rest =
        Driven::new(move |inputs: &Inputs<'_, f64>| paint(faded(palette.track, dim(inputs.state))));
    let filled = Driven::new(move |inputs: &Inputs<'_, f64>| {
        let end = x_at(inputs.frame, *inputs.value, rtl);
        let start = if rtl {
            inputs.frame.x1
        } else {
            inputs.frame.x0
        };
        capsule(inputs.frame, start, end)
    });
    let accent = Driven::follow(
        spec.tint().resolve(env),
        move |tint: &WorkingColor, inputs: &Inputs<'_, f64>| {
            Paint::from(tint.with_alpha(tint.components[3] * dim(inputs.state)))
        },
    );
    Custom::new(
        (whole, rest, filled, accent),
        |recorder, _frame, (whole, rest, filled, accent)| {
            recorder.fill(whole, rest);
            recorder.fill(filled, accent);
        },
    )
}

/// The thumb: a shadow, then the knob's white, dimmed over grey while
/// disabled.
fn thumb(glass: &Glass, rtl: bool) -> Custom<f64, impl DrivenSet<f64> + use<>> {
    let palette = glass.palette;
    let shape = Driven::new(move |inputs: &Inputs<'_, f64>| {
        Circle::new(thumb_center(inputs.frame, *inputs.value, rtl), THUMB / 2.0)
    });
    let shadow = Driven::<Shadow, f64>::constant(
        Shadow::new(6.0, working([0.0, 0.0, 0.0, 0.24])).offset(Vec2::new(0.0, 2.0)),
    );
    let fill = Driven::new(move |inputs: &Inputs<'_, f64>| {
        paint(if is_disabled(inputs.state) {
            over([0.5, 0.5, 0.5, 0.3], palette.knob)
        } else {
            palette.knob
        })
    });
    Custom::new(
        (shape, shadow, fill),
        |recorder, _frame, (shape, shadow, fill)| {
            let shape = shape.into_shared();
            recorder.shadow(shape.clone(), shadow);
            recorder.fill(shape, fill);
        },
    )
}

/// The state layer over the thumb and its focus ring.
fn state_layer(glass: &Glass, rtl: bool) -> Custom<f64, impl DrivenSet<f64> + use<>> {
    let palette = glass.palette;
    let radius = THUMB / 2.0 * STATE_LAYER_SCALE;
    let halo = Driven::new(move |inputs: &Inputs<'_, f64>| {
        Circle::new(thumb_center(inputs.frame, *inputs.value, rtl), radius)
    });
    let layer = Driven::new(move |inputs: &Inputs<'_, f64>| {
        paint(alpha(
            palette.state_layer,
            state_layer_opacity(inputs.state),
        ))
    });
    let ring_radius = FOCUS_RING_WIDTH.mul_add(0.5, radius + FOCUS_RING_GAP);
    let ring = Driven::new(move |inputs: &Inputs<'_, f64>| {
        Circle::new(thumb_center(inputs.frame, *inputs.value, rtl), ring_radius)
    });
    let ring_paint = Driven::new(move |inputs: &Inputs<'_, f64>| {
        let focused =
            inputs.state.contains(InteractionState::FOCUSED) && !is_disabled(inputs.state);
        paint(alpha(palette.accent, f32::from(u8::from(focused))))
    });
    Custom::new(
        (halo, layer, ring, ring_paint),
        |recorder, _frame, (halo, layer, ring, ring_paint)| {
            recorder.fill(halo, layer);
            recorder.stroke(ring, Stroke::new(FOCUS_RING_WIDTH), ring_paint);
        },
    )
}

/// The bubble a drag shows above the thumb: the popup panel's surface, with
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
