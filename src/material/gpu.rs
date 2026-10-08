//! The wgpu implementation of the glass material.
//!
//! [`GlassRenderer`] takes a backdrop texture and a [`Scene`] of groups (each
//! group shares one union field) and composites the glass over the backdrop.
//! Groups composite in order, and each one's backdrop is the composite as
//! the groups before it left it. Per group it runs, in order: the
//! composite's mip levels regenerated over the group's region, the backdrop
//! capture (a lod-prefiltered downsample of the composite, then a
//! box-reduction mip chain), the field pass, the material pass (refraction,
//! depth blur, haze, luminance limit, grade, ambient pickup, contour shade
//! and cast shadow, border correction, channel ceiling, highlight and tint
//! siblings)
//! and optionally the foreground dispersion pass.
//!
//! Textures hold premultiplied, sRGB-encoded values in `Rgba16Float`; the
//! shaders never convert to linear light, as the specification's colour
//! math is defined on encoded values.

use super::geometry::{
    CONTINUOUS_EXPONENT, CornerCurve, Element, SMOOTHING_TO_TAU, STANDALONE_UNION_SMOOTHING,
};
use super::recipe::{Recipe, level};
use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

/// Format of the composite and output textures.
pub const COMPOSITE_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;
/// Format of the field texture: `(d, n.x, n.y, owner)`.
pub const FIELD_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba32Float;
/// Cap of the capture chain's mip levels. **[chosen]**
///
/// The chain runs one level per halving until the capture is down to one
/// texel on its long side, or this many levels, whichever comes first.
pub const CAPTURE_MIP_LEVELS: u32 = 9;

/// Rotation of the foreground aberration axis relative to the surface
/// normal, radians — §4.14 leaves the dispersion parameters open.
/// **[chosen]**
pub const DISPERSION_AXIS_ANGLE: f32 = 0.35;
/// Width of the seven spectral taps at the silhouette, pt — §4.14 leaves
/// the dispersion parameters open. **[chosen: one dominant ~1–2 px fringe
/// per side at 3 px/pt]**
pub const DISPERSION_WIDTH_PT: f32 = 0.6;
/// Foreground edge fade band, pt — the §4.14 fade's parameters are open
/// there. **[chosen]**
pub const FOREGROUND_FADE_PT: f32 = 1.5;

/// A set of elements sharing one union field.
#[derive(Clone, Debug, PartialEq)]
pub struct Group {
    /// The members, in ownership-index order.
    pub members: Vec<Element>,
    /// Union smoothing, pt: 8 standalone, 30 for a container.
    pub smoothing: f32,
}

impl Group {
    /// A standalone element.
    #[must_use]
    pub fn single(element: Element) -> Self {
        Self {
            members: vec![element],
            smoothing: STANDALONE_UNION_SMOOTHING,
        }
    }

    /// A container over `members` with the given smoothing.
    #[must_use]
    pub const fn container(members: Vec<Element>, smoothing: f32) -> Self {
        Self { members, smoothing }
    }
}

/// Everything the renderer needs for one frame.
#[derive(Clone, Debug, PartialEq)]
pub struct Scene {
    /// Scene size in pt; the backdrop texture covers exactly this.
    pub size_pt: [f32; 2],
    /// Output pixels per pt.
    pub px_per_pt: f32,
    /// Groups, composited in order.
    pub groups: Vec<Group>,
    /// The display peak §4.11 clamps to — the display's maximum encoded
    /// channel value; [`Scene::new`] defaults it to the SDR peak 1.0.
    pub display_peak: f32,
}

impl Scene {
    /// A scene of `groups` on a `size_pt` pt canvas at `px_per_pt` output
    /// pixels per pt, at the default SDR display peak (1.0).
    #[must_use]
    pub const fn new(size_pt: [f32; 2], px_per_pt: f32, groups: Vec<Group>) -> Self {
        Self {
            size_pt,
            px_per_pt,
            groups,
            display_peak: 1.0,
        }
    }

    /// Output size in pixels.
    #[must_use]
    #[allow(
        clippy::cast_sign_loss,
        clippy::cast_possible_truncation,
        reason = "point sizes are positive and far below u32::MAX; `as` saturates, matching the clamp"
    )]
    pub fn size_px(&self) -> [u32; 2] {
        [
            (self.size_pt[0] * self.px_per_pt).round().max(1.0) as u32,
            (self.size_pt[1] * self.px_per_pt).round().max(1.0) as u32,
        ]
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
struct GpuElement {
    rect: [f32; 4],
    radii: [f32; 4],
    params: [f32; 4],
}

impl From<&Element> for GpuElement {
    fn from(e: &Element) -> Self {
        let s = &e.shape;
        Self {
            rect: [s.rect.x, s.rect.y, s.rect.w, s.rect.h],
            radii: s.radii,
            params: [
                match s.curve {
                    CornerCurve::Circular => 0.0,
                    CornerCurve::Continuous => 1.0,
                },
                e.recipe.ellipse_bend,
                0.0,
                0.0,
            ],
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
struct FieldUniforms {
    scene: [f32; 4],
    misc: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
struct CaptureParams {
    params: [f32; 4],
    /// Source uv rect (`origin`, `span`): identity for the tmp/mip steps,
    /// the group's region for the composite-sampling first step.
    rect: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
struct MaterialUniforms {
    /// Scene size pt, px per pt, this pass's capture max lod.
    scene: [f32; 4],
    /// This pass's capture scale, the display peak, the capture texel's
    /// size in pt along x and y.
    misc: [f32; 4],
    /// Region the material pass renders, pt: min.xy, size.xy.
    region: [f32; 4],
    /// Region origin in composite px (field texel offset).
    origin: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
struct ForegroundUniforms {
    scene: [f32; 4],
    params: [f32; 4],
}

/// The recipe as laid out for `shaders/material.wgsl`.
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct GpuRecipe {
    shifts: [f32; 4],
    rim: [f32; 4],
    blur_depths: [f32; 4],
    blur_gains: [f32; 4],
    haze: [f32; 4],
    limit_grade: [f32; 4],
    grade: [f32; 4],
    wash: [f32; 4],
    ambient: [f32; 4],
    ambient_misc: [f32; 4],
    ambient_grade: [f32; 4],
    contour: [f32; 4],
    contour_misc: [f32; 4],
    border_a: [f32; 4],
    border_b: [f32; 4],
    cast_a: [f32; 4],
    cast_b: [f32; 4],
    hl_a: [f32; 4],
    hl_b: [f32; 4],
    hl_bb: [f32; 4],
    hl_first: [f32; 4],
    hl_second: [f32; 4],
    tint: [f32; 4],
}

const fn flag(b: bool) -> f32 {
    if b { 1.0 } else { 0.0 }
}

impl From<&Recipe> for GpuRecipe {
    fn from(r: &Recipe) -> Self {
        let h = &r.highlight;
        let tint = r
            .tint
            .map_or([0.0; 4], |t| [t.rgb[0], t.rgb[1], t.rgb[2], t.alpha]);
        Self {
            shifts: [r.primary_shift, r.primary_depth, r.rim_shift, r.rim_depth],
            rim: [r.rim_ramp[0], r.rim_ramp[1], r.rim_mix, r.blur_radius],
            blur_depths: r.blur_depths,
            blur_gains: r.blur_gains,
            haze: [
                r.haze_radius,
                r.haze_min_weight,
                r.haze_max_weight,
                r.haze_mix,
            ],
            limit_grade: [r.luminance_limit, r.grade_mix, r.capture_scale, 0.0],
            grade: [r.grade.floor, r.grade.ceiling, r.grade.chroma, 0.0],
            wash: [r.wash[0], r.wash[1], r.wash[2], r.wash_alpha],
            ambient: [
                r.ambient_shift,
                r.ambient_depth,
                r.ambient_blur,
                r.ambient_strength,
            ],
            ambient_misc: [
                r.ambient_ramp[0],
                r.ambient_ramp[1],
                flag(r.ambient_favours_light),
                0.0,
            ],
            ambient_grade: [
                r.ambient_grade.floor,
                r.ambient_grade.ceiling,
                r.ambient_grade.chroma,
                0.0,
            ],
            contour: [
                r.contour_shade_density,
                r.contour_shade_shift,
                r.contour_shade_softness,
                r.contour_shade_width,
            ],
            contour_misc: [r.contour_shade_clip, 0.0, 0.0, 0.0],
            border_a: [
                r.border_strength,
                r.border_bearing,
                r.border_band_width,
                r.border_band_offset,
            ],
            border_b: [r.border_aperture, r.border_colour_bias, 0.0, 0.0],
            cast_a: [
                r.cast_shift[0],
                r.cast_shift[1],
                r.cast_falloff,
                r.cast_density,
            ],
            cast_b: [r.cast_gain, r.exterior_span, r.channel_ceiling, 0.0],
            hl_a: [h.inner_fade, h.first_gain, h.second_gain, h.aperture],
            hl_b: [h.first_bearing, h.second_bearing, h.knee, 0.0],
            hl_bb: [
                h.glow_gain,
                h.glow_width,
                h.glow_aperture,
                flag(r.tint.is_some()),
            ],
            hl_first: h.first_colour,
            hl_second: h.second_colour,
            tint,
        }
    }
}

const COMMON_WGSL: &str = include_str!("../../shaders/common.wgsl");
const CAPTURE_WGSL: &str = include_str!("../../shaders/capture.wgsl");
const FIELD_WGSL: &str = include_str!("../../shaders/field.wgsl");
const MATERIAL_WGSL: &str = include_str!("../../shaders/material.wgsl");
const FOREGROUND_WGSL: &str = include_str!("../../shaders/foreground.wgsl");

fn shader(device: &wgpu::Device, label: &str, body: &str) -> wgpu::ShaderModule {
    device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some(label),
        source: wgpu::ShaderSource::Wgsl(format!("{COMMON_WGSL}\n{body}").into()),
    })
}

const fn texture_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Float { filterable: true },
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        },
        count: None,
    }
}

const fn unfilterable_texture_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Float { filterable: false },
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        },
        count: None,
    }
}

const fn sampler_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
        count: None,
    }
}

const fn uniform_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

const fn storage_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Storage { read_only: true },
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

fn pipeline(
    device: &wgpu::Device,
    label: &str,
    module: &wgpu::ShaderModule,
    entry: &str,
    layout: &wgpu::BindGroupLayout,
    format: wgpu::TextureFormat,
    blend: Option<wgpu::BlendState>,
) -> wgpu::RenderPipeline {
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some(label),
        bind_group_layouts: &[Some(layout)],
        immediate_size: 0,
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(label),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module,
            entry_point: Some("fullscreen_vertex"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            buffers: &[],
        },
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        fragment: Some(wgpu::FragmentState {
            module,
            entry_point: Some(entry),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend,
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        multiview_mask: None,
        cache: None,
    })
}

fn create_texture(
    device: &wgpu::Device,
    label: &str,
    size: [u32; 2],
    format: wgpu::TextureFormat,
    mip_level_count: u32,
) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d {
            width: size[0].max(1),
            height: size[1].max(1),
            depth_or_array_layers: 1,
        },
        mip_level_count,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::COPY_SRC
            | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    })
}

fn create_buffer(
    device: &wgpu::Device,
    label: &str,
    size: u64,
    usage: wgpu::BufferUsages,
) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size: size.max(16),
        usage,
        mapped_at_creation: false,
    })
}

fn init_buffer(
    device: &wgpu::Device,
    label: &str,
    contents: &[u8],
    usage: wgpu::BufferUsages,
) -> wgpu::Buffer {
    device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some(label),
        contents,
        usage,
    })
}

/// Renders glass scenes over a backdrop.
///
/// Every group's passes run on textures sized to the union of its member
/// bounds inflated by the farthest recipe reach (`group_region`), so cost
/// is proportional to the elements, not the screen. Capture, buffer and
/// bind-group objects are kept on [`GroupGpu`] across frames; each
/// `render` only rewrites buffer contents and re-runs the passes.
#[derive(Debug)]
pub struct GlassRenderer {
    capture_layout: wgpu::BindGroupLayout,
    field_layout: wgpu::BindGroupLayout,
    material_layout: wgpu::BindGroupLayout,
    foreground_layout: wgpu::BindGroupLayout,
    blit: wgpu::RenderPipeline,
    downsample: wgpu::RenderPipeline,
    field: wgpu::RenderPipeline,
    material: wgpu::RenderPipeline,
    foreground: wgpu::RenderPipeline,
    sampler: wgpu::Sampler,
    /// Constant `CaptureParams` for plain blits (identity rect).
    blit_params: wgpu::Buffer,
    /// The scene-sized accumulation composite (mipmapped so capture's first
    /// step can prefilter the footprint by lod), its all-mips view, the
    /// per-level views and the bind groups that mip `k - 1` -> `k`.
    composite: Option<wgpu::Texture>,
    composite_view: Option<wgpu::TextureView>,
    composite_mip_views: Vec<wgpu::TextureView>,
    composite_mip_bgs: Vec<wgpu::BindGroup>,
    /// Per-group GPU objects, parallel to the last scene's groups.
    groups: Vec<Option<GroupGpu>>,
    /// Owned copy of the caller's foreground content so the foreground
    /// bind groups can persist across frames.
    fg_input: Option<(wgpu::Texture, wgpu::TextureView)>,
    /// Backdrop -> composite blit bind group, keyed on the caller's texture
    /// so it is rebuilt only when the source changes.
    backdrop_blit: Option<(wgpu::Texture, wgpu::BindGroup)>,
    /// Foreground -> `fg_input` blit bind group, likewise keyed.
    fg_blit: Option<(wgpu::Texture, wgpu::BindGroup)>,
}

/// The textures, parameter buffers and bind groups of one capture chain.
/// All of it is kept across frames — only the contents are redrawn.
#[derive(Debug)]
struct Capture {
    /// All-mips view for the material pass.
    view: wgpu::TextureView,
    max_lod: f32,
    /// One capture texel in pt along x and y: `region_pt` over the
    /// capture's `ceil(region_pt · px_per_pt · scale)` texels per axis.
    texel_pt: [f32; 2],
    /// One `CaptureParams` buffer per pass step: step 0 downsamples the
    /// composite into level 0, the rest box-reduce the previous level.
    params: Vec<wgpu::Buffer>,
    /// Pass targets in submission order: mip 0..levels.
    targets: Vec<wgpu::TextureView>,
    /// Bind groups for steps 1..; each samples `targets[step - 1]`, this
    /// capture's own texture, so they persist across frames.
    chain_bgs: Vec<wgpu::BindGroup>,
    /// Step-0 bind group: samples the renderer's persistent composite.
    src_bg: Option<wgpu::BindGroup>,
}

/// Per-group GPU objects kept across frames: the field, patch and capture
/// textures are sized to the group's bounds plus reach, so a small element
/// never pays a full-screen pass.
#[derive(Debug)]
struct GroupGpu {
    region_px: [u32; 4],
    /// Member capacity of the storage buffers; enlarged on demand.
    capacity: usize,
    field_tex: wgpu::Texture,
    field_view: wgpu::TextureView,
    /// Material result, region-sized.
    patch_a: wgpu::Texture,
    patch_a_view: wgpu::TextureView,
    /// Foreground result, region-sized.
    patch_b: wgpu::Texture,
    patch_b_view: wgpu::TextureView,
    element_buf: wgpu::Buffer,
    recipe_buf: wgpu::Buffer,
    lobe_buf: wgpu::Buffer,
    field_uni: wgpu::Buffer,
    /// One material uniform buffer per distinct capture scale: the
    /// scale-specific values (`scene.w`, `misc.x`, `misc.zw`) differ per
    /// pass, and a single buffer written once per pass inside one encoder
    /// would leave every pass reading the last scale's values. The
    /// buffers are created on demand and never cleared: 64 bytes each,
    /// independent of region and scale set, rewritten before every pass —
    /// only the bind groups follow the region and the scale set.
    material_unis: Vec<wgpu::Buffer>,
    fg_uni: wgpu::Buffer,
    field_bg: wgpu::BindGroup,
    /// One material bind group per distinct capture scale in the group.
    material_bgs: Vec<wgpu::BindGroup>,
    fg_bg: Option<wgpu::BindGroup>,
    /// One capture (with its full mip chain) per distinct capture scale.
    captures: Vec<Capture>,
    /// The distinct member capture scales, in first-seen order.
    capture_scales: Vec<f32>,
}

/// The rect a group's passes run in, px: member bounds inflated by the
/// farthest reach of any recipe effect: the field bound
/// (`exterior_span`), the cast shadow (`2·cast_falloff +
/// |cast_shift|`), the contour shade (`2·contour_shade_softness +
/// contour_shade_shift + contour_shade_width`), the border
/// (`|border_band_offset| + border_band_width`), the ambient and refraction lobes
/// plus the blur footprint of the sample taken there (blur radius in
/// capture texels × `1/(capture_scale · px_per_pt)` pt — the ambient sample's own radius
/// for the ambient tap, `blur_radius · max blur gain` for the refraction
/// taps), and the haze taps (`haze_radius` +
/// `0.25·level(haze_radius)` texels × `1/(capture_scale · px_per_pt)` pt).
///
/// `1/(capture_scale · px_per_pt)` bounds the level-0 texel (`region_pt` over
/// `ceil(region_pt · px_per_pt · scale)` texels); mip texels can be slightly larger
/// (the chain halves by floor), which the reach ignores.
#[allow(
    clippy::cast_sign_loss,
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    reason = "region coordinates are clamped to the scene's pixel bounds, which fit f32 exactly"
)]
fn group_region(group: &Group, scene: &Scene) -> [u32; 4] {
    let mut min = [f32::INFINITY; 2];
    let mut max = [f32::NEG_INFINITY; 2];
    let mut reach = 0.0f32;
    for e in &group.members {
        let r = &e.recipe;
        let off = r.cast_shift[0].hypot(r.cast_shift[1]);
        // The capture's exact texel pt (`region_pt` over
        // `ceil(region_pt · px_per_pt · scale)` texels) isn't known until
        // this region is computed; `ceil` only enlarges the capture, so
        // `1/(scale · px_per_pt)` is a safe upper bound.
        let texel_pt = 1.0 / (r.capture_scale * scene.px_per_pt);
        let refraction_blur_pt =
            r.blur_radius * r.blur_gains.iter().copied().fold(0.0_f32, f32::max) * texel_pt;
        // §4.4's taps: the haze radius's footprint plus their
        // ±0.25·level(radius) texel offset.
        let haze_blur_pt = (0.25f32.mul_add(level(r.haze_radius), r.haze_radius)) * texel_pt;
        reach = reach
            .max(r.exterior_span)
            .max(2.0f32.mul_add(r.cast_falloff, off))
            .max(2.0f32.mul_add(
                r.contour_shade_softness,
                r.contour_shade_shift + r.contour_shade_width,
            ))
            .max(r.ambient_blur.mul_add(
                texel_pt,
                r.ambient_ramp[0].abs().max(r.ambient_ramp[1].abs()) + r.ambient_shift.abs(),
            ))
            .max(r.border_band_offset.abs() + r.border_band_width)
            .max(r.primary_shift.abs() + 1.0 + refraction_blur_pt)
            .max(r.rim_shift.abs() + 1.0 + refraction_blur_pt)
            .max(haze_blur_pt);
        let rect = &e.shape.rect;
        min[0] = min[0].min(rect.x);
        min[1] = min[1].min(rect.y);
        max[0] = max[0].max(rect.x + rect.w);
        max[1] = max[1].max(rect.y + rect.h);
    }
    let pp = scene.px_per_pt;
    let size = scene.size_px();
    let x0 = ((min[0] - reach) * pp).floor().max(0.0) as u32;
    let y0 = ((min[1] - reach) * pp).floor().max(0.0) as u32;
    let x1 = ((max[0] + reach) * pp).ceil().min(size[0] as f32) as u32;
    let y1 = ((max[1] + reach) * pp).ceil().min(size[1] as f32) as u32;
    [
        x0,
        y0,
        x1.saturating_sub(x0).max(1),
        y1.saturating_sub(y0).max(1),
    ]
}

/// A capture's level-0 size in texels: `region_pt` at `px_per_pt · scale`.
#[allow(
    clippy::cast_sign_loss,
    clippy::cast_possible_truncation,
    reason = "capture texel counts are positive and far below u32::MAX"
)]
fn capture_size(scene: &Scene, region_pt: [f32; 4], scale: f32) -> [u32; 2] {
    [
        (region_pt[2] * scene.px_per_pt * scale).ceil().max(1.0) as u32,
        (region_pt[3] * scene.px_per_pt * scale).ceil().max(1.0) as u32,
    ]
}

/// The composite mip level a capture's first step samples: the level at
/// which one composite texel covers the whole `region_px / capture_px`
/// footprint — a real prefilter, unlike box taps, which would alias a
/// periodic backdrop into moiré — clamped to `max_src_lod`.
#[allow(
    clippy::cast_precision_loss,
    reason = "capture texel counts are far below f32's exact integer range"
)]
fn capture_source_lod(scene: &Scene, region_pt: [f32; 4], scale: f32, max_src_lod: f32) -> f32 {
    let size = capture_size(scene, region_pt, scale);
    let stride = (region_pt[2] * scene.px_per_pt / size[0] as f32)
        .max(region_pt[3] * scene.px_per_pt / size[1] as f32);
    stride.log2().clamp(0.0, max_src_lod)
}

/// The texel spans `[lo, hi)` along one axis of composite mip levels
/// `1..=top` (index `k - 1`) that a capture of the level-0 span
/// `[lo0, hi0)` of a `len0`-texel axis depends on. Every level covers the
/// capture's bilinear footprint; every level below `top` also covers the
/// parents its upper neighbour's span reads, so each generated texel is
/// built from generated texels.
fn composite_mip_spans(lo0: u32, hi0: u32, len0: u32, top: u32) -> Vec<[u32; 2]> {
    let len = |k: u32| (len0 >> k).max(1);
    let footprint = [
        f64::from(lo0) / f64::from(len0),
        f64::from(hi0) / f64::from(len0),
    ];
    let mut spans = vec![[0; 2]; top as usize];
    for k in (1..=top).rev() {
        let mut span = bilinear_span(footprint, len(k));
        if k < top {
            // Level `k + 1` texel `i` samples level `k` at its centre's uv.
            let [a, b] = spans[k as usize];
            let above = f64::from(len(k + 1));
            let parents = bilinear_span(
                [(f64::from(a) + 0.5) / above, (f64::from(b) - 0.5) / above],
                len(k),
            );
            span = [span[0].min(parents[0]), span[1].max(parents[1])];
        }
        spans[k as usize - 1] = span;
    }
    spans
}

/// The texels `[lo, hi)` of a `len`-texel axis that bilinear samples at uv
/// in `[uv[0], uv[1]]` read.
#[allow(
    clippy::cast_sign_loss,
    clippy::cast_possible_truncation,
    reason = "both ends are clamped to [0, len] before the cast"
)]
fn bilinear_span(uv: [f64; 2], len: u32) -> [u32; 2] {
    let len = f64::from(len);
    let lo = uv[0].mul_add(len, -0.5).floor().max(0.0);
    let hi = (uv[1].mul_add(len, -0.5).floor() + 2.0).min(len);
    [lo as u32, hi as u32]
}

impl GroupGpu {
    fn new(
        device: &wgpu::Device,
        field_layout: &wgpu::BindGroupLayout,
        region_px: [u32; 4],
        capacity: usize,
    ) -> Self {
        let capacity = capacity.max(1);
        let field_tex = create_texture(
            device,
            "glass field",
            [region_px[2], region_px[3]],
            FIELD_FORMAT,
            1,
        );
        let field_view = field_tex.create_view(&wgpu::TextureViewDescriptor::default());
        let patch_a = create_texture(
            device,
            "glass patch a",
            [region_px[2], region_px[3]],
            COMPOSITE_FORMAT,
            1,
        );
        let patch_b = create_texture(
            device,
            "glass patch b",
            [region_px[2], region_px[3]],
            COMPOSITE_FORMAT,
            1,
        );
        let storage = wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST;
        let uniform = wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST;
        let element_buf = create_buffer(
            device,
            "glass members",
            capacity as u64 * size_of::<GpuElement>() as u64,
            storage,
        );
        let recipe_buf = create_buffer(
            device,
            "glass recipes",
            capacity as u64 * size_of::<GpuRecipe>() as u64,
            storage,
        );
        let lobe_buf = create_buffer(
            device,
            "glass foreground lobes",
            capacity as u64 * 16,
            storage,
        );
        let field_uni = create_buffer(
            device,
            "glass field uniforms",
            size_of::<FieldUniforms>() as u64,
            uniform,
        );
        let fg_uni = create_buffer(
            device,
            "glass foreground uniforms",
            size_of::<ForegroundUniforms>() as u64,
            uniform,
        );
        let field_bg = field_bind_group(field_layout, device, &field_uni, &element_buf);
        Self {
            region_px,
            capacity,
            field_view,
            patch_a_view: patch_a.create_view(&wgpu::TextureViewDescriptor::default()),
            patch_b_view: patch_b.create_view(&wgpu::TextureViewDescriptor::default()),
            field_tex,
            patch_a,
            patch_b,
            element_buf,
            recipe_buf,
            lobe_buf,
            field_uni,
            material_unis: Vec::new(),
            fg_uni,
            field_bg,
            material_bgs: Vec::new(),
            fg_bg: None,
            captures: Vec::new(),
            capture_scales: Vec::new(),
        }
    }

    /// Region moved or resized: field and patch textures are region-sized,
    /// so they and every bind group viewing them are rebuilt.
    fn set_region(&mut self, device: &wgpu::Device, region_px: [u32; 4]) {
        if self.region_px == region_px {
            return;
        }
        self.region_px = region_px;
        let size = [region_px[2], region_px[3]];
        self.field_tex = create_texture(device, "glass field", size, FIELD_FORMAT, 1);
        self.field_view = self
            .field_tex
            .create_view(&wgpu::TextureViewDescriptor::default());
        self.patch_a = create_texture(device, "glass patch a", size, COMPOSITE_FORMAT, 1);
        self.patch_a_view = self
            .patch_a
            .create_view(&wgpu::TextureViewDescriptor::default());
        self.patch_b = create_texture(device, "glass patch b", size, COMPOSITE_FORMAT, 1);
        self.patch_b_view = self
            .patch_b
            .create_view(&wgpu::TextureViewDescriptor::default());
        self.captures.clear();
        self.capture_scales.clear();
        self.material_bgs.clear();
        self.fg_bg = None;
    }

    /// The member count grew past the storage buffers: enlarge them and
    /// rebuild the bind groups that point at them.
    fn ensure_capacity(
        &mut self,
        device: &wgpu::Device,
        field_layout: &wgpu::BindGroupLayout,
        capacity: usize,
    ) {
        if capacity <= self.capacity {
            return;
        }
        self.capacity = capacity;
        let storage = wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST;
        self.element_buf = create_buffer(
            device,
            "glass members",
            capacity as u64 * size_of::<GpuElement>() as u64,
            storage,
        );
        self.recipe_buf = create_buffer(
            device,
            "glass recipes",
            capacity as u64 * size_of::<GpuRecipe>() as u64,
            storage,
        );
        self.lobe_buf = create_buffer(
            device,
            "glass foreground lobes",
            capacity as u64 * 16,
            storage,
        );
        self.field_bg = field_bind_group(field_layout, device, &self.field_uni, &self.element_buf);
        self.material_bgs.clear();
        self.fg_bg = None;
    }
}

fn field_bind_group(
    field_layout: &wgpu::BindGroupLayout,
    device: &wgpu::Device,
    field_uni: &wgpu::Buffer,
    element_buf: &wgpu::Buffer,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("glass field bind group"),
        layout: field_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: field_uni.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: element_buf.as_entire_binding(),
            },
        ],
    })
}

impl GlassRenderer {
    /// Compiles the material's pipelines.
    #[must_use]
    #[allow(
        clippy::too_many_lines,
        reason = "a flat sequence of pipeline/layout/buffer constructions; splitting adds indirection without structure"
    )]
    pub fn new(device: &wgpu::Device) -> Self {
        let capture_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("glass capture layout"),
            entries: &[texture_entry(0), sampler_entry(1), uniform_entry(2)],
        });
        let field_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("glass field layout"),
            entries: &[uniform_entry(0), storage_entry(1)],
        });
        let material_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("glass material layout"),
            entries: &[
                uniform_entry(0),
                storage_entry(1),
                unfilterable_texture_entry(2),
                texture_entry(3),
                texture_entry(4),
                sampler_entry(5),
            ],
        });
        let foreground_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("glass foreground layout"),
            entries: &[
                uniform_entry(0),
                storage_entry(1),
                unfilterable_texture_entry(2),
                texture_entry(3),
                sampler_entry(4),
                texture_entry(5),
            ],
        });

        let capture_module = shader(device, "glass capture", CAPTURE_WGSL);
        let field_module = shader(device, "glass field", FIELD_WGSL);
        let material_module = shader(device, "glass material", MATERIAL_WGSL);
        let foreground_module = shader(device, "glass foreground", FOREGROUND_WGSL);

        let blit = pipeline(
            device,
            "glass blit",
            &capture_module,
            "blit",
            &capture_layout,
            COMPOSITE_FORMAT,
            None,
        );
        let downsample = pipeline(
            device,
            "glass downsample",
            &capture_module,
            "downsample_box",
            &capture_layout,
            COMPOSITE_FORMAT,
            None,
        );
        let field = pipeline(
            device,
            "glass field",
            &field_module,
            "field_fragment",
            &field_layout,
            FIELD_FORMAT,
            None,
        );
        let material = pipeline(
            device,
            "glass material",
            &material_module,
            "material_fragment",
            &material_layout,
            COMPOSITE_FORMAT,
            None,
        );
        let foreground = pipeline(
            device,
            "glass foreground",
            &foreground_module,
            "foreground_fragment",
            &foreground_layout,
            COMPOSITE_FORMAT,
            None,
        );
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("glass linear clamp"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            lod_min_clamp: 0.0,
            lod_max_clamp: 32.0,
            ..Default::default()
        });
        Self {
            capture_layout,
            field_layout,
            material_layout,
            foreground_layout,
            blit,
            downsample,
            field,
            material,
            foreground,
            sampler,
            blit_params: init_buffer(
                device,
                "glass blit params",
                bytemuck::bytes_of(&CaptureParams {
                    params: [0.0; 4],
                    rect: [0.0, 0.0, 1.0, 1.0],
                }),
                wgpu::BufferUsages::UNIFORM,
            ),
            composite: None,
            composite_view: None,
            composite_mip_views: Vec::new(),
            composite_mip_bgs: Vec::new(),
            groups: Vec::new(),
            fg_input: None,
            backdrop_blit: None,
            fg_blit: None,
        }
    }

    #[allow(
        clippy::cast_precision_loss,
        reason = "mip dimensions are powers of two far below f32's exact range"
    )]
    fn ensure_targets(&mut self, device: &wgpu::Device, size: [u32; 2]) {
        if let Some(t) = &self.composite
            && t.size().width == size[0]
            && t.size().height == size[1]
        {
            return;
        }
        // Every cached object views the composite: all of it goes.
        self.groups.clear();
        self.fg_input = None;
        // One mip level per halving, stopping once the capture is down to
        // one texel on its long side, capped at CAPTURE_MIP_LEVELS. The
        // capture's first step samples the composite at
        // `lod = log2(footprint)` for hardware prefiltering.
        let full_levels = 32 - size[0].max(size[1]).leading_zeros();
        let levels = full_levels.clamp(1, CAPTURE_MIP_LEVELS);
        let composite = create_texture(device, "glass composite", size, COMPOSITE_FORMAT, levels);
        let view = composite.create_view(&wgpu::TextureViewDescriptor::default());
        self.composite_mip_views = (0..levels)
            .map(|level| {
                composite.create_view(&wgpu::TextureViewDescriptor {
                    base_mip_level: level,
                    mip_level_count: Some(1),
                    ..Default::default()
                })
            })
            .collect();
        self.composite_mip_bgs = (1..levels)
            .map(|level| {
                let w = (size[0] >> (level - 1)).max(1) as f32;
                let h = (size[1] >> (level - 1)).max(1) as f32;
                let params = init_buffer(
                    device,
                    "glass mip params",
                    bytemuck::bytes_of(&CaptureParams {
                        params: [1.0 / w, 1.0 / h, 0.0, 0.0],
                        rect: [0.0, 0.0, 1.0, 1.0],
                    }),
                    wgpu::BufferUsages::UNIFORM,
                );
                Self::capture_bind_group(
                    device,
                    &self.capture_layout,
                    &self.sampler,
                    &self.composite_mip_views[(level - 1) as usize],
                    &params,
                )
            })
            .collect();
        self.composite = Some(composite);
        self.composite_view = Some(view);
    }

    fn fullscreen_pass(
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        pipeline: &wgpu::RenderPipeline,
        bind_group: &wgpu::BindGroup,
        load: wgpu::LoadOp<wgpu::Color>,
        scissor: Option<[u32; 4]>,
    ) {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("glass pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, bind_group, &[]);
        if let Some([x, y, w, h]) = scissor {
            pass.set_scissor_rect(x, y, w, h);
        }
        pass.draw(0..3, 0..1);
    }

    /// Regenerates composite mip levels `1..=top` over `region` (level-0
    /// px) from the composite as it stands now. Each pass clears its level
    /// and generates only the span a capture of that region samples plus
    /// the parents the level above reads (`composite_mip_spans`), so the
    /// shading cost scales with the region, not the screen.
    fn generate_composite_mips(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        region: [u32; 4],
        top: u32,
    ) {
        let size = self.composite.as_ref().expect("targets").size();
        let xs = composite_mip_spans(region[0], region[0] + region[2], size.width, top);
        let ys = composite_mip_spans(region[1], region[1] + region[3], size.height, top);
        for (((view, bg), x), y) in self.composite_mip_views[1..]
            .iter()
            .zip(&self.composite_mip_bgs)
            .zip(xs)
            .zip(ys)
        {
            Self::fullscreen_pass(
                encoder,
                view,
                &self.blit,
                bg,
                wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                Some([x[0], y[0], x[1] - x[0], y[1] - y[0]]),
            );
        }
    }

    fn capture_bind_group(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        sampler: &wgpu::Sampler,
        source: &wgpu::TextureView,
        params: &wgpu::Buffer,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("glass capture bind group"),
            layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(source),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: params.as_entire_binding(),
                },
            ],
        })
    }

    /// Builds one capture chain's textures, param buffers and chain bind
    /// groups. The chain covers `region_pt` of the composite at `scale`.
    // Free function (not a method): the caller holds a `&mut` borrow of the
    // per-group struct while `self` fields are read here.
    #[allow(
        clippy::cast_precision_loss,
        reason = "capture texel counts and mip levels are small integers; region uv math needs f32"
    )]
    fn build_capture(
        device: &wgpu::Device,
        capture_layout: &wgpu::BindGroupLayout,
        sampler: &wgpu::Sampler,
        scene: &Scene,
        region_pt: [f32; 4],
        scale: f32,
        max_src_lod: f32,
    ) -> Capture {
        let size = capture_size(scene, region_pt, scale);
        // One level per halving, down to one texel on the long side.
        let full_levels = 32 - size[0].max(size[1]).leading_zeros();
        let levels = full_levels.clamp(1, CAPTURE_MIP_LEVELS);
        let texture = create_texture(device, "glass capture", size, COMPOSITE_FORMAT, levels);
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

        let mut targets = Vec::with_capacity(levels as usize);
        for level in 0..levels {
            targets.push(texture.create_view(&wgpu::TextureViewDescriptor {
                base_mip_level: level,
                mip_level_count: Some(1),
                ..Default::default()
            }));
        }

        // One param buffer per pass step; the first step maps the capture's
        // uv space onto the composite's region rect, the rest are identity.
        let scene_pt = scene.size_pt;
        let rect = [
            region_pt[0] / scene_pt[0],
            region_pt[1] / scene_pt[1],
            region_pt[2] / scene_pt[0],
            region_pt[3] / scene_pt[1],
        ];
        let mut params = Vec::with_capacity(levels as usize);
        let mut push = |params_v: [f32; 4], rect_v: [f32; 4]| {
            params.push(init_buffer(
                device,
                "glass capture params",
                bytemuck::bytes_of(&CaptureParams {
                    params: params_v,
                    rect: rect_v,
                }),
                wgpu::BufferUsages::UNIFORM,
            ));
        };
        let identity = [0.0, 0.0, 1.0, 1.0];
        // Step 0 is a lod-filtered blit of the composite mip chain
        // (`capture_source_lod`); the rest of the chain is the spec's box
        // 2x reduction.
        push(
            [
                0.0,
                0.0,
                capture_source_lod(scene, region_pt, scale, max_src_lod),
                0.0,
            ],
            rect,
        );
        for level in 1..levels {
            let w = (size[0] >> (level - 1)).max(1) as f32;
            let h = (size[1] >> (level - 1)).max(1) as f32;
            push([1.0 / w, 1.0 / h, 0.0, 0.0], identity);
        }

        // Steps 1.. sample this capture's own textures: bind once, reuse
        // every frame.
        let chain_bgs = (1..targets.len())
            .map(|step| {
                Self::capture_bind_group(
                    device,
                    capture_layout,
                    sampler,
                    &targets[step - 1],
                    &params[step],
                )
            })
            .collect();
        Capture {
            view,
            max_lod: (levels - 1) as f32,
            texel_pt: [region_pt[2] / size[0] as f32, region_pt[3] / size[1] as f32],
            params,
            targets,
            chain_bgs,
            src_bg: None,
        }
    }

    /// Renders `scene` over `backdrop` (a filterable 2D texture covering the
    /// scene; premultiplied, encoded values). `foreground`, if given, is
    /// content drawn inside the glass elements, run through the dispersion
    /// pass and blended on top. Returns the output texture, which stays
    /// valid until the next call.
    ///
    /// Each group's passes run on the region of the union of its member
    /// bounds inflated by the farthest reach of its recipes
    /// (`group_region`), so cost scales with the elements, not the
    /// screen. Textures, buffers and bind groups are kept across frames
    /// and only rewritten with the new frame's contents.
    ///
    /// # Panics
    /// If called before `ensure_targets` produced the composite (internal).
    pub fn render(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        backdrop: &wgpu::TextureView,
        scene: &Scene,
        foreground: Option<&wgpu::TextureView>,
    ) -> &wgpu::Texture {
        let size = scene.size_px();
        self.ensure_targets(device, size);
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("glass frame"),
        });

        // Bring the backdrop into the composite. The bind group is reused
        // while the caller passes the same source texture.
        let backdrop_key = backdrop.texture().clone();
        let bg = match &self.backdrop_blit {
            Some((tex, bg)) if *tex == backdrop_key => bg.clone(),
            _ => {
                let bg = Self::capture_bind_group(
                    device,
                    &self.capture_layout,
                    &self.sampler,
                    backdrop,
                    &self.blit_params,
                );
                self.backdrop_blit = Some((backdrop_key, bg.clone()));
                bg
            }
        };
        {
            Self::fullscreen_pass(
                &mut encoder,
                &self.composite_mip_views[0],
                &self.blit,
                &bg,
                wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                None,
            );
        }

        // Copy the caller's foreground into an owned texture so the
        // foreground bind groups survive across frames.
        if let Some(content) = foreground {
            if self.fg_input.is_none() {
                let tex =
                    create_texture(device, "glass foreground input", size, COMPOSITE_FORMAT, 1);
                let view = tex.create_view(&wgpu::TextureViewDescriptor::default());
                self.fg_input = Some((tex, view));
            }
            let fg_view = &self.fg_input.as_ref().expect("fg input").1;
            let fg_key = content.texture().clone();
            let bg = match &self.fg_blit {
                Some((tex, bg)) if *tex == fg_key => bg.clone(),
                _ => {
                    let bg = Self::capture_bind_group(
                        device,
                        &self.capture_layout,
                        &self.sampler,
                        content,
                        &self.blit_params,
                    );
                    self.fg_blit = Some((fg_key, bg.clone()));
                    bg
                }
            };
            Self::fullscreen_pass(
                &mut encoder,
                fg_view,
                &self.blit,
                &bg,
                wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                None,
            );
        }

        if self.groups.len() != scene.groups.len() {
            self.groups.clear();
            self.groups.resize_with(scene.groups.len(), || None);
        }

        for (gi, group) in scene.groups.iter().enumerate() {
            if group.members.is_empty() {
                continue;
            }
            self.render_group(
                device,
                queue,
                &mut encoder,
                scene,
                gi,
                group,
                foreground.is_some(),
            );
        }

        queue.submit(Some(encoder.finish()));
        self.composite.as_ref().expect("targets")
    }

    /// Renders one group: union of member bounds plus reach, region-local
    /// field, capture, material and optional foreground passes, then a raw
    /// copy of the finished patch back into the composite.
    #[allow(
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::too_many_arguments,
        clippy::too_many_lines,
        reason = "region pixel coordinates and element counts are small; uniforms need f32; source lods are clamped to [0, CAPTURE_MIP_LEVELS - 1]; the group pass is a flat pass sequence needing the device, queue, encoder and scene"
    )]
    fn render_group(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        scene: &Scene,
        gi: usize,
        group: &crate::material::Group,
        foreground: bool,
    ) {
        let region = group_region(group, scene);
        let pp = scene.px_per_pt;
        let region_pt = [
            region[0] as f32 / pp,
            region[1] as f32 / pp,
            region[2] as f32 / pp,
            region[3] as f32 / pp,
        ];
        // One capture with a full mip chain per distinct member capture
        // scale (§4.1).
        let mut scales: Vec<f32> = Vec::new();
        for e in &group.members {
            let sc = e.recipe.capture_scale;
            if scales.iter().all(|v| (*v - sc).abs() > 1e-4) {
                scales.push(sc);
            }
        }
        // The captures read the composite's mips, which must show the
        // backdrop as it stands now — including every group drawn before
        // this one — just as the material's direct read of level 0 does.
        // Only the levels the captures' first steps sample are rebuilt.
        let max_src_lod = (self.composite_mip_views.len() - 1) as f32;
        let top = scales
            .iter()
            .map(|&scale| capture_source_lod(scene, region_pt, scale, max_src_lod).ceil() as u32)
            .max()
            .expect("a non-empty group has a capture scale");
        self.generate_composite_mips(encoder, region, top);
        let g = self.groups[gi].get_or_insert_with(|| {
            GroupGpu::new(device, &self.field_layout, region, group.members.len())
        });
        g.set_region(device, region);
        g.ensure_capacity(device, &self.field_layout, group.members.len());

        // Per-frame contents: members, field uniforms, recipes.
        let elements: Vec<GpuElement> = group.members.iter().map(GpuElement::from).collect();
        queue.write_buffer(&g.element_buf, 0, bytemuck::cast_slice(&elements));
        queue.write_buffer(
            &g.field_uni,
            0,
            bytemuck::bytes_of(&FieldUniforms {
                scene: [
                    scene.size_pt[0],
                    scene.size_pt[1],
                    scene.px_per_pt,
                    (group.smoothing * SMOOTHING_TO_TAU).max(1e-3),
                ],
                misc: [
                    elements.len() as f32,
                    CONTINUOUS_EXPONENT,
                    region[0] as f32,
                    region[1] as f32,
                ],
            }),
        );

        if g.capture_scales != scales {
            g.captures.clear();
            g.material_bgs.clear();
        }
        g.capture_scales.clone_from(&scales);
        while g.captures.len() < scales.len() {
            let scale = scales[g.captures.len()];
            g.captures.push(Self::build_capture(
                device,
                &self.capture_layout,
                &self.sampler,
                scene,
                region_pt,
                scale,
                max_src_lod,
            ));
        }
        for cap in &mut g.captures {
            if cap.src_bg.is_none() {
                cap.src_bg = Some(Self::capture_bind_group(
                    device,
                    &self.capture_layout,
                    &self.sampler,
                    self.composite_view.as_ref().expect("targets"),
                    &cap.params[0],
                ));
            }
            for (step, target) in cap.targets.iter().enumerate() {
                let bg = if step == 0 {
                    cap.src_bg.as_ref().expect("src bg")
                } else {
                    &cap.chain_bgs[step - 1]
                };
                let pipe = if step == 0 {
                    &self.blit
                } else {
                    &self.downsample
                };
                Self::fullscreen_pass(
                    encoder,
                    target,
                    pipe,
                    bg,
                    wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    None,
                );
            }
        }

        // Field pass over the region.
        Self::fullscreen_pass(
            encoder,
            &g.field_view,
            &self.field,
            &g.field_bg,
            wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
            None,
        );

        // Material passes into patch_a, one per distinct capture scale: a
        // fragment samples the capture of its owning member's scale, and
        // is discarded while another scale's pass runs. The patch holds
        // the fully composited pixel for every region texel, so it is
        // copied back verbatim.
        let recipes: Vec<GpuRecipe> = group
            .members
            .iter()
            .map(|e| GpuRecipe::from(&e.recipe))
            .collect();
        queue.write_buffer(&g.recipe_buf, 0, bytemuck::cast_slice(&recipes));

        // One uniform buffer per distinct capture scale, created on
        // demand and never cleared — 64 bytes, dependent on neither
        // region nor member capacity, rewritten before every pass; only
        // the bind groups are rebuilt on region or scale changes.
        while g.material_unis.len() < scales.len() {
            g.material_unis.push(create_buffer(
                device,
                "glass material uniforms",
                size_of::<MaterialUniforms>() as u64,
                wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            ));
        }
        while g.material_bgs.len() < scales.len() {
            let i = g.material_bgs.len();
            g.material_bgs
                .push(device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("glass material bind group"),
                    layout: &self.material_layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: g.material_unis[i].as_entire_binding(),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: g.recipe_buf.as_entire_binding(),
                        },
                        wgpu::BindGroupEntry {
                            binding: 2,
                            resource: wgpu::BindingResource::TextureView(&g.field_view),
                        },
                        wgpu::BindGroupEntry {
                            binding: 3,
                            resource: wgpu::BindingResource::TextureView(
                                self.composite_view.as_ref().expect("targets"),
                            ),
                        },
                        wgpu::BindGroupEntry {
                            binding: 4,
                            resource: wgpu::BindingResource::TextureView(&g.captures[i].view),
                        },
                        wgpu::BindGroupEntry {
                            binding: 5,
                            resource: wgpu::BindingResource::Sampler(&self.sampler),
                        },
                    ],
                }));
        }
        for (i, scale) in scales.iter().enumerate() {
            queue.write_buffer(
                &g.material_unis[i],
                0,
                bytemuck::bytes_of(&MaterialUniforms {
                    scene: [
                        scene.size_pt[0],
                        scene.size_pt[1],
                        scene.px_per_pt,
                        g.captures[i].max_lod,
                    ],
                    misc: [
                        *scale,
                        scene.display_peak,
                        g.captures[i].texel_pt[0],
                        g.captures[i].texel_pt[1],
                    ],
                    region: region_pt,
                    origin: [region[0] as f32, region[1] as f32, 0.0, 0.0],
                }),
            );
            Self::fullscreen_pass(
                encoder,
                &g.patch_a_view,
                &self.material,
                &g.material_bgs[i],
                if i == 0 {
                    wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT)
                } else {
                    wgpu::LoadOp::Load
                },
                None,
            );
        }

        // Foreground dispersion into patch_b, then the used patch is
        // copied back into the composite at the region origin.
        let result = if foreground {
            let lobes: Vec<[f32; 4]> = group
                .members
                .iter()
                .map(|e| [e.recipe.primary_shift, e.recipe.primary_depth, 0.0, 0.0])
                .collect();
            queue.write_buffer(&g.lobe_buf, 0, bytemuck::cast_slice(&lobes));
            queue.write_buffer(
                &g.fg_uni,
                0,
                bytemuck::bytes_of(&ForegroundUniforms {
                    scene: [
                        scene.size_pt[0],
                        scene.size_pt[1],
                        scene.px_per_pt,
                        DISPERSION_AXIS_ANGLE,
                    ],
                    params: [
                        DISPERSION_WIDTH_PT,
                        FOREGROUND_FADE_PT,
                        region[0] as f32,
                        region[1] as f32,
                    ],
                }),
            );
            let fg_view = &self.fg_input.as_ref().expect("fg input").1;
            if g.fg_bg.is_none() {
                g.fg_bg = Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("glass foreground bind group"),
                    layout: &self.foreground_layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: g.fg_uni.as_entire_binding(),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: g.lobe_buf.as_entire_binding(),
                        },
                        wgpu::BindGroupEntry {
                            binding: 2,
                            resource: wgpu::BindingResource::TextureView(&g.field_view),
                        },
                        wgpu::BindGroupEntry {
                            binding: 3,
                            resource: wgpu::BindingResource::TextureView(fg_view),
                        },
                        wgpu::BindGroupEntry {
                            binding: 4,
                            resource: wgpu::BindingResource::Sampler(&self.sampler),
                        },
                        wgpu::BindGroupEntry {
                            binding: 5,
                            resource: wgpu::BindingResource::TextureView(&g.patch_a_view),
                        },
                    ],
                }));
            }
            Self::fullscreen_pass(
                encoder,
                &g.patch_b_view,
                &self.foreground,
                g.fg_bg.as_ref().expect("fg bg"),
                wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                None,
            );
            &g.patch_b
        } else {
            &g.patch_a
        };

        // Composite the group's region back: the patch is the fully
        // composited region, a raw copy.
        encoder.copy_texture_to_texture(
            wgpu::TexelCopyTextureInfo {
                texture: result,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyTextureInfo {
                texture: self.composite.as_ref().expect("targets"),
                mip_level: 0,
                origin: wgpu::Origin3d {
                    x: region[0],
                    y: region[1],
                    z: 0,
                },
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::Extent3d {
                width: region[2],
                height: region[3],
                depth_or_array_layers: 1,
            },
        );
    }
}

/// Reads a texture back as `f32` RGBA rows (`Rgba16Float` or `Rgba32Float`).
///
/// Verification only: no runtime render path may read the GPU back.
///
/// # Panics
/// On an unsupported format or a failed buffer map.
#[cfg(feature = "verification")]
#[must_use]
pub fn read_texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    texture: &wgpu::Texture,
) -> Vec<[f32; 4]> {
    let size = texture.size();
    let bytes_per_pixel = match texture.format() {
        wgpu::TextureFormat::Rgba16Float => 8,
        wgpu::TextureFormat::Rgba32Float => 16,
        other => panic!("unsupported readback format {other:?}"),
    };
    let unpadded = size.width * bytes_per_pixel;
    let padded =
        unpadded.div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT) * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("glass readback"),
        size: u64::from(padded) * u64::from(size.height),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    encoder.copy_texture_to_buffer(
        texture.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(padded),
                rows_per_image: Some(size.height),
            },
        },
        size,
    );
    queue.submit(Some(encoder.finish()));
    let slice = buffer.slice(..);
    let (tx, rx) = std::sync::mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |r| {
        tx.send(r).ok();
    });
    device
        .poll(wgpu::PollType::wait_indefinitely())
        .expect("poll");
    rx.recv().expect("map result").expect("map");
    let data = slice.get_mapped_range().expect("mapped range");
    let mut out = Vec::with_capacity((size.width * size.height) as usize);
    for row in 0..size.height {
        let start = (row * padded) as usize;
        let row_bytes = &data[start..start + unpadded as usize];
        match texture.format() {
            wgpu::TextureFormat::Rgba16Float => {
                for px in row_bytes.as_chunks::<8>().0 {
                    let f = |i: usize| half_to_f32(u16::from_le_bytes([px[i], px[i + 1]]));
                    out.push([f(0), f(2), f(4), f(6)]);
                }
            }
            _ => {
                for px in row_bytes.as_chunks::<16>().0 {
                    let f = |i: usize| f32::from_le_bytes([px[i], px[i + 1], px[i + 2], px[i + 3]]);
                    out.push([f(0), f(4), f(8), f(12)]);
                }
            }
        }
    }
    drop(data);
    buffer.unmap();
    out
}

/// Decodes an IEEE half-precision float.
#[cfg(feature = "verification")]
#[must_use]
pub fn half_to_f32(h: u16) -> f32 {
    let sign = if h & 0x8000 != 0 { -1.0 } else { 1.0 };
    let exp = i32::from((h >> 10) & 0x1f);
    let frac = f32::from(h & 0x3ff);
    match exp {
        0 => sign * frac * 2f32.powi(-24),
        31 => {
            if frac == 0.0 {
                sign * f32::INFINITY
            } else {
                f32::NAN
            }
        }
        _ => sign * (1.0 + frac / 1024.0) * 2f32.powi(exp - 15),
    }
}

/// Encodes an `f32` as IEEE half precision (round to nearest even).
///
/// # Panics
/// Never: the `expect`s guard masked bit-fields that provably fit.
#[cfg(feature = "verification")]
#[must_use]
pub fn f32_to_half(v: f32) -> u16 {
    let bits = v.to_bits();
    let sign = u16::try_from((bits >> 16) & 0x8000).expect("masked to 16 bits");
    let exp = i32::from(u8::try_from((bits >> 23) & 0xff).expect("masked to 8 bits"));
    let mant = bits & 0x7f_ffff;
    if exp == 0xff {
        return sign | 0x7c00 | if mant != 0 { 0x200 } else { 0 };
    }
    let e = exp - 127 + 15;
    if e >= 0x1f {
        return sign | 0x7c00;
    }
    if e <= 0 {
        if e < -10 {
            return sign;
        }
        let m = (mant | 0x80_0000) >> (1 - e);
        let rounded = (m + 0xfff + ((m >> 13) & 1)) >> 13;
        return sign | u16::try_from(rounded).expect("denormal mantissa fits in 11 bits");
    }
    let mut out = (u32::try_from(e).unwrap_or(0) << 10) | (mant >> 13);
    let rem = mant & 0x1fff;
    if rem > 0x1000 || (rem == 0x1000 && (out & 1) == 1) {
        out += 1;
    }
    sign | u16::try_from(out).expect("half exponent+mantissa fit in 16 bits")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn half_roundtrip() {
        for v in [0.0, 0.5, 1.0, 0.85, 0.001, -0.25, 3.5, 60000.0] {
            let h = f32_to_half(v);
            let back = half_to_f32(h);
            assert!(
                (back - v).abs() <= v.abs().mul_add(1e-3, 1e-6),
                "{v} -> {back}"
            );
        }
    }

    #[test]
    fn recipe_layout_is_23_vec4() {
        assert_eq!(std::mem::size_of::<GpuRecipe>(), 23 * 16);
    }
}
