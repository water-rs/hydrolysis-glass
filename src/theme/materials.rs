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
//! Every blurred read of the engine form is `backdrop_sample_blur` on a
//! Gaussian pyramid, which every glass class registers: the Gaussian of
//! each sample's deviation (§3, §4.4 and, as [`ambient_sigma`], §4.7) in
//! 16 filtered fetches, with no loops.
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
//!   cannot join one standalone group; inside a `.material_group()` the
//!   container recipe ([`GlassContainer`], §5.6) resolves one class for
//!   all members.
//! - **Union.** Every glass class groups as a
//!   [`MaterialGrouping::Union`] field: standalone members share the
//!   standalone 8 pt smoothing, a container's members the 30 pt
//!   container smoothing, carried onto the engine's quadratic smooth
//!   minimum by [`SMOOTHING_TO_UNION_K`] so both renderers fuse the
//!   same field. Members of one scope, canvas and class share one
//!   capture and one field; a single-member union degenerates to the
//!   member's own field.

use core::cmp::Ordering;

use cherenkov_record::kurbo::{Rect as KRect, RoundedRect, RoundedRectRadii};
use cherenkov_record::{
    BackdropShaderSource, BlendSpace, CaptureClass, CaptureLevels, CapturePyramid, CaptureScale,
    ContainerRecipe, ContinuousRect, Fixed, GroupMember, Interpolation, LinearGradient,
    MaterialCapture, MaterialEffect, MaterialGrouping, MaterialRegistry, MaterialShader,
    OuterExtent, Paint, Recorder, ShapeData, UnionSmoothing,
};

use crate::material::geometry::{
    CONTAINER_UNION_SMOOTHING, CONTINUOUS_EXPONENT, CornerCurve, Element, SMOOTHING_TO_UNION_K,
    STANDALONE_UNION_SMOOTHING, Shape,
};
use crate::material::recipe::{Recipe, ambient_sigma, blur_read_reach, blur_sigma, haze_sigma};

use super::palette::{Rgba, working};
use super::surfaces::Wash;

/// The key the glass `backdrop_effect` shader registers under: the one
/// WGSL source serves every capture class.
pub const GLASS_SHADER: MaterialShader = MaterialShader::new(1);

/// The capture-scale lattice the registry declares, ascending.
///
/// These are the recipe's measured `capture_scale` values (§4.1): the
/// clear family and the small regulars at the default translucency sit on
/// 0.5, the largest regulars on 0.25, the deepest-translucency large
/// elements on 0.333 and 0.125.
pub const CAPTURE_SCALES: [f32; 4] = [0.125, 0.25, 0.333, 0.5];

/// `CaptureClass` key of a standalone member at `CAPTURE_SCALES[i]`.
const STANDALONE_CLASS_BASE: u32 = 0x100;
/// `CaptureClass` key of a container member at `CAPTURE_SCALES[i]`.
const CONTAINER_CLASS_BASE: u32 = 0x200;

/// How many pyramid levels the glass captures keep: the recipe's largest
/// reads (the ambient pickup's blur and the haze's) clamp against the
/// deepest level, so the classes take the engine's maximum.
const LEVELS: u32 = CaptureLevels::MAX;

/// The device pixels per pt the declared reach is derived at: the
/// recording bound.
const REACH_PX_PER_PT: f32 = 4.0;
/// The largest primary-lobe displacement, pt (§5).
const PRIMARY_LOBE_PT: f32 = 60.0;
/// The largest blur radius, at gain 1 (§4.3).
const LARGEST_BLUR_RADIUS: f32 = 5.0;
/// The haze's largest radius (§4.4).
const LARGEST_HAZE_RADIUS: f32 = 8.0;
/// The ambient pickup's lobe and blur radius, `0.35·m` at the
/// measured-range bound `m = 200.33` (§4.7): pt for the lobe, recipe
/// capture texels for the blur.
const LARGEST_AMBIENT: f32 = 0.35 * 200.33;

/// The sampling reach the shader declares, device pixels: the largest
/// displacement a `backdrop_sample_blur` coordinate takes from the member
/// pixel at a 4 px/pt recording bound plus the footprint of the read
/// there, whichever sample and capture class is the farthest.
///
/// The reads are the primary sample (lobe 60 pt, blur radius 5 at gain 1:
/// [`blur_sigma`] pt), the haze at the pixel itself (radius 8:
/// [`haze_sigma`] pt) and the ambient sample (lobe `0.35·m` pt, blur
/// `0.35·m` capture texels read as [`ambient_sigma`] texels).
///
/// On a class of capture scale `scale` a pt deviation is `σ_pt · scale · 4`
/// texels at the 4 px/pt bound, and a read of `σ` texels reaches
/// [`blur_read_reach`] of `σ` texels, that is `/ scale` device pixels. The
/// ambient radius counts texels of the recipe's own capture, taken to be
/// the class's (a recipe sits on the lattice), so it is `0.35·m` texels on
/// every class. A finer class reaches farther for the same length, as the
/// footprint's constant term (4.5 texels) is longer on it, so the reach is
/// the maximum over [`CAPTURE_SCALES`].
fn shader_reach() -> f32 {
    let top = LEVELS - 1;
    CAPTURE_SCALES
        .iter()
        .flat_map(|&scale| {
            let texels_per_pt = scale * REACH_PX_PER_PT;
            let device = |sigma_texels: f32| blur_read_reach(sigma_texels, top) / scale;
            [
                PRIMARY_LOBE_PT.mul_add(
                    REACH_PX_PER_PT,
                    device(blur_sigma(LARGEST_BLUR_RADIUS) * texels_per_pt),
                ),
                device(haze_sigma(LARGEST_HAZE_RADIUS) * texels_per_pt),
                LARGEST_AMBIENT.mul_add(REACH_PX_PER_PT, device(ambient_sigma(LARGEST_AMBIENT))),
            ]
        })
        .fold(0.0, f32::max)
}

/// The blend distance `k` the engine's quadratic smooth minimum folds a
/// container's member fields with, pt, and the one every standalone
/// member's field takes.
///
/// The material's smoothing is [`CONTAINER_UNION_SMOOTHING`] /
/// [`STANDALONE_UNION_SMOOTHING`]; [`SMOOTHING_TO_UNION_K`] carries it
/// onto this family, so the engine's fused field has the standalone
/// renderer's fillets rather than a second, looser union.
const fn union_smoothing(container: bool) -> f32 {
    let smoothing = if container {
        CONTAINER_UNION_SMOOTHING
    } else {
        STANDALONE_UNION_SMOOTHING
    };
    smoothing * SMOOTHING_TO_UNION_K
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
/// engine form, after the §3 blur law it shares with the standalone
/// pass, with the sampling reach its lobes and blur reads can take.
fn shader_source() -> BackdropShaderSource {
    BackdropShaderSource::wgsl(concat!(
        include_str!("../../shaders/blur_law.wgsl"),
        "\n",
        include_str!("../../shaders/material_engine.wgsl"),
    ))
    .reach(shader_reach())
}

/// Registers the glass backdrop shader and every capture class the
/// theme's materials name. Called from
/// `WidgetTheme::register_backdrop_shaders`.
///
/// # Panics
/// When [`CAPTURE_SCALES`], the level count or a union smoothing steps
/// outside the contract's valid ranges — a constant, not a value a caller
/// can feed back.
pub fn register(registry: &mut MaterialRegistry) {
    registry.register_shader(GLASS_SHADER, shader_source());
    registry.register_container_recipe(GLASS_SHADER, GlassContainer);
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
                    scale: CaptureScale::new(scale).expect("the lattice stays inside (0, 1]"),
                    levels: CaptureLevels::new(LEVELS).expect("the level count is inside 1..=MAX"),
                    pyramid: CapturePyramid::Gaussian,
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
pub const CONTINUOUS_SMOOTHING: f64 = (CONTINUOUS_EXPONENT as f64 - 2.0) * 0.5;

/// Converts the element's silhouette to the engine's analytic shape.
///
/// Circular corners become a `RoundedRect`, the recipe's continuous
/// corner a `ContinuousRect` at the Lamé smoothing matching the
/// recipe's exponent, and a shape whose radii all clamp to zero a plain
/// `Rect`. The member's own field and the group's union field are both
/// analytic, so only analytic shapes are allowed — a `Path` member
/// could not join a union.
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
/// binds.
///
/// The uniforms are the shader's sixteen `vec4`s in header order; the
/// member's outer extent follows the cast shadow and exterior span so
/// they are not clipped by the composite's draw bound.
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
    let tint = r
        .tint
        .map_or([0.0; 4], |t| [t.rgb[0], t.rgb[1], t.rgb[2], t.alpha]);
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
        crate::material::recipe::Tint::flag(r.tint),
    ];
    debug_assert_eq!(
        uniforms.len(),
        MaterialEffect::MAX_UNIFORMS,
        "the packing table fills exactly the contract's sixteen vec4s"
    );
    let outer = r
        .exterior_span
        .max(CAST_REACH_FALLOFF.mul_add(r.cast_falloff, r.cast_shift[0].hypot(r.cast_shift[1])));
    MaterialEffect::new(uniforms).outer(
        OuterExtent::new(outer)
            .expect("the exterior span and cast reach keep the extent finite and non-negative"),
    )
}

/// Positions in [`effect`]'s uniform table that the container recipe
/// reads or rewrites; `effect_slots_match_the_packing_table` pins them to
/// the table.
mod slot {
    use core::ops::Range;

    /// Primary lobe shift, pt (§5.6: the smallest member's).
    pub const PRIMARY_SHIFT: usize = 0;
    /// Primary lobe depth, pt (§5.6: the smallest member's).
    pub const PRIMARY_DEPTH: usize = 1;
    /// First blur depth `d0` (§5.6: the smallest member's).
    pub const BLUR_DEPTH_0: usize = 8;
    /// The recipe's continuous capture scale (§5.6: the largest member's).
    pub const CAPTURE_SCALE: usize = 22;
    /// Tint colour and alpha (§2: the member's own).
    pub const TINT: Range<usize> = 52..56;
    /// The member's rect `x, y, w, h` (§2: the member's own field).
    pub const RECT: Range<usize> = 58..62;
    /// Ellipse bend (§2: the member's own).
    pub const ELLIPSE_BEND: usize = 62;
    /// Whether the member is tinted (§2: the member's own).
    pub const TINT_FLAG: usize = 63;
}

/// The glass material's container recipe (§5.6), registered for
/// [`GLASS_SHADER`].
///
/// Each member's [`GroupMember::effect`] is its standalone recipe packed
/// by [`effect`], and its `m` is the short side of its current shape's
/// bounds (`Rect::m`, what the recipes key on). From them the container
/// takes the capture scale and every other §5.1 value from the member
/// with the largest `m`, and the primary shift, primary depth and first
/// blur depth from the member with the smallest; each member keeps its
/// own rect, tint and ellipse bend (§2). **[measured; the tint and the
/// ellipse bend are chosen]**
///
/// Members of equal `m` may carry different recipes (families the
/// specification has not observed mixed); the one whose uniforms order
/// first, element by element, is the smallest of them and the one that
/// orders last the largest, so the choice never depends on the members'
/// order. **[chosen]**
#[derive(Clone, Copy, Debug, Default)]
pub struct GlassContainer;

impl GlassContainer {
    /// `m` of a member: the short side of its shape's bounds.
    #[expect(
        clippy::cast_possible_truncation,
        reason = "a short side in pt is far inside f32"
    )]
    fn m(member: &GroupMember<'_>) -> f32 {
        let bounds = member.shape.bounds();
        bounds.width().min(bounds.height()) as f32
    }

    /// Orders members by `m`, then by their recipes' uniforms.
    fn order(a: &GroupMember<'_>, b: &GroupMember<'_>) -> Ordering {
        Self::m(a).total_cmp(&Self::m(b)).then_with(|| {
            a.effect
                .uniforms()
                .iter()
                .zip(b.effect.uniforms())
                .map(|(x, y)| x.total_cmp(y))
                .find(|ordering| ordering.is_ne())
                .unwrap_or(Ordering::Equal)
        })
    }

    /// The members with the largest and the smallest `m`.
    ///
    /// # Panics
    /// On no members: a group has at least one.
    fn extremes<'a, 'm>(
        members: &'a [GroupMember<'m>],
    ) -> (&'a GroupMember<'m>, &'a GroupMember<'m>) {
        let largest = members.iter().max_by(|a, b| Self::order(a, b));
        let smallest = members.iter().min_by(|a, b| Self::order(a, b));
        largest
            .zip(smallest)
            .expect("a container group has at least one member")
    }
}

impl ContainerRecipe for GlassContainer {
    fn capture(&self, members: &[GroupMember<'_>]) -> CaptureClass {
        let (largest, _) = Self::extremes(members);
        capture_class(true, largest.effect.uniforms()[slot::CAPTURE_SCALE])
    }

    #[expect(
        clippy::cast_possible_truncation,
        reason = "a rect in pt is far inside f32"
    )]
    fn effects(&self, members: &[GroupMember<'_>]) -> Vec<MaterialEffect> {
        if members.is_empty() {
            return Vec::new();
        }
        let (largest, smallest) = Self::extremes(members);
        let mut shared = largest.effect.uniforms().to_vec();
        for index in [slot::PRIMARY_SHIFT, slot::PRIMARY_DEPTH, slot::BLUR_DEPTH_0] {
            shared[index] = smallest.effect.uniforms()[index];
        }
        members
            .iter()
            .map(|member| {
                let own = member.effect.uniforms();
                let mut uniforms = shared.clone();
                for range in [slot::TINT, slot::ELLIPSE_BEND..slot::TINT_FLAG + 1] {
                    uniforms[range.clone()].copy_from_slice(&own[range]);
                }
                let bounds = member.shape.bounds();
                uniforms[slot::RECT].copy_from_slice(&[
                    bounds.x0 as f32,
                    bounds.y0 as f32,
                    bounds.width() as f32,
                    bounds.height() as f32,
                ]);
                MaterialEffect::new(uniforms).outer(largest.effect.outer_extent())
            })
            .collect()
    }
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
/// recorder's top level.
///
/// The shape is the member's clip and union-field record, the recipe its
/// live effect; `container` selects the container union field's capture
/// class.
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

/// Colour stops the wash's horizontal profile is sampled at, evenly across
/// the grown width: its alpha is quadratic in the distance from the touch,
/// which a piecewise-linear gradient of this many segments follows within
/// a thousandth of the peak.
const WASH_SEGMENTS: u16 = 32;

/// The paint of a button's press wash (§8.1).
///
/// White, its alpha following [`Wash::alpha_at`] across the grown width,
/// interpolated in encoded space. The look fills the grown silhouette with it, in a group composited in
/// encoded space, over the glass body and below the label.
#[must_use]
pub fn wash_paint(wash: &Wash) -> Paint {
    let r = wash.shape.rect;
    let mut gradient = LinearGradient::new((f64::from(r.x), 0.0), (f64::from(r.x + r.w), 0.0))
        .interpolation(Interpolation::SrgbEncoded);
    for i in 0..=WASH_SEGMENTS {
        let t = f32::from(i) / f32::from(WASH_SEGMENTS);
        let white: Rgba = [1.0, 1.0, 1.0, wash.alpha_at(r.w.mul_add(t, r.x))];
        gradient = gradient.stop(t, working(white));
    }
    Paint::Linear(gradient)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::material::geometry::Rect;
    use crate::material::recipe::{Appearance, Family, Tint, Translucency};

    /// A regular dark element of `shape`, as the theme builds one.
    fn element(shape: Shape) -> Element {
        Element {
            shape,
            recipe: Recipe::for_size(
                shape.m(),
                Family::Regular,
                Appearance::Dark,
                Translucency::default(),
            ),
        }
    }

    fn circle() -> Element {
        element(Shape::circle(30.0, 30.0, 30.0))
    }

    fn card() -> Element {
        element(Shape::rounded_rect(
            Rect::new(-120.0, 100.0, 300.0, 200.33),
            40.0,
            CornerCurve::Continuous,
        ))
    }

    /// The members' recordings: shape and standalone effect.
    fn recorded(elements: &[&Element]) -> Vec<(ShapeData, MaterialEffect)> {
        elements
            .iter()
            .map(|e| (shape_data(&e.shape), effect(e)))
            .collect()
    }

    fn members(recorded: &[(ShapeData, MaterialEffect)]) -> Vec<GroupMember<'_>> {
        recorded
            .iter()
            .map(|(shape, effect)| GroupMember {
                shape,
                capture: CaptureClass::new(STANDALONE_CLASS_BASE),
                effect,
            })
            .collect()
    }

    /// `element` drawn with the container recipe of `largest` and `smallest`
    /// and its own tint and ellipse bend, through the standalone packing.
    fn in_container(element: &Element, largest: &Element, smallest: &Element) -> MaterialEffect {
        effect(&Element {
            shape: element.shape,
            recipe: Recipe {
                tint: element.recipe.tint,
                ellipse_bend: element.recipe.ellipse_bend,
                ..Recipe::for_container(&largest.recipe, &smallest.recipe)
            },
        })
    }

    /// Both methods give the reversed result for the reversed members.
    fn assert_order_independent(elements: &[&Element]) {
        let forward = recorded(elements);
        let backward: Vec<_> = forward.iter().rev().cloned().collect();
        let (forward, backward) = (members(&forward), members(&backward));
        assert_eq!(
            GlassContainer.capture(&forward),
            GlassContainer.capture(&backward)
        );
        let mut reversed = GlassContainer.effects(&backward);
        reversed.reverse();
        assert_eq!(GlassContainer.effects(&forward), reversed);
    }

    #[test]
    fn effect_slots_match_the_packing_table() {
        let mut e = card();
        e.recipe = e.recipe.with_tint(Tint::flat([0.1, 0.2, 0.3], 0.4));
        let effect = effect(&e);
        let u = effect.uniforms();
        let r = &e.recipe;
        assert_eq!(u[slot::PRIMARY_SHIFT], r.primary_shift);
        assert_eq!(u[slot::PRIMARY_DEPTH], r.primary_depth);
        assert_eq!(u[slot::BLUR_DEPTH_0], r.blur_depths[0]);
        assert_eq!(u[slot::CAPTURE_SCALE], r.capture_scale);
        assert_eq!(u[slot::TINT], [0.1, 0.2, 0.3, 0.4]);
        let rect = e.shape.rect;
        assert_eq!(u[slot::RECT], [rect.x, rect.y, rect.w, rect.h]);
        assert_eq!(u[slot::ELLIPSE_BEND], r.ellipse_bend);
        assert_eq!(u[slot::TINT_FLAG], 1.0);
    }

    #[test]
    fn a_mixed_pair_takes_the_largest_recipe_and_the_smallest_primary_lobe() {
        let (circle, card) = (circle(), card());
        assert!(card.recipe.capture_scale < circle.recipe.capture_scale);
        let recorded = recorded(&[&circle, &card]);
        let members = members(&recorded);
        let effects = GlassContainer.effects(&members);

        // The card's capture scale (0.25), in the container's class.
        assert_eq!(
            GlassContainer.capture(&members),
            capture_class(true, card.recipe.capture_scale)
        );
        // Both members carry the circle's primary lobe (−30, 15) and blur
        // depth 0, and the card's every other value.
        for (own, shared) in [&circle, &card].into_iter().zip(&effects) {
            let u = shared.uniforms();
            assert_eq!(u[slot::PRIMARY_SHIFT], -30.0);
            assert_eq!(u[slot::PRIMARY_DEPTH], 15.0);
            assert_eq!(u[slot::BLUR_DEPTH_0], circle.recipe.blur_depths[0]);
            assert_eq!(u[slot::CAPTURE_SCALE], card.recipe.capture_scale);
            assert_eq!(u[slot::BLUR_DEPTH_0 + 1], card.recipe.blur_depths[1]);
            assert_eq!(u[12], card.recipe.blur_gains[0]);
            assert_eq!(u[48], card.recipe.cast_density);
            assert_eq!(shared.outer_extent(), effect(&card).outer_extent());
            // Per member: its own rect.
            let rect = own.shape.rect;
            assert_eq!(u[slot::RECT], [rect.x, rect.y, rect.w, rect.h]);
        }
        // The same terms the standalone renderer's `Recipe::for_container`
        // gives, packed by the standalone packing.
        assert_eq!(effects[0], in_container(&circle, &card, &circle));
        assert_eq!(effects[1], in_container(&card, &card, &circle));
    }

    #[test]
    fn a_member_keeps_its_own_tint_and_ellipse_bend() {
        let mut tinted = circle();
        tinted.recipe = tinted.recipe.with_tint(Tint::flat([0.9, 0.5, 0.1], 0.7));
        let mut card = card();
        card.recipe.ellipse_bend = 0.5;
        let recorded = recorded(&[&tinted, &card]);
        let effects = GlassContainer.effects(&members(&recorded));
        assert_eq!(effects[0], in_container(&tinted, &card, &tinted));
        assert_eq!(effects[1], in_container(&card, &card, &tinted));
        assert_eq!(effects[0].uniforms()[slot::TINT_FLAG], 1.0);
        assert_eq!(effects[1].uniforms()[slot::TINT_FLAG], 0.0);
        assert_eq!(effects[0].uniforms()[slot::ELLIPSE_BEND], 0.0);
        assert_eq!(effects[1].uniforms()[slot::ELLIPSE_BEND], 0.5);
    }

    #[test]
    fn equal_members_reproduce_the_standalone_recipe() {
        let a = element(Shape::capsule(Rect::new(0.0, 0.0, 100.0, 40.0)));
        let b = element(Shape::capsule(Rect::new(0.0, 56.0, 100.0, 40.0)));
        let recorded = recorded(&[&a, &b]);
        let members = members(&recorded);
        assert_eq!(
            GlassContainer.effects(&members),
            vec![effect(&a), effect(&b)]
        );
        assert_eq!(
            GlassContainer.capture(&members),
            capture_class(true, a.recipe.capture_scale)
        );
    }

    #[test]
    fn the_members_order_changes_nothing() {
        let capsule = element(Shape::capsule(Rect::new(0.0, 0.0, 160.0, 80.0)));
        assert_order_independent(&[&circle(), &card(), &capsule]);
    }

    #[test]
    fn equal_sizes_with_different_recipes_resolve_by_content_not_order() {
        let regular = element(Shape::circle(30.0, 30.0, 30.0));
        let clear = Element {
            shape: Shape::circle(100.0, 30.0, 30.0),
            recipe: Recipe::for_size(
                60.0,
                Family::Clear,
                Appearance::Dark,
                Translucency::default(),
            ),
        };
        assert_order_independent(&[&regular, &clear]);
    }

    #[test]
    fn the_registry_holds_the_container_recipe_and_the_class_it_names() {
        let mut registry = MaterialRegistry::new();
        register(&mut registry);
        let (circle, card) = (circle(), card());
        let recorded = recorded(&[&circle, &card]);
        let class = registry
            .container_recipe(GLASS_SHADER)
            .expect("registered")
            .capture(&members(&recorded));
        let capture = registry.capture_class(class).expect("a registered class");
        assert!(matches!(capture.grouping, MaterialGrouping::Union { .. }));
    }
}
