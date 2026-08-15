//! Curated native overlay primitives for popups and floating controls.
//!
//! GPUI supplies two complementary layout seams: a deferred element paints
//! after its ancestors, while an anchored element positions content without
//! overflowing the native window. This module composes those primitives behind
//! `gpui-vue` types so applications do not need to reach through
//! `gpui_vue::gpui`.
//!
//! These helpers do not move content into a different component tree. Event,
//! focus, ownership, and lifecycle semantics remain those of the original GPUI
//! element tree, so this is intentionally not presented as Vue Teleport or as
//! a process-wide overlay registry.

use crate::ui::ScreenPoint;
use gpui::{AnyElement, Edges, IntoElement, ParentElement as _, Pixels, point, px};

/// Corner of an overlay placed at an anchor point.
///
/// The chosen corner belongs to the overlay itself. For example,
/// [`TopLeft`](Self::TopLeft) places the overlay's top-left corner at the
/// anchor point.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum OverlayCorner {
    /// Anchor the overlay by its top-left corner.
    #[default]
    TopLeft,
    /// Anchor the overlay by its top-right corner.
    TopRight,
    /// Anchor the overlay by its bottom-left corner.
    BottomLeft,
    /// Anchor the overlay by its bottom-right corner.
    BottomRight,
}

impl OverlayCorner {
    /// Converts the public overlay corner into the pinned GPUI representation.
    const fn into_native(self) -> gpui::Corner {
        match self {
            Self::TopLeft => gpui::Corner::TopLeft,
            Self::TopRight => gpui::Corner::TopRight,
            Self::BottomLeft => gpui::Corner::BottomLeft,
            Self::BottomRight => gpui::Corner::BottomRight,
        }
    }
}

/// Coordinate space used by an anchored overlay's explicit position.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum OverlayPositionMode {
    /// Interpret the position in native window coordinates.
    #[default]
    Window,
    /// Interpret the position relative to the overlay's parent layout bounds.
    Local,
}

impl OverlayPositionMode {
    /// Converts the public coordinate mode into the pinned GPUI representation.
    const fn into_native(self) -> gpui::AnchoredPositionMode {
        match self {
            Self::Window => gpui::AnchoredPositionMode::Window,
            Self::Local => gpui::AnchoredPositionMode::Local,
        }
    }
}

/// Logical-pixel margins used when fitting an overlay inside its window.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct OverlayInsets {
    /// Margin from the top window edge.
    pub top: f32,
    /// Margin from the right window edge.
    pub right: f32,
    /// Margin from the bottom window edge.
    pub bottom: f32,
    /// Margin from the left window edge.
    pub left: f32,
}

impl OverlayInsets {
    /// Creates independent top, right, bottom, and left margins.
    #[must_use]
    pub const fn new(top: f32, right: f32, bottom: f32, left: f32) -> Self {
        Self {
            top,
            right,
            bottom,
            left,
        }
    }

    /// Creates the same margin on all four window edges.
    #[must_use]
    pub const fn all(margin: f32) -> Self {
        Self::new(margin, margin, margin, margin)
    }

    /// Converts logical scalar values into GPUI's typed pixel edges.
    fn into_native(self) -> Edges<Pixels> {
        Edges {
            top: px(self.top),
            right: px(self.right),
            bottom: px(self.bottom),
            left: px(self.left),
        }
    }
}

impl From<f32> for OverlayInsets {
    fn from(margin: f32) -> Self {
        Self::all(margin)
    }
}

/// Strategy used when anchored content would overflow the native window.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum OverlayFit {
    /// Try the opposite anchor corner before snapping to a window edge.
    #[default]
    SwitchAnchor,
    /// Keep the selected corner and snap overflowing bounds to the window.
    SnapToWindow,
    /// Snap overflowing bounds while retaining the supplied edge margins.
    SnapToWindowWithMargin(OverlayInsets),
}

/// Overlay whose paint is deferred until after all of its ancestors.
///
/// Deferred overlays retain their original layout position, component owner,
/// event routing, and focus semantics. Only their draw ordering changes.
/// Higher priorities paint above lower-priority deferred draws in the same
/// window frame.
pub struct DeferredOverlay {
    /// Child that participates in normal layout but paints later.
    child: AnyElement,
    /// Relative order among native deferred draws.
    priority: usize,
}

impl DeferredOverlay {
    /// Sets the draw priority relative to other deferred overlays.
    ///
    /// Higher values are drawn on top of lower values. Equal priorities retain
    /// GPUI's native scheduling order.
    #[must_use]
    pub const fn priority(mut self, priority: usize) -> Self {
        self.priority = priority;
        self
    }
}

impl IntoElement for DeferredOverlay {
    type Element = gpui::Deferred;

    fn into_element(self) -> Self::Element {
        gpui::deferred(self.child).with_priority(self.priority)
    }
}

/// Defers a child's paint so popup content is not occluded by its ancestors.
///
/// The child still participates in its original layout and element tree. Avoid
/// nesting deferred overlays; use one deferred boundary around the complete
/// floating subtree.
///
/// ```no_run
/// use gpui_vue::{overlay::deferred_overlay, prelude::*, ui::div};
///
/// let popup = deferred_overlay(div().child("Menu")).priority(2);
/// ```
#[must_use]
pub fn deferred_overlay(child: impl IntoElement) -> DeferredOverlay {
    DeferredOverlay {
        child: child.into_any_element(),
        priority: 0,
    }
}

/// Window-aware anchored overlay configuration.
///
/// Children should not carry margins: native anchored measurement derives one
/// combined child bound before choosing or snapping the final position. Put
/// spacing inside the child instead.
pub struct AnchoredOverlay {
    /// Popup or floating control being positioned.
    child: AnyElement,
    /// Corner of the child attached to the anchor position.
    corner: OverlayCorner,
    /// Explicit anchor position, or the element's rendered position when none.
    position: Option<ScreenPoint>,
    /// Offset applied after resolving the anchor position.
    offset: Option<ScreenPoint>,
    /// Coordinate space for an explicit position.
    position_mode: OverlayPositionMode,
    /// Overflow fitting behavior.
    fit: OverlayFit,
}

impl AnchoredOverlay {
    /// Selects which corner of the overlay attaches to its anchor point.
    #[must_use]
    pub const fn anchor(mut self, corner: OverlayCorner) -> Self {
        self.corner = corner;
        self
    }

    /// Sets an explicit anchor position.
    ///
    /// The point is interpreted according to
    /// [`position_mode`](Self::position_mode). Without an explicit position,
    /// GPUI uses the element's rendered position.
    #[must_use]
    pub const fn at(mut self, position: ScreenPoint) -> Self {
        self.position = Some(position);
        self
    }

    /// Sets an explicit anchor position from logical pixel scalars.
    #[must_use]
    pub fn at_xy(self, x: f32, y: f32) -> Self {
        self.at(point(px(x), px(y)))
    }

    /// Offsets the final position by a typed logical-pixel point.
    #[must_use]
    pub const fn offset(mut self, offset: ScreenPoint) -> Self {
        self.offset = Some(offset);
        self
    }

    /// Offsets the final position by logical pixel scalars.
    #[must_use]
    pub fn offset_xy(self, x: f32, y: f32) -> Self {
        self.offset(point(px(x), px(y)))
    }

    /// Selects whether explicit positions are window-relative or parent-local.
    #[must_use]
    pub const fn position_mode(mut self, mode: OverlayPositionMode) -> Self {
        self.position_mode = mode;
        self
    }

    /// Selects the overflow fitting strategy.
    #[must_use]
    pub const fn fit(mut self, fit: OverlayFit) -> Self {
        self.fit = fit;
        self
    }

    /// Snaps overflowing content to the window edge without a margin.
    #[must_use]
    pub const fn snap_to_window(self) -> Self {
        self.fit(OverlayFit::SnapToWindow)
    }

    /// Snaps overflowing content to the window while retaining edge margins.
    ///
    /// Passing a scalar applies one logical-pixel margin to every edge;
    /// [`OverlayInsets`] provides independent values.
    #[must_use]
    pub fn snap_to_window_with_margin(self, margin: impl Into<OverlayInsets>) -> Self {
        self.fit(OverlayFit::SnapToWindowWithMargin(margin.into()))
    }
}

impl IntoElement for AnchoredOverlay {
    type Element = gpui::Anchored;

    fn into_element(self) -> Self::Element {
        let mut anchored = gpui::anchored()
            .anchor(self.corner.into_native())
            .position_mode(self.position_mode.into_native());

        if let Some(position) = self.position {
            anchored = anchored.position(position);
        }
        if let Some(offset) = self.offset {
            anchored = anchored.offset(offset);
        }

        anchored = match self.fit {
            OverlayFit::SwitchAnchor => anchored,
            OverlayFit::SnapToWindow => anchored.snap_to_window(),
            OverlayFit::SnapToWindowWithMargin(margin) => {
                anchored.snap_to_window_with_margin(margin.into_native())
            }
        };

        anchored.child(self.child)
    }
}

/// Positions a child against an anchor point while keeping it inside the
/// native window.
///
/// Anchoring controls placement but does not change paint order. Wrap the
/// result in [`deferred_overlay`] for a conventional popup layer.
///
/// ```no_run
/// use gpui_vue::{
///     overlay::{OverlayCorner, anchored_overlay, deferred_overlay},
///     prelude::*,
///     ui::div,
/// };
///
/// let popup = deferred_overlay(
///     anchored_overlay(div().child("Actions"))
///         .anchor(OverlayCorner::TopRight)
///         .offset_xy(0.0, 6.0)
///         .snap_to_window_with_margin(8.0),
/// )
/// .priority(1);
/// ```
#[must_use]
pub fn anchored_overlay(child: impl IntoElement) -> AnchoredOverlay {
    AnchoredOverlay {
        child: child.into_any_element(),
        corner: OverlayCorner::TopLeft,
        position: None,
        offset: None,
        position_mode: OverlayPositionMode::Window,
        fit: OverlayFit::SwitchAnchor,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn corners_map_exactly_to_native_corners() {
        assert_eq!(OverlayCorner::TopLeft.into_native(), gpui::Corner::TopLeft);
        assert_eq!(
            OverlayCorner::TopRight.into_native(),
            gpui::Corner::TopRight
        );
        assert_eq!(
            OverlayCorner::BottomLeft.into_native(),
            gpui::Corner::BottomLeft
        );
        assert_eq!(
            OverlayCorner::BottomRight.into_native(),
            gpui::Corner::BottomRight
        );
    }

    #[test]
    fn position_modes_map_exactly_to_native_modes() {
        assert!(matches!(
            OverlayPositionMode::Window.into_native(),
            gpui::AnchoredPositionMode::Window
        ));
        assert!(matches!(
            OverlayPositionMode::Local.into_native(),
            gpui::AnchoredPositionMode::Local
        ));
    }

    #[test]
    fn scalar_margin_expands_to_every_pixel_edge() {
        let edges = OverlayInsets::from(8.0).into_native();

        assert_eq!(edges.top, px(8.0));
        assert_eq!(edges.right, px(8.0));
        assert_eq!(edges.bottom, px(8.0));
        assert_eq!(edges.left, px(8.0));
    }

    #[test]
    fn asymmetric_margins_preserve_edge_order() {
        let edges = OverlayInsets::new(1.0, 2.0, 3.0, 4.0).into_native();

        assert_eq!(edges.top, px(1.0));
        assert_eq!(edges.right, px(2.0));
        assert_eq!(edges.bottom, px(3.0));
        assert_eq!(edges.left, px(4.0));
    }

    /// Compile-only fixture proving the public builders compose into an
    /// element without naming GPUI's concrete anchored or deferred types.
    #[allow(dead_code, reason = "compile-time API fixture")]
    fn typed_popup_fixture() -> impl IntoElement {
        deferred_overlay(
            anchored_overlay(crate::ui::div().child("Popup"))
                .anchor(OverlayCorner::BottomLeft)
                .at_xy(120.0, 64.0)
                .offset_xy(0.0, 6.0)
                .position_mode(OverlayPositionMode::Window)
                .snap_to_window_with_margin(OverlayInsets::all(8.0)),
        )
        .priority(3)
    }
}
