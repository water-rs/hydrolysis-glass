//! The radio group look: an outline ring that turns accent and an accent
//! dot that grows in as the item is selected, under a circular state layer
//! and focus ring.

use cherenkov_record::Draw as _;
use cherenkov_record::kurbo::{Circle, Size, Stroke};
use hydrolysis::theme::{
    Custom, Driven, Inputs, Motion, PickerSpec, RadioGroupLook, RadioLayout, ToggleControl,
    ValueMotion,
};
use waterui::animation::Animation;
use waterui_core::Environment;

use super::super::chrome::{FOCUS_RING_GAP, FOCUS_RING_WIDTH};
use super::super::palette::{Rgba, alpha};
use super::super::{Glass, PRESS_DURATION};
use super::{control_motion, focus_ring_paint, hit_region, paint, state_layer_opacity};

/// The indicator's side, pt.
const INDICATOR: f64 = 22.0;
/// The gap between an indicator and its row's label, pt.
const LABEL_SPACING: f64 = 10.0;
/// The ring's stroke width, pt; the ring lies inside the indicator.
const RING_WIDTH: f64 = 2.0;
/// The selected dot's radius, in shares of the indicator's side.
const DOT: f64 = 0.25;
/// The state layer's radius, in multiples of the indicator's radius.
const STATE_LAYER_SCALE: f64 = 1.6;
/// How far the state layer and the focus ring around it reach past the
/// indicator, pt.
const REACH: f64 =
    (INDICATOR * 0.5).mul_add(STATE_LAYER_SCALE - 1.0, FOCUS_RING_GAP) + FOCUS_RING_WIDTH;

/// The measures of the group: at least 120 × 36 pt, its rows inset 12 by 8
/// and 12 pt apart, 8 pt below its heading.
const LAYOUT: RadioLayout = RadioLayout {
    min_width: 120.0,
    min_height: 36.0,
    horizontal_inset: 12.0,
    vertical_inset: 8.0,
    label_spacing: 8.0,
    row_spacing: 12.0,
};

/// `a` moved `t` of the way to `b`, channel by channel in encoded space.
fn mix(a: Rgba, b: Rgba, t: f32) -> Rgba {
    [
        (b[0] - a[0]).mul_add(t, a[0]),
        (b[1] - a[1]).mul_add(t, a[1]),
        (b[2] - a[2]).mul_add(t, a[2]),
        (b[3] - a[3]).mul_add(t, a[3]),
    ]
}

/// 1 for the selected item, 0 for the others.
fn selected(inputs: &Inputs<'_, bool>) -> f32 {
    f32::from(u8::from(*inputs.value))
}

/// The look of a radio group in `spec`.
///
/// A selection change eases the ring's colour, the dot's size and its
/// opacity with the press response's ease, however the change was made;
/// the rows' labels keep their own styling.
pub(in super::super) fn look(
    glass: &Glass,
    _spec: &PickerSpec,
    _env: &Environment,
) -> RadioGroupLook {
    let palette = glass.palette;
    let ring = Driven::new(move |inputs: &Inputs<'_, bool>| {
        paint(mix(palette.border, palette.accent, selected(inputs)))
    });
    let dot = Driven::new(|inputs: &Inputs<'_, bool>| {
        Circle::new(
            inputs.frame.center(),
            inputs.frame.width() * DOT * f64::from(selected(inputs)),
        )
    });
    let dot_paint = Driven::new(move |inputs: &Inputs<'_, bool>| {
        paint(alpha(palette.accent, selected(inputs)))
    });
    let layer = Driven::new(move |inputs: &Inputs<'_, bool>| {
        paint(alpha(
            palette.state_layer,
            state_layer_opacity(inputs.state),
        ))
    });
    let indicator = Custom::<bool, _>::new(
        (
            ring,
            dot,
            dot_paint,
            layer,
            focus_ring_paint(palette.accent),
        ),
        |recorder, frame, (ring, dot, dot_paint, layer, focus)| {
            let center = frame.center();
            let radius = frame.width().min(frame.height()) * 0.5;
            recorder.stroke(
                Circle::new(center, RING_WIDTH.mul_add(-0.5, radius)),
                Stroke::new(RING_WIDTH),
                ring,
            );
            recorder.fill(dot, dot_paint);
            let layer_radius = radius * STATE_LAYER_SCALE;
            recorder.fill(Circle::new(center, layer_radius), layer);
            recorder.stroke(
                Circle::new(
                    center,
                    FOCUS_RING_WIDTH.mul_add(0.5, layer_radius + FOCUS_RING_GAP),
                ),
                Stroke::new(FOCUS_RING_WIDTH),
                focus,
            );
        },
    );
    let motion = Motion {
        value: ValueMotion::Always(Animation::ease_out(PRESS_DURATION)),
        ..control_motion()
    };
    RadioGroupLook::new(
        ToggleControl::new(Size::new(INDICATOR, INDICATOR), motion)
            .label_spacing(LABEL_SPACING)
            .reach(REACH)
            .hit(hit_region())
            .part(indicator),
        LAYOUT,
    )
}
