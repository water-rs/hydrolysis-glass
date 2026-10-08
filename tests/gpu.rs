//! §7 verification scenes: render the twelve elements over each
//! backdrop and write `target/verification/<scene>.png`.
//!
//! GPU output is not pixel-stable across adapters, so the scenes are not
//! compared pixel by pixel — the PNGs are reviewed against §7's
//! observations. Each test asserts only that rendering succeeded and the
//! file was written.

mod common;

use common::*;
use hydrolysis_glass::material::recipe::Translucency;
use hydrolysis_glass::{Appearance, Family, Group, Scene};

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
