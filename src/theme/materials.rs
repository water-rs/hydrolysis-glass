//! The glass material in the engine's recording contract
//! (water-rs/waterui#1788 and followers).
//!
//! [`register`] declares the theme's backdrop shader and capture classes;
//! [`record`] records one [`Element`] as a `Recorder::backdrop_material`
//! member: its shape becomes the member's clip and union-field record,
//! its recipe the member's [`MaterialEffect`]. The engine's group pass
//! then runs `shaders/material_engine.wgsl` — the standalone
//! `shaders/material.wgsl` retargeted onto the `BackdropPixel`
//! contract — against the group's shared capture and union field.
//!
//! Two contract choices live here:
//!
//! - **Capture scale.** A capture class is a registered token, so its
//!   scale cannot ride the recipe's continuous `capture_scale`. The
//!   registry declares the lattice [`CAPTURE_SCALES`] — the recipe's own
//!   measured values — and a member snaps **up** to the smallest step at
//!   or above its recipe's: §4.1 bounds the capture texel at *at most*
//!   `1 / (scale · px per pt)`, the direction the recipe's own `ceil`
//!   already rounds, so a finer capture stays inside the recipe. The
//!   shader then rescales every capture-texel radius by
//!   `class scale / recipe scale`, keeping each physical blur radius
//!   the recipe's whatever step the member landed on. Members whose
//!   recipes quantize to different steps belong to different classes and
//!   cannot join one group — a contract limit, not a silent fallback.
//! - **Union.** Every glass class groups as a
//!   [`MaterialGrouping::Union`] field: standalone members share the
//!   standalone 8 pt smoothing, a container's members the 30 pt
//!   container smoothing, mapped onto the engine's quadratic smooth
//!   minimum. Members of one scope, canvas and class share one capture
//!   and one field; a single-member union degenerates to the member's
//!   own field.

use cherenkov::kurbo::{Rect as KRect, RoundedRect, RoundedRectRadii};
use cherenkov::{BlendSpace, ContinuousRect, Fixed, Recorder, ShapeData};
use cherenkov_record::{
    BackdropShaderSource, CaptureClass, CaptureLevels, CaptureScale, MaterialCapture,
    MaterialEffect, MaterialGrouping, MaterialRegistry, MaterialShader, OuterExtent,
    UnionSmoothing,
};

use crate::material::geometry::{
    CONTAINER_UNION_SMOOTHING, CONTINUOUS_EXPONENT, CornerCurve, Element, Shape,
    STANDALONE_UNION_SMOOTHING,
};
use crate::material::recipe::Recipe;

/// The key the glass `backdrop_effect` shader registers under: the one
/// WGSL source serves every capture class.
pub const GLASS_SHADER: MaterialShader = MaterialShader::new(1);

/// The capture-scale lattice the registry declares, ascending. These are
/// the recipe's measured `capture_scale` values (§4.1): the clear family
/// and the small regulars at the default translucency sit on 0.5, the
/// largest regulars on 0.25, the deepest-translucency large elements on
/// 0.333 and 0.125.
pub const CAPTURE_SCALES: [f32; 4] = [0.125, 0.25, 0.333, 0.5];

/// `CaptureClass` key of a standalone member at `CAPTURE_SCALES[i]`.
const STANDALONE_CLASS_BASE: u32 = 0x100;
/// `CaptureClass` key of a container member at `CAPTURE_SCALES[i]`.
const CONTAINER_CLASS_BASE: u32 = 0x200;

/// How many pyramid levels the glass captures keep: the recipe's largest
/// reads (the ambient pickup's blur and the haze taps) clamp against the
/// deepest level, so the classes take the engine's maximum.
const LEVELS: u32 = CaptureLevels::MAX;

/// The sampling reach the shader declares, device pixels: the largest
/// displacement a `backdrop_sample*` coordinate takes from the member
/// pixel — the recipe's biggest lobe (the ambient shift, `0.35·m` at the
/// measured-range bound `m = 200.33`, ≈70.1 pt) at a 4 px/pt recording
/// bound — plus the haze taps' `0.25 · lod` capture texels (≤14 px at the
/// smallest class's 8 px texel).
const SHADER_REACH: f32 = 0.35 * 200.33 * 4.0 + 14.0;

/// The union smoothing of a container member's field, and of every
/// standalone member's.
const fn union_smoothing(container: bool) -> f32 {
    if container {
        CONTAINER_UNION_SMOOTHING
    } else {
        STANDALONE_UNION_SMOOTHING
    }
}

/// The capture class an element records under: `container` selects the
/// 30 pt container union field over the 8 pt standalone one, and
/// `recipe_scale` snaps up to [`CAPTURE_SCALES`].
///
/// # Panics
/// Unless `0 < recipe_scale ≤ CAPTURE_SCALES[3]` — a recipe is a
/// measured table, not a free parameter.
#[must_use]
#[allow(
    clippy::cast_possible_truncation,
    reason = "the lattice index is at most 3"
)]
pub fn capture_class(container: bool, recipe_scale: f32) -> CaptureClass {
    assert!(
        recipe_scale.is_finite() && recipe_scale > 0.0 && recipe_scale <= CAPTURE_SCALES[3] + 1e-6,
        "a recipe's capture scale is a measured value inside the lattice, not {recipe_scale}"
    );
    let i = CAPTURE_SCALES
        .iter()
        .position(|s| s + 1e-6 >= recipe_scale)
        .unwrap_or(CAPTURE_SCALES.len() - 1);
    let base = if container {
        CONTAINER_CLASS_BASE
    } else {
        STANDALONE_CLASS_BASE
    };
    CaptureClass::new(base + i as u32)
}

/// The `backdrop_effect` source the shader key registers: the recipe's
/// engine form with the sampling reach its lobes can take.
fn shader_source() -> BackdropShaderSource {
    BackdropShaderSource::wgsl(include_str!("../../shaders/material_engine.wgsl"))
        .reach(SHADER_REACH)
}

/// Registers the glass backdrop shader and every capture class the
/// theme's materials name. Called from
/// `WidgetTheme::register_backdrop_shaders`.
pub fn register(registry: &mut MaterialRegistry) {
    registry.register_shader(GLASS_SHADER, shader_source());
    for container in [false, true] {
        let base = if container {
            CONTAINER_CLASS_BASE
        } else {
            STANDALONE_CLASS_BASE
        };
        for (i, &scale) in CAPTURE_SCALES.iter().enumerate() {
            registry.register_capture_class(
                #[allow(clippy::cast_possible_truncation, reason = "the index is at most 3")]
                CaptureClass::new(base + i as u32),
                MaterialCapture {
                    scale: CaptureScale::new(scale)
                        .expect("the lattice stays inside (0, 1]"),
                    levels: CaptureLevels::new(LEVELS)
                        .expect("the level count is inside 1..=MAX"),
                    grouping: MaterialGrouping::Union {
                        smoothing: UnionSmoothing::new(union_smoothing(container))
                            .expect("the union smoothings are positive"),
                    },
                    blend_space: BlendSpace::SrgbEncoded,
                },
            );
        }
    }
}

/// The Lamé smoothing matching the recipe's `CONTINUOUS_EXPONENT` corner:
/// the engine's `ContinuousRect` maps smoothing to exponent `2 + 2·s`,
/// so 2.02 is `s = 0.01`.
const CONTINUOUS_SMOOTHING: f64 = (CONTINUOUS_EXPONENT as f64 - 2.0) * 0.5;

/// Converts the element's silhouette to the engine's analytic shape.
/// Circular corners become a `RoundedRect`, the recipe's continuous
/// corner a `ContinuousRect` at [`CONTINUOUS_SMOOTHING`], and a shape
/// whose radii all clamp to zero a plain `Rect`. The member's own field
/// and the group's union field are both analytic, so only analytic
/// shapes are allowed — a `Path` member could not join a union.
#[must_use]
pub fn shape_data(shape: &Shape) -> ShapeData {
    let r = shape.rect;
    let rect = KRect::new(
        f64::from(r.x),
        f64::from(r.y),
        f64::from(r.x + r.w),
        f64::from(r.y + r.h),
    );
    let half = f64::from(r.w.min(r.h) * 0.5);
    let clamped = shape.radii.map(|v| f64::from(v).clamp(0.0, half));
    if clamped.iter().all(|v| *v <= 0.0) {
        return ShapeData::Rect(rect);
    }
    let radii = RoundedRectRadii::new(clamped[0], clamped[1], clamped[2], clamped[3]);
    match shape.curve {
        CornerCurve::Circular => ShapeData::RoundedRect(RoundedRect::from_rect(rect, radii)),
        CornerCurve::Continuous => ShapeData::Continuous(ContinuousRect {
            rect,
            radii,
            smoothing: CONTINUOUS_SMOOTHING,
        }),
    }
}

/// The distance outside the silhouette the member still writes: the
/// recipe's exterior span, or the cast shadow's reach when that runs
/// farther — `|cast shift| + 1.3·falloff` covers the falloff profile's
/// zero crossing at ≈1.15 normalized distances with margin.
const CAST_REACH_FALLOFF: f32 = 1.3;

/// Packs one element's recipe and member rect into the effect the member
/// binds — the shader's sixteen `vec4`s in header order — and sets the
/// member's outer extent so the cast shadow and exterior span are not
/// clipped by the composite's draw bound.
///
/// # Panics
/// When the recipe contradicts the constants folded into
/// `material_engine.wgsl` — the border correction's measured row and the
/// highlight's shared terms (see the shader's header). A recipe is a
/// measured table; those fields are not free parameters.
#[must_use]
pub fn effect(element: &Element) -> MaterialEffect {
    let r = &element.recipe;
    let rect = element.shape.rect;
    assert_folded_constants(r);
    let tint = r.tint.map_or([0.0; 4], |t| [t.rgb[0], t.rgb[1], t.rgb[2], t.alpha]);
    #[expect(
        clippy::cast_possible_truncation,
        reason = "rect coordinates are display-resolution and well inside f32"
    )]
    let uniforms = vec![
        r.primary_shift,
        r.primary_depth,
        r.rim_shift,
        r.rim_depth,
        r.rim_ramp[0],
        r.rim_ramp[1],
        r.rim_mix,
        r.blur_radius,
        r.blur_depths[0],
        r.blur_depths[1],
        r.blur_depths[2],
        r.blur_depths[3],
        r.blur_gains[0],
        r.blur_gains[1],
        r.blur_gains[2],
        r.blur_gains[3],
        r.haze_radius,
        r.haze_min_weight,
        r.haze_max_weight,
        r.haze_mix,
        r.luminance_limit,
        r.grade_mix,
        r.capture_scale,
        r.exterior_span,
        r.grade.floor,
        r.grade.ceiling,
        r.grade.chroma,
        r.wash_alpha,
        r.wash[0],
        r.wash[1],
        r.wash[2],
        f32::from(r.ambient_favours_light),
        r.ambient_shift,
        r.ambient_depth,
        r.ambient_blur,
        r.ambient_strength,
        r.ambient_ramp[0],
        r.ambient_ramp[1],
        r.ambient_grade.floor,
        r.ambient_grade.ceiling,
        r.ambient_grade.chroma,
        r.contour_shade_density,
        r.contour_shade_shift,
        r.contour_shade_softness,
        r.contour_shade_width,
        r.contour_shade_clip,
        r.border_aperture,
        r.cast_falloff,
        r.cast_density,
        r.cast_gain,
        r.channel_ceiling,
        r.highlight.aperture,
        tint[0],
        tint[1],
        tint[2],
        tint[3],
        r.cast_shift[0],
        r.cast_shift[1],
        rect.x,
        rect.y,
        rect.w,
        rect.h,
        r.ellipse_bend,
        f32::from(r.tint.is_some()),
    ];
    debug_assert_eq!(
        uniforms.len(),
        MaterialEffect::MAX_UNIFORMS,
        "the packing table fills exactly the contract's sixteen vec4s"
    );
    let outer = r
        .exterior_span
        .max(r.cast_shift[0].hypot(r.cast_shift[1]) + CAST_REACH_FALLOFF * r.cast_falloff);
    MaterialEffect::new(uniforms).outer(
        OuterExtent::new(outer)
            .expect("the exterior span and cast reach keep the extent finite and non-negative"),
    )
}

/// Asserts the recipe fields `material_engine.wgsl` folds into constants:
/// the border correction's measured row and the highlight's shared
/// terms. A recipe that moved them needs the packing widened, not a
/// silently wrong shader.
#[track_caller]
fn assert_folded_constants(r: &Recipe) {
    let near = |a: f32, b: f32| (a - b).abs() < 1e-4;
    assert!(
        near(r.border_strength, 0.4)
            && near(r.border_bearing, core::f32::consts::FRAC_PI_2)
            && near(r.border_band_width, 0.533)
            && near(r.border_band_offset, -0.533)
            && near(r.border_colour_bias, -0.3),
        "the border correction's measured row is folded into the shader constants"
    );
    let h = &r.highlight;
    assert!(
        near(h.inner_fade, 0.75)
            && near(h.first_gain, 0.5)
            && near(h.second_gain, 0.5)
            && near(h.knee, 0.5)
            && near(h.first_bearing, 0.0)
            && near(h.second_bearing, core::f32::consts::PI)
            && near(h.glow_gain, 0.15)
            && near(h.glow_width, 8.0)
            && near(h.glow_aperture, 0.65)
            && h.first_colour.iter().all(|c| near(*c, 1.0))
            && h.second_colour.iter().all(|c| near(*c, 1.0)),
        "the highlight's shared terms are folded into the shader constants"
    );
}

/// Records one glass element as a `Recorder::backdrop_material` at the
/// recorder's top level: the shape is the member's clip and union-field
/// record, the recipe its live effect. `container` selects the
/// container union field's capture class.
///
/// # Panics
/// As `Recorder::backdrop_material` does: outside a
/// `Content::record_layered` recording, or inside a clip, transform or
/// group scope.
pub fn record(recorder: &mut Recorder, element: &Element, container: bool) {
    recorder.backdrop_material(
        Fixed(shape_data(&element.shape)),
        GLASS_SHADER,
        capture_class(container, element.recipe.capture_scale),
        Fixed(effect(element)),
    );
}
