//! The tabs look (specification §8.7): the bar a standalone glass capsule
//! with no separator, each item a capsule state layer with a focus ring,
//! and the selected item an inset soft capsule highlight.

use cherenkov_record::Draw as _;
use cherenkov_record::kurbo::{RoundedRect, Size};
use hydrolysis_style::theme::{
    CornerShape, Custom, Driven, HitRegion, Inputs, Motion, Surface as ChromeSurface, TabItem,
    TabsLook, TabsMetrics, TabsSpec, ValueMotion,
};
use waterui::animation::Animation;
use waterui_core::Environment;

use super::super::button::{CANCEL_DISTANCE, MIN_HIT_SIDE};
use super::super::palette::alpha;
use super::super::{Glass, PRESS_DURATION, record_standalone};
use super::{control_motion, paint, state_chrome};

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

/// The look of a tab container in `spec` (§8.7).
///
/// The highlight cross-fades across selection changes over
/// [`PRESS_DURATION`]; hover, focus and press ease the items' state layers
/// as on every control.
pub(in super::super) fn look(glass: &Glass, _spec: TabsSpec, _env: &Environment) -> TabsLook {
    let tab_bar = glass.clone();
    let bar = Custom::<(), _>::new((), move |recorder, frame, ()| {
        record_standalone(recorder, &tab_bar.tab_bar_surface(frame));
    });
    // A standalone glass capsule uses its rim highlight and cast shadow to
    // separate from content, so no hairline divider protrudes past the
    // capsule shoulders (§8.7, Colours).
    let divider = Custom::<(), _>::new((), |_recorder, _frame, ()| {});
    let palette = glass.palette;
    let motion = Motion {
        value: ValueMotion::Own(Animation::ease_out(PRESS_DURATION)),
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
        move |_item, _selected| {
            state_chrome(&items, ChromeSurface::clear(CornerShape::full())).hit(hit)
        },
    )
    .horizontal(HORIZONTAL_BREAKPOINT, HORIZONTAL_METRICS)
    .highlight(move |_item| {
        let highlight = Driven::new(move |inputs: &Inputs<'_, TabItem>| {
            let shown = f32::from(u8::from(inputs.value.selected));
            paint(alpha(palette.state_layer, HIGHLIGHT_ALPHA * shown))
        });
        Custom::<TabItem, _>::new((highlight,), |recorder, frame, (highlight,)| {
            let pill = frame.inset(-HIGHLIGHT_INSET);
            recorder.fill(
                RoundedRect::from_rect(pill, pill.height().min(pill.width()) * 0.5),
                highlight,
            );
        })
    })
}
