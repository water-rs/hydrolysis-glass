//! Shared harness for the §7 verification-scene exports.

use hydrolysis_glass::material::geometry::{CornerCurve, Rect, Shape};
use hydrolysis_glass::material::gpu::{COMPOSITE_FORMAT, f32_to_half, read_texture};
use hydrolysis_glass::material::recipe::Translucency;
use hydrolysis_glass::{Appearance, Element, Family, GlassRenderer, Group, Recipe, Scene};
use image::ImageEncoder as _;
use std::path::PathBuf;

/// Record scale: output pixels per pt (§7: 3 px/pt).
pub const PX_PER_PT: f32 = 3.0;

/// The scene's size, pt (§7 layout).
pub const SCENE_PT: [f32; 2] = [402.0, 874.0];

pub struct Gpu {
    pub adapter_name: String,
    pub backend: String,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
}

/// Every backend wgpu supports: the suite runs on the host's native GPU
/// backend, software rasterizer included.
pub const BACKENDS: wgpu::Backends = wgpu::Backends::all();

impl Gpu {
    /// The device and queue from whatever backend wgpu picks on the host.
    pub fn new() -> Self {
        let mut desc = wgpu::InstanceDescriptor::new_without_display_handle();
        desc.backends = BACKENDS;
        let instance = wgpu::Instance::new(desc);
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::LowPower,
            force_fallback_adapter: false,
            compatible_surface: None,
            apply_limit_buckets: false,
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
        tracing::info!(
            adapter = %info.name,
            backend = ?info.backend,
            "verification adapter"
        );
        Self {
            adapter_name: info.name,
            backend: format!("{:?}", info.backend),
            device,
            queue,
        }
    }
}

impl Default for Gpu {
    fn default() -> Self {
        Self::new()
    }
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

    #[allow(
        clippy::cast_precision_loss,
        reason = "pixel indices stay far below 2^24"
    )]
    pub fn from_fn(width: u32, height: u32, f: impl Fn(f32, f32) -> [f32; 4]) -> Self {
        let mut img = Self::new(width, height);
        for y in 0..height {
            for x in 0..width {
                img.pixels[(y * width + x) as usize] = f(x as f32 + 0.5, y as f32 + 0.5);
            }
        }
        img
    }

    /// Writes the scene PNG to `target/verification/<name>.png`.
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "channel values are clamped to [0, 1] before encoding to u8"
    )]
    pub fn save(&self, name: &str) -> PathBuf {
        let mut dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        dir.push("target/verification");
        std::fs::create_dir_all(&dir).ok();
        let mut bytes = Vec::with_capacity(self.pixels.len() * 4);
        for p in &self.pixels {
            for c in p {
                bytes.push((c.clamp(0.0, 1.0) * 255.0).round() as u8);
            }
        }
        let path = dir.join(format!("{name}.png"));
        let file = std::io::BufWriter::new(std::fs::File::create(&path).expect("create png"));
        image::codecs::png::PngEncoder::new_with_quality(
            file,
            image::codecs::png::CompressionType::Fast,
            image::codecs::png::FilterType::Sub,
        )
        .write_image(
            &bytes,
            self.width,
            self.height,
            image::ExtendedColorType::Rgba8,
        )
        .expect("write png");
        path
    }
}

pub const fn grey(v: f32) -> [f32; 4] {
    [v, v, v, 1.0]
}

/// The §7 split's red, encoded 8-bit.
pub const SPLIT_RED: [f32; 4] = [230.0 / 255.0, 38.0 / 255.0, 38.0 / 255.0, 1.0];
/// The §7 split's blue, encoded 8-bit.
pub const SPLIT_BLUE: [f32; 4] = [26.0 / 255.0, 90.0 / 255.0, 230.0 / 255.0, 1.0];

/// The §7 backdrops over the 402 × 874 pt scene.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Backdrop {
    /// Black.
    Black,
    /// Grey 0.5.
    Grey,
    /// White.
    White,
    /// Red | blue vertical split at the scene's centre.
    Split,
    /// 8 pt grid of 0.5 pt black lines on white.
    Grid,
    /// The grid over the split.
    GridOverSplit,
}

impl Backdrop {
    /// All §7 backdrops.
    pub const ALL: [Self; 6] = [
        Self::Black,
        Self::Grey,
        Self::White,
        Self::Split,
        Self::Grid,
        Self::GridOverSplit,
    ];

    /// The image at the scene's pixel size.
    #[must_use]
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "pixel coordinates are positive and far below u32::MAX"
    )]
    pub fn image(self) -> Image {
        let (w, h) = scene_px(SCENE_PT[0], SCENE_PT[1]);
        let cx = w / 2;
        Image::from_fn(w, h, |x, y| {
            let split = || {
                if (x as u32) < cx {
                    SPLIT_RED
                } else {
                    SPLIT_BLUE
                }
            };
            let gridline = |x: f32, y: f32| {
                // §7: 0.5 pt black lines every 8 pt — 1.5 px at 3 px/pt.
                // Each pixel's coverage is the line's area share of it:
                // one fully covered px and one half-covered px per line.
                let cover = |v: f32| {
                    0.5f32
                        .mul_add(PX_PER_PT, -(v - 0.5).rem_euclid(8.0 * PX_PER_PT))
                        .clamp(0.0, 1.0)
                };
                (1.0 - cover(x)).mul_add(-(1.0 - cover(y)), 1.0)
            };
            match self {
                Self::Black => grey(0.0),
                Self::Grey => grey(0.5),
                Self::White => grey(1.0),
                Self::Split => split(),
                Self::Grid => grey(1.0 - gridline(x, y)),
                Self::GridOverSplit => {
                    let covered = gridline(x, y);
                    let s = split();
                    [
                        s[0] * (1.0 - covered),
                        s[1] * (1.0 - covered),
                        s[2] * (1.0 - covered),
                        1.0,
                    ]
                }
            }
        })
    }

    /// The backdrop's name in the scene file name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Black => "black",
            Self::Grey => "grey",
            Self::White => "white",
            Self::Split => "split",
            Self::Grid => "grid",
            Self::GridOverSplit => "grid_split",
        }
    }
}

/// Uploads an image as the backdrop texture.
pub fn upload(g: &Gpu, img: &Image) -> wgpu::Texture {
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

/// The per-test harness: owns the device and the `GlassRenderer` every
/// scene reuses, and builds each backdrop's texture lazily on first use.
pub struct Harness {
    gpu: Gpu,
    renderer: GlassRenderer,
    /// Backdrops uploaded once per type.
    backdrops: Vec<UploadedBackdrop>,
}

/// A backdrop's texture view; the view keeps the texture alive.
struct UploadedBackdrop {
    kind: Backdrop,
    view: wgpu::TextureView,
}

impl Harness {
    /// A fresh harness: device, pipelines, layouts and target textures
    /// build once per test.
    #[must_use]
    pub fn new() -> Self {
        let gpu = Gpu::new();
        Self {
            renderer: GlassRenderer::new(&gpu.device),
            gpu,
            backdrops: Vec::new(),
        }
    }

    /// The device this harness renders on.
    pub const fn gpu(&self) -> &Gpu {
        &self.gpu
    }

    /// The backdrop's index in `backdrops`, uploaded on first use per
    /// type.
    fn backdrop_index(&mut self, backdrop: Backdrop) -> usize {
        if let Some(i) = self.backdrops.iter().position(|b| b.kind == backdrop) {
            return i;
        }
        let texture = upload(&self.gpu, &backdrop.image());
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        self.backdrops.push(UploadedBackdrop {
            kind: backdrop,
            view,
        });
        self.backdrops.len() - 1
    }

    /// Renders `scene` over `backdrop` and returns the output image.
    pub fn render(&mut self, backdrop: Backdrop, scene: &Scene) -> Image {
        let i = self.backdrop_index(backdrop);
        let out = self.renderer.render(
            &self.gpu.device,
            &self.gpu.queue,
            &self.backdrops[i].view,
            scene,
            None,
        );
        let size = out.size();
        let pixels = read_texture(&self.gpu.device, &self.gpu.queue, out);
        Image {
            width: size.width,
            height: size.height,
            pixels,
        }
    }

    /// Renders the scene and writes `target/verification/<name>.png`;
    /// returns the path.
    pub fn export(&mut self, backdrop: Backdrop, scene: &Scene, name: &str) -> PathBuf {
        let path = self.render(backdrop, scene).save(name);
        assert!(path.is_file(), "{name}: PNG written");
        path
    }
}

#[must_use]
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "point sizes are positive and far below u32::MAX"
)]
pub fn scene_px(w_pt: f32, h_pt: f32) -> (u32, u32) {
    ((w_pt * PX_PER_PT) as u32, (h_pt * PX_PER_PT) as u32)
}

#[allow(
    clippy::many_single_char_names,
    reason = "the parameters are the shape's x, y, w, h and radius"
)]
pub const fn rounded(x: f32, y: f32, w: f32, h: f32, r: f32) -> Shape {
    Shape {
        rect: Rect { x, y, w, h },
        radii: [r; 4],
        curve: CornerCurve::Continuous,
    }
}

pub fn capsule(x: f32, y: f32, w: f32, h: f32) -> Shape {
    rounded(x, y, w, h, h.min(w) / 2.0)
}

pub fn element(
    shape: Shape,
    family: Family,
    appearance: Appearance,
    translucency: Translucency,
) -> Element {
    let recipe = Recipe::for_element(shape.rect.w, shape.rect.h, family, appearance, translucency);
    Element { shape, recipe }
}

/// The twelve §7 elements as one scene's groups: standalone elements 1–5
/// and 7–12, the container element 6 as its own group.
#[must_use]
pub fn verification_groups(
    appearance: Appearance,
    translucency: Translucency,
    container_circles: [f32; 3],
) -> Vec<Group> {
    let reg = |shape: Shape| element(shape, Family::Regular, appearance, translucency);
    let clr = |shape: Shape| element(shape, Family::Clear, appearance, translucency);
    // Element 3: the only tinted member, white at declared 0.3.
    let mut e3 = reg(capsule(101.0, 380.0, 200.0, 80.0));
    e3.recipe = e3
        .recipe
        .with_tint(hydrolysis_glass::material::recipe::Tint {
            rgb: [1.0; 3],
            alpha: 0.3,
        });
    vec![
        Group::single(reg(rounded(51.0, 51.67, 300.0, 200.33, 40.0))),
        Group::single(clr(rounded(51.0, 276.0, 300.0, 80.0, 30.0))),
        Group::single(e3),
        Group::single(reg(capsule(121.0, 484.0, 160.0, 55.67))),
        Group::single(reg(capsule(168.0, 563.67, 66.0, 34.33))),
        Group::container(
            container_circles
                .iter()
                .map(|x| {
                    element(
                        Shape::circle(95.0 + x + 30.0, 622.0 + 30.0, 30.0),
                        Family::Regular,
                        appearance,
                        translucency,
                    )
                })
                .collect(),
            hydrolysis_glass::material::geometry::CONTAINER_UNION_SMOOTHING,
        ),
        Group::single(reg(capsule(45.0, 716.33, 40.0, 40.0))),
        Group::single(reg(capsule(101.0, 716.33, 80.0, 40.0))),
        Group::single(reg(capsule(197.0, 706.33, 160.0, 60.0))),
        Group::single(clr(capsule(45.0, 800.33, 40.0, 40.0))),
        Group::single(clr(capsule(101.0, 800.33, 80.0, 40.0))),
        Group::single(clr(capsule(197.0, 790.33, 160.0, 60.0))),
    ]
}

/// The §7 scene: all twelve elements at their frames, container
/// circles at `container_circles` x-offsets (rest: [0, 76, 152]).
#[must_use]
pub fn verification_scene(
    appearance: Appearance,
    translucency: Translucency,
    container_circles: [f32; 3],
) -> Scene {
    Scene::new(
        SCENE_PT,
        PX_PER_PT,
        verification_groups(appearance, translucency, container_circles),
    )
}
