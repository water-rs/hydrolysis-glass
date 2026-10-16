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
use hydrolysis_glass::{Appearance, Family, Group, Scene, Shape};

#[test]
fn adapter_reports() {
    let h = Harness::new();
    assert_ne!(h.gpu().adapter_name, "");
    assert_ne!(h.gpu().backend, "");
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
    // §7, scene 5: a regular 160 × 80 pt capsule standing alone with its
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
