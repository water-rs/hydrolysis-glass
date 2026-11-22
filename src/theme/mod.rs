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
    BadgeLook, ButtonLook, ButtonSpec, CheckboxLook, GlassLook, IndicatorLook, LinearProgressLook,
    ListLook, ListSpec, MenuLook, MenuPickerLook, MenuSpec, NavigationBarLook, NavigationSpec,
    PickerSpec, ProgressSpec, RadioGroupLook, ScrollIndicatorLook, SegmentedLook, SegmentedSpec,
    SliderLook, SliderSpec, StepperLook, StepperSpec, SwitchLook, TableLook, TabsLook, TabsSpec,
    TextFieldLook, TextFieldSpec, ToggleSpec, WidgetTheme,
};
use waterui::Plugin as _;
use waterui::color::{Color, ColorScheme};
use waterui::interaction::InteractionState;
use waterui::reactive::Signal as _;
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

    fn scroll_indicator(&self, _env: &Environment) -> ScrollIndicatorLook {
        looks::scroll_indicator(self)
    }

    fn badge(&self, _env: &Environment) -> BadgeLook {
        looks::badge(self)
    }

    fn list(&self, _spec: &ListSpec, _env: &Environment) -> ListLook {
        looks::list(self)
    }

    fn table(&self, _env: &Environment) -> TableLook {
        looks::table(self)
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
            (extent - expected).abs() < 1e-6,
            "expected popup elevation extent {expected}, got {extent}"
        );
    }

    #[test]
    fn stepper_look_dims_disabled_ends_and_mirrors_rtl_corners() {
        use hydrolysis_style::theme::Corners;

        let g = Glass::light();
        let env = environment(&g);
        let frame = Rect::new(0.0, 0.0, 32.0, 32.0);
        let ltr = g.stepper(
            &StepperSpec::new(LayoutDirection::LeftToRight, SpecTint::default()),
            &env,
        );
        let rtl = g.stepper(
            &StepperSpec::new(LayoutDirection::RightToLeft, SpecTint::default()),
            &env,
        );
        assert_eq!(ltr.button(), cherenkov_record::kurbo::Size::new(32.0, 32.0));
        assert!((ltr.get_spacing() - 1.0).abs() < 1e-9);
        assert!((ltr.get_label_spacing() - 12.0).abs() < 1e-9);
        assert_eq!(
            ltr.decrement().get_hit().get_minimum(),
            cherenkov_record::kurbo::Size::new(0.0, f64::from(button::MIN_HIT_SIDE)),
        );
        assert!(
            (ltr.decrement().get_hit().get_cancel_distance() - f64::from(button::CANCEL_DISTANCE))
                .abs()
                < 1e-9
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
        assert!((metrics.label_height - 20.0).abs() < 1e-9);
        assert!((metrics.min_width - 120.0).abs() < 1e-9);
        assert!((metrics.min_height - 36.0).abs() < 1e-9);
        assert!((metrics.horizontal_inset - 12.0).abs() < 1e-9);
        assert!((metrics.vertical_inset - 8.0).abs() < 1e-9);
        assert!((look.floating_label().get_scale() - 0.75).abs() < 1e-9);
        assert!((look.floating_label().get_top() - 4.0).abs() < 1e-9);
        let blink = look.caret().blink();
        assert_eq!(blink.fade_cycle_duration, Duration::from_millis(1_000));
        assert_eq!(blink.frame_interval, Duration::from_millis(16));
        assert!(blink.min_opacity.abs() < 1e-6);

        let Fill::Paint(fill) = look.container().surface().fill() else {
            panic!("expected paint fill on text field container");
        };
        let Fill::Paint(_) = look.container().surface().fill() else {
            unreachable!();
        };
        let enabled_fill = fill.target(&Inputs::at_rest(frame, InteractionState::empty(), &()));
        let disabled_fill = fill.target(&Inputs::at_rest(frame, InteractionState::DISABLED, &()));
        let (cherenkov_record::Paint::Solid(ef), cherenkov_record::Paint::Solid(df)) =
            (enabled_fill, disabled_fill)
        else {
            panic!("expected solid fill paints");
        };
        assert!(
            ef.components[3].mul_add(-0.5, df.components[3]).abs() < 1e-6,
            "expected disabled container alpha to halve"
        );

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
    fn tabs_look_switches_layout_at_600pt_and_cross_fades_selection() {
        use hydrolysis_style::theme::{TabItemLayout, ValueMotion};
        use waterui::navigation::tab::NativeTabStyle;

        let g = Glass::dark();
        let env = environment(&g);
        let look = g.tabs(&TabsSpec::new(NativeTabStyle::TabBar), &env);
        assert_eq!(look.item_layout(402.0), TabItemLayout::Vertical);
        assert_eq!(look.item_layout(600.0), TabItemLayout::Horizontal);
        let vertical = look.metrics(TabItemLayout::Vertical);
        assert!((vertical.bar_height - 60.0).abs() < 1e-9);
        assert!((vertical.button_min_width - 64.0).abs() < 1e-9);
        let horizontal = look.metrics(TabItemLayout::Horizontal);
        assert!((horizontal.bar_height - 48.0).abs() < 1e-9);
        assert!((horizontal.button_horizontal_inset - 16.0).abs() < 1e-9);
        let ValueMotion::Own(anim) = &look.motion().value else {
            panic!("expected Own value motion on tab bar highlight");
        };
        assert_eq!(
            *anim,
            waterui::animation::Animation::ease_out(PRESS_DURATION)
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
}
