//! The glass material: geometry, recipes and the wgpu renderer.
//!
//! The stages follow the specification's per-pixel evaluation order: the
//! cast shadow outside the silhouette, the captured and filtered backdrop
//! interior (refraction, depth blur, haze, luminance limit, grade, ambient
//! pickup, contour shade, border correction), the channel ceiling, then the
//! highlight and tint siblings and the dispersed content.

pub mod geometry;
pub mod gpu;
pub mod recipe;

pub use geometry::{CornerCurve, Element, Rect, Shape, UnionField};
pub use gpu::{GlassRenderer, Group, Scene};
pub use recipe::{Appearance, Family, Recipe, Tint, Translucency};
