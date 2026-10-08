//! Drawing for the parts of the theme that are not glass.
//!
//! Toggles, steppers, inputs, pickers, sliders, progress, badges, lists,
//! tables, dividers and the opaque menu panels take ordinary fills recorded
//! through the engine's [`Recorder`]. Nothing here samples a backdrop, so
//! nothing here is a stand-in for the material.

use core::f64::consts::{PI, TAU};
use core::time::Duration;

use cherenkov::kurbo::{
    Affine, BezPath, Circle, Line, Point, Rect, RoundedRect, RoundedRectRadii, Stroke, Vec2,
};
use cherenkov::{Draw as _, Recorder, Shadow, WorkingColor};
use waterui::interaction::InteractionState;
use waterui_backend_core::widget::{RadioIndicatorState, StepperEnd, WidgetInteractionState};

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

fn lerp(a: f64, b: f64, t: f64) -> f64 {
    (b - a).mul_add(t, a)
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

/// A toggle switch: a track that crosses to the accent as it turns on, and a
/// knob that slides along it.
pub fn toggle_switch(
    palette: &Palette,
    draw: &mut Recorder,
    bounds: Rect,
    progress: f32,
    state: WidgetInteractionState,
) {
    let t = progress.clamp(0.0, 1.0);
    let mut track = mix(palette.track, palette.accent, t);
    if state.state.contains(InteractionState::DISABLED) {
        track[3] *= 0.5;
    }
    draw.fill(rounded(bounds, capsule(bounds)), working(track));
    let pad = 2.0;
    let r = bounds.height().mul_add(0.5, -pad).max(1.0);
    let x0 = bounds.x0 + pad + r;
    let x1 = bounds.x1 - pad - r;
    let center = Point::new(lerp(x0, x1, f64::from(t)), bounds.center().y);
    draw.shadow(
        rounded(
            Rect::from_center_size(center, (r * 2.0, r * 2.0)),
            uniform(r),
        ),
        Shadow::new(4.0, working([0.0, 0.0, 0.0, 0.2])).offset(Vec2::new(0.0, 2.0)),
    );
    draw.fill(Circle::new(center, r), working(palette.knob));
}

/// A checkbox: an outlined continuous square that fills with the accent and
/// draws its check mark as `progress` rises.
#[allow(
    clippy::many_single_char_names,
    reason = "the locals mirror the checkbox's mark geometry"
)]
pub fn toggle_checkbox(
    palette: &Palette,
    draw: &mut Recorder,
    bounds: Rect,
    progress: f32,
    state: WidgetInteractionState,
) {
    let t = progress.clamp(0.0, 1.0);
    let radii = uniform(bounds.width().min(bounds.height()) * 0.3);
    let mut fill = alpha(palette.accent, t);
    let mut outline = palette.border;
    if state.state.contains(InteractionState::DISABLED) {
        fill[3] *= 0.5;
        outline[3] *= 0.5;
    }
    draw.stroke(
        rounded(bounds.inset(-0.75), radii),
        Stroke::new(1.5),
        working(outline),
    );
    draw.fill(rounded(bounds, radii), working(fill));
    if t > 0.0 {
        let w = bounds.width();
        let h = bounds.height();
        let a = Point::new(0.22f64.mul_add(w, bounds.x0), 0.52f64.mul_add(h, bounds.y0));
        let b = Point::new(0.42f64.mul_add(w, bounds.x0), 0.72f64.mul_add(h, bounds.y0));
        let c = Point::new(0.78f64.mul_add(w, bounds.x0), 0.30f64.mul_add(h, bounds.y0));
        let mut path = BezPath::new();
        path.move_to(a);
        let first = f64::from((t * 2.0).min(1.0));
        path.line_to(a.lerp(b, first));
        if t > 0.5 {
            path.line_to(b.lerp(c, f64::from((t - 0.5) * 2.0)));
        }
        draw.stroke(
            path,
            Stroke::new(w * 0.12),
            working(palette.accent_foreground),
        );
    }
}

/// One half of a stepper: a capsule-ended pill split at the seam.
pub fn stepper_button(
    palette: &Palette,
    draw: &mut Recorder,
    bounds: Rect,
    end: StepperEnd,
    state: WidgetInteractionState,
) {
    let r = bounds.height() * 0.5;
    let radii = match end {
        StepperEnd::Decrement => RoundedRectRadii::new(r, 0.0, 0.0, r),
        StepperEnd::Increment => RoundedRectRadii::new(0.0, r, r, 0.0),
    };
    let mut fill = palette.track;
    if state.state.contains(InteractionState::DISABLED) {
        fill[3] *= 0.5;
    }
    draw.fill(rounded(bounds, radii), working(fill));
    state_layer(palette, draw, bounds, radii, state);
}

/// A minus glyph.
pub fn stepper_decrement_icon(palette: &Palette, draw: &mut Recorder, bounds: Rect) {
    let c = bounds.center();
    let half = bounds.width().min(bounds.height()) * 0.22;
    draw.stroke(
        Line::new(Point::new(c.x - half, c.y), Point::new(c.x + half, c.y)),
        Stroke::new(2.0),
        working(palette.foreground),
    );
}

/// A plus glyph.
pub fn stepper_increment_icon(palette: &Palette, draw: &mut Recorder, bounds: Rect) {
    stepper_decrement_icon(palette, draw, bounds);
    let c = bounds.center();
    let half = bounds.width().min(bounds.height()) * 0.22;
    draw.stroke(
        Line::new(Point::new(c.x, c.y - half), Point::new(c.x, c.y + half)),
        Stroke::new(2.0),
        working(palette.foreground),
    );
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

/// The segmented picker's outer track.
pub fn segmented_container(palette: &Palette, draw: &mut Recorder, bounds: Rect) {
    draw.fill(rounded(bounds, uniform(9.0)), working(palette.track));
}

/// One segment: the selected one is a raised knob.
pub fn segmented_segment(palette: &Palette, draw: &mut Recorder, bounds: Rect, selected: bool) {
    if selected {
        let knob = bounds.inset(-2.0);
        draw.shadow(
            rounded(knob, uniform(7.0)),
            Shadow::new(6.0, working([0.0, 0.0, 0.0, 0.12])).offset(Vec2::new(0.0, 2.0)),
        );
        draw.fill(
            rounded(knob, uniform(7.0)),
            working(palette.knob_for_segment()),
        );
    }
}

/// The slider track and its filled portion.
pub fn slider_track(
    palette: &Palette,
    draw: &mut Recorder,
    track: Rect,
    fill: Rect,
    state: WidgetInteractionState,
) {
    let mut a = palette.track;
    let mut b = palette.accent;
    if state.state.contains(InteractionState::DISABLED) {
        a[3] *= 0.5;
        b[3] *= 0.5;
    }
    draw.fill(rounded(track, capsule(track)), working(a));
    draw.fill(rounded(fill, capsule(fill)), working(b));
}

/// The slider handle.
pub fn slider_thumb(
    palette: &Palette,
    draw: &mut Recorder,
    center: Point,
    radius: f64,
    state: WidgetInteractionState,
) {
    draw.shadow(
        rounded(
            Rect::from_center_size(center, (radius * 2.0, radius * 2.0)),
            uniform(radius),
        ),
        Shadow::new(6.0, working([0.0, 0.0, 0.0, 0.24])).offset(Vec2::new(0.0, 2.0)),
    );
    let mut knob = palette.knob;
    if state.state.contains(InteractionState::DISABLED) {
        knob = over([0.5, 0.5, 0.5, 0.3], knob);
    }
    draw.fill(Circle::new(center, radius), working(knob));
}

/// The linear progress track, leaving `active_end..` at full track colour.
pub fn progress_linear_track(
    palette: &Palette,
    draw: &mut Recorder,
    bounds: Rect,
    active_end: Option<f64>,
) {
    let mut track = bounds;
    if let Some(x) = active_end {
        track.x0 = x.clamp(bounds.x0, bounds.x1);
    }
    draw.fill(rounded(track, capsule(bounds)), working(palette.track));
}

/// The linear progress fill.
pub fn progress_linear_fill(palette: &Palette, draw: &mut Recorder, bounds: Rect) {
    draw.fill(rounded(bounds, capsule(bounds)), working(palette.accent));
}

/// An indeterminate linear bar: one segment sweeping the track each cycle.
pub fn progress_linear_indeterminate(
    palette: &Palette,
    draw: &mut Recorder,
    bounds: Rect,
    elapsed: Duration,
    cycle: Duration,
) {
    draw.fill(rounded(bounds, capsule(bounds)), working(palette.track));
    let phase = (elapsed.as_secs_f64() / cycle.as_secs_f64().max(1e-3)).fract();
    let w = bounds.width();
    let seg = w * 0.35;
    let x0 = (w + seg).mul_add(phase, bounds.x0 - seg);
    let fill = Rect::new(
        x0.max(bounds.x0),
        bounds.y0,
        (x0 + seg).min(bounds.x1),
        bounds.y1,
    );
    if fill.width() > 0.0 {
        draw.fill(rounded(fill, capsule(bounds)), working(palette.accent));
    }
}

#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    reason = "arc subdivision counts are small positive values; kurbo wants f64 points"
)]
fn arc(center: Point, radius: f64, from: f64, to: f64) -> BezPath {
    let mut path = BezPath::new();
    let steps = ((to - from).abs() / (PI / 24.0)).ceil().max(1.0) as usize;
    for i in 0..=steps {
        let a = lerp(from, to, i as f64 / steps as f64);
        let p = Point::new(
            radius.mul_add(a.cos(), center.x),
            radius.mul_add(a.sin(), center.y),
        );
        if i == 0 {
            path.move_to(p);
        } else {
            path.line_to(p);
        }
    }
    path
}

/// The circular progress track.
pub fn progress_circular_track(
    palette: &Palette,
    draw: &mut Recorder,
    center: Point,
    radius: f64,
    width: f64,
    active_turns: Option<f64>,
) {
    let start = active_turns.map_or(0.0, |t| t.clamp(0.0, 1.0) * TAU) - PI / 2.0;
    draw.stroke(
        arc(center, radius, start, TAU - PI / 2.0),
        Stroke::new(width),
        working(palette.track),
    );
}

/// The circular progress fill along a renderer-supplied path.
pub fn progress_circular_fill(palette: &Palette, draw: &mut Recorder, path: &BezPath, width: f64) {
    draw.stroke(path.clone(), Stroke::new(width), working(palette.accent));
}

/// An indeterminate circular indicator: a rotating arc that breathes.
pub fn progress_circular_indeterminate(
    palette: &Palette,
    draw: &mut Recorder,
    center: Point,
    radius: f64,
    width: f64,
    elapsed: Duration,
    cycle: Duration,
) {
    let phase = (elapsed.as_secs_f64() / cycle.as_secs_f64().max(1e-3)).fract();
    let start = phase * TAU * 2.0;
    let sweep = lerp(0.15, 0.7, 0.5f64.mul_add(-(phase * TAU).cos(), 0.5)) * TAU;
    draw.stroke(
        arc(center, radius, start, start + sweep),
        Stroke::new(width),
        working(palette.accent),
    );
}

/// The loading indicator: a rounded square that rotates and softens into a
/// circle and back.
pub fn progress_loading(
    palette: &Palette,
    draw: &mut Recorder,
    bounds: Rect,
    size: f64,
    elapsed: Duration,
    cycle: Duration,
) {
    let phase = (elapsed.as_secs_f64() / cycle.as_secs_f64().max(1e-3)).fract();
    let center = bounds.center();
    let morph = 0.5f64.mul_add(-(phase * TAU * 3.0).cos(), 0.5);
    let radius = lerp(size * 0.25, size * 0.5, morph);
    draw.transform(
        Affine::translate(center.to_vec2()) * Affine::rotate(phase * TAU),
        |draw| {
            draw.fill(
                rounded(
                    Rect::from_center_size(Point::ORIGIN, (size, size)),
                    uniform(radius),
                ),
                working(palette.accent),
            );
        },
    );
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

impl Palette {
    /// The knob colour of a selected segment: the surface in light, a lifted
    /// variant in dark so the knob reads against the track.
    #[must_use]
    pub fn knob_for_segment(&self) -> Rgba {
        match self.scheme() {
            waterui::color::ColorScheme::Light => self.surface,
            waterui::color::ColorScheme::Dark => over([1.0, 1.0, 1.0, 0.18], self.surface),
        }
    }
}

/// Disabled content colour: the foreground faded to `alpha` over the background.
#[must_use]
pub fn disabled_content(palette: &Palette, a: f32) -> WorkingColor {
    working(over(alpha(palette.foreground, a), palette.background))
}
