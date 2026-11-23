//! Standard `#[preview]` fixture targets for comparing `waterui-apple`
//! (`--platform ios`) against `hydrolysis` + `hydrolysis-glass` (`--theme glass`).

use waterui::app::{App, Windows};
use waterui::form::picker::{PickerStyle, picker};
use waterui::graphics::color::Srgb;
use waterui::prelude::*;
use waterui::preview;
use waterui::theme::color::Background;

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
    App::new(button_glass, env)
}

/// Bordered capsule button preview.
#[preview]
pub fn button_bordered() -> impl View {
    framed(button("Continue").style(ButtonStyle::Bordered))
}

/// Regular glass button preview over the theme background.
#[preview]
pub fn button_glass() -> impl View {
    framed(button("Continue").style(ButtonStyle::Glass))
}

/// Prominent tinted glass button preview over the theme background.
#[preview]
pub fn button_prominent() -> impl View {
    framed(button("Continue").style(ButtonStyle::GlassProminent))
}

/// Regular glass button preview over saturated color stripes.
#[preview]
pub fn button_glass_backdrop() -> impl View {
    backdrop_framed(button("Continue").style(ButtonStyle::Glass))
}

/// Prominent glass button preview over saturated color stripes.
#[preview]
pub fn button_prominent_backdrop() -> impl View {
    backdrop_framed(button("Continue").style(ButtonStyle::GlassProminent))
}

/// Switch toggle preview.
#[preview]
pub fn toggle_switch() -> impl View {
    let on = Binding::bool(true);
    framed(toggle("Wi-Fi", &on).style(ToggleStyle::Switch))
}

/// Checkbox toggle preview.
#[preview]
pub fn toggle_checkbox() -> impl View {
    let on = Binding::bool(true);
    framed(toggle("Wi-Fi", &on).style(ToggleStyle::Checkbox))
}

/// Continuous slider preview.
#[preview]
pub fn slider_control() -> impl View {
    let value = Binding::f64(0.55);
    framed(slider("Volume", &value).range(0.0..=1.0))
}

/// Stepper control preview.
#[preview]
pub fn stepper_control() -> impl View {
    let count = Binding::i32(5);
    framed(stepper("Copies", &count).range(0..=10))
}

/// Text field preview.
#[preview]
pub fn text_field() -> impl View {
    let value = Binding::container(Str::from("Ada Lovelace"));
    framed(field("Name", &value).prompt("Your name"))
}

/// Segmented picker preview.
#[preview]
pub fn picker_segmented() -> impl View {
    let selection = Binding::container(1_i32);
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
    let selection = Binding::container(0_usize);
    container_framed(Tabs::new(
        &selection,
        (
            Tab::new("Home", || NavigationView::new("Home", text("Home"))),
            Tab::new("Search", || NavigationView::new("Search", text("Search"))),
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
