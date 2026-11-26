//! The glass card look: a continuous-corner translucent glass container
//! surface for `Automatic` and `Elevated` cards, with opaque surface fills for
//! `Filled` and `Outlined` variants.

use cherenkov_record::Shadow;
use cherenkov_record::kurbo::Vec2;
use hydrolysis_style::theme::{
    Backdrop, Border, CardLayout, CardLook, ContentEffect, CornerShape, Driven, Elevation, Inputs,
    Surface,
};
use waterui_controls::card::CardStyle;
use waterui_core::Environment;

use crate::material::recipe::{Family, Recipe};

use super::super::Glass;
use super::super::chrome::{HAIRLINE, POPUP_SHADOW_ALPHA, POPUP_SHADOW_BLUR, POPUP_SHADOW_OFFSET};
use super::super::materials::{self, CONTINUOUS_SMOOTHING, GLASS_CONTENT_SHADER, GLASS_SHADER};
use super::super::palette::working;
use super::super::surfaces::{self, CONTAINER_CORNER_RADIUS};

/// Inner padding around card title, subtitle, and body content, pt.
const CARD_PADDING: f32 = 16.0;
/// Vertical gap between title, subtitle, and body content, pt.
const CARD_CONTENT_SPACING: f32 = 6.0;

/// The glass theme's look for a card of `style`.
///
/// - `Automatic` and `Elevated` use the translucent glass container material
///   (`surfaces::container`) on continuous container corners; `Elevated` adds
///   a soft elevation shadow beneath the container.
/// - `Filled` uses the palette's opaque `surface_variant` fill.
/// - `Outlined` uses the palette's opaque `surface` fill with a hairline border.
pub(in super::super) fn look(glass: &Glass, style: CardStyle, _env: &Environment) -> CardLook {
    let (palette, translucency) = (glass.palette, glass.translucency);
    let shape = CornerShape::radius(CONTAINER_CORNER_RADIUS).smoothing(CONTINUOUS_SMOOTHING);

    let glass_surface = || {
        let effect = Driven::new(move |inputs: &Inputs<'_, ()>| {
            let surface = surfaces::container(&palette, inputs.frame, translucency);
            materials::effect(&surface.element)
        });
        let content = Driven::new(move |inputs: &Inputs<'_, ()>| {
            let surface = surfaces::container(&palette, inputs.frame, translucency);
            materials::content_effect(&surface.element.recipe)
        });
        let capture =
            Recipe::for_size(0.0, Family::Regular, palette.appearance, translucency).capture_scale;
        Surface::new(
            shape.clone(),
            Backdrop::new(
                GLASS_SHADER,
                materials::capture_class(false, capture),
                effect,
            ),
        )
        .content_effect(ContentEffect::new(GLASS_CONTENT_SHADER, content))
    };

    let surface = match style {
        CardStyle::Elevated => glass_surface().elevation(Elevation::new(
            Shadow::new(
                POPUP_SHADOW_BLUR,
                working([0.0, 0.0, 0.0, POPUP_SHADOW_ALPHA]),
            )
            .offset(Vec2::new(0.0, POPUP_SHADOW_OFFSET)),
            Elevation::NONE.ambient,
        )),
        CardStyle::Filled => Surface::new(shape, working(palette.surface_variant)),
        CardStyle::Outlined => Surface::new(shape, working(palette.surface))
            .border(Border::new(HAIRLINE, working(palette.border))),
        CardStyle::Automatic | _ => glass_surface(),
    };

    CardLook::new(
        surface,
        CardLayout::uniform(CARD_PADDING, CARD_CONTENT_SPACING),
    )
}
