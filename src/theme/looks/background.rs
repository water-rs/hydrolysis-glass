//! The glass background look: the glass material behind arbitrary content,
//! in the outline of the `Glass`'s shape, which Hydrolysis supplies.

use cherenkov_record::WorkingColor;
use hydrolysis::theme::{Backdrop, Driven, GlassLook, Inputs};
use waterui::background::{Glass as GlassBackground, GlassStyle};
use waterui_core::Environment;

use crate::material::recipe::{Family, Recipe};

use super::super::materials::{self, GLASS_SHADER};
use super::super::{Glass, surfaces};
use super::{control_motion, rgba_of};

/// The look of `background`: the regular or clear family's material for the
/// content's frame, washed with the glass's tint.
///
/// The recipe keys on the frame's short side, which the effect follows
/// live. Interaction changes nothing: the interactive material never
/// changes its recipe or its geometry on press (§5.5).
pub(in super::super) fn look(
    glass: &Glass,
    background: &GlassBackground,
    env: &Environment,
) -> GlassLook {
    let family = match background.style() {
        GlassStyle::Regular => Family::Regular,
        GlassStyle::Clear => Family::Clear,
    };
    let (palette, translucency) = (glass.palette, glass.translucency);
    let effect_of = move |frame, tint: Option<[f32; 4]>| {
        let recipe = surfaces::background_recipe(&palette, frame, family, tint, translucency);
        materials::pack(&recipe, surfaces::rect(frame))
    };
    let effect = match background.tint_color() {
        Some(tint) => Driven::follow(
            tint.resolve(env),
            move |tint: &WorkingColor, inputs: &Inputs<'_, ()>| {
                effect_of(inputs.frame, Some(rgba_of(*tint)))
            },
        ),
        None => Driven::new(move |inputs: &Inputs<'_, ()>| effect_of(inputs.frame, None)),
    };
    // The capture class is the look's, not the frame's: the recipe's
    // capture scale at the family's smallest measured element (`m` clamps
    // to it), the finest the family takes, which every larger recipe's
    // scale snaps up to. A member of a `.material_group()` takes the
    // container's class instead.
    let capture = Recipe::for_size(0.0, family, palette.appearance, translucency).capture_scale;
    GlassLook::new(
        Backdrop::new(
            GLASS_SHADER,
            materials::capture_class(false, capture),
            effect,
        ),
        control_motion(),
    )
}
