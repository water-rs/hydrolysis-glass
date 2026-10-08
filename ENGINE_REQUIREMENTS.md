# Engine requirements

The glass material samples the content *behind* each surface: it captures the
backdrop, downsamples and blurs it through a mip pyramid, and each member
reads that shared result through its own refraction, colour, shadow,
highlight and tint stages. The theme therefore refuses to draw a glass
surface through `WidgetTheme` (it panics naming this document and
water-rs/waterui#1788) and the `engine` feature is a `compile_error!` until
the engine ships the APIs listed below.

Everything in this file is a statement about the Chérenkov engine at
water-rs/waterui revision `46552dd30ef8fcd84b85b7003496d7b3e5606d3d` — the
crate moved in-tree to `graphics/cherenkov` (water-rs/waterui#1449) —
checked against `graphics/cherenkov/src/` and
`graphics/cherenkov/record/src/`.

## What the engine provides at the pinned revision

- **Backdrop groups.** `Surface::backdrop_group(filter)` and
  `backdrop_group_unfiltered()` (in `cherenkov::surface`) allocate a
  `BackdropGroup`: one capture and one spatial filter chain shared by its
  members. `filter` is any `BackdropChain<K: filtrate_core::Kind>` — today a
  `SpatialFilter` or `ColorFilter`, so `Blur`, `Chain` and friends from
  `filtrate` compose the capture.
- **Per-member effects.** `BackdropGroup::sample()` /
  `sample_with(effect)` produce a `BackdropSample` that
  `LayerEdit::backdrop` attaches to a layer; the layer composites the
  group's capture as the bottom-most draw inside its clip. The effect is a
  `BackdropEffect`: a premultiplied `ColorMatrix`, an analytic
  `Refraction`, a `Rim`, or a registered shader.
- **Backdrop shaders.** `Engine::backdrop_shader(BackdropShaderSource)`
  validates and registers a WGSL fragment that defines
  `backdrop_effect(p, sdf, normal, size, params) -> vec4<f32>` — `p` the
  member pixel centre in device space, `sdf` and `normal` the member's clip
  edge, `size` the member's device bounds — calling
  `backdrop_sample(q)` to read the filtered capture bilinearly.
  `BackdropShader::effect(uniforms)` builds the `BackdropEffect::Shader`
  carrying up to 64 floats (packed four per `vec4`) and the shader's
  declared `reach` grows the group's capture region so displaced samples
  stay inside it. The result is written premultiplied and unclamped.
- **Shader paints.** `Paint::Shader(ShaderPaint)` records an arbitrary
  registered shader over any shape inside ordinary content — the
  foreground and overlay layers of the material can be recorded this way.
- **Signal-bound values.** Recording takes `Live<T>` operands
  (`From<S: Signal>`): shapes, paints, strokes, shadows, group properties
  and layer edits bind nami signals and update incrementally without
  re-recording.

## What is still missing

Three pieces keep the material's `WidgetTheme` path refused:

1. **Capture scale and mip-mapped capture with explicit-level sampling**
   (water-rs/waterui#1786). A group's spatial chain produces a
   single-resolution filtered result at the member's paint scale, and
   `backdrop_sample` takes no LOD. The material needs a capture at the
   recipe's `capture_scale` and a box-filtered pyramid (≥ 9 levels) sampled
   at `level(b)` — `log2`-scaled from the blurred depth — inside
   `backdrop_effect`.
2. **A union field across group members, and an outer extent**
   (water-rs/waterui#1787). `backdrop_effect`'s `sdf`/`normal` describe the
   member's own clip, not the union of the group's members, and the effect
   is evaluated inside the clip. The material's members share one smooth
   union field (8 pt standalone / 30 pt container smoothing) that owns
   every pixel, and the drop shadow is drawn *outside* the member clip —
   so the effect contract needs the union field (all member shapes) and an
   outer extent to draw into.
3. **Backdrop chrome from a widget theme** (water-rs/waterui#1788).
   `WidgetTheme`'s drawing methods receive a `Recorder` over the widget's
   recorded content; there is no way to create a `BackdropGroup`, attach a
   `BackdropSample` to a member, or register the group at the widget's
   paint-order position. The theme records fills, strokes, shadows, clips,
   transforms, groups, glyphs, images and pictures — none of them samples
   the backdrop — so the glass surfaces panic rather than record a
   look-alike.

## How the material's stages map once the pieces exist

- One `BackdropGroup` per `Group` (per smooth union), created at the paint
  order of the group's lowest member, running a pyramid chain at the
  recipe's `capture_scale` (#1786).
- One `BackdropShader` carrying the glass-body model of spec §4.1–§4.11 —
  refraction, blur at `level(b)`, haze, luminance limit, grade, ambient
  pickup, contour shade, border correction, cast shadow, channel ceiling —
  evaluated per member through `sample_with` against the group's union
  field (#1787); the cast shadow draws in the effect's outer extent
  (#1787).

  `BackdropShader::effect` takes `params: array<vec4<f32>, 16>` — 64 floats
  — while the recipe block is 23 vec4 (92 floats), so the member's `Recipe`
  cannot go into `params` wholesale. Deriving `Recipe::for_size` over every
  family, appearance, size and translucency splits its fields by where the
  values are consumed — fields constant across every recipe become shader
  constants of whichever layer reads them, the capture scale belongs to
  the `BackdropGroup`, the highlight aperture and tint ride on the
  `Paint::Shader` layers, and the rest are the backdrop shader's `params`:

  | consumer | `Recipe` fields | floats |
  |---|---|---|
  | highlight-layer shader constants (constant across recipes) | highlight bearings, gains, inner fade, knee, glow multipliers and both light colours | 17 |
  | backdrop-shader constants (constant across recipes) | rim ramp (2), blur depths 1–3 (3), grade mix (1), ambient ramp (2), border strength, bearing, band width, band offset and colour bias (5), cast shadow shift (2) | 15 |
  | union field / shape | ellipse bend (the field pass bends the normal the highlight and tint also read) | 1 |
  | `BackdropGroup` | capture scale | 1 |
  | `Paint::Shader` layers | highlight aperture (1), tint flag and tint (5) | 6 |
  | backdrop shader `params` | blur radius (1), blur depth 0 (1), blur gains (4), primary and rim lobe shift and depth (4), rim mix (1), luminance limit (1), grade floor/ceiling/chroma (3), wash colour and alpha (4), ambient shift/depth/blur/strength (4), ambient grade (3), ambient gate (1), haze radius/min weight/max weight/mix (4), contour shade density/shift/softness/width/clip (5), border aperture (1), cast shadow falloff/density/gain (3), channel ceiling (1), exterior span (1) | 42 |

  The backdrop shader's `params` are 42 floats: they fit the 64-float
  `params` with 22 to spare (#1786).

- The highlight (§4.12) and tint (§4.13) layers recorded as `Paint::Shader`
  fills over the member shape in the widget's recorded content, above the
  backdrop composite; the foreground content records above them with its
  dispersion (#1788).
- The theme's `Binding<Group>` bound through `Live` operands so press,
  drag and attraction update the recorded uniforms without a transaction
  per frame.

## What `hydrolysis-glass` already provides

The material is complete on wgpu (`GlassRenderer`, `shaders/*.wgsl`)
against a caller-supplied backdrop texture, and the theme already decides
silhouette and recipe for every glass surface (`theme::surfaces`,
`Glass::button_surface`, `tab_bar_surface`, `toolbar_surface`,
`sheet_surface`, `container_surface`, `Glass::group`). Integration is a
matter of feeding a `Group` and a capture to `GlassRenderer`, or of
expressing the same stages as the group's members once the engine exposes
them.

Until then, enabling `engine` fails to build with a message pointing here.
There is no stand-in: a glass surface is either rendered by the material or
not drawn.
