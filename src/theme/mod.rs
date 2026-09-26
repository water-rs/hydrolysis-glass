//! The glass widget theme: a Hydrolysis [`Style`] whose interactive surfaces
//! are the glass material.
//!
//! Buttons and capsules, tab bars, toolbars, sheets and containers are glass;
//! [`surfaces`] decides their silhouette and recipe. The material samples the
//! backdrop behind the widget, and no [`DrawContext`] primitive exposes that
//! backdrop, so those surfaces cannot be drawn through the `WidgetTheme`
//! drawing methods: calling one panics naming the missing engine capability
//! (`ENGINE_REQUIREMENTS.md`) rather than drawing a look-alike. Everything
//! else — toggles, steppers, inputs, pickers, sliders, progress, badges, lists,
//! tables, dividers and opaque menus — is drawn through [`DrawContext`] in
//! [`chrome`].

pub mod chrome;
pub mod palette;
pub mod surfaces;

use core::time::Duration;

use hydrolysis::Style;
use vello::kurbo::{BezPath, Point, Rect, RoundedRectRadii};
use vello::peniko::Color as PenikoColor;
use waterui::Plugin as _;
use waterui::animation::Animation;
use waterui::color::{Color, ColorScheme};
use waterui::reactive::Signal as _;
use waterui::text::font::{Font, Footnote, Subheadline};
use waterui::theme::{ColorSettings, FontSettings, Theme, installed_color_scheme};
use waterui_backend_core::widget::{
    BadgeMetrics, Brush, ButtonMetrics, DividerMetrics, DrawContext, InputFieldMetrics,
    InteractionMotion, ListDividerMetrics, ListMetrics, ListRowMetrics, ListSectionMetrics,
    ListTrailingControlMetrics, NavigationMetrics, NavigationMotion, PickerMetrics,
    ProgressIndicatorStyle, ProgressMetrics, ProgressMotion, RadioIndicatorState,
    RadioSelectionMotion, SliderMetrics, StepperEnd, StepperMetrics, TableMetrics, TabsMetrics,
    TextCaretMotion, TextContextMenuMetrics, ToggleMetrics, WidgetInteractionState, WidgetTheme,
};
use waterui_controls::button::{ButtonSize, ButtonStyle};
use waterui_controls::toggle::ToggleStyle;
use waterui_core::{EasingCurve, Environment};
use waterui_form::picker::PickerStyle;

use crate::material::gpu::Group;
use palette::{Palette, peniko, resolved};
use surfaces::Surface;

/// Duration of the eased press response: the time an exponential ease at
/// `PRESS_RATE` per 60 Hz frame takes to close 95 % of the gap,
/// `ln(0.05) / ln(1 − 0.22) / 60 ≈ 0.201 s`.
pub const PRESS_DURATION: Duration = Duration::from_millis(201);
/// Duration of the eased drag response, likewise from `DRAG_RATE`:
/// `ln(0.05) / ln(1 − 0.28) / 60 ≈ 0.152 s`.
pub const DRAG_DURATION: Duration = Duration::from_millis(152);
/// Duration of a navigation transition.
pub const NAVIGATION_DURATION: Duration = Duration::from_millis(350);
/// Linear indeterminate progress cycle.
pub const LINEAR_INDETERMINATE_CYCLE: Duration = Duration::from_millis(1_800);
/// Circular indeterminate progress cycle.
pub const CIRCULAR_INDETERMINATE_CYCLE: Duration = Duration::from_millis(1_400);
/// Loading indicator cycle.
pub const LOADING_CYCLE: Duration = Duration::from_millis(6_000);

/// The glass material as a Hydrolysis style.
#[derive(Clone, Debug)]
pub struct Glass {
    palette: Palette,
}

impl Glass {
    /// The theme for the light appearance.
    #[must_use]
    pub const fn light() -> Self {
        Self {
            palette: Palette::LIGHT,
        }
    }

    /// The theme for the dark appearance.
    #[must_use]
    pub const fn dark() -> Self {
        Self {
            palette: Palette::DARK,
        }
    }

    /// The theme for a `WaterUI` colour scheme.
    #[must_use]
    pub const fn for_scheme(scheme: ColorScheme) -> Self {
        Self {
            palette: Palette::for_scheme(scheme),
        }
    }

    /// The theme matching the colour scheme already installed in `env`, light
    /// when none is.
    #[must_use]
    pub fn from_environment(env: &Environment) -> Self {
        let scheme = installed_color_scheme(env).map_or(ColorScheme::Light, |s| s.snapshot());
        Self::for_scheme(scheme)
    }

    /// The colour roles the non-glass chrome draws with.
    #[must_use]
    pub const fn palette(&self) -> &Palette {
        &self.palette
    }

    /// The glass surface of a button as the theme would place it, or `None`
    /// for a style that draws no chrome.
    #[must_use]
    pub fn button_surface(
        &self,
        bounds: Rect,
        style: ButtonStyle,
        size: ButtonSize,
        icon_only: bool,
        state: WidgetInteractionState,
    ) -> Option<Surface> {
        let press = surfaces::press_amount(state, self.interaction_motion().pressed_opacity);
        surfaces::button(&self.palette, bounds, style, size, icon_only, press)
    }

    /// The glass surface of a tab bar.
    #[must_use]
    pub fn tab_bar_surface(&self, bounds: Rect) -> Surface {
        surfaces::tab_bar(&self.palette, bounds)
    }

    /// The glass surface of a toolbar or navigation bar.
    #[must_use]
    pub fn toolbar_surface(&self, bounds: Rect) -> Surface {
        surfaces::toolbar(&self.palette, bounds)
    }

    /// The glass surface of a sheet.
    #[must_use]
    pub fn sheet_surface(&self, bounds: Rect) -> Surface {
        surfaces::sheet(&self.palette, bounds)
    }

    /// The glass surface of a container.
    #[must_use]
    pub fn container_surface(&self, bounds: Rect) -> Surface {
        surfaces::container(&self.palette, bounds)
    }

    /// Fuses placed surfaces into one renderable group.
    #[must_use]
    pub fn group(surfaces: Vec<Surface>) -> Group {
        surfaces::group(surfaces)
    }

    fn color(c: palette::Rgba) -> Color {
        Color::from(resolved(c))
    }
}

impl Default for Glass {
    fn default() -> Self {
        Self::light()
    }
}

/// Panics naming the engine capability a glass surface needs.
///
/// The material refracts and blurs the content behind the widget; drawing it
/// needs a capture of that backdrop, which no `DrawContext` primitive offers.
/// This is the Hydrolysis rule for a component the backend cannot realise:
/// fail at the first draw, never draw a stand-in.
#[track_caller]
fn unsupported_glass_surface(what: &str) -> ! {
    panic!(
        "hydrolysis-glass cannot draw {what} through `DrawContext`: the glass material samples \
         the backdrop behind the widget and no `DrawContext` primitive exposes it. Render the \
         surface through the engine once it ships the APIs listed in ENGINE_REQUIREMENTS.md \
         (`engine` feature), or place it with `hydrolysis_glass::theme::surfaces` and draw the \
         resulting `Group` with `hydrolysis_glass::GlassRenderer` over a captured backdrop."
    )
}

impl Style for Glass {
    fn install_tokens(&self, env: &mut Environment) {
        let p = &self.palette;
        Theme::new()
            .color_scheme(p.scheme())
            .colors(
                ColorSettings::new()
                    .background(resolved(p.background))
                    .surface(resolved(p.surface))
                    .surface_variant(resolved(p.surface_variant))
                    .border(resolved(p.border))
                    .foreground(resolved(p.foreground))
                    .muted_foreground(resolved(p.muted_foreground))
                    .accent(resolved(p.accent))
                    .accent_container(resolved(palette::over(
                        palette::alpha(p.accent, 0.16),
                        p.background,
                    )))
                    .accent_foreground(resolved(p.accent_foreground))
                    .tertiary(resolved(p.muted_foreground))
                    .tertiary_container(resolved(p.surface_variant))
                    .selection_container(resolved(palette::alpha(p.accent, 0.3)))
                    .selection_foreground(resolved(p.foreground))
                    .error(resolved(p.error))
                    .error_foreground(resolved(p.error_foreground)),
            )
            .fonts(FontSettings::default_scale())
            .install(env);
    }
}

impl WidgetTheme for Glass {
    fn interaction_motion(&self) -> InteractionMotion {
        InteractionMotion {
            hover_opacity: chrome::HOVER_ALPHA,
            focus_opacity: chrome::HOVER_ALPHA,
            pressed_opacity: chrome::PRESSED_ALPHA,
            dragged_opacity: chrome::PRESSED_ALPHA,
            hover_enter: Animation::ease_out(PRESS_DURATION),
            hover_exit: Animation::ease_out(PRESS_DURATION),
            focus_enter: Animation::ease_out(PRESS_DURATION),
            focus_exit: Animation::ease_out(PRESS_DURATION),
            press_fade_in: Animation::ease_out(PRESS_DURATION),
            press_fade_out: Animation::ease_out(PRESS_DURATION),
            press_grow: Animation::ease_out(PRESS_DURATION),
            minimum_press_duration: PRESS_DURATION,
            touch_delay: Duration::ZERO,
        }
    }

    fn progress_motion(&self) -> ProgressMotion {
        ProgressMotion {
            linear_determinate: Animation::ease_in_out(Duration::from_millis(250)),
            circular_determinate: Animation::ease_in_out(Duration::from_millis(250)),
            linear_indeterminate_cycle: LINEAR_INDETERMINATE_CYCLE,
            circular_indeterminate_cycle: CIRCULAR_INDETERMINATE_CYCLE,
            loading_cycle: LOADING_CYCLE,
        }
    }

    fn text_caret_motion(&self) -> TextCaretMotion {
        TextCaretMotion {
            fade_cycle_duration: Duration::from_millis(1_000),
            frame_interval: Duration::from_millis(16),
            min_opacity: 0.0,
        }
    }

    fn navigation_motion(&self) -> NavigationMotion {
        NavigationMotion {
            transition_duration: NAVIGATION_DURATION,
            transition_easing: EasingCurve::bezier(0.2, 0.0, 0.0, 1.0),
            shared_axis_slide_distance: 30.0,
            fade_through_threshold: 0.35,
        }
    }

    fn button_metrics(&self, _style: ButtonStyle, size: ButtonSize) -> ButtonMetrics {
        match size {
            ButtonSize::ExtraSmall => ButtonMetrics::new(10.0, 4.0, 28.0, 28.0),
            ButtonSize::Small => ButtonMetrics::new(14.0, 7.0, 34.3, 34.3),
            ButtonSize::Medium => ButtonMetrics::new(20.0, 11.0, 44.0, 44.0),
            ButtonSize::Large => ButtonMetrics::new(24.0, 14.0, 55.7, 55.7),
            _ => ButtonMetrics::new(28.0, 16.0, 60.0, 60.0),
        }
    }

    fn icon_button_metrics(&self, style: ButtonStyle, size: ButtonSize) -> ButtonMetrics {
        let well = self.button_metrics(style, size).min_height;
        ButtonMetrics::new(0.0, 0.0, well, well)
    }

    fn button_label_color(&self, style: ButtonStyle, disabled: bool) -> Option<Color> {
        let p = &self.palette;
        let c = if surfaces::button_is_prominent(style) {
            p.accent_foreground
        } else if matches!(style, ButtonStyle::Link) {
            p.accent
        } else {
            p.foreground
        };
        Some(if disabled {
            Self::color(palette::over(
                palette::alpha(c, self.disabled_content_alpha()),
                p.background,
            ))
        } else {
            Self::color(c)
        })
    }

    fn button_label_font(&self, _style: ButtonStyle) -> Option<Font> {
        Some(Font::from(Subheadline))
    }

    fn draw_button_chrome(
        &self,
        _draw: &mut dyn DrawContext,
        _bounds: Rect,
        style: ButtonStyle,
        _icon_only: bool,
        _state: WidgetInteractionState,
    ) {
        if surfaces::button_is_glass(style) {
            unsupported_glass_surface("a glass button capsule");
        }
    }

    fn draw_button_state_layer(
        &self,
        draw: &mut dyn DrawContext,
        bounds: Rect,
        style: ButtonStyle,
        icon_only: bool,
        state: WidgetInteractionState,
    ) {
        if surfaces::button_is_glass(style) {
            return;
        }
        let shape = surfaces::button_shape(surfaces::rect(bounds), icon_only);
        let r = f64::from(shape.radii[0]);
        chrome::state_layer(
            &self.palette,
            draw,
            bounds,
            RoundedRectRadii::from_single_radius(r),
            state,
        );
    }

    fn draw_interaction_state_layer(
        &self,
        draw: &mut dyn DrawContext,
        bounds: Rect,
        radii: RoundedRectRadii,
        color: PenikoColor,
        state: WidgetInteractionState,
    ) {
        if state.disabled || state.state_layer_opacity <= 0.0 {
            return;
        }
        draw.fill_rounded_rect(
            bounds,
            radii,
            &Brush::from(color.with_alpha(state.state_layer_opacity)),
        );
    }

    fn toggle_metrics(&self, style: ToggleStyle) -> ToggleMetrics {
        match style {
            ToggleStyle::Checkbox => ToggleMetrics::new(22.0, 22.0, 10.0),
            _ => ToggleMetrics::new(51.0, 31.0, 12.0),
        }
    }

    fn toggle_value_animation(&self) -> Animation {
        Animation::ease_out(PRESS_DURATION)
    }

    fn draw_toggle_switch(
        &self,
        draw: &mut dyn DrawContext,
        bounds: Rect,
        progress: f32,
        _selected: bool,
        state: WidgetInteractionState,
    ) {
        chrome::toggle_switch(&self.palette, draw, bounds, progress, state);
    }

    fn draw_toggle_switch_state_layer(
        &self,
        draw: &mut dyn DrawContext,
        bounds: Rect,
        _progress: f32,
        _selected: bool,
        state: WidgetInteractionState,
    ) {
        let r = bounds.height() * 0.5;
        chrome::state_layer(
            &self.palette,
            draw,
            bounds,
            RoundedRectRadii::from_single_radius(r),
            state,
        );
    }

    fn draw_toggle_checkbox(
        &self,
        draw: &mut dyn DrawContext,
        bounds: Rect,
        progress: f32,
        state: WidgetInteractionState,
    ) {
        chrome::toggle_checkbox(&self.palette, draw, bounds, progress, state);
    }

    fn draw_toggle_checkbox_state_layer(
        &self,
        draw: &mut dyn DrawContext,
        bounds: Rect,
        _progress: f32,
        state: WidgetInteractionState,
    ) {
        chrome::state_layer(
            &self.palette,
            draw,
            bounds,
            RoundedRectRadii::from_single_radius(bounds.width() * 0.3),
            state,
        );
    }

    fn stepper_metrics(&self) -> StepperMetrics {
        StepperMetrics::new(32.0, 44.0, 32.0, 1.0, 12.0)
    }

    fn draw_stepper_button(
        &self,
        draw: &mut dyn DrawContext,
        bounds: Rect,
        end: StepperEnd,
        state: WidgetInteractionState,
    ) {
        chrome::stepper_button(&self.palette, draw, bounds, end, state);
    }

    fn draw_stepper_decrement_icon(&self, draw: &mut dyn DrawContext, bounds: Rect) {
        chrome::stepper_decrement_icon(&self.palette, draw, bounds);
    }

    fn draw_stepper_increment_icon(&self, draw: &mut dyn DrawContext, bounds: Rect) {
        chrome::stepper_increment_icon(&self.palette, draw, bounds);
    }

    fn input_field_metrics(&self) -> InputFieldMetrics {
        InputFieldMetrics::new(20.0, 120.0, 36.0, 12.0, 8.0)
    }

    fn input_placeholder_color(&self) -> Color {
        Self::color(self.palette.muted_foreground)
    }

    fn input_selection_brush(&self) -> Brush {
        Brush::from(peniko(palette::alpha(self.palette.accent, 0.3)))
    }

    fn input_caret_brush(&self, opacity: f32) -> Brush {
        Brush::from(peniko(palette::alpha(self.palette.accent, opacity)))
    }

    fn draw_input_field(
        &self,
        draw: &mut dyn DrawContext,
        bounds: Rect,
        state: WidgetInteractionState,
    ) {
        chrome::input_field(&self.palette, draw, bounds, state);
    }

    fn text_context_menu_metrics(&self) -> TextContextMenuMetrics {
        TextContextMenuMetrics {
            row_height: 40.0,
            horizontal_padding: 16.0,
            vertical_padding: 6.0,
            min_width: 120.0,
            max_width: 280.0,
            width_per_char: 8.0,
            corner_radius: chrome::MENU_CORNER_RADIUS,
            separator_horizontal_inset: 0.0,
            separator_thickness: chrome::HAIRLINE,
        }
    }

    fn draw_text_context_menu_panel(&self, draw: &mut dyn DrawContext, bounds: Rect) {
        chrome::popup_panel(&self.palette, draw, bounds);
    }

    fn draw_text_context_menu_separator(&self, draw: &mut dyn DrawContext, bounds: Rect) {
        chrome::separator(&self.palette, draw, bounds);
    }

    fn picker_metrics(&self, _style: PickerStyle) -> PickerMetrics {
        PickerMetrics {
            min_width: 120.0,
            min_height: 36.0,
            horizontal_inset: 12.0,
            vertical_inset: 8.0,
            label_spacing: 8.0,
            indicator_space: 24.0,
            radio_indicator_size: 22.0,
            radio_label_spacing: 10.0,
            radio_row_spacing: 12.0,
            popup_top_spacing: 6.0,
            popup_row_height: 40.0,
            popup_corner_radius: chrome::MENU_CORNER_RADIUS,
            segment_min_width: 64.0,
        }
    }

    fn radio_selection_motion(&self) -> RadioSelectionMotion {
        RadioSelectionMotion {
            inner_grow: Animation::ease_out(PRESS_DURATION),
            inner_opacity: Animation::ease_out(PRESS_DURATION),
            outer_color: Animation::ease_out(PRESS_DURATION),
        }
    }

    fn draw_picker_indicator(&self, draw: &mut dyn DrawContext, bounds: Rect) {
        chrome::picker_indicator(&self.palette, draw, bounds);
    }

    fn draw_picker_state_layer(
        &self,
        draw: &mut dyn DrawContext,
        bounds: Rect,
        state: WidgetInteractionState,
    ) {
        chrome::state_layer(
            &self.palette,
            draw,
            bounds,
            RoundedRectRadii::from_single_radius(10.0),
            state,
        );
    }

    fn draw_picker_popup(&self, draw: &mut dyn DrawContext, popup_rect: Rect) {
        chrome::popup_panel(&self.palette, draw, popup_rect);
    }

    fn draw_picker_popup_row_background(
        &self,
        draw: &mut dyn DrawContext,
        row_rect: Rect,
        selected: bool,
    ) {
        chrome::popup_row_background(&self.palette, draw, row_rect, selected);
    }

    fn draw_picker_popup_row_state_layer(
        &self,
        draw: &mut dyn DrawContext,
        row_rect: Rect,
        _selected: bool,
        state: WidgetInteractionState,
    ) {
        chrome::state_layer(
            &self.palette,
            draw,
            row_rect.inset(-4.0),
            RoundedRectRadii::from_single_radius(8.0),
            state,
        );
    }

    fn draw_picker_separator(&self, draw: &mut dyn DrawContext, separator: Rect) {
        chrome::separator(&self.palette, draw, separator);
    }

    fn draw_radio_indicator(
        &self,
        draw: &mut dyn DrawContext,
        center: Point,
        radius: f64,
        state: RadioIndicatorState,
    ) {
        chrome::radio_indicator(&self.palette, draw, center, radius, state);
    }

    fn draw_radio_state_layer(
        &self,
        draw: &mut dyn DrawContext,
        center: Point,
        radius: f64,
        _selected: bool,
        state: WidgetInteractionState,
    ) {
        chrome::circular_state_layer(&self.palette, draw, center, radius * 1.6, state);
    }

    fn segmented_picker_label_color(&self, _selected: bool) -> Option<Color> {
        Some(Self::color(self.palette.foreground))
    }

    fn draw_segmented_picker_container(
        &self,
        draw: &mut dyn DrawContext,
        bounds: Rect,
        _segment_count: usize,
    ) {
        chrome::segmented_container(&self.palette, draw, bounds);
    }

    fn draw_segmented_picker_segment(
        &self,
        draw: &mut dyn DrawContext,
        bounds: Rect,
        selected: bool,
        _is_first: bool,
        _is_last: bool,
    ) {
        chrome::segmented_segment(&self.palette, draw, bounds, selected);
    }

    fn draw_segmented_picker_state_layer(
        &self,
        draw: &mut dyn DrawContext,
        bounds: Rect,
        _selected: bool,
        _is_first: bool,
        _is_last: bool,
        state: WidgetInteractionState,
    ) {
        chrome::state_layer(
            &self.palette,
            draw,
            bounds,
            RoundedRectRadii::from_single_radius(7.0),
            state,
        );
    }

    fn slider_metrics(&self) -> SliderMetrics {
        SliderMetrics::new(0.0, 8.0, 8.0, 120.0, 4.0, 28.0, 28.0)
    }

    fn draw_slider_track(
        &self,
        draw: &mut dyn DrawContext,
        track_rect: Rect,
        fill_rect: Rect,
        state: WidgetInteractionState,
    ) {
        chrome::slider_track(&self.palette, draw, track_rect, fill_rect, state);
    }

    fn draw_slider_thumb(
        &self,
        draw: &mut dyn DrawContext,
        center: Point,
        radius: f64,
        state: WidgetInteractionState,
    ) {
        chrome::slider_thumb(&self.palette, draw, center, radius, state);
    }

    fn draw_slider_thumb_state_layer(
        &self,
        draw: &mut dyn DrawContext,
        center: Point,
        radius: f64,
        state: WidgetInteractionState,
    ) {
        chrome::circular_state_layer(&self.palette, draw, center, radius * 1.4, state);
    }

    fn progress_metrics(&self, style: ProgressIndicatorStyle) -> ProgressMetrics {
        match style {
            ProgressIndicatorStyle::Linear => {
                ProgressMetrics::linear(20.0, 6.0, 4.0, 0.0, 6.0, 120.0)
            }
            ProgressIndicatorStyle::Circular => ProgressMetrics::circular(28.0, 3.0),
            ProgressIndicatorStyle::Loading => ProgressMetrics::loading(48.0, 28.0),
        }
    }

    fn draw_progress_linear_track(
        &self,
        draw: &mut dyn DrawContext,
        bounds: Rect,
        active_end: Option<f64>,
    ) {
        chrome::progress_linear_track(&self.palette, draw, bounds, active_end);
    }

    fn draw_progress_linear_fill(&self, draw: &mut dyn DrawContext, bounds: Rect) {
        chrome::progress_linear_fill(&self.palette, draw, bounds);
    }

    fn draw_progress_linear_indeterminate(
        &self,
        draw: &mut dyn DrawContext,
        bounds: Rect,
        elapsed: Duration,
        _four_color: bool,
    ) {
        chrome::progress_linear_indeterminate(
            &self.palette,
            draw,
            bounds,
            elapsed,
            LINEAR_INDETERMINATE_CYCLE,
        );
    }

    fn draw_progress_circular_track(
        &self,
        draw: &mut dyn DrawContext,
        center: Point,
        radius: f64,
        width: f64,
        active_turns: Option<f64>,
    ) {
        chrome::progress_circular_track(&self.palette, draw, center, radius, width, active_turns);
    }

    fn draw_progress_circular_fill(&self, draw: &mut dyn DrawContext, path: &BezPath, width: f64) {
        chrome::progress_circular_fill(&self.palette, draw, path, width);
    }

    fn draw_progress_loading(
        &self,
        draw: &mut dyn DrawContext,
        bounds: Rect,
        elapsed: Duration,
        _four_color: bool,
    ) {
        let size = self
            .progress_metrics(ProgressIndicatorStyle::Loading)
            .loading_indicator_size;
        chrome::progress_loading(&self.palette, draw, bounds, size, elapsed, LOADING_CYCLE);
    }

    fn draw_progress_circular_indeterminate(
        &self,
        draw: &mut dyn DrawContext,
        center: Point,
        radius: f64,
        width: f64,
        elapsed: Duration,
        _four_color: bool,
    ) {
        chrome::progress_circular_indeterminate(
            &self.palette,
            draw,
            center,
            radius,
            width,
            elapsed,
            CIRCULAR_INDETERMINATE_CYCLE,
        );
    }

    fn navigation_metrics(&self) -> NavigationMetrics {
        NavigationMetrics {
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
        }
    }

    fn draw_navigation_bar(&self, _draw: &mut dyn DrawContext, _bounds: Rect, _background: &Brush) {
        unsupported_glass_surface("a glass toolbar");
    }

    fn draw_navigation_bar_separator(&self, _draw: &mut dyn DrawContext, _bounds: Rect) {}

    fn draw_navigation_back_button(&self, draw: &mut dyn DrawContext, bounds: Rect) {
        chrome::back_button(&self.palette, draw, bounds);
    }

    fn tabs_metrics(&self) -> TabsMetrics {
        TabsMetrics::new(60.0, 64.0, 12.0, 0.0, 0.0)
    }

    fn draw_tabs_bar(&self, _draw: &mut dyn DrawContext, _bounds: Rect, _top_edge: bool) {
        unsupported_glass_surface("a glass tab bar");
    }

    fn draw_tabs_highlight(&self, draw: &mut dyn DrawContext, bounds: Rect) {
        chrome::tabs_highlight(&self.palette, draw, bounds);
    }

    fn draw_tabs_button_state_layer(
        &self,
        draw: &mut dyn DrawContext,
        bounds: Rect,
        _selected: bool,
        state: WidgetInteractionState,
    ) {
        let r = bounds.height().min(bounds.width()) * 0.5;
        chrome::state_layer(
            &self.palette,
            draw,
            bounds,
            RoundedRectRadii::from_single_radius(r),
            state,
        );
    }

    fn draw_scroll_indicator(&self, draw: &mut dyn DrawContext, bounds: Rect) {
        chrome::scroll_indicator(&self.palette, draw, bounds);
    }

    fn divider_metrics(&self) -> DividerMetrics {
        DividerMetrics::new(chrome::HAIRLINE)
    }

    fn draw_divider(&self, draw: &mut dyn DrawContext, bounds: Rect) {
        chrome::separator(&self.palette, draw, bounds);
    }

    fn badge_metrics(&self) -> BadgeMetrics {
        BadgeMetrics::new(8.0, 18.0, 6.0, 4.0, 4.0, 9.0, 9.0)
    }

    fn badge_label_color(&self) -> Color {
        Self::color(self.palette.error_foreground)
    }

    fn badge_label_font(&self) -> Font {
        Font::from(Footnote)
    }

    fn draw_badge_small(&self, draw: &mut dyn DrawContext, bounds: Rect) {
        chrome::badge_small(&self.palette, draw, bounds);
    }

    fn draw_badge_large(&self, draw: &mut dyn DrawContext, bounds: Rect) {
        chrome::badge_large(&self.palette, draw, bounds);
    }

    fn list_metrics(&self) -> ListMetrics {
        ListMetrics::new(
            ListRowMetrics::new(44.0, 16.0, 11.0),
            ListDividerMetrics::new(16.0, 0.0),
            ListTrailingControlMetrics::new(32.0, 32.0, 8.0, 6.0),
            ListSectionMetrics::new(36.0, 28.0),
        )
    }

    fn draw_list_row_background(&self, draw: &mut dyn DrawContext, bounds: Rect, alternate: bool) {
        chrome::list_row_background(&self.palette, draw, bounds, alternate);
    }

    fn draw_list_move_control(&self, draw: &mut dyn DrawContext, bounds: Rect) {
        chrome::list_move_control(&self.palette, draw, bounds);
    }

    fn draw_list_move_control_state_layer(
        &self,
        draw: &mut dyn DrawContext,
        bounds: Rect,
        state: WidgetInteractionState,
    ) {
        chrome::circular_state_layer(
            &self.palette,
            draw,
            bounds.center(),
            bounds.width().min(bounds.height()) * 0.5,
            state,
        );
    }

    fn draw_list_delete_control(&self, draw: &mut dyn DrawContext, bounds: Rect) {
        chrome::list_delete_control(&self.palette, draw, bounds);
    }

    fn draw_list_delete_control_state_layer(
        &self,
        draw: &mut dyn DrawContext,
        bounds: Rect,
        state: WidgetInteractionState,
    ) {
        chrome::circular_state_layer(
            &self.palette,
            draw,
            bounds.center(),
            bounds.width().min(bounds.height()) * 0.5,
            state,
        );
    }

    fn draw_list_swipe_dismiss_background(
        &self,
        draw: &mut dyn DrawContext,
        bounds: Rect,
        progress: f64,
        toward_start: bool,
    ) {
        chrome::list_swipe_dismiss_background(&self.palette, draw, bounds, progress, toward_start);
    }

    fn draw_list_row_lifted(&self, draw: &mut dyn DrawContext, bounds: Rect, elevation: f64) {
        chrome::list_row_lifted(&self.palette, draw, bounds, elevation);
    }

    fn draw_list_separator(&self, draw: &mut dyn DrawContext, bounds: Rect) {
        chrome::separator(&self.palette, draw, bounds);
    }

    fn table_metrics(&self) -> TableMetrics {
        TableMetrics {
            min_column_width: 80.0,
            cell_horizontal_padding: 12.0,
            cell_vertical_inset: 8.0,
            header_height: 36.0,
            row_height: 40.0,
            outline_width: chrome::HAIRLINE,
        }
    }

    fn draw_table_background(&self, draw: &mut dyn DrawContext, bounds: Rect) {
        chrome::table_background(&self.palette, draw, bounds);
    }

    fn draw_table_header_background(&self, draw: &mut dyn DrawContext, bounds: Rect) {
        chrome::table_header_background(&self.palette, draw, bounds);
    }

    fn draw_table_cell_border(&self, draw: &mut dyn DrawContext, bounds: Rect) {
        chrome::table_cell_border(&self.palette, draw, bounds);
    }

    fn draw_table_column_separator(&self, draw: &mut dyn DrawContext, from: Point, to: Point) {
        chrome::table_column_separator(&self.palette, draw, from, to);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_install_the_scheme() {
        let mut env = Environment::new();
        Glass::dark().install_tokens(&mut env);
        let scheme = installed_color_scheme(&env).expect("installed").snapshot();
        assert_eq!(scheme, ColorScheme::Dark);
        assert_eq!(
            Glass::from_environment(&env).palette().scheme(),
            ColorScheme::Dark
        );
    }

    #[test]
    fn plain_button_draws_no_chrome() {
        struct Nop;
        impl DrawContext for Nop {
            fn fill_rect(&mut self, _: Rect, _: &Brush) {}
            fn fill_rounded_rect(&mut self, _: Rect, _: RoundedRectRadii, _: &Brush) {}
            fn stroke_rect(&mut self, _: Rect, _: &Brush, _: f64) {}
            fn stroke_rounded_rect(&mut self, _: Rect, _: RoundedRectRadii, _: &Brush, _: f64) {}
            fn stroke_line(&mut self, _: Point, _: Point, _: &Brush, _: f64) {}
            fn stroke_circle(&mut self, _: Point, _: f64, _: &Brush, _: f64) {}
            fn fill_circle(&mut self, _: Point, _: f64, _: &Brush) {}
            fn fill_path(&mut self, _: &BezPath, _: &Brush) {}
            fn stroke_path(&mut self, _: &BezPath, _: &Brush, _: f64) {}
            fn draw_shadow(
                &mut self,
                _: Rect,
                _: RoundedRectRadii,
                _: vello::kurbo::Vec2,
                _: f64,
                _: PenikoColor,
            ) {
            }
            fn push_layer(&mut self, _: f32, _: Option<&Rect>) {}
            fn pop_layer(&mut self) {}
            fn push_transform(&mut self, _: vello::kurbo::Affine) {}
            fn pop_transform(&mut self) {}
        }
        let g = Glass::light();
        let b = Rect::new(0.0, 0.0, 100.0, 40.0);
        g.draw_button_chrome(
            &mut Nop,
            b,
            ButtonStyle::Plain,
            false,
            WidgetInteractionState::NONE,
        );
        assert!(
            g.button_surface(
                b,
                ButtonStyle::Plain,
                ButtonSize::Small,
                false,
                WidgetInteractionState::NONE
            )
            .is_none()
        );
        assert!(
            g.button_surface(
                b,
                ButtonStyle::Glass,
                ButtonSize::Small,
                false,
                WidgetInteractionState::NONE
            )
            .is_some()
        );
    }

    #[test]
    #[should_panic(expected = "ENGINE_REQUIREMENTS.md")]
    fn glass_button_refuses_a_stand_in() {
        struct Nop;
        impl DrawContext for Nop {
            fn fill_rect(&mut self, _: Rect, _: &Brush) {}
            fn fill_rounded_rect(&mut self, _: Rect, _: RoundedRectRadii, _: &Brush) {}
            fn stroke_rect(&mut self, _: Rect, _: &Brush, _: f64) {}
            fn stroke_rounded_rect(&mut self, _: Rect, _: RoundedRectRadii, _: &Brush, _: f64) {}
            fn stroke_line(&mut self, _: Point, _: Point, _: &Brush, _: f64) {}
            fn stroke_circle(&mut self, _: Point, _: f64, _: &Brush, _: f64) {}
            fn fill_circle(&mut self, _: Point, _: f64, _: &Brush) {}
            fn fill_path(&mut self, _: &BezPath, _: &Brush) {}
            fn stroke_path(&mut self, _: &BezPath, _: &Brush, _: f64) {}
            fn draw_shadow(
                &mut self,
                _: Rect,
                _: RoundedRectRadii,
                _: vello::kurbo::Vec2,
                _: f64,
                _: PenikoColor,
            ) {
            }
            fn push_layer(&mut self, _: f32, _: Option<&Rect>) {}
            fn pop_layer(&mut self) {}
            fn push_transform(&mut self, _: vello::kurbo::Affine) {}
            fn pop_transform(&mut self) {}
        }
        Glass::light().draw_button_chrome(
            &mut Nop,
            Rect::new(0.0, 0.0, 100.0, 40.0),
            ButtonStyle::Glass,
            false,
            WidgetInteractionState::NONE,
        );
    }

    #[test]
    fn ease_durations_follow_the_frame_rates() {
        use crate::interaction::{DRAG_RATE, PRESS_RATE};
        let seconds = |rate: f32| 0.05f32.log(1.0 - rate) / 60.0;
        assert!((PRESS_DURATION.as_secs_f32() - seconds(PRESS_RATE)).abs() < 0.002);
        assert!((DRAG_DURATION.as_secs_f32() - seconds(DRAG_RATE)).abs() < 0.002);
    }
}
