# hydrolysis-glass

A glass material widget theme for the `WaterUI` Hydrolysis backend: translucent
surfaces that refract, blur and tint the content behind them, with rim
highlights and shape-merging interaction.

- `material` — the material itself: CPU signed-distance geometry, recipes and
  the wgpu pipeline (`GlassRenderer`) that renders a `Group` of elements over a
  captured backdrop.
- `interaction` — press, drag, corner morphing and neighbour attraction as
  `nami` signals publishing a `Binding<Group>`.
- `theme` — `Glass`, a `hydrolysis::Style` and `WidgetTheme`. Buttons and
  capsules, tab bars, toolbars, sheets and containers are glass; the theme
  places them (`theme::surfaces`) and everything else is drawn through
  `DrawContext`.

The glass surfaces need a backdrop capture the renderer's `DrawContext` cannot
provide, so the theme refuses to draw them there instead of drawing a
look-alike. Rendering through the engine sits behind the off-by-default `engine`
feature; `ENGINE_REQUIREMENTS.md` lists exactly which engine APIs it waits for.
