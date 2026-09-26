#![cfg_attr(
    test,
    allow(
        clippy::float_cmp,
        reason = "tests assert exact recipe values from the specification"
    )
)]
#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap,
    reason = "pixel/texel sizes and half-float packing convert between integer and f32 by design"
)]
#![allow(
    clippy::many_single_char_names,
    clippy::too_long_first_doc_paragraph,
    clippy::items_after_statements,
    clippy::struct_excessive_bools,
    clippy::too_many_lines,
    clippy::missing_panics_doc,
    reason = "the material mirrors the specification's notation and the WGSL stage structure"
)]
#![doc = include_str!("../README.md")]

pub mod interaction;
pub mod material;

pub use interaction::{Controls, Interactive, Member};
pub use material::{Appearance, Element, GlassRenderer, Group, Recipe, Scene, Shape, Variant};
