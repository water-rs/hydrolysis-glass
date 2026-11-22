//! The menu look (specification §8.9): one opaque elevated panel for the
//! picker popup, the text context menu and a menu button's popup, its rows
//! split by hairlines and a picker popup's selected row on a soft accent
//! container.

use cherenkov_record::Shadow;
use cherenkov_record::kurbo::Vec2;
use hydrolysis_style::theme::{
    Border, CornerShape, Custom, Elevation, MenuLayout, MenuLook, MenuSpec, Surface,
};
use waterui_core::Environment;

use super::super::Glass;
use super::super::chrome::{
    self, HAIRLINE, MENU_CORNER_RADIUS, POPUP_SHADOW_ALPHA, POPUP_SHADOW_BLUR, POPUP_SHADOW_OFFSET,
};
use super::super::palette::working;

/// The measures of a menu: rows 40 pt tall, padded 16 by 6, 120 to 280 pt
/// wide at 8 pt a character before text layout, with full-width hairline
/// separators.
const LAYOUT: MenuLayout = MenuLayout {
    row_height: 40.0,
    horizontal_padding: 16.0,
    vertical_padding: 6.0,
    min_width: 120.0,
    max_width: 280.0,
    width_per_char: 8.0,
    separator_horizontal_inset: 0.0,
    separator_thickness: HAIRLINE,
};

/// The look of a menu in `spec`. A menu has no live state of its own, so
/// its parts draw from the palette alone.
pub(in super::super) fn look(glass: &Glass, _spec: MenuSpec, _env: &Environment) -> MenuLook {
    let palette = glass.palette;
    let panel = Surface::new(
        CornerShape::radius(MENU_CORNER_RADIUS),
        working(palette.surface),
    )
    .border(Border::new(HAIRLINE, working(palette.border)))
    .elevation(Elevation::new(
        Shadow::new(
            POPUP_SHADOW_BLUR,
            working([0.0, 0.0, 0.0, POPUP_SHADOW_ALPHA]),
        )
        .offset(Vec2::new(0.0, POPUP_SHADOW_OFFSET)),
        Elevation::NONE.ambient,
    ));
    MenuLook::new(
        LAYOUT,
        panel,
        Custom::<(), _>::new((), move |recorder, frame, ()| {
            chrome::separator(&palette, recorder, frame);
        }),
    )
    .selected_row(Custom::<(), _>::new((), move |recorder, frame, ()| {
        chrome::selected_row(&palette, recorder, frame);
    }))
}
