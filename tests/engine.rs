//! §7 verification scenes through the in-tree engine: the theme's
//! `materials` recording realized into `BackdropGroup`s on Cherenkov's GPU
//! backend — `target/verification/<scene>_engine.png` — plus the contract
//! assertions a scene render cannot make: which classes and groupings the
//! registry declares, how scope keys map onto captures, and that the
//! output agrees with the standalone renderer's.
//!
//! GPU output is not pixel-stable across adapters, so the scenes are not
//! compared pixel by pixel — the PNGs are reviewed against §7's
//! observations beside the standalone `tests/gpu.rs` exports. Each test
//! asserts that rendering succeeded and the file was written, and — where
//! the contract leaves a footprint in the pixels — that the footprint is
//! there.

#[allow(
    dead_code,
    reason = "the harness is shared across test binaries; each uses a different subset"
)]
mod common;

use cherenkov::BlendSpace;
use cherenkov_record::{CaptureLevels, MaterialGrouping, MaterialScope};
use common::*;
use hydrolysis_glass::material::geometry::{CONTAINER_UNION_SMOOTHING, Shape};
use hydrolysis_glass::material::recipe::Translucency;
use hydrolysis_glass::theme::materials;
use hydrolysis_glass::{Appearance, Family, Group, Scene};

#[test]
fn host_registers_shader_and_classes() {
    let h = EngineHarness::new();
    assert!(h.host.shader(materials::GLASS_SHADER).is_some());
    let registry = h.host.registry();
    assert!(registry.shader(materials::GLASS_SHADER).is_some());
    let classes: Vec<_> = registry.capture_classes().collect();
    assert_eq!(classes.len(), 8, "4 capture scales × 2 groupings");
    for (_, class) in &classes {
        assert!(
            matches!(class.grouping, MaterialGrouping::Union { .. }),
            "every glass class unions its members",
        );
        assert_eq!(class.levels.get(), CaptureLevels::MAX);
        assert_eq!(class.blend_space, BlendSpace::SrgbEncoded);
    }
}

#[test]
fn scope_keys_shape_groups() {
    let mut h = EngineHarness::new();
    let scene = verification_scene(
        Appearance::Dark,
        Translucency::default(),
        [0.0, 76.0, 152.0],
    );
    // Every §7 group mounts under its own scope. The container's three
    // circles share one scope and class — one group of three; the eleven
    // singles mount one group each.
    let mut mounts = Vec::new();
    for (i, group) in scene.groups.iter().enumerate() {
        mounts.push(h.host.mount(record_group(group, (i + 1) as u64)));
    }
    assert_eq!(h.host.group_count(), 12);
    drop(mounts);

    // Members of one scope and class join one capture.
    let mut h2 = EngineHarness::new();
    let container = &scene.groups[5];
    assert_eq!(container.members.len(), 3);
    let _keep = h2.host.mount(record_group(container, 1));
    assert_eq!(h2.host.group_count(), 1, "three members, one union group");
}

#[test]
fn members_at_different_capture_scales_do_not_share() {
    let mut h = EngineHarness::new();
    // One scope, two members whose recipe scales snap to different lattice
    // steps — a large regular beside a small clear: the contract puts them
    // in different classes, so each gets its own capture even inside one
    // scope.
    let a = element(
        rounded(10.0, 10.0, 300.0, 200.0, 40.0),
        Family::Regular,
        Appearance::Dark,
        Translucency::default(),
    );
    let b = element(
        rounded(10.0, 240.0, 40.0, 40.0, 20.0),
        Family::Clear,
        Appearance::Dark,
        Translucency::default(),
    );
    assert_ne!(
        materials::capture_class(false, a.recipe.capture_scale),
        materials::capture_class(false, b.recipe.capture_scale),
        "the test's premise: distinct classes",
    );
    let layered = cherenkov::Content::record_layered(
        &cherenkov::LayoutSize::new(),
        MaterialScope::new(core::num::NonZeroU64::new(7).expect("scope ids are nonzero")),
        |rec| {
            materials::record(rec, &a, false);
            materials::record(rec, &b, false);
        },
    );
    let _keep = h.host.mount(layered);
    assert_eq!(
        h.host.group_count(),
        2,
        "one scope, two classes → two groups"
    );
}

#[test]
fn solo_member_never_shares_a_scope_group() {
    let mut h = EngineHarness::new();
    // Two recordings of same-class members: one under `MaterialScope::SOLO`,
    // one scoped — each is a group of its own.
    let a = element(
        rounded(10.0, 10.0, 100.0, 60.0, 20.0),
        Family::Regular,
        Appearance::Dark,
        Translucency::default(),
    );
    let b = element(
        rounded(10.0, 100.0, 100.0, 60.0, 20.0),
        Family::Regular,
        Appearance::Dark,
        Translucency::default(),
    );
    let solo = cherenkov::Content::record_layered(
        &cherenkov::LayoutSize::new(),
        MaterialScope::SOLO,
        |rec| materials::record(rec, &a, false),
    );
    let scoped = cherenkov::Content::record_layered(
        &cherenkov::LayoutSize::new(),
        MaterialScope::new(core::num::NonZeroU64::new(9).expect("scope ids are nonzero")),
        |rec| materials::record(rec, &b, false),
    );
    let _a = h.host.mount(solo);
    let _b = h.host.mount(scoped);
    assert_eq!(h.host.group_count(), 2);
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

#[test]
fn outer_extent_carries_the_cast_shadow() {
    let mut h = EngineHarness::new();
    // A single element over white: the cast shadow writes outside the
    // silhouette, which only draws when the member's outer extent reaches
    // it — a pixel in the shadow band beyond the clip must land darker
    // than the backdrop.
    let shape = rounded(60.0, 60.0, 120.0, 80.0, 24.0);
    let el = element(
        shape,
        Family::Regular,
        Appearance::Dark,
        Translucency::default(),
    );
    // The outer extent the effect declares: the cast reach, far past the
    // recipe's exterior span, so the band this test probes exists only
    // because the extent grew for the shadow.
    let span = el.recipe.exterior_span;
    let reach = 1.3f32.mul_add(el.recipe.cast_falloff, el.recipe.cast_shift[1]);
    assert!(
        reach > span + 5.0,
        "the cast reach ({reach}) runs well past the exterior span ({span})",
    );
    let x = shape.rect.w.mul_add(0.5, shape.rect.x);
    let bottom = shape.rect.y + shape.rect.h;
    let scene = Scene::new(SCENE_PT, PX_PER_PT, vec![Group::single(el)]);
    let img = h.render(Backdrop::White, &scene);
    // The cast shift pushes the shadow down; it fades with distance, so
    // the band stays monotone from the edge out to the extent's limit and
    // every pt of it is darker than the white backdrop's 3.0.
    let mut last = 0.0f32;
    for (i, &dy) in [1.0f32, 4.0, 10.0, 15.0].iter().enumerate() {
        let px = probe_px(&img, x, bottom + dy);
        let lum = px[0] + px[1] + px[2];
        assert!(
            lum < 2.995,
            "the cast shadow darkens {dy} pt past the silhouette (lum {lum})",
        );
        assert!(
            i == 0 || lum > last,
            "the shadow fades with distance ({lum} at {dy} pt against {last})",
        );
        last = lum;
    }
}

/// The union field's footprint the contract asserts: touching members
/// composite as one field, so the pixel midway between two circle centres
/// reads the glass interior, not the backdrop or a seam.
#[test]
fn union_field_merges_touching_members() {
    let mut h = EngineHarness::new();
    let make = |cx: f32| {
        element(
            Shape::circle(cx, 100.0, 30.0),
            Family::Regular,
            Appearance::Dark,
            Translucency::default(),
        )
    };
    let group = Group::container(vec![make(100.0), make(160.0)], CONTAINER_UNION_SMOOTHING);
    let scene = Scene::new(SCENE_PT, PX_PER_PT, vec![group]);
    let img = h.render(Backdrop::White, &scene);
    // Midway between the centres, inside the union field: the member body
    // grades the backdrop, so the pixel reads the glass's darker interior.
    let mid = probe_px(&img, 130.0, 100.0);
    assert!(
        mid[0] < 0.95,
        "the union field covers the seam (pixel {mid:?} reads backdrop)",
    );
}

/// The two renderers evaluate the same material against the same
/// capture — their outputs agree within the noise the capture
/// quantization, field evaluation and colour-space round trips make.
#[test]
fn engine_matches_standalone() {
    let mut engine = EngineHarness::new();
    let mut standalone = Harness::new();
    let scene = verification_scene(
        Appearance::Dark,
        Translucency::default(),
        [0.0, 76.0, 152.0],
    );
    let engine_out = engine.render(Backdrop::White, &scene);
    let standalone_out = standalone.render(Backdrop::White, &scene);
    assert_eq!(engine_out.pixels.len(), standalone_out.pixels.len());
    let mut worst = 0.0f32;
    let mut sum = 0.0f64;
    for (p, q) in engine_out.pixels.iter().zip(standalone_out.pixels.iter()) {
        for c in 0..4 {
            let d = (p[c] - q[c]).abs();
            worst = worst.max(d);
            sum += f64::from(d);
        }
    }
    #[expect(
        clippy::cast_precision_loss,
        reason = "a channel count well inside f64's exact integer range"
    )]
    let mean = sum / (engine_out.pixels.len() * 4) as f64;
    assert!(
        mean < 0.02 && worst < 0.4,
        "engine vs standalone: mean |Δ| {mean}, worst {worst}",
    );
}

#[test]
fn scenes_at_default_translucency() {
    let mut h = EngineHarness::new();
    for backdrop in Backdrop::ALL {
        for (appearance, name) in [(Appearance::Dark, "dark"), (Appearance::Light, "light")] {
            let scene = verification_scene(appearance, Translucency::default(), [0.0, 76.0, 152.0]);
            h.export(
                backdrop,
                &scene,
                &format!("{}_{name}_s05_engine", backdrop.name()),
            );
        }
    }
}

#[test]
fn scenes_at_translucency_extremes() {
    let mut h = EngineHarness::new();
    for s in [0.0, 1.0] {
        for (appearance, name) in [(Appearance::Dark, "dark"), (Appearance::Light, "light")] {
            let scene = verification_scene(appearance, Translucency::new(s), [0.0, 76.0, 152.0]);
            h.export(
                Backdrop::White,
                &scene,
                &format!("white_{name}_s{s:02}_engine"),
            );
        }
    }
}

#[test]
fn container_circles_touch() {
    let mut h = EngineHarness::new();
    for (appearance, name) in [(Appearance::Dark, "dark"), (Appearance::Light, "light")] {
        let scene = verification_scene(appearance, Translucency::default(), [0.0, 60.0, 120.0]);
        h.export(
            Backdrop::White,
            &scene,
            &format!("container_touch_{name}_engine"),
        );
    }
}

#[test]
fn element_inside_card_shadow() {
    let mut h = EngineHarness::new();
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
        h.export(
            Backdrop::White,
            &scene,
            &format!("card_shadow_{name}_engine"),
        );
    }
}
