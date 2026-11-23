//! The menu look (specification §8.9): one opaque elevated panel for the
//! picker popup, the text context menu and a menu button's popup, its rows
//! split by hairlines and a picker popup's selected row on a soft accent
//! container.

use cherenkov_record::Shadow;
use cherenkov_record::kurbo::Vec2;
use hydrolysis_style::theme::{
    Border, CornerShape, Custom, Elevation, MenuLayout, MenuLook, MenuPresentationMotion, MenuSpec,
    Surface,
};
use waterui_core::Environment;

use crate::dynamics::{POPOVER_TARGET_RADIUS, Spring};
use crate::material::geometry::{CornerCurve, Element, Shape};
use crate::material::recipe::{Family, Recipe};

use super::super::Glass;
use super::super::chrome::{
    self, HAIRLINE, MENU_CORNER_RADIUS, POPUP_SHADOW_ALPHA, POPUP_SHADOW_BLUR, POPUP_SHADOW_OFFSET,
};
use super::super::materials;
use super::super::palette::working;
use super::super::surfaces;
use super::spring_animation;

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

/// The look of a menu in `spec`: a fallback opaque surface when no scene
/// backdrop is present, paired with animated popover presentation morphing
/// (`r_1 = 26.0 pt` continuous-corner regular glass) for anchored overlays.
pub(in super::super) fn look(glass: &Glass, _spec: MenuSpec, _env: &Environment) -> MenuLook {
    let (palette, translucency) = (glass.palette, glass.translucency);
    let appearance = palette.appearance;
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
    let presentation_motion = MenuPresentationMotion::new(
        spring_animation(Spring::POPOVER_PRESENT),
        spring_animation(Spring::POPOVER_DISMISS),
    )
    .trigger_opacity_window(0.0, 0.25)
    .content_opacity_window(0.25, 0.75)
    .content_scale_from(0.92)
    .content_scale_window(0.15, 0.85);
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
    .presentation_motion(presentation_motion)
    .presentation_panel(move |recorder, sample| {
        let rect = surfaces::rect(sample.surface);
        if rect.w <= 0.0 || rect.h <= 0.0 {
            return;
        }
        let q = sample.progress.max(0.0);
        let r0 = 0.5 * sample.origin.width().min(sample.origin.height());
        let r1 = f64::from(POPOVER_TARGET_RADIUS);
        let max_r = 0.5 * sample.surface.width().min(sample.surface.height());
        #[expect(clippy::cast_possible_truncation, reason = "corner radius fits in f32")]
        let radius = (r1 - r0).mul_add(q, r0).clamp(0.0, max_r) as f32;
        let element = Element {
            shape: Shape::rounded_rect(rect, radius, CornerCurve::Continuous),
            recipe: Recipe::for_size(rect.m(), Family::Regular, appearance, translucency),
        };
        materials::record(recorder, &element, false);
    })
}
