//! The glass stepper look (specification §8.5): a pair of buttons joined into
//! one capsule, each a track-coloured half with a state layer, focus ring and
//! a minus or plus icon.

use cherenkov_record::Draw as _;
use cherenkov_record::kurbo::{BezPath, Point, RoundedRectRadii, Size, Stroke};
use hydrolysis_style::theme::{
    Chrome, CornerShape, Corners, Custom, Driven, HitRegion, Inputs, StepperLook, StepperSpec,
    Surface as ChromeSurface,
};
use waterui_core::Environment;

use super::super::Glass;
use super::super::button::{CANCEL_DISTANCE, MIN_HIT_SIDE};
use super::{faded, is_disabled, paint, state_chrome};

/// The side of a stepper's button, pt (§8.5, Geometry).
const BUTTON: f64 = 32.0;
/// The gap between the two buttons, pt (§8.5, Geometry).
const SPACING: f64 = 1.0;
/// The gap between the stepper's label and its buttons, pt (§8.5, Geometry).
const LABEL_SPACING: f64 = 12.0;
/// An icon's half length, in shares of the button's shorter side (§8.5, Geometry).
const ICON_HALF: f64 = 0.22;
/// An icon stroke's width, pt (§8.5, Geometry).
const ICON_STROKE: f64 = 2.0;
/// Disabled opacity for an end's fill and icon (§8.5, States).
const DISABLED_OPACITY: f32 = 0.5;

/// Which end of the pair a button is.
#[derive(Clone, Copy, PartialEq, Eq)]
enum End {
    Decrement,
    Increment,
}

/// The look of a stepper in `spec` (§8.5).
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

/// One button of the pair: the half capsule, its hit region, its state layer
/// and focus ring, and its icon.
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
            if is_disabled(inputs.state) {
                DISABLED_OPACITY
            } else {
                1.0
            },
        ))
    });
    let icon_paint = Driven::new(move |inputs: &Inputs<'_, ()>| {
        paint(faded(
            palette.foreground,
            if is_disabled(inputs.state) {
                DISABLED_OPACITY
            } else {
                1.0
            },
        ))
    });
    let icon = Custom::<(), _>::new((icon_paint,), move |recorder, frame, (paint,)| {
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
    });
    // Keep each button's 32 pt horizontal bounds so the 1 pt seam never
    // overlaps targets, while extending vertically to 44 pt (§8.5, Geometry).
    let hit = HitRegion::NONE
        .minimum(Size::new(0.0, f64::from(MIN_HIT_SIDE)))
        .cancel_distance(f64::from(CANCEL_DISTANCE));
    state_chrome(glass, ChromeSurface::new(CornerShape::new(corners), fill))
        .hit(hit)
        .custom(icon)
}
