//! The glass widget looks: what the theme returns for a button, a switch,
//! a checkbox, a stepper, a slider, a segmented control, a progress
//! indicator, a text field, a menu, a menu picker, a radio group, the
//! navigation bar and a tab bar, built from Hydrolysis's look vocabulary
//! (`hydrolysis_style::theme`).
//!
//! A look is data: every value that follows the widget's interaction state,
//! value, drag or frame is a [`Driven`](hydrolysis_style::theme::Driven) the engine
//! animates, so a press, a hover or a value change records nothing. The
//! glass body of a button and the held knob of a switch are
//! [`Backdrop`](hydrolysis_style::theme::Backdrop) fills over the shader and the
//! capture classes [`materials::register`](super::materials::register)
//! declares.

mod background;
mod button;
mod card;
mod menu;
mod menu_picker;
mod navigation;
mod progress;
mod radio;
mod segmented;
mod slider;
mod snackbar;
mod stepper;
mod tabs;
mod text_field;
mod toggle;

pub(super) use background::look as glass_background;
pub(super) use button::look as button;
pub(super) use card::look as card;
pub(super) use menu::look as menu;
pub(super) use menu_picker::look as menu_picker;
pub(super) use navigation::look as navigation_bar;
pub(super) use progress::{circular as circular_progress, linear as linear_progress, loading};
pub(super) use radio::look as radio_group;
pub(super) use segmented::look as segmented_picker;
pub(super) use slider::look as slider;
pub(super) use snackbar::look as snackbar;
pub(super) use stepper::look as stepper;
pub(super) use tabs::look as tabs;
pub(super) use text_field::look as text_field;
pub(super) use toggle::{checkbox, switch};

use cherenkov_record::kurbo::{Line, Point, Rect, Size, Vec2};
use cherenkov_record::{Draw as _, Live, Paint, ShapeData, Stroke, WorkingColor};
use hydrolysis_style::theme::{
    BadgeLook, BadgeMetrics, Chrome, CornerShape, Custom, Driven, FocusIndicator, HitRegion,
    Inputs, ListControls, ListDividerMetrics, ListGestures, ListLook, ListMetrics, ListRowMetrics,
    ListSectionMetrics, ListTrailingControlMetrics, Motion, PressFeedback, ScrollIndicatorLook,
    StateLayer, Surface as ChromeSurface, TableLook, TableMetrics, Transition, ValueMotion,
};
use waterui::animation::Animation;
use waterui::interaction::InteractionState;
use waterui::text::font::{Font, Footnote};

use super::Glass;
use super::button::{CANCEL_DISTANCE, MIN_HIT_SIDE, Spring};
use super::chrome::{self, FOCUS_RING_GAP, FOCUS_RING_WIDTH, HOVER_ALPHA, PRESSED_ALPHA};
use super::palette::{Rgba, alpha, working};
use super::{DRAG_DURATION, PRESS_DURATION};

/// Default opacity applied to disabled button labels over the background.
const DISABLED_CONTENT_ALPHA: f32 = 0.38;

/// `color` as a paint.
fn paint(color: Rgba) -> Paint {
    Paint::from(working(color))
}

/// Encodes a linear Display P3 component with the Display P3 transfer curve
/// without converting gamut primaries.
fn encode_display_p3(c: f32) -> f32 {
    if c <= 0.0 {
        0.0
    } else if c <= 0.003_130_8 {
        12.92 * c
    } else {
        1.055f32.mul_add(c.powf(1.0 / 2.4), -0.055)
    }
}

/// The Display P3-encoded colour and alpha of a working colour, the space
/// the palette's compositing arithmetic is stated in.
fn rgba_of(color: WorkingColor) -> Rgba {
    let [r, g, b, a] = color.components;
    [
        encode_display_p3(r),
        encode_display_p3(g),
        encode_display_p3(b),
        a,
    ]
}

/// Whether `state` is disabled.
const fn is_disabled(state: InteractionState) -> bool {
    state.contains(InteractionState::DISABLED)
}

/// Whether a touch holds a control's knob or thumb: the lens's `p` at 1.
const fn held(state: InteractionState) -> bool {
    state.intersects(InteractionState::PRESSED.union(InteractionState::DRAGGED))
        && !is_disabled(state)
}

/// The paint of a focus ring in `accent`: opaque while the control has
/// keyboard focus and is enabled, clear otherwise.
fn focus_ring_paint<V: 'static>(accent: Rgba) -> Driven<Paint, V> {
    Driven::new(move |inputs: &Inputs<'_, V>| {
        let focused =
            inputs.state.contains(InteractionState::FOCUSED) && !is_disabled(inputs.state);
        paint(alpha(accent, f32::from(u8::from(focused))))
    })
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

/// The chrome of a control drawn as `surface`: the theme's state layer over
/// it, which a press tints, and an accent focus ring around it.
fn state_chrome(glass: &Glass, surface: ChromeSurface) -> Chrome {
    let palette = glass.palette;
    Chrome::new(
        surface,
        StateLayer::new(Glass::color(palette.state_layer)).opacities(
            HOVER_ALPHA,
            HOVER_ALPHA,
            PRESSED_ALPHA,
            PRESSED_ALPHA,
        ),
        PressFeedback::new().tint(),
        FocusIndicator::Ring {
            width: FOCUS_RING_WIDTH,
            offset: FOCUS_RING_GAP,
            color: Glass::color(palette.accent),
        },
        control_motion(),
    )
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

/// The look of a scroll indicator thumb.
pub(super) fn scroll_indicator(glass: &Glass) -> ScrollIndicatorLook {
    let palette = glass.palette;
    ScrollIndicatorLook::new(
        2.5,
        6.5,
        Custom::<(), _>::new((), move |recorder, frame, ()| {
            chrome::scroll_indicator(&palette, recorder, frame);
        }),
    )
    .edge_inset(1.5)
    .min_extent(12.0)
    .gutter(12.0)
}

/// The look of a badge indicator.
pub(super) fn badge(glass: &Glass) -> BadgeLook {
    let palette = glass.palette;
    BadgeLook::new(
        BadgeMetrics::new(8.0, 18.0, 6.0, 4.0, 4.0, 9.0, 9.0),
        Font::from(Footnote),
        Glass::color(palette.error_foreground),
        Custom::<(), _>::new((), move |recorder, frame, ()| {
            chrome::badge_small(&palette, recorder, frame);
        }),
        Custom::<(), _>::new((), move |recorder, frame, ()| {
            chrome::badge_large(&palette, recorder, frame);
        }),
    )
}

/// The look of a list and its row editing controls.
pub(super) fn list(glass: &Glass) -> ListLook {
    let palette = glass.palette;
    let move_control = || {
        state_chrome(glass, ChromeSurface::clear(CornerShape::circle())).custom(
            Custom::<(), _>::new((), move |recorder, frame, ()| {
                chrome::list_move_control(&palette, recorder, frame);
            }),
        )
    };
    let delete_control = state_chrome(glass, ChromeSurface::clear(CornerShape::circle())).custom(
        Custom::<(), _>::new((), move |recorder, frame, ()| {
            chrome::list_delete_control(&palette, recorder, frame);
        }),
    );
    let swipe_bg: Driven<Paint, f64> = Driven::new(move |inputs: &Inputs<'_, f64>| {
        #[expect(
            clippy::cast_possible_truncation,
            reason = "progress is clamped to [0, 1] before cast"
        )]
        let t = inputs.value.abs().clamp(0.0, 1.0) as f32;
        paint(alpha(palette.error, 0.4f32.mul_add(t, 0.6)))
    });
    let swipe_icon: Driven<ShapeData, f64> = Driven::new(|inputs: &Inputs<'_, f64>| {
        let bounds = inputs.frame;
        let r = bounds.height().min(44.0) * 0.2;
        let x = if *inputs.value < 0.0 {
            bounds.x1 - 24.0
        } else {
            bounds.x0 + 24.0
        };
        let c = Point::new(x, bounds.center().y);
        ShapeData::Line(Line::new(
            Point::new(c.x - r, c.y),
            Point::new(c.x + r, c.y),
        ))
    });
    ListLook::new(
        ListMetrics::new(
            ListRowMetrics::new(44.0, 16.0, 11.0),
            ListDividerMetrics::new(16.0, 0.0),
            ListTrailingControlMetrics::new(32.0, 32.0, 8.0, 6.0),
            ListSectionMetrics::new(36.0, 28.0),
        ),
        ListGestures {
            swipe_threshold: 0.5,
            reorder_threshold: 0.5,
            slop: 8.0,
            settle: Animation::ease_out(PRESS_DURATION),
        },
        Custom::<(), _>::new((), move |recorder, frame, ()| {
            chrome::list_row_background(&palette, recorder, frame, false);
        }),
        Custom::<(), _>::new((), move |recorder, frame, ()| {
            chrome::separator(&palette, recorder, frame);
        }),
        ListControls {
            move_up: move_control(),
            move_down: move_control(),
            delete: delete_control,
        },
    )
    .alternate_row(Custom::<(), _>::new((), move |recorder, frame, ()| {
        chrome::list_row_background(&palette, recorder, frame, true);
    }))
    .lifted_row(Custom::<(), _>::new((), move |recorder, frame, ()| {
        chrome::list_row_lifted(&palette, recorder, frame, 8.0);
    }))
    .swipe_background(Custom::<f64, _>::new(
        (swipe_bg, swipe_icon),
        move |recorder, frame, (bg, icon): (Live<Paint>, Live<ShapeData>)| {
            recorder.fill(frame, bg);
            recorder.stroke(icon, Stroke::new(2.0), working(palette.error_foreground));
        },
    ))
}

/// Converts a [`crate::dynamics::Spring`] to a framework [`Animation`].
pub(super) fn spring_animation(spring: crate::dynamics::Spring) -> Animation {
    Animation::spring(spring.stiffness(), spring.damping())
}

/// Applies horizontal or vertical affine stretch-squash
/// (`eps = 0.12 * tanh(|v_axis| / 600.0)`, `w_eff = w * (1 + eps)`,
/// `h_eff = h / (1 + 0.65 * eps)`) to `rect`, keeping box convexity at all
/// velocities without pinching the silhouette.
pub(super) fn stretch_along_axis(rect: Rect, velocity: Option<Vec2>, vertical: bool) -> Rect {
    let Some(v) = velocity else {
        return rect;
    };
    let v_axis = if vertical { v.y.abs() } else { v.x.abs() };
    let eps = 0.12 * (v_axis / 600.0).tanh();
    if eps <= 1e-6 {
        return rect;
    }
    let parallel = 1.0 + eps;
    let perp = 1.0 / 0.65f64.mul_add(eps, 1.0);
    let size = rect.size();
    let stretched = if vertical {
        Size::new(size.width * perp, size.height * parallel)
    } else {
        Size::new(size.width * parallel, size.height * perp)
    };
    Rect::from_center_size(rect.center(), stretched)
}

/// The look of a backend-rendered table.
pub(super) fn table(glass: &Glass) -> TableLook {
    let palette = glass.palette;
    TableLook::new(
        TableMetrics {
            min_column_width: 80.0,
            cell_horizontal_padding: 12.0,
            cell_vertical_inset: 8.0,
            header_height: 36.0,
            row_height: 40.0,
            outline_width: chrome::HAIRLINE,
        },
        Custom::<(), _>::new((), move |recorder, frame, ()| {
            chrome::table_background(&palette, recorder, frame);
        }),
        Custom::<(), _>::new((), move |recorder, frame, ()| {
            chrome::table_header_background(&palette, recorder, frame);
        }),
        Custom::<(), _>::new((), move |recorder, frame, ()| {
            chrome::table_cell_border(&palette, recorder, frame);
        }),
        Custom::<(), _>::new((), move |recorder, frame, ()| {
            chrome::table_column_separator(
                &palette,
                recorder,
                Point::new(frame.x0, frame.y0),
                Point::new(frame.x0, frame.y1),
            );
        }),
    )
}
