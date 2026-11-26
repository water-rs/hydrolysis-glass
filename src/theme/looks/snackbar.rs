//! The glass snackbar look: floating regular-glass banner with theme-token
//! foreground and accent typography and soft elevation shadows.

use core::time::Duration;

use hydrolysis_style::theme::SnackbarLook;
use waterui::animation::Animation;
use waterui::background::Glass as GlassBackground;
use waterui::color::Color;
use waterui::layout::padding::EdgeInsets;
use waterui::text::font::{Body, Font, Headline};
use waterui::theme::color::{Accent, Foreground, Surface};

const HORIZONTAL_PADDING: f32 = 20.0;
const VERTICAL_PADDING: f32 = 14.0;
const VIEWPORT_PADDING: f32 = 16.0;
const CONTENT_SPACING: f32 = 10.0;
const MIN_WIDTH: f32 = 288.0;
const MAX_WIDTH: f32 = 480.0;
const ACTION_TRAILING_PADDING: f32 = 12.0;
const CLOSE_ICON_SIZE: f32 = 14.0;
const CLOSE_STATE_LAYER_SIZE: f32 = 32.0;
const SINGLE_LINE_MIN_HEIGHT: f32 = 50.0;
const STACK_GAP: f32 = 8.0;
const CORNER_RADIUS: f32 = 14.0;
const MOTION_OFFSET_Y: f32 = 16.0;
const ENTER_DURATION: Duration = Duration::from_millis(280);
const EXIT_DURATION: Duration = Duration::from_millis(220);

/// Floating glass snackbar look for the glass style.
#[must_use]
pub(in super::super) fn look() -> SnackbarLook {
    SnackbarLook {
        container_color: Surface.into(),
        supporting_text_color: Foreground.into(),
        action_label_color: Accent.into(),
        supporting_text_font: Font::from(Body),
        action_label_font: Font::from(Headline),
        content_padding: EdgeInsets::symmetric(VERTICAL_PADDING, HORIZONTAL_PADDING),
        viewport_padding: EdgeInsets::all(VIEWPORT_PADDING),
        content_spacing: CONTENT_SPACING,
        min_width: MIN_WIDTH,
        max_width: MAX_WIDTH,
        action_trailing_padding: ACTION_TRAILING_PADDING,
        close_icon_size: CLOSE_ICON_SIZE,
        close_state_layer_size: CLOSE_STATE_LAYER_SIZE,
        single_line_min_height: SINGLE_LINE_MIN_HEIGHT,
        stack_gap: STACK_GAP,
        corner_radius: CORNER_RADIUS,
        background_glass: Some(GlassBackground::regular()),
        shadow_color: Color::srgb(0, 0, 0).with_opacity(0.18),
        shadow_radius: 18.0,
        shadow_offset_y: 6.0,
        ambient_shadow_color: Color::srgb(0, 0, 0).with_opacity(0.06),
        ambient_shadow_radius: 2.0,
        ambient_shadow_offset_y: 1.0,
        motion_offset_y: MOTION_OFFSET_Y,
        enter_animation: Animation::bezier(ENTER_DURATION, 0.2, 0.0, 0.0, 1.0),
        exit_animation: Animation::bezier(EXIT_DURATION, 0.4, 0.0, 1.0, 1.0),
    }
}

#[cfg(test)]
mod tests {
    use hydrolysis_style::theme::WidgetTheme as _;
    use waterui_core::Environment;

    use super::{CORNER_RADIUS, MAX_WIDTH, MIN_WIDTH, SINGLE_LINE_MIN_HEIGHT, STACK_GAP};
    use crate::Glass;

    #[test]
    fn snackbar_look_uses_regular_glass_surface() {
        let look = Glass::light().snackbar(&Environment::new());

        assert!(look.background_glass.is_some());
        assert_eq!(look.min_width, MIN_WIDTH);
        assert_eq!(look.max_width, MAX_WIDTH);
        assert_eq!(look.single_line_min_height, SINGLE_LINE_MIN_HEIGHT);
        assert_eq!(look.stack_gap, STACK_GAP);
        assert_eq!(look.corner_radius, CORNER_RADIUS);
    }
}
