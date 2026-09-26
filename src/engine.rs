//! Rendering the material through the engine.
//!
//! The material needs a capture of the content behind each surface, a
//! spatial filter chain over that capture, and a per-member shader effect
//! sampled through it. The engine documents these as `BackdropGroup`,
//! `BackdropGroup::sample` and the `Backdrop` capability, but the current
//! crate cannot be consumed (its dependency graph does not resolve) and the
//! public API does not yet expose what the stages need. `ENGINE_REQUIREMENTS.md`
//! lists every missing piece exactly. Until those ship, enabling the `engine`
//! feature is a build error rather than a stand-in.

compile_error!(
    "hydrolysis-glass/engine: the engine does not yet expose the APIs the material needs; \
     see ENGINE_REQUIREMENTS.md for the exact list. Build without the `engine` feature."
);
