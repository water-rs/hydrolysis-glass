//! The glass widget looks: what the theme returns for a button, a switch,
//! a checkbox, a stepper, a slider and a progress indicator, built from
//! Hydrolysis's look vocabulary (`hydrolysis::theme`).
//!
//! A look is data: every value that follows the widget's interaction state,
//! value, drag or frame is a [`Driven`](hydrolysis::theme::Driven) the engine
//! animates, so a press, a hover or a value change records nothing. The
//! glass body of a button and the held knob of a switch are
//! [`Backdrop`](hydrolysis::theme::Backdrop) fills over the shader and the
//! capture classes [`materials::register`](super::materials::register)
//! declares.

mod background;
mod button;
mod progress;
mod slider;
mod stepper;
mod toggle;

pub(super) use background::look as glass_background;
pub(super) use button::look as button;
pub(super) use progress::{circular as circular_progress, linear as linear_progress, loading};
pub(super) use slider::look as slider;
pub(super) use stepper::look as stepper;
pub(super) use toggle::{checkbox, switch};

use cherenkov_record::kurbo::Size;
use cherenkov_record::{Paint, WorkingColor};
use hydrolysis::theme::{HitRegion, Motion, Transition, ValueMotion};
use waterui::animation::Animation;
use waterui::color::working::to_srgb;
use waterui::interaction::InteractionState;

use super::button::{CANCEL_DISTANCE, MIN_HIT_SIDE, Spring};
use super::chrome::{HOVER_ALPHA, PRESSED_ALPHA};
use super::palette::{Rgba, alpha, working};
use super::{DRAG_DURATION, PRESS_DURATION};

/// `color` as a paint.
fn paint(color: Rgba) -> Paint {
    Paint::from(working(color))
}

/// The encoded sRGB colour and alpha of a working colour, the space the
/// palette's compositing arithmetic is stated in.
fn rgba_of(color: WorkingColor) -> Rgba {
    let encoded = to_srgb(color);
    [
        encoded.red,
        encoded.green,
        encoded.blue,
        color.components[3],
    ]
}

/// Whether `state` is disabled.
const fn is_disabled(state: InteractionState) -> bool {
    state.contains(InteractionState::DISABLED)
}

/// `color` at `opacity` of its alpha, the colour kept so that fading it
/// never runs through another hue.
fn faded(color: Rgba, opacity: f32) -> Rgba {
    alpha(color, color[3] * opacity)
}

/// The opacity of a state layer: the dragged or pressed state outranks
/// focus, which outranks hover; a disabled control has none.
const fn state_layer_opacity(state: InteractionState) -> f32 {
    if is_disabled(state) {
        0.0
    } else if state.intersects(InteractionState::DRAGGED.union(InteractionState::PRESSED)) {
        PRESSED_ALPHA
    } else if state.intersects(InteractionState::FOCUSED.union(InteractionState::HOVERED)) {
        HOVER_ALPHA
    } else {
        0.0
    }
}

/// A control's touch target: at least 44 pt each way, taking a release
/// within the cancel distance as a tap (§8.1).
fn hit_region() -> HitRegion {
    HitRegion::NONE
        .minimum(Size::new(f64::from(MIN_HIT_SIDE), f64::from(MIN_HIT_SIDE)))
        .cancel_distance(f64::from(CANCEL_DISTANCE))
}

/// The motion of the controls that animate their state layers: an eased
/// hover and focus, the press and release springs of §8.1, an eased drag
/// and a value that follows the change that moved it, or snaps.
fn control_motion() -> Motion {
    let eased = Transition::symmetric(Animation::ease_out(PRESS_DURATION));
    Motion {
        hover: eased.clone(),
        focus: eased,
        press: Transition::new(Spring::PRESS.physical(), Spring::RELEASE.physical()),
        drag: Transition::symmetric(Animation::ease_out(DRAG_DURATION)),
        value: ValueMotion::Follow(Animation::linear(core::time::Duration::ZERO)),
    }
}
