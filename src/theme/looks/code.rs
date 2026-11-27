//! The glass code-block look (specification §8.11): a continuous-corner
//! `SurfaceVariant` container with a hairline `Border` outline, `Caption`
//! medium header typography, a `MutedForeground` / `Accent` copy action tint,
//! and monospaced `Body` source text.

use hydrolysis_style::theme::{Border, CodeLook, CodeMetrics, CornerShape, Surface};
use waterui_text::font::{Body, Caption, Font, FontWeight};

use super::super::Glass;
use super::super::chrome::HAIRLINE;
use super::super::materials::CONTINUOUS_SMOOTHING;
use super::super::palette::working;

/// Corner radius of a code-block container, pt (§8.11).
const CODE_CORNER_RADIUS: f32 = 12.0;
/// Uniform inner padding of a code-block container, pt (§8.11).
const CODE_PADDING: f32 = 14.0;
/// Vertical gap between the header row and code body, pt (§8.11).
const CODE_HEADER_SPACING: f32 = 8.0;

/// The glass theme's look for a syntax-highlighted code block (§8.11).
pub(in super::super) fn look(glass: &Glass) -> CodeLook {
    let palette = glass.palette;
    let shape = CornerShape::radius(CODE_CORNER_RADIUS).smoothing(CONTINUOUS_SMOOTHING);
    let surface = Surface::new(shape, working(palette.surface_variant))
        .border(Border::new(HAIRLINE, working(palette.border)));
    CodeLook::new(
        surface,
        CodeMetrics::uniform(CODE_PADDING, CODE_HEADER_SPACING),
    )
    .header_font(Font::from(Caption).weight(FontWeight::Medium))
    .header_color(Glass::color(palette.muted_foreground))
    .copy_color(Glass::color(palette.muted_foreground))
    .copied_color(Glass::color(palette.accent))
    .body_font(Font::from(Body).monospaced(true))
}
