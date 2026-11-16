//! The navigation bar look: every bar a standalone glass toolbar with no
//! separator, and the back button an accent chevron.

use hydrolysis::theme::{
    BarSurface, Chrome, CornerShape, Custom, FocusIndicator, NavigationBarLook, NavigationMetrics,
    NavigationMotion, NavigationSpec, PressFeedback, StateLayer, Surface as ChromeSurface,
};
use waterui_core::{EasingCurve, Environment};

use super::super::chrome;
use super::super::{Glass, NAVIGATION_DURATION, record_standalone};
use super::control_motion;

/// The measures of the navigation chrome: 44 pt inline bars and 96 pt
/// medium and large ones, titles and items inset 16 pt and 8 pt apart, a
/// 36 pt search field and a 44 pt back button 8 pt from the leading edge.
const METRICS: NavigationMetrics = NavigationMetrics {
    automatic_bar_height: 44.0,
    inline_bar_height: 44.0,
    medium_bar_height: 96.0,
    large_bar_height: 96.0,
    inline_title_height: 44.0,
    medium_title_height: 52.0,
    large_title_height: 52.0,
    title_leading_inset: 16.0,
    title_trailing_inset: 16.0,
    large_title_bottom_inset: 8.0,
    horizontal_inset: 16.0,
    item_spacing: 8.0,
    search_height: 36.0,
    search_vertical_inset: 8.0,
    back_button_size: 44.0,
    back_button_leading_inset: 8.0,
    back_button_top_inset: 0.0,
};

/// A page transition: [`NAVIGATION_DURATION`] on the emphasized decelerate
/// curve, sliding 30 pt and fading through at 35 % of its progress.
const fn motion() -> NavigationMotion {
    NavigationMotion {
        transition_duration: NAVIGATION_DURATION,
        transition_easing: EasingCurve::bezier(0.2, 0.0, 0.0, 1.0),
        shared_axis_slide_distance: 30.0,
        fade_through_threshold: 0.35,
    }
}

/// The look of the navigation chrome in `spec`.
///
/// The bar is the theme's toolbar surface over the bar's frame, whatever
/// background the application gives it. The back button draws its chevron
/// and nothing else: no container, no state layer and no focus ring.
pub(in super::super) fn look(
    glass: &Glass,
    _spec: NavigationSpec,
    _env: &Environment,
) -> NavigationBarLook {
    let toolbar = glass.clone();
    let bar = Custom::<BarSurface, _>::new((), move |recorder, frame, ()| {
        record_standalone(recorder, &toolbar.toolbar_surface(frame));
    });
    let palette = glass.palette;
    let chevron = Custom::<(), _>::new((), move |recorder, frame, ()| {
        chrome::back_button(&palette, recorder, frame);
    });
    let back = Chrome::new(
        ChromeSurface::clear(CornerShape::full()),
        StateLayer::new(Glass::color(palette.state_layer)),
        PressFeedback::new(),
        FocusIndicator::Tint,
        control_motion(),
    )
    .custom(chevron);
    NavigationBarLook::new(METRICS, motion(), back, bar)
}
