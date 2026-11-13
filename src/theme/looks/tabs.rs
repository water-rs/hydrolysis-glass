//! The tabs look: the bar a standalone glass tab bar over a hairline
//! divider, each item a capsule state layer with a focus ring, and the
//! selected item a soft capsule highlight.

use core::time::Duration;

use cherenkov_record::Draw as _;
use cherenkov_record::kurbo::RoundedRect;
use hydrolysis::theme::{
    CornerShape, Custom, Driven, Inputs, Motion, Surface as ChromeSurface, TabItem, TabsLook,
    TabsMetrics, TabsSpec, ValueMotion,
};
use waterui::animation::Animation;
use waterui_core::Environment;

use super::super::palette::alpha;
use super::super::{Glass, chrome, record_standalone};
use super::{control_motion, paint, state_chrome};

/// The measures of the items, laid out vertically at every width: a 60 pt
/// bar, items at least 64 pt wide with their labels inset 12 pt, and 6 pt
/// between an icon and its label.
const METRICS: TabsMetrics = TabsMetrics::new(60.0, 64.0, 12.0, 6.0);
/// The alpha of the state-layer colour the selected item's highlight takes.
const HIGHLIGHT_ALPHA: f32 = 0.08;

/// The look of a tab container in `spec`.
///
/// The highlight moves to a newly selected item at once; hover, focus and
/// press ease the items' state layers as on every control.
pub(in super::super) fn look(glass: &Glass, _spec: TabsSpec, _env: &Environment) -> TabsLook {
    let tab_bar = glass.clone();
    let bar = Custom::<(), _>::new((), move |recorder, frame, ()| {
        record_standalone(recorder, &tab_bar.tab_bar_surface(frame));
    });
    let palette = glass.palette;
    let divider = Custom::<(), _>::new((), move |recorder, frame, ()| {
        chrome::separator(&palette, recorder, frame);
    });
    let motion = Motion {
        value: ValueMotion::Own(Animation::linear(Duration::ZERO)),
        ..control_motion()
    };
    let items = glass.clone();
    TabsLook::new(METRICS, bar, divider, motion, move |_item, _selected| {
        state_chrome(&items, ChromeSurface::clear(CornerShape::full()))
    })
    .highlight(move |_item| {
        let highlight = Driven::new(move |inputs: &Inputs<'_, TabItem>| {
            let shown = f32::from(u8::from(inputs.value.selected));
            paint(alpha(palette.state_layer, HIGHLIGHT_ALPHA * shown))
        });
        Custom::<TabItem, _>::new((highlight,), |recorder, frame, (highlight,)| {
            recorder.fill(
                RoundedRect::from_rect(frame, frame.height().min(frame.width()) * 0.5),
                highlight,
            );
        })
    })
}
