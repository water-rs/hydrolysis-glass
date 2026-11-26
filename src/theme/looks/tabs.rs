//! The tabs look (specification §8.7): the bar a standalone glass capsule
//! with no separator, each item a capsule state layer with a focus ring,
//! and one shared bar-level selection pill gliding across tab items that
//! becomes a clear refractive lens (`1 + 0.12·p`) while pressed or dragged.

use cherenkov_record::kurbo::{Rect, RoundedRectRadii, Size};
use cherenkov_record::{BlendSpace, ContinuousRect, Draw as _, Group, ShapeData};
use hydrolysis_style::theme::{
    Chrome, CornerShape, Custom, Driven, DrivenSet, FocusIndicator, HitRegion, Inputs,
    PressFeedback, StateLayer, Surface as ChromeSurface, TabBarWidth, TabsDrag, TabsLook,
    TabsMetrics, TabsSpec, TabsValue, Transition, ValueMotion,
};
use waterui::interaction::InteractionState;
use waterui::navigation::tab::TabStyle;
use waterui_core::Environment;

use crate::dynamics;
use crate::material::geometry::{CornerCurve, Element, Rect as ElementRect, Shape};
use crate::material::recipe::{Appearance, Family, Recipe};

use super::super::button::{CANCEL_DISTANCE, MIN_HIT_SIDE};
use super::super::chrome::{FOCUS_RING_GAP, FOCUS_RING_WIDTH, HOVER_ALPHA};
use super::super::materials::{self, CONTINUOUS_SMOOTHING, GLASS_SHADER};
use super::super::{Glass, record_standalone};
use super::{control_motion, is_disabled, paint, spring_animation, stretch_along_axis};

/// The measures of compact vertical items: a 60 pt bar, items at least
/// 64 pt wide with their labels inset 12 pt, and 6 pt between an icon and
/// its label (§8.7, Geometry).
const VERTICAL_METRICS: TabsMetrics = TabsMetrics::new(60.0, 64.0, 12.0, 6.0);
/// Width threshold where bottom bar items switch to horizontal layout, pt
/// (§8.7, Geometry).
const HORIZONTAL_BREAKPOINT: f64 = 600.0;
/// The measures of wide horizontal items: a 48 pt bar, items at least
/// 64 pt wide with their labels inset 16 pt, and 8 pt between an icon and
/// its label (§8.7, Geometry).
const HORIZONTAL_METRICS: TabsMetrics = TabsMetrics::new(48.0, 64.0, 16.0, 8.0);
/// Minimum slot width per item in a compact floating bottom tab pill, pt
/// (§8.7, Geometry).
const COMPACT_ITEM_MIN_WIDTH: f64 = 94.0;
/// Outer floating clearance from the bottom and horizontal viewport edges, pt
/// (§8.7, Geometry).
const FLOATING_MARGIN: f64 = 12.0;
/// Inset of the selected highlight capsule inside the item's bounds, pt
/// (§8.7, Geometry).
const HIGHLIGHT_INSET: f64 = 4.0;
/// Alpha of the resting selection pill over the tab bar in encoded space (§5):
/// black @ `0.055` produces `(235, 235, 235)` over Light `(248, 248, 248)`,
/// while black @ `0.6875` produces the recessed `(10, 10, 10)` well inside
/// Dark `(32, 32, 32)`.
const REST_PILL_ALPHA_LIGHT: f32 = 0.055;
const REST_PILL_ALPHA_DARK: f32 = 0.6875;
/// Active glass lens expansion over the resting pill (§1.4): `+36%` width and
/// `+28%` height so the lifted lens spans full item width while overhanging
/// the top and bottom of the `60 pt` bar.
const LENS_GROWTH_X: f64 = 0.36;
const LENS_GROWTH_Y: f64 = 0.28;

/// Whether the bar-level tab pill should show the active refractive lens.
const fn active_lens(state: InteractionState) -> bool {
    state.intersects(InteractionState::PRESSED.union(InteractionState::DRAGGED))
        && !is_disabled(state)
}

/// Continuous capsule shape filling `rect`.
fn capsule(rect: Rect) -> ShapeData {
    let radius = rect.height().min(rect.width()) * 0.5;
    ShapeData::Continuous(
        ContinuousRect::new(rect, RoundedRectRadii::from_single_radius(radius))
            .with_smoothing(CONTINUOUS_SMOOTHING),
    )
}

/// Exact grown and anti-pinch stretched lens bounds for the current input state.
fn lens_rect(inputs: &Inputs<'_, TabsValue>, vertical: bool) -> Rect {
    let Some(bounds) = inputs.value.indicator_bounds() else {
        return Rect::ZERO;
    };
    let rest = bounds.inset(-HIGHLIGHT_INSET);
    let active = f64::from(u8::from(active_lens(inputs.state)));
    let is_sidebar = inputs
        .value
        .items
        .first()
        .map_or(vertical, |item| item.placement == TabStyle::Sidebar);
    let (gx, gy) = if is_sidebar {
        (LENS_GROWTH_Y, LENS_GROWTH_X)
    } else {
        (LENS_GROWTH_X, LENS_GROWTH_Y)
    };
    let size = rest.size();
    let grown = Rect::from_center_size(
        rest.center(),
        Size::new(
            size.width * gx.mul_add(active, 1.0),
            size.height * gy.mul_add(active, 1.0),
        ),
    );
    stretch_along_axis(grown, inputs.velocity, is_sidebar)
}

/// Shared selection pill recorded beneath the tab labels at rest, composited
/// in encoded space so Dark renders a recessed `(10, 10, 10)` well inside the
/// `(32, 32, 32)` bar and Light renders `(235, 235, 235)` over `(248, 248, 248)`.
fn rest_selection_indicator(glass: &Glass) -> Custom<TabsValue, impl DrivenSet<TabsValue> + use<>> {
    let appearance = glass.palette.appearance;
    let rest_alpha = match appearance {
        Appearance::Dark => REST_PILL_ALPHA_DARK,
        Appearance::Light => REST_PILL_ALPHA_LIGHT,
    };
    let rest_pill = Driven::new(|inputs: &Inputs<'_, TabsValue>| {
        let rect = inputs
            .value
            .indicator_bounds()
            .map_or(Rect::ZERO, |bounds| bounds.inset(-HIGHLIGHT_INSET));
        capsule(rect)
    });
    let rest_paint = Driven::new(move |inputs: &Inputs<'_, TabsValue>| {
        let visible = inputs.value.indicator_bounds().is_some() && !active_lens(inputs.state);
        paint([0.0, 0.0, 0.0, if visible { rest_alpha } else { 0.0 }])
    })
    .smoothstep_window(0.05, 0.55);
    Custom::new(
        (rest_pill, rest_paint),
        |recorder, _frame, (shape, paint)| {
            recorder.group(
                Group::new().blend_space(BlendSpace::SrgbEncoded),
                |recorder| {
                    recorder.fill(shape, paint);
                },
            );
        },
    )
}

/// Shared clear refractive glass pill recorded over the tab labels while
/// pressed or dragged, with uniform affine velocity stretch-squash (§1.4).
fn lens_selection_indicator(
    glass: &Glass,
    vertical: bool,
) -> Custom<TabsValue, impl DrivenSet<TabsValue> + use<>> {
    let (palette, translucency) = (glass.palette, glass.translucency);
    let capture = materials::capture_class(
        false,
        Recipe::for_element(40.0, 40.0, Family::Clear, palette.appearance, translucency)
            .capture_scale,
    );
    let lens_shape =
        Driven::new(move |inputs: &Inputs<'_, TabsValue>| capsule(lens_rect(inputs, vertical)));
    let lens_effect = Driven::new(move |inputs: &Inputs<'_, TabsValue>| {
        let rect = lens_rect(inputs, vertical);
        #[expect(
            clippy::cast_possible_truncation,
            reason = "a tab pill is tens to hundreds of points across, far inside f32"
        )]
        let (x, y, w, h) = (
            rect.x0 as f32,
            rect.y0 as f32,
            (rect.width() as f32).max(1.0),
            (rect.height() as f32).max(1.0),
        );
        let recipe = Recipe::for_element(w, h, Family::Clear, palette.appearance, translucency);
        let shape = Shape::rounded_rect(
            ElementRect::new(x, y, w, h),
            w.min(h) * 0.5,
            CornerCurve::Continuous,
        );
        materials::effect(&Element { shape, recipe })
    });
    let lens_opacity = Driven::new(|inputs: &Inputs<'_, TabsValue>| {
        let active = inputs.value.indicator_bounds().is_some() && active_lens(inputs.state);
        f32::from(u8::from(active))
    })
    .smoothstep_window(0.05, 0.55);
    Custom::new(
        (lens_shape, lens_effect),
        move |recorder, _frame, (shape, effect)| {
            recorder.backdrop_material(shape, GLASS_SHADER, capture, effect);
        },
    )
    .opacity(lens_opacity)
}

/// The look of a tab container in `spec` (§8.7).
///
/// One shared bar-level pill glides across items on the [`SEGMENTED_PILL`]
/// spring (`T_r = 0.400 s, ζ = 0.850`); when pressed or dragged, it scales by
/// `1 + 0.12·p` with elastic axis stretch and activates a clear refractive lens
/// over the tab labels via `smoothstep(0.05, 0.55)`.
pub(in super::super) fn look(glass: &Glass, spec: TabsSpec, _env: &Environment) -> TabsLook {
    let tab_bar = glass.clone();
    let bar = Custom::<(), _>::new((), move |recorder, frame, ()| {
        record_standalone(recorder, &tab_bar.tab_bar_surface(frame));
    });
    // A standalone glass capsule uses its rim highlight and cast shadow to
    // separate from content, so no hairline divider protrudes past the
    // capsule shoulders (§8.7, Colours).
    let divider = Custom::<(), _>::new((), |_recorder, _frame, ()| {});
    let vertical = spec.placement() == TabStyle::Sidebar;
    let mut motion = control_motion();
    motion.drag = Transition::new(
        spring_animation(dynamics::Spring::JELLY_DRAG_ACTIVE),
        spring_animation(dynamics::Spring::JELLY_DRAG_RELEASE),
    );
    motion.value = ValueMotion::Always(spring_animation(dynamics::Spring::SEGMENTED_PILL));
    let hit = HitRegion::NONE
        .minimum(Size::new(0.0, f64::from(MIN_HIT_SIDE)))
        .cancel_distance(f64::from(CANCEL_DISTANCE));
    let items = glass.clone();

    TabsLook::new(
        VERTICAL_METRICS,
        bar,
        divider,
        motion,
        move |item, _selected| {
            let size = item.geometry.bounds.size();
            let pill = Size::new(
                HIGHLIGHT_INSET.mul_add(-2.0, size.width).max(0.0),
                HIGHLIGHT_INSET.mul_add(-2.0, size.height).max(0.0),
            );
            Chrome::new(
                ChromeSurface::clear(CornerShape::full().smoothing(CONTINUOUS_SMOOTHING)),
                StateLayer::new(Glass::color(items.palette.state_layer)).opacities(
                    0.0,
                    HOVER_ALPHA,
                    0.0,
                    0.0,
                ),
                PressFeedback::new(),
                FocusIndicator::Ring {
                    width: FOCUS_RING_WIDTH,
                    offset: FOCUS_RING_GAP,
                    color: Glass::color(items.palette.accent),
                },
                control_motion(),
            )
            .centered(pill)
            .hit(hit)
        },
    )
    .horizontal(HORIZONTAL_BREAKPOINT, HORIZONTAL_METRICS)
    .bar_width(TabBarWidth::Compact {
        item_min_width: COMPACT_ITEM_MIN_WIDTH,
        max_width: None,
        horizontal_padding: 0.0,
    })
    .floating_margin(FLOATING_MARGIN)
    .selected_foreground(Glass::color(glass.palette.accent))
    .unselected_foreground(Glass::color(glass.palette.foreground))
    .drag(TabsDrag::new())
    .selection_indicator(rest_selection_indicator(glass))
    .selection_indicator_over_labels(lens_selection_indicator(glass, vertical))
}
