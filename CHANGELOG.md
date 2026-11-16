# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0](https://github.com/water-rs/hydrolysis-glass/releases/tag/v0.1.0) - 2026-11-16

### Added

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
