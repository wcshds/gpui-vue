//! Native time-based animation helpers.
//!
//! These are the curated GPUI primitives for animating one retained element.
//! Higher-level enter/leave coordination, list transitions, and reduced-motion
//! policy remain application concerns.

pub use gpui::{Animation, AnimationElement, AnimationExt};

/// Native easing functions accepted by [`Animation::with_easing`].
pub mod easing {
    pub use gpui::{ease_in_out, ease_out_quint, linear, quadratic};
}
