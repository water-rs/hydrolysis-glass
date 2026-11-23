//! The tabs look (specification §8.7): the bar a standalone glass capsule
//! with no separator, each item a capsule state layer with a focus ring,
//! and the selected item an inset soft capsule highlight that becomes a
//! clear refractive lens (`1 + 0.12·p`) while pressed.

use cherenkov_record::kurbo::{Rect, RoundedRectRadii, Size};
use cherenkov_record::{ContinuousRect, Draw as _, ShapeData};
use hydrolysis_style::theme::{
    CornerShape, Custom, Driven, HitRegion, Inputs, Motion, PressFeedback,
    Surface as ChromeSurface, TabItem, TabsLook, TabsMetrics, TabsSpec, ValueMotion,
};
use waterui_core::Environment;

use crate::material::geometry::{CornerCurve, Element, Rect as ElementRect, Shape};
use crate::material::recipe::{Family, Recipe};

use super::super::button::{CANCEL_DISTANCE, MIN_HIT_SIDE};
use super::super::materials::{self, CONTINUOUS_SMOOTHING, GLASS_SHADER};
use super::super::palette::alpha;
use super::super::segmented::SELECTION;
use super::super::{Glass, record_standalone};
use super::{control_motion, held, paint, state_chrome};

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
/// Inset of the selected highlight capsule inside the item's bounds, pt
/// (§8.7, Geometry).
const HIGHLIGHT_INSET: f64 = 4.0;
/// The alpha of the state-layer colour the selected item's highlight takes
/// (§8.7, Colours).
const HIGHLIGHT_ALPHA: f32 = 0.08;
/// Growth of a pressed tab pill about its centre (`1 + 0.12·p`).
const LENS_GROWTH: f64 = 0.12;

/// Continuous capsule shape filling `rect`.
fn capsule(rect: Rect) -> ShapeData {
    let radius = rect.height().min(rect.width()) * 0.5;
    ShapeData::Continuous(
        ContinuousRect::new(rect, RoundedRectRadii::from_single_radius(radius))
            .with_smoothing(CONTINUOUS_SMOOTHING),
    )
}

/// The look of a tab container in `spec` (§8.7).
///
/// The selection runs on the [`SELECTION`] spring (`T_r = 0.400 s, ζ = 0.85`);
/// a pressed item grows `1 + 0.12·p` into a clear refractive lens and settles
/// back to the translucent highlight on release.
pub(in super::super) fn look(glass: &Glass, _spec: TabsSpec, _env: &Environment) -> TabsLook {
    let tab_bar = glass.clone();
    let bar = Custom::<(), _>::new((), move |recorder, frame, ()| {
        record_standalone(recorder, &tab_bar.tab_bar_surface(frame));
    });
    // A standalone glass capsule uses its rim highlight and cast shadow to
    // separate from content, so no hairline divider protrudes past the
    // capsule shoulders (§8.7, Colours).
    let divider = Custom::<(), _>::new((), |_recorder, _frame, ()| {});
    let (palette, translucency) = (glass.palette, glass.translucency);
    let capture = materials::capture_class(
        false,
        Recipe::for_element(40.0, 40.0, Family::Clear, palette.appearance, translucency)
            .capture_scale,
    );
    let motion = Motion {
        value: ValueMotion::Always(SELECTION.physical()),
        ..control_motion()
    };
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
            let lens_shape = Driven::new(move |inputs: &Inputs<'_, ()>| {
                let scale = LENS_GROWTH.mul_add(f64::from(u8::from(held(inputs.state))), 1.0);
                capsule(Rect::from_center_size(
                    inputs.frame.center(),
                    inputs.frame.size() * scale,
                ))
            });
            let lens_effect = Driven::new(move |inputs: &Inputs<'_, ()>| {
                #[expect(
                    clippy::cast_possible_truncation,
                    reason = "a tab pill is tens to hundreds of points across, far inside f32"
                )]
                let (w, h) = (
                    (inputs.frame.width() as f32).max(1.0),
                    (inputs.frame.height() as f32).max(1.0),
                );
                let recipe =
                    Recipe::for_element(w, h, Family::Clear, palette.appearance, translucency);
                let shape = Shape::rounded_rect(
                    ElementRect::new(0.0, 0.0, w, h),
                    w.min(h) * 0.5,
                    CornerCurve::Continuous,
                );
                materials::effect(&Element { shape, recipe })
            });
            let lens = Custom::<(), _>::new(
                (lens_shape, lens_effect),
                move |recorder, _frame, (shape, effect)| {
                    recorder.backdrop_material(shape, GLASS_SHADER, capture, effect);
                },
            )
            .opacity(Driven::new(|inputs: &Inputs<'_, ()>| {
                f32::from(u8::from(held(inputs.state)))
            }));
            let mut chrome = state_chrome(
                &items,
                ChromeSurface::clear(CornerShape::full().smoothing(CONTINUOUS_SMOOTHING)),
            )
            .centered(pill)
            .hit(hit)
            .custom(lens);
            chrome.set_press(PressFeedback::new().scale(LENS_GROWTH));
            chrome
        },
    )
    .horizontal(HORIZONTAL_BREAKPOINT, HORIZONTAL_METRICS)
    .highlight(move |_item| {
        let highlight = Driven::new(move |inputs: &Inputs<'_, TabItem>| {
            let shown = f32::from(u8::from(inputs.value.selected && !held(inputs.state)));
            paint(alpha(palette.state_layer, HIGHLIGHT_ALPHA * shown))
        });
        Custom::<TabItem, _>::new((highlight,), |recorder, frame, (highlight,)| {
            recorder.fill(capsule(frame.inset(-HIGHLIGHT_INSET)), highlight);
        })
    })
}
