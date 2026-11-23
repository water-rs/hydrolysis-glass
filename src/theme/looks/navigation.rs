//! The navigation bar look (specification §8.8): transparent bar chrome at
//! rest with no hairline separator, and the back button a direction-aware
//! accent chevron with state feedback and focus ring.

use hydrolysis_style::theme::{
    BarSurface, CornerShape, Custom, NavigationBarLook, NavigationMetrics, NavigationMotion,
    NavigationSpec, Surface as ChromeSurface,
};
use waterui_core::{EasingCurve, Environment};

use super::super::chrome;
use super::super::{Glass, NAVIGATION_DURATION};
use super::{hit_region, state_chrome};

/// The measures of the navigation chrome: 44 pt inline bars and 96 pt
/// medium and large ones, titles and items inset 16 pt and 8 pt apart, a
/// 56 pt search field (§8.6) and a 44 pt back button 8 pt from the leading
/// edge (§8.8, Geometry).
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
    search_height: 56.0,
    search_vertical_inset: 8.0,
    back_button_size: 44.0,
    back_button_leading_inset: 8.0,
    back_button_top_inset: 0.0,
};

/// A page transition: [`NAVIGATION_DURATION`] on the emphasized decelerate
/// curve, sliding 30 pt and fading through at 35 % of its progress (§8.8,
/// Motion).
const fn motion() -> NavigationMotion {
    NavigationMotion {
        transition_duration: NAVIGATION_DURATION,
        transition_easing: EasingCurve::bezier(0.2, 0.0, 0.0, 1.0),
        shared_axis_slide_distance: 30.0,
        fade_through_threshold: 0.35,
    }
}

/// The look of the navigation chrome in `spec` (§8.8).
///
/// The bar leaves its resting background transparent over whatever content sits
/// beneath it. The back button draws its direction-aware chevron over a clear
/// capsule with the theme's state layer, focus ring and 44 pt hit target.
pub(in super::super) fn look(
    glass: &Glass,
    spec: NavigationSpec,
    _env: &Environment,
) -> NavigationBarLook {
    let bar = Custom::<BarSurface, _>::new((), |_recorder, _frame, ()| {});
    let palette = glass.palette;
    let rtl = spec.direction().is_right_to_left();
    let chevron = Custom::<(), _>::new((), move |recorder, frame, ()| {
        chrome::back_button_directed(&palette, recorder, frame, rtl);
    });
    let back = state_chrome(glass, ChromeSurface::clear(CornerShape::full()))
        .hit(hit_region())
        .custom(chevron);
    NavigationBarLook::new(METRICS, motion(), back, bar)
}
