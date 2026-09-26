//! Interaction dynamics: press, drag, neighbour attraction, merge smoothing
//! and shape morphing.
//!
//! Inputs are reactive signals ([`nami::Binding`] / [`nami::Computed`]);
//! the dynamics are stepped once per frame and publish the [`Group`] the
//! renderer draws. Every constant here is **[inferred]**: the reference
//! exercise produced no measurable interactive change, so these are starting
//! values to verify on a device (see the specification's interaction
//! chapter).

use nami::{Binding, Computed, Signal, SignalExt};

use crate::material::geometry::Shape;
use crate::material::gpu::{Element, Group};
use crate::material::recipe::{Appearance, Recipe, Variant};

/// Bounds growth at full press: `1 + 0.07·press` about the centre.
/// **[inferred]**
pub const PRESS_SCALE: f32 = 0.07;
/// Exponential approach rate of `press` per frame (≈95 % in ~220 ms at
/// 60 fps). **[inferred]**
pub const PRESS_RATE: f32 = 0.22;
/// Exponential approach rate of the drag offset per frame (≈95 % in
/// ~170 ms). **[inferred]**
pub const DRAG_RATE: f32 = 0.28;
/// Corner-radius morphs ease at the press rate. **[unknown; chosen]**
pub const MORPH_RATE: f32 = PRESS_RATE;
/// Centre-to-centre radius inside which neighbours lean towards a dragged
/// element, pt. **[inferred]**
pub const ATTRACTION_RADIUS: f32 = 140.0;
/// Peak of the quadratic lean term, pt. **[inferred]**
pub const ATTRACTION_PEAK: f32 = 18.0;
/// Surface gap kept free of the lean, pt. **[inferred]**
pub const ATTRACTION_GAP_MARGIN: f32 = 6.0;
/// Fraction of the remaining gap a neighbour may close by leaning.
/// **[inferred]**
pub const ATTRACTION_GAP_FRACTION: f32 = 0.5;
/// Factor by which a group's union smoothing grows while a member is
/// grabbed (`τ` tracks drag state). **[unknown; chosen]**
///
/// ×1.35 at full drag on the 30 pt container smoothing, which bridges the
/// gap closed by an 8 pt pull across a 16 pt gap while the gap opening on
/// the far side (≈19 pt after the press growth and the neighbour's lean)
/// stays open.
pub const DRAG_SMOOTHING_GAIN: f32 = 0.35;

/// Exponential approach of `current` towards `target` by `rate`.
#[must_use]
pub fn approach(current: f32, target: f32, rate: f32) -> f32 {
    (target - current).mul_add(rate.clamp(0.0, 1.0), current)
}

fn approach2(current: [f32; 2], target: [f32; 2], rate: f32) -> [f32; 2] {
    [
        approach(current[0], target[0], rate),
        approach(current[1], target[1], rate),
    ]
}

/// Neighbour lean: `û · min(18·(1 − dist/140)², 0.5·max(gap − 6, 0))`.
///
/// For an element whose centre lies `delta` away from the dragged
/// element's centre, with `gap` the remaining surface-to-surface distance.
/// Returns the zero vector beyond the attraction radius.
#[must_use]
pub fn attraction(delta: [f32; 2], gap: f32) -> [f32; 2] {
    let dist = delta[0].hypot(delta[1]);
    if dist <= 1e-3 || dist >= ATTRACTION_RADIUS {
        return [0.0; 2];
    }
    let lean = ATTRACTION_PEAK * (1.0 - dist / ATTRACTION_RADIUS).powi(2);
    let cap = ATTRACTION_GAP_FRACTION * (gap - ATTRACTION_GAP_MARGIN).max(0.0);
    let pull = lean.min(cap);
    [delta[0] / dist * pull, delta[1] / dist * pull]
}

/// Reactive inputs of one interactive element.
#[derive(Clone, Debug)]
pub struct Controls {
    /// `true` while pressed or held; `press` approaches 1.
    pub pressed: Binding<bool>,
    /// Pointer offset from the grab point while dragged, `None` at rest.
    pub drag: Binding<Option<[f32; 2]>>,
    /// Target corner radii; morphs a capsule into a rect and back.
    pub radii: Binding<[f32; 4]>,
}

impl Controls {
    /// Idle controls with the given corner radii.
    #[must_use]
    pub fn new(radii: [f32; 4]) -> Self {
        Self {
            pressed: Binding::container(false),
            drag: Binding::container(None),
            radii: Binding::container(radii),
        }
    }
}

/// One member of an interactive group: its resting shape, its reactive
/// controls and the eased dynamic state.
#[derive(Clone, Debug)]
pub struct Member {
    /// Resting shape (bounds without press growth or drag translation).
    pub rest: Shape,
    /// Variant used for the recipe.
    pub variant: Variant,
    /// Reactive inputs.
    pub controls: Controls,
    press: f32,
    offset: [f32; 2],
    radii: [f32; 4],
}

impl Member {
    /// A member at rest.
    #[must_use]
    pub fn new(rest: Shape, variant: Variant) -> Self {
        Self {
            rest,
            variant,
            controls: Controls::new(rest.radii),
            press: 0.0,
            offset: [0.0; 2],
            radii: rest.radii,
        }
    }

    /// Current eased press, `0..=1`.
    #[must_use]
    pub const fn press(&self) -> f32 {
        self.press
    }

    /// Current eased translation, pt.
    #[must_use]
    pub const fn offset(&self) -> [f32; 2] {
        self.offset
    }

    fn grabbed(&self) -> bool {
        self.controls.drag.snapshot().is_some()
    }

    /// The shape as drawn this frame: rest bounds translated by the offset,
    /// grown by `1 + 0.07·press` about the centre, with the eased radii.
    #[must_use]
    pub fn shape(&self) -> Shape {
        let grow = PRESS_SCALE.mul_add(self.press, 1.0);
        Shape {
            rect: self
                .rest
                .rect
                .scaled_about_center(grow)
                .translated(self.offset[0], self.offset[1]),
            radii: self.radii.map(|r| r * grow),
            curve: self.rest.curve,
            ovalization: self.rest.ovalization,
        }
    }

    /// The element as drawn this frame.
    #[must_use]
    pub fn element(&self, appearance: Appearance) -> Element {
        let shape = self.shape();
        Element {
            shape,
            recipe: Recipe::for_element(
                self.rest.rect.w,
                self.rest.rect.h,
                self.variant,
                appearance,
            )
            .pressed(self.press),
        }
    }

    fn step(&mut self, pull: [f32; 2]) {
        let press_target = if self.controls.pressed.snapshot() || self.grabbed() {
            1.0
        } else {
            0.0
        };
        self.press = approach(self.press, press_target, PRESS_RATE);
        let offset_target = self.controls.drag.snapshot().unwrap_or(pull);
        self.offset = approach2(self.offset, offset_target, DRAG_RATE);
        let radii_target = self.controls.radii.snapshot();
        for (r, t) in self.radii.iter_mut().zip(radii_target) {
            *r = approach(*r, t, MORPH_RATE);
        }
    }
}

/// Surface-to-surface gap between two shapes along the line of centres,
/// approximated from the inscribed radii in that direction.
fn surface_gap(a: &Shape, b: &Shape) -> f32 {
    let [ax, ay] = a.rect.center();
    let [bx, by] = b.rect.center();
    let delta = [bx - ax, by - ay];
    let dist = delta[0].hypot(delta[1]);
    if dist <= 1e-3 {
        return 0.0;
    }
    let dir = [delta[0] / dist, delta[1] / dist];
    let extent = |s: &Shape| {
        let hx = s.rect.w * 0.5;
        let hy = s.rect.h * 0.5;
        // Distance from the centre to the silhouette along `dir`, found by
        // marching the signed distance.
        let mut t = 0.0f32;
        let [cx, cy] = s.rect.center();
        for _ in 0..8 {
            let d = s.distance([dir[0].mul_add(t, cx), dir[1].mul_add(t, cy)]);
            t -= d;
        }
        t.clamp(0.0, hx.hypot(hy))
    };
    (dist - extent(a) - extent(b)).max(0.0)
}

/// A group of interactive members sharing one union field. Stepping it once
/// per frame eases every member towards its signal-driven targets and
/// publishes the [`Group`] to draw.
#[derive(Debug)]
pub struct Interactive {
    /// Members in ownership-index order.
    pub members: Vec<Member>,
    /// Resting union smoothing, pt (8 standalone, 30 container).
    pub smoothing: f32,
    /// Appearance used for the recipes.
    pub appearance: Appearance,
    drag_activity: f32,
    frame: Binding<Group>,
}

impl Interactive {
    /// A group at rest.
    #[must_use]
    pub fn new(members: Vec<Member>, smoothing: f32, appearance: Appearance) -> Self {
        let this = Self {
            members,
            smoothing,
            appearance,
            drag_activity: 0.0,
            frame: Binding::container(Group::container(Vec::new(), smoothing)),
        };
        this.publish();
        this
    }

    /// The group as published by the last [`step`](Self::step), as a signal.
    #[must_use]
    pub fn frame(&self) -> Computed<Group> {
        self.frame.computed()
    }

    /// The group as published by the last step.
    #[must_use]
    pub fn group(&self) -> Group {
        self.frame.snapshot()
    }

    /// Effective union smoothing this frame: the resting value grown by the
    /// eased drag activity.
    #[must_use]
    pub fn effective_smoothing(&self) -> f32 {
        self.smoothing * DRAG_SMOOTHING_GAIN.mul_add(self.drag_activity, 1.0)
    }

    /// Advances the dynamics by one frame.
    pub fn step(&mut self) {
        let dragged = self.members.iter().position(Member::grabbed);
        let shapes: Vec<Shape> = self.members.iter().map(Member::shape).collect();
        let pulls: Vec<[f32; 2]> = dragged.map_or_else(
            || vec![[0.0; 2]; shapes.len()],
            |i| {
                let target = &shapes[i];
                let [tx, ty] = target.rect.center();
                shapes
                    .iter()
                    .enumerate()
                    .map(|(j, s)| {
                        if j == i {
                            return [0.0; 2];
                        }
                        let [cx, cy] = s.rect.center();
                        attraction([tx - cx, ty - cy], surface_gap(s, target))
                    })
                    .collect()
            },
        );
        for (m, pull) in self.members.iter_mut().zip(pulls) {
            m.step(pull);
        }
        let activity_target = if dragged.is_some() { 1.0 } else { 0.0 };
        self.drag_activity = approach(self.drag_activity, activity_target, DRAG_RATE);
        self.publish();
    }

    fn publish(&self) {
        let appearance = self.appearance;
        let group = Group::container(
            self.members.iter().map(|m| m.element(appearance)).collect(),
            self.effective_smoothing(),
        );
        self.frame.set(group);
    }

    /// Steps until every eased quantity is within `epsilon` of its target or
    /// `max_frames` elapse; returns the frames taken.
    pub fn settle(&mut self, epsilon: f32, max_frames: usize) -> usize {
        for frame in 0..max_frames {
            let before = self.group();
            self.step();
            let after = self.group();
            let moved = before.members.iter().zip(&after.members).any(|(a, b)| {
                (a.shape.rect.x - b.shape.rect.x).abs() > epsilon
                    || (a.shape.rect.y - b.shape.rect.y).abs() > epsilon
                    || (a.shape.rect.w - b.shape.rect.w).abs() > epsilon
                    || (a.shape.radii[0] - b.shape.radii[0]).abs() > epsilon
                    || (a.recipe.shadow_op - b.recipe.shadow_op).abs() > epsilon
            }) || (before.smoothing - after.smoothing).abs() > epsilon;
            if !moved {
                return frame + 1;
            }
        }
        max_frames
    }
}

#[cfg(test)]
#[allow(
    clippy::suboptimal_flops,
    reason = "tests spell out the specification's arithmetic"
)]
mod tests {
    use super::*;
    use crate::material::geometry::{CONTAINER_UNION_SMOOTHING, CornerCurve, Rect, UnionField};

    fn circle(x: f32) -> Shape {
        Shape {
            rect: Rect {
                x,
                y: 0.0,
                w: 60.0,
                h: 60.0,
            },
            radii: [30.0; 4],
            curve: CornerCurve::Circular,
            ovalization: 0.0,
        }
    }

    fn container() -> Interactive {
        Interactive::new(
            vec![
                Member::new(circle(0.0), Variant::Regular),
                Member::new(circle(76.0), Variant::Regular),
                Member::new(circle(152.0), Variant::Regular),
            ],
            CONTAINER_UNION_SMOOTHING,
            Appearance::Dark,
        )
    }

    #[test]
    fn idle_is_static() {
        let mut g = container();
        let a = g.group();
        for _ in 0..10 {
            g.step();
        }
        assert_eq!(g.group(), a);
    }

    #[test]
    fn press_grows_seven_percent_and_deepens_the_shadow() {
        let mut g = container();
        let rest = g.group().members[1].recipe.shadow_op;
        g.members[1].controls.pressed.set(true);
        // 95 % settle in about 13 frames at 0.22/frame.
        for _ in 0..13 {
            g.step();
        }
        assert!(g.members[1].press() > 0.95);
        g.settle(1e-4, 200);
        let e = &g.group().members[1];
        assert!((e.shape.rect.w - 60.0 * 1.07).abs() < 1e-2);
        let [cx, cy] = e.shape.rect.center();
        assert!(
            (cx - 106.0).abs() < 1e-3 && (cy - 30.0).abs() < 1e-3,
            "grows about the centre"
        );
        assert!((e.recipe.shadow_op - (rest * 2.2).min(1.0)).abs() < 1e-3);
        g.members[1].controls.pressed.set(false);
        g.settle(1e-4, 200);
        assert!(
            (g.group().members[1].shape.rect.w - 60.0).abs() < 1e-2,
            "symmetric return"
        );
    }

    #[test]
    fn drag_settles_in_about_170ms_and_neighbours_lean_in() {
        let mut g = container();
        g.members[1].controls.drag.set(Some([8.0, 0.0]));
        for _ in 0..10 {
            g.step();
        }
        // 1 - 0.72^10 ≈ 0.963
        let off = g.members[1].offset()[0];
        assert!(
            off > 0.95 * 8.0 && off < 8.0,
            "offset after 10 frames: {off}"
        );
        g.settle(1e-4, 200);
        let left = g.members[0].offset();
        let right = g.members[2].offset();
        assert!(left[0] > 0.0, "left neighbour leans right: {left:?}");
        assert!(right[0] < 0.0, "right neighbour leans left: {right:?}");
        // The gap cap keeps neighbours from closing more than half the gap.
        let gap_right = 16.0 - 8.0; // the pull closed 8 of the 16 pt gap
        assert!(-right[0] <= 0.5 * (gap_right - 6.0) + 1e-3, "{right:?}");
        // Smoothing grows while grabbed.
        assert!(g.effective_smoothing() > CONTAINER_UNION_SMOOTHING * 1.3);
        // Release: everything returns.
        g.members[1].controls.drag.set(None);
        g.settle(1e-4, 400);
        for m in &g.members {
            assert!(m.offset()[0].abs() < 1e-2);
        }
        assert!((g.effective_smoothing() - CONTAINER_UNION_SMOOTHING).abs() < 1e-2);
    }

    #[test]
    fn eight_point_pull_bridges_the_sixteen_point_gap() {
        let mut g = container();
        g.members[1].controls.drag.set(Some([8.0, 0.0]));
        g.settle(1e-4, 400);
        let group = g.group();
        let shapes: Vec<Shape> = group.members.iter().map(|e| e.shape).collect();
        let field = UnionField::new(&shapes, group.smoothing);
        // Midpoint of the (pulled) right gap.
        let right = group.members[2].shape.rect;
        let mid = group.members[1].shape.rect;
        let x = (mid.x + mid.w + right.x) * 0.5;
        assert!(
            field.sample([x, 30.0]).distance < 0.0,
            "gap bridged at x={x}"
        );
        // The static left gap stays open (its neighbour only leans).
        let left = group.members[0].shape.rect;
        let x_left = (left.x + left.w + mid.x) * 0.5;
        assert!(
            field.sample([x_left, 30.0]).distance > 0.0,
            "left gap open at x={x_left}"
        );
    }

    #[test]
    fn corner_radius_morphs_continuously() {
        let mut g = Interactive::new(
            vec![Member::new(
                Shape {
                    rect: Rect {
                        x: 0.0,
                        y: 0.0,
                        w: 200.0,
                        h: 80.0,
                    },
                    radii: [40.0; 4],
                    curve: CornerCurve::Continuous,
                    ovalization: 0.5,
                },
                Variant::Regular,
            )],
            8.0,
            Appearance::Light,
        );
        g.members[0].controls.radii.set([12.0; 4]);
        let mut last = 40.0;
        for _ in 0..30 {
            g.step();
            let r = g.group().members[0].shape.radii[0];
            assert!(r < last && r >= 12.0);
            last = r;
        }
        g.settle(1e-4, 200);
        assert!((g.group().members[0].shape.radii[0] - 12.0).abs() < 1e-2);
    }

    #[test]
    fn attraction_law() {
        assert_eq!(attraction([200.0, 0.0], 100.0), [0.0; 2]);
        let far = attraction([100.0, 0.0], 100.0);
        assert!((far[0] - 18.0 * (1.0 - 100.0 / 140.0f32).powi(2)).abs() < 1e-4);
        let capped = attraction([70.0, 0.0], 10.0);
        assert!((capped[0] - 2.0).abs() < 1e-4);
    }

    #[test]
    fn frame_signal_tracks_steps() {
        let mut g = container();
        let frame = g.frame();
        g.members[0].controls.pressed.set(true);
        g.step();
        assert!(frame.snapshot().members[0].shape.rect.w > 60.0);
    }
}
