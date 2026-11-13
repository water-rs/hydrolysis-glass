//! The glass widget theme: a Hydrolysis [`Style`] whose interactive surfaces
//! are the glass material.
//!
//! Buttons and capsules, tab bars, toolbars, sheets and containers are glass;
//! [`surfaces`] decides their silhouette and recipe and [`materials`] records
//! each surface as a `Recorder::backdrop_material` — the engine's material
//! contract (water-rs/waterui#1788 and followers). A backend drawing chrome
//! through `Content::record_layered` realizes each surface as a backdrop
//! member of the group its capture class names; [`materials::register`]
//! declares the shader and the capture classes the members record under.
//! Buttons, switches, checkboxes, steppers, sliders, segmented controls,
//! progress indicators, text fields, menus, menu pickers, radio groups,
//! navigation bars and tab bars are looks (`looks`): data whose values
//! follow the widget's state, and whose glass bodies are backdrop fills over
//! the same shader and capture classes. Everything else — badges, lists,
//! tables and dividers — is recorded through the `Recorder` in [`chrome`],
//! whose drawing the looks reuse for their parts that are not glass.

pub mod button;
pub mod chrome;
mod looks;
pub mod materials;
pub mod palette;
pub mod segmented;
pub mod slider;
pub mod surfaces;
pub mod switch;

use core::time::Duration;

use cherenkov_record::kurbo::{Point, Rect};
use cherenkov_record::{MaterialRegistry, Recorder};
use hydrolysis::Style;
use hydrolysis::theme::{
    BadgeMetrics, ButtonLook, ButtonSpec, CheckboxLook, DividerMetrics, GlassLook, IndicatorLook,
    InteractionMotion, LinearProgressLook, ListDividerMetrics, ListMetrics, ListRowMetrics,
    ListSectionMetrics, ListTrailingControlMetrics, MenuLook, MenuPickerLook, MenuSpec,
    NavigationBarLook, NavigationSpec, PickerSpec, ProgressSpec, RadioGroupLook, SegmentedLook,
    SegmentedSpec, SliderLook, SliderSpec, StepperLook, StepperSpec, SwitchLook, TableMetrics,
    TabsLook, TabsSpec, TextFieldLook, TextFieldSpec, ToggleSpec, WidgetInteractionState,
    WidgetTheme,
};
use waterui::Plugin as _;
use waterui::animation::Animation;
use waterui::color::{Color, ColorScheme};
use waterui::interaction::InteractionState;
use waterui::reactive::Signal as _;
use waterui::text::font::{Font, Footnote};
use waterui::theme::{ColorSettings, FontSettings, Theme, installed_color_scheme};
use waterui_controls::ControlSize;
use waterui_controls::button::ButtonStyle;
use waterui_core::Environment;

use crate::material::gpu::Group;
use crate::material::recipe::Translucency;
use palette::{Palette, resolved};
use surfaces::Surface;

/// Duration of the eased press response: the time an exponential ease at
/// 0.22 per 60 Hz frame takes to close 95 % of the gap,
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
    translucency: Translucency,
}

impl Glass {
    /// The theme for the light appearance.
    #[must_use]
    pub const fn light() -> Self {
        Self {
            palette: Palette::LIGHT,
            translucency: Translucency::DEFAULT,
        }
    }

    /// The theme for the dark appearance.
    #[must_use]
    pub const fn dark() -> Self {
        Self {
            palette: Palette::DARK,
            translucency: Translucency::DEFAULT,
        }
    }

    /// The theme for a `WaterUI` colour scheme.
    #[must_use]
    pub const fn for_scheme(scheme: ColorScheme) -> Self {
        Self {
            palette: Palette::for_scheme(scheme),
            translucency: Translucency::DEFAULT,
        }
    }

    /// The theme with a different translucency setting (spec §5.4).
    #[must_use]
    pub const fn with_translucency(mut self, translucency: Translucency) -> Self {
        self.translucency = translucency;
        self
    }

    /// The translucency setting the surfaces' recipes use.
    #[must_use]
    pub const fn translucency(&self) -> Translucency {
        self.translucency
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
    /// for a style that draws no chrome. `bounds` is the button's frame.
    #[must_use]
    pub fn button_surface(
        &self,
        bounds: Rect,
        style: ButtonStyle,
        size: ControlSize,
        state: WidgetInteractionState,
    ) -> Option<Surface> {
        let kind = surfaces::button_kind(style)?;
        let spec = surfaces::ButtonSpec {
            enabled: !state.state.contains(InteractionState::DISABLED),
            ..surfaces::ButtonSpec::new(kind, button::SizeGroup::of(size))
        };
        let press = surfaces::Press {
            progress: surfaces::press_progress(state, self.interaction_motion().pressed_opacity),
            // The renderer hands the chrome the touch in window space; the
            // press is centred on the frame until it is local.
            touch_x: None,
        };
        Some(surfaces::button(
            &self.palette,
            bounds,
            &spec,
            press,
            self.translucency,
        ))
    }

    /// The glass surface of a tab bar.
    #[must_use]
    pub fn tab_bar_surface(&self, bounds: Rect) -> Surface {
        surfaces::tab_bar(&self.palette, bounds, self.translucency)
    }

    /// The glass surface of a toolbar or navigation bar.
    #[must_use]
    pub fn toolbar_surface(&self, bounds: Rect) -> Surface {
        surfaces::toolbar(&self.palette, bounds, self.translucency)
    }

    /// The glass surface of a sheet.
    #[must_use]
    pub fn sheet_surface(&self, bounds: Rect) -> Surface {
        surfaces::sheet(&self.palette, bounds, self.translucency)
    }

    /// The glass surface of a container.
    #[must_use]
    pub fn container_surface(&self, bounds: Rect) -> Surface {
        surfaces::container(&self.palette, bounds, self.translucency)
    }

    /// Fuses placed surfaces into one renderable group.
    #[must_use]
    pub fn group(surfaces: Vec<Surface>) -> Group {
        surfaces::group(surfaces)
    }

    fn color(c: palette::Rgba) -> Color {
        Color::from(waterui::color::Working(resolved(c)))
    }
}

impl Default for Glass {
    fn default() -> Self {
        Self::light()
    }
}

/// Records `surface`'s element as a standalone backdrop material — the
/// widget-chrome surfaces (a button, a tab bar, a toolbar) draw as their
/// own members; the containers that fuse neighbours mount under a
/// `MaterialScope` the host gives the recording.
fn record_standalone(recorder: &mut Recorder, surface: &Surface) {
    materials::record(recorder, &surface.element, false);
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
            // The glass button's press progress is the newest press wave's
            // opacity, so the wave's fade-in and fade-out are the press
            // and release springs of §8.1, and a release starts at once.
            press_fade_in: button::Spring::PRESS.physical(),
            press_fade_out: button::Spring::RELEASE.physical(),
            press_grow: Animation::ease_out(PRESS_DURATION),
            // `press_progress` drives the physical shape morph; the press
            // spring is the contract's one animation for both directions.
            press_shape: button::Spring::PRESS.physical(),
            minimum_press_duration: Duration::ZERO,
            touch_delay: Duration::ZERO,
        }
    }

    fn register_backdrop_shaders(&self, registry: &mut MaterialRegistry) {
        materials::register(registry);
    }

    fn glass_background(
        &self,
        glass: &waterui::background::Glass,
        env: &Environment,
    ) -> Option<GlassLook> {
        Some(looks::glass_background(self, glass, env))
    }

    fn button(&self, spec: &ButtonSpec, env: &Environment) -> ButtonLook {
        looks::button(self, spec, env)
    }

    fn switch(&self, spec: &ToggleSpec, env: &Environment) -> SwitchLook {
        looks::switch(self, spec, env)
    }

    fn checkbox(&self, spec: &ToggleSpec, env: &Environment) -> CheckboxLook {
        looks::checkbox(self, spec, env)
    }

    fn stepper(&self, spec: &StepperSpec, env: &Environment) -> StepperLook {
        looks::stepper(self, spec, env)
    }

    fn text_field(&self, spec: &TextFieldSpec, env: &Environment) -> TextFieldLook {
        looks::text_field(self, spec, env)
    }

    fn menu(&self, spec: &MenuSpec, env: &Environment) -> MenuLook {
        looks::menu(self, *spec, env)
    }

    fn menu_picker(&self, spec: &PickerSpec, env: &Environment) -> MenuPickerLook {
        looks::menu_picker(self, spec, env)
    }

    fn radio_group(&self, spec: &PickerSpec, env: &Environment) -> RadioGroupLook {
        looks::radio_group(self, spec, env)
    }

    fn segmented_picker(&self, spec: &SegmentedSpec, env: &Environment) -> SegmentedLook {
        looks::segmented_picker(self, spec, env)
    }

    fn slider(&self, spec: &SliderSpec, env: &Environment) -> SliderLook {
        looks::slider(self, spec, env)
    }

    fn linear_progress(&self, spec: &ProgressSpec, env: &Environment) -> LinearProgressLook {
        looks::linear_progress(self, spec, env)
    }

    fn circular_progress(&self, spec: &ProgressSpec, env: &Environment) -> IndicatorLook {
        looks::circular_progress(self, spec, env)
    }

    fn loading_indicator(&self, spec: &ProgressSpec, env: &Environment) -> IndicatorLook {
        looks::loading(self, spec, env)
    }

    fn navigation_bar(&self, spec: &NavigationSpec, env: &Environment) -> NavigationBarLook {
        looks::navigation_bar(self, *spec, env)
    }

    fn tabs(&self, spec: &TabsSpec, env: &Environment) -> TabsLook {
        looks::tabs(self, *spec, env)
    }

    fn draw_scroll_indicator(&self, draw: &mut Recorder, bounds: Rect) {
        chrome::scroll_indicator(&self.palette, draw, bounds);
    }

    fn divider_metrics(&self) -> DividerMetrics {
        DividerMetrics::new(chrome::HAIRLINE)
    }

    fn draw_divider(&self, draw: &mut Recorder, bounds: Rect) {
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

    fn draw_badge_small(&self, draw: &mut Recorder, bounds: Rect) {
        chrome::badge_small(&self.palette, draw, bounds);
    }

    fn draw_badge_large(&self, draw: &mut Recorder, bounds: Rect) {
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

    fn draw_list_row_background(&self, draw: &mut Recorder, bounds: Rect, alternate: bool) {
        chrome::list_row_background(&self.palette, draw, bounds, alternate);
    }

    fn draw_list_move_control(&self, draw: &mut Recorder, bounds: Rect) {
        chrome::list_move_control(&self.palette, draw, bounds);
    }

    fn draw_list_move_control_state_layer(
        &self,
        draw: &mut Recorder,
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

    fn draw_list_delete_control(&self, draw: &mut Recorder, bounds: Rect) {
        chrome::list_delete_control(&self.palette, draw, bounds);
    }

    fn draw_list_delete_control_state_layer(
        &self,
        draw: &mut Recorder,
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
        draw: &mut Recorder,
        bounds: Rect,
        progress: f64,
        toward_start: bool,
    ) {
        chrome::list_swipe_dismiss_background(&self.palette, draw, bounds, progress, toward_start);
    }

    fn draw_list_row_lifted(&self, draw: &mut Recorder, bounds: Rect, elevation: f64) {
        chrome::list_row_lifted(&self.palette, draw, bounds, elevation);
    }

    fn draw_list_separator(&self, draw: &mut Recorder, bounds: Rect) {
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

    fn draw_table_background(&self, draw: &mut Recorder, bounds: Rect) {
        chrome::table_background(&self.palette, draw, bounds);
    }

    fn draw_table_header_background(&self, draw: &mut Recorder, bounds: Rect) {
        chrome::table_header_background(&self.palette, draw, bounds);
    }

    fn draw_table_cell_border(&self, draw: &mut Recorder, bounds: Rect) {
        chrome::table_cell_border(&self.palette, draw, bounds);
    }

    fn draw_table_column_separator(&self, draw: &mut Recorder, from: Point, to: Point) {
        chrome::table_column_separator(&self.palette, draw, from, to);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cherenkov_record::{Content, LayoutSize};
    use hydrolysis::theme::{Fill, Inputs, SpecTint};
    use waterui_core::layout::LayoutDirection;

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

    fn environment(g: &Glass) -> Environment {
        let mut env = Environment::new();
        g.install_tokens(&mut env);
        env
    }

    fn button_spec(style: ButtonStyle, size: ControlSize) -> ButtonSpec {
        ButtonSpec::new(
            style,
            size,
            false,
            LayoutDirection::LeftToRight,
            SpecTint::default(),
        )
    }

    #[test]
    fn plain_button_draws_no_glass_body() {
        let g = Glass::light();
        let env = environment(&g);
        let b = Rect::new(0.0, 0.0, 100.0, 40.0);
        let plain = g.button(&button_spec(ButtonStyle::Plain, ControlSize::Small), &env);
        assert!(matches!(plain.chrome().surface().fill(), Fill::Paint(_)));
        let glass = g.button(&button_spec(ButtonStyle::Glass, ControlSize::Small), &env);
        assert!(matches!(glass.chrome().surface().fill(), Fill::Backdrop(_)));
        assert!(
            g.button_surface(
                b,
                ButtonStyle::Plain,
                ControlSize::Small,
                WidgetInteractionState::NONE
            )
            .is_none()
        );
    }

    fn layered_recorder(body: impl FnOnce(&mut Recorder)) -> cherenkov_record::LayeredContent {
        Content::record_layered(
            &LayoutSize::new(),
            cherenkov_record::MaterialScope::SOLO,
            body,
        )
    }

    #[test]
    fn registry_declares_the_shader_and_classes() {
        let g = Glass::light();
        let mut registry = MaterialRegistry::new();
        g.register_backdrop_shaders(&mut registry);
        assert!(registry.shader(materials::GLASS_SHADER).is_some());
        // 4 capture scales × standalone and container.
        assert_eq!(registry.capture_classes().count(), 8);
    }

    #[test]
    fn a_glass_button_is_a_backdrop_fill_of_the_standalone_surface() {
        let g = Glass::light();
        let env = environment(&g);
        let look = g.button(&button_spec(ButtonStyle::Glass, ControlSize::Medium), &env);
        let Fill::Backdrop(body) = look.chrome().surface().fill() else {
            panic!("a glass button's body is a backdrop");
        };
        assert_eq!(body.shader(), materials::GLASS_SHADER);
        let frame = Rect::new(0.0, 0.0, 100.0, 40.0);
        let surface = g
            .button_surface(
                frame,
                ButtonStyle::Glass,
                ControlSize::Medium,
                WidgetInteractionState::NONE,
            )
            .expect("glass surface");
        // The button's recipe capture scale snaps up to the smallest
        // registered class at or above it.
        assert_eq!(
            body.capture(),
            materials::capture_class(false, surface.element.recipe.capture_scale),
        );
        // At rest the effect is the standalone surface's own recipe.
        assert_eq!(
            body.effect()
                .target(&Inputs::at_rest(frame, InteractionState::empty(), &())),
            materials::effect(&surface.element),
        );
        // A press grows the silhouette the effect is stated for, and the
        // chrome grows by the same press growth.
        assert_eq!(
            look.chrome().press().get_scale(),
            Some(f64::from(button::GROWTH_PER_PRESS))
        );
    }

    #[test]
    fn tab_bar_and_toolbar_record_backdrop_materials() {
        let g = Glass::light();
        let layered = layered_recorder(|r| {
            record_standalone(r, &g.tab_bar_surface(Rect::new(0.0, 0.0, 390.0, 50.0)));
            record_standalone(r, &g.toolbar_surface(Rect::new(0.0, 0.0, 390.0, 44.0)));
        });
        assert_eq!(layered.runs.len(), 2);
        for run in &layered.runs {
            assert_eq!(run.material.shader(), materials::GLASS_SHADER);
        }
    }
}
