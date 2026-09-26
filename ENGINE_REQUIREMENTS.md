# Engine requirements

The glass material samples the content *behind* each surface: it captures the
backdrop, downsamples and blurs it, and each member reads that shared result
through its own refraction, colour, shadow, highlight and tint stages. None of
this is expressible through Hydrolysis's `DrawContext`, whose primitives are
fills, strokes, paths, shadows, layers and transforms with no access to what
was painted before. The theme therefore refuses to draw a glass surface through
`WidgetTheme` (it panics naming this document) and the `engine` feature is a
`compile_error!` until the engine ships the APIs below.

Everything in this file is a statement about `water-rs/cherenkov` at revision
`a77d2b6571b5a65f59a7fbd1e279dbdaec75a4d0`, checked against `docs/api.md` and
`src/`.

## Taking the dependency

`cherenkov`'s manifest requires `nami-core` with the `kurbo` feature, which is
not on any released `nami-core`; the manifest pins a git revision of `nami`
for that. A released `nami 0.11` does not provide it:

```text
package `cherenkov` depends on `nami-core` with feature `kurbo`
but `nami-core` does not have that feature
```

The resolution path — not a blocker — is the project's rule for unreleased
changes: consume them through a `[patch.crates-io]` git pin to the exact commit
on the dependency's dev branch, never a branch or a path. The `kurbo` feature
lives at `6908aac816de14ee5aff23bfffd7d2b109d08fea` on `nami`'s dev branch
(water-rs/nami#28; it lands with water-rs/nami#27), and `cherenkov` itself is
unpublished (`version = "0.0.0"`, `publish = false`), so it arrives as a git
dependency pinned to the same revision this document was checked against:

```toml
[dependencies]
cherenkov = { git = "https://github.com/water-rs/cherenkov", rev = "a77d2b6571b5a65f59a7fbd1e279dbdaec75a4d0" }

[patch.crates-io]
nami = { git = "https://github.com/water-rs/nami", rev = "6908aac816de14ee5aff23bfffd7d2b109d08fea" }
nami-core = { git = "https://github.com/water-rs/nami", rev = "6908aac816de14ee5aff23bfffd7d2b109d08fea" }
```

Once a released `nami` ships `kurbo`, the patch table and the `nami` pins go
away; the `cherenkov` git pin stays until the crate is published.

## Documented but not exposed in source

`docs/api.md` § Backdrop describes the exact shape the material needs:

```rust
let glass: BackdropGroup = surface.backdrop_group(Blur::new(24.0).then(Saturation(1.8)));
tx[&toolbar].backdrop(glass.sample(refraction.clone()));
tx[&tab_bar].backdrop(glass.sample(refraction));
```

"Absent" below means absent from the `cherenkov` crate at the pinned revision
— its `src/` (`color.rs`, `display_list.rs`, `glyph.rs`, `paint.rs`,
`record.rs`, `shape.rs`, `style.rs`) is a recording layer only, with no
`BackdropGroup`, `backdrop_group`, `BackdropGroup::sample`, layer `backdrop`
property or `Backdrop` capability. `ShaderPaint { shader: ShaderId, uniforms:
Vec<f32> }` and `Paint::Shader` exist, but a `ShaderPaint` paints a shape from
its own inputs — it has no backdrop to sample.

The workspace's `filtrate` crates (already in this crate's dependency graph) do
provide the contract types the "absent" rows would otherwise imply:
`filtrate-core` 0.2.0 defines `trait SpatialFilter` with `footprint() ->
Footprint`, `Chain` and `ColorFilter`, and `filtrate` 0.2.1 defines `Blur<T>`
with a wgpu executor. `SpatialFilter::footprint` is exactly the blur-contract
carrier the capture row wants; what is still missing is a mip-mapped capture the
member shader can sample with an explicit LOD (filtrate's executor produces
`mip_level_count: 1`) and the backdrop plumbing inside `cherenkov` proper.

Likewise signal support already exists: `src/record.rs` implements `Live<T>:
From<S: Signal>` — a snapshot plus `signal.watch` producing `SlotUpdate`s, with
`Content` owning the guards — exactly the signal-bound value slots
`docs/api.md` § Recording describes. What is absent is a layer/property surface
to bind them to (`scene`'s `Layer` is a serde snapshot with transform, clip,
opacity, blend and items only — no backdrop or filter field).

## What the material needs, stage by stage

| Material stage (this crate) | Engine capability required | Status at pinned revision |
| --- | --- | --- |
| Capture at `0.25` (regular) / `0.5` (clear) of device scale, premultiplied sRGB | `surface.backdrop_group(...)`: one capture per group at the group's paint-order position, with a `SpatialFilter` whose `footprint()`-style blur contract permits reduced resolution | documented; `SpatialFilter` exists in `filtrate-core`, absent from `cherenkov` |
| Preblur + ~9-level mip chain, depth-dependent blur `lod = log2(max(r_eff, ε))` | the group's spatial filter chain producing a mip-mapped result the member shader can sample with an explicit LOD | `filtrate`'s `Blur<T>` + executor exist but emit `mip_level_count: 1`; mip-mapped capture and explicit-LOD member sampling absent |
| Shared union field with nearest-member ownership; `8 pt` standalone / `30 pt` container smoothing | `BackdropGroup` members sharing one capture *and* the member effect receiving the shapes of all members (a "filter that takes shape input") | mentioned in `docs/api.md` § Backdrop, absent |
| Refraction (inner/outer lobes, `2×2` displacement), face, bleed, shadow, highlight, tint, SDR seam, foreground dispersion | `glass.sample(effect)` where `effect` is a user shader (`ShaderPaint`-like) that reads the shared capture at a displaced coordinate and LOD; time and signal-bound uniforms | `ShaderPaint` exists for ordinary paints; backdrop sampling from a shader is absent |
| Signal-driven press/drag/attraction (`nami::Binding<Group>`) | layer properties accepting nami signals so the member shapes and uniforms update without a transaction per frame | the mechanism exists (`Live<T>: From<S: Signal>`, `signal.watch` -> `SlotUpdate`s in `src/record.rs`); the `Layer`/property surface to bind to is absent |

## What `hydrolysis-glass` already provides

The material is complete on wgpu (`GlassRenderer`, `shaders/*.wgsl`) against a
caller-supplied backdrop texture, and the theme already decides silhouette and
recipe for every glass surface (`theme::surfaces`, `Glass::button_surface`,
`tab_bar_surface`, `toolbar_surface`, `sheet_surface`, `container_surface`,
`Glass::group`). Integration is a matter of feeding a `Group` and a capture to
`GlassRenderer`, or of expressing the same stages as `BackdropGroup` members
once the engine exposes them.

## What the `engine` feature will do

When the APIs above exist and the crate resolves, the `engine` feature will:

1. create one `BackdropGroup` per `Group` (per union), at the paint-order
   position of the group's lowest member;
2. install the capture scale, preblur and mip chain as the group's spatial
   filter;
3. attach `glass.sample(material)` to every member layer, passing the member's
   `Recipe` and the whole group's shapes as uniforms;
4. bind the theme's `Binding<Group>` from `Interactive` to those uniforms.

Until then, enabling `engine` fails to build with a message pointing here. There
is no stand-in: a glass surface is either rendered by the material or not drawn.
