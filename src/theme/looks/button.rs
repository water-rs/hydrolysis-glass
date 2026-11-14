//! The glass button look (specification §8.1): a glass or tinted-glass
//! capsule that grows and washes while pressed, and the plain, link and
//! borderless styles, which draw no body.

use cherenkov_record::{BlendSpace, Draw as _, Group, Paint, WorkingColor};
use hydrolysis::theme::{
    Backdrop, ButtonLook, ButtonSpec, Chrome, CornerShape, Custom, Driven, FocusIndicator, Inputs,
    Label, PressFeedback, StateLayer, Surface as ChromeSurface, WidgetTheme as _,
};
use waterui::color::Color;
use waterui::interaction::InteractionState;
use waterui::reactive::Computed;
use waterui::text::font::{Body, Font, FontWeight};
use waterui::theme::Tint;
use waterui_controls::button::ButtonStyle;
use waterui_core::Environment;

use crate::material::recipe::{Family, Recipe};

use super::super::button::{BorderShape, GROWTH_PER_PRESS, Kind, SizeGroup};
use super::super::chrome::{FOCUS_RING_GAP, FOCUS_RING_WIDTH, HOVER_ALPHA, PRESSED_ALPHA};
use super::super::materials::{self, CONTINUOUS_SMOOTHING, GLASS_SHADER};
use super::super::palette::{alpha, over};
use super::super::surfaces::{self, Press, Wash};
use super::super::{Glass, button};
use super::{control_motion, hit_region, is_disabled, rgba_of};

/// The look of a button in `spec`.
///
/// The glass and prominent styles take a glass body whose recipe the
/// button's frame and press decide; the other styles take a state layer
/// over no body.
pub(in super::super) fn look(glass: &Glass, spec: &ButtonSpec, env: &Environment) -> ButtonLook {
    let group = SizeGroup::of(spec.size());
    let metrics = group.metrics();
    let kind = surfaces::button_kind(spec.style());
    let tinted = Tint::inherited(env).is_some();
    let tint = spec.tint().resolve(env);
    let chrome = kind.map_or_else(
        || plain_chrome(glass),
        |kind| glass_chrome(glass, group, kind, tinted, tint.clone()),
    );
    let mut label = Label::new()
        .font(
            Font::from(Body)
                .size(metrics.text_size)
                .weight(FontWeight::Normal)
                .line_height(metrics.line_height),
        )
        .color(label_color(glass, spec.style(), kind, tinted, tint));
    if kind.is_some() {
        label = label.follows_press_scale();
    }
    // The frame is the label plus the size group's insets and nothing more
    // (§8.1), so the look carries no minimum size. A symbol takes the same
    // insets as a title.
    ButtonLook::new(chrome, label).padding(f64::from(metrics.inset_x), f64::from(metrics.inset_y))
}

/// The label's colour in every state: white on a prominent body, the tint
/// on a link or a tinted glass body, the foreground otherwise, faint while
/// disabled.
fn label_color(
    glass: &Glass,
    style: ButtonStyle,
    kind: Option<Kind>,
    tinted: bool,
    tint: Computed<WorkingColor>,
) -> Driven<Color> {
    let palette = glass.palette;
    let dimmed = glass.disabled_content_alpha();
    let color_of = move |base: [f32; 4], state: InteractionState| {
        Glass::color(if is_disabled(state) {
            over(alpha(base, dimmed), palette.background)
        } else {
            base
        })
    };
    let own = if kind == Some(Kind::Prominent) {
        Some(palette.accent_foreground)
    } else if style != ButtonStyle::Link && !(tinted && kind == Some(Kind::Glass)) {
        Some(palette.foreground)
    } else {
        None
    };
    match own {
        Some(base) => Driven::new(move |inputs: &Inputs<'_, ()>| color_of(base, inputs.state)),
        None => Driven::follow(tint, move |tint: &WorkingColor, inputs: &Inputs<'_, ()>| {
            color_of(rgba_of(*tint), inputs.state)
        }),
    }
}

/// The chrome of a style with no body: a state layer over a capsule and a
/// focus ring.
fn plain_chrome(glass: &Glass) -> Chrome {
    let palette = glass.palette;
    Chrome::new(
        ChromeSurface::clear(CornerShape::full()),
        StateLayer::new(Glass::color(palette.state_layer)).opacities(
            HOVER_ALPHA,
            HOVER_ALPHA,
            PRESSED_ALPHA,
            PRESSED_ALPHA,
        ),
        PressFeedback::new().tint(),
        FocusIndicator::Ring {
            width: FOCUS_RING_WIDTH,
            offset: FOCUS_RING_GAP,
            color: Glass::color(palette.accent),
        },
        control_motion(),
    )
    .hit(hit_region())
}

/// The chrome of a glass or prominent button: the glass body as a backdrop
/// fill, the press growth, and the wash as a part over the body.
fn glass_chrome(
    glass: &Glass,
    group: SizeGroup,
    kind: Kind,
    tinted: bool,
    tint: Computed<WorkingColor>,
) -> Chrome {
    let (palette, translucency) = (glass.palette, glass.translucency);
    let spec_of = move |state: InteractionState, tint: Option<[f32; 4]>| surfaces::ButtonSpec {
        tint,
        enabled: !is_disabled(state),
        ..surfaces::ButtonSpec::new(kind, group)
    };
    let pressed = |state: InteractionState| Press {
        progress: f32::from(u8::from(state.contains(InteractionState::PRESSED))),
        touch_x: None,
    };
    let effect = Driven::follow(tint, move |tint: &WorkingColor, inputs: &Inputs<'_, ()>| {
        let spec = spec_of(inputs.state, tinted.then(|| rgba_of(*tint)));
        let surface = surfaces::button(
            &palette,
            inputs.frame,
            &spec,
            pressed(inputs.state),
            translucency,
        );
        materials::effect(&surface.element)
    });
    // The capture class is the look's, not the frame's: the recipe's
    // capture scale for the regular family below 140 pt, the sizes a button
    // has.
    let capture = Recipe::for_element(
        40.0,
        40.0,
        Family::Regular,
        palette.appearance,
        translucency,
    )
    .capture_scale;
    let body = Backdrop::new(
        GLASS_SHADER,
        materials::capture_class(false, capture),
        effect,
    );

    // The wash is white over the grown silhouette: its alpha across the
    // width is the press progress's, so it is the gradient of a fully
    // pressed button while pressed and of none at rest.
    let wash_shape = Driven::new(move |inputs: &Inputs<'_, ()>| {
        let idle = button::shape(surfaces::rect(inputs.frame), BorderShape::Capsule, group);
        let shape = if inputs.state.contains(InteractionState::PRESSED) {
            surfaces::grown(idle, 1.0)
        } else {
            idle
        };
        materials::shape_data(&shape)
    });
    let wash_paint = Driven::new(move |inputs: &Inputs<'_, ()>| {
        let pressed = inputs.state.contains(InteractionState::PRESSED);
        let idle = button::shape(surfaces::rect(inputs.frame), BorderShape::Capsule, group);
        let shape = if pressed {
            surfaces::grown(idle, 1.0)
        } else {
            idle
        };
        #[expect(
            clippy::cast_possible_truncation,
            reason = "a touch coordinate is display-resolution and far inside f32"
        )]
        let touch_x = inputs
            .press_point
            .map_or_else(|| shape.rect.center()[0], |point| point.x as f32);
        materials::wash_paint(&Wash {
            shape,
            progress: f32::from(u8::from(pressed && !is_disabled(inputs.state))),
            touch_x,
        })
    });
    let wash = Custom::<(), _>::new(
        (wash_shape, wash_paint),
        |recorder, _frame, (shape, paint): (_, cherenkov_record::Live<Paint>)| {
            recorder.group(
                Group::new().blend_space(BlendSpace::SrgbEncoded),
                |recorder| {
                    recorder.fill(shape, paint);
                },
            );
        },
    );

    Chrome::new(
        ChromeSurface::new(CornerShape::full().smoothing(CONTINUOUS_SMOOTHING), body),
        // The glass body has no state layer: a hover or a focus shows
        // nothing, and a press shows the wash.
        StateLayer::new(Glass::color(palette.state_layer)),
        PressFeedback::new().scale(f64::from(GROWTH_PER_PRESS)),
        FocusIndicator::Tint,
        control_motion(),
    )
    .hit(hit_region())
    .custom(wash)
}
