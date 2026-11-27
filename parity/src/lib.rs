//! Standard `#[preview]` fixture targets for comparing reference native
//! control captures against `hydrolysis` with the `glass` theme.

#[cfg(not(target_vendor = "apple"))]
compile_error!("hydrolysis_glass_parity requires a host symbol catalog target");

use waterui::app::{App, Windows};
use waterui::form::picker::{PickerStyle, picker};
use waterui::graphics::color::Srgb;
use waterui::prelude::*;
use waterui::preview;
use waterui::theme::color::Background;
#[cfg(target_vendor = "apple")]
use waterui_backend_core::SfSymbol;

// Keep `hydrolysis_glass` linked so `water preview --theme glass` resolves the
// workspace crate without manual edits when adding new glass styles here.
use hydrolysis_glass as _;

fn framed<V: View>(view: V) -> impl View {
    zstack((Background, view.padding_with(EdgeInsets::all(12.0))))
}

fn container_framed<V: View>(view: V) -> impl View {
    zstack((Background, view))
}

fn backdrop_stripes() -> impl View {
    hstack((
        Srgb::from_hex("#EA4335"),
        Srgb::from_hex("#22A861"),
        Srgb::from_hex("#1D6FF2"),
        Srgb::from_hex("#F59E0B"),
    ))
    .spacing(0.0)
}

fn backdrop_framed<V: View>(view: V) -> impl View {
    zstack((backdrop_stripes(), view.padding_with(EdgeInsets::all(12.0))))
}

/// Application entry point required by `water preview` host templates.
pub fn app(env: Environment) -> App<impl Windows> {
    App::new(env).window(Window::new("Hydrolysis Glass Parity", button_glass))
}

/// Bordered capsule button preview.
#[preview]
pub fn button_bordered() -> impl View {
    framed(
        button("Continue")
            .action(|| {})
            .style(ButtonStyle::Bordered),
    )
}

/// Regular glass button preview over the theme background.
#[preview]
pub fn button_glass() -> impl View {
    framed(button("Continue").action(|| {}).style(ButtonStyle::Glass))
}

/// Prominent tinted glass button preview over the theme background.
#[preview]
pub fn button_prominent() -> impl View {
    framed(
        button("Continue")
            .action(|| {})
            .style(ButtonStyle::GlassProminent),
    )
}

/// Regular glass button preview over saturated color stripes.
#[preview]
pub fn button_glass_backdrop() -> impl View {
    backdrop_framed(button("Continue").action(|| {}).style(ButtonStyle::Glass))
}

/// Prominent glass button preview over saturated color stripes.
#[preview]
pub fn button_prominent_backdrop() -> impl View {
    backdrop_framed(
        button("Continue")
            .action(|| {})
            .style(ButtonStyle::GlassProminent),
    )
}

/// Symbol-bearing regular glass button preview over the theme background.
#[preview]
pub fn button_symbol_glass() -> impl View {
    framed(
        button(label("Play").icon(SfSymbol::PLAY_FILL))
            .action(|| {})
            .style(ButtonStyle::Glass),
    )
}

/// Switch toggle preview.
#[preview]
pub fn toggle_switch() -> impl View {
    let on = Binding::new(true);
    framed(toggle("Wi-Fi", &on).style(ToggleStyle::Switch))
}

/// Checkbox toggle preview.
#[preview]
pub fn toggle_checkbox() -> impl View {
    let on = Binding::new(true);
    framed(toggle("Wi-Fi", &on).style(ToggleStyle::Checkbox))
}

/// Continuous slider preview.
#[preview]
pub fn slider_control() -> impl View {
    let value = Binding::new(0.55_f64);
    framed(slider("Volume", &value).range(0.0..=1.0))
}

/// Stepper control preview.
#[preview]
pub fn stepper_control() -> impl View {
    let count = Binding::new(5_i32);
    framed(stepper("Copies", &count).range(0..=10))
}

/// Text field preview.
#[preview]
pub fn text_field() -> impl View {
    let value = Binding::new(Str::from("Ada Lovelace"));
    framed(field("Name", &value).prompt("Your name"))
}

/// Segmented picker preview.
#[preview]
pub fn picker_segmented() -> impl View {
    let selection = Binding::new(1_i32);
    let items: Vec<_> = ["Day", "Week", "Month"]
        .into_iter()
        .zip(0_i32..)
        .map(|(label, tag)| text(label).tag(tag))
        .collect();
    framed(picker("View", items, &selection).style(PickerStyle::Segmented))
}

/// Linear progress indicator preview.
#[preview]
pub fn progress_linear() -> impl View {
    framed(progress(0.65))
}

/// Circular progress indicator preview.
#[preview]
pub fn progress_circular() -> impl View {
    framed(progress(0.65).circular())
}

/// Tab bar navigation preview.
#[preview]
pub fn tabs() -> impl View {
    let selection = Binding::new(0_usize);
    container_framed(Tabs::new(
        &selection,
        (
            Tab::new(label("Home").icon(SfSymbol::HOUSE_FILL), || {
                NavigationView::new("Home", text("Home"))
            }),
            Tab::new(label("Search").icon(SfSymbol::MAGNIFYINGGLASS), || {
                NavigationView::new("Search", text("Search"))
            }),
        ),
    ))
}

/// Four-tab bar navigation preview with symbol icons.
#[preview]
pub fn tabs_four() -> impl View {
    let selection = Binding::new(0_usize);
    container_framed(Tabs::new(
        &selection,
        (
            Tab::new(label("Materials").icon(SfSymbol::SQUARE_ON_SQUARE), || {
                NavigationView::new("Materials", text("Materials"))
            }),
            Tab::new(
                label("Controls").icon(SfSymbol::BUTTON_HORIZONTAL_FILL),
                || NavigationView::new("Controls", text("Controls")),
            ),
            Tab::new(label("Info").icon(SfSymbol::INFO_CIRCLE_FILL), || {
                NavigationView::new("Info", text("Info"))
            }),
            Tab::new(label("Search").icon(SfSymbol::MAGNIFYINGGLASS), || {
                NavigationView::new("Search", text("Search"))
            }),
        ),
    ))
}

/// Navigation stack preview.
#[preview]
pub fn navigation() -> impl View {
    container_framed(NavigationStack::new(NavigationView::new(
        "Root",
        text("Root"),
    )))
}
