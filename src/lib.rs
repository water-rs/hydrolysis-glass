#![cfg_attr(
    test,
    allow(
        clippy::float_cmp,
        reason = "tests assert exact recipe values from the specification"
    )
)]
#![doc = include_str!("../README.md")]

#[cfg(feature = "engine")]
mod engine;
pub mod interaction;
pub mod material;
pub mod theme;

pub use interaction::{Controls, Interactive, Member};
pub use material::{Appearance, Element, GlassRenderer, Group, Recipe, Scene, Shape, Variant};
pub use theme::Glass;
