//! §7 verification scenes: render the twelve elements over each
//! backdrop and write `target/verification/<scene>.png`.
//!
//! GPU output is not pixel-stable across adapters, so the scenes are not
//! compared pixel by pixel — the PNGs are reviewed against §7's
//! observations. Each test asserts only that rendering succeeded and the
//! file was written.

#[allow(
    dead_code,
    reason = "the harness is shared across test binaries; each uses a different subset"
)]
mod common;

use common::*;
use hydrolysis_glass::material::geometry::CONTAINER_UNION_SMOOTHING;
use hydrolysis_glass::material::recipe::Translucency;
use hydrolysis_glass::theme::button::{BorderShape, Kind, SizeGroup};
use hydrolysis_glass::{Appearance, Family, Group, Scene, Shape};

#[test]
fn adapter_reports() {
    let h = Harness::new();
    assert_ne!(h.gpu().adapter_name, "");
    assert_ne!(h.gpu().backend, "");
    // The image's system WARP misexecutes optimized DXIL and stalls the
    // mounted tests (water-rs/waterui#3093). wgpu reports a NuGet WARP's
    // version as 1.0.x and a system one's as 10.x (wgpu-hal's dx12
    // adapter), so a suite that would run on the system one fails here
    // instead of hanging.
    if h.gpu().backend == "Dx12"
        && h.gpu()
            .adapter_name
            .contains("Microsoft Basic Render Driver")
    {
        assert!(
            h.gpu().driver.starts_with("1.0."),
            "WARP {} is the system one, not the NuGet redistributable: \
             stage d3d10warp.dll beside the test binaries",
            h.gpu().driver,
        );
    }
}

#[test]
fn scenes_at_default_translucency() {
    let mut h = Harness::new();
    for backdrop in Backdrop::ALL {
        for (appearance, name) in [(Appearance::Dark, "dark"), (Appearance::Light, "light")] {
            let scene = verification_scene(appearance, Translucency::default(), [0.0, 76.0, 152.0]);
            let file = format!("{}_{}_s05", backdrop.name(), name);
            h.export(backdrop, &scene, &file);
        }
    }
}

#[test]
fn scenes_at_translucency_extremes() {
    let mut h = Harness::new();
    // §7, scene 2: white in dark and light at translucency 0 and 1.
    for s in [0.0, 1.0] {
        for (appearance, name) in [(Appearance::Dark, "dark"), (Appearance::Light, "light")] {
            let scene = verification_scene(appearance, Translucency::new(s), [0.0, 76.0, 152.0]);
            let file = format!("white_{name}_s{s:02}");
            h.export(Backdrop::White, &scene, &file);
        }
    }
}

#[test]
fn container_circles_touch() {
    let mut h = Harness::new();
    // §7, scene 3: the three circles moved together until they touch (centres
    // 60 pt apart) — the union merges them with no seam.
    for (appearance, name) in [(Appearance::Dark, "dark"), (Appearance::Light, "light")] {
        let scene = verification_scene(appearance, Translucency::default(), [0.0, 60.0, 120.0]);
        h.export(Backdrop::White, &scene, &format!("container_touch_{name}"));
    }
}

#[test]
fn mixed_container() {
    let mut h = Harness::new();
    // §7, scene 4: the large card and a 60 pt circle in one container,
    // beside the same two shapes standing alone. In the container the
    // circle renders the card's recipe, its capture scale among it (§5.6).
    let reg = |shape| {
        element(
            shape,
            Family::Regular,
            Appearance::Dark,
            Translucency::default(),
        )
    };
    let card = |y: f32| rounded(51.0, y, 300.0, 200.33, 40.0);
    let circle = |y: f32| Shape::circle(201.0, y + 30.0, 30.0);
    let scene = Scene::new(
        SCENE_PT,
        PX_PER_PT,
        vec![
            Group::container(
                vec![reg(card(120.0)), reg(circle(360.33))],
                CONTAINER_UNION_SMOOTHING,
            ),
            Group::single(reg(card(480.0))),
            Group::single(reg(circle(720.33))),
        ],
    );
    for backdrop in [
        Backdrop::Black,
        Backdrop::Grey,
        Backdrop::White,
        Backdrop::Split,
        Backdrop::Edge,
    ] {
        h.export(
            backdrop,
            &scene,
            &format!("mixed_container_{}_dark", backdrop.name()),
        );
    }
}

#[test]
fn capsule_over_edge() {
    let mut h = Harness::new();
    // §7, scene 7: a regular 160 × 80 pt capsule standing alone with its
    // centre on the black | white edge — a capture at 0.5 with a nonzero
    // interior blur, beside the mixed container's circle (0.5, none) and
    // card (0.25).
    let scene = Scene::new(
        SCENE_PT,
        PX_PER_PT,
        vec![Group::single(element(
            capsule(121.0, 397.0, 160.0, 80.0),
            Family::Regular,
            Appearance::Dark,
            Translucency::default(),
        ))],
    );
    h.export(Backdrop::Edge, &scene, "capsule_edge_dark");
}

#[test]
fn element_inside_card_shadow() {
    let mut h = Harness::new();
    // A regular element on element 2's frame, moved up until its top 20 pt
    // sit inside the large card's cast shadow: the card renders first, so
    // the element's whole interior — refracted and blurred samples as well
    // as the direct backdrop read — sees the shadow.
    for (appearance, name) in [(Appearance::Dark, "dark"), (Appearance::Light, "light")] {
        let translucency = Translucency::default();
        let mut groups = verification_groups(appearance, translucency, [0.0, 76.0, 152.0]);
        groups[1] = Group::single(element(
            rounded(51.0, 256.0, 300.0, 80.0, 30.0),
            Family::Regular,
            appearance,
            translucency,
        ));
        let scene = Scene::new(SCENE_PT, PX_PER_PT, groups);
        h.export(Backdrop::White, &scene, &format!("card_shadow_{name}"));
    }
}

/// One §7 button-scene entry: the label's layout size (§8.1's observed
/// frames minus the insets), the spec, and the frame's top-left corner or
/// a centred column position.
struct Entry {
    size: SizeGroup,
    border: BorderShape,
    kind: Kind,
    label: [f32; 2],
    tint: Option<[f32; 4]>,
    enabled: bool,
    /// Frame width override: a button given a larger width.
    width: Option<f32>,
}

impl Entry {
    const fn new(size: SizeGroup, kind: Kind, label: [f32; 2]) -> Self {
        Self {
            size,
            border: BorderShape::Capsule,
            kind,
            label,
            tint: None,
            enabled: true,
            width: None,
        }
    }
}

#[test]
#[allow(
    clippy::many_single_char_names,
    clippy::suboptimal_flops,
    clippy::cast_precision_loss,
    reason = "layout arithmetic over a handful of rows; the names are the scene's x, y, w, h"
)]
fn button_scene() {
    use hydrolysis_glass::theme::button::frame_size;
    use hydrolysis_glass::theme::palette::Palette;
    use hydrolysis_glass::theme::surfaces::{ButtonSpec, Press, button};

    const RED: [f32; 4] = [1.0, 56.0 / 255.0, 61.0 / 255.0, 1.0];
    const TEXT: [[f32; 2]; 3] = [[37.67, 18.0], [42.0, 20.33], [42.0, 20.33]];
    const ICON: [[f32; 2]; 3] = [[19.33, 18.33], [22.0, 20.67], [22.0, 20.67]];
    const BOTH: [[f32; 2]; 3] = [[65.0, 18.62], [72.0, 21.0], [72.0, 21.0]];
    let groups = [SizeGroup::Compact, SizeGroup::Regular, SizeGroup::Large];
    let regular = SizeGroup::Regular;
    let mut rows: Vec<Vec<Entry>> = (0..3)
        .map(|i| {
            vec![
                Entry::new(groups[i], Kind::Glass, TEXT[i]),
                Entry::new(groups[i], Kind::Glass, ICON[i]),
                Entry::new(groups[i], Kind::Glass, BOTH[i]),
            ]
        })
        .collect();
    let with = |mut e: Entry, f: &dyn Fn(&mut Entry)| {
        f(&mut e);
        e
    };
    let t = TEXT[1];
    rows.push(vec![
        Entry::new(regular, Kind::Prominent, t),
        with(Entry::new(regular, Kind::Glass, t), &|e| {
            e.enabled = false;
        }),
        with(Entry::new(regular, Kind::Prominent, t), &|e| {
            e.enabled = false;
        }),
    ]);
    rows.push(vec![
        with(Entry::new(regular, Kind::Glass, t), &|e| {
            e.tint = Some(RED);
        }),
        with(Entry::new(regular, Kind::Prominent, t), &|e| {
            e.tint = Some(RED);
        }),
    ]);
    rows.push(vec![
        with(Entry::new(regular, Kind::Glass, ICON[1]), &|e| {
            e.border = BorderShape::RoundedRect(None);
        }),
        with(Entry::new(regular, Kind::Glass, ICON[1]), &|e| {
            e.border = BorderShape::Circle;
        }),
        Entry::new(regular, Kind::Glass, [62.33, 42.33]),
    ]);
    rows.push(vec![with(Entry::new(regular, Kind::Glass, t), &|e| {
        e.width = Some(322.0);
    })]);

    let mut h = Harness::new();
    for backdrop in [Backdrop::Black, Backdrop::White] {
        for (palette, name) in [(Palette::DARK, "dark"), (Palette::LIGHT, "light")] {
            let mut groups_out = Vec::new();
            let mut y = 40.0_f32;
            for row in &rows {
                let frames: Vec<[f32; 2]> = row
                    .iter()
                    .map(|e| {
                        let [w, h] = frame_size(e.size, e.label, e.border);
                        [e.width.unwrap_or(w), h]
                    })
                    .collect();
                let gap = 20.0;
                let total: f32 =
                    frames.iter().map(|f| f[0]).sum::<f32>() + gap * (row.len() as f32 - 1.0);
                let mut x = (SCENE_PT[0] - total) * 0.5;
                let tall = frames.iter().map(|f| f[1]).fold(0.0, f32::max);
                for (e, f) in row.iter().zip(&frames) {
                    let frame = cherenkov_record::kurbo::Rect::new(
                        f64::from(x),
                        f64::from(y + (tall - f[1]) * 0.5),
                        f64::from(x + f[0]),
                        f64::from(y + (tall - f[1]) * 0.5 + f[1]),
                    );
                    let spec = ButtonSpec {
                        kind: e.kind,
                        border: e.border,
                        size: e.size,
                        tint: e.tint,
                        enabled: e.enabled,
                    };
                    let s = button(&palette, frame, &spec, Press::REST, Translucency::default());
                    groups_out.push(Group::single(s.element));
                    x += f[0] + gap;
                }
                y += tall + 40.0;
            }
            let scene = Scene::new(SCENE_PT, PX_PER_PT, groups_out);
            h.export(
                backdrop,
                &scene,
                &format!("button_{}_{name}_s05", backdrop.name()),
            );
        }
    }
}

#[test]
fn sheet_scene() {
    use cherenkov_record::kurbo::Rect as KRect;
    use hydrolysis_glass::Glass;

    let mut h = Harness::new();
    let sheet_bounds = KRect::new(0.0, 260.0, f64::from(SCENE_PT[0]), f64::from(SCENE_PT[1]));
    for backdrop in [Backdrop::Black, Backdrop::White, Backdrop::GridOverSplit] {
        for (glass, name) in [(Glass::dark(), "dark"), (Glass::light(), "light")] {
            let sheet = glass.sheet_surface(sheet_bounds);
            let scene = Scene::new(SCENE_PT, PX_PER_PT, vec![Group::single(sheet.element)]);
            h.export(
                backdrop,
                &scene,
                &format!("sheet_{}_{name}_s05", backdrop.name()),
            );
        }
    }
}

#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "scene coordinates are small non-negative points inside the test canvas"
)]
fn sample_pt(img: &Image, x_pt: f32, y_pt: f32) -> [f32; 4] {
    let x = (x_pt * PX_PER_PT).round() as u32;
    let y = (y_pt * PX_PER_PT).round() as u32;
    img.pixels[(y * img.width + x) as usize]
}

#[test]
fn wide_gamut_display_p3_preserves_chroma() {
    use hydrolysis_glass::material::geometry::Rect;
    use hydrolysis_glass::material::recipe::Tint;

    let mut h = Harness::new();
    let shape = Shape::capsule(Rect::new(120.0, 200.0, 140.0, 44.0));
    let mut elem = element(
        shape,
        Family::Regular,
        Appearance::Dark,
        Translucency::default(),
    );
    elem.recipe = elem.recipe.with_tint(Tint::flat([1.0, 0.0, 0.0], 1.0));
    let scene = Scene::new(SCENE_PT, PX_PER_PT, vec![Group::single(elem)]);
    let img = h.render(Backdrop::Grey, &scene);
    let px = sample_pt(&img, 190.0, 222.0);
    assert!(
        px[0] > 0.85 && px[1] < 0.25 && px[2] < 0.25,
        "pure Display P3 red prominent tint preserves saturated chroma without sRGB clipping: {px:?}"
    );
}

#[test]
fn hdr_specular_rim_and_press_boost_exceed_sdr_peak() {
    use hydrolysis_glass::material::geometry::Rect;
    use hydrolysis_glass::material::recipe::Tint;

    let mut h = Harness::new();
    let bounds = Rect::new(100.0, 200.0, 160.0, 44.0);
    let mut idle_elem = element(
        Shape::capsule(bounds),
        Family::Regular,
        Appearance::Dark,
        Translucency::default(),
    );
    // White prominent fill brings the upper specular rim right up to SDR white
    // at rest so HDR press boost cleanly pushes past 1.0.
    idle_elem.recipe = idle_elem
        .recipe
        .with_tint(Tint::flat([0.92, 0.92, 0.92], 1.0));

    let touch = [180.0, 201.0];
    let mut pressed_elem = idle_elem;
    pressed_elem.recipe = pressed_elem.recipe.with_press(1.0, Some(touch));

    let mut idle_scene = Scene::new(SCENE_PT, PX_PER_PT, vec![Group::single(idle_elem)]);
    idle_scene.display_peak = 2.0;
    let mut pressed_scene = Scene::new(SCENE_PT, PX_PER_PT, vec![Group::single(pressed_elem)]);
    pressed_scene.display_peak = 2.0;

    let idle_img = h.render(Backdrop::White, &idle_scene);
    let pressed_img = h.render(Backdrop::White, &pressed_scene);

    // Probe top rim 1 px inside top edge (y = 200.05 -> px 600, d = -0.17 pt inside 1.0 pt rim)
    // and bottom rim 1 px inside bottom edge (y = 243.8 -> px 731, d = -0.17 pt).
    let idle_near = sample_pt(&idle_img, 180.0, 200.05);
    let pressed_near = sample_pt(&pressed_img, 180.0, 200.05);
    let idle_far = sample_pt(&idle_img, 180.0, 243.8);
    let pressed_far = sample_pt(&pressed_img, 180.0, 243.8);

    assert!(
        pressed_near[0] > 1.0 && pressed_near[1] > 1.0 && pressed_near[2] > 1.0,
        "HDR pressed specular rim must exceed SDR 1.0 (got {pressed_near:?})"
    );
    assert!(
        pressed_near[0] > idle_near[0] + 0.01,
        "press boost must brighten rim near touch: pressed {pressed_near:?} vs idle {idle_near:?}"
    );
    assert!(
        (pressed_near[0] - idle_near[0]) > (pressed_far[0] - idle_far[0]),
        "touch-point localization must boost rim more near touch ({idle_near:?} -> {pressed_near:?}) than far ({idle_far:?} -> {pressed_far:?})",
    );
}

#[test]
fn cross_element_bleed_illuminates_adjacent_within_36pt() {
    use hydrolysis_glass::material::geometry::Rect;
    use hydrolysis_glass::material::recipe::Tint;

    let mut h = Harness::new();
    let neutral = element(
        Shape::capsule(Rect::new(80.0, 200.0, 100.0, 44.0)),
        Family::Regular,
        Appearance::Dark,
        Translucency::default(),
    );
    let blue_emitter = |x: f32| {
        let mut e = element(
            Shape::capsule(Rect::new(x, 200.0, 100.0, 44.0)),
            Family::Regular,
            Appearance::Dark,
            Translucency::default(),
        );
        e.recipe = e.recipe.with_tint(Tint::flat([0.05, 0.35, 1.0], 1.0));
        e
    };

    let isolated_scene = Scene::new(SCENE_PT, PX_PER_PT, vec![Group::single(neutral)]);
    // Sibling spaced 8 pt away (< 36 pt bleed radius).
    let near_scene = Scene::new(
        SCENE_PT,
        PX_PER_PT,
        vec![Group::container(vec![neutral, blue_emitter(188.0)], 8.0)],
    );
    // Sibling spaced 48 pt away (> 36 pt bleed radius).
    let far_scene = Scene::new(
        SCENE_PT,
        PX_PER_PT,
        vec![Group::container(vec![neutral, blue_emitter(228.0)], 8.0)],
    );

    let isolated = h.render(Backdrop::Grey, &isolated_scene);
    let near = h.render(Backdrop::Grey, &near_scene);
    let far = h.render(Backdrop::Grey, &far_scene);

    // Facing inner edge of neutral capsule (right cap at x = 180, probe at x = 178.5, y = 222.0)
    // vs opposite inner edge (left cap at x = 80, probe at x = 81.5, y = 222.0).
    let iso_facing = sample_pt(&isolated, 178.5, 222.0);
    let near_facing = sample_pt(&near, 178.5, 222.0);
    let near_opposite = sample_pt(&near, 81.5, 222.0);
    let iso_opposite = sample_pt(&isolated, 81.5, 222.0);
    let far_facing = sample_pt(&far, 178.5, 222.0);

    assert!(
        near_facing[2] > iso_facing[2] + 0.02
            && (near_facing[2] - near_facing[0]) > (iso_facing[2] - iso_facing[0]) + 0.02,
        "blue prominent sibling within 36 pt must illuminate facing edge with blue bleed: \
         near {near_facing:?} vs isolated {iso_facing:?}"
    );
    assert!(
        (near_facing[2] - iso_facing[2]) > (near_opposite[2] - iso_opposite[2]),
        "bleed must be stronger on facing edge ({near_facing:?} vs {iso_facing:?}) \
         than opposite edge ({near_opposite:?} vs {iso_opposite:?})"
    );
    for c in 0..3 {
        assert!(
            (far_facing[c] - iso_facing[c]).abs() < 1e-3,
            "sibling beyond 36 pt must emit zero bleed: far {far_facing:?} vs isolated {iso_facing:?}"
        );
    }
}
