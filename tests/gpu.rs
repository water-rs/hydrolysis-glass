#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::many_single_char_names,
    clippy::needless_range_loop,
    clippy::large_types_passed_by_value,
    clippy::float_cmp,
    reason = "verification harness: pixel arithmetic on synthetic images"
)]
//! Offscreen verification of the glass material against the specification's
//! synthetic scenes. Every test saves its render to `target/verification/`.
//!
//! Tests that need a reference capture call `compare_reference`, which
//! reports them as pending until `GLASS_REFERENCE_CAPTURES` points at a
//! directory of captures.

mod common;

use common::*;
use hydrolysis_glass::material::geometry::{
    CONTAINER_UNION_SMOOTHING, CornerCurve, Rect, Shape, meniscus,
};
use hydrolysis_glass::material::recipe::ColorMatrix;
use hydrolysis_glass::{Appearance, Element, Group, Recipe, Scene, Variant};

const CX: f32 = STD.x + STD.w / 2.0;
const CY: f32 = STD.y + STD.h / 2.0;
const LEFT: f32 = STD.x;
const BOTTOM: f32 = STD.y + STD.h;
const TOP: f32 = STD.y;

fn flat(v: f32) -> Image {
    let (w, h) = scene_px(SCENE[0], SCENE[1]);
    Image::from_fn(w, h, |_, _| grey(v))
}

fn transparent() -> Image {
    let (w, h) = scene_px(SCENE[0], SCENE[1]);
    Image::new(w, h)
}

/// Horizontal luma ramp 0.1 → 0.9 over the scene width.
fn ramp() -> Image {
    let (w, h) = scene_px(SCENE[0], SCENE[1]);
    Image::from_fn(w, h, |x, _| grey(0.1 + 0.8 * x / w as f32))
}

fn ramp_x_of_luma(y: f32) -> f32 {
    (y - 0.1) / 0.8 * SCENE[0]
}

/// Field distance at pt coordinates.
fn d_at(r: &Render, x: f32, y: f32) -> f32 {
    r.field.at_pt(x, y)[0]
}

#[test]
fn adapter_reports() {
    let g = gpu();
    assert!(!g.adapter_name.is_empty());
    eprintln!("VERIFY adapter: {} via {}", g.adapter_name, g.backend);
}

#[test]
fn silhouette_position_and_antialiasing() {
    let backdrop = flat(0.85);
    // Offset by half a pixel so the edge falls inside a pixel, not on a
    // pixel boundary.
    let mut e = std_element(Variant::Regular, Appearance::Dark);
    e.shape.rect.x += 0.5 / PX_PER_PT;
    let scene = single_scene(SCENE, e);
    let r = render(&backdrop, &scene, None);
    r.output.save("silhouette");
    // Zero crossing of the field at the left edge, mid height, within 0.5 px.
    let y = (CY * PX_PER_PT) as u32;
    let mut crossing = None;
    for x in 0..r.field.width - 1 {
        if r.field.at(x, y)[0] > 0.0 && r.field.at(x + 1, y)[0] <= 0.0 {
            crossing = Some(
                x as f32 + 1.0
                    - r.field.at(x + 1, y)[0] / (r.field.at(x, y)[0] - r.field.at(x + 1, y)[0]),
            );
        }
    }
    let crossing = crossing.expect("edge");
    let expected = LEFT.mul_add(PX_PER_PT, 0.5);
    assert!(
        (crossing - expected).abs() <= 0.5,
        "edge at {crossing}, expected {expected}"
    );
    // Coverage (output alpha over a transparent backdrop) transitions over
    // ~1 px: at most two partially covered pixels on the scanline.
    let t = render(&transparent(), &scene, None);
    let mut partial = 0;
    for x in ((LEFT - 3.0) * PX_PER_PT) as u32..((LEFT + 3.0) * PX_PER_PT) as u32 {
        let a = t.output.at(x, y)[3];
        // The shadow alone stays below ~0.3 alpha just outside the edge.
        if a > 0.35 && a < 0.98 {
            partial += 1;
        }
    }
    assert!(
        (1..=2).contains(&partial),
        "partial-coverage pixels: {partial}"
    );
    // Continuous corner: the field lies between the circular corner and the
    // square corner along the diagonal.
    let corner = 40.0;
    let px = LEFT + 0.5 / PX_PER_PT + corner * (1.0 - std::f32::consts::FRAC_1_SQRT_2);
    let py = TOP + corner * (1.0 - std::f32::consts::FRAC_1_SQRT_2);
    let d_diag = d_at(&r, px, py);
    assert!(
        d_diag < 0.0 && d_diag > -corner * 0.3,
        "continuous corner d={d_diag}"
    );
    eprintln!(
        "VERIFY silhouette: edge {crossing:.2} px vs {expected:.2}; AA pixels {partial}; corner d {d_diag:.2}"
    );
    compare_reference(&r.output, "flat_grey_dark", 6.0 / 255.0);
}

#[test]
fn circular_and_continuous_corners_differ() {
    let backdrop = flat(0.85);
    let mut e = std_element(Variant::Regular, Appearance::Dark);
    e.shape.curve = CornerCurve::Circular;
    let circ = render(&backdrop, &single_scene(SCENE, e), None);
    e.shape.curve = CornerCurve::Continuous;
    let cont = render(&backdrop, &single_scene(SCENE, e), None);
    let corner = 40.0;
    // Halfway along the corner arc, the continuous curve sits outside.
    let px = LEFT + corner * (1.0 - std::f32::consts::FRAC_1_SQRT_2);
    let py = TOP + corner * (1.0 - std::f32::consts::FRAC_1_SQRT_2);
    let dc = d_at(&circ, px, py);
    let dk = d_at(&cont, px, py);
    assert!(
        dc.abs() < 0.6,
        "circular corner should pass through the arc point, d={dc}"
    );
    // The continuous curve sits slightly outside the arc, within 0.5 px.
    assert!(
        dk < dc && dk > dc - 0.5 / PX_PER_PT,
        "continuous corner: {dk} vs circular {dc}"
    );
    // Both meet the straight edges exactly.
    assert!((d_at(&cont, LEFT + 0.05, CY) - d_at(&circ, LEFT + 0.05, CY)).abs() < 0.05);
    eprintln!("VERIFY corners: circular d {dc:.2}, continuous d {dk:.2} at the 45° arc point");
}

fn merge_scene(offset: f32) -> Scene {
    // Three 60 pt circles on a row with 16 pt gaps; the middle one is pulled
    // `offset` pt towards the right one.
    let y = 150.0;
    let mk = |x: f32| Element {
        shape: Shape {
            rect: Rect {
                x,
                y,
                w: 60.0,
                h: 60.0,
            },
            radii: [30.0; 4],
            curve: CornerCurve::Circular,
            ovalization: 0.0,
        },
        recipe: Recipe::for_element(60.0, 60.0, Variant::Regular, Appearance::Dark),
    };
    let members = vec![mk(100.0), mk(176.0 + offset), mk(252.0)];
    let smoothing = if offset > 0.0 {
        CONTAINER_UNION_SMOOTHING * 2.0
    } else {
        CONTAINER_UNION_SMOOTHING
    };
    Scene {
        size_pt: SCENE,
        px_per_pt: PX_PER_PT,
        groups: vec![Group::container(members, smoothing)],
    }
}

fn thirds_backdrop() -> Image {
    let (w, h) = scene_px(SCENE[0], SCENE[1]);
    Image::from_fn(w, h, |x, _| {
        let xp = x / PX_PER_PT;
        if xp < 168.0 {
            [1.0, 0.2, 0.2, 1.0]
        } else if xp < 244.0 {
            [0.2, 1.0, 0.2, 1.0]
        } else {
            [0.2, 0.2, 1.0, 1.0]
        }
    })
}

#[test]
fn union_gaps_stay_open_and_ownership_is_local() {
    let r = render(&thirds_backdrop(), &merge_scene(0.0), None);
    r.output.save("union_static");
    // Gap midpoints (x=168, x=244 at y=180) are outside the fused field.
    let g1 = d_at(&r, 168.0, 180.0);
    let g2 = d_at(&r, 244.0, 180.0);
    assert!(g1 > 0.5 && g2 > 0.5, "gaps must stay open: {g1} {g2}");
    // Smoothing: the field near the gap is less than the raw distance (fillet).
    let raw = 8.0;
    assert!(
        g1 < raw - 0.5,
        "the union fillets across the gap: {g1} vs raw {raw}"
    );
    // Ownership: each circle's centre belongs to itself and its face is
    // derived from its own backdrop region.
    for (i, cx) in [130.0, 206.0, 282.0].iter().enumerate() {
        let f = r.field.at_pt(*cx, 180.0);
        assert_eq!(f[3].round() as usize, i, "owner at {cx}");
    }
    let mid = r.output.at_pt(206.0, 180.0);
    assert!(
        mid[1] > mid[0] + 0.1 && mid[1] > mid[2] + 0.1,
        "middle face follows the green region: {mid:?}"
    );
    let left = r.output.at_pt(130.0, 180.0);
    assert!(
        left[0] > left[1] + 0.1,
        "left face follows the red region: {left:?}"
    );
    eprintln!(
        "VERIFY union: gap d {g1:.2}/{g2:.2} > 0 (raw 8), owners 0/1/2, faces track own backdrop"
    );
    compare_reference(&r.output, "merge_offset_0", 8.0 / 255.0);
}

#[test]
fn drag_merge_bridges_the_gap() {
    let r4 = render(&thirds_backdrop(), &merge_scene(4.0), None);
    let r8 = render(&thirds_backdrop(), &merge_scene(8.0), None);
    r4.output.save("union_drag_4");
    r8.output.save("union_drag_8");
    // Middle-right gap midpoint after an 8 pt pull: x = (236+8+252)/2 = 248.
    let d8 = d_at(&r8, 248.0, 180.0);
    assert!(d8 < 0.0, "an 8 pt pull across the 16 pt gap fuses: d={d8}");
    let d4 = d_at(&r4, 246.0, 180.0);
    // No pocket: the bridge is solid along the axis.
    let mut min_bridge = f32::MAX;
    for x in 240..256 {
        min_bridge = min_bridge.min(d_at(&r8, x as f32, 180.0));
    }
    assert!(min_bridge < 0.0);
    eprintln!("VERIFY drag merge: d at midpoint offset 4 = {d4:.2}, offset 8 = {d8:.2} (fused)");
    compare_reference(&r4.output, "merge_offset_4", 8.0 / 255.0);
    compare_reference(&r8.output, "merge_offset_8", 8.0 / 255.0);
}

#[test]
fn refraction_follows_the_meniscus() {
    let backdrop = ramp();
    let scene = single_scene(SCENE, std_element(Variant::Regular, Appearance::Dark));
    let r = render(&backdrop, &scene, None);
    r.output.save("refraction_ramp");
    let recipe = &scene.groups[0].members[0].recipe;
    let face = &recipe.face;
    // Invert the face matrix on the left edge scanline: Y = (out − B)/(W − B).
    let mut worst = 0.0f32;
    let mut errors = Vec::new();
    for depth in [1.5f32, 3.0, 6.0, 10.0, 15.0, 19.0] {
        let x = LEFT + depth;
        let out = luma(r.output.at_pt(x, CY));
        let y_backdrop = (out - face.black) / (face.white - face.black);
        let x_src = ramp_x_of_luma(y_backdrop);
        let measured = x - x_src;
        let expected = -recipe.inner_amt * meniscus(depth, recipe.inner_h);
        let err = (measured - expected).abs();
        eprintln!("VERIFY refraction d=-{depth}: reach {measured:.2} pt, expected {expected:.2}");
        worst = worst.max(err);
        errors.push((depth, measured, expected, err));
    }
    for (depth, measured, expected, err) in errors {
        assert!(
            err <= 0.05f32.mul_add(expected, 1.5),
            "d=-{depth}: {measured} vs {expected}"
        );
    }
    // The interior is undisplaced.
    let out = luma(r.output.at_pt(CX, CY));
    let y_backdrop = (out - face.black) / (face.white - face.black);
    let drift = (ramp_x_of_luma(y_backdrop) - CX).abs();
    assert!(drift < 2.0, "interior displacement {drift}");
    eprintln!("VERIFY refraction: worst reach error {worst:.2} pt; interior drift {drift:.2} pt");
}

#[test]
fn sample_reach_shows_distant_content_in_the_rim() {
    let (w, h) = scene_px(SCENE[0], SCENE[1]);
    let block = |start: f32| {
        Image::from_fn(w, h, |_, y| {
            if y / PX_PER_PT >= BOTTOM + start {
                grey(0.0)
            } else {
                grey(1.0)
            }
        })
    };
    let scene = single_scene(SCENE, std_element(Variant::Regular, Appearance::Dark));
    let near = render(&block(58.0), &scene, None);
    let far = render(&block(70.0), &scene, None);
    near.output.save("reach_block_58");
    // The last interior pixel row on the bottom edge.
    let y = BOTTOM - 0.5 / PX_PER_PT;
    let mut darkest_near = 1.0f32;
    let mut darkest_far = 1.0f32;
    for x in 200..280 {
        darkest_near = darkest_near.min(luma(near.output.at_pt(x as f32, y)));
        darkest_far = darkest_far.min(luma(far.output.at_pt(x as f32, y)));
    }
    assert!(
        darkest_near < darkest_far - 0.01,
        "a block ending 58 pt below must echo in the rim: {darkest_near} vs {darkest_far}"
    );
    eprintln!(
        "VERIFY reach: rim luma with block at 58 pt = {darkest_near:.3}, at 70 pt = {darkest_far:.3}"
    );
}

#[test]
fn outer_lobe_band_pulls_inward() {
    // Red left of x = LEFT+10, blue right of it: the inner lobe on the left
    // rim reaches outward into red, the outer lobe reaches inward into blue.
    let (w, h) = scene_px(SCENE[0], SCENE[1]);
    let backdrop = Image::from_fn(w, h, |x, _| {
        if x / PX_PER_PT < LEFT + 10.0 {
            [1.0, 0.0, 0.0, 1.0]
        } else {
            [0.0, 0.0, 1.0, 1.0]
        }
    });
    let mut scene = single_scene(SCENE, std_element(Variant::Regular, Appearance::Dark));
    scene.px_per_pt = 6.0;
    let r = render(
        &Image::from_fn((SCENE[0] * 6.0) as u32, (SCENE[1] * 6.0) as u32, |x, _| {
            if x / 6.0 < LEFT + 10.0 {
                [1.0, 0.0, 0.0, 1.0]
            } else {
                [0.0, 0.0, 1.0, 1.0]
            }
        }),
        &scene,
        None,
    );
    drop(backdrop);
    r.output.save("outer_lobe_split");
    let y = (CY * 6.0) as u32;
    let sliver = r.output.at(((LEFT + 0.2) * 6.0) as u32, y);
    let deeper = r.output.at(((LEFT + 2.0) * 6.0) as u32, y);
    assert!(
        sliver[2] > deeper[2] + 0.05,
        "the outer-lobe sliver must carry inward (blue) content: {sliver:?} vs {deeper:?}"
    );
    eprintln!(
        "VERIFY outer lobe: blue in sliver {:.3}, at d=-2 {:.3}",
        sliver[2], deeper[2]
    );
}

#[test]
fn interior_blur_and_rim_dip() {
    let backdrop = grid_backdrop();
    let scene = single_scene(SCENE, std_element(Variant::Regular, Appearance::Dark));
    let r = render(&backdrop, &scene, None);
    r.output.save("grid_regular_dark");
    let base = row_contrast(&backdrop, CY, CX - 40.0, CX + 40.0);
    let interior = row_contrast(&r.output, CY, CX - 40.0, CX + 40.0) / base;
    assert!(interior < 0.05, "interior grid contrast {interior}");
    // Rim dip: the blur weight halves over the last pt inside the edge, so
    // the vertical grid lines regain contrast there. Measured on a 300×60
    // element (blurR ≈ 1.6): its interior is smeared while the dip reaches a
    // negative LOD, which blends towards the unblurred backdrop.
    let mut thin = std_element(Variant::Regular, Appearance::Dark);
    thin.shape.rect = Rect {
        x: STD.x,
        y: CY - 30.0,
        w: STD.w,
        h: 60.0,
    };
    thin.recipe = Recipe::for_element(STD.w, 60.0, Variant::Regular, Appearance::Dark);
    let t = render(&backdrop, &single_scene(SCENE, thin), None);
    t.output.save("grid_regular_thin");
    let thin_interior = row_contrast(&t.output, CY, CX - 40.0, CX + 40.0) / base;
    let rim = row_contrast(&t.output, CY + 30.0 - 0.5, CX - 40.0, CX + 40.0) / base;
    assert!(
        rim > thin_interior + 0.05,
        "rim contrast {rim} should exceed interior {thin_interior}"
    );
    eprintln!(
        "VERIFY blur: interior residual {:.1}% of backdrop contrast; m=60 interior {:.1}%, rim dip {:.1}%",
        interior * 100.0,
        thin_interior * 100.0,
        rim * 100.0
    );
    compare_reference(&r.output, "grid_dark", 8.0 / 255.0);
    let light = render(
        &backdrop,
        &single_scene(SCENE, std_element(Variant::Regular, Appearance::Light)),
        None,
    );
    light.output.save("grid_regular_light");
    compare_reference(&light.output, "grid_light", 8.0 / 255.0);
}

#[test]
fn blur_grows_with_element_size() {
    let backdrop = grid_backdrop();
    let mut residuals = Vec::new();
    for m in [40.0f32, 60.0, 80.0, 200.0] {
        let shape = rounded(CX - m, CY - m / 2.0, 2.0 * m, m, m / 4.0);
        let scene = single_scene(SCENE, element(shape, Variant::Regular, Appearance::Dark));
        let r = render(&backdrop, &scene, None);
        r.output.save(&format!("grid_size_{m}"));
        let span = (m / 2.0 - 8.0).max(8.0);
        let base = row_contrast(&backdrop, CY, CX - span, CX + span);
        let res = row_contrast(&r.output, CY, CX - span, CX + span) / base;
        eprintln!(
            "VERIFY blur size m={m}: blurR {:.1}, residual {:.1}%",
            scene.groups[0].members[0].recipe.blur_r,
            res * 100.0
        );
        residuals.push(res);
    }
    assert!(
        residuals[0] >= residuals[1]
            && residuals[1] >= residuals[2]
            && residuals[2] >= residuals[3],
        "{residuals:?}"
    );
    assert!(
        residuals[3] < 0.05,
        "m=200 smears the grid: {}",
        residuals[3]
    );
}

#[test]
fn clear_keeps_the_grid_readable() {
    let backdrop = grid_backdrop();
    let shape = rounded(CX - 150.0, CY - 40.0, 300.0, 80.0, 24.0);
    let scene = single_scene(SCENE, element(shape, Variant::Clear, Appearance::Dark));
    let r = render(&backdrop, &scene, None);
    r.output.save("grid_clear");
    let base = row_contrast(&backdrop, CY, CX - 40.0, CX + 40.0);
    let res = row_contrast(&r.output, CY, CX - 40.0, CX + 40.0) / base;
    // The fixed σ≈1-texel pre-blur at 0.5 scale leaves ~10–30 % of the 8 pt
    // grid's contrast.
    assert!(res > 0.1, "clear residual {res}");
    // No shadow outside.
    let below = r.output.at_pt(CX, CY + 60.0);
    assert!((luma(below) - luma(backdrop.at_pt(CX, CY + 60.0))).abs() < 0.01);
    eprintln!(
        "VERIFY clear: grid residual {:.0}%; no exterior shadow",
        res * 100.0
    );
}

#[test]
fn face_colour_response() {
    for (variant, appearance, v, lo, hi) in [
        (Variant::Regular, Appearance::Dark, 0.85, 0.45, 0.60),
        (Variant::Regular, Appearance::Light, 0.85, 0.86, 1.0),
    ] {
        let r = render(
            &flat(v),
            &single_scene(SCENE, std_element(variant, appearance)),
            None,
        );
        let out = luma(r.output.at_pt(CX, CY));
        assert!(
            out >= lo && out <= hi,
            "{variant:?}/{appearance:?} face {out}"
        );
        eprintln!("VERIFY face {variant:?}/{appearance:?} over {v}: {out:.3}");
    }
    // Clear: 0.075 + 1.075·Y.
    for v in [0.2f32, 0.5, 0.85] {
        let shape = rounded(CX - 150.0, CY - 40.0, 300.0, 80.0, 24.0);
        let r = render(
            &flat(v),
            &single_scene(SCENE, element(shape, Variant::Clear, Appearance::Dark)),
            None,
        );
        let out = luma(r.output.at_pt(CX, CY));
        let expected = 1.075f32.mul_add(v, 0.075).min(1.0);
        assert!(
            (out - expected).abs() < 0.03,
            "clear over {v}: {out} vs {expected}"
        );
        eprintln!("VERIFY clear face over {v}: {out:.3} (expected {expected:.3})");
    }
}

#[test]
fn luma_lut_residuals() {
    // Horizontal ramps: grey, red, green, blue in four bands.
    let (w, h) = scene_px(SCENE[0], SCENE[1]);
    let backdrop = Image::from_fn(w, h, |x, y| {
        let t = 0.05 + 0.9 * x / w as f32;
        match ((y / PX_PER_PT - STD.y) / (STD.h / 4.0)).floor() as i32 {
            0 => [t, t, t, 1.0],
            1 => [t, 0.1, 0.1, 1.0],
            2 => [0.1, t, 0.1, 1.0],
            _ => [0.1, 0.1, t, 1.0],
        }
    });
    let scene = single_scene(SCENE, std_element(Variant::Regular, Appearance::Dark));
    let recipe = scene.groups[0].members[0].recipe;
    let r = render(&backdrop, &scene, None);
    r.output.save("lut_dark");
    let mut worst = [0.0f32; 4];
    for band in 0..4 {
        let y = STD.y + (band as f32 + 0.5) * STD.h / 4.0;
        for i in 0..20 {
            let x = (i as f32).mul_add(12.0, LEFT + 30.0);
            let src = backdrop.at_pt(x, y);
            let expected = recipe.face.apply([src[0], src[1], src[2]]);
            let out = r.output.at_pt(x, y);
            for c in 0..3 {
                worst[band] = worst[band].max((out[c] - expected[c]).abs());
            }
        }
    }
    eprintln!(
        "VERIFY LUT residuals: grey {:.1}/255, red {:.1}/255, green {:.1}/255, blue {:.1}/255",
        worst[0] * 255.0,
        worst[1] * 255.0,
        worst[2] * 255.0,
        worst[3] * 255.0
    );
    assert!(
        worst[0] <= 6.0 / 255.0,
        "grey residual {}",
        worst[0] * 255.0
    );
    for c in 1..4 {
        assert!(
            worst[c] <= 8.0 / 255.0,
            "primary residual {}",
            worst[c] * 255.0
        );
    }
    compare_reference(&r.output, "lut_dark", 8.0 / 255.0);
}

#[test]
fn colour_matrix_on_cpu_matches_definition() {
    let cm = ColorMatrix::new(0.5, 0.6, 1.0);
    let out = cm.apply([0.85, 0.85, 0.85]);
    assert!((out[0] - 0.585).abs() < 1e-5);
}

#[test]
fn shadow_offset_reach_and_opacity() {
    let backdrop = flat(1.0);
    let scene = single_scene(SCENE, std_element(Variant::Regular, Appearance::Dark));
    let recipe = scene.groups[0].members[0].recipe;
    let r = render(&backdrop, &scene, None);
    r.output.save("shadow_white");
    // Peak below the bottom edge: the shadow colour for the 200 pt recipe is
    // the matrixed backdrop (B 0, W 0.5 → 0.5 grey); opacity = shadowOp.
    let just_below = luma(r.output.at_pt(CX, BOTTOM + 1.0));
    let shadow_colour = recipe.shadow_cm.apply([1.0, 1.0, 1.0])[0];
    let opacity = (1.0 - just_below) / (1.0 - shadow_colour);
    assert!(
        (opacity - recipe.shadow_op).abs() < 0.04,
        "shadow opacity {opacity} vs {}",
        recipe.shadow_op
    );
    // Offset: darker below than above at the same distance.
    let below = 1.0 - luma(r.output.at_pt(CX, BOTTOM + 20.0));
    let above = 1.0 - luma(r.output.at_pt(CX, TOP - 20.0));
    assert!(
        below > above + 0.01,
        "shadow offset: below {below} above {above}"
    );
    // Reach: the displaced falloff extends 2·shadowRadius = 48 pt past the
    // shifted silhouette — perceptible to ≈48 pt below, not truncated at
    // positive_range (37). Truly gone only past reach + offset.
    let far = 1.0 - luma(r.output.at_pt(CX, BOTTOM + 30.0));
    let past_range = 1.0 - luma(r.output.at_pt(CX, BOTTOM + recipe.positive_range + 4.0));
    let gone = 1.0
        - luma(r.output.at_pt(
            CX,
            BOTTOM + 2.0 * recipe.shadow_radius + recipe.shadow_offset[1] + 1.0,
        ));
    assert!(
        far > 0.003 && past_range > 0.001 && gone.abs() < 1e-3,
        "reach: {far} at 30 pt, {past_range} past positive_range, {gone} beyond shadow reach"
    );
    eprintln!(
        "VERIFY shadow: peak opacity {opacity:.3} (recipe {:.3}); darkening below/above at 20 pt {below:.3}/{above:.3}; at 30 pt {far:.4}",
        recipe.shadow_op
    );
    // Small element: the shadow colour is black and the opacity ≈ 0.38.
    let small = rounded(CX - 30.0, CY - 30.0, 60.0, 60.0, 30.0);
    let sr = render(
        &backdrop,
        &single_scene(SCENE, element(small, Variant::Regular, Appearance::Dark)),
        None,
    );
    let sm_recipe = Recipe::for_element(60.0, 60.0, Variant::Regular, Appearance::Dark);
    let sm_op = 1.0 - luma(sr.output.at_pt(CX, CY + 30.0 + 1.0));
    assert!(
        (sm_op - sm_recipe.shadow_op).abs() < 0.05,
        "small shadow {sm_op} vs {}",
        sm_recipe.shadow_op
    );
    eprintln!(
        "VERIFY shadow m=60: opacity {sm_op:.3} (recipe {:.3})",
        sm_recipe.shadow_op
    );
}

#[test]
fn interactive_shadow_and_tint() {
    let backdrop = flat(1.0);
    let shape = capsule(CX - 80.0, CY - 22.0, 160.0, 44.0);
    let mut e = element(shape, Variant::Interactive, Appearance::Dark);
    e.recipe.tint = Some(hydrolysis_glass::material::Tint {
        rgb: [0.2, 0.5, 1.0],
        alpha: 1.0,
    });
    let r = render(&backdrop, &single_scene(SCENE, e), None);
    r.output.save("interactive_tinted");
    let face = r.output.at_pt(CX, CY);
    assert!(face[2] > face[0] + 0.1, "tint shows on the face: {face:?}");
    let below = 1.0 - luma(r.output.at_pt(CX, CY + 22.0 + 1.0));
    assert!(below > 0.05, "interactive elements cast a shadow: {below}");
    let pressed = Element {
        recipe: e.recipe.pressed(1.0),
        ..e
    };
    let pr = render(&backdrop, &single_scene(SCENE, pressed), None);
    let below_pressed = 1.0 - luma(pr.output.at_pt(CX, CY + 22.0 + 1.0));
    assert!(
        below_pressed > below * 1.5,
        "press deepens the shadow: {below_pressed} vs {below}"
    );
    eprintln!(
        "VERIFY interactive: tinted face {face:?}; shadow {below:.3}, pressed {below_pressed:.3}"
    );
}

#[test]
fn highlight_is_directional_narrow_and_content_modulated() {
    let backdrop = flat(0.5);
    let scene = single_scene(SCENE, std_element(Variant::Regular, Appearance::Dark));
    let r = render(&backdrop, &scene, None);
    r.output.save("highlight_grey");
    let face = luma(r.output.at_pt(CX, CY));
    let inset = 0.5 / PX_PER_PT;
    // Upper-left arc vs lower-right arc, 45° points.
    let k = 40.0f32.mul_add(1.0 - std::f32::consts::FRAC_1_SQRT_2, inset);
    let ul = luma(r.output.at_pt(LEFT + k, TOP + k));
    let lr = luma(r.output.at_pt(LEFT + STD.w - k, BOTTOM - k));
    let left_mid = luma(r.output.at_pt(LEFT + inset, CY));
    assert!(
        ul > face + 0.05,
        "key highlight present at upper-left: {ul} vs face {face}"
    );
    assert!(
        ul > lr + 0.05,
        "key is the dominant light, strictly over the fill: {ul} vs {lr}"
    );
    // Band width: bright pixels along the left edge at mid height span ≤ 2 pt.
    let y = (CY * PX_PER_PT) as u32;
    let mut bright = 0;
    for x in (LEFT * PX_PER_PT) as u32..((LEFT + 6.0) * PX_PER_PT) as u32 {
        if luma(r.output.at(x, y)) > face + 0.03 {
            bright += 1;
        }
    }
    assert!(
        bright as f32 <= 2.0 * PX_PER_PT,
        "highlight band {bright} px"
    );
    // Content modulation: dimmer over black.
    let dark = render(&flat(0.0), &scene, None);
    let ul_dark = luma(dark.output.at_pt(LEFT + k, TOP + k)) - luma(dark.output.at_pt(CX, CY));
    assert!(
        ul_dark < ul - face,
        "highlight dimmer over black: {ul_dark} vs {}",
        ul - face
    );
    eprintln!(
        "VERIFY highlight: UL {ul:.3}, LR {lr:.3}, left-mid {left_mid:.3}, face {face:.3}; band {bright} px; over black +{ul_dark:.3}"
    );
    compare_reference(&r.output, "rim_grey", 8.0 / 255.0);
}

#[test]
fn bleed_halo_outside_large_elements() {
    let backdrop = flat(1.0);
    let scene = single_scene(SCENE, std_element(Variant::Regular, Appearance::Dark));
    let mut without = scene.clone();
    without.groups[0].members[0].recipe.bleed_op = 0.0;
    let with = render(&backdrop, &scene, None);
    let none = render(&backdrop, &without, None);
    // Just outside the left edge (no shadow contribution horizontally at the
    // mid height beyond the falloff).
    let x = LEFT - 0.5;
    let a = luma(with.output.at_pt(x, CY));
    let b = luma(none.output.at_pt(x, CY));
    assert!(
        (a - b).abs() > 0.01,
        "bleed halo changes the exterior band: {a} vs {b}"
    );
    assert!(
        (luma(with.output.at_pt(LEFT - 6.0, CY)) - luma(none.output.at_pt(LEFT - 6.0, CY))).abs()
            < 0.01
    );
    eprintln!("VERIFY bleed: exterior 0.5 pt luma {a:.3} with bleed, {b:.3} without");
}

#[test]
fn sdr_band_pulls_the_rim_to_a_fixed_tone() {
    // With the band at full strength, the texel at mid-band (d ≈ −0.5 pt)
    // becomes the fixed tone `sdrWhite·sat(a)` regardless of the backdrop —
    // over black that is pure sdrWhite, not the darkened material.
    let mut e = std_element(Variant::Regular, Appearance::Dark);
    e.recipe.sdr_op = 1.0;
    let scene = single_scene(SCENE, e);
    let r = render(&flat(0.0), &scene, None);
    r.output.save("sdr_band_full");
    let band = luma(r.output.at_pt(LEFT + 0.5, CY));
    assert!(
        band > 0.9,
        "band pixel pinned to sdrWhite over black: {band}"
    );
    // Deeper inside, the same recipe shows the ordinary dark material.
    let interior = luma(r.output.at_pt(LEFT + 4.0, CY));
    assert!(
        interior < band - 0.3,
        "the pull is confined to the band: {interior} vs {band}"
    );
    // With the band off, the same rim texel keeps the material colour.
    let mut off = std_element(Variant::Regular, Appearance::Dark);
    off.recipe.sdr_op = 0.0;
    let r0 = render(&flat(0.0), &single_scene(SCENE, off), None);
    let unheld = luma(r0.output.at_pt(LEFT + 0.5, CY));
    assert!(
        band > unheld + 0.3,
        "the band changes the rim tone: {band} vs {unheld}"
    );
    eprintln!("VERIFY SDR tone: band {band:.3}, interior {interior:.3}, off {unheld:.3}");
}

#[test]
fn sdr_band_leaves_no_translucent_hairline() {
    let scene = single_scene(SCENE, std_element(Variant::Regular, Appearance::Dark));
    let r = render(&transparent(), &scene, None);
    r.output.save("hairline_alpha");
    let y = (CY * PX_PER_PT) as u32;
    let mut min_alpha = 1.0f32;
    for x in ((LEFT + 0.5) * PX_PER_PT).ceil() as u32..((LEFT + 4.0) * PX_PER_PT) as u32 {
        min_alpha = min_alpha.min(r.output.at(x, y)[3]);
    }
    assert!(min_alpha > 0.98, "translucent hairline: alpha {min_alpha}");
    // With an opaque backdrop the band stays monotone: no pixel inside the
    // first 2 pt falls back to the backdrop colour.
    let o = render(&flat(0.85), &scene, None);
    for x in ((LEFT + 0.5) * PX_PER_PT).ceil() as u32..((LEFT + 2.0) * PX_PER_PT) as u32 {
        let l = luma(o.output.at(x, y));
        assert!(
            (l - 0.85).abs() > 0.05,
            "backdrop shows through at x={x}: {l}"
        );
    }
    eprintln!("VERIFY SDR band: minimum interior alpha {min_alpha:.4}, no seam");
}

#[test]
fn foreground_dispersion_fringes_and_fades() {
    let (w, h) = scene_px(SCENE[0], SCENE[1]);
    // Thin black strokes crossing the left edge, on a transparent layer.
    let content = Image::from_fn(w, h, |x, y| {
        let yp = y / PX_PER_PT;
        let xp = x / PX_PER_PT;
        if (yp - CY).rem_euclid(12.0) < 1.5 && xp > LEFT - 20.0 && xp < LEFT + 60.0 {
            [0.0, 0.0, 0.0, 1.0]
        } else {
            [0.0; 4]
        }
    });
    let backdrop = flat(1.0);
    let scene = single_scene(SCENE, std_element(Variant::Regular, Appearance::Dark));
    let r = render(&backdrop, &scene, Some(&content));
    r.output.save("text_edge");
    // Fringe: chroma near the edge exceeds chroma deep inside.
    let chroma = |x0: f32, x1: f32| {
        let mut m = 0.0f32;
        for y in ((CY - 8.0) * PX_PER_PT) as u32..((CY + 8.0) * PX_PER_PT) as u32 {
            for x in (x0 * PX_PER_PT) as u32..(x1 * PX_PER_PT) as u32 {
                let p = r.output.at(x, y);
                m = m.max((p[0] - p[2]).abs());
            }
        }
        m
    };
    let rim = chroma(LEFT + 0.5, LEFT + 6.0);
    let deep = chroma(LEFT + 40.0, LEFT + 55.0);
    assert!(rim > 0.02, "spectral fringe at the rim: {rim}");
    assert!(
        rim > deep,
        "fringe strongest at the edge: rim {rim} deep {deep}"
    );
    // Fade: at the silhouette the stroke is faint; deep inside it is dark.
    let at_edge = luma(r.output.at_pt(LEFT + 0.3, CY + 0.7));
    let inside = luma(r.output.at_pt(LEFT + 30.0, CY + 0.7));
    let face = luma(r.output.at_pt(LEFT + 30.0, CY + 6.0));
    assert!(
        inside < face - 0.3,
        "stroke visible inside: {inside} vs face {face}"
    );
    assert!(
        at_edge > inside + 0.1,
        "stroke fades at the edge: {at_edge} vs {inside}"
    );
    // Outside the glass the content is untouched (not drawn by this pass).
    eprintln!(
        "VERIFY dispersion: rim chroma {rim:.3}, deep {deep:.3}; stroke luma edge {at_edge:.3}, inside {inside:.3}"
    );
    compare_reference(&r.output, "text_edge", 10.0 / 255.0);
}

#[test]
fn press_hold_frames_pending_reference() {
    let backdrop = flat(0.85);
    let shape = capsule(CX - 80.0, CY - 22.0, 160.0, 44.0);
    let e = element(shape, Variant::Interactive, Appearance::Dark);
    let pressed = Element {
        recipe: e.recipe.pressed(1.0),
        ..e
    };
    let r = render(&backdrop, &single_scene(SCENE, pressed), None);
    r.output.save("press_hold");
    compare_reference(&r.output, "press_hold", 8.0 / 255.0);
}
