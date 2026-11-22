//! The text field look (specification §8.6): a bordered `surface`-filled
//! rounded rectangle under the theme's state layer, with an accent outline just
//! inside its edge and an outer focus ring while focused, an inside label that
//! floats to the top at three quarters of its size, and an accent caret and
//! selection.
//!
//! [`menu_picker`](super::menu_picker) reuses the soft `10` pt track container
//! helper [`container`] from here.

use core::time::Duration;

use cherenkov_record::Draw as _;
use cherenkov_record::kurbo::{RoundedRect, Stroke};
use hydrolysis_style::theme::{
    Border, Caret, Chrome, CornerShape, Custom, Driven, FloatingLabel, InputFieldMetrics, Inputs,
    Label, Surface as ChromeSurface, TextCaretMotion, TextFieldLook, TextFieldSpec, ValueMotion,
};
use waterui::animation::Animation;
use waterui::color::Color;
use waterui::text::font::{Body, Font, FontWeight};
use waterui_core::Environment;

use super::super::Glass;
use super::super::PRESS_DURATION;
use super::super::palette::{Rgba, alpha};
use super::{faded, focus_ring_paint, is_disabled, paint, state_chrome};

/// The field's measures: a 20 pt band for the floated label, at least
/// 120 × 36 pt, its text inset 12 by 8 (§8.6, Geometry).
const METRICS: InputFieldMetrics = InputFieldMetrics::new(20.0, 120.0, 36.0, 12.0, 8.0);
/// The text field's resting corner radius, pt.
const FIELD_CORNER_RADIUS: f64 = 6.0;
/// The picker container's corner radius, pt (§8.6, Geometry).
const CORNER_RADIUS: f64 = 10.0;
/// The resting text size inside a text field, pt.
const FIELD_TEXT_SIZE: f32 = 13.0;
/// The resting line height inside a text field, pt.
const FIELD_LINE_HEIGHT: f32 = 16.0;
/// The width of the accent outline a focused field draws inside its
/// container edge, pt (§8.6, Geometry).
const OUTLINE_WIDTH: f64 = 1.5;
/// The floated label's size relative to its resting size (§8.6, Geometry).
const FLOAT_SCALE: f64 = 0.75;
/// The floated label's distance below the container's top edge, pt (§8.6, Geometry).
const FLOAT_TOP: f64 = 4.0;
/// The alpha of the accent the selection is painted in (§8.6, Colours).
const SELECTION_ALPHA: f32 = 0.3;
/// Disabled opacity applied to fill, label, value and placeholder (§8.6, States).
pub(super) const DISABLED_OPACITY: f32 = 0.5;
/// The caret's blink: a full fade to nothing and back each second, redrawn
/// every 16 ms (§8.6, Motion).
const CARET_BLINK: TextCaretMotion = TextCaretMotion {
    fade_cycle_duration: Duration::from_millis(1_000),
    frame_interval: Duration::from_millis(16),
    min_opacity: 0.0,
};

/// `base` dimmed to [`DISABLED_OPACITY`] while disabled (§8.6, States).
pub(super) fn state_color(base: Rgba) -> Driven<Color> {
    Driven::new(move |inputs: &Inputs<'_, ()>| {
        Glass::color(faded(
            base,
            if is_disabled(inputs.state) {
                DISABLED_OPACITY
            } else {
                1.0
            },
        ))
    })
}

/// The look of a text field, a secure field, or a navigation search field in
/// `spec` (§8.6).
///
/// The label floats with the focus response's ease whenever the field gains
/// or loses focus or content, however the change was made; the placeholder
/// takes the muted foreground, the value and label take the foreground, and
/// all three dim while disabled.
pub(in super::super) fn look(
    glass: &Glass,
    _spec: &TextFieldSpec,
    _env: &Environment,
) -> TextFieldLook {
    let palette = glass.palette;
    let field_font = Font::from(Body)
        .size(FIELD_TEXT_SIZE)
        .weight(FontWeight::Normal)
        .line_height(FIELD_LINE_HEIGHT);
    TextFieldLook::new(
        field_container(glass),
        METRICS,
        FloatingLabel::new(
            Label::new()
                .font(field_font.clone())
                .color(state_color(palette.foreground)),
            ValueMotion::Always(Animation::ease_out(PRESS_DURATION)),
        )
        .scale(FLOAT_SCALE)
        .top(FLOAT_TOP),
        Caret::new(paint(palette.accent), CARET_BLINK),
        paint(alpha(palette.accent, SELECTION_ALPHA)),
    )
    .placeholder(
        Label::new()
            .font(field_font.clone())
            .color(state_color(palette.muted_foreground)),
    )
    .value(
        Label::new()
            .font(field_font)
            .color(state_color(palette.foreground)),
    )
}

/// The text field's resting container: a `surface`-filled rounded rectangle
/// with a 1 pt hairline border (`palette.border`), both dimming to 0.5 while
/// disabled, plus the shared focus outline and ring.
fn field_container(glass: &Glass) -> Chrome {
    let palette = glass.palette;
    let fill = Driven::new(move |inputs: &Inputs<'_, ()>| {
        paint(faded(
            palette.surface,
            if is_disabled(inputs.state) {
                DISABLED_OPACITY
            } else {
                1.0
            },
        ))
    });
    let border = Driven::new(move |inputs: &Inputs<'_, ()>| {
        Border::new(
            1.0,
            paint(faded(
                palette.border,
                if is_disabled(inputs.state) {
                    DISABLED_OPACITY
                } else {
                    1.0
                },
            )),
        )
    });
    let outline = Custom::<(), _>::new(
        (focus_ring_paint(palette.accent),),
        |recorder, frame, (outline,)| {
            let inset = OUTLINE_WIDTH * 0.5;
            recorder.stroke(
                RoundedRect::from_rect(frame.inset(-inset), (FIELD_CORNER_RADIUS - inset).max(0.0)),
                Stroke::new(OUTLINE_WIDTH),
                outline,
            );
        },
    );
    state_chrome(
        glass,
        ChromeSurface::new(CornerShape::radius(FIELD_CORNER_RADIUS), fill).border(border),
    )
    .custom(outline)
}

/// The field's container: the track colour, at half its alpha while
/// disabled, in a rounded rectangle under the theme's state layer and focus
/// ring, with an accent outline just inside its edge while focused (§8.6).
pub(super) fn container(glass: &Glass) -> Chrome {
    let palette = glass.palette;
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
    let outline = Custom::<(), _>::new(
        (focus_ring_paint(palette.accent),),
        |recorder, frame, (outline,)| {
            let inset = OUTLINE_WIDTH * 0.5;
            recorder.stroke(
                RoundedRect::from_rect(frame.inset(-inset), (CORNER_RADIUS - inset).max(0.0)),
                Stroke::new(OUTLINE_WIDTH),
                outline,
            );
        },
    );
    state_chrome(
        glass,
        ChromeSurface::new(CornerShape::radius(CORNER_RADIUS), fill),
    )
    .custom(outline)
}
