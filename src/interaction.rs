//! Interaction dynamics: spring-driven drag, neighbour attraction, merge
//! smoothing, jelly deformation, and shape morphing.
//!
//! Inputs are reactive signals ([`nami::Binding`] / [`nami::Computed`]);
//! the dynamics step analytically through [`Spring`] presets and publish the
//! [`Group`] the renderer draws.

use nami::{Binding, Computed, Signal, SignalExt};

pub use crate::dynamics::*;
use crate::material::geometry::{Element, Shape};
use crate::material::gpu::Group;
use crate::material::recipe::{Appearance, Family, Recipe, Translucency};

/// Default fixed step duration for [`Interactive::step`], corresponding to 60 Hz.
pub const DEFAULT_FRAME_DT: f32 = 1.0 / 60.0;

/// Position tolerance for spring settlement checks, pt.
const SETTLE_POS_EPS: f32 = 1e-4;
/// Velocity tolerance for spring settlement checks, pt/s.
const SETTLE_VEL_EPS: f32 = 1e-2;

/// Centre-to-centre radius inside which neighbours lean towards a dragged
/// element, pt. **[chosen]**
pub const ATTRACTION_RADIUS: f32 = 140.0;
/// Peak of the quadratic lean term, pt. **[chosen]**
pub const ATTRACTION_PEAK: f32 = 18.0;
/// Surface gap kept free of the lean, pt. **[chosen]**
pub const ATTRACTION_GAP_MARGIN: f32 = 6.0;
/// Fraction of the remaining gap a neighbour may close by leaning.
/// **[chosen]**
pub const ATTRACTION_GAP_FRACTION: f32 = 0.5;
/// Factor by which a group's union smoothing grows while a member is
/// grabbed (the blend distance tracks drag state). **[chosen]**
pub const DRAG_SMOOTHING_GAIN: f32 = 0.2;

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
            drag: Binding::container(None),
            radii: Binding::container(radii),
        }
    }
}

/// One member of an interactive group: its resting shape, its reactive
/// controls and its spring-driven dynamic state.
#[derive(Clone, Debug)]
pub struct Member {
    /// Resting shape (bounds without drag translation).
    pub rest: Shape,
    /// The family used for the recipe.
    pub family: Family,
    /// Reactive inputs.
    pub controls: Controls,
    offset: SpringState2,
    strain: SpringState,
    stretch_axis: [f32; 2],
    radii: [SpringState; 4],
}

impl Member {
    /// A member at rest.
    #[must_use]
    pub fn new(rest: Shape, family: Family) -> Self {
        Self {
            rest,
            family,
            controls: Controls::new(rest.radii),
            offset: SpringState2::at([0.0; 2]),
            strain: SpringState::at(0.0),
            stretch_axis: [1.0, 0.0],
            radii: rest.radii.map(SpringState::at),
        }
    }

    /// Current spring-eased translation, pt.
    #[must_use]
    pub const fn offset(&self) -> [f32; 2] {
        self.offset.value()
    }

    /// Current offset velocity, pt/s.
    #[must_use]
    pub const fn velocity(&self) -> [f32; 2] {
        self.offset.velocity()
    }

    /// Current area-preserving jelly drag deformation tensor.
    #[must_use]
    pub fn tensor(&self) -> JellyTensor {
        JellyTensor::from_strain(self.stretch_axis, self.strain.value, self.offset())
    }

    fn grabbed(&self) -> bool {
        self.controls.drag.snapshot().is_some()
    }

    /// The shape as drawn this frame: rest bounds translated by the offset,
    /// with the eased radii. Press and drag never change its size (spec
    /// §5.5).
    #[must_use]
    pub fn shape(&self) -> Shape {
        let mut s = self.rest.with_radii(self.radii.map(|r| r.value));
        let offset = self.offset();
        s.rect = s.rect.translated(offset[0], offset[1]);
        s
    }

    /// The element as drawn this frame: translated, but with the idle recipe —
    /// interaction does not change the recipe (spec §5.5).
    #[must_use]
    pub fn element(&self, appearance: Appearance, translucency: Translucency) -> Element {
        let shape = self.shape();
        let recipe = Recipe::for_element(
            self.rest.rect.w,
            self.rest.rect.h,
            self.family,
            appearance,
            translucency,
        );
        Element { shape, recipe }
    }

    fn step_dt(&mut self, pull: [f32; 2], dt: f32) {
        let grabbed_delta = self.controls.drag.snapshot();
        let grabbed = grabbed_delta.is_some();
        let offset_target = grabbed_delta.unwrap_or(pull);
        let drag_spring = if grabbed {
            Spring::JELLY_DRAG_ACTIVE
        } else {
            Spring::JELLY_DRAG_RELEASE
        };
        let next_offset = self.offset.step(drag_spring, offset_target, dt);

        let dir_norm = offset_target[0].hypot(offset_target[1]);
        if dir_norm > 1e-6 {
            self.stretch_axis = [offset_target[0] / dir_norm, offset_target[1] / dir_norm];
        } else {
            let vel = self.offset.velocity();
            let vel_norm = vel[0].hypot(vel[1]);
            if vel_norm > 1e-3 {
                self.stretch_axis = [vel[0] / vel_norm, vel[1] / vel_norm];
            }
        }
        let strain_target = if grabbed {
            jelly_strain_target(next_offset, self.offset.velocity())
        } else {
            0.0
        };
        self.strain.step(drag_spring, strain_target, dt);

        let radii_target = self.controls.radii.snapshot();
        for (state, target) in self.radii.iter_mut().zip(radii_target) {
            state.step(Spring::SWITCH_KNOB, target, dt);
        }
    }

    fn is_settled(&self, pull: [f32; 2], epsilon: f32) -> bool {
        let offset_target = self.controls.drag.snapshot().unwrap_or(pull);
        let radii_target = self.controls.radii.snapshot();
        let strain_target = if self.grabbed() {
            jelly_strain_target(offset_target, self.offset.velocity())
        } else {
            0.0
        };
        self.offset
            .is_settled(offset_target, epsilon, SETTLE_VEL_EPS)
            && self
                .strain
                .is_settled(strain_target, epsilon, SETTLE_VEL_EPS)
            && self
                .radii
                .iter()
                .zip(radii_target)
                .all(|(s, t)| s.is_settled(t, epsilon, SETTLE_VEL_EPS))
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
    /// The user translucency setting the recipes are built with.
    pub translucency: Translucency,
    drag_activity: SpringState,
    frame: Binding<Group>,
}

impl Interactive {
    /// A group at rest.
    #[must_use]
    pub fn new(
        members: Vec<Member>,
        smoothing: f32,
        appearance: Appearance,
        translucency: Translucency,
    ) -> Self {
        let this = Self {
            members,
            smoothing,
            appearance,
            translucency,
            drag_activity: SpringState::at(0.0),
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
    /// spring-eased drag activity.
    #[must_use]
    pub fn effective_smoothing(&self) -> f32 {
        self.smoothing * DRAG_SMOOTHING_GAIN.mul_add(self.drag_activity.value.clamp(0.0, 1.0), 1.0)
    }

    /// Advances the dynamics by one 60 Hz frame (`1 / 60` s).
    pub fn step(&mut self) {
        self.step_dt(DEFAULT_FRAME_DT);
    }

    /// Advances the analytical spring dynamics by `dt` seconds.
    pub fn step_dt(&mut self, dt: f32) {
        let dragged = self.members.iter().position(Member::grabbed);
        let pulls = self.compute_pulls(dragged);
        for (m, pull) in self.members.iter_mut().zip(pulls) {
            m.step_dt(pull, dt);
        }
        let (activity_target, spring) = if dragged.is_some() {
            (1.0, Spring::JELLY_DRAG_ACTIVE)
        } else {
            (0.0, Spring::JELLY_DRAG_RELEASE)
        };
        self.drag_activity.step(spring, activity_target, dt);
        self.publish();
    }

    fn compute_pulls(&self, dragged: Option<usize>) -> Vec<[f32; 2]> {
        let shapes: Vec<Shape> = self.members.iter().map(Member::shape).collect();
        dragged.map_or_else(
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
        )
    }

    fn publish(&self) {
        let appearance = self.appearance;
        let translucency = self.translucency;
        let group = Group::container(
            self.members
                .iter()
                .map(|m| m.element(appearance, translucency))
                .collect(),
            self.effective_smoothing(),
        );
        self.frame.set(group);
    }

    /// Steps at 60 Hz until every spring-driven quantity settles within `epsilon`
    /// of its target or `max_frames` elapse; returns the frames taken.
    pub fn settle(&mut self, epsilon: f32, max_frames: usize) -> usize {
        self.settle_dt(DEFAULT_FRAME_DT, epsilon, max_frames)
    }

    /// Steps by `dt` seconds until every spring-driven quantity settles within
    /// `epsilon` of its target or `max_steps` elapse; returns the steps taken.
    pub fn settle_dt(&mut self, dt: f32, epsilon: f32, max_steps: usize) -> usize {
        let pos_eps = epsilon.max(SETTLE_POS_EPS);
        for step in 0..max_steps {
            let before = self.group();
            self.step_dt(dt);
            let after = self.group();
            let moved = before.members().iter().zip(after.members()).any(|(a, b)| {
                (a.shape.rect.x - b.shape.rect.x).abs() > epsilon
                    || (a.shape.rect.y - b.shape.rect.y).abs() > epsilon
                    || (a.shape.rect.w - b.shape.rect.w).abs() > epsilon
                    || (a.shape.rect.h - b.shape.rect.h).abs() > epsilon
                    || (a.shape.radii[0] - b.shape.radii[0]).abs() > epsilon
                    || (a.recipe.cast_density - b.recipe.cast_density).abs() > epsilon
            }) || (before.smoothing() - after.smoothing()).abs() > epsilon;
            if !moved {
                let dragged = self.members.iter().position(Member::grabbed);
                let pulls = self.compute_pulls(dragged);
                let activity_target = if dragged.is_some() { 1.0 } else { 0.0 };
                let settled =
                    self.drag_activity
                        .is_settled(activity_target, pos_eps, SETTLE_VEL_EPS)
                        && self
                            .members
                            .iter()
                            .zip(pulls)
                            .all(|(m, pull)| m.is_settled(pull, pos_eps));
                if settled {
                    return step + 1;
                }
            }
        }
        max_steps
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
        }
    }

    fn container() -> Interactive {
        Interactive::new(
            vec![
                Member::new(circle(0.0), Family::Regular),
                Member::new(circle(76.0), Family::Regular),
                Member::new(circle(152.0), Family::Regular),
            ],
            CONTAINER_UNION_SMOOTHING,
            Appearance::Dark,
            Translucency::default(),
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
    fn drag_steps_through_jelly_springs_and_neighbours_lean_in() {
        let mut g = container();
        g.members[1].controls.drag.set(Some([8.0, 0.0]));
        for _ in 0..10 {
            g.step();
        }
        // Analytical `JELLY_DRAG_ACTIVE` (T_r = 0.340, zeta = 0.600) crosses >95%
        // of the 8 pt pull within 10 frames (~167 ms) with mild underdamped overshoot.
        let off = g.members[1].offset()[0];
        assert!(
            off > 0.95 * 8.0 && off < 1.15 * 8.0,
            "offset after 10 frames: {off}"
        );
        assert!(g.members[1].tensor().parallel > 1.0);
        g.settle(1e-4, 400);
        let group = g.group();
        let dragged = &group.members()[1];
        assert!(
            (dragged.shape.rect.w - 60.0).abs() < 1e-4
                && (dragged.shape.rect.h - 60.0).abs() < 1e-4,
            "the interactive material keeps its size through a drag"
        );
        let left = g.members[0].offset();
        let right = g.members[2].offset();
        assert!(left[0] > 0.0, "left neighbour leans right: {left:?}");
        assert!(right[0] < 0.0, "right neighbour leans left: {right:?}");
        // The gap cap keeps neighbours from closing more than half the gap.
        let gap_right = 16.0 - 8.0; // the pull closed 8 of the 16 pt gap
        assert!(-right[0] <= 0.5 * (gap_right - 6.0) + 1e-3, "{right:?}");
        // Smoothing grows while grabbed.
        assert!(
            g.effective_smoothing()
                > CONTAINER_UNION_SMOOTHING * DRAG_SMOOTHING_GAIN.mul_add(0.85, 1.0),
            "{}",
            g.effective_smoothing()
        );
        // Release through `JELLY_DRAG_RELEASE` rebounds past zero before settling.
        g.members[1].controls.drag.set(None);
        let mut min_x = 0.0_f32;
        let mut min_par = 1.0_f32;
        for _ in 0..40 {
            g.step();
            min_x = min_x.min(g.members[1].offset()[0]);
            min_par = min_par.min(g.members[1].tensor().parallel);
        }
        assert!(
            min_x < -1.0,
            "release spring should overshoot past 0: {min_x}"
        );
        assert!(
            min_par < 1.0,
            "release strain should rebound into squash: {min_par}"
        );
        g.settle(1e-4, 400);
        for m in &g.members {
            assert!(m.offset()[0].abs() < 1e-2);
        }
        assert!((g.effective_smoothing() - CONTAINER_UNION_SMOOTHING).abs() < 1e-2);
    }

    #[test]
    fn eight_point_pull_bridges_the_sixteen_point_gap() {
        let mut g = container();
        g.members.push(Member::new(circle(400.0), Family::Regular));
        g.members[1].controls.drag.set(Some([8.0, 0.0]));
        g.settle(1e-4, 400);
        let group = g.group();
        let field = UnionField::new(group.members(), group.smoothing());
        // Midpoint of the (pulled) right gap.
        let right = group.members()[2].shape.rect;
        let mid = group.members()[1].shape.rect;
        let x = (mid.x + mid.w + right.x) * 0.5;
        assert!(
            field.sample([x, 30.0]).distance < 0.0,
            "gap bridged at x={x}"
        );
        // A member sitting farther away than the attraction radius keeps its
        // gap open.
        let far = group.members()[3].shape.rect;
        let x_far = (right.x + right.w + far.x) * 0.5;
        assert!(
            field.sample([x_far, 30.0]).distance > 0.0,
            "far gap open at x={x_far}"
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
                },
                Family::Regular,
            )],
            8.0,
            Appearance::Light,
            Translucency::default(),
        );
        g.members[0].controls.radii.set([12.0; 4]);
        let mut last = 40.0;
        for _ in 0..30 {
            g.step();
            let r = g.group().members()[0].shape.radii[0];
            assert!(r < last && r >= 12.0);
            last = r;
        }
        g.settle(1e-4, 200);
        assert!((g.group().members()[0].shape.radii[0] - 12.0).abs() < 1e-2);
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
        g.members[0].controls.drag.set(Some([8.0, 0.0]));
        g.step();
        assert!(frame.snapshot().members()[0].shape.rect.x > 0.0);
    }
}
