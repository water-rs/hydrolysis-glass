//! The menu look: one opaque elevated panel for the picker popup, the text
//! context menu and a menu button's popup, its rows split by hairlines and
//! a picker popup's selected row on a soft accent container.

use hydrolysis::theme::{Custom, MenuLayout, MenuLook, MenuSpec};
use waterui_core::Environment;

use super::super::Glass;
use super::super::chrome::{self, HAIRLINE, MENU_CORNER_RADIUS};

/// The measures of a menu: rows 40 pt tall, padded 16 by 6, 120 to 280 pt
/// wide at 8 pt a character before text layout, on the theme's menu corner,
/// with full-width hairline separators.
const LAYOUT: MenuLayout = MenuLayout {
    row_height: 40.0,
    horizontal_padding: 16.0,
    vertical_padding: 6.0,
    min_width: 120.0,
    max_width: 280.0,
    width_per_char: 8.0,
    corner_radius: MENU_CORNER_RADIUS,
    separator_horizontal_inset: 0.0,
    separator_thickness: HAIRLINE,
};

/// The look of a menu in `spec`. A menu has no live state of its own, so
/// its parts draw from the palette alone.
pub(in super::super) fn look(glass: &Glass, _spec: MenuSpec, _env: &Environment) -> MenuLook {
    let palette = glass.palette;
    MenuLook::new(
        LAYOUT,
        Custom::<(), _>::new((), move |recorder, frame, ()| {
            chrome::popup_panel(&palette, recorder, frame);
        }),
        Custom::<(), _>::new((), move |recorder, frame, ()| {
            chrome::separator(&palette, recorder, frame);
        }),
    )
    .selected_row(Custom::<(), _>::new((), move |recorder, frame, ()| {
        chrome::selected_row(&palette, recorder, frame);
    }))
}
