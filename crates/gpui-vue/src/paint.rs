//! Typed low-level painting bridge for custom `gpui-vue` visuals.
//!
//! Most interfaces should use [`crate::view!`]. A drawing surface is intended
//! for precision editors, charts, and other content whose paint and hit-test
//! geometry must share one native coordinate system.

pub use gpui::{
    App, BorderStyle, Bounds, BoxShadow, ContentMask, IntoElement, PathBuilder, Pixels,
    Point as ScreenPoint, Rgba, Window, bounds, fill, point, px, quad, rgba, size,
};

/// Builds a native drawing surface with paired prepaint and paint phases.
///
/// The value returned by `prepaint` is passed to `paint` for the same frame,
/// allowing layout-derived transforms to be calculated exactly once.
#[must_use]
pub fn drawing_surface<T: 'static>(
    prepaint: impl 'static + FnOnce(Bounds<Pixels>, &mut Window, &mut App) -> T,
    paint: impl 'static + FnOnce(Bounds<Pixels>, T, &mut Window, &mut App),
) -> gpui::Canvas<T> {
    gpui::canvas(prepaint, paint)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drawing_surface_keeps_prepaint_state_typed() {
        let _surface = drawing_surface(
            |_bounds, _window, _cx| 42_u8,
            |_bounds, state, _window, _cx| assert_eq!(state, 42),
        );
    }
}
