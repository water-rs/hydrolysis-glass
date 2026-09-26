//! The wgpu implementation of the glass material.
//!
//! [`GlassRenderer`] takes a backdrop texture and a [`Scene`] of groups (each
//! group shares one union field) and composites the glass over the backdrop.
//! Per group it runs, in order: the backdrop capture (downsample, σ=1 texel
//! pre-blur, mip chain), the field pass, the material pass (refraction,
//! blur, face, bleed, shadow, SDR band, output clamp, highlight and tint
//! siblings) and optionally the foreground dispersion pass.
//!
//! Textures hold premultiplied, sRGB-encoded values in `Rgba16Float`; the
//! shaders never convert to linear light, as the specification's colour
//! math is defined on encoded values.

use super::geometry::{
    CONTINUOUS_EXPONENT, CornerCurve, SMOOTHING_TO_TAU, STANDALONE_UNION_SMOOTHING, Shape,
};
use super::recipe::Recipe;
use bytemuck::{Pod, Zeroable};
use std::cell::Cell;
use wgpu::util::DeviceExt;

/// Format of the composite and output textures.
pub const COMPOSITE_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;
/// Format of the field texture: `(d, n.x, n.y, owner)`.
pub const FIELD_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba32Float;
/// Mip levels of the capture chain: ≥7 are required, 9 reproduces the
/// captures. **[inferred]**
pub const CAPTURE_MIP_LEVELS: u32 = 9;

/// Rotation of the foreground aberration axis relative to the surface
/// normal, radians. **[unknown; chosen]**
pub const DISPERSION_AXIS_ANGLE: f32 = 0.35;
/// Spread of the seven spectral taps at the silhouette, pt. **[unmeasured;
/// chosen to give one dominant ~1–2 px fringe per side at 3 px/pt]**
pub const DISPERSION_SPREAD_PT: f32 = 0.6;
/// Foreground edge fade band, pt. **[inferred 1–2 pt]**
pub const FOREGROUND_FADE_PT: f32 = 1.5;

/// One glass element: a shape and its recipe.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Element {
    /// The silhouette.
    pub shape: Shape,
    /// The parameter block.
    pub recipe: Recipe,
}

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
}

impl Scene {
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

impl From<&Shape> for GpuElement {
    fn from(s: &Shape) -> Self {
        Self {
            rect: [s.rect.x, s.rect.y, s.rect.w, s.rect.h],
            radii: s.radii,
            params: [
                match s.curve {
                    CornerCurve::Circular => 0.0,
                    CornerCurve::Continuous => 1.0,
                },
                s.ovalization,
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
    scene: [f32; 4],
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
    disp_mat: [f32; 4],
    lobes: [f32; 4],
    refr: [f32; 4],
    blur_dist: [f32; 4],
    blur_op: [f32; 4],
    blur_tail: [f32; 4],
    face_bws: [f32; 4],
    face_fill: [f32; 4],
    bleed: [f32; 4],
    bleed_misc: [f32; 4],
    bleed_bws: [f32; 4],
    shadow_a: [f32; 4],
    shadow_b: [f32; 4],
    shadow_bws: [f32; 4],
    shadow_fill: [f32; 4],
    sdr: [f32; 4],
    output: [f32; 4],
    flags: [f32; 4],
    hl_a: [f32; 4],
    hl_b: [f32; 4],
    hl_key: [f32; 4],
    hl_fill: [f32; 4],
    tint: [f32; 4],
}

const fn flag(b: bool) -> f32 {
    if b { 1.0 } else { 0.0 }
}

impl From<&Recipe> for GpuRecipe {
    fn from(r: &Recipe) -> Self {
        let tint = r
            .tint
            .map_or([0.0; 4], |t| [t.rgb[0], t.rgb[1], t.rgb[2], t.alpha]);
        Self {
            disp_mat: r.disp_mat,
            lobes: [r.inner_amt, r.inner_h, r.outer_amt, r.outer_h],
            refr: [r.refr_thr[0], r.refr_thr[1], r.refr_op, r.blur_r],
            blur_dist: [
                r.blur_dist[0],
                r.blur_dist[1],
                r.blur_dist[2],
                r.blur_dist[3],
            ],
            blur_op: [r.blur_op[0], r.blur_op[1], r.blur_op[2], r.blur_op[3]],
            blur_tail: [
                r.blur_dist[4],
                r.blur_op[4],
                r.capture_scale,
                flag(r.capture_scale > 0.375),
            ],
            face_bws: [r.face.black, r.face.white, r.face.saturation, r.face_op],
            face_fill: r.face.fill,
            bleed: [r.bleed_amt, r.bleed_h, r.bleed_blur_r, r.bleed_op],
            bleed_misc: [r.bleed_dist[0], r.bleed_dist[1], flag(r.bleed_darken), 0.0],
            bleed_bws: [
                r.bleed_cm.black,
                r.bleed_cm.white,
                r.bleed_cm.saturation,
                0.0,
            ],
            shadow_a: [
                r.shadow_offset[0],
                r.shadow_offset[1],
                r.shadow_amt,
                r.shadow_h,
            ],
            shadow_b: [
                r.shadow_blur_r,
                r.shadow_radius,
                r.shadow_op,
                r.shadow_contrib,
            ],
            shadow_bws: [
                r.shadow_cm.black,
                r.shadow_cm.white,
                r.shadow_cm.saturation,
                0.0,
            ],
            shadow_fill: r.shadow_cm.fill,
            sdr: [r.sdr_dist[0], r.sdr_dist[1], r.sdr_op, r.sdr_white],
            output: [
                r.max_headroom,
                flag(r.preserve_hue),
                r.edr_scale,
                r.positive_range,
            ],
            flags: [
                flag(r.material_enabled),
                flag(r.luma_tracking),
                flag(r.tint.is_some()),
                0.0,
            ],
            hl_a: [
                r.highlight.curvature,
                r.highlight.key_amount,
                r.highlight.fill_amount,
                r.highlight.spread,
            ],
            hl_b: [
                r.highlight.key_angle,
                r.highlight.fill_angle,
                r.highlight.knee,
                0.0,
            ],
            hl_key: r.highlight.key_color,
            hl_fill: r.highlight.fill_color,
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
    created: &Cell<usize>,
) -> wgpu::Texture {
    created.set(created.get() + 1);
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
    created: &Cell<usize>,
) -> wgpu::Buffer {
    created.set(created.get() + 1);
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
    created: &Cell<usize>,
) -> wgpu::Buffer {
    created.set(created.get() + 1);
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
    gaussian: wgpu::RenderPipeline,
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
    /// Index into `groups` of the last group that ran a field pass.
    last_field: Option<usize>,
    /// Textures, buffers and bind groups ever created.
    created: Cell<usize>,
}

/// The textures, parameter buffers and bind groups of one capture chain.
/// All of it is kept across frames — only the contents are redrawn.
#[derive(Debug)]
struct Capture {
    /// All-mips view for the material pass.
    view: wgpu::TextureView,
    max_lod: f32,
    /// One `CaptureParams` buffer per pass step (see `run_capture`).
    params: Vec<wgpu::Buffer>,
    /// Pass targets in submission order: `tmp_a`, `tmp_b`, mip 0..levels.
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
    material_uni: wgpu::Buffer,
    fg_uni: wgpu::Buffer,
    field_bg: wgpu::BindGroup,
    material_bg: Option<wgpu::BindGroup>,
    fg_bg: Option<wgpu::BindGroup>,
    /// `0` = the 0.25 capture, `1` = the 0.5 capture.
    captures: [Option<Capture>; 2],
}

/// The rect a group's passes run in, px: member bounds inflated by the
/// farthest reach of any recipe effect — the field bound
/// (`positive_range`), the displaced shadow falloff
/// (`2·shadow_radius + |shadow_offset|`), and the capture taps displaced
/// by the shadow/bleed/refraction lobes.
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
        let off = r.shadow_offset[0].hypot(r.shadow_offset[1]);
        reach = reach
            .max(r.positive_range)
            .max(2.0f32.mul_add(r.shadow_radius, off))
            .max(r.sdr_dist[0].max(r.sdr_dist[1]).abs())
            .max(r.bleed_dist[0].max(r.bleed_dist[1]).abs() + r.bleed_amt.abs())
            .max(r.shadow_amt.abs() + off)
            .max(r.inner_amt.abs() + 1.0)
            .max(r.outer_amt.abs() + 1.0);
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

impl GroupGpu {
    fn new(
        device: &wgpu::Device,
        field_layout: &wgpu::BindGroupLayout,
        region_px: [u32; 4],
        capacity: usize,
        created: &Cell<usize>,
    ) -> Self {
        let capacity = capacity.max(1);
        let field_tex = create_texture(
            device,
            "glass field",
            [region_px[2], region_px[3]],
            FIELD_FORMAT,
            1,
            created,
        );
        let field_view = field_tex.create_view(&wgpu::TextureViewDescriptor::default());
        let patch_a = create_texture(
            device,
            "glass patch a",
            [region_px[2], region_px[3]],
            COMPOSITE_FORMAT,
            1,
            created,
        );
        let patch_b = create_texture(
            device,
            "glass patch b",
            [region_px[2], region_px[3]],
            COMPOSITE_FORMAT,
            1,
            created,
        );
        let storage = wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST;
        let uniform = wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST;
        let element_buf = create_buffer(
            device,
            "glass members",
            capacity as u64 * size_of::<GpuElement>() as u64,
            storage,
            created,
        );
        let recipe_buf = create_buffer(
            device,
            "glass recipes",
            capacity as u64 * size_of::<GpuRecipe>() as u64,
            storage,
            created,
        );
        let lobe_buf = create_buffer(
            device,
            "glass foreground lobes",
            capacity as u64 * 16,
            storage,
            created,
        );
        let field_uni = create_buffer(
            device,
            "glass field uniforms",
            size_of::<FieldUniforms>() as u64,
            uniform,
            created,
        );
        let material_uni = create_buffer(
            device,
            "glass material uniforms",
            size_of::<MaterialUniforms>() as u64,
            uniform,
            created,
        );
        let fg_uni = create_buffer(
            device,
            "glass foreground uniforms",
            size_of::<ForegroundUniforms>() as u64,
            uniform,
            created,
        );
        let field_bg = field_bind_group(field_layout, device, &field_uni, &element_buf, created);
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
            material_uni,
            fg_uni,
            field_bg,
            material_bg: None,
            fg_bg: None,
            captures: [None, None],
        }
    }

    /// Region moved or resized: field and patch textures are region-sized,
    /// so they and every bind group viewing them are rebuilt.
    fn set_region(&mut self, device: &wgpu::Device, region_px: [u32; 4], created: &Cell<usize>) {
        if self.region_px == region_px {
            return;
        }
        self.region_px = region_px;
        let size = [region_px[2], region_px[3]];
        self.field_tex = create_texture(device, "glass field", size, FIELD_FORMAT, 1, created);
        self.field_view = self
            .field_tex
            .create_view(&wgpu::TextureViewDescriptor::default());
        self.patch_a = create_texture(device, "glass patch a", size, COMPOSITE_FORMAT, 1, created);
        self.patch_a_view = self
            .patch_a
            .create_view(&wgpu::TextureViewDescriptor::default());
        self.patch_b = create_texture(device, "glass patch b", size, COMPOSITE_FORMAT, 1, created);
        self.patch_b_view = self
            .patch_b
            .create_view(&wgpu::TextureViewDescriptor::default());
        self.captures = [None, None];
        self.material_bg = None;
        self.fg_bg = None;
    }

    /// The member count grew past the storage buffers: enlarge them and
    /// rebuild the bind groups that point at them.
    fn ensure_capacity(
        &mut self,
        device: &wgpu::Device,
        field_layout: &wgpu::BindGroupLayout,
        capacity: usize,
        created: &Cell<usize>,
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
            created,
        );
        self.recipe_buf = create_buffer(
            device,
            "glass recipes",
            capacity as u64 * size_of::<GpuRecipe>() as u64,
            storage,
            created,
        );
        self.lobe_buf = create_buffer(
            device,
            "glass foreground lobes",
            capacity as u64 * 16,
            storage,
            created,
        );
        self.field_bg = field_bind_group(
            field_layout,
            device,
            &self.field_uni,
            &self.element_buf,
            created,
        );
        self.material_bg = None;
        self.fg_bg = None;
    }
}

fn field_bind_group(
    field_layout: &wgpu::BindGroupLayout,
    device: &wgpu::Device,
    field_uni: &wgpu::Buffer,
    element_buf: &wgpu::Buffer,
    created: &Cell<usize>,
) -> wgpu::BindGroup {
    created.set(created.get() + 1);
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
        let created = std::cell::Cell::new(0);
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
                texture_entry(5),
                sampler_entry(6),
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
        let gaussian = pipeline(
            device,
            "glass gaussian",
            &capture_module,
            "gaussian_1",
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
            gaussian,
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
                &created,
            ),
            composite: None,
            composite_view: None,
            composite_mip_views: Vec::new(),
            composite_mip_bgs: Vec::new(),
            groups: Vec::new(),
            fg_input: None,
            backdrop_blit: None,
            fg_blit: None,
            last_field: None,
            created,
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
        self.last_field = None;
        // One mip level per halving; the capture's first step samples the
        // composite at `lod = log2(footprint)` for hardware prefiltering.
        let full_levels = 32 - size[0].max(size[1]).leading_zeros();
        let levels = full_levels.clamp(1, CAPTURE_MIP_LEVELS);
        let composite = create_texture(
            device,
            "glass composite",
            size,
            COMPOSITE_FORMAT,
            levels,
            &self.created,
        );
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
                    &self.created,
                );
                Self::capture_bind_group(
                    device,
                    &self.capture_layout,
                    &self.sampler,
                    &self.composite_mip_views[(level - 1) as usize],
                    &params,
                    &self.created,
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

    fn capture_bind_group(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        sampler: &wgpu::Sampler,
        source: &wgpu::TextureView,
        params: &wgpu::Buffer,
        created: &Cell<usize>,
    ) -> wgpu::BindGroup {
        created.set(created.get() + 1);
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
        clippy::too_many_arguments,
        reason = "the chain needs the device, layout, sampler, scene, region, scale, source lod bound and resource counter"
    )]
    #[allow(
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
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
        created: &Cell<usize>,
    ) -> Capture {
        let size = [
            (region_pt[2] * scale).ceil().max(1.0) as u32,
            (region_pt[3] * scale).ceil().max(1.0) as u32,
        ];
        let full_levels = 32 - size[0].max(size[1]).leading_zeros();
        let levels = full_levels.clamp(1, CAPTURE_MIP_LEVELS);
        let texel = [1.0 / size[0] as f32, 1.0 / size[1] as f32];
        let texture = create_texture(
            device,
            "glass capture",
            size,
            COMPOSITE_FORMAT,
            levels,
            created,
        );
        let tmp_a = create_texture(
            device,
            "glass capture tmp a",
            size,
            COMPOSITE_FORMAT,
            1,
            created,
        );
        let tmp_b = create_texture(
            device,
            "glass capture tmp b",
            size,
            COMPOSITE_FORMAT,
            1,
            created,
        );
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

        let view_a = tmp_a.create_view(&wgpu::TextureViewDescriptor::default());
        let view_b = tmp_b.create_view(&wgpu::TextureViewDescriptor::default());
        let mut targets = vec![view_a, view_b];
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
        let mut params = Vec::with_capacity(levels as usize + 2);
        let mut push = |params_v: [f32; 4], rect_v: [f32; 4]| {
            params.push(init_buffer(
                device,
                "glass capture params",
                bytemuck::bytes_of(&CaptureParams {
                    params: params_v,
                    rect: rect_v,
                }),
                wgpu::BufferUsages::UNIFORM,
                created,
            ));
        };
        let identity = [0.0, 0.0, 1.0, 1.0];
        // Step 0 is a lod-filtered blit of the composite mip chain: the mip
        // level is chosen so one composite mip texel covers the whole
        // `region_px / capture_px` footprint — a real prefilter, unlike the
        // box taps which would alias a periodic backdrop into moiré.
        let stride = (region_pt[2] * scene.px_per_pt / size[0] as f32)
            .max(region_pt[3] * scene.px_per_pt / size[1] as f32);
        let lod0 = stride.log2().clamp(0.0, max_src_lod);
        push([0.0, 0.0, lod0, 0.0], rect);
        push([texel[0], texel[1], 1.0, 0.0], identity);
        push([texel[0], texel[1], 0.0, 1.0], identity);
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
                    created,
                )
            })
            .collect();
        Capture {
            view,
            max_lod: (levels - 1) as f32,
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
    /// bounds inflated by the farthest reach of its recipes — the field
    /// bound (`positive_range`), the displaced shadow falloff
    /// (`2·shadow_radius + |shadow_offset|`) and the capture taps displaced
    /// by the shadow, bleed and refraction lobes — so cost scales with the
    /// elements, not the screen. Textures, buffers and bind groups are kept
    /// across frames and only rewritten with the new frame's contents.
    ///
    /// # Panics
    /// If called before `ensure_targets` produced the composite (internal).
    #[allow(
        clippy::cast_precision_loss,
        clippy::too_many_lines,
        reason = "mip and region pixel counts are well inside f32's exact range; the frame is a flat sequence of passes"
    )]
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
                    &self.created,
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
                let tex = create_texture(
                    device,
                    "glass foreground input",
                    size,
                    COMPOSITE_FORMAT,
                    1,
                    &self.created,
                );
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
                        &self.created,
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

        // Composite mip chain, scoped to the union of the groups' regions:
        // each level is written only where a capture can sample it, so the
        // prefilter cost scales with the elements, not the screen.
        let union = scene
            .groups
            .iter()
            .filter(|g| !g.members.is_empty())
            .map(|g| group_region(g, scene))
            .reduce(|a, b| {
                let x1 = (a[0] + a[2]).max(b[0] + b[2]);
                let y1 = (a[1] + a[3]).max(b[1] + b[3]);
                [
                    a[0].min(b[0]),
                    a[1].min(b[1]),
                    x1 - a[0].min(b[0]),
                    y1 - a[1].min(b[1]),
                ]
            });
        if let Some(u) = union {
            for (k, (view, bg)) in self.composite_mip_views[1..]
                .iter()
                .zip(self.composite_mip_bgs.iter())
                .enumerate()
            {
                let k = u32::try_from(k + 1).expect("mip level fits u32");
                let mw = (size[0] >> k).max(1);
                let mh = (size[1] >> k).max(1);
                // Grow the region by this level's source footprint so every
                // destination texel's parents are themselves generated.
                let pad = 1 << (k - 1);
                let x0 = u[0].saturating_sub(pad) >> k;
                let y0 = u[1].saturating_sub(pad) >> k;
                let x1 = ((u[0] + u[2]).saturating_add(pad + (1 << k) - 1) >> k).min(mw);
                let y1 = ((u[1] + u[3]).saturating_add(pad + (1 << k) - 1) >> k).min(mh);
                Self::fullscreen_pass(
                    &mut encoder,
                    view,
                    &self.blit,
                    bg,
                    wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    Some([
                        x0.min(mw - 1),
                        y0.min(mh - 1),
                        (x1 - x0.min(mw - 1)).max(1),
                        (y1 - y0.min(mh - 1)).max(1),
                    ]),
                );
            }
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
        clippy::cast_possible_wrap,
        clippy::too_many_arguments,
        clippy::too_many_lines,
        reason = "region pixel coordinates and element counts are small; uniforms need f32; the group pass is a flat pass sequence needing the device, queue, encoder and scene"
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
        self.last_field = Some(gi);
        let region = group_region(group, scene);
        let pp = scene.px_per_pt;
        let region_pt = [
            region[0] as f32 / pp,
            region[1] as f32 / pp,
            region[2] as f32 / pp,
            region[3] as f32 / pp,
        ];
        let g = self.groups[gi].get_or_insert_with(|| {
            GroupGpu::new(
                device,
                &self.field_layout,
                region,
                group.members.len(),
                &self.created,
            )
        });
        g.set_region(device, region, &self.created);
        g.ensure_capacity(
            device,
            &self.field_layout,
            group.members.len(),
            &self.created,
        );

        // Per-frame contents: members, field uniforms, recipes.
        let elements: Vec<GpuElement> = group
            .members
            .iter()
            .map(|e| GpuElement::from(&e.shape))
            .collect();
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

        // Capture chains over the region of the current composite.
        let needs_regular = group
            .members
            .iter()
            .any(|e| e.recipe.capture_scale <= 0.375);
        let needs_clear = group.members.iter().any(|e| e.recipe.capture_scale > 0.375);
        for (slot, needed, scale) in [(0, needs_regular, 0.25), (1, needs_clear, 0.5)] {
            if !needed {
                continue;
            }
            if g.captures[slot].is_none() {
                g.captures[slot] = Some(Self::build_capture(
                    device,
                    &self.capture_layout,
                    &self.sampler,
                    scene,
                    region_pt,
                    scale,
                    (self.composite_mip_views.len() - 1) as f32,
                    &self.created,
                ));
            }
            let cap = g.captures[slot].as_mut().expect("capture");
            if cap.src_bg.is_none() {
                cap.src_bg = Some(Self::capture_bind_group(
                    device,
                    &self.capture_layout,
                    &self.sampler,
                    self.composite_view.as_ref().expect("targets"),
                    &cap.params[0],
                    &self.created,
                ));
            }
            // Step 0: composite mip -> tmp_a (lod-prefiltered blit);
            // steps 1,2: separable gaussian; steps 3..: the mip chain.
            for (step, target) in cap.targets.iter().enumerate() {
                let bg = if step == 0 {
                    cap.src_bg.as_ref().expect("src bg")
                } else {
                    &cap.chain_bgs[step - 1]
                };
                let pipe = if step == 0 {
                    &self.blit
                } else if step == 1 || step == 2 {
                    &self.gaussian
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

        // Material pass into patch_a; the patch holds the fully
        // composited pixel for every region texel, so it is copied
        // back verbatim.
        let regular_ref = g.captures[0].as_ref().or(g.captures[1].as_ref());
        let clear_ref = g.captures[1].as_ref().or(g.captures[0].as_ref());
        let (regular_ref, clear_ref) = (
            regular_ref.expect("at least one capture"),
            clear_ref.expect("at least one capture"),
        );
        queue.write_buffer(
            &g.material_uni,
            0,
            bytemuck::bytes_of(&MaterialUniforms {
                scene: [
                    scene.size_pt[0],
                    scene.size_pt[1],
                    scene.px_per_pt,
                    regular_ref.max_lod,
                ],
                misc: [clear_ref.max_lod, 0.0, 0.0, 0.0],
                region: region_pt,
                origin: [region[0] as f32, region[1] as f32, 0.0, 0.0],
            }),
        );
        let recipes: Vec<GpuRecipe> = group
            .members
            .iter()
            .map(|e| GpuRecipe::from(&e.recipe))
            .collect();
        queue.write_buffer(&g.recipe_buf, 0, bytemuck::cast_slice(&recipes));

        if g.material_bg.is_none() {
            self.created.set(self.created.get() + 1);
            g.material_bg = Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("glass material bind group"),
                layout: &self.material_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: g.material_uni.as_entire_binding(),
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
                        resource: wgpu::BindingResource::TextureView(&regular_ref.view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 5,
                        resource: wgpu::BindingResource::TextureView(&clear_ref.view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 6,
                        resource: wgpu::BindingResource::Sampler(&self.sampler),
                    },
                ],
            }));
        }
        Self::fullscreen_pass(
            encoder,
            &g.patch_a_view,
            &self.material,
            g.material_bg.as_ref().expect("material bg"),
            wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
            None,
        );

        // Foreground dispersion into patch_b, then the used patch is
        // copied back into the composite at the region origin.
        let result = if foreground {
            let lobes: Vec<[f32; 4]> = group
                .members
                .iter()
                .map(|e| [e.recipe.inner_amt, e.recipe.inner_h, 0.0, 0.0])
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
                        DISPERSION_SPREAD_PT,
                        FOREGROUND_FADE_PT,
                        region[0] as f32,
                        region[1] as f32,
                    ],
                }),
            );
            let fg_view = &self.fg_input.as_ref().expect("fg input").1;
            if g.fg_bg.is_none() {
                self.created.set(self.created.get() + 1);
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

    /// The field texture of the last rendered group and the region's
    /// origin in composite px. The texture covers only that region.
    #[cfg(feature = "verification")]
    #[must_use]
    pub fn field_texture(&self) -> Option<(&wgpu::Texture, [u32; 2])> {
        let g = self.groups.get(self.last_field?)?.as_ref()?;
        Some((&g.field_tex, [g.region_px[0], g.region_px[1]]))
    }

    /// Textures, buffers and bind groups created so far; a second identical
    /// frame must allocate none.
    #[cfg(feature = "verification")]
    #[must_use]
    pub const fn created_resources(&self) -> usize {
        self.created.get()
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
    let data = slice.get_mapped_range();
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
