//! Which widget surfaces are glass, and what shape and recipe each one takes.
//!
//! Every surface here is a [`Element`]: bounds in points, a silhouette, a
//! recipe scaled to the element's size and a press response. The descriptors
//! are pure so both render paths build from the same decisions — the widget
//! theme records them as `backdrop_material` members ([`super::materials`]),
//! and the standalone [`crate::GlassRenderer`] takes them as the frame's
//! [`Group`]s over a captured backdrop.

use cherenkov::kurbo::Rect as KRect;
use waterui::interaction::InteractionState;
use waterui_backend_core::widget::WidgetInteractionState;
use waterui_controls::button::ButtonStyle;

use crate::material::geometry::{
    CONTAINER_UNION_SMOOTHING, CornerCurve, Element, Rect, STANDALONE_UNION_SMOOTHING, Shape,
};
use crate::material::gpu::Group;
use crate::material::recipe::{Appearance, Family, Recipe, Tint, Translucency};

use super::button::{self, BorderShape, Kind, SizeGroup};
use super::palette::{Palette, Rgba};

/// Continuous corner radius of a sheet's exposed corners, pt.
pub const SHEET_CORNER_RADIUS: f32 = 38.0;
/// Continuous corner radius of a container the union fuses members into, pt.
pub const CONTAINER_CORNER_RADIUS: f32 = 26.0;

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
    /// The press wash lying over the glass body, below the label.
    pub wash: Option<Wash>,
}

/// The white press wash of a button (§8.1, Press motion).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Wash {
    /// The grown silhouette the wash fills.
    pub shape: Shape,
    /// The press progress `p` driving its alpha.
    pub progress: f32,
    /// The touch point's horizontal position.
    pub touch_x: f32,
}

impl Wash {
    /// The wash alpha at horizontal position `x`.
    #[must_use]
    pub fn alpha_at(&self, x: f32) -> f32 {
        let half = self.shape.rect.w * 0.5;
        button::wash_alpha(self.progress, (x - self.touch_x).abs() / half)
    }
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

/// The press progress `p` the theme reads from a renderer state snapshot:
/// the newest press wave's opacity over the pressed opacity.
///
/// The renderer animates that opacity with the theme's press and release
/// animations (the springs of §8.1), so `p` follows the springs, overshoot
/// included. A wave is dropped once its opacity reaches zero, so the
/// release spring's undershoot below rest never reaches the theme. A
/// disabled button does not respond to touches (§8.1).
#[must_use]
pub fn press_progress(state: WidgetInteractionState, pressed_opacity: f32) -> f32 {
    if state.state.contains(InteractionState::DISABLED) {
        return 0.0;
    }
    state
        .press_waves
        .latest()
        .map_or(0.0, |wave| wave.opacity / pressed_opacity)
}

/// The kind of glass a button style is realised as, or `None` for the
/// styles that draw only their label (`Plain`, `Link`, `Borderless`).
#[must_use]
pub const fn button_kind(style: ButtonStyle) -> Option<Kind> {
    match style {
        ButtonStyle::Plain | ButtonStyle::Link | ButtonStyle::Borderless => None,
        ButtonStyle::GlassProminent | ButtonStyle::BorderedProminent => Some(Kind::Prominent),
        _ => Some(Kind::Glass),
    }
}

fn element(
    shape: Shape,
    family: Family,
    appearance: Appearance,
    translucency: Translucency,
) -> Element {
    let recipe = Recipe::for_element(shape.rect.w, shape.rect.h, family, appearance, translucency);
    Element { shape, recipe }
}

const fn tinted(mut e: Element, rgb: [f32; 3], alpha: f32) -> Element {
    e.recipe = e.recipe.with_tint(Tint { rgb, alpha });
    e
}

/// What a button is made of (§8.1).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ButtonSpec {
    /// Glass or prominent.
    pub kind: Kind,
    /// The border shape.
    pub border: BorderShape,
    /// The size group.
    pub size: SizeGroup,
    /// The tint; `None` takes the default tint.
    pub tint: Option<Rgba>,
    /// Whether the button responds to touches.
    pub enabled: bool,
}

impl ButtonSpec {
    /// An enabled, default-tinted capsule button.
    #[must_use]
    pub const fn new(kind: Kind, size: SizeGroup) -> Self {
        Self {
            kind,
            border: BorderShape::Capsule,
            size,
            tint: None,
            enabled: true,
        }
    }
}

/// The press of a button: the progress `p` and where the touch is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Press {
    /// The press progress, 0 at rest, 1 pressed; the springs overshoot it.
    pub progress: f32,
    /// The touch point's horizontal position; `None` is the frame's centre.
    pub touch_x: Option<f32>,
}

impl Press {
    /// A button at rest.
    pub const REST: Self = Self {
        progress: 0.0,
        touch_x: None,
    };
}

/// The glass surface of a button laid out at `frame` (§8.1).
///
/// The regular glass body at the frame's `m`, the prominent fill or the
/// disabled prominent fill as a tint layer, and the grown silhouette and
/// wash of a press. The recipe keeps the idle `m`.
#[must_use]
pub fn button(
    palette: &Palette,
    frame: KRect,
    spec: &ButtonSpec,
    press: Press,
    translucency: Translucency,
) -> Surface {
    let idle = button::shape(rect(frame), spec.border, spec.size);
    let body = element(idle, Family::Regular, palette.appearance, translucency);
    let mut e = match (spec.kind, spec.enabled) {
        (Kind::Glass, _) => body,
        (Kind::Prominent, true) => {
            let tint = spec
                .tint
                .unwrap_or_else(|| button::default_tint(palette.appearance));
            tinted(
                body,
                [tint[0], tint[1], tint[2]],
                button::FULL_STRENGTH_TINT_ALPHA,
            )
        }
        (Kind::Prominent, false) => {
            let (grey, strength) = button::disabled_prominent_fill(palette.appearance);
            tinted(
                body,
                [grey; 3],
                strength / crate::material::recipe::TINT_EFFECTIVE_ALPHA,
            )
        }
    };
    // A pressed button grows uniformly about its centre; the recipe stays
    // the idle one (§5.5, §8.1).
    let p = if spec.enabled { press.progress } else { 0.0 };
    e.shape = grown(e.shape, p);
    let wash = (p > 0.0).then(|| Wash {
        shape: e.shape,
        progress: p,
        touch_x: press.touch_x.unwrap_or_else(|| e.shape.rect.center()[0]),
    });
    Surface {
        role: Role::Button,
        element: e,
        wash,
    }
}

/// A button silhouette grown `1 + 0.245·p` about its centre, the radii
/// scaled the same.
fn grown(shape: Shape, p: f32) -> Shape {
    let s = button::growth(p);
    Shape {
        rect: shape.rect.scaled_about_center(s),
        radii: shape.radii.map(|r| r * s),
        ..shape
    }
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
        ),
        wash: None,
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
        ),
        wash: None,
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
        ),
        wash: None,
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
        ),
        wash: None,
    }
}

/// Fuses surfaces into one group: container smoothing when any member is a
/// container, standalone smoothing otherwise. The members share the
/// container recipe (§5.6, [`Group::container`]).
#[must_use]
pub fn group(surfaces: Vec<Surface>) -> Group {
    let smoothing = if surfaces.iter().any(|s| s.role == Role::Container) {
        CONTAINER_UNION_SMOOTHING
    } else {
        STANDALONE_UNION_SMOOTHING
    };
    Group::container(surfaces.into_iter().map(|s| s.element).collect(), smoothing)
}

#[cfg(test)]
#[allow(
    clippy::suboptimal_flops,
    reason = "tests spell out the specification's arithmetic"
)]
mod tests {
    use super::*;
    use waterui_backend_core::widget::{PressWave, PressWaves};

    const P: Palette = Palette::LIGHT;
    const FRAME: KRect = KRect::new(10.0, 20.0, 86.0, 54.33);

    fn spec(kind: Kind) -> ButtonSpec {
        ButtonSpec::new(kind, SizeGroup::Regular)
    }

    fn surface(spec: &ButtonSpec, press: Press) -> Surface {
        button(&P, FRAME, spec, press, Translucency::default())
    }

    #[test]
    fn plain_styles_have_no_glass() {
        for s in [
            ButtonStyle::Plain,
            ButtonStyle::Link,
            ButtonStyle::Borderless,
        ] {
            assert_eq!(button_kind(s), None);
        }
        assert_eq!(button_kind(ButtonStyle::Glass), Some(Kind::Glass));
        assert_eq!(button_kind(ButtonStyle::Bordered), Some(Kind::Glass));
        assert_eq!(
            button_kind(ButtonStyle::GlassProminent),
            Some(Kind::Prominent)
        );
    }

    #[test]
    fn a_glass_button_is_an_untinted_capsule_filling_its_frame() {
        let s = surface(&spec(Kind::Glass), Press::REST);
        let shape = s.element.shape;
        assert_eq!(shape.rect, rect(FRAME));
        assert_eq!(shape.radii, [34.33 * 0.5; 4]);
        assert_eq!(shape.curve, CornerCurve::Continuous);
        assert!(s.element.recipe.tint.is_none());
        assert!(s.wash.is_none());
    }

    #[test]
    fn a_prominent_button_fills_with_the_default_tint_at_full_strength() {
        let s = surface(&spec(Kind::Prominent), Press::REST);
        let tint = s.element.recipe.tint.expect("tinted");
        assert_eq!(tint.rgb, [0.0, 136.0 / 255.0, 1.0]);
        assert!((tint.alpha * crate::material::recipe::TINT_EFFECTIVE_ALPHA - 1.0).abs() < 1e-6);
        let red = ButtonSpec {
            tint: Some([1.0, 56.0 / 255.0, 61.0 / 255.0, 1.0]),
            ..spec(Kind::Prominent)
        };
        let tint = surface(&red, Press::REST)
            .element
            .recipe
            .tint
            .expect("tinted");
        assert_eq!(tint.rgb, [1.0, 56.0 / 255.0, 61.0 / 255.0]);
    }

    #[test]
    fn a_disabled_prominent_button_drops_the_tint_for_the_grey_fill() {
        let off = ButtonSpec {
            enabled: false,
            ..spec(Kind::Prominent)
        };
        let tint = surface(&off, Press::REST)
            .element
            .recipe
            .tint
            .expect("tinted");
        assert_eq!(tint.rgb, [0.55; 3]);
        assert!((tint.alpha * crate::material::recipe::TINT_EFFECTIVE_ALPHA - 0.25).abs() < 1e-6);
        // A disabled glass button's body is unchanged.
        let glass = ButtonSpec {
            enabled: false,
            ..spec(Kind::Glass)
        };
        assert_eq!(
            surface(&glass, Press::REST),
            surface(&spec(Kind::Glass), Press::REST)
        );
    }

    #[test]
    fn a_disabled_button_ignores_a_press() {
        let off = ButtonSpec {
            enabled: false,
            ..spec(Kind::Glass)
        };
        let down = Press {
            progress: 1.0,
            touch_x: None,
        };
        assert_eq!(surface(&off, down), surface(&off, Press::REST));
    }

    #[test]
    fn press_grows_the_silhouette_and_keeps_the_idle_recipe() {
        let rest = surface(&spec(Kind::Glass), Press::REST);
        let down = surface(
            &spec(Kind::Glass),
            Press {
                progress: 1.0,
                touch_x: None,
            },
        );
        assert!((down.element.shape.rect.w - 76.0 * 1.245).abs() < 1e-3);
        assert!((down.element.shape.rect.h - 34.33 * 1.245).abs() < 1e-3);
        assert!((down.element.shape.radii[0] - 34.33 * 0.5 * 1.245).abs() < 1e-3);
        assert_eq!(
            down.element.shape.rect.center(),
            rest.element.shape.rect.center()
        );
        assert_eq!(down.element.recipe, rest.element.recipe);
    }

    #[test]
    fn the_wash_follows_the_touch_and_the_grown_width() {
        let down = surface(
            &spec(Kind::Glass),
            Press {
                progress: 1.0,
                touch_x: None,
            },
        );
        let wash = down.wash.expect("pressed");
        let [cx, _] = down.element.shape.rect.center();
        assert!((wash.alpha_at(cx) - 0.30).abs() < 1e-6);
        let end = down.element.shape.rect.x;
        assert!((wash.alpha_at(end) - 0.30 * 0.63).abs() < 1e-4);
        // Centred on a touch near the left end.
        let off = surface(
            &spec(Kind::Glass),
            Press {
                progress: 1.0,
                touch_x: Some(end),
            },
        )
        .wash
        .expect("pressed");
        assert!((off.alpha_at(end) - 0.30).abs() < 1e-6);
    }

    #[test]
    fn press_progress_is_the_newest_waves_opacity_over_the_pressed_opacity() {
        let mut state = WidgetInteractionState::NONE;
        assert_eq!(press_progress(state, 0.12), 0.0);
        // The hover layer is not a press.
        state.state_layer_opacity = 0.06;
        assert_eq!(press_progress(state, 0.12), 0.0);
        let mut waves = PressWaves::EMPTY;
        waves.push(PressWave {
            origin: None,
            progress: 1.0,
            opacity: 0.06,
        });
        waves.push(PressWave {
            origin: None,
            progress: 1.0,
            opacity: 0.1284,
        });
        state.press_waves = waves;
        assert!((press_progress(state, 0.12) - 1.07).abs() < 1e-4);
        state.state = InteractionState::PRESSED | InteractionState::DISABLED;
        assert_eq!(press_progress(state, 0.12), 0.0);
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
        assert_eq!(g.smoothing(), CONTAINER_UNION_SMOOTHING);
        let g = group(vec![toolbar(
            &P,
            KRect::new(0.0, 0.0, 300.0, 50.0),
            Translucency::default(),
        )]);
        assert_eq!(g.smoothing(), STANDALONE_UNION_SMOOTHING);
    }
}
