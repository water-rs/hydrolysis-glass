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
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
struct MaterialUniforms {
    scene: [f32; 4],
    misc: [f32; 4],
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
            | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    })
}

/// Renders glass scenes over a backdrop.
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
    composites: Option<([wgpu::Texture; 2], wgpu::Texture, [u32; 2])>,
    current: usize,
}

struct Capture {
    texture: wgpu::Texture,
    max_lod: f32,
}

impl GlassRenderer {
    /// Compiles the material's pipelines.
    #[must_use]
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
            composites: None,
            current: 0,
        }
    }

    fn ensure_targets(&mut self, device: &wgpu::Device, size: [u32; 2]) {
        if let Some((_, _, s)) = &self.composites
            && *s == size
        {
            return;
        }
        let a = create_texture(device, "glass composite a", size, COMPOSITE_FORMAT, 1);
        let b = create_texture(device, "glass composite b", size, COMPOSITE_FORMAT, 1);
        let field = create_texture(device, "glass field", size, FIELD_FORMAT, 1);
        self.composites = Some(([a, b], field, size));
    }

    fn fullscreen_pass(
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        pipeline: &wgpu::RenderPipeline,
        bind_group: &wgpu::BindGroup,
        load: wgpu::LoadOp<wgpu::Color>,
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
        pass.draw(0..3, 0..1);
    }

    fn capture_bind_group(
        &self,
        device: &wgpu::Device,
        source: &wgpu::TextureView,
        params: [f32; 4],
    ) -> wgpu::BindGroup {
        let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("glass capture params"),
            contents: bytemuck::bytes_of(&CaptureParams { params }),
            usage: wgpu::BufferUsages::UNIFORM,
        });
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("glass capture bind group"),
            layout: &self.capture_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(source),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: buffer.as_entire_binding(),
                },
            ],
        })
    }

    fn build_capture(
        &self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        source: &wgpu::Texture,
        scene: &Scene,
        scale: f32,
    ) -> Capture {
        let size = [
            (scene.size_pt[0] * scale).ceil().max(1.0) as u32,
            (scene.size_pt[1] * scale).ceil().max(1.0) as u32,
        ];
        let full_levels = 32 - size[0].max(size[1]).leading_zeros();
        let levels = full_levels.clamp(1, CAPTURE_MIP_LEVELS);
        let texel = [1.0 / size[0] as f32, 1.0 / size[1] as f32];
        let texture = create_texture(device, "glass capture", size, COMPOSITE_FORMAT, levels);
        let tmp_a = create_texture(device, "glass capture tmp a", size, COMPOSITE_FORMAT, 1);
        let tmp_b = create_texture(device, "glass capture tmp b", size, COMPOSITE_FORMAT, 1);
        let source_view = source.create_view(&wgpu::TextureViewDescriptor::default());
        let src_size = source.size();
        let src_texel = [1.0 / src_size.width as f32, 1.0 / src_size.height as f32];

        let view_a = tmp_a.create_view(&wgpu::TextureViewDescriptor::default());
        let view_b = tmp_b.create_view(&wgpu::TextureViewDescriptor::default());
        // Downsample the composite into the capture resolution.
        let bg =
            self.capture_bind_group(device, &source_view, [src_texel[0], src_texel[1], 0.0, 0.0]);
        Self::fullscreen_pass(
            encoder,
            &view_a,
            &self.downsample,
            &bg,
            wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
        );
        // Pre-blur, σ = 1 capture texel, separable.
        let bg = self.capture_bind_group(device, &view_a, [texel[0], texel[1], 1.0, 0.0]);
        Self::fullscreen_pass(
            encoder,
            &view_b,
            &self.gaussian,
            &bg,
            wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
        );
        let mip0 = texture.create_view(&wgpu::TextureViewDescriptor {
            base_mip_level: 0,
            mip_level_count: Some(1),
            ..Default::default()
        });
        let bg = self.capture_bind_group(device, &view_b, [texel[0], texel[1], 0.0, 1.0]);
        Self::fullscreen_pass(
            encoder,
            &mip0,
            &self.gaussian,
            &bg,
            wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
        );
        // Mip chain.
        for level in 1..levels {
            let src = texture.create_view(&wgpu::TextureViewDescriptor {
                base_mip_level: level - 1,
                mip_level_count: Some(1),
                ..Default::default()
            });
            let dst = texture.create_view(&wgpu::TextureViewDescriptor {
                base_mip_level: level,
                mip_level_count: Some(1),
                ..Default::default()
            });
            let w = (size[0] >> (level - 1)).max(1) as f32;
            let h = (size[1] >> (level - 1)).max(1) as f32;
            let bg = self.capture_bind_group(device, &src, [1.0 / w, 1.0 / h, 0.0, 0.0]);
            Self::fullscreen_pass(
                encoder,
                &dst,
                &self.downsample,
                &bg,
                wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
            );
        }
        Capture {
            texture,
            max_lod: (levels - 1) as f32,
        }
    }

    /// Renders `scene` over `backdrop` (a filterable 2D texture covering the
    /// scene; premultiplied, encoded values). `foreground`, if given, is
    /// content drawn inside the glass elements, run through the dispersion
    /// pass and blended on top. Returns the output texture, which stays
    /// valid until the next call.
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
        let (composites, field_tex, _) = self.composites.as_ref().expect("targets");
        self.current = 0;
        let field_view = field_tex.create_view(&wgpu::TextureViewDescriptor::default());

        // Bring the backdrop into the composite.
        {
            let dst = composites[0].create_view(&wgpu::TextureViewDescriptor::default());
            let bg = self.capture_bind_group(device, backdrop, [0.0; 4]);
            Self::fullscreen_pass(
                &mut encoder,
                &dst,
                &self.blit,
                &bg,
                wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
            );
        }

        for group in &scene.groups {
            if group.members.is_empty() {
                continue;
            }
            let source = &composites[self.current];
            let target = &composites[1 - self.current];

            let needs_regular = group
                .members
                .iter()
                .any(|e| e.recipe.capture_scale <= 0.375);
            let needs_clear = group.members.iter().any(|e| e.recipe.capture_scale > 0.375);
            let regular = needs_regular
                .then(|| self.build_capture(device, &mut encoder, source, scene, 0.25));
            let clear =
                needs_clear.then(|| self.build_capture(device, &mut encoder, source, scene, 0.5));
            let fallback = regular
                .as_ref()
                .or(clear.as_ref())
                .expect("at least one capture");
            let regular_ref = regular.as_ref().unwrap_or(fallback);
            let clear_ref = clear.as_ref().unwrap_or(fallback);

            // Field pass.
            let elements: Vec<GpuElement> = group
                .members
                .iter()
                .map(|e| GpuElement::from(&e.shape))
                .collect();
            let element_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("glass members"),
                contents: bytemuck::cast_slice(&elements),
                usage: wgpu::BufferUsages::STORAGE,
            });
            let field_uniforms = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("glass field uniforms"),
                contents: bytemuck::bytes_of(&FieldUniforms {
                    scene: [
                        scene.size_pt[0],
                        scene.size_pt[1],
                        scene.px_per_pt,
                        (group.smoothing * SMOOTHING_TO_TAU).max(1e-3),
                    ],
                    misc: [elements.len() as f32, CONTINUOUS_EXPONENT, 0.0, 0.0],
                }),
                usage: wgpu::BufferUsages::UNIFORM,
            });
            let field_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("glass field bind group"),
                layout: &self.field_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: field_uniforms.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: element_buffer.as_entire_binding(),
                    },
                ],
            });
            Self::fullscreen_pass(
                &mut encoder,
                &field_view,
                &self.field,
                &field_bg,
                wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
            );

            // Material pass.
            let recipes: Vec<GpuRecipe> = group
                .members
                .iter()
                .map(|e| GpuRecipe::from(&e.recipe))
                .collect();
            let recipe_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("glass recipes"),
                contents: bytemuck::cast_slice(&recipes),
                usage: wgpu::BufferUsages::STORAGE,
            });
            let material_uniforms = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("glass material uniforms"),
                contents: bytemuck::bytes_of(&MaterialUniforms {
                    scene: [
                        scene.size_pt[0],
                        scene.size_pt[1],
                        scene.px_per_pt,
                        regular_ref.max_lod,
                    ],
                    misc: [clear_ref.max_lod, 0.0, 0.0, 0.0],
                }),
                usage: wgpu::BufferUsages::UNIFORM,
            });
            let source_view = source.create_view(&wgpu::TextureViewDescriptor::default());
            let regular_view = regular_ref
                .texture
                .create_view(&wgpu::TextureViewDescriptor::default());
            let clear_view = clear_ref
                .texture
                .create_view(&wgpu::TextureViewDescriptor::default());
            let material_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("glass material bind group"),
                layout: &self.material_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: material_uniforms.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: recipe_buffer.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::TextureView(&field_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: wgpu::BindingResource::TextureView(&source_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 4,
                        resource: wgpu::BindingResource::TextureView(&regular_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 5,
                        resource: wgpu::BindingResource::TextureView(&clear_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 6,
                        resource: wgpu::BindingResource::Sampler(&self.sampler),
                    },
                ],
            });
            let target_view = target.create_view(&wgpu::TextureViewDescriptor::default());
            Self::fullscreen_pass(
                &mut encoder,
                &target_view,
                &self.material,
                &material_bg,
                wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
            );

            // Foreground dispersion pass.
            if let Some(content) = foreground {
                let lobes: Vec<[f32; 4]> = group
                    .members
                    .iter()
                    .map(|e| [e.recipe.inner_amt, e.recipe.inner_h, 0.0, 0.0])
                    .collect();
                let lobe_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("glass foreground lobes"),
                    contents: bytemuck::cast_slice(&lobes),
                    usage: wgpu::BufferUsages::STORAGE,
                });
                let fg_uniforms = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("glass foreground uniforms"),
                    contents: bytemuck::bytes_of(&ForegroundUniforms {
                        scene: [
                            scene.size_pt[0],
                            scene.size_pt[1],
                            scene.px_per_pt,
                            DISPERSION_AXIS_ANGLE,
                        ],
                        params: [DISPERSION_SPREAD_PT, FOREGROUND_FADE_PT, 0.0, 0.0],
                    }),
                    usage: wgpu::BufferUsages::UNIFORM,
                });
                let fg_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("glass foreground bind group"),
                    layout: &self.foreground_layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: fg_uniforms.as_entire_binding(),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: lobe_buffer.as_entire_binding(),
                        },
                        wgpu::BindGroupEntry {
                            binding: 2,
                            resource: wgpu::BindingResource::TextureView(&field_view),
                        },
                        wgpu::BindGroupEntry {
                            binding: 3,
                            resource: wgpu::BindingResource::TextureView(content),
                        },
                        wgpu::BindGroupEntry {
                            binding: 4,
                            resource: wgpu::BindingResource::Sampler(&self.sampler),
                        },
                        wgpu::BindGroupEntry {
                            binding: 5,
                            resource: wgpu::BindingResource::TextureView(&target_view),
                        },
                    ],
                });
                // The foreground composites per channel (spectral coverage), so
                // it reads the material result and writes the other buffer,
                // which leaves `current` pointing at the final image.
                Self::fullscreen_pass(
                    &mut encoder,
                    &source_view,
                    &self.foreground,
                    &fg_bg,
                    wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                );
            } else {
                self.current = 1 - self.current;
            }
        }

        queue.submit(Some(encoder.finish()));
        &composites[self.current]
    }

    /// The field texture of the last rendered group.
    #[must_use]
    pub fn field_texture(&self) -> Option<&wgpu::Texture> {
        self.composites.as_ref().map(|(_, f, _)| f)
    }
}

/// Reads a texture back as `f32` RGBA rows (`Rgba16Float` or `Rgba32Float`).
///
/// # Panics
/// On an unsupported format or a failed buffer map.
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
#[must_use]
pub fn f32_to_half(v: f32) -> u16 {
    let bits = v.to_bits();
    let sign = ((bits >> 16) & 0x8000) as u16;
    let exp = ((bits >> 23) & 0xff) as i32;
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
        return sign | rounded as u16;
    }
    let mut out = (u32::try_from(e).unwrap_or(0) << 10) | (mant >> 13);
    let rem = mant & 0x1fff;
    if rem > 0x1000 || (rem == 0x1000 && (out & 1) == 1) {
        out += 1;
    }
    sign | out as u16
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
