//! The text field look: a soft filled rounded rectangle under the theme's
//! state layer, with an accent outline and focus ring while focused, an
//! inside label that floats to the top at three quarters of its size, and
//! an accent caret and selection.
//!
//! The menu picker's field wears the same container, so
//! [`menu_picker`](super::menu_picker) takes it from here.

use core::time::Duration;

use cherenkov_record::Draw as _;
use cherenkov_record::kurbo::{RoundedRect, Stroke};
use hydrolysis::theme::{
    Caret, Chrome, CornerShape, Custom, Driven, FloatingLabel, InputFieldMetrics, Inputs, Label,
    Surface as ChromeSurface, TextCaretMotion, TextFieldLook, TextFieldSpec, ValueMotion,
};
use waterui::animation::Animation;
use waterui_core::Environment;

use super::super::Glass;
use super::super::PRESS_DURATION;
use super::super::palette::alpha;
use super::{faded, focus_ring_paint, is_disabled, paint, state_chrome};

/// The field's measures: a 20 pt band for the floated label, at least
/// 120 × 36 pt, its text inset 12 by 8.
const METRICS: InputFieldMetrics = InputFieldMetrics::new(20.0, 120.0, 36.0, 12.0, 8.0);
/// The container's corner radius, pt.
const CORNER_RADIUS: f64 = 10.0;
/// The width of the accent outline a focused field draws around its
/// container, pt.
const OUTLINE_WIDTH: f64 = 1.5;
/// The floated label's size relative to its resting size.
const FLOAT_SCALE: f64 = 0.75;
/// The floated label's distance below the container's top edge, pt.
const FLOAT_TOP: f64 = 4.0;
/// The alpha of the accent the selection is painted in.
const SELECTION_ALPHA: f32 = 0.3;
/// The caret's blink: a full fade to nothing and back each second, redrawn
/// every 16 ms.
const CARET_BLINK: TextCaretMotion = TextCaretMotion {
    fade_cycle_duration: Duration::from_millis(1_000),
    frame_interval: Duration::from_millis(16),
    min_opacity: 0.0,
};

/// The look of a text field or a secure field in `spec`.
///
/// The label floats with the focus response's ease whenever the field gains
/// or loses focus or content, however the change was made; the placeholder
/// takes the muted foreground, and the value and label keep their own
/// styling.
pub(in super::super) fn look(
    glass: &Glass,
    _spec: &TextFieldSpec,
    _env: &Environment,
) -> TextFieldLook {
    let palette = glass.palette;
    TextFieldLook::new(
        container(glass),
        METRICS,
        FloatingLabel::new(
            Label::new(),
            ValueMotion::Always(Animation::ease_out(PRESS_DURATION)),
        )
        .scale(FLOAT_SCALE)
        .top(FLOAT_TOP),
        Caret::new(paint(palette.accent), CARET_BLINK),
        paint(alpha(palette.accent, SELECTION_ALPHA)),
    )
    .placeholder(Label::new().color(Glass::color(palette.muted_foreground)))
}

/// The field's container: the track colour, at half its alpha while
/// disabled, in a rounded rectangle under the theme's state layer and focus
/// ring, with an accent outline just inside its edge while focused.
pub(super) fn container(glass: &Glass) -> Chrome {
    let palette = glass.palette;
    let fill = Driven::new(move |inputs: &Inputs<'_, ()>| {
        paint(faded(
            palette.track,
            if is_disabled(inputs.state) { 0.5 } else { 1.0 },
        ))
    });
    let outline = Custom::<(), _>::new(
        (focus_ring_paint(palette.accent),),
        |recorder, frame, (outline,)| {
            recorder.stroke(
                RoundedRect::from_rect(frame.inset(-OUTLINE_WIDTH * 0.5), CORNER_RADIUS),
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
