//! The glass material: geometry, recipes and the wgpu renderer.
//!
//! The stages follow the behavioural specification's per-pixel evaluation
//! order: coverage from the field, the edge/shadow term where coverage is
//! partial, the refracted and matrixed interior where it is nonzero, the SDR
//! holding band, the output clamp, then the highlight and tint siblings.

pub mod geometry;
pub mod gpu;
pub mod recipe;

pub use geometry::{CornerCurve, Rect, Shape, UnionField};
pub use gpu::{Element, GlassRenderer, Group, Scene};
pub use recipe::{Appearance, Recipe, Tint, Variant};
