//! The menu picker look (specification §8.9): the text field's container
//! with the picker's chevron pair over it, dimming both its selected value
//! and its chevrons while disabled. The popup the field opens is the
//! [menu look](super::menu).

use cherenkov_record::kurbo::Rect;
use hydrolysis_style::theme::{
    Custom, Driven, Inputs, Label, MenuPickerLayout, MenuPickerLook, PickerSpec,
};
use waterui_core::Environment;

use super::super::Glass;
use super::super::chrome;
use super::{faded, is_disabled, paint, text_field};

/// The measures of the field: at least 120 × 36 pt, its content inset 12 by
/// 8 with 8 pt between its label and its value, 24 pt kept for the chevrons,
/// and the popup 6 pt below it.
const LAYOUT: MenuPickerLayout = MenuPickerLayout {
    min_width: 120.0,
    min_height: 36.0,
    horizontal_inset: 12.0,
    vertical_inset: 8.0,
    label_spacing: 8.0,
    indicator_space: 24.0,
    popup_gap: 6.0,
};

/// The band of a field at `frame` its chevrons draw in: the
/// `indicator_space` Hydrolysis keeps clear of the value, inside the
/// trailing inset (at the left in RTL), over the field's whole height.
fn indicator_band(frame: Rect, rtl: bool) -> Rect {
    let inset = f64::from(LAYOUT.horizontal_inset);
    let space = f64::from(LAYOUT.indicator_space);
    if rtl {
        let x0 = frame.x0 + inset;
        Rect::new(x0, frame.y0, x0 + space, frame.y1)
    } else {
        let x1 = frame.x1 - inset;
        Rect::new(x1 - space, frame.y0, x1, frame.y1)
    }
}

/// The look of a menu picker's field in `spec` (§8.9).
pub(in super::super) fn look(
    glass: &Glass,
    spec: &PickerSpec,
    _env: &Environment,
) -> MenuPickerLook {
    let palette = glass.palette;
    let rtl = spec.direction().is_right_to_left();
    let chevron_paint = Driven::new(move |inputs: &Inputs<'_, ()>| {
        paint(faded(
            palette.muted_foreground,
            if is_disabled(inputs.state) {
                text_field::DISABLED_OPACITY
            } else {
                1.0
            },
        ))
    });
    let chevrons = Custom::<(), _>::new((chevron_paint,), move |recorder, frame, (stroke,)| {
        chrome::picker_indicator_with(recorder, indicator_band(frame, rtl), stroke);
    });
    MenuPickerLook::new(text_field::container(glass).custom(chevrons), LAYOUT)
        .value(Label::new().color(text_field::state_color(palette.foreground)))
}
