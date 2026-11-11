//! The glass stepper look: a pair of buttons joined into one capsule, each
//! a track-coloured half with a state layer and a minus or plus icon.

use cherenkov_record::kurbo::{BezPath, Point, RoundedRectRadii, Size, Stroke};
use cherenkov_record::{Draw as _, Paint};
use hydrolysis::theme::{
    Chrome, CornerShape, Corners, Custom, Driven, FocusIndicator, Inputs, PressFeedback,
    StateLayer, StepperLook, StepperSpec, Surface as ChromeSurface,
};
use waterui_core::Environment;

use super::super::Glass;
use super::super::chrome::{FOCUS_RING_GAP, FOCUS_RING_WIDTH, HOVER_ALPHA, PRESSED_ALPHA};
use super::super::palette::alpha;
use super::{control_motion, faded, is_disabled, paint};

/// The side of a stepper's button, pt.
const BUTTON: f64 = 32.0;
/// The gap between the two buttons, pt.
const SPACING: f64 = 1.0;
/// The gap between the stepper's label and its buttons, pt.
const LABEL_SPACING: f64 = 12.0;
/// An icon's half length, in shares of the button's shorter side.
const ICON_HALF: f64 = 0.22;
/// An icon stroke's width, pt.
const ICON_STROKE: f64 = 2.0;

/// Which end of the pair a button is.
#[derive(Clone, Copy, PartialEq, Eq)]
enum End {
    Decrement,
    Increment,
}

/// The look of a stepper in `spec`.
pub(in super::super) fn look(glass: &Glass, spec: &StepperSpec, _env: &Environment) -> StepperLook {
    // The decrement end leads the pair, so its outer corners are the
    // leading ones.
    let rtl = spec.direction().is_right_to_left();
    StepperLook::new(
        end(glass, End::Decrement, rtl),
        end(glass, End::Increment, rtl),
        Size::new(BUTTON, BUTTON),
    )
    .spacing(SPACING)
    .label_spacing(LABEL_SPACING)
}

/// One button of the pair: the half capsule, its state layer and focus
/// ring, and its icon.
fn end(glass: &Glass, end: End, rtl: bool) -> Chrome {
    let palette = glass.palette;
    // A half capsule rounds its outer corners fully and tucks the corners
    // at the seam in.
    let leading = (end == End::Decrement) != rtl;
    let corners = Driven::new(move |inputs: &Inputs<'_, ()>| {
        let r = inputs.frame.height() * 0.5;
        Corners::Radii(if leading {
            RoundedRectRadii::new(r, 0.0, 0.0, r)
        } else {
            RoundedRectRadii::new(0.0, r, r, 0.0)
        })
    });
    let fill = Driven::new(move |inputs: &Inputs<'_, ()>| {
        paint(faded(
            palette.track,
            if is_disabled(inputs.state) { 0.5 } else { 1.0 },
        ))
    });
    let icon = Custom::<(), _>::new(
        (Driven::<Paint, ()>::constant(paint(palette.foreground)),),
        move |recorder, frame, (paint,)| {
            let c = frame.center();
            let half = frame.width().min(frame.height()) * ICON_HALF;
            let mut path = BezPath::new();
            path.move_to(Point::new(c.x - half, c.y));
            path.line_to(Point::new(c.x + half, c.y));
            if end == End::Increment {
                path.move_to(Point::new(c.x, c.y - half));
                path.line_to(Point::new(c.x, c.y + half));
            }
            recorder.stroke(path, Stroke::new(ICON_STROKE), paint);
        },
    );
    Chrome::new(
        ChromeSurface::new(CornerShape::new(corners), fill),
        StateLayer::new(super::super::Glass::color(palette.state_layer)).opacities(
            HOVER_ALPHA,
            HOVER_ALPHA,
            PRESSED_ALPHA,
            PRESSED_ALPHA,
        ),
        PressFeedback::new().tint(),
        FocusIndicator::Ring {
            width: FOCUS_RING_WIDTH,
            offset: FOCUS_RING_GAP,
            color: super::super::Glass::color(alpha(palette.accent, 1.0)),
        },
        control_motion(),
    )
    .custom(icon)
}
