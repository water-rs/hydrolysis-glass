#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::many_single_char_names,
    clippy::needless_range_loop,
    clippy::large_types_passed_by_value,
    clippy::float_cmp,
    reason = "verification harness: pixel arithmetic on synthetic images"
)]
//! Shared harness for the offscreen verification renders.

#![allow(dead_code)]

use hydrolysis_glass::material::geometry::{CornerCurve, Rect, Shape};
use hydrolysis_glass::material::gpu::{COMPOSITE_FORMAT, f32_to_half, read_texture};
use hydrolysis_glass::{Appearance, Element, GlassRenderer, Group, Recipe, Scene, Variant};
use std::path::PathBuf;
use std::sync::OnceLock;

/// Record scale: output pixels per pt.
pub const PX_PER_PT: f32 = 3.0;

pub struct Gpu {
    pub adapter_name: String,
    pub backend: String,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
}

static GPU: OnceLock<Gpu> = OnceLock::new();

/// Every backend wgpu supports: the suite runs on CI's native backend
/// (Vulkan/lavapipe on Linux, Metal on macOS, DX12 on Windows).
pub const BACKENDS: wgpu::Backends = wgpu::Backends::all();

/// The shared device; Vulkan first so Lavapipe is picked when present.
pub fn gpu() -> &'static Gpu {
    GPU.get_or_init(|| {
        let mut desc = wgpu::InstanceDescriptor::new_without_display_handle();
        desc.backends = BACKENDS;
        let instance = wgpu::Instance::new(desc);
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::LowPower,
            force_fallback_adapter: false,
            compatible_surface: None,
        }))
        .unwrap_or_else(|_| panic!("no wgpu adapter among {BACKENDS:?} on this platform"));
        let info = adapter.get_info();
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("glass verification"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::downlevel_defaults().using_resolution(adapter.limits()),
            ..Default::default()
        }))
        .expect("a device");
        eprintln!(
            "adapter: {} ({:?}, {:?})",
            info.name, info.backend, info.device_type
        );
        Gpu {
            adapter_name: info.name,
            backend: format!("{:?}", info.backend),
            device,
            queue,
        }
    })
}

/// An RGBA image of premultiplied encoded values.
#[derive(Clone, Debug)]
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<[f32; 4]>,
}

impl Image {
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            pixels: vec![[0.0; 4]; (width * height) as usize],
        }
    }

    pub fn from_fn(width: u32, height: u32, f: impl Fn(f32, f32) -> [f32; 4]) -> Self {
        let mut img = Self::new(width, height);
        for y in 0..height {
            for x in 0..width {
                img.pixels[(y * width + x) as usize] = f(x as f32 + 0.5, y as f32 + 0.5);
            }
        }
        img
    }

    pub fn at(&self, x: u32, y: u32) -> [f32; 4] {
        self.pixels[(y * self.width + x) as usize]
    }

    /// Sample at pt coordinates (nearest pixel).
    pub fn at_pt(&self, x: f32, y: f32) -> [f32; 4] {
        let px = ((x * PX_PER_PT) as i64).clamp(0, i64::from(self.width) - 1) as u32;
        let py = ((y * PX_PER_PT) as i64).clamp(0, i64::from(self.height) - 1) as u32;
        self.at(px, py)
    }

    pub fn save(&self, name: &str) {
        let mut dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        dir.push("target/verification");
        std::fs::create_dir_all(&dir).ok();
        let mut bytes = Vec::with_capacity(self.pixels.len() * 4);
        for p in &self.pixels {
            for c in p {
                bytes.push((c.clamp(0.0, 1.0) * 255.0).round() as u8);
            }
        }
        image::save_buffer(
            dir.join(format!("{name}.png")),
            &bytes,
            self.width,
            self.height,
            image::ColorType::Rgba8,
        )
        .expect("write png");
    }
}

pub const fn grey(v: f32) -> [f32; 4] {
    [v, v, v, 1.0]
}

pub fn luma(p: [f32; 4]) -> f32 {
    0.0722f32.mul_add(p[2], 0.7152f32.mul_add(p[1], 0.2126 * p[0]))
}

/// Uploads an image as the backdrop texture.
pub fn upload(img: &Image) -> wgpu::Texture {
    let g = gpu();
    let texture = g.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("backdrop"),
        size: wgpu::Extent3d {
            width: img.width,
            height: img.height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: COMPOSITE_FORMAT,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    let mut bytes = Vec::with_capacity(img.pixels.len() * 8);
    for p in &img.pixels {
        for c in p {
            bytes.extend_from_slice(&f32_to_half(*c).to_le_bytes());
        }
    }
    g.queue.write_texture(
        texture.as_image_copy(),
        &bytes,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(img.width * 8),
            rows_per_image: Some(img.height),
        },
        texture.size(),
    );
    texture
}

pub struct Render {
    pub output: Image,
    pub field: Image,
}

/// Renders `scene` over `backdrop` (and optional foreground content).
pub fn render(backdrop: &Image, scene: &Scene, foreground: Option<&Image>) -> Render {
    let g = gpu();
    let mut renderer = GlassRenderer::new(&g.device);
    let bd = upload(backdrop);
    let fg = foreground.map(upload);
    let bd_view = bd.create_view(&wgpu::TextureViewDescriptor::default());
    let fg_view = fg
        .as_ref()
        .map(|t| t.create_view(&wgpu::TextureViewDescriptor::default()));
    let out = renderer.render(&g.device, &g.queue, &bd_view, scene, fg_view.as_ref());
    let size = out.size();
    let pixels = read_texture(&g.device, &g.queue, out);
    // The field texture covers only the rendered group's bounds plus
    // reach; pixels outside it are far from every member.
    let (field_tex, origin) = renderer.field_texture().expect("field");
    let fsize = field_tex.size();
    let data = read_texture(&g.device, &g.queue, field_tex);
    let mut field = vec![[1e6f32, 0.0, 0.0, -1.0]; (size.width * size.height) as usize];
    for y in 0..fsize.height {
        for x in 0..fsize.width {
            field[((origin[1] + y) * size.width + origin[0] + x) as usize] =
                data[(y * fsize.width + x) as usize];
        }
    }
    Render {
        output: Image {
            width: size.width,
            height: size.height,
            pixels,
        },
        field: Image {
            width: size.width,
            height: size.height,
            pixels: field,
        },
    }
}

pub fn scene_px(w_pt: f32, h_pt: f32) -> (u32, u32) {
    ((w_pt * PX_PER_PT) as u32, (h_pt * PX_PER_PT) as u32)
}

pub const fn rounded(x: f32, y: f32, w: f32, h: f32, r: f32) -> Shape {
    Shape {
        rect: Rect { x, y, w, h },
        radii: [r; 4],
        curve: CornerCurve::Continuous,
        ovalization: 0.0,
    }
}

pub fn capsule(x: f32, y: f32, w: f32, h: f32) -> Shape {
    rounded(x, y, w, h, h.min(w) / 2.0)
}

pub fn element(shape: Shape, variant: Variant, appearance: Appearance) -> Element {
    Element {
        shape,
        recipe: Recipe::for_element(shape.rect.w, shape.rect.h, variant, appearance),
    }
}

pub fn single_scene(size_pt: [f32; 2], element: Element) -> Scene {
    Scene {
        size_pt,
        px_per_pt: PX_PER_PT,
        groups: vec![Group::single(element)],
    }
}

/// Standard 300×200 element centred in a 480×360 scene.
pub const SCENE: [f32; 2] = [480.0, 360.0];
pub const STD: Rect = Rect {
    x: 90.0,
    y: 80.0,
    w: 300.0,
    h: 200.0,
};

pub fn std_element(variant: Variant, appearance: Appearance) -> Element {
    element(
        rounded(STD.x, STD.y, STD.w, STD.h, 40.0),
        variant,
        appearance,
    )
}

/// 1 pt lines every 8 pt (black on white).
pub fn grid_backdrop() -> Image {
    let (w, h) = scene_px(SCENE[0], SCENE[1]);
    Image::from_fn(w, h, |x, y| {
        let xp = (x / PX_PER_PT).rem_euclid(8.0);
        let yp = (y / PX_PER_PT).rem_euclid(8.0);
        if xp < 1.0 || yp < 1.0 {
            grey(0.0)
        } else {
            grey(1.0)
        }
    })
}

/// Grid contrast along a horizontal row between pt x0..x1: (max-min) of luma.
pub fn row_contrast(img: &Image, y_pt: f32, x0_pt: f32, x1_pt: f32) -> f32 {
    let mut lo = f32::MAX;
    let mut hi = f32::MIN;
    let y = (y_pt * PX_PER_PT) as u32;
    for x in ((x0_pt * PX_PER_PT) as u32)..((x1_pt * PX_PER_PT) as u32) {
        let l = luma(img.at(x, y));
        lo = lo.min(l);
        hi = hi.max(l);
    }
    hi - lo
}

/// The reference-capture directory, if the orchestrator supplied one.
pub fn reference_dir() -> Option<PathBuf> {
    let dir = std::env::var_os("GLASS_REFERENCE_CAPTURES").map(PathBuf::from)?;
    dir.is_dir().then_some(dir)
}

/// Compares `img` to `<dir>/<name>.png` within `tol` (0–1 per channel), or
/// reports the test as pending when no capture is available.
pub fn compare_reference(img: &Image, name: &str, tol: f32) {
    let Some(dir) = reference_dir() else {
        eprintln!("PENDING reference capture: {name}.png (set GLASS_REFERENCE_CAPTURES)");
        return;
    };
    let path = dir.join(format!("{name}.png"));
    let reference = image::open(&path)
        .unwrap_or_else(|e| panic!("open {}: {e}", path.display()))
        .to_rgba8();
    assert_eq!(
        (reference.width(), reference.height()),
        (img.width, img.height),
        "{name}: size"
    );
    let mut worst = 0.0f32;
    for (i, p) in reference.pixels().enumerate() {
        let ours = img.pixels[i];
        for c in 0..4 {
            worst = worst.max((f32::from(p.0[c]) / 255.0 - ours[c].clamp(0.0, 1.0)).abs());
        }
    }
    assert!(
        worst <= tol,
        "{name}: worst channel deviation {worst} > {tol}"
    );
}
