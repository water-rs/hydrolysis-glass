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
use hydrolysis_glass::material::geometry::CONTAINER_UNION_SMOOTHING;
use hydrolysis_glass::{Appearance, Element, Glass, Group, Scene};
use nami::Signal as _;
use std::time::Duration;
use waterui::color::Srgb;
use waterui::form::picker::picker;
use waterui::layout::stack::hstack;
use waterui::layout::{PositionExt as _, absolute};
use waterui::navigation::{NavigationView, Tab, Tabs};
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
    app.advance(Duration::from_millis(1000).saturating_sub(clock));
    clock = Duration::from_millis(1000);
    app.touch_up(centre.0, centre.1);
    capture_at(&mut app, &mut clock, 1215, "press_t1215");
    capture_at(&mut app, &mut clock, 1430, "press_t1430");
    capture_at(&mut app, &mut clock, 2000, "press_t2000");
}

/// Advances the mounted app to `t_ms` since the touch and exports that
/// frame; the capturing pump itself takes the last frame of the interval.
fn capture_at(app: &mut GlassApp, clock: &mut Duration, t_ms: u64, name: &str) -> Image {
    const FRAME: Duration = Duration::from_micros(16_667);
    let target = Duration::from_millis(t_ms);
    app.advance(target.saturating_sub(*clock + FRAME));
    let image = app.capture_now();
    *clock = target;
    let path = image.save(name);
    assert!(path.is_file(), "{name}: PNG written");
    image
}

/// §7, scene 8: the switch scene, the part a view tree can express through
/// the theme's hooks. Over black, grey 0.5 and white in both appearances:
/// an off and an on switch, a disabled off and a disabled on switch, and a
/// "Label" switch 300 pt wide. The tinted switches need a tint the
/// `WidgetTheme` toggle hooks do not carry.
#[test]
fn switch_scene() {
    for backdrop in [Backdrop::Black, Backdrop::Grey, Backdrop::White] {
        for (appearance, name) in [(Appearance::Dark, "dark"), (Appearance::Light, "light")] {
            let mut app = GlassApp::mount(
                move || {
                    absolute((
                        backdrop_view(backdrop),
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
    app.advance(Duration::from_millis(1000).saturating_sub(clock));
    clock = Duration::from_millis(1000);
    value.set(false);
    capture_at(&mut app, &mut clock, 1100, "toggle_t1100");
    capture_at(&mut app, &mut clock, 1500, "toggle_t1500");
}

/// A 300 pt segmented picker of `labels` with its top-left corner at
/// `(51, y)`, `selected` segment selected.
///
/// The picker's label is hidden: the scene is about the control, and a
/// heading would move its row.
fn segmented(
    labels: &'static [&'static str],
    selection: &Binding<i32>,
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
        .width(300.0)
        .disabled(disabled)
        .position_anchor(waterui::layout::UnitPoint::TOP_LEADING, 51.0, y)
}

/// §7, scene 12 (§8.4): the segmented controls over black, grey 0.5 and
/// white in both appearances. The large size, the symbol labels, the red
/// tint, the 300 pt width (a picker keeps its intrinsic width inside a
/// frame) and the disabled look (the segmented picker passes no disabled
/// state to its chrome) need inputs the `WidgetTheme` hooks, `PickerItem`
/// and the picker's layout do not carry; symbols are stood in for by text
/// glyphs.
#[test]
fn segmented_scene() {
    const THREE: &[&str] = &["One", "Two", "Three"];
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
                    ))
                },
                theme(appearance),
            );
            app.export(&format!("segmented_{}_{name}", backdrop.name()));
        }
    }
}

/// §7, scene 13 (§8.4): a 300 pt "One | Two | Three" control alone over
/// black in dark appearance, its selection moved from the first to the
/// third segment at t = 0, captured at 0, 0.05, 0.1, 0.2, 0.38 and 0.6 s.
///
/// The `WidgetTheme` contract has no hook for the selection's motion, so
/// the thumb is at the new segment in every frame: the images show the
/// jump the contract allows, not the selection spring of
/// `theme::segmented`.
#[test]
fn selection_sequence() {
    let selection = Binding::container(0i32);
    let shared = selection.clone();
    let mut app = GlassApp::sized(
        move || {
            absolute((
                backdrop_view(Backdrop::Black),
                segmented(&["One", "Two", "Three"], &shared, false, 30.0),
            ))
        },
        theme(Appearance::Dark),
        [SCENE_PT[0], 100.0],
    );
    app.settle();
    selection.set(2);
    let mut clock = Duration::ZERO;
    for (t_ms, name) in [
        (17, "t0000"),
        (50, "t0050"),
        (100, "t0100"),
        (200, "t0200"),
        (380, "t0380"),
        (600, "t0600"),
    ] {
        capture_at(&mut app, &mut clock, t_ms, &format!("selection_{name}"));
    }
}

/// A tap on a segment selects it, and a touch held on the selected
/// segment presses its thumb into the glass lens (§8.4, Touch).
#[test]
fn a_tap_selects_and_a_press_lenses_the_thumb() {
    let selection = Binding::container(0i32);
    let shared = selection.clone();
    let mut app = GlassApp::sized(
        move || {
            absolute((
                backdrop_view(Backdrop::Black),
                segmented(&["One", "Two", "Three"], &shared, false, 30.0),
            ))
        },
        theme(Appearance::Dark),
        [SCENE_PT[0], 100.0],
    );
    app.settle();
    // The second segment: x from 151 to 251, the control's row at y = 45.
    app.touch_down(201.0, 46.0);
    app.touch_up(201.0, 46.0);
    app.settle();
    assert_eq!(selection.snapshot(), 1, "a tap selects the touched segment");
    // The press, on the now selected segment.
    app.touch_down(201.0, 46.0);
    let mut clock = Duration::ZERO;
    capture_at(&mut app, &mut clock, 500, "segmented_press_dark");
    app.touch_up(201.0, 46.0);
    app.settle();
    assert_eq!(
        selection.snapshot(),
        1,
        "a touch on the selected segment keeps it"
    );
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
