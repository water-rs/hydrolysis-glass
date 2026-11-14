//! Drawing for the parts of the theme that are not glass.
//!
//! Toggles, steppers, inputs, pickers, sliders, progress, badges, lists,
//! tables, dividers and the opaque menu panels take ordinary fills recorded
//! through the engine's [`Recorder`]. Nothing here samples a backdrop, so
//! nothing here is a stand-in for the material.

use cherenkov_record::kurbo::{
    BezPath, Circle, Line, Point, Rect, RoundedRect, RoundedRectRadii, Stroke, Vec2,
};
use cherenkov_record::{Draw as _, Recorder, Shadow};
use hydrolysis::theme::{RadioIndicatorState, WidgetInteractionState};
use waterui::interaction::InteractionState;
use waterui_graphics::WorkingColor;

use super::palette::{Palette, Rgba, alpha, over, working};

/// Opaque menu and popup corner radius, pt.
pub const MENU_CORNER_RADIUS: f64 = 14.0;
/// Hairline thickness, pt.
pub const HAIRLINE: f64 = 0.5;
/// Elevation shadow blur of opaque popups, pt.
pub const POPUP_SHADOW_BLUR: f64 = 24.0;
/// Vertical displacement of the opaque popups' elevation shadow, pt.
pub const POPUP_SHADOW_OFFSET: f64 = 8.0;
/// Alpha of the popup elevation shadow.
pub const POPUP_SHADOW_ALPHA: f32 = 0.18;
/// Focus ring thickness, pt.
pub const FOCUS_RING_WIDTH: f64 = 2.0;
/// Focus ring gap from the control, pt.
pub const FOCUS_RING_GAP: f64 = 2.0;
/// Alpha of a hover state layer.
pub const HOVER_ALPHA: f32 = 0.06;
/// Alpha of a pressed state layer.
pub const PRESSED_ALPHA: f32 = 0.12;

const fn uniform(radius: f64) -> RoundedRectRadii {
    RoundedRectRadii::from_single_radius(radius)
}

fn capsule(rect: Rect) -> RoundedRectRadii {
    uniform(rect.height().min(rect.width()) * 0.5)
}

fn rounded(rect: Rect, radii: impl Into<RoundedRectRadii>) -> RoundedRect {
    RoundedRect::from_rect(rect, radii)
}

fn mix(a: Rgba, b: Rgba, t: f32) -> Rgba {
    [
        (b[0] - a[0]).mul_add(t, a[0]),
        (b[1] - a[1]).mul_add(t, a[1]),
        (b[2] - a[2]).mul_add(t, a[2]),
        (b[3] - a[3]).mul_add(t, a[3]),
    ]
}

/// Draws a translucent state layer and focus ring for a rounded control.
pub fn state_layer(
    palette: &Palette,
    draw: &mut Recorder,
    rect: Rect,
    radii: RoundedRectRadii,
    state: WidgetInteractionState,
) {
    if state.state.contains(InteractionState::DISABLED) {
        return;
    }
    if state.state_layer_opacity > 0.0 {
        draw.fill(
            rounded(rect, radii),
            working(alpha(palette.state_layer, state.state_layer_opacity)),
        );
    }
    if state.state.contains(InteractionState::FOCUSED) && state.focus_progress > 0.0 {
        let inset = -FOCUS_RING_WIDTH.mul_add(0.5, FOCUS_RING_GAP);
        let ring = rect.inset(inset);
        let grow = -inset;
        let ring_radii = RoundedRectRadii::new(
            radii.top_left + grow,
            radii.top_right + grow,
            radii.bottom_right + grow,
            radii.bottom_left + grow,
        );
        draw.stroke(
            rounded(ring, ring_radii),
            Stroke::new(FOCUS_RING_WIDTH),
            working(alpha(palette.accent, state.focus_progress)),
        );
    }
}

/// Draws a state layer over a circular control.
pub fn circular_state_layer(
    palette: &Palette,
    draw: &mut Recorder,
    center: Point,
    radius: f64,
    state: WidgetInteractionState,
) {
    if state.state.contains(InteractionState::DISABLED) {
        return;
    }
    if state.state_layer_opacity > 0.0 {
        draw.fill(
            Circle::new(center, radius),
            working(alpha(palette.state_layer, state.state_layer_opacity)),
        );
    }
    if state.state.contains(InteractionState::FOCUSED) && state.focus_progress > 0.0 {
        draw.stroke(
            Circle::new(
                center,
                FOCUS_RING_WIDTH.mul_add(0.5, radius + FOCUS_RING_GAP),
            ),
            Stroke::new(FOCUS_RING_WIDTH),
            working(alpha(palette.accent, state.focus_progress)),
        );
    }
}

/// A text field: a soft filled rounded rectangle with an accent outline when
/// focused.
pub fn input_field(
    palette: &Palette,
    draw: &mut Recorder,
    bounds: Rect,
    state: WidgetInteractionState,
) {
    let radii = uniform(10.0);
    let mut fill = palette.track;
    if state.state.contains(InteractionState::DISABLED) {
        fill[3] *= 0.5;
    }
    draw.fill(rounded(bounds, radii), working(fill));
    if state.state.contains(InteractionState::FOCUSED) && state.focus_progress > 0.0 {
        draw.stroke(
            rounded(bounds.inset(-0.75), radii),
            Stroke::new(1.5),
            working(alpha(palette.accent, state.focus_progress)),
        );
    }
}

/// An opaque elevated panel: menus, popups, context menus.
pub fn popup_panel(palette: &Palette, draw: &mut Recorder, bounds: Rect) {
    let radii = uniform(MENU_CORNER_RADIUS);
    draw.shadow(
        rounded(bounds, radii),
        Shadow::new(
            POPUP_SHADOW_BLUR,
            working([0.0, 0.0, 0.0, POPUP_SHADOW_ALPHA]),
        )
        .offset(Vec2::new(0.0, POPUP_SHADOW_OFFSET)),
    );
    draw.fill(rounded(bounds, radii), working(palette.surface));
    draw.stroke(
        rounded(bounds.inset(-0.25), radii),
        Stroke::new(HAIRLINE),
        working(palette.border),
    );
}

/// A hairline separator filling `bounds`.
pub fn separator(palette: &Palette, draw: &mut Recorder, bounds: Rect) {
    draw.fill(bounds, working(palette.border));
}

/// The chevron pair of a menu picker.
pub fn picker_indicator(palette: &Palette, draw: &mut Recorder, bounds: Rect) {
    let c = bounds.center();
    let w = bounds.width().min(bounds.height()) * 0.18;
    let mut path = BezPath::new();
    path.move_to(Point::new(c.x - w, c.y - 1.5));
    path.line_to(Point::new(c.x, c.y - 1.5 - w));
    path.line_to(Point::new(c.x + w, c.y - 1.5));
    path.move_to(Point::new(c.x - w, c.y + 1.5));
    path.line_to(Point::new(c.x, c.y + 1.5 + w));
    path.line_to(Point::new(c.x + w, c.y + 1.5));
    draw.stroke(path, Stroke::new(1.8), working(palette.muted_foreground));
}

/// A popup row: the selected row carries a soft accent container.
pub fn popup_row_background(palette: &Palette, draw: &mut Recorder, row: Rect, selected: bool) {
    if selected {
        draw.fill(
            rounded(row.inset(-4.0), uniform(8.0)),
            working(alpha(palette.accent, 0.14)),
        );
    }
}

/// A radio indicator: an outer ring that turns accent, and an inner dot.
pub fn radio_indicator(
    palette: &Palette,
    draw: &mut Recorder,
    center: Point,
    radius: f64,
    state: RadioIndicatorState,
) {
    let ring = mix(
        palette.border,
        palette.accent,
        state.outer_selected_progress.clamp(0.0, 1.0),
    );
    draw.stroke(
        Circle::new(center, radius - 1.0),
        Stroke::new(2.0),
        working(ring),
    );
    if state.inner_opacity > 0.0 && state.inner_scale > 0.0 {
        draw.fill(
            Circle::new(center, radius * 0.5 * f64::from(state.inner_scale)),
            working(alpha(palette.accent, state.inner_opacity)),
        );
    }
}

/// A back chevron.
pub fn back_button(palette: &Palette, draw: &mut Recorder, bounds: Rect) {
    let c = bounds.center();
    let h = bounds.height() * 0.22;
    let mut path = BezPath::new();
    path.move_to(Point::new(h.mul_add(0.5, c.x), c.y - h));
    path.line_to(Point::new(h.mul_add(-0.5, c.x), c.y));
    path.line_to(Point::new(h.mul_add(0.5, c.x), c.y + h));
    draw.stroke(path, Stroke::new(2.5), working(palette.accent));
}

/// The selected-tab highlight: a soft capsule under the tab's content.
pub fn tabs_highlight(palette: &Palette, draw: &mut Recorder, bounds: Rect) {
    draw.fill(
        rounded(bounds, capsule(bounds)),
        working(alpha(palette.state_layer, 0.08)),
    );
}

/// A scroll indicator pill.
pub fn scroll_indicator(palette: &Palette, draw: &mut Recorder, bounds: Rect) {
    draw.fill(
        rounded(bounds, capsule(bounds)),
        working(alpha(palette.foreground, 0.35)),
    );
}

/// A small badge dot.
pub fn badge_small(palette: &Palette, draw: &mut Recorder, bounds: Rect) {
    draw.fill(
        Circle::new(bounds.center(), bounds.width().min(bounds.height()) * 0.5),
        working(palette.error),
    );
}

/// A large labelled badge capsule.
pub fn badge_large(palette: &Palette, draw: &mut Recorder, bounds: Rect) {
    draw.fill(rounded(bounds, capsule(bounds)), working(palette.error));
}

/// A list row background: alternate rows take the variant surface.
pub fn list_row_background(palette: &Palette, draw: &mut Recorder, bounds: Rect, alternate: bool) {
    if alternate {
        draw.fill(bounds, working(alpha(palette.state_layer, 0.03)));
    }
}

/// The reorder grip: three short bars.
pub fn list_move_control(palette: &Palette, draw: &mut Recorder, bounds: Rect) {
    let c = bounds.center();
    let half = bounds.width().min(bounds.height()) * 0.25;
    for dy in [-5.0, 0.0, 5.0] {
        draw.stroke(
            Line::new(
                Point::new(c.x - half, c.y + dy),
                Point::new(c.x + half, c.y + dy),
            ),
            Stroke::new(1.5),
            working(palette.muted_foreground),
        );
    }
}

/// The delete affordance: a red disc with a minus.
pub fn list_delete_control(palette: &Palette, draw: &mut Recorder, bounds: Rect) {
    let c = bounds.center();
    let r = bounds.width().min(bounds.height()) * 0.5;
    draw.fill(Circle::new(c, r), working(palette.error));
    draw.stroke(
        Line::new(
            Point::new(r.mul_add(-0.5, c.x), c.y),
            Point::new(r.mul_add(0.5, c.x), c.y),
        ),
        Stroke::new(2.0),
        working(palette.error_foreground),
    );
}

/// The background revealed behind a swiping row.
#[allow(
    clippy::cast_possible_truncation,
    reason = "progress is clamped to [0, 1] before use"
)]
pub fn list_swipe_dismiss_background(
    palette: &Palette,
    draw: &mut Recorder,
    bounds: Rect,
    progress: f64,
    toward_start: bool,
) {
    let t = progress.clamp(0.0, 1.0) as f32;
    draw.fill(
        bounds,
        working(alpha(palette.error, 0.4f32.mul_add(t, 0.6))),
    );
    let r = bounds.height().min(44.0) * 0.2;
    let x = if toward_start {
        bounds.x1 - 24.0
    } else {
        bounds.x0 + 24.0
    };
    let c = Point::new(x, bounds.center().y);
    draw.stroke(
        Line::new(Point::new(c.x - r, c.y), Point::new(c.x + r, c.y)),
        Stroke::new(2.0),
        working(palette.error_foreground),
    );
}

/// A row lifted for reordering.
pub fn list_row_lifted(palette: &Palette, draw: &mut Recorder, bounds: Rect, elevation: f64) {
    let radii = uniform(12.0);
    draw.shadow(
        rounded(bounds, radii),
        Shadow::new(elevation.max(1.0) * 2.0, working([0.0, 0.0, 0.0, 0.2]))
            .offset(Vec2::new(0.0, elevation.max(1.0) * 0.5)),
    );
    draw.fill(rounded(bounds, radii), working(palette.surface));
}

/// The table background.
pub fn table_background(palette: &Palette, draw: &mut Recorder, bounds: Rect) {
    draw.fill(bounds, working(palette.surface));
}

/// The table header band.
pub fn table_header_background(palette: &Palette, draw: &mut Recorder, bounds: Rect) {
    draw.fill(bounds, working(palette.surface_variant));
}

/// A cell border hairline.
pub fn table_cell_border(palette: &Palette, draw: &mut Recorder, bounds: Rect) {
    draw.stroke(bounds, Stroke::new(HAIRLINE), working(palette.border));
}

/// A column separator hairline.
pub fn table_column_separator(palette: &Palette, draw: &mut Recorder, from: Point, to: Point) {
    draw.stroke(
        Line::new(from, to),
        Stroke::new(HAIRLINE),
        working(palette.border),
    );
}

/// Disabled content colour: the foreground faded to `alpha` over the background.
#[must_use]
pub fn disabled_content(palette: &Palette, a: f32) -> WorkingColor {
    working(over(alpha(palette.foreground, a), palette.background))
}
