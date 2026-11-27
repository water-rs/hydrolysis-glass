//! The glass stepper look (specification §8.5): a pair of buttons joined into
//! one capsule, each a track-coloured half with a state layer, focus ring and
//! a minus or plus icon.

use cherenkov_record::Draw as _;
use cherenkov_record::kurbo::{BezPath, Cap, Point, Rect, RoundedRectRadii, Size, Stroke};
use hydrolysis_style::theme::{
    Chrome, CornerShape, Corners, Custom, Driven, HitRegion, Inputs, StepperLook, StepperSpec,
    Surface as ChromeSurface,
};
use waterui_core::Environment;

use crate::material::recipe::Appearance;

use super::super::Glass;
use super::super::button::{CANCEL_DISTANCE, MIN_HIT_SIDE};
use super::super::materials::CONTINUOUS_SMOOTHING;
use super::super::palette::Rgba;
use super::{faded, is_disabled, paint, state_chrome};

/// Track fill yielding Light `(238, 238, 239)` over `#FFFFFF` and Dark
/// `(18, 18, 19)` over `#000000` in the linear compositing canvas (§8.5).
const fn end_fill(appearance: Appearance) -> Rgba {
    match appearance {
        Appearance::Dark => [118.0 / 255.0, 118.0 / 255.0, 128.0 / 255.0, 0.0338],
        Appearance::Light => [118.0 / 255.0, 118.0 / 255.0, 128.0 / 255.0, 0.1771],
    }
}

/// Central vertical hairline divider colour between the two halves (§8.5).
const fn divider_color(appearance: Appearance) -> Rgba {
    match appearance {
        Appearance::Dark => [83.0 / 255.0, 83.0 / 255.0, 86.0 / 255.0, 1.0],
        Appearance::Light => [185.0 / 255.0, 185.0 / 255.0, 187.0 / 255.0, 1.0],
    }
}

/// Width of each stepper half (`46.0` pt end + `0.5` pt divider half = `93.0` pt
/// total pill with zero background-coloured gap, §8.5, Geometry).
const BUTTON_WIDTH: f64 = 46.5;
/// Height of each stepper end, pt (§8.5, Geometry).
const BUTTON_HEIGHT: f64 = 32.0;
/// Gap between the two halves (`0.0` so the seam never opens a background gap, §8.5).
const SPACING: f32 = 0.0;
/// Width of the central vertical divider drawn across the seam (split `0.5` pt per half), pt (§8.5).
const DIVIDER_HALF_WIDTH: f64 = 0.5;
/// Top and bottom inset of the `24.0` pt central divider inside the `32.0` pt pill, pt (§8.5).
const DIVIDER_VERTICAL_INSET: f64 = 4.0;
/// The gap between the stepper's label and its buttons, pt (§8.5, Geometry).
const LABEL_SPACING: f32 = 12.0;
/// An icon's half length, in shares of the pill height (`6.5 / 32.0`, `13.0` pt total span, §8.5).
const ICON_HALF: f64 = 6.5 / BUTTON_HEIGHT;
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
        Size::new(BUTTON_WIDTH, BUTTON_HEIGHT),
    )
    .spacing(SPACING)
    .label_spacing(LABEL_SPACING)
}

/// One button of the pair: the continuous half capsule, its hit region, its
/// state layer and focus ring, its half of the central divider, and its icon.
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
    let base_fill = end_fill(palette.appearance);
    let fill = Driven::new(move |inputs: &Inputs<'_, ()>| {
        paint(faded(
            base_fill,
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
    let base_divider = divider_color(palette.appearance);
    let divider_paint = Driven::new(move |inputs: &Inputs<'_, ()>| {
        paint(faded(
            base_divider,
            if is_disabled(inputs.state) {
                DISABLED_OPACITY
            } else {
                1.0
            },
        ))
    });
    let icon = Custom::<(), _>::new(
        (icon_paint, divider_paint),
        move |recorder, frame, (icon_paint, divider_paint)| {
            let seam_rect = if leading {
                Rect::new(
                    frame.x1 - DIVIDER_HALF_WIDTH,
                    frame.y0 + DIVIDER_VERTICAL_INSET,
                    frame.x1,
                    frame.y1 - DIVIDER_VERTICAL_INSET,
                )
            } else {
                Rect::new(
                    frame.x0,
                    frame.y0 + DIVIDER_VERTICAL_INSET,
                    frame.x0 + DIVIDER_HALF_WIDTH,
                    frame.y1 - DIVIDER_VERTICAL_INSET,
                )
            };
            if seam_rect.height() > 0.0 {
                recorder.fill(seam_rect, divider_paint);
            }
            let c = frame.center();
            let half = frame.height() * ICON_HALF;
            let mut path = BezPath::new();
            path.move_to(Point::new(c.x - half, c.y));
            path.line_to(Point::new(c.x + half, c.y));
            if end == End::Increment {
                path.move_to(Point::new(c.x, c.y - half));
                path.line_to(Point::new(c.x, c.y + half));
            }
            recorder.stroke(
                path,
                Stroke::new(ICON_STROKE).with_caps(Cap::Round),
                icon_paint,
            );
        },
    );
    // Keep each button's horizontal bounds so the central divider never
    // overlaps targets, while extending vertically to 44 pt (§8.5, Geometry).
    let hit = HitRegion::NONE
        .minimum(Size::new(0.0, f64::from(MIN_HIT_SIDE)))
        .cancel_distance(CANCEL_DISTANCE);
    state_chrome(
        glass,
        ChromeSurface::new(
            CornerShape::new(corners).smoothing(CONTINUOUS_SMOOTHING),
            fill,
        ),
    )
    .hit(hit)
    .custom(icon)
}
