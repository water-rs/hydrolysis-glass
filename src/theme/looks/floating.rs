//! The floating-surface look: a regular-family continuous-corner glass
//! container with a subtle drop shadow, matching the native floating chrome.

use cherenkov_record::Shadow;
use cherenkov_record::kurbo::Vec2;
use hydrolysis_style::theme::{
    Backdrop, ContentEffect, CornerShape, Driven, Elevation, FloatingLook, Inputs,
    Surface as ChromeSurface,
};
use waterui_core::Environment;

use crate::material::recipe::{Family, Recipe};

use super::super::Glass;
use super::super::button::MIN_HIT_SIDE;
use super::super::materials::{self, CONTINUOUS_SMOOTHING, GLASS_CONTENT_SHADER, GLASS_SHADER};
use super::super::palette::working;
use super::super::surfaces;
use super::{DISABLED_CONTENT_ALPHA, control_motion};

/// Maximum corner radius of a floating glass surface, pt (chosen to yield a
/// pill on standard 44 pt controls and a softly rounded card on larger panels).
const FLOATING_MAX_CORNER_RADIUS: f64 = 22.0;
/// Drop-shadow blur sigma of a floating glass surface, pt.
const FLOATING_SHADOW_BLUR: f64 = 12.0;
/// Vertical offset of a floating glass surface's drop shadow, pt.
const FLOATING_SHADOW_OFFSET_Y: f64 = 4.0;
/// Opacity of a floating glass surface's drop shadow.
const FLOATING_SHADOW_ALPHA: f32 = 0.14;

/// The look of a `.floating()` surface in `glass`.
pub(in super::super) fn look(glass: &Glass, _env: &Environment) -> FloatingLook {
    let (palette, translucency) = (glass.palette, glass.translucency);
    let effect = Driven::new(move |inputs: &Inputs<'_, ()>| {
        let recipe = surfaces::background_recipe(
            &palette,
            inputs.frame,
            Family::Regular,
            None,
            translucency,
        );
        materials::pack(&recipe, surfaces::rect(inputs.frame))
    });
    let capture =
        Recipe::for_size(0.0, Family::Regular, palette.appearance, translucency).capture_scale;
    let content = Driven::new(move |inputs: &Inputs<'_, ()>| {
        let recipe = surfaces::background_recipe(
            &palette,
            inputs.frame,
            Family::Regular,
            None,
            translucency,
        );
        materials::content_effect(&recipe)
    });
    let elevation = Elevation::new(
        Shadow::new(
            FLOATING_SHADOW_BLUR,
            working([0.0, 0.0, 0.0, FLOATING_SHADOW_ALPHA]),
        )
        .offset(Vec2::new(0.0, FLOATING_SHADOW_OFFSET_Y)),
        Elevation::NONE.ambient,
    );
    let surface = ChromeSurface::new(
        CornerShape::radius(FLOATING_MAX_CORNER_RADIUS).smoothing(CONTINUOUS_SMOOTHING),
        Backdrop::new(
            GLASS_SHADER,
            materials::capture_class(false, capture),
            effect,
        ),
    )
    .elevation(elevation)
    .content_effect(ContentEffect::new(GLASS_CONTENT_SHADER, content));

    FloatingLook::new(
        surface,
        control_motion(),
        Glass::color(palette.foreground),
        Glass::color(palette.state_layer),
    )
    .minimum_size(f64::from(MIN_HIT_SIDE), f64::from(MIN_HIT_SIDE))
    .disabled_content_opacity(DISABLED_CONTENT_ALPHA)
}
