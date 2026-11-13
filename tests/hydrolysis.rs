//! The theme rendered through the engine, driven from the app side.
//!
//! Every scene here is a real `WaterUI` view tree mounted on Hydrolysis's
//! headless offscreen runtime under this crate's [`Glass`] style: the
//! style installs its tokens, the backend registers the theme's backdrop
//! shader and capture classes, the widgets record their chrome as
//! `backdrop_material` members, and the engine mounts them as backdrop
//! groups against the captured content behind them. Nothing in this file
//! reaches past Hydrolysis's public runtime API — what it verifies is the
//! path an application takes.
//!
//! Three kinds of check live here:
//!
//! - **Engine against standalone.** The chrome a mounted glass button
//!   paints is compared pixel by pixel with the same surface rendered by
//!   the standalone [`GlassRenderer`](hydrolysis_glass::GlassRenderer)
//!   over the same backdrop. The two evaluate the same recipe against
//!   the same capture, so they agree within the noise the capture
//!   quantization, field evaluation and colour-space round trips make.
//! - **Footprints the contract leaves in the pixels.** That
//!   `.material_group()` fuses neighbouring chrome into one union field,
//!   and that a member's outer extent carries its cast shadow past its
//!   silhouette.
//! - **Scene exports** for review against §7's observations, written to
//!   `target/verification/<scene>.png`. GPU output is not pixel-stable
//!   across adapters, so these assert only that the frame rendered and
//!   the file was written.
//!
//! The grid backdrops stay with the standalone §7 exports in
//! `tests/gpu.rs`: a grid is 161 positioned views, and what it is there
//! to show — refraction through the lensing band — is a property of the
//! recipe both renderers share, which the comparison test here pins
//! directly.

#[allow(
    dead_code,
    reason = "the harness is shared across test binaries; each uses a different subset"
)]
mod common;

use common::*;
use hydrolysis::theme::WidgetInteractionState;
use hydrolysis_glass::material::geometry::{CONTAINER_UNION_SMOOTHING, Shape};
use hydrolysis_glass::material::recipe::{Family, Translucency};
use hydrolysis_glass::{Appearance, Element, Glass, Group, Scene};
use nami::{CustomBinding, Signal};
use std::time::Duration;
use waterui::animation::{Animation, NoAnimation};
use waterui::background::Glass as WaterGlass;
use waterui::color::Srgb;
use waterui::form::picker::picker;
use waterui::layout::stack::hstack;
use waterui::layout::{PositionExt as _, absolute};
use waterui::navigation::{NavigationView, Tab, Tabs};
use waterui::shape::{Circle, FixedRoundedRectangle};
use waterui::text::text;
use waterui::{AnyView, Binding, Color, View, ViewExt as _};
use waterui_controls::slider::{Slider, slider};
use waterui_controls::{ButtonStyle, ControlSize, Label, button::button, toggle::toggle};

/// The control size every button in this file takes: the size
/// `draw_button_chrome` resolves the surface at, so the standalone
/// reference asks the theme for the same metrics the frame was laid out
/// against.
const SIZE: ControlSize = ControlSize::Medium;

/// A glass-styled button with its top-left corner at `(x, y)`, at the
/// size its own metrics give it.
///
/// The label is empty, so the button is the metrics' insets alone: this
/// scene is about the chrome, and a glyph run over it would be compared
/// against a standalone surface that has none. Nothing frames the button
/// — a `Frame` would centre the intrinsic size inside it, which puts the
/// chrome somewhere the caller did not name.
fn glass_button(x: f32, y: f32) -> impl View {
    button("")
        .style(ButtonStyle::Glass)
        .size(SIZE)
        .position_anchor(waterui::layout::UnitPoint::TOP_LEADING, x, y)
}

/// The frame [`glass_button`] lays out at `(x, y)`: the metrics' insets on
/// both sides of an empty label (§8.1: the frame is the label plus the
/// insets).
#[allow(
    clippy::cast_possible_truncation,
    reason = "the metrics are small positive point sizes"
)]
fn button_rect(_glass: &Glass, x: f32, y: f32) -> [f32; 4] {
    let m = hydrolysis_glass::theme::button::SizeGroup::of(SIZE).metrics();
    [x, y, 2.0 * m.inset_x, 2.0 * m.inset_y]
}

/// The §7 backdrop as a view, for the backdrops a view tree can express
/// without hundreds of layers.
///
/// # Panics
/// For [`Backdrop::Grid`] and [`Backdrop::GridOverSplit`] — see the
/// module header.
fn backdrop_view(backdrop: Backdrop) -> AnyView {
    let solid = |v: f32| Color::new(Srgb::new(v, v, v));
    match backdrop {
        Backdrop::Black => AnyView::new(solid(0.0)),
        Backdrop::Grey => AnyView::new(solid(0.5)),
        Backdrop::White => AnyView::new(solid(1.0)),
        Backdrop::Split => AnyView::new(
            hstack((
                Color::new(Srgb::new(SPLIT_RED[0], SPLIT_RED[1], SPLIT_RED[2])),
                Color::new(Srgb::new(SPLIT_BLUE[0], SPLIT_BLUE[1], SPLIT_BLUE[2])),
            ))
            .spacing(0.0),
        ),
        Backdrop::Edge => AnyView::new(hstack((solid(0.0), solid(1.0))).spacing(0.0)),
        Backdrop::Grid | Backdrop::GridOverSplit => {
            panic!("a grid backdrop is reviewed on the standalone exports, not through a view tree")
        }
    }
}

/// The scene-space pt → image pixel read.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "scene coordinates are positive and far below u32::MAX"
)]
fn probe_px(img: &Image, x_pt: f32, y_pt: f32) -> [f32; 4] {
    let x = (x_pt * PX_PER_PT).round() as u32;
    let y = (y_pt * PX_PER_PT).round() as u32;
    img.pixels[(y * img.width + x) as usize]
}

/// The theme under `appearance`.
const fn theme(appearance: Appearance) -> Glass {
    match appearance {
        Appearance::Dark => Glass::dark(),
        Appearance::Light => Glass::light(),
    }
}

/// The surface the theme places for a glass button laid out at `rect`.
fn button_element(appearance: Appearance, rect: [f32; 4]) -> Element {
    theme(appearance)
        .button_surface(
            kurbo_rect(rect),
            ButtonStyle::Glass,
            SIZE,
            WidgetInteractionState::NONE,
        )
        .expect("a glass button carries a surface")
        .element
}

/// `[x, y, w, h]` pt as the engine's rect.
fn kurbo_rect([x, y, w, h]: [f32; 4]) -> cherenkov_record::kurbo::Rect {
    cherenkov_record::kurbo::Rect::new(
        f64::from(x),
        f64::from(y),
        f64::from(x + w),
        f64::from(y + h),
    )
}

/// Asserts the engine's frame agrees with the standalone render of the
/// same material over the same backdrop.
///
/// The two evaluate one recipe against one capture, so the residual is
/// the capture quantization, the field evaluation and the colour-space
/// round trips — bounded in the mean over the whole frame, and per
/// channel at the worst pixel. The message names the worst pixel: a
/// geometry mismatch lands there, on a silhouette edge, rather than in
/// the mean.
///
/// Coverage carries none of that noise: both paths composite over an
/// opaque backdrop, so both frames are opaque everywhere and their alpha
/// agrees to the engine readback's 8-bit step. A member that leaves the
/// frame translucent — its shadow or exterior scaling the backdrop's
/// coverage rather than darkening it — fails here, however few pixels
/// it touches.
fn assert_agrees(label: &str, engine: &Image, reference: &Image) {
    if let Err(message) = agreement(label, engine, reference) {
        panic!("{message}");
    }
}

/// [`assert_agrees`]'s comparison, with the verdict returned: `Err` names
/// the worst pixel when `engine` is outside the tolerance of `reference`.
#[allow(
    clippy::cast_possible_truncation,
    reason = "a pixel index inside a frame far below u32::MAX"
)]
fn agreement(label: &str, engine: &Image, reference: &Image) -> Result<(), String> {
    /// The engine frame's alpha resolution: it is read back as RGBA8.
    const ALPHA_STEP: f32 = 1.0 / 255.0;
    assert_eq!(
        (engine.width, engine.height),
        (reference.width, reference.height),
        "{label}: both paths render the scene at {PX_PER_PT} px/pt",
    );
    let mut worst = 0.0f32;
    let mut worst_at = 0usize;
    let mut worst_alpha = 0.0f32;
    let mut worst_alpha_at = 0usize;
    let mut sum = 0.0f64;
    for (i, (p, q)) in engine
        .pixels
        .iter()
        .zip(reference.pixels.iter())
        .enumerate()
    {
        for c in 0..4 {
            let d = (p[c] - q[c]).abs();
            if d > worst {
                worst = d;
                worst_at = i;
            }
            sum += f64::from(d);
        }
        let d = (p[3] - q[3]).abs();
        if d > worst_alpha {
            worst_alpha = d;
            worst_alpha_at = i;
        }
    }
    if worst_alpha > ALPHA_STEP {
        return Err(format!(
            "{label}: engine vs standalone coverage differs by {worst_alpha} at px ({}, {}) \
             = engine {:?} vs standalone {:?}",
            worst_alpha_at as u32 % engine.width,
            worst_alpha_at as u32 / engine.width,
            engine.pixels[worst_alpha_at],
            reference.pixels[worst_alpha_at],
        ));
    }
    #[expect(
        clippy::cast_precision_loss,
        reason = "a channel count well inside f64's exact integer range"
    )]
    let mean = sum / (engine.pixels.len() * 4) as f64;
    let (wx, wy) = (
        worst_at as u32 % engine.width,
        worst_at as u32 / engine.width,
    );
    if mean < 0.02 && worst < 0.4 {
        Ok(())
    } else {
        Err(format!(
            "{label}: engine vs standalone mean |Δ| {mean}, worst {worst} at px ({wx}, {wy}) \
             = engine {:?} vs standalone {:?}",
            engine.pixels[worst_at], reference.pixels[worst_at],
        ))
    }
}

/// The two renderers evaluate the same material against the same
/// capture: a glass button mounted on the backend agrees with the
/// standalone render of the surface the theme places for its frame.
#[test]
fn a_mounted_glass_button_matches_the_standalone_surface() {
    const AT: [f32; 2] = [101.0, 400.0];
    let mut standalone = Harness::new();
    for (appearance, name) in [(Appearance::Dark, "dark"), (Appearance::Light, "light")] {
        let mut app = GlassApp::mount(
            move || absolute((backdrop_view(Backdrop::White), glass_button(AT[0], AT[1]))),
            theme(appearance),
        );
        let engine = app.export(&format!("button_{name}_hydrolysis"));
        let rect = button_rect(&theme(appearance), AT[0], AT[1]);
        let scene = Scene::new(
            SCENE_PT,
            PX_PER_PT,
            vec![Group::single(button_element(appearance, rect))],
        );
        let reference = standalone.render(Backdrop::White, &scene);
        reference.save(&format!("button_{name}_standalone"));
        assert_agrees(name, &engine, &reference);
    }
}

/// A large glass button across the black | white edge agrees with the
/// standalone render of its surface: the edge is where the capture's blur
/// and the refraction lobe show, and the black half is where anything the
/// member writes outside its silhouette shows.
#[test]
fn a_mounted_glass_button_matches_the_standalone_surface_across_the_edge() {
    /// The label block, pt: with a large button's insets the frame is
    /// 200 × 70, centred on the scene's edge at x = 201.
    const LABEL: [f32; 2] = [160.0, 40.0];
    const AT: [f32; 2] = [101.0, 400.0];
    let rect = [AT[0], AT[1], LABEL[0] + 40.0, LABEL[1] + 30.0];
    let mut standalone = Harness::new();
    for (appearance, name) in [(Appearance::Dark, "dark"), (Appearance::Light, "light")] {
        let mut app = GlassApp::mount(
            move || {
                absolute((
                    backdrop_view(Backdrop::Edge),
                    blank_button(ControlSize::Large, LABEL, AT[0], AT[1]),
                ))
            },
            theme(appearance),
        );
        let engine = app.export(&format!("button_edge_{name}_hydrolysis"));
        let scene = Scene::new(
            SCENE_PT,
            PX_PER_PT,
            vec![Group::single(sized_button_element(
                appearance,
                ControlSize::Large,
                rect,
            ))],
        );
        let reference = standalone.render(Backdrop::Edge, &scene);
        reference.save(&format!("button_edge_{name}_standalone"));
        assert_agrees(name, &engine, &reference);
    }
}

/// `.material_group()` fuses what it holds: two glass buttons under one
/// group share a capture and a union field, so the frame agrees with the
/// standalone render of the same two elements as one union at the
/// container smoothing. The same pair outside a group are two solo
/// members, each its own field — the gap between them reads differently,
/// which is what the grouping is for.
#[test]
fn a_material_group_fuses_neighbouring_chrome() {
    const AT: [f32; 2] = [101.0, 400.0];
    /// The gap between the two buttons, pt. The container union blends
    /// over `k = 30 pt`, which bridges a gap of up to `k/2` = 15 pt, so a
    /// 2 pt gap puts the midpoint inside the fused field and outside
    /// both silhouettes.
    const GAP: f32 = 2.0;
    let glass = Glass::dark();
    let first = button_rect(&glass, AT[0], AT[1]);
    let second = button_rect(&glass, AT[0] + first[2] + GAP, AT[1]);
    let pair = move || {
        // The pair as its own absolute canvas: nested in the scene's, it
        // stretches over the same window, so the buttons keep their
        // scene coordinates while the group wraps exactly the two.
        absolute((
            glass_button(first[0], first[1]),
            glass_button(second[0], second[1]),
        ))
    };
    let mut grouped = GlassApp::mount(
        move || absolute((backdrop_view(Backdrop::White), pair().material_group())),
        Glass::dark(),
    );
    let grouped = grouped.export("material_group_dark_hydrolysis");
    let mut solo = GlassApp::mount(
        move || absolute((backdrop_view(Backdrop::White), pair())),
        Glass::dark(),
    );
    let solo = solo.export("material_solo_dark_hydrolysis");

    let fused = Scene::new(
        SCENE_PT,
        PX_PER_PT,
        vec![Group::container(
            vec![
                button_element(Appearance::Dark, first),
                button_element(Appearance::Dark, second),
            ],
            CONTAINER_UNION_SMOOTHING,
        )],
    );
    let reference = Harness::new().render(Backdrop::White, &fused);
    reference.save("material_group_dark_standalone");
    assert_agrees("material group", &grouped, &reference);

    // The gap's midpoint: inside the fused field, outside both
    // silhouettes. Two solo members leave it to their exteriors, so the
    // two frames part there.
    let x = first[0] + first[2] + GAP / 2.0;
    let y = first[1] + first[3] / 2.0;
    let (fused_px, solo_px) = (probe_px(&grouped, x, y), probe_px(&solo, x, y));
    let apart = (0..3)
        .map(|c| (fused_px[c] - solo_px[c]).abs())
        .fold(0.0f32, f32::max);
    assert!(
        apart > 0.05,
        "the group's union field covers the {GAP} pt gap the solo members leave \
         (fused {fused_px:?} against solo {solo_px:?})",
    );
}

/// A glass button of `size` whose label is a transparent `label` pt
/// block, so the frame is the label plus the size's insets and nothing
/// else draws over the chrome, with its top-left corner at `(x, y)`.
fn blank_button(size: ControlSize, label: [f32; 2], x: f32, y: f32) -> impl View {
    button(Label::new("", move || {
        Color::new(Srgb::new(0.0, 0.0, 0.0))
            .opacity(0.0)
            .width(label[0])
            .height(label[1])
    }))
    .style(ButtonStyle::Glass)
    .size(size)
    .position_anchor(waterui::layout::UnitPoint::TOP_LEADING, x, y)
}

/// The surface the theme places for a glass button of `size` laid out at
/// `rect`.
fn sized_button_element(appearance: Appearance, size: ControlSize, rect: [f32; 4]) -> Element {
    theme(appearance)
        .button_surface(
            kurbo_rect(rect),
            ButtonStyle::Glass,
            size,
            WidgetInteractionState::NONE,
        )
        .expect("a glass button carries a surface")
        .element
}

/// A container whose members differ in size renders with one recipe
/// (§5.6): two glass buttons of different sizes under one
/// `.material_group()` take the larger's capture and recipe values and
/// the smaller's primary lobe, so the frame agrees with the standalone
/// renderer's container of the same two elements.
#[test]
fn a_material_group_of_mixed_sizes_renders_with_the_container_recipe() {
    /// The label block both buttons carry, pt: a regular button's frame
    /// is this plus `24 × 14`, a large one's plus `40 × 30`.
    const LABEL: [f32; 2] = [60.0, 20.33];
    const GAP: f32 = 16.0;
    const AT: [f32; 2] = [101.0, 400.0];
    let regular = [AT[0], AT[1], LABEL[0] + 24.0, LABEL[1] + 14.0];
    let large = [
        regular[0] + regular[2] + GAP,
        AT[1],
        LABEL[0] + 40.0,
        LABEL[1] + 30.0,
    ];
    let small_element = sized_button_element(Appearance::Dark, ControlSize::Medium, regular);
    let large_element = sized_button_element(Appearance::Dark, ControlSize::Large, large);
    // The premise: the two standalone recipes differ in the terms the
    // container takes from the smallest member.
    assert!(
        large_element.recipe.primary_shift < small_element.recipe.primary_shift,
        "the larger button's standalone primary shift ({}) is deeper than the smaller's ({})",
        large_element.recipe.primary_shift,
        small_element.recipe.primary_shift,
    );

    let mut grouped = GlassApp::mount(
        move || {
            absolute((
                backdrop_view(Backdrop::Split),
                absolute((
                    blank_button(ControlSize::Medium, LABEL, regular[0], regular[1]),
                    blank_button(ControlSize::Large, LABEL, large[0], large[1]),
                ))
                .material_group(),
            ))
        },
        Glass::dark(),
    );
    let grouped = grouped.export("material_group_mixed_dark_hydrolysis");

    let scene = Scene::new(
        SCENE_PT,
        PX_PER_PT,
        vec![Group::container(
            vec![small_element, large_element],
            CONTAINER_UNION_SMOOTHING,
        )],
    );
    let reference = Harness::new().render(Backdrop::Split, &scene);
    reference.save("material_group_mixed_dark_standalone");
    assert_agrees("mixed-size material group", &grouped, &reference);
}

/// `glass` behind empty content of `frame`'s size, placed at its top-left
/// corner `(x, y)`: the glass is the only thing drawn, on the content's
/// frame. The background goes on the sized content, before the placement,
/// which would otherwise be the frame the glass fills.
fn glass_behind_blank(glass: WaterGlass, [x, y, w, h]: [f32; 4]) -> impl View {
    Color::new(Srgb::new(0.0, 0.0, 0.0))
        .opacity(0.0)
        .width(w)
        .height(h)
        .background(glass)
        .position_anchor(waterui::layout::UnitPoint::TOP_LEADING, x, y)
}

/// A `.background(Glass::regular())` behind content agrees with the
/// standalone render of the regular capsule on the content's frame.
#[test]
fn a_glass_background_matches_the_standalone_element() {
    const FRAME: [f32; 4] = [101.0, 400.0, 200.0, 80.0];
    let mut app = GlassApp::mount(
        || {
            absolute((
                backdrop_view(Backdrop::Edge),
                glass_behind_blank(WaterGlass::regular(), FRAME),
            ))
        },
        Glass::dark(),
    );
    let engine = app.export("glass_background_dark_hydrolysis");
    let scene = Scene::new(
        SCENE_PT,
        PX_PER_PT,
        vec![Group::single(element(
            capsule(FRAME[0], FRAME[1], FRAME[2], FRAME[3]),
            Family::Regular,
            Appearance::Dark,
            Translucency::default(),
        ))],
    );
    let reference = Harness::new().render(Backdrop::Edge, &scene);
    reference.save("glass_background_dark_standalone");
    assert_agrees("glass background", &engine, &reference);
}

/// §5.6's verification scene mounted through Hydrolysis: a 60 pt circle and
/// the 300 × 200.33 pt card (radius 40), 40 pt below it, each a
/// `.background(Glass…)` over empty content in one `.material_group()`,
/// agrees with the standalone renderer's container of the same two
/// elements.
#[test]
fn a_glass_background_container_matches_the_standalone_scene() {
    const CARD: [f32; 4] = [51.0, 120.0, 300.0, 200.33];
    const CIRCLE: [f32; 4] = [171.0, 360.33, 60.0, 60.0];
    let mut app = GlassApp::mount(
        || {
            absolute((
                backdrop_view(Backdrop::Edge),
                absolute((
                    glass_behind_blank(
                        WaterGlass::regular().shape(FixedRoundedRectangle::new(40.0)),
                        CARD,
                    ),
                    glass_behind_blank(WaterGlass::regular().shape(Circle), CIRCLE),
                ))
                .material_group(),
            ))
        },
        Glass::dark(),
    );
    let engine = app.export("glass_background_container_dark_hydrolysis");
    let regular = |shape| {
        element(
            shape,
            Family::Regular,
            Appearance::Dark,
            Translucency::default(),
        )
    };
    let scene = Scene::new(
        SCENE_PT,
        PX_PER_PT,
        vec![Group::container(
            vec![
                regular(rounded(CARD[0], CARD[1], CARD[2], CARD[3], 40.0)),
                regular(Shape::circle(CIRCLE[0] + 30.0, CIRCLE[1] + 30.0, 30.0)),
            ],
            CONTAINER_UNION_SMOOTHING,
        )],
    );
    let reference = Harness::new().render(Backdrop::Edge, &scene);
    reference.save("glass_background_container_dark_standalone");
    assert_agrees("glass background container", &engine, &reference);
}

/// A member's outer extent carries what it paints outside its
/// silhouette: the band below a mounted button darkens with its cast
/// shadow and fades with distance. Those pixels lie outside the member's
/// clip, so they only reach the frame when the engine honours the extent
/// the effect declares.
#[test]
fn the_outer_extent_carries_what_paints_outside_the_silhouette() {
    const AT: [f32; 2] = [181.0, 400.0];
    /// Where the band is probed below the silhouette, pt.
    const PROBES: [f32; 3] = [1.0, 4.0, 8.0];
    let glass = Glass::dark();
    let rect = button_rect(&glass, AT[0], AT[1]);
    let recipe = button_element(Appearance::Dark, rect).recipe;
    // The test's premise: the extent is the cast reach rather than the
    // recipe's exterior span — the shadow is what grew it.
    let span = recipe.exterior_span;
    let reach = 1.3f32.mul_add(recipe.cast_falloff, recipe.cast_shift[1]);
    assert!(
        reach > span,
        "the cast reach ({reach}) runs past the exterior span ({span}), \
         so the extent follows the shadow",
    );

    let mut app = GlassApp::mount(
        move || absolute((backdrop_view(Backdrop::White), glass_button(AT[0], AT[1]))),
        glass,
    );
    let img = app.export("cast_shadow_dark_hydrolysis");
    let x = rect[2].mul_add(0.5, rect[0]);
    let bottom = rect[1] + rect[3];
    let band: Vec<f32> = PROBES
        .iter()
        .map(|dy| {
            let px = probe_px(&img, x, bottom + dy);
            px[0] + px[1] + px[2]
        })
        .collect();
    for (dy, lum) in PROBES.iter().zip(&band) {
        assert!(
            *lum < 2.995,
            "the shadow darkens the white backdrop {dy} pt past the silhouette (lum {lum})",
        );
    }
    assert!(
        band.windows(2).all(|w| w[1] >= w[0]) && band[band.len() - 1] > band[0],
        "the shadow fades with distance ({band:?} at {PROBES:?} pt)",
    );
}

/// The app chrome over every backdrop a view tree can express: a tab
/// container whose page carries a navigation bar, glass buttons on the
/// page and a fused pair, in both appearances. Reviewed against §7 beside
/// the standalone exports.
#[test]
fn app_chrome_over_the_backdrops() {
    for backdrop in [
        Backdrop::Black,
        Backdrop::Grey,
        Backdrop::White,
        Backdrop::Split,
    ] {
        for (appearance, name) in [(Appearance::Dark, "dark"), (Appearance::Light, "light")] {
            let mut app = GlassApp::mount(move || app_chrome(backdrop), theme(appearance));
            app.export(&format!("{}_{name}_chrome_hydrolysis", backdrop.name()));
        }
    }
}

/// The reviewed scene: the backdrop, a page of glass controls — three
/// standalone and a fused pair — inside a navigation view, inside a tab
/// container, so the frame carries the theme's bars as well as its
/// buttons.
fn app_chrome(backdrop: Backdrop) -> impl View {
    let selection = Binding::container(0usize);
    let page = move || {
        NavigationView::new(
            "Glass",
            absolute((
                backdrop_view(backdrop),
                glass_button(45.0, 160.0),
                glass_button(121.0, 160.0),
                glass_button(197.0, 160.0),
                absolute((
                    glass_button(45.0, 280.0),
                    glass_button(91.0, 280.0),
                    glass_button(137.0, 280.0),
                ))
                .material_group(),
            )),
        )
    };
    Tabs::new(
        &selection,
        vec![
            Tab::new("Glass", page),
            Tab::new("Second", move || {
                NavigationView::new("Second", backdrop_view(backdrop))
            }),
        ],
    )
}

/// A glass or prominent text button of `size` with its top-left corner at
/// `(x, y)`.
fn text_button(
    label: &'static str,
    style: ButtonStyle,
    size: ControlSize,
    x: f32,
    y: f32,
) -> impl View {
    button(label).style(style).size(size).position_anchor(
        waterui::layout::UnitPoint::TOP_LEADING,
        x,
        y,
    )
}

/// §7 scene 5, the part a view tree can express through the theme's
/// hooks: a "Glass" text button per size group and a prominent button,
/// over black and white in both appearances, labels drawn by the backend.
/// The disabled, tinted, shaped, icon and two-line variants need inputs
/// the `WidgetTheme` hooks do not carry and are exported by `tests/gpu.rs`.
#[test]
fn button_scene_with_labels() {
    for backdrop in [Backdrop::Black, Backdrop::White] {
        for (appearance, name) in [(Appearance::Dark, "dark"), (Appearance::Light, "light")] {
            let mut app = GlassApp::mount(
                move || {
                    absolute((
                        backdrop_view(backdrop),
                        text_button("Glass", ButtonStyle::Glass, ControlSize::Small, 40.0, 100.0),
                        text_button(
                            "Glass",
                            ButtonStyle::Glass,
                            ControlSize::Medium,
                            40.0,
                            160.0,
                        ),
                        text_button("Glass", ButtonStyle::Glass, ControlSize::Large, 40.0, 220.0),
                        text_button(
                            "Continue",
                            ButtonStyle::GlassProminent,
                            ControlSize::Medium,
                            40.0,
                            300.0,
                        ),
                    ))
                },
                theme(appearance),
            );
            app.export(&format!("button_scene_{}_{name}_labels", backdrop.name()));
        }
    }
}

/// §7 scene 6: the regular "Glass" button alone over black in dark
/// appearance, touched at its centre at t = 0 and released at t = 1 s,
/// captured at 0, 0.147, 0.5, 1.215, 1.43 and 2 s.
#[test]
fn press_sequence() {
    const AT: [f32; 2] = [168.0, 400.0];
    let mut app = GlassApp::mount(
        move || {
            absolute((
                backdrop_view(Backdrop::Black),
                text_button(
                    "Glass",
                    ButtonStyle::Glass,
                    ControlSize::Medium,
                    AT[0],
                    AT[1],
                ),
            ))
        },
        theme(Appearance::Dark),
    );
    app.settle();
    // The frame the idle button lays out at: 66 x 34.33 pt.
    let centre = (AT[0] + 33.0, AT[1] + 17.165);
    let mut clock = Duration::ZERO;
    app.touch_down(centre.0, centre.1);
    capture_at(&mut app, &mut clock, 17, "press_t0000");
    capture_at(&mut app, &mut clock, 147, "press_t0147");
    capture_at(&mut app, &mut clock, 500, "press_t0500");
    advance_to(&mut app, &mut clock, 1000);
    app.touch_up(centre.0, centre.1);
    capture_at(&mut app, &mut clock, 1215, "press_t1215");
    capture_at(&mut app, &mut clock, 1430, "press_t1430");
    capture_at(&mut app, &mut clock, 2000, "press_t2000");
}

/// The harness's virtual frame, 60 Hz: the app's pump moves time only in
/// whole frames of it.
const FRAME: Duration = Duration::from_micros(16_667);

/// How many frames take a sequence whose frames so far reach `clock` to the
/// frame nearest `t_ms`.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "a sequence is a few seconds of frames at most, never negative"
)]
fn frames_until(clock: Duration, t_ms: u64) -> u32 {
    let ahead = Duration::from_millis(t_ms).saturating_sub(clock);
    (ahead.as_secs_f64() / FRAME.as_secs_f64()).round() as u32
}

/// Pumps the mounted app to the frame nearest `t_ms` since the sequence
/// began, without reading it back. `clock` is the virtual time the frames
/// pumped so far have reached.
fn advance_to(app: &mut GlassApp, clock: &mut Duration, t_ms: u64) {
    let frames = frames_until(*clock, t_ms);
    app.advance(FRAME * frames);
    *clock += FRAME * frames;
}

/// Pumps the mounted app to the frame nearest `t_ms` since the sequence
/// began and exports that frame; the capturing pump itself takes the last
/// frame of the interval. `clock` is as [`advance_to`]'s.
fn capture_at(app: &mut GlassApp, clock: &mut Duration, t_ms: u64, name: &str) -> Image {
    let frames = frames_until(*clock, t_ms).max(1);
    app.advance(FRAME * (frames - 1));
    let image = app.capture_now();
    *clock += FRAME * frames;
    let path = image.save(name);
    assert!(path.is_file(), "{name}: PNG written");
    image
}

/// §7, scene 8: the switch scene, the part a view tree can express through
/// the theme's hooks. Over black, grey 0.5 and white in both appearances:
/// an off and an on switch, a disabled off and a disabled on switch, and a
/// "Label" switch 300 pt wide, and a red-tinted on, a red-tinted off and a
/// blue-tinted on switch.
#[test]
fn switch_scene() {
    for backdrop in [Backdrop::Black, Backdrop::Grey, Backdrop::White] {
        for (appearance, name) in [(Appearance::Dark, "dark"), (Appearance::Light, "light")] {
            let tinted = |on: bool, tint: [f32; 4], x: f32| {
                toggle("", &Binding::bool(on))
                    .switch()
                    .hide_label()
                    .tint(srgb(tint))
                    .position_anchor(waterui::layout::UnitPoint::TOP_LEADING, x, 400.0)
            };
            let mut app = GlassApp::mount(
                move || {
                    absolute((
                        backdrop_view(backdrop),
                        tinted(
                            true,
                            hydrolysis_glass::theme::slider::red_tint(appearance),
                            60.0,
                        ),
                        tinted(
                            false,
                            hydrolysis_glass::theme::slider::red_tint(appearance),
                            140.0,
                        ),
                        tinted(true, blue_tint(appearance), 220.0),
                        toggle("", &Binding::bool(false))
                            .switch()
                            .hide_label()
                            .position_anchor(waterui::layout::UnitPoint::TOP_LEADING, 60.0, 100.0),
                        toggle("", &Binding::bool(true))
                            .switch()
                            .hide_label()
                            .position_anchor(waterui::layout::UnitPoint::TOP_LEADING, 200.0, 100.0),
                        toggle("", &Binding::bool(false))
                            .switch()
                            .hide_label()
                            .disabled(true)
                            .position_anchor(waterui::layout::UnitPoint::TOP_LEADING, 60.0, 200.0),
                        toggle("", &Binding::bool(true))
                            .switch()
                            .hide_label()
                            .disabled(true)
                            .position_anchor(waterui::layout::UnitPoint::TOP_LEADING, 200.0, 200.0),
                        toggle("Label", &Binding::bool(true))
                            .switch()
                            .width(300.0)
                            .position_anchor(waterui::layout::UnitPoint::TOP_LEADING, 51.0, 300.0),
                    ))
                },
                theme(appearance),
            );
            app.export(&format!("switch_{}_{name}", backdrop.name()));
        }
    }
}

/// §7, scene 9: the toggle sequence. An off switch alone over black in
/// dark appearance, switched on at t = 0 and off at t = 1 s, captured at
/// 0, 0.05, 0.1, 0.15, 0.3, 1.1 and 1.5 s.
#[test]
fn toggle_sequence() {
    const AT: [f32; 2] = [170.0, 400.0];
    let value = Binding::bool(false);
    let mut app = GlassApp::mount(
        {
            let value = value.clone();
            move || {
                absolute((
                    backdrop_view(Backdrop::Black),
                    toggle("", &value).switch().hide_label().position_anchor(
                        waterui::layout::UnitPoint::TOP_LEADING,
                        AT[0],
                        AT[1],
                    ),
                ))
            }
        },
        theme(Appearance::Dark),
    );
    app.settle();
    let mut clock = Duration::ZERO;
    value.set(true);
    capture_at(&mut app, &mut clock, 17, "toggle_t0000");
    capture_at(&mut app, &mut clock, 50, "toggle_t0050");
    capture_at(&mut app, &mut clock, 100, "toggle_t0100");
    capture_at(&mut app, &mut clock, 150, "toggle_t0150");
    capture_at(&mut app, &mut clock, 300, "toggle_t0300");
    advance_to(&mut app, &mut clock, 1000);
    value.set(false);
    capture_at(&mut app, &mut clock, 1100, "toggle_t1100");
    capture_at(&mut app, &mut clock, 1500, "toggle_t1500");
}

/// `rgba` as an sRGB colour, for a `.tint(...)`.
fn srgb(rgba: [f32; 4]) -> Color {
    Color::new(Srgb::new(rgba[0], rgba[1], rgba[2]))
}

/// The blue the §8.2 scene tints a switch with: (0, 145, 255) in dark
/// appearance and (0, 136, 255) in light.
const fn blue_tint(appearance: Appearance) -> [f32; 4] {
    match appearance {
        Appearance::Dark => [0.0, 145.0 / 255.0, 1.0, 1.0],
        Appearance::Light => [0.0, 136.0 / 255.0, 1.0, 1.0],
    }
}

/// The window the switch tests mount in, pt: small, so the switch is most of
/// the frame and a displaced knob or a different fill is outside
/// [`agreement`]'s tolerance.
const SWITCH_WINDOW: [f32; 2] = [101.0, 44.0];
/// The switch's frame is 61 pt wide, centred on the window's vertical centre
/// line, so that reflecting the window reflects the frame onto itself.
const SWITCH_AT: [f32; 2] = [20.0, 8.0];

/// A switch bound to `value`, `direction`, in `tint` when it has one, alone
/// over `backdrop` in a [`SWITCH_WINDOW`].
fn switch_app(
    appearance: Appearance,
    backdrop: impl Fn() -> AnyView + Clone + 'static,
    value: &Binding<bool>,
    direction: waterui::layout::LayoutDirection,
    tint: Option<Color>,
) -> GlassApp {
    let value = value.clone();
    GlassApp::sized(
        move || {
            let switch = toggle("", &value).switch().hide_label().with(direction);
            let switch = match tint.clone() {
                Some(tint) => AnyView::new(switch.tint(tint)),
                None => AnyView::new(switch),
            };
            absolute((
                backdrop(),
                switch.position_anchor(
                    waterui::layout::UnitPoint::TOP_LEADING,
                    SWITCH_AT[0],
                    SWITCH_AT[1],
                ),
            ))
        },
        theme(appearance),
        SWITCH_WINDOW,
    )
}

/// A switch at rest showing `on`, over black.
fn switch_at_rest(appearance: Appearance, on: bool, tint: Option<Color>) -> Image {
    switch_app(
        appearance,
        || backdrop_view(Backdrop::Black),
        &Binding::bool(on),
        waterui::layout::LayoutDirection::LeftToRight,
        tint,
    )
    .snapshot()
}

/// §8.2, Tint: the tint replaces the on fill only. A switch tinted with the
/// default green is the untinted switch; an off switch ignores its tint; a
/// red and a blue tint make an on switch different from the default and from
/// each other. Every tinted state is exported for review.
#[test]
fn a_tint_replaces_the_on_fill_only() {
    for (appearance, name) in [(Appearance::Dark, "dark"), (Appearance::Light, "light")] {
        let green = srgb(hydrolysis_glass::theme::switch::default_tint(appearance));
        let red = srgb(hydrolysis_glass::theme::slider::red_tint(appearance));
        let blue = srgb(blue_tint(appearance));

        let untinted_on = switch_at_rest(appearance, true, None);
        let untinted_off = switch_at_rest(appearance, false, None);
        assert_agrees(
            &format!("{name}: the default tint as an explicit tint"),
            &switch_at_rest(appearance, true, Some(green)),
            &untinted_on,
        );
        let red_on = switch_at_rest(appearance, true, Some(red.clone()));
        let red_off = switch_at_rest(appearance, false, Some(red));
        let blue_on = switch_at_rest(appearance, true, Some(blue));
        assert_agrees(
            &format!("{name}: a tinted off switch is the untinted one"),
            &red_off,
            &untinted_off,
        );
        assert!(
            agreement("red is not green", &red_on, &untinted_on).is_err(),
            "{name}: a red tint leaves the on track green"
        );
        assert!(
            agreement("blue is not red", &blue_on, &red_on).is_err(),
            "{name}: a blue tint draws the red track"
        );
        red_on.save(&format!("switch_tint_{name}_red_on"));
        red_off.save(&format!("switch_tint_{name}_red_off"));
        blue_on.save(&format!("switch_tint_{name}_blue_on"));
    }
}

/// A binding whose changes carry `change` as their metadata, as the changes
/// of a value an app sets under an animation (or with animation disabled)
/// do.
#[derive(Clone)]
struct Changing<T: Clone + 'static, M: Clone + 'static> {
    binding: Binding<T>,
    change: M,
}

impl<T: Clone + 'static, M: Clone + 'static> Signal for Changing<T, M> {
    type Output = T;
    type Guard = <Binding<T> as Signal>::Guard;

    fn snapshot(&self) -> T {
        self.binding.snapshot()
    }

    fn watch(&self, watcher: impl Fn(nami::watcher::Context<T>) + 'static) -> Self::Guard {
        let change = self.change.clone();
        self.binding
            .watch(move |context| watcher(context.with(change.clone())))
    }
}

impl<T: Clone + 'static, M: Clone + 'static> CustomBinding for Changing<T, M> {
    fn set(&self, value: T) {
        self.binding.set(value);
    }
}

/// An off switch over black in dark appearance, bound through `bind`, set to
/// on at t = 0 and captured at each of `times` (ms), the frames exported as
/// `<name>_t<ms>`.
fn switched_on(
    name: &str,
    bind: impl FnOnce(Binding<bool>) -> Binding<bool>,
    times: &[u64],
) -> Vec<Image> {
    let value = Binding::bool(false);
    let mut app = switch_app(
        Appearance::Dark,
        || backdrop_view(Backdrop::Black),
        &bind(value.clone()),
        waterui::layout::LayoutDirection::LeftToRight,
        None,
    );
    app.settle();
    value.set(true);
    let mut clock = Duration::ZERO;
    times
        .iter()
        .map(|&t| capture_at(&mut app, &mut clock, t, &format!("{name}_t{t:04}")))
        .collect()
}

/// §8.2, Toggle motion: the switch's motion is its own. A change made under
/// a linear 1.5 s animation moves the knob and the track exactly like one
/// made with the default animation, and one that disables animation lands on
/// the new end in the next frame. The default change passes through frames
/// that are neither the off nor the on switch.
#[test]
fn the_switch_moves_by_its_own_spring_whatever_animation_a_change_names() {
    const TIMES: [u64; 5] = [17, 50, 100, 150, 300];
    let default = switched_on("switch_default_motion", |value| value, &TIMES);
    let explicit = switched_on(
        "switch_explicit_motion",
        |value| {
            Binding::custom(Changing {
                binding: value,
                change: Animation::linear(Duration::from_millis(1500)),
            })
        },
        &TIMES,
    );
    for ((t, default), explicit) in TIMES.iter().zip(&default).zip(&explicit) {
        assert_agrees(
            &format!("a 1.5 s linear change at {t} ms"),
            explicit,
            default,
        );
    }

    let off = switch_at_rest(Appearance::Dark, false, None);
    let on = switch_at_rest(Appearance::Dark, true, None);
    // (The first frame, one frame after the change, is the switch within a
    // point of rest: inside the tolerance.)
    for (t, frame) in TIMES.iter().zip(&default).skip(1).take(3) {
        assert!(
            agreement("not off", frame, &off).is_err(),
            "the frame at {t} ms shows the switch still off"
        );
        assert!(
            agreement("not on", frame, &on).is_err(),
            "the frame at {t} ms shows the switch already on"
        );
    }

    // No animation: the next frame is the settled switch.
    let snapped = switched_on(
        "switch_snapped",
        |value| {
            Binding::custom(Changing {
                binding: value,
                change: NoAnimation,
            })
        },
        &[17],
    );
    assert_agrees("a change without animation", &snapped[0], &on);
}

/// §8.2, Toggle motion, frame by frame: the knob and the track move from
/// frame to frame through the sequence, no two of its first frames are
/// alike, and the switch settles on the on switch.
#[test]
fn a_toggle_passes_through_distinct_mid_transition_frames() {
    const TIMES: [u64; 5] = [50, 100, 150, 200, 300];
    let frames = switched_on("switch_mid", |value| value, &TIMES);
    // 200 and 300 ms are within 1.5 pt of the end and of each other; the
    // three before them are not.
    for (i, a) in frames.iter().enumerate().take(3) {
        for (j, b) in frames.iter().enumerate().skip(i + 1) {
            assert!(
                agreement("distinct", a, b).is_err(),
                "the frames at {} and {} ms are alike",
                TIMES[i],
                TIMES[j]
            );
        }
    }
    let mut settled = switch_app(
        Appearance::Dark,
        || backdrop_view(Backdrop::Black),
        &Binding::bool(true),
        waterui::layout::LayoutDirection::LeftToRight,
        None,
    );
    assert_agrees(
        "the settled switch",
        &settled.snapshot(),
        &switch_at_rest(Appearance::Dark, true, None),
    );
}

/// The blue | red split of §7 as a view, reflected: red | blue.
fn reflected_split() -> AnyView {
    let red = Color::new(Srgb::new(SPLIT_RED[0], SPLIT_RED[1], SPLIT_RED[2]));
    let blue = Color::new(Srgb::new(SPLIT_BLUE[0], SPLIT_BLUE[1], SPLIT_BLUE[2]));
    AnyView::new(hstack((blue, red)).spacing(0.0))
}

/// §8.2, Direction: a right-to-left switch is the left-to-right switch
/// reflected about the window's centre line, off or on, at rest or held (a
/// glass lens over the reflected backdrop), and a drag toward the trailing
/// end switches it on whichever way that is.
#[test]
fn a_right_to_left_switch_is_the_mirror_of_a_left_to_right_one() {
    use waterui::layout::LayoutDirection::{LeftToRight, RightToLeft};

    let width = SWITCH_WINDOW[0];
    let y = SWITCH_AT[1] + 14.0;
    // The knob's centre: 2 + 18.5 pt in from the track's end, 22 pt further
    // on when on.
    let knob_x = |on: bool| SWITCH_AT[0] + 20.5 + if on { 22.0 } else { 0.0 };

    for (on, held, name) in [
        (false, false, "off"),
        (true, false, "on"),
        (false, true, "off_held"),
        (true, true, "on_held"),
    ] {
        let render = |direction, backdrop: fn() -> AnyView, touch_x: f32| {
            let mut app = switch_app(
                Appearance::Dark,
                backdrop,
                &Binding::bool(on),
                direction,
                None,
            );
            app.settle();
            if held {
                app.touch_down(touch_x, y);
                app.advance(Duration::from_millis(600));
                app.capture_now()
            } else {
                app.snapshot()
            }
        };
        let ltr = render(LeftToRight, reflected_split, knob_x(on));
        let rtl = render(
            RightToLeft,
            || backdrop_view(Backdrop::Split),
            width - knob_x(on),
        );
        flipped(&ltr).save(&format!("switch_{name}_ltr_flipped"));
        rtl.save(&format!("switch_{name}_rtl"));
        assert_agrees(
            &format!("right-to-left switch, {name}"),
            &rtl,
            &flipped(&ltr),
        );
    }

    // A drag of 14 pt, past the 10 pt slop and past the half-way point of
    // the 22 pt travel, toward the trailing end turns the switch on; toward
    // the leading end it leaves it off.
    for (direction, toward_on) in [(LeftToRight, 14.0), (RightToLeft, -14.0)] {
        for (sign, expect_on) in [(1.0, true), (-1.0, false)] {
            let value = Binding::bool(false);
            let mut app = switch_app(
                Appearance::Dark,
                || backdrop_view(Backdrop::Black),
                &value,
                direction,
                None,
            );
            app.settle();
            let from = if direction == LeftToRight {
                knob_x(false)
            } else {
                width - knob_x(false)
            };
            app.touch_down(from, y);
            app.touch_move(f32::mul_add(sign, toward_on, from), y);
            app.advance(Duration::from_millis(50));
            app.touch_up(f32::mul_add(sign, toward_on, from), y);
            app.settle();
            assert_eq!(
                value.snapshot(),
                expect_on,
                "{direction:?}: a drag of {} pt",
                sign * toward_on
            );
        }
    }
}

/// A 300 pt segmented picker of `labels` at `size`, laid out `direction`,
/// with its top-left corner at `(51, y)`, its selection bound to
/// `selection`.
///
/// The picker's label is hidden: the scenes are about the control, and a
/// heading would move its row.
fn segmented_picker(
    labels: &'static [&'static str],
    selection: &Binding<i32>,
    size: ControlSize,
    direction: waterui::layout::LayoutDirection,
    disabled: bool,
    y: f32,
) -> impl View + use<> {
    let items: Vec<_> = labels
        .iter()
        .zip(0i32..)
        .map(|(l, i)| text(*l).tag(i))
        .collect();
    picker("Choice", items, selection)
        .hide_label()
        .segmented()
        .size(size)
        .width(300.0)
        .disabled(disabled)
        .with(direction)
        .position_anchor(waterui::layout::UnitPoint::TOP_LEADING, 51.0, y)
}

/// A regular, left-to-right [`segmented_picker`].
fn segmented(
    labels: &'static [&'static str],
    selection: &Binding<i32>,
    disabled: bool,
    y: f32,
) -> impl View + use<> {
    segmented_picker(
        labels,
        selection,
        ControlSize::Medium,
        waterui::layout::LayoutDirection::LeftToRight,
        disabled,
        y,
    )
}

/// The labels of the single-control segmented tests.
const THREE: &[&str] = &["One", "Two", "Three"];
/// The window the single-control segmented tests mount in, pt.
const SEGMENTED_WINDOW: [f32; 2] = [402.0, 100.0];
/// The top of the row of a single control, pt.
const ROW_Y: f32 = 30.0;
/// The vertical centre of a regular control's 31 pt row, pt.
const ROW_MID: f32 = ROW_Y + 15.5;

/// The centre of segment `index` of a 300 pt, three-segment control at
/// x = 51, counted from the left: its segments are 100 pt wide.
fn segment_x(index: u8) -> f32 {
    f32::from(index).mul_add(100.0, 101.0)
}

/// A [`THREE`]-segment control alone over `backdrop` in a
/// [`SEGMENTED_WINDOW`], its selection bound to `selection`.
fn segmented_app(
    appearance: Appearance,
    backdrop: fn() -> AnyView,
    selection: &Binding<i32>,
    size: ControlSize,
    direction: waterui::layout::LayoutDirection,
    disabled: bool,
) -> GlassApp {
    let selection = selection.clone();
    GlassApp::sized(
        move || {
            absolute((
                backdrop(),
                segmented_picker(THREE, &selection, size, direction, disabled, ROW_Y),
            ))
        },
        theme(appearance),
        SEGMENTED_WINDOW,
    )
}

/// A regular, left-to-right, enabled [`segmented_app`] over black.
fn plain_segmented_app(appearance: Appearance, selection: &Binding<i32>) -> GlassApp {
    segmented_app(
        appearance,
        || backdrop_view(Backdrop::Black),
        selection,
        ControlSize::Medium,
        waterui::layout::LayoutDirection::LeftToRight,
        false,
    )
}

/// A [`plain_segmented_app`] at rest with `selected` selected.
fn segmented_at_rest(appearance: Appearance, selected: i32) -> Image {
    plain_segmented_app(appearance, &Binding::container(selected)).snapshot()
}

/// §7, scene 12 (§8.4): the segmented controls over black, grey 0.5 and
/// white in both appearances, the large size and a disabled control
/// included. The red tint has no effect (§8.4, States) and `PickerItem` is
/// text-only, so symbols are stood in for by text glyphs.
#[test]
fn segmented_scene() {
    const TWO: &[&str] = &["One", "Two"];
    const FOUR: &[&str] = &["A", "Bb", "Ccc", "Dddd"];
    const LONG: &[&str] = &["Short", "A much longer label"];
    const GLYPHS: &[&str] = &["\u{2605}", "\u{2665}", "\u{25cf}"];
    for backdrop in [Backdrop::Black, Backdrop::Grey, Backdrop::White] {
        for (appearance, name) in [(Appearance::Dark, "dark"), (Appearance::Light, "light")] {
            let mut app = GlassApp::mount(
                move || {
                    let first = Binding::container(0i32);
                    let second = Binding::container(1i32);
                    let third = Binding::container(2i32);
                    absolute((
                        backdrop_view(backdrop),
                        segmented(THREE, &first, false, 60.0),
                        segmented(TWO, &second, false, 140.0),
                        segmented(FOUR, &third, false, 220.0),
                        segmented(LONG, &first, false, 300.0),
                        segmented(GLYPHS, &first, false, 380.0),
                        segmented(THREE, &first, true, 460.0),
                        segmented_picker(
                            THREE,
                            &second,
                            ControlSize::Large,
                            waterui::layout::LayoutDirection::LeftToRight,
                            false,
                            540.0,
                        ),
                    ))
                },
                theme(appearance),
            );
            app.export(&format!("segmented_{}_{name}", backdrop.name()));
        }
    }
}

/// A [`plain_segmented_app`] bound through `bind`, its selection moved from
/// the first to the third segment at t = 0 and captured at each of `times`
/// (ms), the frames exported as `<name>_t<ms>`.
fn selection_moved(
    appearance: Appearance,
    name: &str,
    bind: impl FnOnce(Binding<i32>) -> Binding<i32>,
    times: &[u64],
) -> Vec<Image> {
    let selection = Binding::container(0i32);
    let mut app = plain_segmented_app(appearance, &bind(selection.clone()));
    app.settle();
    selection.set(2);
    let mut clock = Duration::ZERO;
    times
        .iter()
        .map(|&t| capture_at(&mut app, &mut clock, t, &format!("{name}_t{t:04}")))
        .collect()
}

/// §7, scene 13 (§8.4): a 300 pt "One | Two | Three" control alone over
/// black in dark appearance, its selection moved from the first to the
/// third segment at t = 0, captured one frame after the change and at
/// 0.05, 0.1, 0.2, 0.38 and 0.6 s.
#[test]
fn selection_sequence() {
    selection_moved(
        Appearance::Dark,
        "selection",
        |selection| selection,
        &[17, 50, 100, 200, 380, 600],
    );
}

/// §8.4, Selection motion, frame by frame: the thumb leaves the first
/// segment, passes through distinct positions and settles on the third,
/// and every change moves it by the selection spring — one made under a
/// linear 1.5 s animation moves exactly like the default, and one that
/// disables animation still animates. Light appearance, where the opaque
/// thumb makes a displacement of a point plain.
#[test]
fn every_selection_change_moves_the_thumb_by_the_selection_spring() {
    const TIMES: [u64; 4] = [50, 100, 150, 200];
    let start = segmented_at_rest(Appearance::Light, 0);
    let end = segmented_at_rest(Appearance::Light, 2);
    let default = selection_moved(
        Appearance::Light,
        "selection_light",
        |selection| selection,
        &TIMES,
    );
    for (t, frame) in TIMES.iter().zip(&default) {
        assert!(
            agreement("not at the first segment", frame, &start).is_err(),
            "the frame at {t} ms shows the thumb still on the first segment"
        );
        assert!(
            agreement("not at the third segment", frame, &end).is_err(),
            "the frame at {t} ms shows the thumb already on the third segment"
        );
    }
    for (i, a) in default.iter().enumerate() {
        for (j, b) in default.iter().enumerate().skip(i + 1) {
            assert!(
                agreement("distinct", a, b).is_err(),
                "the frames at {} and {} ms are alike",
                TIMES[i],
                TIMES[j]
            );
        }
    }

    let explicit = selection_moved(
        Appearance::Light,
        "selection_light_explicit",
        |selection| {
            Binding::custom(Changing {
                binding: selection,
                change: Animation::linear(Duration::from_millis(1500)),
            })
        },
        &TIMES,
    );
    let snapped = selection_moved(
        Appearance::Light,
        "selection_light_disabled",
        |selection| {
            Binding::custom(Changing {
                binding: selection,
                change: NoAnimation,
            })
        },
        &TIMES,
    );
    for ((t, default), (explicit, snapped)) in TIMES
        .iter()
        .zip(&default)
        .zip(explicit.iter().zip(&snapped))
    {
        assert_agrees(
            &format!("a 1.5 s linear change at {t} ms"),
            explicit,
            default,
        );
        assert_agrees(
            &format!("a change without animation at {t} ms"),
            snapped,
            default,
        );
    }

    let selection = Binding::container(0i32);
    let mut app = plain_segmented_app(Appearance::Light, &selection);
    app.settle();
    selection.set(2);
    assert_agrees("the settled thumb", &app.snapshot(), &end);
}

/// A tap on a segment selects it, and a touch held on the selected
/// segment presses its thumb into the glass lens (§8.4, Touch).
#[test]
fn a_tap_selects_and_a_press_lenses_the_thumb() {
    let selection = Binding::container(0i32);
    let mut app = plain_segmented_app(Appearance::Dark, &selection);
    app.settle();
    app.touch_down(segment_x(1), ROW_MID);
    app.touch_up(segment_x(1), ROW_MID);
    app.settle();
    assert_eq!(selection.snapshot(), 1, "a tap selects the touched segment");
    // The press, on the now selected segment.
    app.touch_down(segment_x(1), ROW_MID);
    let mut clock = Duration::ZERO;
    capture_at(&mut app, &mut clock, 500, "segmented_press_dark");
    app.touch_up(segment_x(1), ROW_MID);
    app.settle();
    assert_eq!(
        selection.snapshot(),
        1,
        "a touch on the selected segment keeps it"
    );
}

/// §8.4, Lens: the selected middle segment pressed and held until the press
/// settles is a clear glass lens 1.15 times the thumb, reaching into both
/// neighbouring segments over the container and the labels, its fill
/// cleared; at rest no part is glass. Both appearances, exported for
/// review.
#[test]
fn the_press_lens_reaches_over_the_neighbouring_segments() {
    for (appearance, name) in [(Appearance::Dark, "dark"), (Appearance::Light, "light")] {
        let rest = segmented_at_rest(appearance, 1);
        rest.save(&format!("segmented_lens_{name}_rest"));
        let mut app = plain_segmented_app(appearance, &Binding::container(1));
        app.settle();
        app.touch_down(segment_x(1), ROW_MID);
        let mut clock = Duration::ZERO;
        let held = capture_at(
            &mut app,
            &mut clock,
            600,
            &format!("segmented_lens_{name}_held"),
        );
        assert!(
            agreement("held is not at rest", &held, &rest).is_err(),
            "{name}: a held thumb is drawn as at rest"
        );
        // Released, the lens collapses and the thumb is flat again.
        app.touch_up(segment_x(1), ROW_MID);
        assert_agrees(&format!("{name}: released"), &app.snapshot(), &rest);
    }
}

/// §8.4, Drag: a press on the selected segment drags the thumb by the
/// touch's offset over the segment width, wherever the touch goes
/// vertically, and the release settles on the nearest segment by the
/// selection spring; a drag past the end holds the thumb on the last
/// segment, and a drag from an unselected segment moves nothing.
#[test]
fn a_drag_moves_the_thumb_and_the_release_settles_on_the_nearest_segment() {
    let selection = Binding::container(0i32);
    let mut app = plain_segmented_app(Appearance::Light, &selection);
    app.settle();

    // 130 pt is 1.3 segments: the thumb follows to 1.3 and settles on 1.
    app.touch_down(segment_x(0), ROW_MID);
    app.touch_move(segment_x(0) + 30.0, ROW_MID);
    app.touch_move(segment_x(0) + 130.0, ROW_MID + 60.0);
    let mut clock = Duration::ZERO;
    let dragged = capture_at(&mut app, &mut clock, 300, "segmented_drag_light");
    assert_eq!(selection.snapshot(), 0, "a drag selects only at release");
    assert!(
        agreement(
            "dragged is not at rest",
            &dragged,
            &segmented_at_rest(Appearance::Light, 0)
        )
        .is_err(),
        "the dragged thumb is drawn on the first segment"
    );
    app.touch_up(segment_x(0) + 130.0, ROW_MID + 60.0);
    let settling = capture_at(&mut app, &mut clock, 350, "segmented_drag_settling_light");
    assert_eq!(
        selection.snapshot(),
        1,
        "the release selects the nearest segment"
    );
    let at_one = segmented_at_rest(Appearance::Light, 1);
    assert!(
        agreement("settling", &settling, &at_one).is_err(),
        "the thumb lands on the second segment in the frame after the release"
    );
    assert_agrees("settled on the second segment", &app.snapshot(), &at_one);

    // Past the end: held on the last segment.
    app.touch_down(segment_x(1), ROW_MID);
    app.touch_move(segment_x(1) + 30.0, ROW_MID);
    app.touch_move(900.0, ROW_MID);
    app.advance(Duration::from_millis(100));
    app.touch_up(900.0, ROW_MID);
    app.settle();
    assert_eq!(
        selection.snapshot(),
        2,
        "a drag past the end settles on the last"
    );

    // From an unselected segment: the release lands outside the touched
    // segment, so nothing is selected and the thumb never moved.
    app.touch_down(segment_x(0), ROW_MID);
    app.touch_move(segment_x(0) + 30.0, ROW_MID);
    app.touch_move(segment_x(1), ROW_MID);
    app.advance(Duration::from_millis(100));
    app.touch_up(segment_x(1), ROW_MID);
    app.settle();
    assert_eq!(selection.snapshot(), 2, "a drag from an unselected segment");
}

/// §8.4, Direction: a right-to-left control is the left-to-right one
/// reflected about the window's centre line, its first segment on the
/// right, at rest and with the thumb held as a lens over the reflected
/// backdrop; a tap on the leftmost segment selects the last item. The
/// labels are empty here, since glyphs do not mirror; the labelled
/// right-to-left control is exported for review.
#[test]
fn a_right_to_left_control_is_the_mirror_of_a_left_to_right_one() {
    use waterui::layout::LayoutDirection::{LeftToRight, RightToLeft};
    const BLANK: &[&str] = &["", "", ""];

    let width = SEGMENTED_WINDOW[0];
    for held in [false, true] {
        let render = |direction, backdrop: fn() -> AnyView, touch_x: f32| {
            let selection = Binding::container(0i32);
            let mut app = GlassApp::sized(
                move || {
                    absolute((
                        backdrop(),
                        segmented_picker(
                            BLANK,
                            &selection,
                            ControlSize::Medium,
                            direction,
                            false,
                            ROW_Y,
                        ),
                    ))
                },
                theme(Appearance::Dark),
                SEGMENTED_WINDOW,
            );
            app.settle();
            if held {
                app.touch_down(touch_x, ROW_MID);
                app.advance(Duration::from_millis(600));
                app.capture_now()
            } else {
                app.snapshot()
            }
        };
        let ltr = render(LeftToRight, reflected_split, segment_x(0));
        let rtl = render(
            RightToLeft,
            || backdrop_view(Backdrop::Split),
            width - segment_x(0),
        );
        let name = if held { "held" } else { "rest" };
        flipped(&ltr).save(&format!("segmented_{name}_ltr_flipped"));
        rtl.save(&format!("segmented_{name}_rtl"));
        assert_agrees(
            &format!("right-to-left control, {name}"),
            &rtl,
            &flipped(&ltr),
        );
    }

    let selection = Binding::container(0i32);
    let mut app = segmented_app(
        Appearance::Dark,
        || backdrop_view(Backdrop::Black),
        &selection,
        ControlSize::Medium,
        RightToLeft,
        false,
    );
    app.export("segmented_rtl_labels_dark");
    app.touch_down(segment_x(0), ROW_MID);
    app.touch_up(segment_x(0), ROW_MID);
    app.settle();
    assert_eq!(
        selection.snapshot(),
        2,
        "the leftmost segment is the last item"
    );
}

/// The opaque frame `image` composited at `opacity` over the uniform grey
/// `backdrop` in encoded space, the space §8.4 composites the control in.
fn composited(image: &Image, backdrop: f32, opacity: f32) -> Image {
    let pixels = image
        .pixels
        .iter()
        .map(|p| {
            let mix = |c: f32| c.mul_add(opacity, backdrop * (1.0 - opacity));
            [mix(p[0]), mix(p[1]), mix(p[2]), 1.0]
        })
        .collect();
    Image {
        width: image.width,
        height: image.height,
        pixels,
    }
}

/// §8.4, States: a disabled control is the enabled one composited over the
/// backdrop at opacity 0.5 in encoded space, labels included, so its
/// labels are 128 over black in dark appearance and 127 over white in
/// light, where §8.4 measured them; a tap does not select.
#[test]
fn a_disabled_control_is_the_enabled_one_at_half_opacity() {
    let black: fn() -> AnyView = || backdrop_view(Backdrop::Black);
    let white: fn() -> AnyView = || backdrop_view(Backdrop::White);
    for (appearance, name, backdrop, grey) in [
        (Appearance::Dark, "dark", black, 0.0),
        (Appearance::Light, "light", white, 1.0),
    ] {
        let selection = Binding::container(0i32);
        let mut disabled = segmented_app(
            appearance,
            backdrop,
            &selection,
            ControlSize::Medium,
            waterui::layout::LayoutDirection::LeftToRight,
            true,
        );
        let image = disabled.export(&format!("segmented_disabled_{name}"));
        disabled.touch_down(segment_x(2), ROW_MID);
        disabled.touch_up(segment_x(2), ROW_MID);
        disabled.settle();
        assert_eq!(
            selection.snapshot(),
            0,
            "{name}: a disabled control took a tap"
        );
        let enabled = segmented_app(
            appearance,
            backdrop,
            &Binding::container(0i32),
            ControlSize::Medium,
            waterui::layout::LayoutDirection::LeftToRight,
            false,
        )
        .snapshot();
        let faded = composited(
            &enabled,
            grey,
            hydrolysis_glass::theme::segmented::DISABLED_OPACITY,
        );
        faded.save(&format!("segmented_disabled_{name}_expected"));
        assert_agrees(&format!("{name}: disabled"), &image, &faded);
        assert!(
            agreement("disabled is not enabled", &image, &enabled).is_err(),
            "{name}: a disabled control is drawn as an enabled one"
        );
    }
}

/// §8.4, Geometry: the large and extra large sizes are the 48 pt group, a
/// 49 pt container and a 45 pt thumb, with the same 13 pt labels; the
/// regular group is 31 pt. Exported for review over black and white.
#[test]
fn the_large_size_is_the_48_point_group() {
    for (appearance, name) in [(Appearance::Dark, "dark"), (Appearance::Light, "light")] {
        let large = |size| {
            segmented_app(
                appearance,
                || backdrop_view(Backdrop::Black),
                &Binding::container(1i32),
                size,
                waterui::layout::LayoutDirection::LeftToRight,
                false,
            )
            .export(&format!("segmented_{size:?}_{name}"))
        };
        let regular = segmented_at_rest(appearance, 1);
        let lg = large(ControlSize::Large);
        assert_agrees(
            &format!("{name}: extra large is large"),
            &large(ControlSize::ExtraLarge),
            &lg,
        );
        assert!(
            agreement("large is not regular", &lg, &regular).is_err(),
            "{name}: the large control is drawn at the regular size"
        );
    }
}

/// `control` `width` pt wide and as tall as a slider (§8.3) with its
/// top-left corner at `(x, y)`.
fn placed(control: Slider, width: f32, x: f32, y: f32) -> impl View {
    // `Slider::size` is the control size; the frame's is the view's.
    waterui::ViewExt::size(control.hide_label(), width, 31.0).position_anchor(
        waterui::layout::UnitPoint::TOP_LEADING,
        x,
        y,
    )
}

/// A slider 300 pt wide bound to `value`.
fn slider_at(value: &Binding<f64>, y: f32) -> impl View + use<> {
    placed(slider("Volume", value), 300.0, 51.0, y)
}

/// §7 scene 10, the slider scene: 300 pt sliders at values 0, 0.3, 0.5 and
/// 1, a disabled and a red-tinted one at 0.5 and one at 0.5 with a label at
/// each end, over black, grey 0.5 and white in dark and light appearance.
/// The end labels are text standing in for the speaker symbols.
#[test]
fn slider_scene() {
    for backdrop in [Backdrop::Black, Backdrop::Grey, Backdrop::White] {
        for (appearance, name) in [(Appearance::Dark, "dark"), (Appearance::Light, "light")] {
            let red = hydrolysis_glass::theme::slider::red_tint(appearance);
            let mut app = GlassApp::mount(
                move || {
                    let at = |v: f64| Binding::container(v);
                    absolute((
                        backdrop_view(backdrop),
                        slider_at(&at(0.0), 120.0),
                        slider_at(&at(0.3), 200.0),
                        slider_at(&at(0.5), 280.0),
                        slider_at(&at(1.0), 360.0),
                        placed(slider("Volume", &at(0.5)), 300.0, 51.0, 440.0).disabled(true),
                        placed(
                            slider("Volume", &at(0.5))
                                .min_value_label("Lo")
                                .max_value_label("Hi"),
                            300.0,
                            51.0,
                            520.0,
                        ),
                        placed(slider("Volume", &at(0.5)), 300.0, 51.0, 600.0)
                            .tint(Color::new(Srgb::new(red[0], red[1], red[2]))),
                    ))
                },
                theme(appearance),
            );
            app.export(&format!("slider_{}_{name}", backdrop.name()));
        }
    }
}

/// §7 scene 11, the slider sequence: a 300 pt slider alone over black in
/// dark appearance at value 0.2, set to 0.8 at t = 0 and captured at 0,
/// 0.05, 0.1, 0.2, 0.33 and 0.6 s.
#[test]
fn slider_sequence() {
    let value = Binding::container(0.2f64);
    let shown = value.clone();
    let mut app = GlassApp::mount(
        move || absolute((backdrop_view(Backdrop::Black), slider_at(&shown, 400.0))),
        theme(Appearance::Dark),
    );
    // The slider at rest at `v`: where the knob and the fill belong once the
    // value has arrived there.
    let at_rest = |v: f64| {
        let shown = Binding::container(v);
        GlassApp::mount(
            move || absolute((backdrop_view(Backdrop::Black), slider_at(&shown, 400.0))),
            theme(Appearance::Dark),
        )
        .snapshot()
    };
    let start = at_rest(0.2);
    let end = at_rest(0.8);
    app.settle();
    value.set(0.8);
    let mut clock = Duration::ZERO;
    let frames: Vec<(u64, Image)> = [
        (17, "t0000"),
        (50, "t0050"),
        (100, "t0100"),
        (200, "t0200"),
        (330, "t0330"),
        (600, "t0600"),
    ]
    .into_iter()
    .map(|(t_ms, name)| {
        (
            t_ms,
            capture_at(&mut app, &mut clock, t_ms, &format!("slider_{name}")),
        )
    })
    .collect();
    // The painted knob and fill follow the value (a look whose parts did
    // not repaint holds every frame at the first one): no frame of the
    // sequence is the slider at rest at 0.2, none before the last is it at
    // rest at 0.8 (the spring is still within a pixel or two of its end at
    // 0.6 s), and once the motion settles the slider is at rest at 0.8.
    // (The first frame, one frame after the change, is the slider within a
    // few points of rest at 0.2: inside the tolerance.)
    for (t_ms, frame) in &frames[1..] {
        assert!(
            agreement("not at 0.2", frame, &start).is_err(),
            "the frame at {t_ms} ms shows the slider still at 0.2"
        );
    }
    for (t_ms, frame) in &frames[..frames.len() - 1] {
        assert!(
            agreement("not yet at 0.8", frame, &end).is_err(),
            "the frame at {t_ms} ms shows the slider already at 0.8"
        );
    }
    assert_agrees("slider settled at 0.8", &app.snapshot(), &end);
}

/// The knob of a pressed slider is a glass lens (§8.3, Touch): a 300 pt
/// slider at 0.5 over a split backdrop, touched on its knob and held
/// until the press has settled, exported for review.
#[test]
fn slider_lens_while_pressed() {
    let value = Binding::container(0.5f64);
    let mut app = GlassApp::mount(
        move || absolute((backdrop_view(Backdrop::Split), slider_at(&value, 400.0))),
        theme(Appearance::Dark),
    );
    app.settle();
    // The knob's centre: the frame's middle at 0.5.
    app.touch_down(201.0, 415.5);
    app.advance(Duration::from_millis(600));
    let image = app.capture_now();
    assert!(image.save("slider_lens_pressed").is_file());
}

/// `image` reflected about its vertical centre line.
fn flipped(image: &Image) -> Image {
    let width = image.width as usize;
    let pixels = image
        .pixels
        .chunks_exact(width)
        .flat_map(|row| row.iter().rev().copied())
        .collect();
    Image {
        width: image.width,
        height: image.height,
        pixels,
    }
}

/// A right-to-left slider is the left-to-right slider reflected about the
/// window's centre line: the fill grows from the right, the knob sits at
/// `W − v·(W − 37)` and the held knob is still a glass lens (§8.3). The
/// slider is 300 pt wide at x = 51 in the 402 pt window, so the frame is
/// centred on the reflection axis, and the backdrop is reflected with it, so
/// the left-to-right frame, flipped, is the frame the right-to-left slider
/// must produce. The knob at 0.45 straddles the backdrop's seam, where a
/// missing lens would show as a missing refraction.
#[test]
fn a_right_to_left_slider_is_the_mirror_of_a_left_to_right_one() {
    use hydrolysis_glass::theme::slider::{KNOB_WIDTH, TRAVEL_INSET};
    use waterui::layout::LayoutDirection;

    const VALUE: f64 = 0.45;
    const KNOB_Y: f32 = 415.5;
    #[allow(
        clippy::cast_possible_truncation,
        reason = "a distance of at most 300 pt, far inside f32"
    )]
    let along = VALUE.mul_add(300.0 - KNOB_WIDTH, TRAVEL_INSET) as f32;
    let knob_x = 51.0 + along;
    let window_width = SCENE_PT[0];

    let held = |direction: LayoutDirection, backdrop: fn() -> AnyView, touch_x: f32| {
        let value = Binding::container(VALUE);
        let mut app = GlassApp::mount(
            move || {
                absolute((
                    backdrop(),
                    waterui::ViewExt::size(
                        slider("Volume", &value).hide_label().with(direction),
                        300.0,
                        31.0,
                    )
                    .position_anchor(
                        waterui::layout::UnitPoint::TOP_LEADING,
                        51.0,
                        400.0,
                    ),
                ))
            },
            theme(Appearance::Dark),
        );
        app.settle();
        app.touch_down(touch_x, KNOB_Y);
        app.advance(Duration::from_millis(600));
        app.capture_now()
    };

    // Reflected, the blue | red backdrop reads red | blue, the split's own.
    let reflected_split = || {
        let red = Color::new(Srgb::new(SPLIT_RED[0], SPLIT_RED[1], SPLIT_RED[2]));
        let blue = Color::new(Srgb::new(SPLIT_BLUE[0], SPLIT_BLUE[1], SPLIT_BLUE[2]));
        AnyView::new(hstack((blue, red)).spacing(0.0))
    };
    let ltr = held(LayoutDirection::LeftToRight, reflected_split, knob_x);
    let rtl = held(
        LayoutDirection::RightToLeft,
        || backdrop_view(Backdrop::Split),
        window_width - knob_x,
    );
    flipped(&ltr).save("slider_lens_pressed_ltr_flipped");
    rtl.save("slider_lens_pressed_rtl");
    assert_agrees("right-to-left slider", &rtl, &flipped(&ltr));
}

/// A drag that begins on the knob moves the value by the touch's offset over
/// the knob's travel, `W − 37` pt, wherever the touch goes vertically, and a
/// touch elsewhere on the track does nothing (§8.3, Touch).
#[test]
fn a_drag_from_the_knob_moves_the_value() {
    let value = Binding::container(0.5f64);
    let shown = value.clone();
    let mut app = GlassApp::mount(
        move || absolute((backdrop_view(Backdrop::Split), slider_at(&shown, 400.0))),
        theme(Appearance::Dark),
    );
    app.settle();

    // A touch on the track away from the knob does not move it.
    app.touch_down(80.0, 415.5);
    app.touch_move(120.0, 415.5);
    app.advance(Duration::from_millis(100));
    app.touch_up(120.0, 415.5);
    app.settle();
    assert!(
        (value.snapshot() - 0.5).abs() < 1e-9,
        "a touch off the knob moved the value to {}",
        value.snapshot()
    );

    // The knob's centre is at x = 201; 52.6 pt over the 263 pt travel is
    // 0.2 of the range, and the touch may leave the slider vertically.
    app.touch_down(201.0, 415.5);
    app.touch_move(221.0, 415.5);
    app.touch_move(253.6, 700.0);
    app.advance(Duration::from_millis(100));
    assert!(
        (value.snapshot() - 0.7).abs() < 1e-3,
        "a drag of 52.6 pt moved the value to {}",
        value.snapshot()
    );
    // Clamped to the range wherever the touch goes.
    app.touch_move(900.0, 415.5);
    app.advance(Duration::from_millis(100));
    assert!(
        (value.snapshot() - 1.0).abs() < 1e-9,
        "a drag past the end left the value at {}",
        value.snapshot()
    );
    app.touch_move(120.0, 415.5);
    app.advance(Duration::from_millis(100));
    let at_release = value.snapshot();
    app.touch_up(120.0, 415.5);
    app.settle();
    assert!(
        (value.snapshot() - at_release).abs() < 1e-9,
        "the value stays where the drag left it"
    );
}

/// Each appearance with the backdrop it is reviewed over: the dark theme
/// over black, the light theme over white.
const APPEARANCES: [(Appearance, Backdrop, &str); 2] = [
    (Appearance::Dark, Backdrop::Black, "dark"),
    (Appearance::Light, Backdrop::White, "light"),
];

/// The height of the widget scenes below, pt: room for the widgets and the
/// popups they open, a fraction of the full scene.
const WIDGET_SCENE_HEIGHT: f32 = 360.0;

/// `view` 300 pt wide with its top-left corner at `(51, y)`.
fn row(view: impl View, y: f32) -> impl View {
    view.width(300.0)
        .position_anchor(waterui::layout::UnitPoint::TOP_LEADING, 51.0, y)
}

/// A text field labelled "Name" bound to `value`, prompting "Your name".
fn name_field(value: &str) -> waterui_controls::TextField {
    waterui_controls::field(
        "Name",
        &Binding::container(waterui::Str::from(value.to_owned())),
    )
    .prompt("Your name")
}

/// The text field in both appearances: empty with its prompt, holding a
/// value, and disabled.
#[test]
fn text_field_scene() {
    for (appearance, backdrop, name) in APPEARANCES {
        let mut app = GlassApp::sized(
            move || {
                absolute((
                    backdrop_view(backdrop),
                    row(name_field(""), 40.0),
                    row(name_field("Hello"), 140.0),
                    row(name_field("Hello"), 240.0).disabled(true),
                ))
            },
            theme(appearance),
            [SCENE_PT[0], WIDGET_SCENE_HEIGHT],
        );
        app.export(&format!("text_field_{name}"));
    }
}

/// The text field's context menu, opened by a secondary click on a field
/// holding a value: the menu look's panel and separators. The click
/// focuses the field, whose caret blinks for as long as it holds focus, so
/// the frame is read 400 ms after the click rather than settled.
#[test]
fn text_context_menu_scene() {
    for (appearance, backdrop, name) in APPEARANCES {
        let mut app = GlassApp::sized(
            move || absolute((backdrop_view(backdrop), row(name_field("Hello"), 40.0))),
            theme(appearance),
            [SCENE_PT[0], WIDGET_SCENE_HEIGHT],
        );
        app.settle();
        app.secondary_click(120.0, 70.0);
        app.advance(Duration::from_millis(400));
        let path = app.capture_now().save(&format!("text_context_menu_{name}"));
        assert!(path.is_file(), "{name}: PNG written");
    }
}

/// A picker of three fruits in `style` bound to `selection`.
fn fruit_picker(
    style: waterui::form::picker::PickerStyle,
    selection: &Binding<i32>,
) -> waterui::form::picker::Picker {
    let items: Vec<_> = ["Apple", "Banana", "Cherry"]
        .into_iter()
        .zip(0i32..)
        .map(|(l, i)| text(l).tag(i))
        .collect();
    picker("Fruit", items, selection).style(style)
}

/// The menu picker in both appearances: closed, disabled, and open on its
/// popup, which is the menu look.
#[test]
fn menu_picker_scene() {
    use waterui::form::picker::PickerStyle;
    for (appearance, backdrop, name) in APPEARANCES {
        for open in [false, true] {
            let mut app = GlassApp::sized(
                move || {
                    let selection = Binding::container(1i32);
                    absolute((
                        backdrop_view(backdrop),
                        row(fruit_picker(PickerStyle::Menu, &selection), 40.0),
                        row(fruit_picker(PickerStyle::Menu, &selection), 280.0).disabled(true),
                    ))
                },
                theme(appearance),
                [SCENE_PT[0], WIDGET_SCENE_HEIGHT],
            );
            app.settle();
            if open {
                app.touch_down(200.0, 55.0);
                app.touch_up(200.0, 55.0);
            }
            let state = if open { "open" } else { "closed" };
            app.export(&format!("menu_picker_{state}_{name}"));
        }
    }
}

/// The radio group in both appearances, its second item selected, enabled
/// and disabled.
#[test]
fn radio_group_scene() {
    use waterui::form::picker::PickerStyle;
    for (appearance, backdrop, name) in APPEARANCES {
        let mut app = GlassApp::sized(
            move || {
                let selection = Binding::container(1i32);
                absolute((
                    backdrop_view(backdrop),
                    row(fruit_picker(PickerStyle::Radio, &selection), 40.0),
                    row(fruit_picker(PickerStyle::Radio, &selection), 200.0).disabled(true),
                ))
            },
            theme(appearance),
            [SCENE_PT[0], WIDGET_SCENE_HEIGHT],
        );
        app.export(&format!("radio_group_{name}"));
    }
}

/// A navigation stack with one page pushed over its root, in both
/// appearances: the navigation bar and its back button.
#[test]
fn navigation_bar_scene() {
    use waterui::navigation::{NavigationPath, NavigationStack};
    for (appearance, backdrop, name) in APPEARANCES {
        let path = NavigationPath::<i32>::new();
        path.push(1);
        let mut app = GlassApp::sized(
            move || {
                NavigationStack::with_path(
                    path.clone(),
                    NavigationView::new("Root", backdrop_view(backdrop)),
                )
                .destination(move |_: i32| NavigationView::new("Detail", backdrop_view(backdrop)))
            },
            theme(appearance),
            [SCENE_PT[0], WIDGET_SCENE_HEIGHT],
        );
        app.export(&format!("navigation_bar_{name}"));
    }
}

/// A tab container of two tabs in both appearances, the first selected: the
/// tab bar, its divider, the selected item's highlight and the items.
#[test]
fn tabs_scene() {
    for (appearance, backdrop, name) in APPEARANCES {
        let mut app = GlassApp::sized(
            move || {
                let selection = Binding::container(0usize);
                Tabs::new(
                    &selection,
                    vec![
                        Tab::new("First", move || {
                            NavigationView::new("First", backdrop_view(backdrop))
                        }),
                        Tab::new("Second", move || {
                            NavigationView::new("Second", backdrop_view(backdrop))
                        }),
                    ],
                )
            },
            theme(appearance),
            [SCENE_PT[0], WIDGET_SCENE_HEIGHT],
        );
        app.export(&format!("tabs_{name}"));
    }
}
