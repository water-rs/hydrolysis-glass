//! The glass progress looks: a capsule bar, a ring and a rounded square
//! that morphs as it turns.
//!
//! A determinate indicator's parts are functions of its value. An
//! indeterminate indicator's motion is keyframes the engine samples on its
//! own clock, so it records once and does no work per frame.

use core::f64::consts::TAU;
use core::time::Duration;

use cherenkov_record::kurbo::{
    Affine, Arc, BezPath, Rect, RoundedRect, RoundedRectRadii, Shape as _, Size, Stroke,
};
use cherenkov_record::{
    Curve, Draw as _, Fixed, Keyframes, Paint, Repeat, ShapeData, WorkingColor,
};
use hydrolysis::theme::{
    Custom, Driven, DrivenSet, IndicatorLook, Inputs, LinearProgressLook, Motion, ProgressControl,
    ProgressSpec, ValueMotion,
};
use waterui::animation::Animation;
use waterui_core::Environment;

use super::super::palette::Rgba;
use super::super::{
    CIRCULAR_INDETERMINATE_CYCLE, Glass, LINEAR_INDETERMINATE_CYCLE, LOADING_CYCLE,
};
use super::paint;

/// The bar's length and thickness, pt.
const BAR: Size = Size::new(120.0, 4.0);
/// The height of the label above a bar, pt.
const LABEL_HEIGHT: f64 = 20.0;
/// The gap between a bar and its label above and its value label below, pt.
const BAR_SPACING: f64 = 6.0;
/// The ring's diameter and the stroke's width, pt.
const RING: f64 = 28.0;
const RING_STROKE: f64 = 3.0;
/// The loading indicator's container and the rounded square inside it, pt.
const LOADING_CONTAINER: f64 = 48.0;
const LOADING_SHAPE: f64 = 28.0;
/// How far a determinate value takes to arrive.
const DETERMINATE: Duration = Duration::from_millis(250);
/// The indeterminate bar's segment, in shares of the bar.
const SEGMENT: f64 = 0.35;
/// The indeterminate ring's arc, in shares of the circle, shortest and
/// longest.
const ARC: (f64, f64) = (0.15, 0.7);
/// The loading square's corner radius, in shares of its side, tightest and
/// roundest.
const MORPH: (f64, f64) = (0.25, 0.5);
/// How many times the loading square morphs and back in one turn.
const MORPHS_PER_TURN: u32 = 3;
/// The ring turns this many times in one indeterminate cycle.
const RING_TURNS: f64 = 2.0;
/// The sine ease: the cubic Bézier closest to half a cosine.
const SINE: (f64, f64, f64, f64) = (0.37, 0.0, 0.63, 1.0);

/// The motion of a progress indicator: its value eases in over
/// [`DETERMINATE`]. Nothing interacts with it.
fn motion() -> Motion {
    Motion {
        value: ValueMotion::Own(Animation::ease_in_out(DETERMINATE)),
        ..Motion::uniform(Animation::ease_in_out(DETERMINATE))
    }
}

/// `color` as a paint.
fn solid(color: Rgba) -> Driven<Paint, f64> {
    Driven::constant(paint(color))
}

/// The tint as a paint that follows it.
fn tint(spec: &ProgressSpec, env: &Environment) -> Driven<Paint, f64> {
    Driven::follow(
        spec.tint().resolve(env),
        |tint: &WorkingColor, _: &Inputs<'_, f64>| Paint::from(*tint),
    )
}

/// The look of a linear progress indicator.
pub(in super::super) fn linear(
    glass: &Glass,
    spec: &ProgressSpec,
    env: &Environment,
) -> LinearProgressLook {
    let rtl = spec.direction().is_right_to_left();
    let control = ProgressControl::new(BAR, motion());
    let control = if spec.determinate() {
        control.part(linear_determinate(glass, spec, env, rtl))
    } else {
        control.part(linear_indeterminate(glass, spec, env, rtl))
    };
    LinearProgressLook::new(control, 0.0)
        .label_band(LABEL_HEIGHT, BAR_SPACING)
        .value_label_band(BAR_SPACING)
}

/// The look of a circular progress indicator.
pub(in super::super) fn circular(
    glass: &Glass,
    spec: &ProgressSpec,
    env: &Environment,
) -> IndicatorLook {
    let control = ProgressControl::new(Size::new(RING, RING), motion());
    IndicatorLook::new(if spec.determinate() {
        control.part(circular_determinate(glass, spec, env))
    } else {
        control.part(circular_indeterminate(spec, env))
    })
}

/// The look of the loading indicator.
pub(in super::super) fn loading(
    _glass: &Glass,
    spec: &ProgressSpec,
    env: &Environment,
) -> IndicatorLook {
    IndicatorLook::new(
        ProgressControl::new(Size::new(LOADING_CONTAINER, LOADING_CONTAINER), motion())
            .part(loading_part(spec, env)),
    )
}

/// A capsule `[x0, x1]` wide across `frame`'s height, mirrored for a
/// right-to-left indicator. Its radius is half the bar's height whatever
/// its width.
fn bar(frame: Rect, x0: f64, x1: f64, rtl: bool) -> ShapeData {
    let (x0, x1) = if rtl {
        (frame.x0 + frame.x1 - x1, frame.x0 + frame.x1 - x0)
    } else {
        (x0, x1)
    };
    ShapeData::of(&RoundedRect::from_rect(
        Rect::new(x0, frame.y0, x1, frame.y1),
        RoundedRectRadii::from_single_radius(frame.height() / 2.0),
    ))
}

fn linear_determinate(
    glass: &Glass,
    spec: &ProgressSpec,
    env: &Environment,
    rtl: bool,
) -> Custom<f64, impl DrivenSet<f64> + use<>> {
    // The track runs from where the fill ends to the bar's end.
    let track = Driven::new(move |inputs: &Inputs<'_, f64>| {
        let end = inputs.frame.width().mul_add(*inputs.value, inputs.frame.x0);
        bar(inputs.frame, end, inputs.frame.x1, rtl)
    });
    let fill = Driven::new(move |inputs: &Inputs<'_, f64>| {
        let end = inputs.frame.width().mul_add(*inputs.value, inputs.frame.x0);
        bar(inputs.frame, inputs.frame.x0, end, rtl)
    });
    Custom::new(
        (track, solid(glass.palette.track), fill, tint(spec, env)),
        |recorder, _frame, (track, track_paint, fill, fill_paint)| {
            recorder.fill(track, track_paint);
            recorder.fill(fill, fill_paint);
        },
    )
}

/// The segment sweeping across `frame`: it enters at the leading end,
/// crosses and leaves at the trailing one, clipped to the bar. Its edges
/// move linearly except where one meets the bar's end, so four keyframes
/// hold the whole sweep.
fn sweep(frame: Rect, cycle: Duration) -> Keyframes<ShapeData> {
    let width = frame.width();
    let segment = width * SEGMENT;
    let at = |phase: f64| {
        let x0 = (width + segment).mul_add(phase, frame.x0 - segment);
        bar(frame, x0.max(frame.x0), (x0 + segment).min(frame.x1), false)
    };
    let linear = Curve::linear(cycle);
    let enters = segment / (width + segment);
    let leaves = width / (width + segment);
    Keyframes::new(cycle, at(0.0))
        .then(enters, linear, at(enters))
        .then(leaves, linear, at(leaves))
        .then(1.0, linear, at(1.0))
}

fn linear_indeterminate(
    glass: &Glass,
    spec: &ProgressSpec,
    env: &Environment,
    rtl: bool,
) -> Custom<f64, impl DrivenSet<f64> + use<>> {
    Custom::new(
        (
            solid(glass.palette.track),
            Driven::repeating(
                |frame| sweep(frame, LINEAR_INDETERMINATE_CYCLE),
                Repeat::Forever,
            ),
            tint(spec, env),
        ),
        move |recorder, frame, (track_paint, segment, segment_paint)| {
            let draw = |recorder: &mut cherenkov_record::Recorder| {
                recorder.fill(Fixed(bar(frame, frame.x0, frame.x1, false)), track_paint);
                recorder.fill(segment, segment_paint);
            };
            if rtl {
                let mirror = Affine::translate((frame.x0 + frame.x1, 0.0))
                    * Affine::scale_non_uniform(-1.0, 1.0);
                recorder.transform(Fixed(mirror), draw);
            } else {
                draw(recorder);
            }
        },
    )
}

/// The ring a circular indicator strokes in `frame`, from `from` radians
/// clockwise, and its circumference.
fn ring(frame: Rect, from: f64) -> (BezPath, f64) {
    let radius = (frame.width().min(frame.height()) - RING_STROKE).max(0.0) / 2.0;
    (
        Arc::new(frame.center(), (radius, radius), from, TAU, 0.0).into_path(0.1),
        TAU * radius,
    )
}

/// The arc of the ring's circumference `share` of it long, stroked by one
/// dash and the rest of the ring as gap.
fn arc_stroke(circumference: f64, share: f64) -> Stroke {
    let arc = share * circumference;
    Stroke::new(RING_STROKE).with_dashes(0.0, [arc, circumference - arc])
}

/// The ring minus its first `value` of it: one dash of the rest, shifted so
/// that it begins where the arc ends.
fn track_stroke(circumference: f64, value: f64) -> Stroke {
    let active = value * circumference;
    let dash = (circumference - active).max(0.0);
    let period = dash + circumference;
    Stroke::new(RING_STROKE)
        .with_dashes((period - active).rem_euclid(period), [dash, circumference])
}

fn circular_determinate(
    glass: &Glass,
    spec: &ProgressSpec,
    env: &Environment,
) -> Custom<f64, impl DrivenSet<f64> + use<>> {
    let start = -core::f64::consts::FRAC_PI_2;
    Custom::new(
        (
            Driven::new(move |inputs: &Inputs<'_, f64>| {
                track_stroke(ring(inputs.frame, start).1, *inputs.value)
            }),
            solid(glass.palette.track),
            Driven::new(move |inputs: &Inputs<'_, f64>| {
                arc_stroke(ring(inputs.frame, start).1, *inputs.value)
            }),
            tint(spec, env),
        ),
        move |recorder, frame, (track, track_paint, active, active_paint)| {
            let (path, _) = ring(frame, start);
            recorder.stroke(path.clone(), track, track_paint);
            recorder.stroke(path, active, active_paint);
        },
    )
}

/// The arc's length over a cycle: from [`ARC`]'s shortest to its longest at
/// mid-cycle and back, along the sine ease.
fn arc_keyframes(circumference: f64) -> Keyframes<Stroke> {
    let cycle = CIRCULAR_INDETERMINATE_CYCLE;
    let ease = Curve::bezier(cycle, SINE.0, SINE.1, SINE.2, SINE.3);
    Keyframes::new(cycle, arc_stroke(circumference, ARC.0))
        .then(0.5, ease, arc_stroke(circumference, ARC.1))
        .then(1.0, ease, arc_stroke(circumference, ARC.0))
}

fn circular_indeterminate(
    spec: &ProgressSpec,
    env: &Environment,
) -> Custom<f64, impl DrivenSet<f64> + use<>> {
    let cycle = CIRCULAR_INDETERMINATE_CYCLE;
    Custom::new(
        (
            Driven::repeating(|frame| arc_keyframes(ring(frame, 0.0).1), Repeat::Forever),
            tint(spec, env),
        ),
        |recorder, frame, (arc, paint)| {
            recorder.stroke(ring(frame, 0.0).0, arc, paint);
        },
    )
    // The whole ring turns as one layer, so the arc keeps its place in the
    // layer and only its length is keyframed.
    .rotating(
        Keyframes::new(cycle, 0.0).then(1.0, Curve::linear(cycle), TAU * RING_TURNS),
        Repeat::Forever,
    )
}

/// The rounded square at `radius` shares of its side, centred in `frame`.
fn square(frame: Rect, share: f64) -> ShapeData {
    ShapeData::of(&RoundedRect::from_rect(
        Rect::from_center_size(frame.center(), (LOADING_SHAPE, LOADING_SHAPE)),
        RoundedRectRadii::from_single_radius(LOADING_SHAPE * share),
    ))
}

/// The square's corners over a turn: tightest to roundest and back
/// [`MORPHS_PER_TURN`] times, each half along the sine ease.
fn morph(frame: Rect) -> Keyframes<ShapeData> {
    let cycle = LOADING_CYCLE;
    let ease = Curve::bezier(cycle, SINE.0, SINE.1, SINE.2, SINE.3);
    let halves = MORPHS_PER_TURN * 2;
    (1..=halves).fold(
        Keyframes::new(cycle, square(frame, MORPH.0)),
        |keyframes, half| {
            let share = if half % 2 == 1 { MORPH.1 } else { MORPH.0 };
            keyframes.then(
                f64::from(half) / f64::from(halves),
                ease,
                square(frame, share),
            )
        },
    )
}

fn loading_part(
    spec: &ProgressSpec,
    env: &Environment,
) -> Custom<f64, impl DrivenSet<f64> + use<>> {
    Custom::new(
        (Driven::repeating(morph, Repeat::Forever), tint(spec, env)),
        |recorder, _frame, (shape, paint)| {
            recorder.fill(shape, paint);
        },
    )
    .rotating(
        Keyframes::new(LOADING_CYCLE, 0.0).then(1.0, Curve::linear(LOADING_CYCLE), TAU),
        Repeat::Forever,
    )
}
