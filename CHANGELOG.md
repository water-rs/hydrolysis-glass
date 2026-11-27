# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0](https://github.com/water-rs/hydrolysis-glass/releases/tag/v0.1.0) - 2026-11-27

### Added

- *(tabs)* set 23 pt stacked and 18 pt inline tab icon metrics
- *(material,theme)* inset glass body perimeter ring, feature gating, and parity refresh
- *(theme)* configure cross-platform Inter fallback and exterior perimeter ring
- *(theme)* implement WidgetTheme::code and wire TabsLook::label_font
- *(theme)* [**breaking**] implement specification motion dynamics, sheets chrome, and control parity
- *(floating)* implement WidgetTheme::floating with continuous glass surface
- *(modals)* implement WidgetTheme dialog and sheet looks for Glass
- *(snackbar)* implement WidgetTheme::snackbar for Glass
- *(card)* implement WidgetTheme::card with glass container surface
- *(menu)* calibrate anisotropic popover morphing, blur reveal, and layout metrics
- *(material,theme)* align lens refraction, HDR press boost, sibling bleed, and P3 colorimetry
- *(material)* wire live engine cross-element emissive color bleed
- *(material)* native Display P3, HDR specular rims, cross-element bleed, and continuous translucency
- *(dynamics)* add analytical springs, press scale, jelly drag, and morph helpers
- *(material)* calibrate haze, color grade, prominent transfer, and rim optics
- *(theme)* wire §4.14 content dispersion effect onto glass chrome
- *(theme)* [**breaking**] draw text fields, menus, pickers, radio groups, navigation and tabs as looks
- *(theme)* the glass segmented control as a segmented look
- *(theme)* a glass background behind arbitrary content
- *(theme)* implement the glass slider from the specification
- *(theme)* implement the glass segmented control from the specification
- *(theme)* implement the glass switch from the specification
- *(material)* draw the prominent button fill flat, under the highlight
- *(theme)* implement the glass button from the specification
- *(engine)* render the glass material through Cherenkov backdrop groups
- glass material theme

### Fixed

- *(material)* calibrate rim highlight, control spacing, and sync waterui opsz
- *(theme)* align bordered button fill and control vertical spacing with specification
- *(table)* align TableMetrics::outline_width with f32
- *(theme)* polish switch lens, tab glide, menu morph, and dark bordered button
- *(material)* inline prominent tint blend cleanly
- *(material)* simplify prominent tint and single-pass layer compositing
- *(material)* match branchless backdrop_effect control flow
- *(material)* scope exterior hairline outside glass_body
- *(material)* keep highlight and tint compositing branchless
- *(material)* simplify tint-order dispatch and branchless hairline
- *(theme)* align bordered/glass buttons and text fields with target parity
- *(theme)* improve progress, slider, segmented, and stepper visual parity
- *(theme)* draw the menu picker's chevrons in the indicator space
- *(theme)* fade disabled controls in encoded space and the lens with the press
- *(material)* keep the backdrop opaque under the standalone cast shadow
- blend the union over its smoothing distance
- fuse the standalone union field with the engine's quadratic smooth minimum
- play every spring as its physical step response
- *(material)* take the field's derivative before the engine shader branches
- *(material)* keep the blur and haze deviations in points
- *(material)* compute the highlight and tint layers once
- *(material)* declare the tint's effective alpha in the recipe
- *(material)* blur the haze by a Gaussian of its radius
- *(material)* reduce the composite mips by 2x2 means
- *(material)* run the blur reads straight through
- *(material)* blur by a fitted Gaussian instead of a mip level
- *(interaction)* grow only the glass button on press

### Other

- *(theme)* [**breaking**] sync hydrolysis-style f32 fields and logical-pixel constants
- replace waterui facade dependency with foundation crates
- Merge remote-tracking branch 'origin/main' into refactor/5129-snackbar-look
- Merge remote-tracking branch 'origin/main' into feat/menu-liquid-morph
- calibrate Display P3 foreground, accent, and encoded track blending
- Merge remote-tracking branch 'origin/main' into feat/81-nav-tabs-contracts
- wire inline navigation title font and compact floating tab pill
- transparent resting navigation bar and refined progress sizing
- update dark palette elevation and encode controls in Display P3
- *(diag)* [2/2] isolate cubic add_highlight + hairline without prominent_tint_rgb
- *(diag)* isolate cubic add_highlight + recipe without prominent_tint or hairline
- wire dynamic press morphs, drag stretch, and popover motion
- *(parity)* address review cleanup and check reference blank frames
- *(parity)* replace custom apple_parity harness with water preview + waterui-testing
- align light surface polarity and tab press lens
- update Cargo.lock to waterui c77075dc
- replace fwidth(px.sdf) with analytical normal norm
- drop palette background change and update waterui lock to 1124cee2
- align window background palette tokens and lock waterui main
- improve text field, bordered button, and checkbox visual parity
- *(progress)* install Glass theme tokens in progress look unit tests
- Merge remote-tracking branch 'origin/main' into feat/14-controls-sheets
- Merge remote-tracking branch 'origin/main' into feat/14-controls-sheets
- specify and verify glass sheet surface (§8.10)
- Merge remote-tracking branch 'origin/main' into feat/13-controls-menus-popups
- *(parity)* add per-component offscreen FLIP parity harness
- specify and refine menus and menu pickers
- *(navigation)* drop duplicate comment line
- Merge remote-tracking branch 'origin/main' into feat/12-controls-bar-items
- Merge remote-tracking branch 'origin/main' into feat/12-controls-bar-items
- *(theme)* compare recorded chevron debug output for RTL
- Merge remote-tracking branch 'origin/main' into feat/11-controls-tab-bar
- Merge remote-tracking branch 'origin/main' into feat/12-controls-bar-items
- specify and align navigation bar and bar items
- Merge remote-tracking branch 'origin/main' into feat/11-controls-tab-bar
- Merge origin/main and restore inward focus outline inset
- specify and align glass tab bar
- Merge remote-tracking branch 'origin/main' into feat/16-controls-text-search-fields
- specify and align text and search fields
- specify and align glass stepper
- update to waterui main, hydrolysis-style, and Surface menu panel
- Merge origin/main into theme/61-widget-looks
- Merge the final dev into theme/61-widget-looks
- switch water-rs git dependencies and workflow refs from dev to main ([#78](https://github.com/water-rs/hydrolysis-glass/pull/78))
- *(deps)* point migrated Batch 2 git dependencies to main ([#75](https://github.com/water-rs/hydrolysis-glass/pull/75))
- *(deps)* point migrated Batch 1 git dependencies to main ([#67](https://github.com/water-rs/hydrolysis-glass/pull/67))
- move the waterui lock to dev's tip
- move the waterui lock to dev's tip
- Merge remote-tracking branch 'origin/dev' into controls/9-slider
- lock waterui dev with the glass background contract
- move the waterui pins to dev's tip
- Merge remote-tracking branch 'origin/dev' into controls/9-slider
- measure the drag smoothing's growth against its gain
- compare the container recipe's member effects within a tolerance
- pack the glass uniforms through one layout the container recipe inverts
- register the §5.6 container recipe for the glass shader
- move to waterui dev's container recipe contract
- Merge remote-tracking branch 'origin/dev' into controls/9-slider
- Merge remote-tracking branch 'origin/dev' into perf/46-gaussian-blur
- *(material)* read every glass blur from the engine's Gaussian pyramid
- follow the framework's svg source on its dev branch
- raise the toolchain floor to the framework's 1.99
- *(theme)* [**breaking**] describe the glass controls as Hydrolysis looks
- Merge remote-tracking branch 'origin/dev' into controls/9-slider
- Merge remote-tracking branch 'origin/dev' into controls/10-segmented
- Merge remote-tracking branch 'origin/dev' into fix/29-edge-spread
- Merge remote-tracking branch 'origin/dev' into controls/7-button
- *(material)* take no rim sample for a zero rim mix
- Merge remote-tracking branch 'origin/dev' into fix/4-clear-contrast
- measure a mixed-size container's capture scale and recipe
- stop carrying patches outside the glass dependency graph
- *(engine)* verify the glass material through Hydrolysis's own mount
- *(material)* render glass through the in-tree engine
