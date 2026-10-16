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
use hydrolysis_glass::material::geometry::STANDALONE_UNION_SMOOTHING;
use hydrolysis_glass::{Appearance, Element, Glass, Group, Scene};
use std::time::Duration;
use waterui::color::Srgb;
use waterui::layout::stack::hstack;
use waterui::layout::{PositionExt as _, absolute};
use waterui::navigation::{NavigationView, Tab, Tabs};
use waterui::{AnyView, Binding, Color, View, ViewExt as _};
use waterui_backend_core::widget::{WidgetInteractionState, WidgetTheme as _};
use waterui_controls::{ButtonStyle, ControlSize, button::button};

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
fn button_rect(glass: &Glass, x: f32, y: f32) -> [f32; 4] {
    let m = glass.button_metrics(ButtonStyle::Glass, SIZE);
    [x, y, (2.0 * m.padding_x) as f32, (2.0 * m.padding_y) as f32]
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
fn kurbo_rect([x, y, w, h]: [f32; 4]) -> cherenkov::kurbo::Rect {
    cherenkov::kurbo::Rect::new(
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
#[allow(
    clippy::cast_possible_truncation,
    reason = "a pixel index inside a frame far below u32::MAX"
)]
fn assert_agrees(label: &str, engine: &Image, reference: &Image) {
    assert_eq!(
        (engine.width, engine.height),
        (reference.width, reference.height),
        "{label}: both paths render the scene at {PX_PER_PT} px/pt",
    );
    let mut worst = 0.0f32;
    let mut worst_at = 0usize;
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
    assert!(
        mean < 0.02 && worst < 0.4,
        "{label}: engine vs standalone mean |Δ| {mean}, worst {worst} at px ({wx}, {wy}) \
         = engine {:?} vs standalone {:?}",
        engine.pixels[worst_at],
        reference.pixels[worst_at],
    );
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

/// `.material_group()` fuses what it holds: two glass buttons under one
/// group share a capture and a union field, so the frame agrees with the
/// standalone render of the same two elements as one union at the
/// standalone smoothing. The same pair outside a group are two solo
/// members, each its own field — the gap between them reads differently,
/// which is what the grouping is for.
#[test]
fn a_material_group_fuses_neighbouring_chrome() {
    const AT: [f32; 2] = [101.0, 400.0];
    /// The gap between the two buttons, pt. The standalone union blends
    /// over `k = 0.9 · 8 pt`, which bridges a gap of up to 3.6 pt, so a
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
            STANDALONE_UNION_SMOOTHING,
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
    let selection = Binding::container(0i32);
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
            Tab::new(0i32, "Glass", page),
            Tab::new(1i32, "Second", move || {
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
    capture_at(&mut app, &mut clock, 17, "t0000");
    capture_at(&mut app, &mut clock, 147, "t0147");
    capture_at(&mut app, &mut clock, 500, "t0500");
    app.advance(Duration::from_millis(1000).saturating_sub(clock));
    clock = Duration::from_millis(1000);
    app.touch_up(centre.0, centre.1);
    capture_at(&mut app, &mut clock, 1215, "t1215");
    capture_at(&mut app, &mut clock, 1430, "t1430");
    capture_at(&mut app, &mut clock, 2000, "t2000");
}

/// Advances the mounted app to `t_ms` since the touch and exports that
/// frame; the capturing pump itself takes the last frame of the interval.
fn capture_at(app: &mut GlassApp, clock: &mut Duration, t_ms: u64, name: &str) {
    const FRAME: Duration = Duration::from_micros(16_667);
    let target = Duration::from_millis(t_ms);
    app.advance(target.saturating_sub(*clock + FRAME));
    let image = app.capture_now();
    *clock = target;
    let path = image.save(&format!("press_{name}"));
    assert!(path.is_file(), "press_{name}: PNG written");
}
