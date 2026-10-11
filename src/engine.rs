//! The material realized on the in-tree engine (the `engine` feature).
//!
//! [`EngineHost`] owns a Cherenkov [`Engine`] on the GPU backend and one
//! readable offscreen [`Surface`], and realizes the recording contract the
//! theme speaks: [`EngineHost::mount`] installs a
//! [`Content::record_layered`](cherenkov_record::Content::record_layered)
//! result under the surface root exactly the way Hydrolysis's mount
//! realizes a `ChromeMaterial` run — the `below` content as a content
//! layer, then each [`BackdropMaterial`](cherenkov_record::BackdropMaterial)
//! as a member layer of the [`BackdropGroup`] its `(scope, capture
//! class)` key names, created from the class's registered
//! [`MaterialCapture`], then the run's `above` content, in paint order.
//!
//! The member's own terms bind through the same conversions Hydrolysis's
//! `chrome_spec`/`chrome_sample` apply: the class's logical union
//! smoothing and the effect's logical outer extent convert to device
//! pixels at the host's recording scale, and the scale itself rides the
//! sample as [`RecordingScale`] so `px.scale` converts the recipe's
//! lengths (water-rs/waterui#1788 and followers).

use std::collections::HashMap;
use std::num::NonZeroU64;
use std::rc::{Rc, Weak};

use cherenkov::kurbo::Affine;
use cherenkov::{
    BackdropGroup, BackdropId, BackdropOuter, BackdropSample, BackdropShader, BackdropSpec,
    BackdropUnion, Content, Display, Engine, EngineError, FrameTime, Image, ImageData, Layer,
    LayerId, Next, Offscreen, OffscreenFormat, Readback, RecordingScale, RenderError,
    ResourceError, Rgba16F, Surface, SurfaceError,
};
use cherenkov_gpu::{Gpu, GpuConfig};
use cherenkov_record::{
    CaptureClass, LayeredContent, MaterialCapture, MaterialEffect, MaterialGrouping,
    MaterialRegistry, MaterialScope, MaterialShader,
};

/// The engine backend the host drives.
pub type GpuEngine = Engine<Gpu>;

/// Why an [`EngineHost`] could not be built or draw a frame.
#[derive(Debug, thiserror::Error)]
pub enum EngineHostError {
    /// The GPU backend failed to initialize.
    #[error(transparent)]
    Engine(#[from] EngineError),
    /// The offscreen surface could not be created.
    #[error(transparent)]
    Surface(#[from] SurfaceError),
    /// A frame's render or readback failed.
    #[error(transparent)]
    Render(#[from] RenderError),
    /// An image or a backdrop shader could not be registered.
    #[error(transparent)]
    Resource(#[from] ResourceError),
}

/// One mounted [`LayeredContent`]: the layers [`EngineHost::mount`]
/// created and the `Rc` shares of every group its members sample.
///
/// Dropping it unmounts the content — a layer's drop removes it — while
/// the `Rc` keeps each shared group alive until the last mount sampling
/// it drops. A member sampling a released group fails the frame, so the
/// mount owns the group's lifetime for as long as it owns the members.
#[derive(Debug, Default)]
#[must_use = "dropping the mount unmounts the recording's layers"]
pub struct Mount {
    layers: Vec<Layer>,
    groups: Vec<Rc<BackdropGroup>>,
}

/// Which backdrop group a member joins: the tuple `(scope, capture
/// class)` — Hydrolysis's `ChromeGroupKey` at one install canvas, which
/// this host's single surface root is. Members in one scope and class
/// share one capture; a solo scope — or a member under a `Solo`
/// grouping — keys the member by its own layer, so it never shares.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct GroupKey {
    /// `MaterialScope::SOLO` maps to `Solo(member)`: the member is a
    /// group of its own.
    scope: ScopeKey,
    class: CaptureClass,
}

/// The scope half of [`GroupKey`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum ScopeKey {
    /// The member's own layer: a group of one, which keeps the
    /// first-member rule.
    Solo(LayerId),
    /// The host-identified scope the member's recording was opened
    /// under.
    Scoped(NonZeroU64),
}

/// An engine plus one readable offscreen surface, mounting layered
/// recordings of backdrop materials.
#[derive(Debug)]
pub struct EngineHost {
    engine: GpuEngine,
    surface: Surface<Gpu>,
    /// The device pixels one logical recording pixel spans: the
    /// `px.scale` member shaders convert the recipe's lengths by, and
    /// the scale the group's device-pixel terms convert at.
    scale: f64,
    /// The theme's registry: the capture classes resolve at member
    /// install and each registered shader's engine handle rides along.
    registry: MaterialRegistry,
    /// `MaterialShader` → the engine handle, filled by
    /// [`EngineHost::new`] registering every declared source.
    shaders: HashMap<MaterialShader, BackdropShader>,
    /// The live groups, keyed the way Hydrolysis's chrome table keys
    /// them: `(scope, capture class)` at this host's single canvas.
    /// A mount holds the `Rc`; the map weakly indexes it, so a group
    /// dies when the last mount sampling it unmounts — the engine's own
    /// release-once-unmounted rule.
    groups: HashMap<GroupKey, Weak<BackdropGroup>>,
}

impl EngineHost {
    /// An engine on `Gpu` with a readable `size`-pixel offscreen
    /// surface at `scale` device pixels per logical recording pixel,
    /// and `registry`'s shaders registered on it.
    ///
    /// The surface's display reports `scale` and SDR headroom; the
    /// offscreen target keeps linear `Rgba16Float` storage, so a
    /// [`render`](Self::render) readback is premultiplied linear
    /// Display P3.
    ///
    /// # Errors
    /// `EngineHostError::Engine` creating the backend,
    /// `EngineHostError::Surface` creating the surface, and
    /// `EngineHostError::Resource` when a declared shader source fails
    /// the engine's validation — the source is this crate's own WGSL,
    /// so a rejection is a bug in the shader, surfaced.
    pub fn new(
        size: (u32, u32),
        scale: f64,
        registry: MaterialRegistry,
    ) -> Result<Self, EngineHostError> {
        let engine = Engine::<Gpu>::new(GpuConfig::default())?;
        let surface = engine.surface(Offscreen::new(size, OffscreenFormat::LinearF16), || {})?;
        surface.display(Display {
            scale,
            headroom: 1.0,
        })?;
        let mut shaders = HashMap::new();
        for (key, source) in registry.shaders() {
            shaders.insert(key, engine.backdrop_shader(source.clone())?);
        }
        Ok(Self {
            engine,
            surface,
            scale,
            registry,
            shaders,
            groups: HashMap::new(),
        })
    }

    /// The engine the host drives.
    #[must_use]
    pub const fn engine(&self) -> &GpuEngine {
        &self.engine
    }

    /// The offscreen surface mounted recordings draw on.
    #[must_use]
    pub const fn surface(&self) -> &Surface<Gpu> {
        &self.surface
    }

    /// The host's recording scale: device pixels per logical recording
    /// pixel.
    #[must_use]
    pub const fn scale(&self) -> f64 {
        self.scale
    }

    /// The theme's registry the members' terms resolve against.
    #[must_use]
    pub const fn registry(&self) -> &MaterialRegistry {
        &self.registry
    }

    /// The engine handle a declared shader key resolved to.
    #[must_use]
    pub fn shader(&self, key: MaterialShader) -> Option<&BackdropShader> {
        self.shaders.get(&key)
    }

    /// How many live backdrop groups the host keeps: a group lives while
    /// a mounted member samples it.
    #[must_use]
    pub fn group_count(&self) -> usize {
        self.groups
            .values()
            .filter(|g| g.strong_count() > 0)
            .count()
    }

    /// Registers an image with the engine — the backdrop content a
    /// mounted recording's capture samples.
    ///
    /// # Errors
    /// `ResourceError` when the data fails the engine's image limits.
    pub fn image(&self, image: ImageData<Rgba16F>) -> Result<Image<Rgba16F>, ResourceError> {
        self.engine.image(image)
    }

    /// Mounts recorded `content` as a layer under the surface root at
    /// the host's recording scale: the recording's logical coordinates
    /// map onto the surface's device pixels by it.
    pub fn mount_content(&mut self, content: Content) -> Layer {
        let layer = self.surface.layer();
        let t = Affine::scale(self.scale);
        self.surface.update(|tx| {
            tx[&layer].transform(t);
            tx[&layer].content(content);
            tx[self.surface.root()].push(&layer);
        });
        layer
    }

    /// Realizes `layered` under the surface root at the host's
    /// recording scale, mirroring Hydrolysis's mount of a layered
    /// recording: the `below` content, then each `BackdropMaterial` as
    /// a member layer — clip its live shape, backdrop its live sample —
    /// of the group its `(scope, capture class)` key names, then the
    /// run's `above` content. Members of one scope and class share one
    /// capture; `SOLO` members and `Solo` classes stay groups of one.
    ///
    /// # Panics
    /// When a material names a shader or capture class the registry
    /// does not declare — like Hydrolysis's mount, a material outside
    /// the theme's declared contract is a programmer error, not a
    /// silent sample — or when a class's union smoothing is invalid at
    /// the host's scale.
    pub fn mount(&mut self, layered: LayeredContent) -> Mount {
        let Self {
            surface,
            scale,
            registry,
            shaders,
            groups,
            ..
        } = self;
        let scale = *scale;
        let t = Affine::scale(scale);
        let mut mount = Mount::default();
        surface.update(|tx| {
            mount_content_tx(tx, surface, layered.below, t, &mut mount);
            for run in layered.runs {
                let material = run.material;
                let member = surface.layer();
                tx[&member].transform(t);
                // The clip rebinds from the shape's value now, like
                // Hydrolysis's `lower_chrome`: a binding starts from
                // the signal's current value, never a recorded one.
                tx[&member].clip(material.shape().rebound());
                let group = group_for(
                    surface,
                    registry,
                    groups,
                    material.scope(),
                    material.capture(),
                    member.id(),
                    scale,
                );
                let shader = shaders
                    .get(&material.shader())
                    .unwrap_or_else(|| {
                        panic!(
                            "hydrolysis-glass: backdrop shader {:?} is not registered; the theme \
                             declares its shaders through `WidgetTheme::register_backdrop_shaders`",
                            material.shader(),
                        )
                    })
                    .clone();
                let id = group.id();
                let sample = material
                    .effect()
                    .rebound()
                    .map(move |effect| sample_on(id, &shader, &effect, scale));
                tx[&member].backdrop(sample);
                tx[surface.root()].push(&member);
                mount.layers.push(member);
                mount.groups.push(group);
                mount_content_tx(tx, surface, run.above, t, &mut mount);
            }
        });
        mount
    }

    /// Draws one frame and returns what the engine asks for next.
    ///
    /// # Errors
    /// `EngineHostError::Render` when the frame fails.
    pub fn render(&self) -> Result<Next, EngineHostError> {
        Ok(self.engine.render(FrameTime::now())?)
    }

    /// Draws one frame and reads the surface back: premultiplied linear
    /// Display P3 pixels, row-major.
    ///
    /// # Errors
    /// `EngineHostError::Render` when the frame or the readback fails.
    pub fn render_readback(&self) -> Result<Readback, EngineHostError> {
        self.render()?;
        Ok(self.surface.readback()?)
    }
}

/// A `Content` as a layer under the surface root, skipped when empty —
/// the `below`/`above` parts of a `LayeredContent` that recorded nothing
/// draw nothing.
fn mount_content_tx(
    tx: &mut cherenkov::Transaction<'_, Gpu>,
    surface: &Surface<Gpu>,
    content: Content,
    t: Affine,
    mount: &mut Mount,
) {
    if content.is_empty() {
        return;
    }
    let layer = surface.layer();
    tx[&layer].transform(t);
    tx[&layer].content(content);
    tx[surface.root()].push(&layer);
    mount.layers.push(layer);
}

/// The group `member` joins for `scope` and `class`, created on first
/// use from the class's registered [`MaterialCapture`].
///
/// The key mirrors Hydrolysis's `ChromeGroupKey`: a `SOLO` scope or a
/// `Solo` grouping keys the member by its own layer so it never shares;
/// a scoped member shares the `(scope, class)` group, which anchors at
/// the first member — the recordings this host mounts carry no
/// `.material_group()` anchor items, so the first-member rule is the
/// scope's capture position.
///
/// # Panics
/// As [`EngineHost::mount`] does for an unregistered class or an
/// invalid union smoothing.
fn group_for(
    surface: &Surface<Gpu>,
    registry: &MaterialRegistry,
    groups: &mut HashMap<GroupKey, Weak<BackdropGroup>>,
    scope: MaterialScope,
    class: CaptureClass,
    member: LayerId,
    scale: f64,
) -> Rc<BackdropGroup> {
    let params = registry.capture_class(class).copied().unwrap_or_else(|| {
        panic!(
            "hydrolysis-glass: capture class {class:?} is not registered; the theme declares its \
             classes through `WidgetTheme::register_backdrop_shaders`"
        )
    });
    let scope = match scope.id() {
        Some(scope) if params.grouping != MaterialGrouping::Solo => ScopeKey::Scoped(scope),
        _ => ScopeKey::Solo(member),
    };
    let key = GroupKey { scope, class };
    if let Some(group) = groups.get(&key).and_then(Weak::upgrade) {
        return group;
    }
    let mut spec = BackdropSpec::new(params.scale, params.levels).blend_space(params.blend_space);
    if let Some(union) = union_of(&params, class, scale) {
        spec = spec.union(union);
    }
    let group = Rc::new(surface.backdrop_group_unfiltered(spec));
    groups.insert(key, Rc::downgrade(&group));
    group
}

/// The union field `params` declares for a group built at `scale`:
/// `None` for `Solo` and `Shared` classes; for `Union`, the class's
/// logical smoothing converted to device pixels — [`BackdropUnion`]'s
/// input — with its error surfaced, never clamped.
///
/// # Panics
/// Panics, naming the class, smoothing and scale, when the converted
/// smoothing is not a valid [`BackdropUnion`].
fn union_of(params: &MaterialCapture, class: CaptureClass, scale: f64) -> Option<BackdropUnion> {
    let MaterialGrouping::Union { smoothing } = params.grouping else {
        return None;
    };
    #[expect(
        clippy::cast_possible_truncation,
        reason = "a union smoothing is a display-scale length far inside f32"
    )]
    let device = (f64::from(smoothing.get()) * scale) as f32;
    Some(BackdropUnion::new(device).unwrap_or_else(|error| {
        panic!(
            "hydrolysis-glass: capture class {class:?} union smoothing {} is invalid at \
                 recording scale {scale}: {error}",
            smoothing.get(),
        )
    }))
}

/// The member's logical outer `extent` as device pixels at `scale` —
/// the recording scale the group was built for.
///
/// # Panics
/// Panics when `extent` converted to device pixels is not a valid
/// [`BackdropOuter`]: its error is surfaced, never clamped.
fn outer_of(extent: cherenkov_record::OuterExtent, scale: f64) -> BackdropOuter {
    #[expect(
        clippy::cast_possible_truncation,
        reason = "an outer extent is a display-scale length far inside f32"
    )]
    let device = (f64::from(extent.get()) * scale) as f32;
    BackdropOuter::new(device).unwrap_or_else(|error| {
        panic!(
            "hydrolysis-glass: outer extent {} is invalid at recording scale {scale}: {error}",
            extent.get(),
        )
    })
}

/// The recording `scale` as the member's [`RecordingScale`]: the device
/// pixels one logical pixel of the recording spans.
///
/// # Panics
/// Panics when `scale` is not a valid recording scale: its error is
/// surfaced, never clamped.
fn recording_scale_of(scale: f64) -> RecordingScale {
    #[expect(
        clippy::cast_possible_truncation,
        reason = "a recording scale is a small positive ratio far inside f32"
    )]
    let scale = scale as f32;
    RecordingScale::new(scale).unwrap_or_else(|error| {
        panic!("hydrolysis-glass: recording scale {scale} is invalid: {error}")
    })
}

/// The backdrop sample a member binds: the group's id and the member's
/// effect mapped through its shader, the effect's logical outer extent
/// converted to device pixels at `scale`, and `scale` itself as the
/// member's recording scale so its shader converts the recipe's logical
/// lengths — Hydrolysis's `group_sample_with`, verbatim in spirit.
fn sample_on(
    id: BackdropId,
    shader: &BackdropShader,
    effect: &MaterialEffect,
    scale: f64,
) -> BackdropSample {
    BackdropSample::with_effect(id, shader.effect(effect.uniforms().to_vec()))
        .outer(outer_of(effect.outer_extent(), scale))
        .scale(recording_scale_of(scale))
}
