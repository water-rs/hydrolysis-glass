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
                    let frame = cherenkov::kurbo::Rect::new(
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
