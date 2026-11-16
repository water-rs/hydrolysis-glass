//! The menu picker look: the text field's container with the picker's
//! chevron pair over it. The popup the field opens is the
//! [menu look](super::menu).

use hydrolysis::theme::{Custom, MenuPickerLayout, MenuPickerLook, PickerSpec};
use waterui_core::Environment;

use super::super::Glass;
use super::super::chrome;
use super::text_field;

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

/// The look of a menu picker's field in `spec`. The selected value keeps
/// its own styling.
pub(in super::super) fn look(
    glass: &Glass,
    _spec: &PickerSpec,
    _env: &Environment,
) -> MenuPickerLook {
    let palette = glass.palette;
    let chevrons = Custom::<(), _>::new((), move |recorder, frame, ()| {
        chrome::picker_indicator(&palette, recorder, frame);
    });
    MenuPickerLook::new(text_field::container(glass).custom(chevrons), LAYOUT)
}
