//! Time-series motion, dt-invariance, deformation tensor, press-scale, and
//! morph invariant tests for clean-room glass dynamics.

use hydrolysis_glass::dynamics::{
    BUTTON_PRESS_SCALE_GAIN, JellyTensor, POPOVER_TARGET_RADIUS, PRESS_CANCEL_DISTANCE,
    PRESS_MAX_HALF_AXIS_GROWTH, PopoverMorph, SEGMENTED_PRESS_SCALE_GAIN, Spring, SpringState,
    SpringState2, TAB_BAR_PRESS_SCALE_GAIN, THUMB_PRESS_SCALE_GAIN, jelly_strain_target,
    lens_blend, lens_color_mix, press_scale, rubber_band_lag, segmented_press_scale,
    sheet_rubber_band_offset, specular_press_boost, tab_bar_press_scale, thumb_press_scale,
    touch_wash_alpha,
};
use hydrolysis_glass::interaction::{Interactive, Member};
use hydrolysis_glass::material::geometry::{CONTINUOUS_EXPONENT, CornerCurve, Rect, Shape};
use hydrolysis_glass::material::recipe::{Appearance, Family, Translucency};

fn step_series(spring: Spring, initial: f32, target: f32, dt: f32, steps: usize) -> Vec<f32> {
    let mut state = SpringState::at(initial);
    let mut out = Vec::with_capacity(steps);
    for _ in 0..steps {
        out.push(state.step(spring, target, dt));
    }
    out
}

#[test]
fn spring_presets_match_specification_coefficients() {
    assert!((Spring::BUTTON_PRESS.stiffness() - 793.87).abs() < 0.05);
    assert!((Spring::BUTTON_PRESS.stiffness() - 795.2).abs() < 1.5);
    assert!((Spring::BUTTON_PRESS.damping() - 36.63).abs() < 0.05);
    assert!((Spring::BUTTON_RELEASE.stiffness() - 249.21).abs() < 0.05);
    assert!((Spring::BUTTON_RELEASE.damping() - 12.16).abs() < 0.05);

    assert_eq!(Spring::SWITCH_KNOB.damping_ratio, 1.0);
    assert_eq!(Spring::SWITCH_TRACK_ON.damping_ratio, 1.0);
    assert_eq!(Spring::SWITCH_TRACK_OFF.damping_ratio, 1.0);
    assert_eq!(Spring::SLIDER_VALUE, Spring::new(0.403, 0.790));
    assert_eq!(Spring::SEGMENTED_PILL, Spring::new(0.400, 0.850));
    assert_eq!(Spring::JELLY_DRAG_ACTIVE, Spring::new(0.340, 0.600));
    assert_eq!(Spring::JELLY_DRAG_RELEASE, Spring::new(0.349, 0.450));
    assert_eq!(Spring::POPOVER_PRESENT, Spring::new(0.286, 0.760));
    assert_eq!(Spring::POPOVER_PRESENT_VERTICAL, Spring::new(0.311, 0.333));
    assert_eq!(Spring::POPOVER_DISMISS, Spring::new(0.624, 0.619));
    assert_eq!(Spring::POPOVER_DISMISS_VERTICAL, Spring::new(0.807, 0.756));
    assert_eq!(Spring::POPOVER_RADIUS, Spring::new(0.311, 0.750));
    assert_eq!(Spring::SHEET_PRESENT, Spring::new(0.400, 0.850));
    assert_eq!(Spring::SHEET_DISMISS, Spring::new(0.350, 1.000));
    assert_eq!(Spring::SHEET_DETENT, Spring::new(0.380, 0.850));

    // §8.10 / §9.3 sheet spring stiffness and damping.
    assert!((Spring::SHEET_PRESENT.stiffness() - 246.74).abs() < 0.5);
    assert!((Spring::SHEET_PRESENT.damping() - 26.70).abs() < 0.2);
    assert!((Spring::SHEET_DISMISS.stiffness() - 322.27).abs() < 0.5);
    assert!((Spring::SHEET_DISMISS.damping() - 35.90).abs() < 0.2);
    assert!((Spring::SHEET_DETENT.stiffness() - 273.40).abs() < 0.5);
    assert!((Spring::SHEET_DETENT.damping() - 28.11).abs() < 0.2);
}

#[test]
fn sheet_presentation_dismissal_and_rubber_band_match_specification() {
    // §8.10: `SHEET_PRESENT` (`T_r = 0.400`, `zeta = 0.850`) overshoots by ~0.6%.
    let present = step_series(Spring::SHEET_PRESENT, 0.0, 1.0, 0.001, 800);
    let peak = present.iter().copied().fold(0.0_f32, f32::max);
    assert!(
        (peak - 1.0063).abs() < 0.003,
        "expected ~0.6% overshoot on SHEET_PRESENT, got {peak}"
    );
    assert!((*present.last().expect("non-empty") - 1.0).abs() < 1e-3);

    // §8.10: `SHEET_DISMISS` (`T_r = 0.350`, `zeta = 1.000`) exits critically damped with zero overshoot.
    let dismiss = step_series(Spring::SHEET_DISMISS, 1.0, 0.0, 0.001, 800);
    let mut prev = 1.0_f32;
    for &v in &dismiss {
        assert!(v <= prev + 1e-6, "sheet dismiss bounced: {prev} -> {v}");
        assert!(v >= -1e-6, "sheet dismiss overshot 0: {v}");
        prev = v;
    }
    assert!(dismiss.last().expect("non-empty").abs() < 1e-3);

    // §8.10: rubber-band resistance `12 * delta / (delta + 24)` pt past max detent.
    assert_eq!(sheet_rubber_band_offset(-5.0), 0.0);
    assert_eq!(sheet_rubber_band_offset(0.0), 0.0);
    assert!((sheet_rubber_band_offset(24.0) - 6.0).abs() < 1e-6);
    assert!(sheet_rubber_band_offset(10_000.0) < 12.0);
    assert!((THUMB_PRESS_SCALE_GAIN - 0.60).abs() < 1e-6);
}

#[test]
fn analytical_spring_overshoot_and_critical_damping_invariants() {
    // `JELLY_DRAG_RELEASE` (zeta = 0.450): theoretical step overshoot
    // exp(-pi * 0.45 / sqrt(1 - 0.45^2)) ≈ 0.205 (~20%).
    let release = step_series(Spring::JELLY_DRAG_RELEASE, 1.0, 0.0, 0.001, 600);
    let min_val = release.iter().copied().fold(f32::INFINITY, f32::min);
    let overshoot = -min_val;
    assert!(
        (overshoot - 0.205).abs() < 0.015,
        "expected ~20.5% overshoot on JELLY_DRAG_RELEASE, got {overshoot}"
    );
    assert!(release.last().expect("non-empty").abs() < 1e-2);

    // `SWITCH_KNOB` (zeta = 1.000): critically damped monotone approach, zero overshoot.
    let knob = step_series(Spring::SWITCH_KNOB, 0.0, 1.0, 0.001, 800);
    let mut prev = 0.0_f32;
    for &v in &knob {
        assert!(v >= prev - 1e-6, "critical damping dipped: {prev} -> {v}");
        assert!(v <= 1.0 + 1e-6, "critical damping overshot 1.0: {v}");
        prev = v;
    }
    assert!((*knob.last().expect("non-empty") - 1.0).abs() < 1e-3);

    // Overdamped oscillator also converges monotonically from rest without overshoot.
    let overdamped = step_series(Spring::new(0.30, 1.35), 0.0, 1.0, 0.002, 600);
    let mut prev_od = 0.0_f32;
    for &v in &overdamped {
        assert!(v >= prev_od - 1e-6 && v <= 1.0 + 1e-6);
        prev_od = v;
    }
}

#[test]
fn analytical_spring_is_dt_invariant_across_refresh_rates() {
    let cases = [
        Spring::BUTTON_PRESS,
        Spring::BUTTON_RELEASE,
        Spring::SWITCH_KNOB,
        Spring::JELLY_DRAG_ACTIVE,
        Spring::JELLY_DRAG_RELEASE,
        Spring::new(0.32, 1.40),
    ];
    for spring in cases {
        // 1 step of 16 ms vs 16 steps of 1 ms.
        let mut coarse = SpringState {
            value: 0.25,
            velocity: 1.75,
        };
        let (v_coarse, vel_coarse) = (coarse.step(spring, 1.0, 0.016), coarse.velocity);

        let mut fine = SpringState {
            value: 0.25,
            velocity: 1.75,
        };
        for _ in 0..16 {
            fine.step(spring, 1.0, 0.001);
        }
        assert!(
            (v_coarse - fine.value).abs() < 1e-5 && (vel_coarse - fine.velocity).abs() < 1e-4,
            "16ms vs 16x1ms mismatch for {spring:?}: ({v_coarse}, {vel_coarse}) vs ({}, {})",
            fine.value,
            fine.velocity
        );

        // 60 Hz (6 steps) vs 120 Hz (12 steps) vs 240 Hz (24 steps) over 0.1 s.
        let mut s60 = SpringState::at(0.0);
        let mut s120 = SpringState::at(0.0);
        let mut s240 = SpringState::at(0.0);
        for _ in 0..6 {
            s60.step(spring, 10.0, 1.0 / 60.0);
        }
        for _ in 0..12 {
            s120.step(spring, 10.0, 1.0 / 120.0);
        }
        for _ in 0..24 {
            s240.step(spring, 10.0, 1.0 / 240.0);
        }
        assert!((s60.value - s120.value).abs() < 1e-4);
        assert!((s120.value - s240.value).abs() < 1e-4);
        assert!((s60.velocity - s240.velocity).abs() < 1e-3);
    }
}

#[test]
fn press_scale_caps_half_axis_growth_and_matches_presets() {
    assert!((PRESS_CANCEL_DISTANCE - 70.0).abs() < 1e-6);
    assert!((PRESS_MAX_HALF_AXIS_GROWTH - 10.0).abs() < 1e-6);

    // Compact 44x44 capsule grows by full 1 + 0.245 = 1.245 on both axes (< 10 pt cap).
    let compact = press_scale(1.0, 44.0, 44.0);
    assert!((compact[0] - (1.0 + BUTTON_PRESS_SCALE_GAIN)).abs() < 1e-6);
    assert!((compact[1] - (1.0 + BUTTON_PRESS_SCALE_GAIN)).abs() < 1e-6);

    // Wide 240x44 pill caps horizontal half-axis growth at exactly 10.0 pt while
    // vertical half-axis grows by full 24.5% (5.39 pt < 10 pt).
    let wide = press_scale(1.0, 240.0, 44.0);
    let half_grow_x = 0.5 * 240.0 * (wide[0] - 1.0);
    let half_grow_y = 0.5 * 44.0 * (wide[1] - 1.0);
    let expected_y = 0.5 * 44.0 * 0.245;
    assert!((half_grow_x - 10.0).abs() < 1e-5);
    assert!((half_grow_y - expected_y).abs() < 1e-5);

    // Huge 1000x200 surface still caps both half-axes at <= 10.0 pt.
    let huge = press_scale(1.0, 1000.0, 200.0);
    let huge_half_x = 0.5 * 1000.0 * (huge[0] - 1.0);
    let huge_half_y = 0.5 * 200.0 * (huge[1] - 1.0);
    assert!((huge_half_x - 10.0).abs() < 1e-4);
    assert!((huge_half_y - 10.0).abs() < 1e-4);

    assert!((thumb_press_scale(1.0) - (1.0 + THUMB_PRESS_SCALE_GAIN)).abs() < 1e-6);
    assert!((segmented_press_scale(1.0) - (1.0 + SEGMENTED_PRESS_SCALE_GAIN)).abs() < 1e-6);
    assert!((tab_bar_press_scale(1.0) - (1.0 + TAB_BAR_PRESS_SCALE_GAIN)).abs() < 1e-6);

    // Touch wash & specular press boost peak at touch origin (distance = 0) and decay outward.
    assert!(touch_wash_alpha(0.0, 0.0, 100.0).abs() < 1e-6);
    let wash_center = touch_wash_alpha(1.0, 0.0, 100.0);
    let wash_edge = touch_wash_alpha(1.0, 50.0, 100.0);
    let expected_wash_edge = 0.30 * (1.0 - 0.37);
    assert!((wash_center - 0.30).abs() < 1e-6);
    assert!((wash_edge - expected_wash_edge).abs() < 1e-5);
    assert!((specular_press_boost(0.0, 0.0, 100.0) - 1.0).abs() < 1e-6);
    assert!(specular_press_boost(1.0, 0.0, 100.0) > specular_press_boost(1.0, 60.0, 100.0));
}

#[test]
fn jelly_drag_lag_and_area_preserving_tensor_invariants() {
    // Rubber-band lag stays strictly <= 12.0 pt and matches d*24/(d+24) below saturation.
    let lag_small = rubber_band_lag([12.0, 0.0]);
    assert!((lag_small[0] - 8.0).abs() < 1e-5);
    assert!(lag_small[1].abs() < 1e-6);

    let lag_large = rubber_band_lag([300.0, 400.0]);
    assert!((lag_large[0].hypot(lag_large[1]) - 12.0).abs() < 1e-5);

    // Area preservation lambda_par * lambda_perp == 1.0 across strains and directions.
    for &(dir, strain) in &[
        ([1.0, 0.0], 0.16),
        ([3.0, 4.0], 0.10),
        ([-2.0, 5.0], -0.25),
        ([0.0, -1.0], 0.50),
    ] {
        let tensor = JellyTensor::from_strain(dir, strain, [4.0, -2.0]);
        assert!((0.86..=1.16).contains(&tensor.parallel));
        assert!((tensor.determinant() - 1.0).abs() < 1e-6);
        let m = tensor.matrix2();
        let mat_det = m[0][1].mul_add(-m[1][0], m[0][0] * m[1][1]);
        assert!(
            (mat_det - 1.0).abs() < 1e-5,
            "2x2 matrix determinant drifted from 1.0: {mat_det}"
        );
    }

    let dragged = JellyTensor::from_drag([200.0, 200.0], [1200.0, 1200.0]);
    assert!((dragged.parallel - 1.16).abs() < 1e-6);
    assert!((dragged.determinant() - 1.0).abs() < 1e-6);
    assert!((dragged.translation[0].hypot(dragged.translation[1]) - 12.0).abs() < 1e-5);
    assert!((jelly_strain_target([0.0, 0.0], [0.0, 0.0])).abs() < 1e-6);
}

#[test]
fn lens_morph_and_popover_morph_boundary_and_midpoint_invariants() {
    assert!((lens_blend(-0.2)).abs() < 1e-6);
    assert!((lens_blend(0.05)).abs() < 1e-6);
    assert!((lens_blend(0.30) - 0.5).abs() < 1e-6);
    assert!((lens_blend(0.55) - 1.0).abs() < 1e-6);
    assert!((lens_blend(1.2) - 1.0).abs() < 1e-6);

    let rest = [1.0, 1.0, 1.0, 1.0];
    let clear = [0.2, 0.4, 0.6, 0.25];
    assert_eq!(lens_color_mix(rest, clear, 0.0), rest);
    assert_eq!(lens_color_mix(rest, clear, 1.0), clear);
    let mid_mix = lens_color_mix(rest, clear, 0.30);
    for ((got, r), c) in mid_mix.iter().zip(rest).zip(clear) {
        assert!((*got - f32::midpoint(r, c)).abs() < 1e-6);
    }

    let origin_rect = Rect::new(280.0, 20.0, 44.0, 44.0);
    let target_rect = Rect::new(120.0, 72.0, 220.0, 300.0);
    let start = PopoverMorph::sample(0.0, origin_rect, 22.0, target_rect, POPOVER_TARGET_RADIUS);
    assert_eq!(start.rect, origin_rect);
    assert!((start.corner_radius - 22.0).abs() < 1e-6);
    assert!((start.min_dimension - 44.0).abs() < 1e-6);
    assert!((start.corner_exponent - CONTINUOUS_EXPONENT).abs() < 1e-6);
    assert!((start.trigger_glyph_opacity - 1.0).abs() < 1e-6);
    assert!(start.content_opacity.abs() < 1e-6);
    assert!((start.content_scale - 0.92).abs() < 1e-6);

    let mid = PopoverMorph::sample(0.5, origin_rect, 22.0, target_rect, POPOVER_TARGET_RADIUS);
    assert!((mid.rect.x - 200.0).abs() < 1e-5);
    assert!((mid.rect.y - 46.0).abs() < 1e-5);
    assert!((mid.rect.w - 132.0).abs() < 1e-5);
    assert!((mid.rect.h - 172.0).abs() < 1e-5);
    assert!((mid.corner_radius - 24.0).abs() < 1e-6);
    assert!((mid.min_dimension - 132.0).abs() < 1e-5);
    assert!(mid.trigger_glyph_opacity.abs() < 1e-6);
    assert!((mid.content_opacity - 0.5).abs() < 1e-6);
    assert!((mid.content_scale - 0.96).abs() < 1e-5);

    let end = PopoverMorph::sample(1.0, origin_rect, 22.0, target_rect, POPOVER_TARGET_RADIUS);
    assert_eq!(end.rect, target_rect);
    assert!((end.corner_radius - 26.0).abs() < 1e-6);
    assert!((end.min_dimension - 220.0).abs() < 1e-6);
    assert!(end.trigger_glyph_opacity.abs() < 1e-6);
    assert!((end.content_opacity - 1.0).abs() < 1e-6);
    assert!((end.content_scale - 1.0).abs() < 1e-6);
    assert_eq!(end.shape().curve, CornerCurve::Continuous);
}

#[test]
fn interactive_group_and_spring_state2_step_consistently() {
    let shape = Shape {
        rect: Rect::new(0.0, 0.0, 60.0, 60.0),
        radii: [30.0; 4],
        curve: CornerCurve::Continuous,
    };
    let mut a = Interactive::new(
        vec![Member::new(shape, Family::Regular)],
        8.0,
        Appearance::Light,
        Translucency::default(),
    );
    let mut b = Interactive::new(
        vec![Member::new(shape, Family::Regular)],
        8.0,
        Appearance::Light,
        Translucency::default(),
    );
    a.members[0].controls.drag.set(Some([10.0, -4.0]));
    b.members[0].controls.drag.set(Some([10.0, -4.0]));

    // 6 steps at 60 Hz vs 12 steps at 120 Hz.
    for _ in 0..6 {
        a.step();
    }
    for _ in 0..12 {
        b.step_dt(1.0 / 120.0);
    }
    assert!((a.members[0].offset()[0] - b.members[0].offset()[0]).abs() < 1e-4);
    assert!((a.members[0].offset()[1] - b.members[0].offset()[1]).abs() < 1e-4);
    assert!((a.members[0].tensor().determinant() - 1.0).abs() < 1e-6);

    let mut s2 = SpringState2::at([0.0, 0.0]);
    let pos = s2.step(Spring::JELLY_DRAG_ACTIVE, [10.0, -4.0], 0.1);
    assert!((pos[0] - a.members[0].offset()[0]).abs() < 1e-4);
    assert!((pos[1] - a.members[0].offset()[1]).abs() < 1e-4);
    assert!(!s2.is_settled([10.0, -4.0], 1e-4, 1e-3));
}
