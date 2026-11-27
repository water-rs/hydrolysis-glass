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

use cherenkov_record::kurbo::Rect;
use cherenkov_record::{MaterialRegistry, Recorder};
use hydrolysis_style::Style;
use hydrolysis_style::theme::{
    BadgeLook, ButtonLook, ButtonSpec, CardLook, CheckboxLook, CodeLook, DialogLook, FloatingLook,
    GlassLook, IndicatorLook, LinearProgressLook, ListLook, ListSpec, MenuLook, MenuPickerLook,
    MenuSpec, NavigationBarLook, NavigationSpec, PickerSpec, ProgressSpec, RadioGroupLook,
    ScrollIndicatorLook, SegmentedLook, SegmentedSpec, SheetLook, SliderLook, SliderSpec,
    SnackbarLook, StepperLook, StepperSpec, SwitchLook, TableLook, TabsLook, TabsSpec,
    TextFieldLook, TextFieldSpec, ToggleSpec, WidgetTheme,
};
use nami::Signal as _;
use waterui_controls::ControlSize;
use waterui_controls::button::ButtonStyle;
use waterui_controls::card::CardStyle;
use waterui_controls::theme::{ColorSettings, FontSettings, Theme, installed_color_scheme};
use waterui_core::Environment;
use waterui_core::plugin::Plugin as _;
use waterui_graphics::color::{Color, ColorScheme};
use waterui_interaction::interaction::InteractionState;
use waterui_text::font::{FontWeight, ResolvedFont};

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
        state: InteractionState,
    ) -> Option<Surface> {
        let kind = surfaces::button_kind(style)?;
        let spec = surfaces::ButtonSpec {
            enabled: !state.contains(InteractionState::DISABLED),
            ..surfaces::ButtonSpec::new(kind, button::SizeGroup::of(size))
        };
        let press = surfaces::Press {
            progress: f32::from(u8::from(state.contains(InteractionState::PRESSED))),
            // The renderer hands the chrome the touch in window space; the
            // press is centred on the frame until it is local.
            touch_x: None,
            touch_y: None,
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
        Color::from(waterui_graphics::color::Working(resolved(c)))
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
                    .error_foreground(resolved(p.error_foreground))
                    .scrim(resolved(p.scrim)),
            )
            .fonts(
                FontSettings::new()
                    .title(ResolvedFont::new(28.0, FontWeight::Normal))
                    .headline(ResolvedFont::new(17.0, FontWeight::SemiBold))
                    .body(ResolvedFont::new(17.0, FontWeight::Normal))
                    .subheadline(ResolvedFont::new(15.0, FontWeight::Normal))
                    .footnote(ResolvedFont::new(13.0, FontWeight::Normal))
                    .caption(ResolvedFont::new(12.0, FontWeight::Normal)),
            )
            .install(env);
    }
}

impl WidgetTheme for Glass {
    fn register_backdrop_shaders(&self, registry: &mut MaterialRegistry) {
        materials::register(registry);
    }

    fn glass_background(
        &self,
        glass: &waterui_shape::background::Glass,
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

    fn scroll_indicator(&self, _env: &Environment) -> ScrollIndicatorLook {
        looks::scroll_indicator(self)
    }

    fn badge(&self, _env: &Environment) -> BadgeLook {
        looks::badge(self)
    }

    fn card(&self, style: CardStyle, env: &Environment) -> CardLook {
        looks::card(self, style, env)
    }

    fn code(&self, _env: &Environment) -> CodeLook {
        looks::code(self)
    }

    fn floating(&self, env: &Environment) -> FloatingLook {
        looks::floating(self, env)
    }

    fn list(&self, _spec: &ListSpec, _env: &Environment) -> ListLook {
        looks::list(self)
    }

    fn table(&self, _env: &Environment) -> TableLook {
        looks::table(self)
    }

    fn snackbar(&self, _env: &Environment) -> SnackbarLook {
        looks::snackbar()
    }

    fn dialog(&self, _env: &Environment) -> DialogLook {
        looks::dialog(self)
    }

    fn sheet(&self, _env: &Environment) -> SheetLook {
        looks::sheet(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cherenkov_record::{Content, LayoutSize};
    use hydrolysis_style::theme::{Fill, Inputs, SpecTint};
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
                InteractionState::empty(),
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
        assert!(
            registry
                .content_shader(materials::GLASS_CONTENT_SHADER)
                .is_some()
        );
        // 4 capture scales × standalone and container.
        assert_eq!(registry.capture_classes().count(), 8);
    }

    #[test]
    fn bordered_button_is_a_track_filled_capsule_with_accent_label_and_regular_padding() {
        use hydrolysis_style::theme::Corners;

        let g = Glass::light();
        let env = environment(&g);
        let look = g.button(
            &button_spec(ButtonStyle::Bordered, ControlSize::Small),
            &env,
        );
        let frame = Rect::new(0.0, 0.0, 96.0, 34.33);
        let corners = look
            .chrome()
            .surface()
            .shape()
            .corners()
            .target(&Inputs::at_rest(frame, InteractionState::empty(), &()));
        assert!(
            matches!(corners, Corners::Full),
            "expected capsule corners on bordered button, got {corners:?}"
        );
        let Fill::Paint(fill) = look.chrome().surface().fill() else {
            panic!("expected track paint fill on bordered button");
        };
        let cherenkov_record::Paint::Solid(fill_color) =
            fill.target(&Inputs::at_rest(frame, InteractionState::empty(), &()))
        else {
            panic!("expected solid fill paint");
        };
        assert!((fill_color.components[3] - 0.228).abs() < 1e-4);
        let dark = Glass::dark();
        let dark_env = environment(&dark);
        let dark_look = dark.button(
            &button_spec(ButtonStyle::Bordered, ControlSize::Small),
            &dark_env,
        );
        let Fill::Paint(dark_fill) = dark_look.chrome().surface().fill() else {
            panic!("expected track paint fill on dark bordered button");
        };
        let cherenkov_record::Paint::Solid(dark_color) =
            dark_fill.target(&Inputs::at_rest(frame, InteractionState::empty(), &()))
        else {
            panic!("expected solid fill paint on dark bordered button");
        };
        assert!((dark_color.components[3] - 0.105).abs() < 1e-4);
        let label_color = look
            .label()
            .get_color()
            .expect("label color")
            .target(&Inputs::at_rest(frame, InteractionState::empty(), &()))
            .resolve(&env)
            .snapshot();
        assert!(label_color.components[2] > 0.9 && label_color.components[0] < 0.05);
        assert_eq!(
            look.get_padding(),
            cherenkov_record::kurbo::Vec2::new(12.5, 7.0)
        );
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
                InteractionState::empty(),
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

    #[test]
    fn menu_look_reports_elevation_extent_for_popup_shadow() {
        let g = Glass::light();
        let env = environment(&g);
        let look = g.menu(&MenuSpec::new(LayoutDirection::LeftToRight), &env);
        let extent = look.panel_extent(Rect::new(0.0, 0.0, 140.0, 120.0));
        let expected = chrome::POPUP_SHADOW_BLUR.mul_add(3.0, chrome::POPUP_SHADOW_OFFSET);
        assert!(
            (f64::from(extent) - expected).abs() < 1e-6,
            "expected popup elevation extent {expected}, got {extent}"
        );
    }

    #[test]
    fn stepper_look_dims_disabled_ends_and_mirrors_rtl_corners() {
        use hydrolysis_style::theme::Corners;

        let g = Glass::light();
        let env = environment(&g);
        let frame = Rect::new(0.0, 0.0, 46.5, 32.0);
        let ltr = g.stepper(
            &StepperSpec::new(LayoutDirection::LeftToRight, SpecTint::default()),
            &env,
        );
        let rtl = g.stepper(
            &StepperSpec::new(LayoutDirection::RightToLeft, SpecTint::default()),
            &env,
        );
        assert_eq!(ltr.button(), cherenkov_record::kurbo::Size::new(46.5, 32.0));
        assert!(ltr.get_spacing().abs() < 1e-9);
        assert!((ltr.get_label_spacing() - 12.0).abs() < 1e-9);
        assert_eq!(
            ltr.decrement().get_hit().get_minimum(),
            cherenkov_record::kurbo::Size::new(0.0, f64::from(button::MIN_HIT_SIDE)),
        );
        assert!(
            (ltr.decrement().get_hit().get_cancel_distance() - button::CANCEL_DISTANCE).abs()
                < 1e-6
        );

        let radii_of = |chrome: &hydrolysis_style::theme::Chrome| {
            let corner = chrome.surface().shape().corners().target(&Inputs::at_rest(
                frame,
                InteractionState::empty(),
                &(),
            ));
            let Corners::Radii(radii) = corner else {
                panic!("expected concrete radii");
            };
            radii
        };
        let ltr_dec = radii_of(ltr.decrement());
        assert!((ltr_dec.top_left - 16.0).abs() < 1e-9);
        assert!(ltr_dec.top_right.abs() < 1e-9);
        assert!(ltr_dec.bottom_right.abs() < 1e-9);
        assert!((ltr_dec.bottom_left - 16.0).abs() < 1e-9);

        let rtl_dec = radii_of(rtl.decrement());
        assert!(rtl_dec.top_left.abs() < 1e-9);
        assert!((rtl_dec.top_right - 16.0).abs() < 1e-9);
        assert!((rtl_dec.bottom_right - 16.0).abs() < 1e-9);
        assert!(rtl_dec.bottom_left.abs() < 1e-9);

        let Fill::Paint(fill) = ltr.decrement().surface().fill() else {
            panic!("expected paint fill on stepper end");
        };
        let enabled_fill = fill.target(&Inputs::at_rest(frame, InteractionState::empty(), &()));
        let disabled_fill = fill.target(&Inputs::at_rest(frame, InteractionState::DISABLED, &()));
        let cherenkov_record::Paint::Solid(enabled_color) = enabled_fill else {
            panic!("expected solid paint");
        };
        let cherenkov_record::Paint::Solid(disabled_color) = disabled_fill else {
            panic!("expected solid paint");
        };
        assert!(
            enabled_color.components[3]
                .mul_add(-0.5, disabled_color.components[3])
                .abs()
                < 1e-6,
            "expected disabled fill alpha to halve"
        );
    }

    #[test]
    fn text_field_look_dims_disabled_states_and_specifies_metrics_and_blink() {
        use core::time::Duration;

        let g = Glass::light();
        let env = environment(&g);
        let frame = Rect::new(0.0, 0.0, 300.0, 60.0);
        let look = g.text_field(
            &TextFieldSpec::new(LayoutDirection::LeftToRight, SpecTint::default()),
            &env,
        );
        let metrics = look.metrics();
        assert!((metrics.label_height - 25.0).abs() < 1e-9);
        assert!((metrics.min_width - 120.0).abs() < 1e-9);
        assert!((metrics.min_height - 36.0).abs() < 1e-9);
        assert!(metrics.horizontal_inset.abs() < 1e-9);
        assert!((metrics.vertical_inset - 6.0).abs() < 1e-9);
        assert!((look.floating_label().get_scale() - 1.0).abs() < 1e-9);
        assert!((look.floating_label().get_top() - 6.0).abs() < 1e-6);
        let blink = look.caret().blink();
        assert_eq!(blink.fade_cycle_duration, Duration::from_millis(1_000));
        assert_eq!(blink.frame_interval, Duration::from_millis(16));
        assert!(blink.min_opacity.abs() < 1e-6);

        let Fill::Paint(fill) = look.container().surface().fill() else {
            panic!("expected paint fill on text field container");
        };
        let enabled_fill = fill.target(&Inputs::at_rest(frame, InteractionState::empty(), &()));
        let cherenkov_record::Paint::Solid(ef) = enabled_fill else {
            panic!("expected solid fill paint");
        };
        assert!(
            ef.components[3].abs() < 1e-6,
            "expected borderless transparent resting fill"
        );
        assert!(look.container().surface().get_border().is_none());

        let assert_color_halves = |driven: &hydrolysis_style::theme::Driven<Color>, name: &str| {
            let enabled = driven
                .target(&Inputs::at_rest(frame, InteractionState::empty(), &()))
                .resolve(&env)
                .snapshot();
            let disabled = driven
                .target(&Inputs::at_rest(frame, InteractionState::DISABLED, &()))
                .resolve(&env)
                .snapshot();
            assert!(
                enabled.components[3]
                    .mul_add(-0.5, disabled.components[3])
                    .abs()
                    < 1e-6,
                "expected disabled {name} alpha to halve"
            );
        };
        assert_color_halves(
            look.floating_label()
                .label()
                .get_color()
                .expect("floating label color"),
            "floating label",
        );
        assert_color_halves(
            look.get_placeholder()
                .get_color()
                .expect("placeholder color"),
            "placeholder",
        );
        assert_color_halves(
            look.get_value()
                .expect("value label")
                .get_color()
                .expect("value color"),
            "value",
        );
    }

    #[test]
    fn light_palette_uses_white_background_and_grouped_surface_polarity() {
        use waterui_core::resolve::Resolvable as _;

        assert_eq!(Palette::LIGHT.background, [1.0, 1.0, 1.0, 1.0]);
        assert_eq!(Palette::LIGHT.surface, [0.949, 0.949, 0.969, 1.0]);
        assert_eq!(Palette::LIGHT.surface_variant, [0.949, 0.949, 0.969, 1.0]);

        let mut env = Environment::new();
        Glass::light().install_tokens(&mut env);
        let bg = waterui_controls::theme::color::Background
            .resolve(&env)
            .snapshot();
        let surf = waterui_controls::theme::color::Surface
            .resolve(&env)
            .snapshot();
        assert!((bg.components[0] - 1.0).abs() < 1e-4 && (bg.components[2] - 1.0).abs() < 1e-4);
        assert!(surf.components[0] < bg.components[0] && surf.components[2] > surf.components[0]);
    }

    #[test]
    fn dark_palette_uses_oled_black_background_and_surface_hierarchy() {
        use waterui_core::resolve::Resolvable as _;

        assert_eq!(Palette::DARK.background, [0.0, 0.0, 0.0, 1.0]);
        assert_eq!(
            Palette::DARK.surface,
            [28.0 / 255.0, 28.0 / 255.0, 30.0 / 255.0, 1.0]
        );
        assert_eq!(
            Palette::DARK.surface_variant,
            [44.0 / 255.0, 44.0 / 255.0, 46.0 / 255.0, 1.0]
        );
        assert_eq!(Palette::DARK.foreground, [1.0, 1.0, 1.0, 1.0]);
        assert_eq!(Palette::LIGHT.foreground, [0.0, 0.0, 0.0, 1.0]);

        let mut env = Environment::new();
        Glass::dark().install_tokens(&mut env);
        let bg = waterui_controls::theme::color::Background
            .resolve(&env)
            .snapshot();
        let surf = waterui_controls::theme::color::Surface
            .resolve(&env)
            .snapshot();
        let variant = waterui_controls::theme::color::SurfaceVariant
            .resolve(&env)
            .snapshot();
        assert_eq!(bg.components[0], 0.0);
        assert!(bg.components[0] < surf.components[0]);
        assert!(surf.components[0] < variant.components[0]);
    }

    #[test]
    fn tinted_button_preserves_display_p3_gamut() {
        use waterui_controls::theme::Tint;
        use waterui_graphics::color::P3;

        let g = Glass::dark();
        let mut env = environment(&g);
        env.insert(Tint::new(Color::new(P3::new(1.0, 0.0, 0.0))));
        let spec = ButtonSpec::new(
            ButtonStyle::Glass,
            ControlSize::Medium,
            false,
            LayoutDirection::LeftToRight,
            SpecTint::resolve(&env),
        );
        let look = g.button(&spec, &env);
        let resolved = look
            .label()
            .get_color()
            .expect("label color")
            .target(&Inputs::at_rest(
                Rect::new(0.0, 0.0, 96.0, 34.0),
                InteractionState::empty(),
                &(),
            ))
            .resolve(&env)
            .snapshot();
        assert!((resolved.components[0] - 1.0).abs() < 1e-4);
        assert!(resolved.components[1].abs() < 1e-4);
        assert!(resolved.components[2].abs() < 1e-4);
    }

    #[test]
    fn tabs_look_installs_10pt_medium_label_font() {
        use waterui_navigation::tab::TabStyle;

        let g = Glass::dark();
        let env = environment(&g);
        let look = g.tabs(&TabsSpec::new(TabStyle::TabBar), &env);
        let label_font = look
            .get_label_font()
            .expect("tab bar installs 10 pt medium label font")
            .resolve(&env)
            .snapshot();
        assert!((label_font.size() - 10.0).abs() < f32::EPSILON);
        assert_eq!(label_font.weight(), FontWeight::Medium);
    }

    #[test]
    fn tabs_look_switches_layout_at_600pt_and_drives_selection_and_press_lens() {
        use hydrolysis_style::theme::{
            TabBarWidth, TabItemGeometry, TabItemLayout, TabsValue, ValueMotion,
        };
        use waterui_navigation::tab::TabStyle;

        let g = Glass::dark();
        let env = environment(&g);
        let look = g.tabs(&TabsSpec::new(TabStyle::TabBar), &env);
        assert_eq!(look.item_layout(402.0), TabItemLayout::Vertical);
        assert_eq!(look.item_layout(600.0), TabItemLayout::Horizontal);
        assert!(look.get_drag().is_some());
        assert_eq!(
            look.get_bar_width(),
            TabBarWidth::Compact {
                item_min_width: 94.0,
                max_width: None,
                horizontal_padding: 0.0,
            }
        );
        assert!((look.get_floating_margin() - 21.0).abs() < 1e-9);
        let selected_fg = look
            .get_selected_foreground()
            .expect("selected tab foreground")
            .resolve(&env)
            .snapshot();
        assert_eq!(selected_fg, resolved(g.palette().accent));
        let unselected_fg = look
            .get_unselected_foreground()
            .expect("unselected tab foreground")
            .resolve(&env)
            .snapshot();
        assert_eq!(unselected_fg, resolved(g.palette().foreground));
        let vertical = look.metrics(TabItemLayout::Vertical);
        assert!((vertical.bar_height - 62.0).abs() < 1e-9);
        assert!((vertical.button_min_width - 64.0).abs() < 1e-9);
        let horizontal = look.metrics(TabItemLayout::Horizontal);
        assert!((horizontal.bar_height - 48.0).abs() < 1e-9);
        assert!((horizontal.button_horizontal_inset - 16.0).abs() < 1e-9);
        let ValueMotion::Always(anim) = &look.motion().value else {
            panic!("expected Always value motion on tab bar highlight");
        };
        assert_eq!(
            *anim,
            looks::spring_animation(crate::dynamics::Spring::SEGMENTED_PILL)
        );

        let bar = Rect::new(0.0, 0.0, 200.0, 60.0);
        let items: std::rc::Rc<[TabItemGeometry]> = [
            TabItemGeometry {
                bounds: Rect::new(0.0, 0.0, 100.0, 60.0),
                icon: None,
                title: Rect::new(16.0, 20.0, 84.0, 40.0),
                layout: TabItemLayout::Vertical,
                placement: TabStyle::TabBar,
            },
            TabItemGeometry {
                bounds: Rect::new(100.0, 0.0, 200.0, 60.0),
                icon: None,
                title: Rect::new(116.0, 20.0, 184.0, 40.0),
                layout: TabItemLayout::Vertical,
                placement: TabStyle::TabBar,
            },
        ]
        .into();
        let value = TabsValue {
            selected: Some(0),
            position: Some(0.5),
            items,
        };
        let rest_indicator = look
            .get_selection_indicator()
            .expect("bar-level selection indicator");
        let lens_indicator = look
            .get_selection_indicator_over_labels()
            .expect("bar-level over-labels lens indicator");

        let rest_lens =
            lens_indicator.realize(&Inputs::at_rest(bar, InteractionState::SELECTED, &value));
        assert_eq!(rest_lens.opacity().expect("lens opacity").snapshot(), 0.0);
        let pressed_lens = lens_indicator.realize(&Inputs::at_rest(
            bar,
            InteractionState::SELECTED.union(InteractionState::PRESSED),
            &value,
        ));
        assert_eq!(
            pressed_lens.opacity().expect("lens opacity").snapshot(),
            1.0
        );
        let layered = layered_recorder(|r| {
            pressed_lens.record(r, bar);
        });
        assert_eq!(layered.runs.len(), 1);
        assert_eq!(layered.runs[0].material.shader(), materials::GLASS_SHADER);
        assert!(
            rest_indicator
                .realize(&Inputs::at_rest(bar, InteractionState::SELECTED, &value))
                .opacity()
                .is_none()
        );
    }

    #[test]
    fn navigation_bar_look_enables_back_feedback_and_metrics() {
        use hydrolysis_style::theme::FocusIndicator;
        use waterui_text::font::FontWeight;

        let g = Glass::dark();
        let env = environment(&g);
        let look = g.navigation_bar(&NavigationSpec::new(LayoutDirection::LeftToRight), &env);
        let title_font = look
            .get_title_font()
            .expect("inline navigation title font")
            .resolve(&env)
            .snapshot();
        assert!((title_font.size() - 17.0).abs() < 1e-4);
        assert_eq!(title_font.weight(), FontWeight::SemiBold);
        let metrics = look.metrics();
        assert!((metrics.automatic_bar_height - 64.0).abs() < 1e-9);
        assert!((metrics.inline_bar_height - 44.0).abs() < 1e-9);
        assert!((metrics.large_bar_height - 96.0).abs() < 1e-9);
        assert!((metrics.horizontal_inset - 16.0).abs() < 1e-9);
        assert!((metrics.item_spacing - 8.0).abs() < 1e-9);
        assert!((metrics.back_button_size - 44.0).abs() < 1e-9);
        assert!((metrics.back_button_leading_inset - 8.0).abs() < 1e-9);
        let back = look.back();
        assert!(
            (back.state_layer().hover - chrome::HOVER_ALPHA).abs() < 1e-6,
            "back button carries hover state opacity"
        );
        assert!(
            (back.state_layer().pressed - chrome::PRESSED_ALPHA).abs() < 1e-6,
            "back button carries pressed state opacity"
        );
        assert!(matches!(back.focus(), FocusIndicator::Ring { .. }));
        assert_eq!(
            back.get_hit().get_minimum(),
            cherenkov_record::kurbo::Size::new(
                f64::from(button::MIN_HIT_SIDE),
                f64::from(button::MIN_HIT_SIDE)
            )
        );
        assert!((back.get_hit().get_cancel_distance() - button::CANCEL_DISTANCE).abs() < 1e-6);

        let bounds = Rect::new(0.0, 0.0, 44.0, 44.0);
        let record_chevron = |rtl: bool| {
            layered_recorder(|r| {
                chrome::back_button_directed(&g.palette, r, bounds, rtl);
            })
        };
        let ltr_chevron = format!("{:?}", record_chevron(false));
        let rtl_chevron = format!("{:?}", record_chevron(true));
        assert_ne!(ltr_chevron, rtl_chevron, "RTL mirrors the back chevron");

        let bar_surface = hydrolysis_style::theme::BarSurface {
            color: resolved(g.palette().surface),
        };
        let bar_frame = Rect::new(0.0, 0.0, 390.0, 44.0);
        let realized_bar = look.bar().realize(&Inputs::at_rest(
            bar_frame,
            InteractionState::empty(),
            &bar_surface,
        ));
        let layered_bar = layered_recorder(|r| {
            realized_bar.record(r, bar_frame);
        });
        assert!(
            layered_bar.runs.is_empty(),
            "resting navigation bar draws no opaque or backdrop surface"
        );
    }

    #[test]
    fn menu_look_specifies_panel_layout_and_selected_row() {
        let g = Glass::light();
        let env = environment(&g);
        let look = g.menu(&MenuSpec::new(LayoutDirection::LeftToRight), &env);
        let layout = look.layout();
        assert!((layout.row_height - 45.0).abs() < 1e-9);
        assert!((layout.horizontal_padding - 20.0).abs() < 1e-9);
        assert!((layout.vertical_padding - 8.0).abs() < 1e-9);
        assert!((layout.min_width - 250.0).abs() < 1e-9);
        assert!((layout.max_width - 280.0).abs() < 1e-9);
        assert!((layout.width_per_char - 8.0).abs() < 1e-9);
        assert!((layout.separator_horizontal_inset - 20.0).abs() < 1e-9);
        assert!((layout.separator_thickness - chrome::HAIRLINE).abs() < 1e-9);
        assert!(look.has_selected_row());
        assert!(matches!(look.panel().fill(), Fill::Paint(_)));
        assert!(look.panel().get_border().is_some());
        let motion = look
            .get_presentation_motion()
            .expect("popover presentation motion");
        assert!(motion.present_vertical.is_some());
        assert!(motion.dismiss_vertical.is_some());
        assert!((motion.content_scale_from - 0.75).abs() < 1e-9);
        assert!((motion.content_blur_from - 15.0).abs() < 1e-9);
        assert_eq!(motion.content_opacity_window, (0.62, 0.98));
        assert_eq!(motion.dismiss_content_opacity_window, Some((0.55, 1.0)));
        assert_eq!(motion.content_scale_window, (0.15, 0.92));
        assert_eq!(motion.content_blur_window, (0.55, 0.98));
        assert_eq!(motion.dismiss_content_blur_window, Some((0.65, 1.0)));
        assert_eq!(motion.trigger_opacity_window, Some((0.55, 0.96)));
        assert_eq!(motion.dismiss_trigger_opacity_window, Some((0.90, 1.0)));
        assert_eq!(motion.trigger_chrome_opacity_window, Some((0.0, 0.0)));
        assert_eq!(motion.surface_corner_radius, Some(26.0));
        assert!(motion.cover_trigger);
        assert!(look.has_presentation_panel());
        let sample = hydrolysis_style::theme::MenuPresentationSample::at_rest(
            Rect::new(20.0, 20.0, 100.0, 54.0),
            Rect::new(20.0, 60.0, 200.0, 220.0),
        );
        let layered = layered_recorder(|r| {
            assert!(look.record_presentation_panel(r, &sample));
        });
        assert_eq!(layered.runs.len(), 1);
        assert_eq!(layered.runs[0].material.shader(), materials::GLASS_SHADER);
    }

    #[test]
    fn glass_button_and_segmented_wire_capped_growth_jelly_drag_and_label_hiding() {
        use hydrolysis_style::theme::SegmentedSpec;

        let g = Glass::light();
        let env = environment(&g);

        let btn = g.button(&button_spec(ButtonStyle::Glass, ControlSize::Medium), &env);
        let press = btn.chrome().press();
        assert_eq!(press.get_max_growth(), Some(10.0));
        assert!(press.get_drag_deformation().is_some());
        assert!(
            (press.effective_scale(cherenkov_record::kurbo::Size::new(322.0, 34.33))
                - (1.0 + 10.0 / 161.0))
                .abs()
                < 1e-9
        );

        let seg = g.segmented_picker(
            &SegmentedSpec::new(
                ControlSize::Medium,
                3,
                LayoutDirection::LeftToRight,
                SpecTint::default(),
            ),
            &env,
        );
        assert!(!seg.shows_label());
    }

    #[test]
    fn menu_picker_look_dims_disabled_value_and_container() {
        let g = Glass::dark();
        let env = environment(&g);
        let frame = Rect::new(0.0, 0.0, 140.0, 56.0);
        let look = g.menu_picker(
            &PickerSpec::new(
                ControlSize::Medium,
                LayoutDirection::LeftToRight,
                SpecTint::default(),
            ),
            &env,
        );
        let layout = look.layout();
        assert!((layout.min_width - 120.0).abs() < 1e-9);
        assert!((layout.min_height - 36.0).abs() < 1e-9);
        assert!((layout.horizontal_inset - 12.0).abs() < 1e-9);
        assert!((layout.vertical_inset - 8.0).abs() < 1e-9);
        assert!((layout.label_spacing - 8.0).abs() < 1e-9);
        assert!((layout.indicator_space - 24.0).abs() < 1e-9);
        assert!((layout.popup_gap - 6.0).abs() < 1e-9);

        let Fill::Paint(fill) = look.chrome().surface().fill() else {
            panic!("expected paint fill on menu picker container");
        };
        let enabled_fill = fill.target(&Inputs::at_rest(frame, InteractionState::empty(), &()));
        let disabled_fill = fill.target(&Inputs::at_rest(frame, InteractionState::DISABLED, &()));
        let (cherenkov_record::Paint::Solid(ef), cherenkov_record::Paint::Solid(df)) =
            (enabled_fill, disabled_fill)
        else {
            panic!("expected solid fill paints on menu picker container");
        };
        assert!(
            ef.components[3].mul_add(-0.5, df.components[3]).abs() < 1e-6,
            "expected disabled menu picker container alpha to halve"
        );

        let value_color = look
            .get_value()
            .get_color()
            .expect("menu picker value label color");
        let enabled_value = value_color
            .target(&Inputs::at_rest(frame, InteractionState::empty(), &()))
            .resolve(&env)
            .snapshot();
        let disabled_value = value_color
            .target(&Inputs::at_rest(frame, InteractionState::DISABLED, &()))
            .resolve(&env)
            .snapshot();
        assert!(
            enabled_value.components[3]
                .mul_add(-0.5, disabled_value.components[3])
                .abs()
                < 1e-6,
            "expected disabled menu picker value alpha to halve"
        );
    }

    #[test]
    fn sheet_surface_uses_regular_glass_and_clamps_top_corners() {
        use crate::material::geometry::CornerCurve;
        use crate::material::recipe::{Family, Recipe};

        for g in [Glass::dark(), Glass::light()] {
            let full = g.sheet_surface(Rect::new(0.0, 200.0, 402.0, 874.0));
            assert_eq!(full.role, surfaces::Role::Sheet);
            assert_eq!(
                full.element.recipe,
                Recipe::for_size(
                    402.0,
                    Family::Regular,
                    g.palette().appearance,
                    g.translucency()
                )
            );
            assert_eq!(full.element.shape.curve, CornerCurve::Continuous);
            assert_eq!(full.element.shape.radii, [38.0, 38.0, 0.0, 0.0]);
            assert!(full.wash.is_none());

            let compact = g.sheet_surface(Rect::new(0.0, 814.0, 402.0, 874.0));
            assert_eq!(compact.element.shape.radii, [30.0, 30.0, 0.0, 0.0]);

            let layered = layered_recorder(|r| {
                record_standalone(r, &full);
            });
            assert_eq!(layered.runs.len(), 1);
            assert_eq!(layered.runs[0].material.shader(), materials::GLASS_SHADER);
        }
    }

    #[test]
    fn card_look_uses_glass_container_and_style_variants() {
        let frame = Rect::new(0.0, 0.0, 240.0, 120.0);
        let rest = Inputs::at_rest(frame, InteractionState::empty(), &());

        for g in [Glass::light(), Glass::dark()] {
            let env = environment(&g);

            let automatic = g.card(CardStyle::Automatic, &env);
            assert!(matches!(automatic.surface().fill(), Fill::Backdrop(_)));
            assert!(automatic.surface().get_content_effect().is_some());
            assert!(automatic.surface().get_border().is_none());
            assert!(automatic.surface().get_elevation().target(&rest).is_none());

            let elevated = g.card(CardStyle::Elevated, &env);
            assert!(matches!(elevated.surface().fill(), Fill::Backdrop(_)));
            assert!(!elevated.surface().get_elevation().target(&rest).is_none());

            let filled = g.card(CardStyle::Filled, &env);
            assert!(matches!(filled.surface().fill(), Fill::Paint(_)));
            assert!(filled.surface().get_border().is_none());

            let outlined = g.card(CardStyle::Outlined, &env);
            assert!(matches!(outlined.surface().fill(), Fill::Paint(_)));
            let border = outlined
                .surface()
                .get_border()
                .expect("outlined card carries a hairline border")
                .target(&rest);
            assert!((border.width - chrome::HAIRLINE).abs() < 1e-9);
        }
    }

    #[test]
    fn dialog_and_sheet_looks_use_glass_surface_radii_and_palette() {
        use waterui_core::resolve::Resolvable as _;
        use waterui_text::font::{Body, Caption, Footnote, Headline, Subheadline, Title};

        for (g, expected_scrim_alpha) in [(Glass::light(), 0.32_f32), (Glass::dark(), 0.50_f32)] {
            let env = environment(&g);

            // §8.0 type scale and Scrim token installed by `install_tokens`.
            let body = Body.resolve(&env).snapshot();
            assert!((body.size() - 17.0).abs() < 1e-4);
            assert_eq!(body.weight(), FontWeight::Normal);
            let headline = Headline.resolve(&env).snapshot();
            assert!((headline.size() - 17.0).abs() < 1e-4);
            assert_eq!(headline.weight(), FontWeight::SemiBold);
            assert!((Title.resolve(&env).snapshot().size() - 28.0).abs() < 1e-4);
            assert!((Subheadline.resolve(&env).snapshot().size() - 15.0).abs() < 1e-4);
            assert!((Footnote.resolve(&env).snapshot().size() - 13.0).abs() < 1e-4);
            assert!((Caption.resolve(&env).snapshot().size() - 12.0).abs() < 1e-4);

            let scrim = waterui_controls::theme::color::Scrim
                .resolve(&env)
                .snapshot();
            assert!((scrim.components[3] - expected_scrim_alpha).abs() < 1e-4);

            let dialog = g.dialog(&env);
            assert!(
                (dialog.corner_radius - surfaces::CONTAINER_CORNER_RADIUS).abs() < f32::EPSILON
            );
            assert!(dialog.min_width > 0.0 && dialog.max_width > dialog.min_width);

            // §8.10 sheet chrome metrics and grabber alpha (`0.60 * 0.40 = 0.24`).
            let sheet = g.sheet(&env);
            assert!((sheet.max_width - 640.0).abs() < f32::EPSILON);
            assert!((sheet.corner_radius - surfaces::SHEET_CORNER_RADIUS).abs() < f32::EPSILON);
            assert!(sheet.show_drag_handle);
            assert!((sheet.drag_handle_width - 36.0).abs() < f32::EPSILON);
            assert!((sheet.drag_handle_height - 5.0).abs() < f32::EPSILON);
            assert!((sheet.drag_handle_vertical_padding - 8.0).abs() < f32::EPSILON);
            let handle_color = sheet.drag_handle_color.resolve(&env).snapshot();
            assert!((handle_color.components[3] - 0.24).abs() < 1e-3);
        }
    }

    #[test]
    fn floating_look_is_a_continuous_glass_surface_with_elevation() {
        let g = Glass::light();
        let env = environment(&g);
        let look = g.floating(&env);

        let Fill::Backdrop(backdrop) = look.surface().fill() else {
            panic!("expected glass backdrop fill on floating look");
        };
        assert_eq!(backdrop.shader(), materials::GLASS_SHADER);
        assert_eq!(
            look.surface()
                .get_content_effect()
                .expect("floating glass content effect")
                .shader(),
            materials::GLASS_CONTENT_SHADER
        );
        let small = Rect::new(0.0, 0.0, 32.0, 32.0);
        let large = Rect::new(0.0, 0.0, 200.0, 120.0);
        let elevation = look.surface().get_elevation().target(&Inputs::at_rest(
            large,
            InteractionState::empty(),
            &(),
        ));
        assert!(
            elevation.key.sigma > 0.0,
            "floating glass surface carries a drop shadow"
        );
        assert_eq!(look.minimum_width(), button::MIN_HIT_SIDE);
        assert_eq!(look.minimum_height(), button::MIN_HIT_SIDE);

        let corners = look.surface().shape().corners();
        assert_eq!(
            corners
                .target(&Inputs::at_rest(small, InteractionState::empty(), &()))
                .radii_on(small)
                .top_left,
            16.0
        );
        assert_eq!(
            corners
                .target(&Inputs::at_rest(large, InteractionState::empty(), &()))
                .radii_on(large)
                .top_left,
            22.0
        );
    }

    #[test]
    fn code_look_matches_specification_surface_metrics_and_typography() {
        use waterui_text::font::FontDesign;

        let frame = Rect::new(0.0, 0.0, 320.0, 120.0);
        for g in [Glass::light(), Glass::dark()] {
            let env = environment(&g);
            let look = g.code(&env);

            let metrics = look.metrics();
            assert!((metrics.horizontal_padding - 14.0).abs() < f32::EPSILON);
            assert!((metrics.vertical_padding - 14.0).abs() < f32::EPSILON);
            assert!((metrics.header_spacing - 8.0).abs() < f32::EPSILON);

            let corners = look.surface().shape().corners();
            assert!(
                (corners
                    .target(&Inputs::at_rest(frame, InteractionState::empty(), &()))
                    .radii_on(frame)
                    .top_left
                    - 12.0)
                    .abs()
                    < 1e-6
            );

            let Fill::Paint(fill) = look.surface().fill() else {
                panic!("expected solid SurfaceVariant fill on code block");
            };
            let cherenkov_record::Paint::Solid(fill_color) =
                fill.target(&Inputs::at_rest(frame, InteractionState::empty(), &()))
            else {
                panic!("expected solid fill paint on code block");
            };
            assert_eq!(fill_color, palette::working(g.palette().surface_variant));

            let border = look
                .surface()
                .get_border()
                .expect("code block carries a hairline border")
                .target(&Inputs::at_rest(frame, InteractionState::empty(), &()));
            assert!((border.width - 0.5).abs() < f32::EPSILON);
            assert_eq!(
                border.paint,
                cherenkov_record::Paint::Solid(palette::working(g.palette().border))
            );

            let header_font = look.header_font_ref().resolve(&env).snapshot();
            assert!((header_font.size() - 12.0).abs() < f32::EPSILON);
            assert_eq!(header_font.weight(), FontWeight::Medium);

            let body_font = look.body_font_ref().resolve(&env).snapshot();
            assert!((body_font.size() - 17.0).abs() < f32::EPSILON);
            assert_eq!(body_font.design(), FontDesign::Monospaced);

            assert_eq!(
                look.header_color_ref().resolve(&env).snapshot(),
                palette::working(g.palette().muted_foreground)
            );
            assert_eq!(
                look.copy_color_ref().resolve(&env).snapshot(),
                palette::working(g.palette().muted_foreground)
            );
            assert_eq!(
                look.copied_color_ref().resolve(&env).snapshot(),
                palette::working(g.palette().accent)
            );
        }
    }
}
