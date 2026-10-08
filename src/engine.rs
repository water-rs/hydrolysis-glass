//! Rendering the material through the engine.
//!
//! The material needs a capture of the content behind each surface at the
//! recipe's capture scale, a mip-mapped spatial chain over that capture, a
//! per-member effect evaluated against the union field, and widget chrome
//! that records a backdrop sample. The engine ships `BackdropGroup`,
//! `BackdropGroup::sample_with` and `BackdropShader`, but capture scale,
//! mip-level sampling, the union field and the widget-theme path are still
//! missing. `ENGINE_REQUIREMENTS.md` lists every piece and the issue
//! tracking it. Until those ship, enabling the `engine` feature is a build
//! error rather than a stand-in.

compile_error!(
    "hydrolysis-glass/engine: the engine does not yet expose the APIs the material needs; \
     see ENGINE_REQUIREMENTS.md for the exact list (water-rs/waterui#1786, #1787, #1788). \
     Build without the `engine` feature."
);
