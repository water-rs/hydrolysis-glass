//! Clean-room spring physics, press scale, elastic drag deformation, and
//! glass lens / popover morphing (§§6, 7, 10, 11).

use core::f64::consts::TAU;

use crate::material::geometry::{CONTINUOUS_EXPONENT, CornerCurve, Rect, Shape};

/// Pointer travel from the press origin at which an active press cancels, pt.
pub const PRESS_CANCEL_DISTANCE: f32 = 70.0;

/// Press-scale gain for standard buttons (`s(p) = 1 + 0.245 * p`).
pub const BUTTON_PRESS_SCALE_GAIN: f32 = 0.245;

/// Maximum half-axis growth during button press, pt (`Delta_max = 10.0 pt`).
pub const PRESS_MAX_HALF_AXIS_GROWTH: f32 = 10.0;

/// Press-scale gain for toggle and slider lens thumbs (`1 + 0.35 * p`).
pub const THUMB_PRESS_SCALE_GAIN: f32 = 0.35;

/// Press-scale gain for segmented selection pills (`1 + 0.15 * p`).
pub const SEGMENTED_PRESS_SCALE_GAIN: f32 = 0.15;

/// Press-scale gain for tab bar selection pills (`1 + 0.12 * p`).
pub const TAB_BAR_PRESS_SCALE_GAIN: f32 = 0.12;

/// Target corner radius for a presented popover/menu panel, pt (`r_1 = 26.0 pt`).
pub const POPOVER_TARGET_RADIUS: f32 = 26.0;

/// Unit-mass damped harmonic oscillator (`y'' + c * y' + k * y = 0`),
/// parameterized by undamped natural response period `T_r` (seconds) and
/// damping ratio `zeta`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Spring {
    /// Undamped natural response period `T_r` in seconds (`omega_n = 2 * pi / T_r`).
    pub response: f32,
    /// Damping ratio `zeta` (`< 1` underdamped, `1` critically damped, `> 1` overdamped).
    pub damping_ratio: f32,
}

impl Spring {
    /// Button press-down spring (`T_r = 0.223 s`, `zeta = 0.650`).
    pub const BUTTON_PRESS: Self = Self::new(0.223, 0.650);

    /// Button release spring (`T_r = 0.398 s`, `zeta = 0.385`).
    pub const BUTTON_RELEASE: Self = Self::new(0.398, 0.385);

    /// Switch toggle knob spring (`T_r = 0.299 s`, `zeta = 1.000`).
    pub const SWITCH_KNOB: Self = Self::new(0.299, 1.000);

    /// Switch track color-on spring (`T_r = 0.561 s`, `zeta = 1.000`).
    pub const SWITCH_TRACK_ON: Self = Self::new(0.561, 1.000);

    /// Switch track color-off spring (`T_r = 0.311 s`, `zeta = 1.000`).
    pub const SWITCH_TRACK_OFF: Self = Self::new(0.311, 1.000);

    /// Slider external value spring (`T_r = 0.403 s`, `zeta = 0.790`).
    pub const SLIDER_VALUE: Self = Self::new(0.403, 0.790);

    /// Segmented selection pill spring (`T_r = 0.400 s`, `zeta = 0.850`).
    pub const SEGMENTED_PILL: Self = Self::new(0.400, 0.850);

    /// Active jelly drag deformation spring (`T_r = 0.340 s`, `zeta = 0.600`).
    pub const JELLY_DRAG_ACTIVE: Self = Self::new(0.340, 0.600);

    /// Jelly drag snap-back release spring (`T_r = 0.349 s`, `zeta = 0.450`,
    /// ~20% opposite-axis squash-rebound).
    pub const JELLY_DRAG_RELEASE: Self = Self::new(0.349, 0.450);

    /// Popover / menu presentation spring (`T_r = 0.360 s`, `zeta = 0.820`).
    pub const POPOVER_PRESENT: Self = Self::new(0.360, 0.820);

    /// Popover / menu dismissal spring (`T_r = 0.280 s`, `zeta = 0.900`).
    pub const POPOVER_DISMISS: Self = Self::new(0.280, 0.900);

    /// Constructs a unit-mass spring from its response period (seconds) and damping ratio.
    #[must_use]
    pub const fn new(response: f32, damping_ratio: f32) -> Self {
        Self {
            response,
            damping_ratio,
        }
    }

    /// Undamped angular frequency `omega_n = 2 * pi / T_r` (rad/s).
    #[must_use]
    pub fn angular_frequency(self) -> f32 {
        if self.response <= 0.0 || !self.response.is_finite() {
            return 0.0;
        }
        #[allow(
            clippy::cast_possible_truncation,
            reason = "oscillator parameters and public outputs are f32"
        )]
        {
            (TAU / f64::from(self.response)) as f32
        }
    }

    /// Unit-mass spring stiffness `k = omega_n^2`.
    #[must_use]
    pub fn stiffness(self) -> f32 {
        let omega = self.angular_frequency();
        omega * omega
    }

    /// Unit-mass damping coefficient `c = 2 * zeta * omega_n`.
    #[must_use]
    pub fn damping(self) -> f32 {
        2.0 * self.damping_ratio.max(0.0) * self.angular_frequency()
    }

    /// Evaluates the closed-form analytical solution `(value, velocity)` after
    /// advancing `dt` seconds toward a constant `target` from `(value, velocity)`.
    #[must_use]
    pub fn step_scalar(self, value: f32, velocity: f32, target: f32, dt: f32) -> (f32, f32) {
        if dt <= 0.0 || !dt.is_finite() {
            return (value, velocity);
        }
        if self.response <= 0.0 || !self.response.is_finite() {
            return (target, 0.0);
        }

        let omega_n = TAU / f64::from(self.response);
        let zeta = f64::from(self.damping_ratio.max(0.0));
        let dt_f = f64::from(dt);
        let target_f = f64::from(target);
        let y0 = f64::from(value) - target_f;
        let v0 = f64::from(velocity);

        let (y_next, v_next) = if zeta < 1.0 - 1e-6 {
            let omega_d = omega_n * zeta.mul_add(-zeta, 1.0).sqrt();
            let decay = (-zeta * omega_n * dt_f).exp();
            let c1 = y0;
            let c2 = (zeta * omega_n).mul_add(y0, v0) / omega_d;
            let (sin, cos) = (omega_d * dt_f).sin_cos();
            let y = decay * c1.mul_add(cos, c2 * sin);
            let v_cos = c2.mul_add(omega_d, -(c1 * zeta * omega_n));
            let v_sin = c1.mul_add(omega_d, c2 * zeta * omega_n);
            let v = decay * v_cos.mul_add(cos, -(v_sin * sin));
            (y, v)
        } else if (zeta - 1.0).abs() <= 1e-6 {
            let decay = (-omega_n * dt_f).exp();
            let c1 = y0;
            let c2 = omega_n.mul_add(y0, v0);
            let y = decay * c2.mul_add(dt_f, c1);
            let v = decay * omega_n.mul_add(-c2.mul_add(dt_f, c1), c2);
            (y, v)
        } else {
            let gamma = omega_n * zeta.mul_add(zeta, -1.0).sqrt();
            let r1 = (-omega_n).mul_add(zeta, gamma);
            let r2 = (-omega_n).mul_add(zeta, -gamma);
            let inv_2gamma = 0.5 / gamma;
            let c1 = r2.mul_add(-y0, v0) * inv_2gamma;
            let c2 = r1.mul_add(y0, -v0) * inv_2gamma;
            let e1 = (r1 * dt_f).exp();
            let e2 = (r2 * dt_f).exp();
            let y = c1.mul_add(e1, c2 * e2);
            let v = (c1 * r1).mul_add(e1, c2 * r2 * e2);
            (y, v)
        };

        #[allow(
            clippy::cast_possible_truncation,
            reason = "internal f64 integration precision converts back to f32 public state"
        )]
        ((target_f + y_next) as f32, v_next as f32)
    }
}

/// Scalar state driven by a [`Spring`].
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SpringState {
    /// Current scalar value.
    pub value: f32,
    /// Current scalar velocity (units per second).
    pub velocity: f32,
}

impl SpringState {
    /// Resting state initialized at `value` with zero velocity.
    #[must_use]
    pub const fn at(value: f32) -> Self {
        Self {
            value,
            velocity: 0.0,
        }
    }

    /// Advances the state toward `target` over `dt` seconds using `spring`.
    pub fn step(&mut self, spring: Spring, target: f32, dt: f32) -> f32 {
        let (value, velocity) = spring.step_scalar(self.value, self.velocity, target, dt);
        self.value = value;
        self.velocity = velocity;
        value
    }

    /// Whether both displacement from `target` and velocity lie within tolerances.
    #[must_use]
    pub fn is_settled(&self, target: f32, position_eps: f32, velocity_eps: f32) -> bool {
        (self.value - target).abs() <= position_eps && self.velocity.abs() <= velocity_eps
    }

    /// Snaps the state to rest at `target`.
    pub const fn snap(&mut self, target: f32) {
        self.value = target;
        self.velocity = 0.0;
    }
}

/// Two-dimensional state driven axis-by-axis by a [`Spring`].
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SpringState2 {
    /// X-axis state.
    pub x: SpringState,
    /// Y-axis state.
    pub y: SpringState,
}

impl SpringState2 {
    /// Resting 2D state initialized at `value` with zero velocity.
    #[must_use]
    pub const fn at(value: [f32; 2]) -> Self {
        Self {
            x: SpringState::at(value[0]),
            y: SpringState::at(value[1]),
        }
    }

    /// Current 2D position.
    #[must_use]
    pub const fn value(&self) -> [f32; 2] {
        [self.x.value, self.y.value]
    }

    /// Current 2D velocity (units per second).
    #[must_use]
    pub const fn velocity(&self) -> [f32; 2] {
        [self.x.velocity, self.y.velocity]
    }

    /// Advances both axes toward `target` over `dt` seconds using `spring`.
    pub fn step(&mut self, spring: Spring, target: [f32; 2], dt: f32) -> [f32; 2] {
        [
            self.x.step(spring, target[0], dt),
            self.y.step(spring, target[1], dt),
        ]
    }

    /// Whether both axes have settled within `position_eps` and `velocity_eps`.
    #[must_use]
    pub fn is_settled(&self, target: [f32; 2], position_eps: f32, velocity_eps: f32) -> bool {
        self.x.is_settled(target[0], position_eps, velocity_eps)
            && self.y.is_settled(target[1], position_eps, velocity_eps)
    }
}

/// Smooth cubic Hermite interpolation on `[edge0, edge1]`.
#[must_use]
pub fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    if edge1 <= edge0 {
        return if x >= edge1 { 1.0 } else { 0.0 };
    }
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * 2.0f32.mul_add(-t, 3.0)
}

/// Button press scale `[sx, sy]` implementing `s(p) = 1 + 0.245 * p`, capped
/// so neither half-axis expands by more than [`PRESS_MAX_HALF_AXIS_GROWTH`] (`10.0 pt`).
#[must_use]
pub fn press_scale(progress: f32, width: f32, height: f32) -> [f32; 2] {
    [
        axis_press_scale(progress, width, BUTTON_PRESS_SCALE_GAIN),
        axis_press_scale(progress, height, BUTTON_PRESS_SCALE_GAIN),
    ]
}

/// Press scale along one axis with custom `gain`, capping half-axis growth at
/// `10.0 pt` (`Delta_max = 10.0 pt`).
#[must_use]
pub fn axis_press_scale(progress: f32, extent: f32, gain: f32) -> f32 {
    let delta_ratio = gain * progress;
    if extent <= 0.0 || !extent.is_finite() {
        return (1.0 + delta_ratio).max(0.0);
    }
    let max_ratio = (2.0 * PRESS_MAX_HALF_AXIS_GROWTH) / extent;
    delta_ratio.min(max_ratio).mul_add(1.0, 1.0).max(0.0)
}

/// Uniform press scale for toggle and slider glass lens thumbs (`1 + 0.35 * p`).
#[must_use]
pub const fn thumb_press_scale(progress: f32) -> f32 {
    THUMB_PRESS_SCALE_GAIN.mul_add(progress, 1.0)
}

/// Uniform press scale for segmented control selection pills (`1 + 0.15 * p`).
#[must_use]
pub const fn segmented_press_scale(progress: f32) -> f32 {
    SEGMENTED_PRESS_SCALE_GAIN.mul_add(progress, 1.0)
}

/// Uniform press scale for tab bar selection pills (`1 + 0.12 * p`).
#[must_use]
pub const fn tab_bar_press_scale(progress: f32) -> f32 {
    TAB_BAR_PRESS_SCALE_GAIN.mul_add(progress, 1.0)
}

/// Touch-point wash alpha (`§7`):
/// `alpha_wash = 0.30 * max(p, 0) * (1 - 0.37 * min(||x - x_t|| / R_t, 1)^2)`,
/// with `R_t = max(0.5 * w_grown, 40.0 pt)`.
#[must_use]
pub fn touch_wash_alpha(progress: f32, distance: f32, grown_width: f32) -> f32 {
    let p = progress.max(0.0);
    if p <= 0.0 {
        return 0.0;
    }
    let radius = (0.5 * grown_width).max(40.0);
    let u = (distance.max(0.0) / radius).min(1.0);
    0.30 * p * 0.37f32.mul_add(-(u * u), 1.0)
}

/// Specular rim highlight press multiplier (`§7`):
/// `1 + 0.35 * max(p, 0) * (0.4 + 0.6 * exp(-d^2 / (2 * (0.35 * w_grown)^2)))`.
#[must_use]
pub fn specular_press_boost(progress: f32, distance: f32, grown_width: f32) -> f32 {
    let p = progress.max(0.0);
    if p <= 0.0 {
        return 1.0;
    }
    let sigma = (0.35 * grown_width).max(1e-3);
    let falloff = (-(distance * distance) / (2.0 * sigma * sigma)).exp();
    (0.35 * p).mul_add(0.6f32.mul_add(falloff, 0.4), 1.0)
}

/// Rubber-band drag translation lag (`§7`):
/// `t_target = u_hat * (||Delta|| * 24) / (||Delta|| + 24)` clamped to `<= 12 pt`.
#[must_use]
pub fn rubber_band_lag(delta: [f32; 2]) -> [f32; 2] {
    let norm = delta[0].hypot(delta[1]);
    if norm <= 1e-6 || !norm.is_finite() {
        return [0.0, 0.0];
    }
    let mag = ((norm * 24.0) / (norm + 24.0)).min(12.0);
    let scale = mag / norm;
    [delta[0] * scale, delta[1] * scale]
}

/// Target jelly stretch strain (`§7`):
/// `eps_target = min(0.16, 0.14 * tanh(||Delta|| / 48) + 0.12 * tanh(||v|| / 600))`.
#[must_use]
pub fn jelly_strain_target(delta: [f32; 2], velocity: [f32; 2]) -> f32 {
    let delta_norm = delta[0].hypot(delta[1]);
    let vel_norm = velocity[0].hypot(velocity[1]);
    0.14f32
        .mul_add((delta_norm / 48.0).tanh(), 0.12 * (vel_norm / 600.0).tanh())
        .min(0.16)
}

/// Area-preserving 2D jelly drag deformation tensor (`det = lambda_par * lambda_perp = 1`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct JellyTensor {
    /// Unit stretch direction vector.
    pub axis: [f32; 2],
    /// Stretch factor parallel to `axis`, clamped to `[0.86, 1.16]`.
    pub parallel: f32,
    /// Area-preserving perpendicular scale `1.0 / parallel`.
    pub perpendicular: f32,
    /// Rubber-band translation offset, pt.
    pub translation: [f32; 2],
}

impl Default for JellyTensor {
    fn default() -> Self {
        Self::IDENTITY
    }
}

impl JellyTensor {
    /// Undeformed identity tensor.
    pub const IDENTITY: Self = Self {
        axis: [1.0, 0.0],
        parallel: 1.0,
        perpendicular: 1.0,
        translation: [0.0, 0.0],
    };

    /// Constructs an area-preserving tensor along `direction` from signed `strain`
    /// (`lambda_par = clamp(1 + strain, 0.86, 1.16)`, `lambda_perp = 1 / lambda_par`)
    /// and `translation`.
    #[must_use]
    pub fn from_strain(direction: [f32; 2], strain: f32, translation: [f32; 2]) -> Self {
        let norm = direction[0].hypot(direction[1]);
        let axis = if norm > 1e-6 && norm.is_finite() {
            [direction[0] / norm, direction[1] / norm]
        } else {
            [1.0, 0.0]
        };
        let parallel = (1.0 + strain).clamp(0.86, 1.16);
        let perpendicular = 1.0 / parallel;
        Self {
            axis,
            parallel,
            perpendicular,
            translation,
        }
    }

    /// Evaluates the target deformation tensor from raw drag `delta` (pt) and
    /// pointer `velocity` (pt/s) per §7.
    #[must_use]
    pub fn from_drag(delta: [f32; 2], velocity: [f32; 2]) -> Self {
        let delta_norm = delta[0].hypot(delta[1]);
        let direction = if delta_norm > 1e-6 { delta } else { velocity };
        let strain = jelly_strain_target(delta, velocity);
        let translation = rubber_band_lag(delta);
        Self::from_strain(direction, strain, translation)
    }

    /// Determinant `lambda_par * lambda_perp` (always `1.0` up to float rounding).
    #[must_use]
    pub const fn determinant(self) -> f32 {
        self.parallel * self.perpendicular
    }

    /// Row-major `2x2` linear stretch matrix `lambda_par * u u^T + lambda_perp * (I - u u^T)`.
    #[must_use]
    pub fn matrix2(self) -> [[f32; 2]; 2] {
        let [ux, uy] = self.axis;
        let diff = self.parallel - self.perpendicular;
        let m00 = diff.mul_add(ux * ux, self.perpendicular);
        let m01 = diff * ux * uy;
        let m11 = diff.mul_add(uy * uy, self.perpendicular);
        [[m00, m01], [m01, m11]]
    }

    /// Transforms a point relative to the control's center (stretch then translate).
    #[must_use]
    pub fn transform_centered_point(self, p: [f32; 2]) -> [f32; 2] {
        let m = self.matrix2();
        [
            m[0][0].mul_add(p[0], m[0][1].mul_add(p[1], self.translation[0])),
            m[1][0].mul_add(p[0], m[1][1].mul_add(p[1], self.translation[1])),
        ]
    }
}

/// Opaque-to-refractive-lens morph blend factor (`§10`):
/// `alpha_lens(p) = smoothstep(0.05, 0.55, sat(p))`.
#[must_use]
pub fn lens_blend(progress: f32) -> f32 {
    smoothstep(0.05, 0.55, progress.clamp(0.0, 1.0))
}

/// Interpolates between `rest` and `clear` RGBA colors by [`lens_blend(progress)`](lens_blend).
#[must_use]
pub fn lens_color_mix(rest: [f32; 4], clear: [f32; 4], progress: f32) -> [f32; 4] {
    let t = lens_blend(progress);
    if t <= 0.0 {
        return rest;
    }
    if t >= 1.0 {
        return clear;
    }
    [
        (clear[0] - rest[0]).mul_add(t, rest[0]),
        (clear[1] - rest[1]).mul_add(t, rest[1]),
        (clear[2] - rest[2]).mul_add(t, rest[2]),
        (clear[3] - rest[3]).mul_add(t, rest[3]),
    ]
}

/// Continuous popover / menu morph sample (`§11`) between a trigger button and
/// a rounded glass menu panel.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PopoverMorphSample {
    /// Interpolated panel bounds, pt.
    pub rect: Rect,
    /// Interpolated corner radius, pt (`r(q) = mix(origin_radius, target_radius, q)`).
    pub corner_radius: f32,
    /// Minimum dimension `m(q) = min(w(q), h(q))` feeding Regular glass size laws.
    pub min_dimension: f32,
    /// Continuous corner superellipse exponent (`CONTINUOUS_EXPONENT = 2.02`).
    pub corner_exponent: f32,
    /// Trigger button glyph opacity (`1 - smoothstep(0.0, 0.25, q)`).
    pub trigger_glyph_opacity: f32,
    /// Menu content opacity (`smoothstep(0.25, 0.75, q)`).
    pub content_opacity: f32,
    /// Menu content scale (`0.92 + 0.08 * smoothstep(0.15, 0.85, q)`).
    pub content_scale: f32,
}

impl PopoverMorphSample {
    /// Continuous-corner [`Shape`] at this morph instant.
    #[must_use]
    pub const fn shape(self) -> Shape {
        Shape {
            rect: self.rect,
            radii: [self.corner_radius; 4],
            curve: CornerCurve::Continuous,
        }
    }
}

/// Continuous trigger-to-popover morph interpolator (`§11`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PopoverMorph;

impl PopoverMorph {
    /// Samples the popover morph at normalized progress `q` (driven by
    /// [`Spring::POPOVER_PRESENT`] / [`Spring::POPOVER_DISMISS`]).
    #[must_use]
    pub fn sample(
        q: f32,
        origin_rect: Rect,
        origin_radius: f32,
        target_rect: Rect,
        target_radius: f32,
    ) -> PopoverMorphSample {
        let rect = Rect {
            x: (target_rect.x - origin_rect.x).mul_add(q, origin_rect.x),
            y: (target_rect.y - origin_rect.y).mul_add(q, origin_rect.y),
            w: (target_rect.w - origin_rect.w)
                .mul_add(q, origin_rect.w)
                .max(0.0),
            h: (target_rect.h - origin_rect.h)
                .mul_add(q, origin_rect.h)
                .max(0.0),
        };
        let corner_radius = (target_radius - origin_radius)
            .mul_add(q, origin_radius)
            .max(0.0);
        let min_dimension = rect.w.min(rect.h);
        let trigger_glyph_opacity = 1.0 - smoothstep(0.0, 0.25, q);
        let content_opacity = smoothstep(0.25, 0.75, q);
        let content_scale = 0.08f32.mul_add(smoothstep(0.15, 0.85, q), 0.92);
        PopoverMorphSample {
            rect,
            corner_radius,
            min_dimension,
            corner_exponent: CONTINUOUS_EXPONENT,
            trigger_glyph_opacity,
            content_opacity,
            content_scale,
        }
    }
}
