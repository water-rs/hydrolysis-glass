# hydrolysis-glass

A glass material widget theme for the `WaterUI` Hydrolysis backend: translucent
surfaces that refract, blur and tint the content behind them, with rim
highlights and shape-merging interaction.

- `material` — the material itself: CPU signed-distance geometry, per-element
  recipes and the wgpu pipeline (`GlassRenderer`) that renders a `Group` of
  elements over a captured backdrop. Recipes come in two families —
  `Family::Regular` (frosted, appearance-dependent) and `Family::Clear`
  (nearly transparent, identical in both appearances) — and scale with the
  element's short side, the `Appearance` and a user `Translucency` setting.
- `interaction` — drag (the interactive material never grows on press or drag),
  corner morphing and neighbour attraction as `nami` signals publishing a
  `Binding<Group>`; interaction never changes the recipe.
- `theme` — `Glass`, a `hydrolysis::Style` and `WidgetTheme`. Buttons and
  capsules, tab bars, toolbars, sheets and containers are glass; the theme
  places them (`theme::surfaces`) and everything else records through the
  framework's `Recorder`.

The glass surfaces draw as Cherenkov backdrop materials: the theme records
each surface as a `backdrop_material` member under the recording contract's
shared union field, capture class and sRGB-encoded blend space, and the
engine samples a captured, mip-levelled backdrop behind the widget — never a
look-alike (`theme::materials`, `shaders/material_engine.wgsl`). Hydrolysis
owns that mount: it registers the theme's shader and capture classes, groups
the members a `.material_group()` scope holds into one capture and one union
field, and runs the engine pass. The standalone `GlassRenderer` wgpu path
stays for hosts that render `Group`s directly.

The material follows [`docs/specification.md`](docs/specification.md), which
tags every recipe value with its evidence: measured, fitted or chosen.
`cargo nextest run` renders the specification's verification scenes — its
twelve elements over six backdrops in both appearances and at the
translucency extremes — to `target/verification/*.png` for review. The
standalone renderer draws those scenes directly (`tests/gpu.rs`); beside
them, `tests/hydrolysis.rs` mounts real glass-styled views on Hydrolysis's
headless runtime and compares the engine's frame against the standalone
render of the same material.
