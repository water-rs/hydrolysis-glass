//! The glass button look (specification §8.1): a glass or tinted-glass
//! capsule that grows and washes while pressed, and the plain, link and
//! borderless styles, which draw no body.

use cherenkov_record::{BlendSpace, WorkingColor};
use hydrolysis_style::theme::{
    Backdrop, ButtonLook, ButtonSpec, Chrome, ContentEffect, CornerShape, DragDeformation, Driven,
    FocusIndicator, Inputs, Label, PressFeedback, StateLayer, Surface as ChromeSurface, Transition,
};
use nami::Computed;
use waterui_controls::button::ButtonStyle;
use waterui_controls::theme::Tint;
use waterui_core::Environment;
use waterui_graphics::color::Color;
use waterui_interaction::interaction::InteractionState;
use waterui_text::font::{Body, Font, FontWeight};

use crate::dynamics::{self, PRESS_MAX_HALF_AXIS_GROWTH};
use crate::material::recipe::{Appearance, Family, Recipe};

use super::super::Glass;
use super::super::button::{GROWTH_PER_PRESS, Kind, SizeGroup};
use super::super::chrome::{FOCUS_RING_GAP, FOCUS_RING_WIDTH, HOVER_ALPHA, PRESSED_ALPHA};
use super::super::materials::{self, CONTINUOUS_SMOOTHING, GLASS_CONTENT_SHADER, GLASS_SHADER};
use super::super::palette::{alpha, over};
use super::super::surfaces::{self, Press};
use super::{
    DISABLED_CONTENT_ALPHA, control_motion, faded, hit_region, is_disabled, paint, rgba_of,
    spring_animation, state_chrome,
};

/// The look of a button in `spec`.
///
/// The glass and prominent styles take a glass body whose recipe the
/// button's frame and press decide; `ButtonStyle::Bordered` takes a
/// track-filled capsule pill with an accent label under the theme's state
/// layer and focus ring; the remaining borderless styles take a state layer
/// over no body.
pub(in super::super) fn look(glass: &Glass, spec: &ButtonSpec, env: &Environment) -> ButtonLook {
    let group = SizeGroup::of(spec.size());
    let metrics = group.metrics();
    let bordered = spec.style() == ButtonStyle::Bordered;
    let kind = (!bordered)
        .then(|| surfaces::button_kind(spec.style()))
        .flatten();
    let tinted = Tint::inherited(env).is_some();
    let tint = spec.tint().resolve(env);
    let chrome = if bordered {
        bordered_chrome(glass)
    } else if spec.style() == ButtonStyle::Borderless {
        borderless_chrome(glass)
    } else {
        kind.map_or_else(
            || plain_chrome(glass),
            |kind| glass_chrome(glass, group, kind, tinted, tint.clone()),
        )
    };
    let mut label = Label::new()
        .font(
            Font::from(Body)
                .font_size(metrics.text_size)
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
    let mut look = ButtonLook::new(chrome, label)
        .padding(f64::from(metrics.inset_x), f64::from(metrics.inset_y));
    if bordered {
        look = look.group_blend_space(BlendSpace::SrgbEncoded);
    }
    look
}

/// The label's colour in every state: white on a prominent body, the tint
/// on a link or a tinted glass/bordered body, the accent on an untinted
/// bordered body, the foreground otherwise, faint while disabled.
fn label_color(
    glass: &Glass,
    style: ButtonStyle,
    kind: Option<Kind>,
    tinted: bool,
    tint: Computed<WorkingColor>,
) -> Driven<Color> {
    let palette = glass.palette;
    let dimmed = DISABLED_CONTENT_ALPHA;
    let color_of = move |base: [f32; 4], state: InteractionState| {
        Glass::color(if is_disabled(state) {
            over(alpha(base, dimmed), palette.background)
        } else {
            base
        })
    };
    let own = if kind == Some(Kind::Prominent) {
        Some(palette.accent_foreground)
    } else if style == ButtonStyle::Bordered && !tinted {
        Some(palette.accent)
    } else if style != ButtonStyle::Link
        && !(tinted && (kind == Some(Kind::Glass) || style == ButtonStyle::Bordered))
    {
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
    state_chrome(glass, ChromeSurface::clear(CornerShape::full())).hit(hit_region())
}

/// The chrome of a borderless button (including menu rows): preserves focus
/// and press state layers without applying a pointer hover bar after touch
/// activation.
fn borderless_chrome(glass: &Glass) -> Chrome {
    let palette = glass.palette;
    Chrome::new(
        ChromeSurface::clear(CornerShape::full()),
        StateLayer::new(Glass::color(palette.state_layer)).opacities(
            0.0,
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

/// The chrome of a bordered capsule button: a track-filled continuous capsule
/// under the theme's state layer and focus ring.
fn bordered_chrome(glass: &Glass) -> Chrome {
    let base = match glass.palette.appearance {
        Appearance::Dark => alpha(glass.palette.track, 0.105),
        Appearance::Light => glass.palette.track,
    };
    let fill = Driven::new(move |inputs: &Inputs<'_, ()>| {
        paint(faded(
            base,
            if is_disabled(inputs.state) { 0.5 } else { 1.0 },
        ))
    });
    state_chrome(
        glass,
        ChromeSurface::new(CornerShape::full().smoothing(CONTINUOUS_SMOOTHING), fill),
    )
    .hit(hit_region())
}

/// Elastic jelly drag deformation (§6–7): rubber-band lag (`<= 12 pt`) and
/// area-preserving stretch (`lambda_par in [0.86, 1.16]`).
fn jelly_drag() -> DragDeformation {
    DragDeformation::new()
        .lag(12.0, 24.0)
        .displacement_stretch(0.14, 48.0)
        .velocity_stretch(0.12, 600.0)
        .stretch_bounds(0.86, 1.16)
        .active_motion(spring_animation(dynamics::Spring::JELLY_DRAG_ACTIVE))
}

/// The chrome of a glass or prominent button: the glass body as a backdrop
/// fill and press growth; press illumination is folded directly into the
/// material shader (§2.4) without an opaque overlay.
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
    let pressed = |inputs: &Inputs<'_, ()>| {
        #[expect(
            clippy::cast_possible_truncation,
            reason = "a touch coordinate is display-resolution and far inside f32"
        )]
        let (touch_x, touch_y) = inputs
            .press_point
            .map_or((None, None), |pt| (Some(pt.x as f32), Some(pt.y as f32)));
        Press {
            progress: f32::from(u8::from(inputs.state.contains(InteractionState::PRESSED))),
            touch_x,
            touch_y,
        }
    };
    let effect = Driven::follow(tint, move |tint: &WorkingColor, inputs: &Inputs<'_, ()>| {
        let spec = spec_of(inputs.state, tinted.then(|| rgba_of(*tint)));
        let surface =
            surfaces::button(&palette, inputs.frame, &spec, pressed(inputs), translucency);
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

    let content = Driven::new(move |inputs: &Inputs<'_, ()>| {
        let spec = spec_of(inputs.state, None);
        let surface =
            surfaces::button(&palette, inputs.frame, &spec, pressed(inputs), translucency);
        materials::content_effect(&surface.element.recipe)
    });

    let mut motion = control_motion();
    motion.drag = Transition::new(
        spring_animation(dynamics::Spring::JELLY_DRAG_ACTIVE),
        spring_animation(dynamics::Spring::JELLY_DRAG_RELEASE),
    );

    Chrome::new(
        ChromeSurface::new(CornerShape::full().smoothing(CONTINUOUS_SMOOTHING), body)
            .content_effect(ContentEffect::new(GLASS_CONTENT_SHADER, content)),
        // The glass body has no state layer: press lift lives inside the
        // material shader.
        StateLayer::new(Glass::color(palette.state_layer)),
        PressFeedback::new()
            .scale(f64::from(GROWTH_PER_PRESS))
            .max_growth(f64::from(PRESS_MAX_HALF_AXIS_GROWTH))
            .drag_deformation(jelly_drag()),
        FocusIndicator::Tint,
        motion,
    )
    .hit(hit_region())
}
