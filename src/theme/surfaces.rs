//! Which widget surfaces are glass, and what shape and recipe each one takes.
//!
//! Every surface here is a [`Element`] ready for [`crate::GlassRenderer`]:
//! bounds in points, a silhouette, a recipe scaled to the element's size and a
//! press response. The descriptors are pure so a host that owns the backdrop
//! (see `ENGINE_REQUIREMENTS.md`) can build the frame's [`Group`]s from the
//! same decisions the theme makes for layout.

use cherenkov::kurbo::Rect as KRect;
use waterui::interaction::InteractionState;
use waterui_backend_core::widget::WidgetInteractionState;
use waterui_controls::ControlSize;
use waterui_controls::button::ButtonStyle;

use crate::interaction::press_shape;
use crate::material::geometry::{
    CONTAINER_UNION_SMOOTHING, CornerCurve, Element, Rect, STANDALONE_UNION_SMOOTHING, Shape,
};
use crate::material::gpu::Group;
use crate::material::recipe::{Appearance, Family, Recipe, Tint, Translucency};

use super::palette::{Palette, Rgba};

/// Continuous corner radius of a sheet's exposed corners, pt.
pub const SHEET_CORNER_RADIUS: f32 = 38.0;
/// Continuous corner radius of a container the union fuses members into, pt.
pub const CONTAINER_CORNER_RADIUS: f32 = 26.0;
/// Continuous corner radius of a non-capsule button, pt.
pub const BUTTON_CORNER_RADIUS: f32 = 16.0;
/// A button whose aspect is longer than this is a capsule; a squarer one keeps
/// continuous corners so a tall icon well does not turn into a pill.
pub const CAPSULE_ASPECT: f32 = 1.6;
/// Declared alpha of the accent tint folded into a prominent capsule.
pub const PROMINENT_TINT_ALPHA: f32 = 0.9;

/// Which kind of surface a [`Surface`] describes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    /// A button or capsule.
    Button,
    /// A tab bar.
    TabBar,
    /// A navigation or bottom toolbar.
    Toolbar,
    /// A sheet rising over content.
    Sheet,
    /// A container that fuses its members.
    Container,
}

/// One glass surface the theme placed.
#[derive(Clone, Debug, PartialEq)]
pub struct Surface {
    /// What the surface is.
    pub role: Role,
    /// Its element.
    pub element: Element,
}

/// Converts kurbo bounds to material bounds.
#[must_use]
#[allow(
    clippy::cast_possible_truncation,
    reason = "kurbo f64 layout coordinates are display-resolution and well inside f32"
)]
pub const fn rect(bounds: KRect) -> Rect {
    Rect::new(
        bounds.x0 as f32,
        bounds.y0 as f32,
        bounds.width() as f32,
        bounds.height() as f32,
    )
}

/// The press amount `0..=1` the theme reads from a renderer state snapshot.
///
/// The renderer eases the state-layer opacity toward `pressed_opacity` while
/// the pointer is down, so normalising it gives the animated press that scales
/// the bounds by `1 + PRESS_SCALE·press`; a snapshot with no state-layer motion
/// falls back to the raw pressed flag.
#[must_use]
pub fn press_amount(state: WidgetInteractionState, pressed_opacity: f32) -> f32 {
    if state.state.contains(InteractionState::DISABLED) {
        return 0.0;
    }
    if state.state_layer_opacity > 0.0 && pressed_opacity > 0.0 {
        (state.state_layer_opacity / pressed_opacity).clamp(0.0, 1.0)
    } else {
        f32::from(u8::from(state.state.contains(InteractionState::PRESSED)))
    }
}

/// Whether a button style is realised as glass.
///
/// `Plain`, `Link` and `Borderless` draw only their label; every other style
/// is a glass capsule.
#[must_use]
pub const fn button_is_glass(style: ButtonStyle) -> bool {
    !matches!(
        style,
        ButtonStyle::Plain | ButtonStyle::Link | ButtonStyle::Borderless
    )
}

/// Whether a button style is the prominent, accent-tinted capsule.
#[must_use]
pub const fn button_is_prominent(style: ButtonStyle) -> bool {
    matches!(
        style,
        ButtonStyle::GlassProminent | ButtonStyle::BorderedProminent
    )
}

fn element(
    shape: Shape,
    family: Family,
    appearance: Appearance,
    translucency: Translucency,
    press: f32,
) -> Element {
    let rest = shape.rect;
    let recipe = Recipe::for_element(rest.w, rest.h, family, appearance, translucency);
    // A pressed element grows uniformly about its centre while keeping its
    // idle recipe (spec §5.5).
    Element {
        shape: press_shape(shape, press),
        recipe,
    }
}

const fn tinted(mut e: Element, rgba: Rgba) -> Element {
    e.recipe = e.recipe.with_tint(Tint {
        rgb: [rgba[0], rgba[1], rgba[2]],
        alpha: rgba[3],
    });
    e
}

/// The silhouette of a button: a circle when icon-only and square, a capsule
/// when wide, continuous corners otherwise.
#[must_use]
pub fn button_shape(bounds: Rect, icon_only: bool) -> Shape {
    let aspect = bounds.w / bounds.h.max(1e-3);
    if icon_only && (aspect - 1.0).abs() < 0.05 {
        let [cx, cy] = bounds.center();
        Shape::circle(cx, cy, bounds.m() * 0.5)
    } else if aspect >= CAPSULE_ASPECT || aspect <= 1.0 / CAPSULE_ASPECT {
        Shape::capsule(bounds)
    } else {
        Shape::rounded_rect(
            bounds,
            BUTTON_CORNER_RADIUS.min(bounds.m() * 0.5),
            CornerCurve::Continuous,
        )
    }
}

/// The glass surface of a button, or `None` for a style that draws no chrome.
#[must_use]
pub fn button(
    palette: &Palette,
    bounds: KRect,
    style: ButtonStyle,
    _size: ControlSize,
    icon_only: bool,
    press: f32,
    translucency: Translucency,
) -> Option<Surface> {
    if !button_is_glass(style) {
        return None;
    }
    let shape = button_shape(rect(bounds), icon_only);
    let e = if button_is_prominent(style) {
        tinted(
            element(
                shape,
                Family::Regular,
                palette.appearance,
                translucency,
                press,
            ),
            [
                palette.accent[0],
                palette.accent[1],
                palette.accent[2],
                PROMINENT_TINT_ALPHA,
            ],
        )
    } else {
        element(
            shape,
            Family::Regular,
            palette.appearance,
            translucency,
            press,
        )
    };
    Some(Surface {
        role: Role::Button,
        element: e,
    })
}

/// A tab bar: a regular capsule spanning its bounds.
#[must_use]
pub fn tab_bar(palette: &Palette, bounds: KRect, translucency: Translucency) -> Surface {
    Surface {
        role: Role::TabBar,
        element: element(
            Shape::capsule(rect(bounds)),
            Family::Regular,
            palette.appearance,
            translucency,
            0.0,
        ),
    }
}

/// A toolbar: a regular capsule spanning its bounds.
#[must_use]
pub fn toolbar(palette: &Palette, bounds: KRect, translucency: Translucency) -> Surface {
    Surface {
        role: Role::Toolbar,
        element: element(
            Shape::capsule(rect(bounds)),
            Family::Regular,
            palette.appearance,
            translucency,
            0.0,
        ),
    }
}

/// A sheet: continuous top corners, square bottom corners where it meets the
/// screen edge, the regular recipe at the sheet's size.
#[must_use]
pub fn sheet(palette: &Palette, bounds: KRect, translucency: Translucency) -> Surface {
    let r = rect(bounds);
    let radius = SHEET_CORNER_RADIUS.min(r.m() * 0.5);
    Surface {
        role: Role::Sheet,
        element: element(
            Shape::rounded_rect(r, radius, CornerCurve::Continuous)
                .with_radii([radius, radius, 0.0, 0.0]),
            Family::Regular,
            palette.appearance,
            translucency,
            0.0,
        ),
    }
}

/// A container: continuous corners at the container radius, the regular
/// recipe at the container's size.
#[must_use]
pub fn container(palette: &Palette, bounds: KRect, translucency: Translucency) -> Surface {
    let r = rect(bounds);
    Surface {
        role: Role::Container,
        element: element(
            Shape::rounded_rect(
                r,
                CONTAINER_CORNER_RADIUS.min(r.m() * 0.5),
                CornerCurve::Continuous,
            ),
            Family::Regular,
            palette.appearance,
            translucency,
            0.0,
        ),
    }
}

/// Fuses surfaces into one group: container smoothing when any member is a
/// container, standalone smoothing otherwise.
#[must_use]
pub fn group(surfaces: Vec<Surface>) -> Group {
    let smoothing = if surfaces.iter().any(|s| s.role == Role::Container) {
        CONTAINER_UNION_SMOOTHING
    } else {
        STANDALONE_UNION_SMOOTHING
    };
    Group {
        members: surfaces.into_iter().map(|s| s.element).collect(),
        smoothing,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const P: Palette = Palette::LIGHT;

    #[test]
    fn plain_styles_have_no_glass() {
        for s in [
            ButtonStyle::Plain,
            ButtonStyle::Link,
            ButtonStyle::Borderless,
        ] {
            assert!(
                button(
                    &P,
                    KRect::new(0.0, 0.0, 120.0, 44.0),
                    s,
                    ControlSize::Small,
                    false,
                    0.0,
                    Translucency::default(),
                )
                .is_none()
            );
        }
    }

    #[test]
    fn wide_button_is_a_capsule() {
        let s = button(
            &P,
            KRect::new(10.0, 20.0, 130.0, 64.0),
            ButtonStyle::Glass,
            ControlSize::Small,
            false,
            0.0,
            Translucency::default(),
        )
        .expect("glass");
        assert_eq!(s.element.shape.radii, [22.0; 4]);
        assert_eq!(s.element.shape.curve, CornerCurve::Circular);
        assert!(s.element.recipe.tint.is_none());
    }

    #[test]
    fn prominent_button_carries_the_accent_tint() {
        let s = button(
            &P,
            KRect::new(0.0, 0.0, 120.0, 44.0),
            ButtonStyle::GlassProminent,
            ControlSize::Small,
            false,
            0.0,
            Translucency::default(),
        )
        .expect("glass");
        let tint = s.element.recipe.tint.expect("tinted");
        assert_eq!(tint.rgb, [P.accent[0], P.accent[1], P.accent[2]]);
        assert_eq!(tint.alpha, PROMINENT_TINT_ALPHA);
    }

    #[test]
    fn icon_button_is_a_circle() {
        let s = button(
            &P,
            KRect::new(0.0, 0.0, 44.0, 44.0),
            ButtonStyle::Glass,
            ControlSize::Small,
            true,
            0.0,
            Translucency::default(),
        )
        .expect("glass");
        assert_eq!(s.element.shape.radii, [22.0; 4]);
    }

    #[test]
    fn press_grows_bounds_and_keeps_the_idle_recipe() {
        let rest = button(
            &P,
            KRect::new(0.0, 0.0, 100.0, 40.0),
            ButtonStyle::Glass,
            ControlSize::Small,
            false,
            0.0,
            Translucency::default(),
        )
        .expect("glass");
        let down = button(
            &P,
            KRect::new(0.0, 0.0, 100.0, 40.0),
            ButtonStyle::Glass,
            ControlSize::Small,
            false,
            1.0,
            Translucency::default(),
        )
        .expect("glass");
        assert!((down.element.shape.rect.w - 124.0).abs() < 1e-3);
        assert!((down.element.shape.rect.h - 49.6).abs() < 1e-3);
        assert_eq!(
            down.element.shape.rect.center(),
            rest.element.shape.rect.center()
        );
        assert_eq!(down.element.recipe, rest.element.recipe);
    }

    #[test]
    fn press_amount_normalises_the_state_layer() {
        let mut state = WidgetInteractionState::NONE;
        assert_eq!(press_amount(state, 0.12), 0.0);
        state.state = InteractionState::PRESSED;
        assert_eq!(press_amount(state, 0.12), 1.0);
        state.state_layer_opacity = 0.06;
        assert!((press_amount(state, 0.12) - 0.5).abs() < 1e-6);
        state.state = InteractionState::PRESSED | InteractionState::DISABLED;
        assert_eq!(press_amount(state, 0.12), 0.0);
    }

    #[test]
    fn sheet_keeps_square_bottom_corners() {
        let s = sheet(
            &P,
            KRect::new(0.0, 300.0, 390.0, 500.0),
            Translucency::default(),
        );
        assert_eq!(s.element.shape.radii, [38.0, 38.0, 0.0, 0.0]);
        assert_eq!(s.element.shape.curve, CornerCurve::Continuous);
    }

    #[test]
    fn container_group_uses_container_smoothing() {
        let g = group(vec![
            container(
                &P,
                KRect::new(0.0, 0.0, 200.0, 200.0),
                Translucency::default(),
            ),
            tab_bar(
                &P,
                KRect::new(20.0, 150.0, 160.0, 44.0),
                Translucency::default(),
            ),
        ]);
        assert_eq!(g.smoothing, CONTAINER_UNION_SMOOTHING);
        let g = group(vec![toolbar(
            &P,
            KRect::new(0.0, 0.0, 300.0, 50.0),
            Translucency::default(),
        )]);
        assert_eq!(g.smoothing, STANDALONE_UNION_SMOOTHING);
    }
}
