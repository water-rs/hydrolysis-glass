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
- `interaction` — press (a uniform ~1.24 bounds growth at full press), drag,
  corner morphing and neighbour attraction as `nami` signals publishing a
  `Binding<Group>`; interaction never changes the recipe.
- `theme` — `Glass`, a `hydrolysis::Style` and `WidgetTheme`. Buttons and
  capsules, tab bars, toolbars, sheets and containers are glass; the theme
  places them (`theme::surfaces`) and everything else records through the
  framework's `Recorder`.

The glass surfaces need a backdrop capture with explicit mip levels that the
recorder cannot yet express, so the theme refuses to draw them there instead
of drawing a look-alike. Rendering through the engine sits behind the
off-by-default `engine` feature, which fails to compile until the engine
grows the APIs; [`ENGINE_REQUIREMENTS.md`](ENGINE_REQUIREMENTS.md) lists
exactly which ones it waits for and how the material's stages map onto them.

The material follows [`docs/specification.md`](docs/specification.md), which
tags every recipe value with its evidence: measured, fitted or chosen.
`cargo nextest run` renders the specification's verification scenes — its
twelve elements over six backdrops in both appearances and at the
translucency extremes — to `target/verification/*.png` for review.
