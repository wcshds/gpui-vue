//! Native vector editing canvas for the KAGE Editor example package.
//!
//! This module intentionally owns no editor state.  [`CanvasSnapshot`] freezes
//! the model and the transient pointer overlays for one frame, while
//! [`CanvasTransform`] is the single source of truth for painting and hit
//! testing.  Keeping those responsibilities here makes the root view's input
//! handlers small and prevents subtle zoom-coordinate drift.

#![allow(dead_code)]

use std::collections::BTreeSet;

use gpui_vue::paint::{
    App, BorderStyle, Bounds, BoxShadow, ContentMask, IntoElement, PathBuilder, Pixels, Rgba,
    ScreenPoint, Window, bounds, drawing_surface, fill, point, px, quad, rgba, size,
};
use gpui_vue::prelude::*;

use super::engine;
use super::model::{
    CONNECTION_TOLERANCE, CenterlineMode, ControlPointRef, DESIGN_SIZE, EditorModel,
    EditorSettings, MaskMode, Point, Rect, Stroke, StrokeId, StrokeKind, Typeface,
};

/// Empty space kept around a fitted artboard at 100% zoom.
const FIT_PADDING: f32 = 34.0;

/// Smallest useful scale when a canvas is temporarily collapsed by layout.
const MIN_SCALE: f32 = 0.01;

/// Pointer radius, in screen pixels, for control-point hit testing.
const CONTROL_HIT_RADIUS: f32 = 12.0;

/// Pointer radius, in screen pixels, for skeleton hit testing.
const STROKE_HIT_RADIUS: f32 = 6.0;

/// Visual size of one control-point marker.
const CONTROL_SIZE: f32 = 16.0;

/// Visual size of one selection resize handle.
const RESIZE_HANDLE_SIZE: f32 = 16.0;

/// Corner radius shared by every square control and resize marker.
const CONTROL_CORNER_RADIUS: f32 = 1.5;

/// Fixed inset used by the KAGE em-box guides.
const EM_GUIDE_INSET: f32 = 12.0;

/// Maximum number of grid divisions painted along either axis.
const MAX_GRID_LINES: i32 = 512;

/// Active interaction/presentation mode supplied by the root view.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum InteractionMode {
    /// Ordinary selection and direct-manipulation mode.
    #[default]
    Select,
    /// Freehand gesture capture mode.
    Freehand,
    /// Final-glyph preview without editing furniture.
    FinalGlyph,
}

/// Transient information supplied by the root view for one paint pass.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct CanvasOverlay {
    /// Control point currently being dragged.
    pub active_control: Option<ControlPointRef>,
    /// Control point currently under the pointer.
    pub hovered_control: Option<ControlPointRef>,
    /// Stroke currently under the pointer when no point has priority.
    pub hovered_stroke: Option<StrokeId>,
    /// Current pointer position in KAGE design coordinates.
    pub pointer: Option<Point>,
    /// Design-space translation applied after centering the artboard.
    pub pan: Point,
    /// Uncommitted freehand samples in KAGE design coordinates.
    pub freehand: Vec<Point>,
    /// Uncommitted area-selection rectangle in KAGE design coordinates.
    pub marquee: Option<Rect>,
    /// Current editor interaction or clean-preview mode.
    pub mode: InteractionMode,
    /// White-knockout shape selected by the editor UI.
    pub mask_mode: MaskMode,
}

/// Immutable paint data derived from an [`EditorModel`].
#[derive(Clone, Debug)]
pub(crate) struct CanvasSnapshot {
    /// Requested zoom where `1.0` means fit-to-canvas.
    zoom: f32,
    /// Monotonic model revision used by callers that cache snapshots.
    revision: u64,
    /// Persistent rendering preferences.
    settings: EditorSettings,
    /// Ordered editable records.
    strokes: Vec<Stroke>,
    /// Selected record identities.
    selection: BTreeSet<StrokeId>,
    /// Union of selected record control bounds.
    selection_bounds: Option<Rect>,
    /// Filled polygons produced by the real KAGE engine.
    outlines: Vec<engine::Outline>,
    /// Final engine polygons partitioned by their owning source record.
    record_outlines: Vec<Vec<engine::Outline>>,
    /// Ordinary paths after recursive component expansion for centerlines.
    centerline_strokes: Vec<Stroke>,
    /// Pointer and gesture state owned by the root view.
    overlay: CanvasOverlay,
}

impl CanvasSnapshot {
    /// Freezes a model and renders its engine polygons for a paint pass.
    #[must_use]
    pub(crate) fn from_model(model: &EditorModel, zoom: f32, overlay: CanvasOverlay) -> Self {
        let source = model.to_kage();
        let gothic = matches!(model.settings().typeface, Typeface::Gothic);
        let skeleton = matches!(model.settings().typeface, Typeface::Skeleton);
        let outlines = if skeleton {
            Vec::new()
        } else {
            engine::render_outlines(
                &source,
                model.component_library(),
                gothic,
                model.settings().use_curve,
            )
        };
        let record_outlines = if skeleton {
            vec![Vec::new(); model.strokes().len()]
        } else {
            render_record_outlines(model)
        };
        let centerline_strokes =
            if skeleton || matches!(model.settings().centerline, CenterlineMode::Always) {
                expanded_centerline_strokes(model)
            } else {
                model.strokes().to_vec()
            };

        Self {
            zoom: sanitize_zoom(zoom),
            revision: model.revision(),
            settings: *model.settings(),
            strokes: model.strokes().to_vec(),
            selection: model.selection().clone(),
            selection_bounds: model.selection_bounds(),
            outlines,
            record_outlines,
            centerline_strokes,
            overlay,
        }
    }

    /// Returns the source model revision represented by this frame.
    #[must_use]
    pub(crate) const fn revision(&self) -> u64 {
        self.revision
    }

    /// Returns the effective fit-relative zoom.
    #[must_use]
    pub(crate) const fn zoom(&self) -> f32 {
        self.zoom
    }

    /// Returns transient overlay data for input-state reconciliation.
    #[must_use]
    pub(crate) const fn overlay(&self) -> &CanvasOverlay {
        &self.overlay
    }
}

/// Bidirectional mapping between KAGE's 200-unit space and GPUI pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct CanvasTransform {
    /// Full element bounds supplied by GPUI.
    canvas: Bounds<Pixels>,
    /// Screen-space bounds occupied by the 200-by-200 artboard.
    artboard: Bounds<Pixels>,
    /// Logical pixels per design unit.
    scale: f32,
}

impl CanvasTransform {
    /// Creates a centered fit transform and applies a fit-relative zoom.
    #[must_use]
    pub(crate) fn new(canvas_bounds: Bounds<Pixels>, zoom: f32) -> Self {
        Self::new_with_pan(canvas_bounds, zoom, Point::new(0.0, 0.0))
    }

    /// Creates a centered fit transform with an additional design-space pan.
    #[must_use]
    pub(crate) fn new_with_pan(canvas_bounds: Bounds<Pixels>, zoom: f32, pan: Point) -> Self {
        let width = f32::from(canvas_bounds.size.width).max(0.0);
        let height = f32::from(canvas_bounds.size.height).max(0.0);
        let fitted_side = (width.min(height) - FIT_PADDING * 2.0).max(1.0);
        let scale = (fitted_side / DESIGN_SIZE * sanitize_zoom(zoom)).max(MIN_SCALE);
        let side = scale * DESIGN_SIZE;
        let center = canvas_bounds.center();
        let origin = point(
            px(pan.x.mul_add(scale, f32::from(center.x) - side * 0.5)),
            px(pan.y.mul_add(scale, f32::from(center.y) - side * 0.5)),
        );

        Self {
            canvas: canvas_bounds,
            artboard: bounds(origin, size(px(side), px(side))),
            scale,
        }
    }

    /// Returns the entire GPUI canvas bounds.
    #[must_use]
    pub(crate) const fn canvas_bounds(self) -> Bounds<Pixels> {
        self.canvas
    }

    /// Returns the transformed 200-by-200 artboard bounds.
    #[must_use]
    pub(crate) const fn artboard_bounds(self) -> Bounds<Pixels> {
        self.artboard
    }

    /// Returns logical pixels per KAGE design unit.
    #[must_use]
    pub(crate) const fn scale(self) -> f32 {
        self.scale
    }

    /// Maps one KAGE coordinate to the GPUI window coordinate system.
    #[must_use]
    pub(crate) fn design_to_screen(self, design: Point) -> ScreenPoint<Pixels> {
        point(
            self.artboard.origin.x + px(design.x * self.scale),
            self.artboard.origin.y + px(design.y * self.scale),
        )
    }

    /// Maps a GPUI window coordinate to the unbounded KAGE coordinate system.
    #[must_use]
    pub(crate) fn screen_to_design(self, screen: ScreenPoint<Pixels>) -> Point {
        Point::new(
            f32::from(screen.x - self.artboard.origin.x) / self.scale,
            f32::from(screen.y - self.artboard.origin.y) / self.scale,
        )
    }

    /// Maps and clamps a GPUI coordinate to the editable 0…200 design square.
    #[must_use]
    pub(crate) fn screen_to_design_clamped(self, screen: ScreenPoint<Pixels>) -> Point {
        let point = self.screen_to_design(screen);
        Point::new(
            point.x.clamp(0.0, DESIGN_SIZE),
            point.y.clamp(0.0, DESIGN_SIZE),
        )
    }

    /// Returns whether a screen coordinate falls on the transformed artboard.
    #[must_use]
    pub(crate) fn contains_screen(self, screen: ScreenPoint<Pixels>) -> bool {
        self.artboard.contains(&screen)
    }

    /// Maps a normalized model rectangle into GPUI bounds.
    #[must_use]
    fn design_rect_to_screen(self, rect: Rect) -> Bounds<Pixels> {
        let top_left = self.design_to_screen(rect.min);
        bounds(
            top_left,
            size(
                px(rect.width() * self.scale),
                px(rect.height() * self.scale),
            ),
        )
    }
}

/// Repositions the view so a screen-space anchor keeps pointing at the same
/// KAGE coordinate while the fit-relative zoom changes.
///
/// Rebuilding both transforms from the supplied bounds makes this safe for a
/// burst of gesture events that arrives before GPUI paints another frame.
#[must_use]
pub(crate) fn pan_for_anchored_zoom(
    canvas_bounds: Bounds<Pixels>,
    current_zoom: f32,
    current_pan: Point,
    next_zoom: f32,
    anchor: ScreenPoint<Pixels>,
) -> Point {
    if !current_zoom.is_finite()
        || !next_zoom.is_finite()
        || current_zoom <= 0.0
        || next_zoom <= 0.0
        || (current_zoom - next_zoom).abs() <= f32::EPSILON
    {
        return current_pan;
    }

    let before = CanvasTransform::new_with_pan(canvas_bounds, current_zoom, current_pan)
        .screen_to_design(anchor);
    let after = CanvasTransform::new_with_pan(canvas_bounds, next_zoom, current_pan)
        .screen_to_design(anchor);
    current_pan.offset(Point::new(after.x - before.x, after.y - before.y))
}

/// One of the eight conventional selection-box handles.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ResizeHandle {
    /// Top-left corner.
    NorthWest,
    /// Top edge midpoint.
    North,
    /// Top-right corner.
    NorthEast,
    /// Right edge midpoint.
    East,
    /// Bottom-right corner.
    SouthEast,
    /// Bottom edge midpoint.
    South,
    /// Bottom-left corner.
    SouthWest,
    /// Left edge midpoint.
    West,
}

impl ResizeHandle {
    /// Every handle in clockwise display order.
    const ALL: [Self; 8] = [
        Self::NorthWest,
        Self::North,
        Self::NorthEast,
        Self::East,
        Self::SouthEast,
        Self::South,
        Self::SouthWest,
        Self::West,
    ];

    /// Returns the design-space handle position for a selection rectangle.
    #[must_use]
    pub(crate) fn position(self, rect: Rect) -> Point {
        let center = rect.center();
        match self {
            Self::NorthWest => rect.min,
            Self::North => Point::new(center.x, rect.min.y),
            Self::NorthEast => Point::new(rect.max.x, rect.min.y),
            Self::East => Point::new(rect.max.x, center.y),
            Self::SouthEast => rect.max,
            Self::South => Point::new(center.x, rect.max.y),
            Self::SouthWest => Point::new(rect.min.x, rect.max.y),
            Self::West => Point::new(rect.min.x, center.y),
        }
    }

    /// Returns the stationary handle used as a resize anchor.
    #[must_use]
    pub(crate) const fn opposite(self) -> Self {
        match self {
            Self::NorthWest => Self::SouthEast,
            Self::North => Self::South,
            Self::NorthEast => Self::SouthWest,
            Self::East => Self::West,
            Self::SouthEast => Self::NorthWest,
            Self::South => Self::North,
            Self::SouthWest => Self::NorthEast,
            Self::West => Self::East,
        }
    }
}

/// Semantic hit result in front-to-back pointer priority.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum CanvasHit {
    /// One of the eight selection resize handles.
    ResizeHandle(ResizeHandle),
    /// An editable point on a selected record.
    ControlPoint(ControlPointRef),
    /// The skeleton or frame of a record.
    Stroke(StrokeId),
    /// Empty artboard space, including its mapped coordinate.
    Artboard(Point),
    /// Empty pasteboard space outside the artboard.
    Pasteboard,
}

/// Editing furniture shown for the current selection.
///
/// KAGE Editor exposes the points of one ordinary stroke, but uses a bounding
/// box for multi-selection and for the frame-based type 0, 9, and 99 records.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SelectionControlMode {
    /// Nothing is selected.
    None,
    /// One ordinary path exposes its own control points.
    Points,
    /// A multi-selection or one frame record exposes resize handles.
    Bounds,
}

/// Selects one mutually exclusive set of editing furniture.
fn selection_control_mode(
    strokes: &[Stroke],
    selection: &BTreeSet<StrokeId>,
) -> SelectionControlMode {
    if selection.is_empty() {
        return SelectionControlMode::None;
    }
    if selection.len() > 1 {
        return SelectionControlMode::Bounds;
    }
    strokes
        .iter()
        .find(|stroke| selection.contains(&stroke.id()))
        .map_or(SelectionControlMode::None, |stroke| {
            if stroke.kind().is_path() {
                SelectionControlMode::Points
            } else {
                SelectionControlMode::Bounds
            }
        })
}

/// Returns whether the model's current selection uses bounding-box handles.
///
/// The root view uses the same policy before starting a resize gesture, keeping
/// its input priority identical to the furniture painted by this module.
#[must_use]
pub(crate) fn selection_uses_resize_handles(model: &EditorModel) -> bool {
    selection_control_mode(model.strokes(), model.selection()) == SelectionControlMode::Bounds
        && !selection_has_transformed_geometry(model)
}

/// Returns whether a selected path has a later KAGE type-0 transform.
///
/// Type-0 records operate on already generated polygons rather than the raw
/// skeleton. One path can emit several polygons and a transform frame can
/// affect only some of them, so the source points do not contain enough
/// information to prove that they still describe the visible geometry. Treat
/// every selected path or component preceding such a record conservatively as
/// read-only.
#[must_use]
pub(crate) fn selection_has_transformed_geometry(model: &EditorModel) -> bool {
    selection_has_transformed_geometry_in(model.strokes(), model.selection())
}

/// Shared slice-based form used by live models and frozen paint snapshots.
fn selection_has_transformed_geometry_in(
    strokes: &[Stroke],
    selection: &BTreeSet<StrokeId>,
) -> bool {
    strokes.iter().enumerate().any(|(index, stroke)| {
        selection.contains(&stroke.id())
            && (stroke.kind().is_path() || stroke.kind() == StrokeKind::Component)
            && strokes[index + 1..]
                .iter()
                .any(|later| later.kage_transform().is_some())
    })
}

/// Converts the fixed screen-space control target into design-space units.
#[must_use]
pub(crate) fn control_hit_tolerance(transform: CanvasTransform) -> f32 {
    CONTROL_HIT_RADIUS / transform.scale
}

/// Finds a visible control point without allowing an overlapping unselected
/// record to intercept it.
#[must_use]
pub(crate) fn hit_selected_control_point(
    model: &EditorModel,
    pointer: Point,
    tolerance: f32,
) -> Option<ControlPointRef> {
    if selection_control_mode(model.strokes(), model.selection()) != SelectionControlMode::Points
        || selection_has_transformed_geometry(model)
    {
        return None;
    }
    hit_selected_control_point_in(model.strokes(), model.selection(), pointer, tolerance)
}

/// Shared design-space point hit test for model and frozen-snapshot callers.
fn hit_selected_control_point_in(
    strokes: &[Stroke],
    selection: &BTreeSet<StrokeId>,
    pointer: Point,
    tolerance: f32,
) -> Option<ControlPointRef> {
    let tolerance_squared = tolerance * tolerance;
    strokes.iter().rev().find_map(|stroke| {
        if !selection.contains(&stroke.id()) {
            return None;
        }
        stroke
            .points()
            .iter()
            .enumerate()
            .rev()
            .find(|(_, candidate)| candidate.distance_squared(pointer) <= tolerance_squared)
            .map(|(point, _)| ControlPointRef {
                stroke: stroke.id(),
                point,
            })
    })
}

/// Builds a custom-painted element and reports its current transform at prepaint.
///
/// The callback is the intended place for the root view to retain bounds used by
/// subsequent GPUI mouse handlers.
pub(crate) fn canvas_element(
    snapshot: CanvasSnapshot,
    on_prepaint: impl FnOnce(Bounds<Pixels>, CanvasTransform, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let zoom = snapshot.zoom;
    let pan = snapshot.overlay.pan;
    drawing_surface(
        move |canvas_bounds, window, cx| {
            let transform = CanvasTransform::new_with_pan(canvas_bounds, zoom, pan);
            on_prepaint(canvas_bounds, transform, window, cx);
            transform
        },
        move |canvas_bounds, transform, window, _cx| {
            paint_canvas_with_transform(canvas_bounds, &snapshot, transform, window);
        },
    )
    .size_full()
}

/// Paints a snapshot directly into a GPUI paint phase.
pub(crate) fn paint_canvas(
    canvas_bounds: Bounds<Pixels>,
    snapshot: &CanvasSnapshot,
    window: &mut Window,
) {
    let transform =
        CanvasTransform::new_with_pan(canvas_bounds, snapshot.zoom, snapshot.overlay.pan);
    paint_canvas_with_transform(canvas_bounds, snapshot, transform, window);
}

/// Resolves the highest-priority semantic target at a screen coordinate.
#[must_use]
pub(crate) fn hit_test(
    snapshot: &CanvasSnapshot,
    transform: CanvasTransform,
    screen: ScreenPoint<Pixels>,
) -> CanvasHit {
    if let Some(handle) = hit_resize_handle_screen(snapshot, transform, screen) {
        return CanvasHit::ResizeHandle(handle);
    }
    if let Some(control) = hit_control_point(snapshot, transform, screen) {
        return CanvasHit::ControlPoint(control);
    }
    if transform.canvas_bounds().contains(&screen) {
        let design = transform.screen_to_design(screen);
        let tolerance = STROKE_HIT_RADIUS / transform.scale;
        let skeleton = matches!(snapshot.settings.typeface, Typeface::Skeleton);
        if let Some(stroke) =
            find_record_in_visual_order(&snapshot.strokes, &snapshot.selection, |_, stroke| {
                if skeleton {
                    skeleton_record_hit(stroke, design, tolerance)
                } else {
                    stroke.hit_test(design, tolerance)
                }
            })
        {
            return CanvasHit::Stroke(stroke);
        }
        if transform.contains_screen(screen) {
            CanvasHit::Artboard(design)
        } else {
            CanvasHit::Pasteboard
        }
    } else {
        CanvasHit::Pasteboard
    }
}

/// Finds the topmost record using the actual filled polygons emitted by KAGE.
///
/// This intentionally renders on demand for pointer-down accuracy instead of
/// rebuilding per-record polygons during every hover frame. Metadata records,
/// which do not produce their own polygons, retain their frame/skeleton hit
/// behavior. A missing component definition never turns its entire frame into
/// an invisible click interceptor.
#[must_use]
pub(crate) fn hit_test_rendered(
    model: &EditorModel,
    pointer: Point,
    tolerance: f32,
) -> Option<StrokeId> {
    if matches!(model.settings().typeface, Typeface::Skeleton) {
        return find_record_in_visual_order(model.strokes(), model.selection(), |_, stroke| {
            skeleton_record_hit(stroke, pointer, tolerance)
                || (stroke.kind() == StrokeKind::Component
                    && expanded_component_centerlines(model, stroke.id())
                        .iter()
                        .any(|path| skeleton_record_hit(path, pointer, tolerance)))
        });
    }

    let record_outlines = render_record_outlines(model);
    find_record_in_visual_order(model.strokes(), model.selection(), |index, stroke| {
        let outlines = &record_outlines[index];
        if outlines.is_empty() {
            return matches!(stroke.kind(), StrokeKind::Metadata | StrokeKind::Transform)
                && stroke.hit_test(pointer, tolerance);
        }
        outlines
            .iter()
            .any(|outline| polygon_contains_or_near(outline, pointer, tolerance))
    })
}

/// Returns records whose actual filled polygons intersect a marquee rectangle.
///
/// Non-rendering metadata keeps control-frame intersection semantics so an
/// existing type-0/type-9 record remains selectable and editable.
#[must_use]
pub(crate) fn records_intersecting_rendered(model: &EditorModel, rect: Rect) -> Vec<StrokeId> {
    if matches!(model.settings().typeface, Typeface::Skeleton) {
        return model
            .strokes()
            .iter()
            .filter(|stroke| {
                skeleton_record_intersects_rect(stroke, rect)
                    || (stroke.kind() == StrokeKind::Component
                        && expanded_component_centerlines(model, stroke.id())
                            .iter()
                            .any(|path| skeleton_record_intersects_rect(path, rect)))
            })
            .map(Stroke::id)
            .collect();
    }

    let record_outlines = render_record_outlines(model);
    model
        .strokes()
        .iter()
        .zip(record_outlines.iter())
        .filter_map(|(stroke, outlines)| {
            let intersects = if outlines.is_empty() {
                matches!(stroke.kind(), StrokeKind::Metadata | StrokeKind::Transform)
                    && stroke
                        .bounds()
                        .is_some_and(|bounds| bounds.intersects(rect))
            } else {
                outlines
                    .iter()
                    .any(|outline| polygon_intersects_rect(outline, rect))
            };
            intersects.then_some(stroke.id())
        })
        .collect()
}

/// Visits selected records above unselected records, matching editable paint.
fn find_record_in_visual_order(
    strokes: &[Stroke],
    selection: &BTreeSet<StrokeId>,
    mut hit: impl FnMut(usize, &Stroke) -> bool,
) -> Option<StrokeId> {
    for selected in [true, false] {
        for (index, stroke) in strokes.iter().enumerate().rev() {
            if selection.contains(&stroke.id()) == selected && hit(index, stroke) {
                return Some(stroke.id());
            }
        }
    }
    None
}

/// Renders the complete glyph and partitions its final polygons by source record.
///
/// KAGE type-0 operations mutate polygons emitted by earlier records. Rendering
/// records independently therefore leaves hit geometry at its pre-transform
/// position even though the canvas uses the transformed aggregate. The engine
/// retains polygon order while applying those operations, so each record's
/// standalone polygon count provides stable ownership boundaries for the final
/// aggregate output. Type-9 and type-0 records contribute zero polygons and do
/// not disturb those boundaries.
fn render_record_outlines(model: &EditorModel) -> Vec<Vec<engine::Outline>> {
    let counts = model
        .strokes()
        .iter()
        .map(|stroke| {
            let source = super::model::serialize_kage(std::slice::from_ref(stroke));
            render_source_outlines(model, &source).len()
        })
        .collect::<Vec<_>>();
    let outlines = render_source_outlines(model, &model.to_kage());

    if counts.iter().sum::<usize>() == outlines.len() {
        let mut outlines = outlines.into_iter();
        return counts
            .into_iter()
            .map(|count| outlines.by_ref().take(count).collect())
            .collect();
    }

    // A future engine adjustment may change a record's polygon count only in
    // aggregate context. Preserve transformed hit positions in that case by
    // rendering each record with every later non-rendering operation appended.
    model
        .strokes()
        .iter()
        .enumerate()
        .map(|(index, stroke)| {
            let mut records = vec![stroke.clone()];
            records.extend(
                model.strokes()[index + 1..]
                    .iter()
                    .filter(|candidate| {
                        matches!(
                            candidate.kind(),
                            StrokeKind::Metadata | StrokeKind::Transform
                        )
                    })
                    .cloned(),
            );
            render_source_outlines(model, &super::model::serialize_kage(&records))
        })
        .collect()
}

/// Recursively expands available components for `Always`/skeleton centerlines.
fn expanded_centerline_strokes(model: &EditorModel) -> Vec<Stroke> {
    let mut paths = Vec::new();
    for stroke in model.strokes() {
        if stroke.kind().is_path() {
            paths.push(stroke.clone());
        } else if stroke.kind() == StrokeKind::Component {
            paths.extend(expanded_component_centerlines(model, stroke.id()));
        }
    }
    paths
}

/// Recursively expands one component while retaining ownership for hit tests.
fn expanded_component_centerlines(model: &EditorModel, root: StrokeId) -> Vec<Stroke> {
    let mut expanded = model.clone();
    let mut pending = vec![root];
    let mut paths = Vec::new();
    for _ in 0..256 {
        let Some(component) = pending.pop() else {
            break;
        };
        let Ok(children) = expanded.decompose_component(component) else {
            continue;
        };
        for child in children {
            let Some(stroke) = expanded.stroke(child).cloned() else {
                continue;
            };
            if stroke.kind() == StrokeKind::Component {
                pending.push(child);
            } else if stroke.kind().is_path() {
                paths.push(stroke);
            }
        }
    }
    paths
}

/// Renders source while supplying every currently registered component.
fn render_source_outlines(model: &EditorModel, source: &str) -> Vec<engine::Outline> {
    let gothic = matches!(model.settings().typeface, Typeface::Gothic);
    engine::render_outlines(
        source,
        model.component_library(),
        gothic,
        model.settings().use_curve,
    )
}

/// Tests filled-polygon containment plus a small edge tolerance.
fn polygon_contains_or_near(outline: &engine::Outline, pointer: Point, tolerance: f32) -> bool {
    if outline.len() < 3 {
        return false;
    }
    let points = outline
        .iter()
        .map(|&(x, y)| Point::new(x, y))
        .collect::<Vec<_>>();
    point_in_polygon(pointer, &points)
        || points
            .iter()
            .copied()
            .zip(points.iter().copied().cycle().skip(1))
            .take(points.len())
            .any(|(start, end)| distance_to_segment(pointer, start, end) <= tolerance)
}

/// Tests a filled outline against an axis-aligned marquee.
fn polygon_intersects_rect(outline: &engine::Outline, rect: Rect) -> bool {
    if outline.len() < 3 {
        return false;
    }
    let points = outline
        .iter()
        .map(|&(x, y)| Point::new(x, y))
        .collect::<Vec<_>>();
    if points.iter().copied().any(|point| rect.contains(point)) {
        return true;
    }
    let corners = [
        rect.min,
        Point::new(rect.max.x, rect.min.y),
        rect.max,
        Point::new(rect.min.x, rect.max.y),
    ];
    if corners
        .iter()
        .copied()
        .any(|corner| point_in_polygon(corner, &points))
    {
        return true;
    }
    points
        .iter()
        .copied()
        .zip(points.iter().copied().cycle().skip(1))
        .take(points.len())
        .any(|(start, end)| {
            corners
                .iter()
                .copied()
                .zip(corners.iter().copied().cycle().skip(1))
                .take(corners.len())
                .any(|(edge_start, edge_end)| segments_intersect(start, end, edge_start, edge_end))
        })
}

/// Tests only geometry that is actually visible in skeleton mode.
fn skeleton_record_hit(stroke: &Stroke, pointer: Point, tolerance: f32) -> bool {
    if stroke.kind().is_path() {
        return polyline_distance(&stroke.sampled_path(64), pointer) <= tolerance;
    }
    if stroke.kind() == StrokeKind::Component {
        return component_frame_points(stroke)
            .is_some_and(|frame| polyline_distance(&frame, pointer) <= tolerance);
    }
    false
}

/// Tests skeleton paths and component frame edges against a marquee.
fn skeleton_record_intersects_rect(stroke: &Stroke, rect: Rect) -> bool {
    if stroke.kind().is_path() {
        return polyline_intersects_rect(&stroke.sampled_path(64), rect);
    }
    if stroke.kind() == StrokeKind::Component {
        return component_frame_points(stroke)
            .is_some_and(|frame| polyline_intersects_rect(&frame, rect));
    }
    false
}

/// Returns the closed, normalized frame painted for a component skeleton.
fn component_frame_points(stroke: &Stroke) -> Option<[Point; 5]> {
    let bounds = stroke.bounds()?;
    Some([
        bounds.min,
        Point::new(bounds.max.x, bounds.min.y),
        bounds.max,
        Point::new(bounds.min.x, bounds.max.y),
        bounds.min,
    ])
}

/// Returns the shortest distance to any segment of a sampled polyline.
fn polyline_distance(points: &[Point], pointer: Point) -> f32 {
    points
        .windows(2)
        .map(|segment| distance_to_segment(pointer, segment[0], segment[1]))
        .fold(f32::INFINITY, f32::min)
}

/// Tests whether visible polyline geometry crosses or lies inside a marquee.
fn polyline_intersects_rect(points: &[Point], rect: Rect) -> bool {
    if points.iter().copied().any(|point| rect.contains(point)) {
        return true;
    }
    let corners = [
        rect.min,
        Point::new(rect.max.x, rect.min.y),
        rect.max,
        Point::new(rect.min.x, rect.max.y),
    ];
    points.windows(2).any(|segment| {
        corners
            .iter()
            .copied()
            .zip(corners.iter().copied().cycle().skip(1))
            .take(corners.len())
            .any(|(start, end)| segments_intersect(segment[0], segment[1], start, end))
    })
}

/// Applies an even-odd ray crossing test to one polygon.
fn point_in_polygon(point: Point, polygon: &[Point]) -> bool {
    let mut inside = false;
    let Some(mut previous) = polygon.last().copied() else {
        return false;
    };
    for current in polygon.iter().copied() {
        let crosses = (current.y > point.y) != (previous.y > point.y)
            && point.x
                < (previous.x - current.x) * (point.y - current.y) / (previous.y - current.y)
                    + current.x;
        if crosses {
            inside = !inside;
        }
        previous = current;
    }
    inside
}

/// Returns the distance from a point to a finite line segment.
fn distance_to_segment(point: Point, start: Point, end: Point) -> f32 {
    let delta = Point::new(end.x - start.x, end.y - start.y);
    let length_squared = delta.x.mul_add(delta.x, delta.y * delta.y);
    if length_squared <= f32::EPSILON {
        return point.distance(start);
    }
    let offset = Point::new(point.x - start.x, point.y - start.y);
    let projection =
        (offset.x.mul_add(delta.x, offset.y * delta.y) / length_squared).clamp(0.0, 1.0);
    point.distance(Point::new(
        delta.x.mul_add(projection, start.x),
        delta.y.mul_add(projection, start.y),
    ))
}

/// Tests two closed segments, including collinear edge contact.
fn segments_intersect(first: Point, second: Point, third: Point, fourth: Point) -> bool {
    let first_side = cross(
        Point::new(second.x - first.x, second.y - first.y),
        Point::new(third.x - first.x, third.y - first.y),
    );
    let second_side = cross(
        Point::new(second.x - first.x, second.y - first.y),
        Point::new(fourth.x - first.x, fourth.y - first.y),
    );
    let third_side = cross(
        Point::new(fourth.x - third.x, fourth.y - third.y),
        Point::new(first.x - third.x, first.y - third.y),
    );
    let fourth_side = cross(
        Point::new(fourth.x - third.x, fourth.y - third.y),
        Point::new(second.x - third.x, second.y - third.y),
    );
    if first_side.abs() <= f32::EPSILON && point_on_segment(third, first, second)
        || second_side.abs() <= f32::EPSILON && point_on_segment(fourth, first, second)
        || third_side.abs() <= f32::EPSILON && point_on_segment(first, third, fourth)
        || fourth_side.abs() <= f32::EPSILON && point_on_segment(second, third, fourth)
    {
        return true;
    }
    first_side.signum() != second_side.signum() && third_side.signum() != fourth_side.signum()
}

/// Tests a collinear point against a segment's axis-aligned extent.
fn point_on_segment(point: Point, start: Point, end: Point) -> bool {
    point.x >= start.x.min(end.x) - f32::EPSILON
        && point.x <= start.x.max(end.x) + f32::EPSILON
        && point.y >= start.y.min(end.y) - f32::EPSILON
        && point.y <= start.y.max(end.y) + f32::EPSILON
}

/// Finds a selected control point at a screen coordinate.
#[must_use]
pub(crate) fn hit_control_point(
    snapshot: &CanvasSnapshot,
    transform: CanvasTransform,
    screen: ScreenPoint<Pixels>,
) -> Option<ControlPointRef> {
    if selection_control_mode(&snapshot.strokes, &snapshot.selection)
        != SelectionControlMode::Points
        || selection_has_transformed_geometry_in(&snapshot.strokes, &snapshot.selection)
    {
        return None;
    }
    let design = transform.screen_to_design(screen);
    hit_selected_control_point_in(
        &snapshot.strokes,
        &snapshot.selection,
        design,
        control_hit_tolerance(transform),
    )
}

/// Finds one of the eight selection resize handles at a screen coordinate.
#[must_use]
fn hit_resize_handle_screen(
    snapshot: &CanvasSnapshot,
    transform: CanvasTransform,
    screen: ScreenPoint<Pixels>,
) -> Option<ResizeHandle> {
    if selection_control_mode(&snapshot.strokes, &snapshot.selection)
        != SelectionControlMode::Bounds
        || selection_has_transformed_geometry_in(&snapshot.strokes, &snapshot.selection)
    {
        return None;
    }
    let bounds = snapshot.selection_bounds?;
    Some(hit_resize_handle(
        bounds,
        transform.screen_to_design(screen),
        transform,
    )?)
}

/// Finds a selection resize handle using design-space pointer coordinates.
///
/// The transform is used only to keep the hit radius constant on screen.
#[must_use]
pub(crate) fn hit_resize_handle(
    selection_bounds: Rect,
    pointer: Point,
    transform: CanvasTransform,
) -> Option<ResizeHandle> {
    let radius_squared = control_hit_tolerance(transform).powi(2);
    ResizeHandle::ALL.into_iter().find(|handle| {
        handle.position(selection_bounds).distance_squared(pointer) <= radius_squared
    })
}

/// Paints all layers using a transform shared with input handling.
fn paint_canvas_with_transform(
    canvas_bounds: Bounds<Pixels>,
    snapshot: &CanvasSnapshot,
    transform: CanvasTransform,
    window: &mut Window,
) {
    paint_pasteboard(canvas_bounds, transform.artboard, window);

    // Keep records parked on the pasteboard visible without changing the
    // paper's conventional dark ink. The artboard pass below covers this
    // muted preview and repaints the same polygons with paper-appropriate ink.
    if snapshot.overlay.mode != InteractionMode::FinalGlyph {
        paint_editable_outlines(
            transform,
            snapshot,
            rgba(0xD5D6_DC66),
            rgba(0xFF45_3AB8),
            window,
        );
    }

    window.with_content_mask(
        Some(ContentMask {
            bounds: transform.artboard,
        }),
        |window| {
            paint_artboard(transform, snapshot, window);
            if snapshot.overlay.mode == InteractionMode::FinalGlyph {
                paint_engine_outlines(transform, snapshot, rgba(0x1718_1AFF), window);
            } else {
                paint_editable_outlines(
                    transform,
                    snapshot,
                    rgba(0x1718_1AFF),
                    rgba(0xCC00_00FF),
                    window,
                );
                if snapshot.overlay.mask_mode != MaskMode::None {
                    paint_negative_mask(transform, snapshot.overlay.mask_mode, window);
                    paint_masked_engine_outlines(transform, snapshot, window);
                    paint_selected_outlines(transform, snapshot, rgba(0xCC00_00FF), window);
                }
            }
        },
    );

    if snapshot.overlay.mode != InteractionMode::FinalGlyph {
        paint_centerlines(transform, snapshot, window);
        paint_selection(transform, snapshot, window);
        paint_gesture_overlays(transform, snapshot, window);
    }

    paint_artboard_edge(transform.artboard, window);
}

/// Paints the neutral editor pasteboard and subtle artboard elevation.
fn paint_pasteboard(canvas_bounds: Bounds<Pixels>, artboard: Bounds<Pixels>, window: &mut Window) {
    window.paint_quad(fill(canvas_bounds, rgba(0x191A_1DFF)));
    window.paint_shadows(
        artboard,
        0.0.into(),
        &[
            BoxShadow {
                color: rgba(0x0000_0045).into(),
                offset: point(px(0.0), px(8.0)),
                blur_radius: px(22.0),
                spread_radius: px(1.0),
            },
            BoxShadow {
                color: rgba(0x0000_0024).into(),
                offset: point(px(0.0), px(2.0)),
                blur_radius: px(5.0),
                spread_radius: px(0.0),
            },
        ],
    );
}

/// Paints paper, grid, em guides, and center axes.
fn paint_artboard(transform: CanvasTransform, snapshot: &CanvasSnapshot, window: &mut Window) {
    window.paint_quad(fill(transform.artboard, rgba(0xF3F2_EFFF)));

    if snapshot.overlay.mode != InteractionMode::FinalGlyph {
        if snapshot.settings.grid.visible {
            paint_grid(transform, snapshot.settings.grid, window);
        }
        paint_em_guides(transform, window);
        paint_axes(transform, window);
    }
}

/// Paints the configurable major/minor design grid as crisp pixel quads.
fn paint_grid(transform: CanvasTransform, grid: super::model::GridSettings, window: &mut Window) {
    if !grid.origin_x.is_finite()
        || !grid.origin_y.is_finite()
        || !grid.spacing_x.is_finite()
        || grid.spacing_x <= f32::EPSILON
        || !grid.spacing_y.is_finite()
        || grid.spacing_y <= f32::EPSILON
    {
        return;
    }
    let subdivisions = i32::from(grid.subdivisions.max(1));
    paint_grid_axis(
        transform,
        grid.origin_x,
        grid.spacing_x,
        subdivisions,
        true,
        window,
    );
    paint_grid_axis(
        transform,
        grid.origin_y,
        grid.spacing_y,
        subdivisions,
        false,
        window,
    );
}

/// Paints one independently configured axis of the design grid.
fn paint_grid_axis(
    transform: CanvasTransform,
    origin: f32,
    spacing: f32,
    subdivisions: i32,
    vertical: bool,
    window: &mut Window,
) {
    let step = spacing / subdivisions as f32;
    if !step.is_finite() || step <= f32::EPSILON {
        return;
    }
    for index in -MAX_GRID_LINES..=MAX_GRID_LINES {
        let coordinate = (index as f32).mul_add(step, origin);
        if coordinate <= 0.0 || coordinate >= DESIGN_SIZE {
            continue;
        }
        let major = index % subdivisions == 0;
        let color = if major {
            rgba(0x3847_5720)
        } else {
            rgba(0x3847_570F)
        };
        paint_axis_line(transform, coordinate, vertical, color, window);
    }
}

/// Paints one full-width vertical or horizontal one-pixel line.
fn paint_axis_line(
    transform: CanvasTransform,
    coordinate: f32,
    vertical: bool,
    color: Rgba,
    window: &mut Window,
) {
    let screen = transform.design_to_screen(Point::new(coordinate, coordinate));
    let artboard = transform.artboard;
    let line_bounds = if vertical {
        bounds(
            point(crisp(screen.x), artboard.origin.y),
            size(px(1.0), artboard.size.height),
        )
    } else {
        bounds(
            point(artboard.origin.x, crisp(screen.y)),
            size(artboard.size.width, px(1.0)),
        )
    };
    window.paint_quad(fill(line_bounds, color));
}

/// Paints the 12/188 KAGE em guides.
fn paint_em_guides(transform: CanvasTransform, window: &mut Window) {
    for coordinate in [EM_GUIDE_INSET, DESIGN_SIZE - EM_GUIDE_INSET] {
        paint_dashed_axis(transform, coordinate, true, rgba(0x0A84_FF52), window);
        paint_dashed_axis(transform, coordinate, false, rgba(0x0A84_FF52), window);
    }
}

/// Paints a dashed vertical or horizontal guide.
fn paint_dashed_axis(
    transform: CanvasTransform,
    coordinate: f32,
    vertical: bool,
    color: Rgba,
    window: &mut Window,
) {
    let mut builder = PathBuilder::stroke(px(1.0)).dash_array(&[px(4.0), px(4.0)]);
    let start = if vertical {
        Point::new(coordinate, 0.0)
    } else {
        Point::new(0.0, coordinate)
    };
    let end = if vertical {
        Point::new(coordinate, DESIGN_SIZE)
    } else {
        Point::new(DESIGN_SIZE, coordinate)
    };
    builder.move_to(crisp_point(transform.design_to_screen(start)));
    builder.line_to(crisp_point(transform.design_to_screen(end)));
    paint_built_path(builder, color, window);
}

/// Paints the 100-unit center axes slightly stronger than the grid.
fn paint_axes(transform: CanvasTransform, window: &mut Window) {
    paint_axis_line(
        transform,
        DESIGN_SIZE * 0.5,
        true,
        rgba(0x283B_503D),
        window,
    );
    paint_axis_line(
        transform,
        DESIGN_SIZE * 0.5,
        false,
        rgba(0x283B_503D),
        window,
    );
}

/// Paints the selected dark knockout shape over the ordinary black glyph.
fn paint_negative_mask(transform: CanvasTransform, mask: MaskMode, window: &mut Window) {
    let shape = mask_polygon(mask);
    if !shape.is_empty() {
        paint_design_polygon(transform, &shape, rgba(0x1617_19D9), window);
    }
}

/// Paints the filled KAGE polygons with one caller-selected ink color.
fn paint_engine_outlines(
    transform: CanvasTransform,
    snapshot: &CanvasSnapshot,
    color: Rgba,
    window: &mut Window,
) {
    for outline in &snapshot.outlines {
        let points = outline
            .iter()
            .map(|&(x, y)| Point::new(x, y))
            .collect::<Vec<_>>();
        paint_design_polygon(transform, &points, color, window);
    }
}

/// Paints unselected records first and selected records last.
///
/// The ordering matches KAGE Editor's direct-manipulation contract: selected
/// filled strokes remain unmistakable even where they overlap another stroke.
fn paint_editable_outlines(
    transform: CanvasTransform,
    snapshot: &CanvasSnapshot,
    ordinary_color: Rgba,
    selected_color: Rgba,
    window: &mut Window,
) {
    paint_record_outlines(transform, snapshot, false, ordinary_color, window);
    paint_record_outlines(transform, snapshot, true, selected_color, window);
}

/// Paints the engine polygons owned by records with one selection state.
fn paint_record_outlines(
    transform: CanvasTransform,
    snapshot: &CanvasSnapshot,
    selected: bool,
    color: Rgba,
    window: &mut Window,
) {
    for outlines in record_outlines_for_selection(snapshot, selected) {
        for outline in outlines {
            let points = outline
                .iter()
                .map(|&(x, y)| Point::new(x, y))
                .collect::<Vec<_>>();
            paint_design_polygon(transform, &points, color, window);
        }
    }
}

/// Iterates final engine outlines owned by selected or unselected records.
fn record_outlines_for_selection(
    snapshot: &CanvasSnapshot,
    selected: bool,
) -> impl Iterator<Item = &[engine::Outline]> {
    snapshot
        .strokes
        .iter()
        .zip(&snapshot.record_outlines)
        .filter_map(move |(stroke, outlines)| {
            (snapshot.selection.contains(&stroke.id()) == selected).then_some(outlines.as_slice())
        })
}

/// Repaints selected records after a knockout so selection stays red.
fn paint_selected_outlines(
    transform: CanvasTransform,
    snapshot: &CanvasSnapshot,
    color: Rgba,
    window: &mut Window,
) {
    paint_record_outlines(transform, snapshot, true, color, window);
}

/// Repaints only the portions of engine polygons covered by a knockout shape.
fn paint_masked_engine_outlines(
    transform: CanvasTransform,
    snapshot: &CanvasSnapshot,
    window: &mut Window,
) {
    let clip = mask_polygon(snapshot.overlay.mask_mode);
    if clip.len() < 3 {
        return;
    }
    for outlines in record_outlines_for_selection(snapshot, false) {
        for outline in outlines {
            let subject = outline
                .iter()
                .map(|&(x, y)| Point::new(x, y))
                .collect::<Vec<_>>();
            let clipped = clip_polygon_to_convex(&subject, &clip);
            paint_design_polygon(transform, &clipped, rgba(0xF3F2_EFFF), window);
        }
    }
}

/// Builds an original convex approximation of one supported knockout shape.
fn mask_polygon(mask: MaskMode) -> Vec<Point> {
    let center = DESIGN_SIZE * 0.5;
    match mask {
        MaskMode::None => Vec::new(),
        MaskMode::Square => {
            let (min, max) = (15.0, 185.0);
            vec![
                Point::new(min, min),
                Point::new(max, min),
                Point::new(max, max),
                Point::new(min, max),
            ]
        }
        MaskMode::Diamond => {
            let (min, max) = (10.0, 190.0);
            vec![
                Point::new(center, min),
                Point::new(max, center),
                Point::new(center, max),
                Point::new(min, center),
            ]
        }
        MaskMode::Circle => regular_polygon(Point::new(center, center), 88.0, 64),
        MaskMode::RoundedSquare => {
            let (min, max) = (15.0, 185.0);
            let radius = 10.0;
            let centers = [
                (
                    Point::new(max - radius, min + radius),
                    -std::f32::consts::FRAC_PI_2,
                ),
                (Point::new(max - radius, max - radius), 0.0),
                (
                    Point::new(min + radius, max - radius),
                    std::f32::consts::FRAC_PI_2,
                ),
                (Point::new(min + radius, min + radius), std::f32::consts::PI),
            ];
            let mut points = Vec::with_capacity(36);
            for (corner, start_angle) in centers {
                for step in 0..=8 {
                    let angle = start_angle + std::f32::consts::FRAC_PI_2 * step as f32 / 8.0;
                    points.push(Point::new(
                        corner.x + angle.cos() * radius,
                        corner.y + angle.sin() * radius,
                    ));
                }
            }
            points
        }
    }
}

/// Samples a clockwise screen-coordinate polygon around a center point.
fn regular_polygon(center: Point, radius: f32, segments: usize) -> Vec<Point> {
    (0..segments)
        .map(|step| {
            let angle = -std::f32::consts::FRAC_PI_2
                + std::f32::consts::TAU * step as f32 / segments as f32;
            Point::new(
                center.x + angle.cos() * radius,
                center.y + angle.sin() * radius,
            )
        })
        .collect()
}

/// Clips an arbitrary subject polygon against one convex clip polygon.
fn clip_polygon_to_convex(subject: &[Point], clip: &[Point]) -> Vec<Point> {
    if subject.len() < 3 || clip.len() < 3 {
        return Vec::new();
    }
    let orientation = signed_area_twice(clip).signum();
    if orientation == 0.0 {
        return Vec::new();
    }

    let mut output = subject.to_vec();
    for edge_index in 0..clip.len() {
        let edge_start = clip[edge_index];
        let edge_end = clip[(edge_index + 1) % clip.len()];
        let input = std::mem::take(&mut output);
        let Some(mut previous) = input.last().copied() else {
            break;
        };
        let mut previous_inside = is_inside_clip_edge(previous, edge_start, edge_end, orientation);
        for current in input {
            let current_inside = is_inside_clip_edge(current, edge_start, edge_end, orientation);
            if current_inside != previous_inside
                && let Some(intersection) =
                    segment_edge_intersection(previous, current, edge_start, edge_end)
            {
                output.push(intersection);
            }
            if current_inside {
                output.push(current);
            }
            previous = current;
            previous_inside = current_inside;
        }
    }
    output
}

/// Returns twice a polygon's signed area in design coordinates.
fn signed_area_twice(points: &[Point]) -> f32 {
    points
        .iter()
        .copied()
        .zip(points.iter().copied().cycle().skip(1))
        .take(points.len())
        .map(|(first, second)| first.x * second.y - second.x * first.y)
        .sum()
}

/// Tests one point against the consistently oriented side of a clip edge.
fn is_inside_clip_edge(point: Point, start: Point, end: Point, orientation: f32) -> bool {
    cross(
        Point::new(end.x - start.x, end.y - start.y),
        Point::new(point.x - start.x, point.y - start.y),
    ) * orientation
        >= -f32::EPSILON
}

/// Finds the intersection of a subject segment and an infinite clip edge.
fn segment_edge_intersection(
    start: Point,
    end: Point,
    edge_start: Point,
    edge_end: Point,
) -> Option<Point> {
    let segment = Point::new(end.x - start.x, end.y - start.y);
    let edge = Point::new(edge_end.x - edge_start.x, edge_end.y - edge_start.y);
    let denominator = cross(segment, edge);
    if denominator.abs() <= f32::EPSILON {
        return None;
    }
    let offset = Point::new(edge_start.x - start.x, edge_start.y - start.y);
    let position = cross(offset, edge) / denominator;
    Some(Point::new(
        start.x + segment.x * position,
        start.y + segment.y * position,
    ))
}

/// Returns the two-dimensional scalar cross product.
const fn cross(first: Point, second: Point) -> f32 {
    first.x * second.y - first.y * second.x
}

/// Paints centerlines according to the configured visibility policy.
fn paint_centerlines(transform: CanvasTransform, snapshot: &CanvasSnapshot, window: &mut Window) {
    let skeleton = matches!(snapshot.settings.typeface, Typeface::Skeleton);
    let strokes = if skeleton || matches!(snapshot.settings.centerline, CenterlineMode::Always) {
        &snapshot.centerline_strokes
    } else {
        &snapshot.strokes
    };
    for stroke in strokes {
        let selected = snapshot.selection.contains(&stroke.id());
        let visible = skeleton
            || match snapshot.settings.centerline {
                CenterlineMode::None => false,
                CenterlineMode::Selection => snapshot.selection.len() == 1 && selected,
                CenterlineMode::Always => true,
            };
        if !visible {
            continue;
        }

        let hovered = snapshot.overlay.hovered_stroke == Some(stroke.id());
        let color = if selected {
            rgba(0x0A84_FFD9)
        } else if hovered {
            rgba(0x0A84_FF99)
        } else {
            rgba(0x6677_8966)
        };
        let width = if selected { 1.35 } else { 1.0 };
        if !stroke.kind().is_path() {
            continue;
        }
        paint_design_polyline(
            transform,
            &stroke.sampled_path(curve_steps(transform.scale)),
            width,
            color,
            window,
        );
    }
    if skeleton {
        for component in snapshot
            .strokes
            .iter()
            .filter(|stroke| stroke.kind() == StrokeKind::Component)
        {
            if let Some(frame) = component_frame_points(component) {
                let selected = snapshot.selection.contains(&component.id());
                paint_design_polyline(
                    transform,
                    &frame,
                    if selected { 1.35 } else { 1.0 },
                    if selected {
                        rgba(0x0A84_FFD9)
                    } else {
                        rgba(0x6677_8966)
                    },
                    window,
                );
            }
        }
    }
}

/// Paints control polygons, point states, selection bounds, and resize handles.
fn paint_selection(transform: CanvasTransform, snapshot: &CanvasSnapshot, window: &mut Window) {
    match selection_control_mode(&snapshot.strokes, &snapshot.selection) {
        SelectionControlMode::None => {}
        SelectionControlMode::Points => {
            let Some(stroke) = snapshot
                .strokes
                .iter()
                .find(|stroke| snapshot.selection.contains(&stroke.id()))
            else {
                return;
            };
            if selection_has_transformed_geometry_in(&snapshot.strokes, &snapshot.selection) {
                if let Some(bounds) = rendered_record_bounds(snapshot, stroke.id()) {
                    paint_read_only_bounds(transform, bounds, window);
                }
                return;
            }
            paint_control_polygon(transform, stroke, window);
            for (point_index, &design_point) in stroke.points().iter().enumerate() {
                let reference = ControlPointRef {
                    stroke: stroke.id(),
                    point: point_index,
                };
                let state = control_state(snapshot, reference);
                paint_control_point(
                    transform,
                    design_point,
                    state,
                    is_off_curve_control(stroke, point_index),
                    snapshot.overlay.hovered_control == Some(reference),
                    snapshot.overlay.active_control == Some(reference),
                    window,
                );
            }
        }
        SelectionControlMode::Bounds => {
            if selection_has_transformed_geometry_in(&snapshot.strokes, &snapshot.selection) {
                if let Some(bounds) = rendered_selection_bounds(snapshot) {
                    paint_read_only_bounds(transform, bounds, window);
                }
            } else if let Some(selection_bounds) = snapshot.selection_bounds {
                paint_selection_bounds(transform, selection_bounds, window);
            }
        }
    }
}

/// Returns the final engine-outline bounds owned by one source record.
fn rendered_record_bounds(snapshot: &CanvasSnapshot, id: StrokeId) -> Option<Rect> {
    let index = snapshot
        .strokes
        .iter()
        .position(|stroke| stroke.id() == id)?;
    let points = snapshot
        .record_outlines
        .get(index)?
        .iter()
        .flatten()
        .map(|&(x, y)| Point::new(x, y))
        .collect::<Vec<_>>();
    Rect::from_points(&points)
}

/// Returns aggregate final engine bounds for every selected source record.
fn rendered_selection_bounds(snapshot: &CanvasSnapshot) -> Option<Rect> {
    let points = snapshot
        .strokes
        .iter()
        .zip(&snapshot.record_outlines)
        .filter(|(stroke, _)| snapshot.selection.contains(&stroke.id()))
        .flat_map(|(_, outlines)| outlines.iter().flatten())
        .map(|&(x, y)| Point::new(x, y))
        .collect::<Vec<_>>();
    Rect::from_points(&points)
}

/// Identifies KAGE points that steer a curve without lying on that curve.
fn is_off_curve_control(stroke: &Stroke, point: usize) -> bool {
    matches!(
        (stroke.kind(), stroke.points().len(), point),
        (StrokeKind::Curve, 3, 1) | (StrokeKind::Bezier, 4, 1 | 2) | (StrokeKind::Sweep, 4, 2)
    )
}

/// Paints straight control-polygon edges for the selected record.
fn paint_control_polygon(transform: CanvasTransform, stroke: &Stroke, window: &mut Window) {
    if stroke.points().len() < 2 {
        return;
    }
    let color = match stroke.kind() {
        StrokeKind::Curve | StrokeKind::Bezier | StrokeKind::Sweep => rgba(0x13A8_A85C),
        _ => rgba(0x0A84_FF42),
    };
    let mut builder = PathBuilder::stroke(px(1.0)).dash_array(&[px(3.0), px(3.0)]);
    for (index, &design_point) in stroke.points().iter().enumerate() {
        let screen = crisp_point(transform.design_to_screen(design_point));
        if index == 0 {
            builder.move_to(screen);
        } else {
            builder.line_to(screen);
        }
    }
    paint_built_path(builder, color, window);
}

/// Visual state of one selected control point.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ControlState {
    /// Ordinary unconnected point.
    Normal,
    /// Endpoint resting on another record's horizontal or vertical segment.
    Online,
    /// Endpoint coincident with another record's endpoint.
    Matched,
}

/// Determines visual priority and connection state for one control point.
fn control_state(snapshot: &CanvasSnapshot, reference: ControlPointRef) -> ControlState {
    if matches_other_endpoint(snapshot, reference) {
        return ControlState::Matched;
    }
    if lies_on_other_axis_segment(snapshot, reference) {
        return ControlState::Online;
    }
    ControlState::Normal
}

/// Returns the current endpoint, rejecting interior controls and frames.
fn endpoint_anchor(snapshot: &CanvasSnapshot, reference: ControlPointRef) -> Option<Point> {
    let stroke = snapshot
        .strokes
        .iter()
        .find(|stroke| stroke.id() == reference.stroke)?;
    if !stroke.kind().is_path()
        || (reference.point != 0 && reference.point + 1 != stroke.points().len())
    {
        return None;
    }
    stroke.points().get(reference.point).copied()
}

/// Returns whether an endpoint coincides with another path's endpoint.
fn matches_other_endpoint(snapshot: &CanvasSnapshot, reference: ControlPointRef) -> bool {
    let Some(anchor) = endpoint_anchor(snapshot, reference) else {
        return false;
    };
    let tolerance_squared = CONNECTION_TOLERANCE * CONNECTION_TOLERANCE;
    snapshot
        .strokes
        .iter()
        .filter(|stroke| stroke.id() != reference.stroke && stroke.kind().is_path())
        .any(|stroke| {
            [stroke.points().first(), stroke.points().last()]
                .into_iter()
                .flatten()
                .any(|point| point.distance_squared(anchor) <= tolerance_squared)
        })
}

/// Returns whether an endpoint rests on another straight axis-aligned segment.
fn lies_on_other_axis_segment(snapshot: &CanvasSnapshot, reference: ControlPointRef) -> bool {
    let Some(anchor) = endpoint_anchor(snapshot, reference) else {
        return false;
    };
    snapshot
        .strokes
        .iter()
        .filter(|stroke| stroke.id() != reference.stroke)
        .flat_map(straight_connection_segments)
        .any(|(start, end)| point_on_axis_segment(anchor, start, end))
}

/// Returns straight segments eligible for KAGE endpoint connection feedback.
fn straight_connection_segments(stroke: &Stroke) -> Vec<(Point, Point)> {
    let segment_count = match stroke.kind() {
        StrokeKind::Line | StrokeKind::Sweep => 1,
        StrokeKind::Bend | StrokeKind::Corner => 2,
        _ => 0,
    };
    stroke
        .points()
        .windows(2)
        .take(segment_count)
        .map(|points| (points[0], points[1]))
        .collect()
}

/// Tests inclusive membership in a near-horizontal or near-vertical segment.
fn point_on_axis_segment(point: Point, start: Point, end: Point) -> bool {
    let tolerance = CONNECTION_TOLERANCE;
    let vertical = (start.x - end.x).abs() <= tolerance
        && (point.x - start.x).abs() <= tolerance
        && point.y >= start.y.min(end.y) - tolerance
        && point.y <= start.y.max(end.y) + tolerance;
    let horizontal = (start.y - end.y).abs() <= tolerance
        && (point.y - start.y).abs() <= tolerance
        && point.x >= start.x.min(end.x) - tolerance
        && point.x <= start.x.max(end.x) + tolerance;
    vertical || horizontal
}

/// Paints one endpoint/control marker with a stable state color.
fn paint_control_point(
    transform: CanvasTransform,
    design_point: Point,
    state: ControlState,
    off_curve: bool,
    hovered: bool,
    active: bool,
    window: &mut Window,
) {
    let center = transform.design_to_screen(design_point);
    let marker_size = control_marker_size(hovered, active);
    let marker_bounds = centered_square(center, marker_size);
    let state_color = match state {
        ControlState::Normal => rgba(0xF579_00FF),
        ControlState::Online => rgba(0x73D2_16FF),
        ControlState::Matched => rgba(0x729F_CFFF),
    };
    let fill_color = if off_curve {
        rgba(0x0000_0000)
    } else {
        state_color
    };
    let border_color = if off_curve {
        state_color
    } else if active {
        rgba(0xFFFF_FFFF)
    } else if hovered {
        rgba(0x2424_28FF)
    } else {
        match state {
            ControlState::Normal => rgba(0xB84F_00FF),
            ControlState::Online => rgba(0x4E9A_06FF),
            ControlState::Matched => rgba(0x3465_A4FF),
        }
    };
    window.paint_quad(quad(
        marker_bounds,
        px(CONTROL_CORNER_RADIUS),
        fill_color,
        px(if off_curve {
            2.0
        } else if active {
            1.5
        } else {
            1.0
        }),
        border_color,
        BorderStyle::Solid,
    ));
}

/// Returns the screen-space marker size for its interaction state.
const fn control_marker_size(hovered: bool, active: bool) -> f32 {
    if active {
        CONTROL_SIZE + 4.0
    } else if hovered {
        CONTROL_SIZE + 2.0
    } else {
        CONTROL_SIZE
    }
}

/// Paints a dashed selection rectangle plus all eight resize handles.
fn paint_selection_bounds(transform: CanvasTransform, selection: Rect, window: &mut Window) {
    let corners = [
        selection.min,
        Point::new(selection.max.x, selection.min.y),
        selection.max,
        Point::new(selection.min.x, selection.max.y),
    ];
    let mut builder = PathBuilder::stroke(px(1.0)).dash_array(&[px(5.0), px(3.0)]);
    for (index, corner) in corners.into_iter().enumerate() {
        let screen = crisp_point(transform.design_to_screen(corner));
        if index == 0 {
            builder.move_to(screen);
        } else {
            builder.line_to(screen);
        }
    }
    builder.close();
    paint_built_path(builder, rgba(0xF579_00FF), window);

    for handle in ResizeHandle::ALL {
        let center = transform.design_to_screen(handle.position(selection));
        window.paint_quad(quad(
            centered_square(center, RESIZE_HANDLE_SIZE),
            px(CONTROL_CORNER_RADIUS),
            rgba(0xF579_00FF),
            px(1.0),
            rgba(0xB84F_00FF),
            BorderStyle::Solid,
        ));
    }
}

/// Paints transformed engine geometry as a read-only box without edit handles.
fn paint_read_only_bounds(transform: CanvasTransform, selection: Rect, window: &mut Window) {
    let corners = [
        selection.min,
        Point::new(selection.max.x, selection.min.y),
        selection.max,
        Point::new(selection.min.x, selection.max.y),
    ];
    let mut builder = PathBuilder::stroke(px(1.0)).dash_array(&[px(5.0), px(3.0)]);
    for (index, corner) in corners.into_iter().enumerate() {
        let screen = crisp_point(transform.design_to_screen(corner));
        if index == 0 {
            builder.move_to(screen);
        } else {
            builder.line_to(screen);
        }
    }
    builder.close();
    paint_built_path(builder, rgba(0xF579_00CC), window);
}

/// Paints freehand, marquee, and pointer feedback above committed geometry.
fn paint_gesture_overlays(
    transform: CanvasTransform,
    snapshot: &CanvasSnapshot,
    window: &mut Window,
) {
    if snapshot.overlay.freehand.len() >= 2 {
        paint_design_polyline(
            transform,
            &snapshot.overlay.freehand,
            2.0,
            rgba(0xFF9F_0AE6),
            window,
        );
    }

    if let Some(marquee) = snapshot.overlay.marquee {
        let screen_bounds = transform.design_rect_to_screen(marquee);
        window.paint_quad(fill(screen_bounds, rgba(0x0A84_FF1F)));
        let corners = [
            marquee.min,
            Point::new(marquee.max.x, marquee.min.y),
            marquee.max,
            Point::new(marquee.min.x, marquee.max.y),
        ];
        let mut builder = PathBuilder::stroke(px(1.0)).dash_array(&[px(4.0), px(3.0)]);
        for (index, corner) in corners.into_iter().enumerate() {
            let screen = crisp_point(transform.design_to_screen(corner));
            if index == 0 {
                builder.move_to(screen);
            } else {
                builder.line_to(screen);
            }
        }
        builder.close();
        paint_built_path(builder, rgba(0x0A84_FFD9), window);
    }

    if let Some(pointer) = snapshot.overlay.pointer {
        let center = transform.design_to_screen(pointer);
        let arm = px(5.0);
        let mut builder = PathBuilder::stroke(px(1.0));
        builder.move_to(point(center.x - arm, crisp(center.y)));
        builder.line_to(point(center.x + arm, crisp(center.y)));
        builder.move_to(point(crisp(center.x), center.y - arm));
        builder.line_to(point(crisp(center.x), center.y + arm));
        paint_built_path(builder, rgba(0x2526_2ACC), window);
    }
}

/// Paints the hairline around the paper after clipped content.
fn paint_artboard_edge(artboard: Bounds<Pixels>, window: &mut Window) {
    window.paint_quad(quad(
        artboard,
        px(0.0),
        rgba(0x0000_0000),
        px(1.0),
        rgba(0xFFFF_FF20),
        BorderStyle::Solid,
    ));
}

/// Paints a filled design-space polygon.
fn paint_design_polygon(
    transform: CanvasTransform,
    points: &[Point],
    color: Rgba,
    window: &mut Window,
) {
    if points.len() < 3 {
        return;
    }
    let screen_points = points
        .iter()
        .copied()
        .map(|point| transform.design_to_screen(point))
        .collect::<Vec<_>>();
    let mut builder = PathBuilder::fill();
    builder.add_polygon(&screen_points, true);
    paint_built_path(builder, color, window);
}

/// Paints a design-space polyline at a screen-constant width.
fn paint_design_polyline(
    transform: CanvasTransform,
    points: &[Point],
    width: f32,
    color: Rgba,
    window: &mut Window,
) {
    if points.len() < 2 {
        return;
    }
    let mut builder = PathBuilder::stroke(px(width));
    for (index, design_point) in points.iter().copied().enumerate() {
        let screen = transform.design_to_screen(design_point);
        if index == 0 {
            builder.move_to(screen);
        } else {
            builder.line_to(screen);
        }
    }
    paint_built_path(builder, color, window);
}

/// Builds and paints a path while treating degenerate geometry as non-fatal.
fn paint_built_path(builder: PathBuilder, color: Rgba, window: &mut Window) {
    if let Ok(path) = builder.build() {
        window.paint_path(path, color);
    }
}

/// Returns a centered square in screen coordinates.
fn centered_square(center: ScreenPoint<Pixels>, side: f32) -> Bounds<Pixels> {
    let half = px(side * 0.5);
    bounds(
        point(center.x - half, center.y - half),
        size(px(side), px(side)),
    )
}

/// Aligns a logical coordinate to a stable half-pixel hairline.
fn crisp(value: Pixels) -> Pixels {
    px(f32::from(value).round() + 0.5)
}

/// Aligns both coordinates to half-pixel hairlines.
fn crisp_point(value: ScreenPoint<Pixels>) -> ScreenPoint<Pixels> {
    point(crisp(value.x), crisp(value.y))
}

/// Chooses enough curve segments to remain smooth at the current zoom.
fn curve_steps(scale: f32) -> usize {
    if scale < 1.6 {
        16
    } else if scale < 3.2 {
        24
    } else if scale < 4.8 {
        40
    } else {
        64
    }
}

/// Normalizes invalid zoom values before they enter transform arithmetic.
fn sanitize_zoom(zoom: f32) -> f32 {
    if zoom.is_finite() {
        zoom.clamp(0.1, 8.0)
    } else {
        1.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a deterministic square canvas for pure geometry tests.
    fn test_transform(zoom: f32) -> CanvasTransform {
        CanvasTransform::new(
            bounds(point(px(10.0), px(20.0)), size(px(468.0), px(468.0))),
            zoom,
        )
    }

    /// Installs deterministic component data only for offline canvas tests.
    fn model_with_component_fixtures() -> EditorModel {
        let mut model = EditorModel::new();
        model.set_component_library(super::super::model::ComponentLibrary::builtin());
        model
    }

    /// Design coordinates survive a screen-space round trip at arbitrary zoom.
    #[test]
    fn transform_round_trip_is_stable() {
        let transform = test_transform(1.75);
        let design = Point::new(12.375, 188.625);
        let restored = transform.screen_to_design(transform.design_to_screen(design));
        assert!((restored.x - design.x).abs() < 0.000_1);
        assert!((restored.y - design.y).abs() < 0.000_1);
    }

    /// Panning translates the artboard while preserving transform inversion.
    #[test]
    fn panned_transform_remains_invertible() {
        let canvas_bounds = bounds(point(px(10.0), px(20.0)), size(px(468.0), px(468.0)));
        let centered = CanvasTransform::new(canvas_bounds, 1.75);
        let panned = CanvasTransform::new_with_pan(canvas_bounds, 1.75, Point::new(12.0, -8.0));
        assert!(
            (f32::from(panned.artboard.origin.x - centered.artboard.origin.x)
                - 12.0 * centered.scale)
                .abs()
                < 0.001
        );
        assert!(
            (f32::from(panned.artboard.origin.y - centered.artboard.origin.y)
                + 8.0 * centered.scale)
                .abs()
                < 0.001
        );
        let design = Point::new(34.0, 166.0);
        let restored = panned.screen_to_design(panned.design_to_screen(design));
        assert!(restored.distance(design) < 0.000_1);
    }

    /// Pinch zoom keeps the design coordinate under an off-center gesture
    /// stable, including when the view was already panned.
    #[test]
    fn anchored_zoom_preserves_the_gesture_coordinate() {
        let canvas_bounds = bounds(point(px(10.0), px(20.0)), size(px(620.0), px(460.0)));
        let anchor = point(px(487.25), px(138.75));
        let current_pan = Point::new(17.5, -9.25);
        let before = CanvasTransform::new_with_pan(canvas_bounds, 0.85, current_pan)
            .screen_to_design(anchor);
        let next_pan = pan_for_anchored_zoom(canvas_bounds, 0.85, current_pan, 2.15, anchor);
        let after =
            CanvasTransform::new_with_pan(canvas_bounds, 2.15, next_pan).screen_to_design(anchor);

        assert!(after.distance(before) < 0.000_1);
    }

    /// Consecutive pinch deltas use the last in-memory view state rather than
    /// waiting for a paint callback to publish another transform.
    #[test]
    fn consecutive_anchored_zooms_do_not_drift() {
        let canvas_bounds = bounds(point(px(5.0), px(8.0)), size(px(540.0), px(420.0)));
        let anchor = point(px(411.0), px(97.0));
        let initial_pan = Point::new(-14.0, 22.0);
        let initial_design =
            CanvasTransform::new_with_pan(canvas_bounds, 1.0, initial_pan).screen_to_design(anchor);

        let first_pan = pan_for_anchored_zoom(canvas_bounds, 1.0, initial_pan, 1.4, anchor);
        let second_pan = pan_for_anchored_zoom(canvas_bounds, 1.4, first_pan, 0.7, anchor);
        let final_design =
            CanvasTransform::new_with_pan(canvas_bounds, 0.7, second_pan).screen_to_design(anchor);

        assert!(final_design.distance(initial_design) < 0.000_1);
    }

    /// Invalid or ineffective gesture scales are a strict no-op for pan.
    #[test]
    fn anchored_zoom_rejects_invalid_scales_without_pan_drift() {
        let canvas_bounds = bounds(point(px(0.0), px(0.0)), size(px(500.0), px(500.0)));
        let anchor = point(px(370.0), px(120.0));
        let pan = Point::new(8.0, -13.0);

        for next_zoom in [1.0, 0.0, f32::NAN, f32::INFINITY] {
            assert_eq!(
                pan_for_anchored_zoom(canvas_bounds, 1.0, pan, next_zoom, anchor),
                pan
            );
        }
    }

    /// Fit zoom leaves the documented margin and centers the artboard.
    #[test]
    fn fit_transform_is_centered() {
        let transform = test_transform(1.0);
        let artboard = transform.artboard_bounds();
        assert!((f32::from(artboard.size.width) - 400.0).abs() < 0.001);
        assert!((f32::from(artboard.origin.x) - 44.0).abs() < 0.001);
        assert!((f32::from(artboard.origin.y) - 54.0).abs() < 0.001);
    }

    /// Every resize handle maps to the expected edge or corner.
    #[test]
    fn resize_handle_positions_cover_all_edges() {
        let rect = Rect::new(Point::new(20.0, 40.0), Point::new(180.0, 160.0));
        assert_eq!(ResizeHandle::NorthWest.position(rect), rect.min);
        assert_eq!(ResizeHandle::North.position(rect), Point::new(100.0, 40.0));
        assert_eq!(ResizeHandle::East.position(rect), Point::new(180.0, 100.0));
        assert_eq!(ResizeHandle::SouthEast.position(rect), rect.max);
        assert_eq!(ResizeHandle::West.opposite(), ResizeHandle::East);
    }

    /// Control and resize markers stay large and square in every state.
    #[test]
    fn control_markers_use_the_larger_visual_scale() {
        for (actual, expected) in [
            (control_marker_size(false, false), 16.0),
            (control_marker_size(true, false), 18.0),
            (control_marker_size(false, true), 20.0),
            (RESIZE_HANDLE_SIZE, 16.0),
            (CONTROL_CORNER_RADIUS, 1.5),
        ] {
            assert!((actual - expected).abs() < f32::EPSILON);
        }
    }

    /// Point and resize targets retain the same twelve-pixel screen radius as
    /// the canvas zoom changes.
    #[test]
    fn control_targets_keep_a_fixed_screen_radius() {
        let mut model = EditorModel::from_kage("1:0:0:20:20:180:180").expect("line");
        let selected = model.strokes()[0].id();
        assert!(model.select(selected, super::super::model::SelectionMode::Replace));
        let frame = Rect::new(Point::new(20.0, 20.0), Point::new(180.0, 180.0));

        for zoom in [0.75, 2.0] {
            let transform = test_transform(zoom);
            let snapshot = CanvasSnapshot::from_model(&model, zoom, CanvasOverlay::default());
            let endpoint = transform.design_to_screen(frame.min);
            let inside = point(endpoint.x + px(CONTROL_HIT_RADIUS - 0.1), endpoint.y);
            let outside = point(endpoint.x + px(CONTROL_HIT_RADIUS + 0.1), endpoint.y);

            assert_eq!(
                hit_control_point(&snapshot, transform, inside),
                Some(ControlPointRef {
                    stroke: selected,
                    point: 0,
                })
            );
            assert_eq!(hit_control_point(&snapshot, transform, outside), None);
            assert_eq!(
                hit_resize_handle(frame, transform.screen_to_design(inside), transform),
                Some(ResizeHandle::NorthWest)
            );
            assert_eq!(
                hit_resize_handle(frame, transform.screen_to_design(outside), transform),
                None
            );
        }
    }

    /// Invalid zoom never creates a non-finite or zero scale.
    #[test]
    fn invalid_zoom_falls_back_to_fit() {
        for zoom in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            let transform = test_transform(zoom);
            assert!(transform.scale().is_finite());
            assert!(transform.scale() > 0.0);
        }
    }

    /// Knockout clipping preserves the mask boundary instead of whitening the
    /// entire glyph outline.
    #[test]
    fn knockout_clipping_is_limited_to_the_selected_shape() {
        let subject = [
            Point::new(0.0, 0.0),
            Point::new(200.0, 0.0),
            Point::new(200.0, 200.0),
            Point::new(0.0, 200.0),
        ];
        let diamond = mask_polygon(MaskMode::Diamond);
        let clipped = clip_polygon_to_convex(&subject, &diamond);
        assert_eq!(clipped.len(), 4);
        for expected in diamond {
            assert!(
                clipped
                    .iter()
                    .any(|actual| actual.distance(expected) < 0.001)
            );
        }
    }

    /// A polygon wholly outside a knockout shape contributes no inverted ink.
    #[test]
    fn knockout_clipping_rejects_disjoint_outlines() {
        let subject = [
            Point::new(-40.0, -40.0),
            Point::new(-20.0, -40.0),
            Point::new(-20.0, -20.0),
            Point::new(-40.0, -20.0),
        ];
        let clipped = clip_polygon_to_convex(&subject, &mask_polygon(MaskMode::Circle));
        assert!(clipped.is_empty());
    }

    /// Filled-outline hit testing does not let an empty component interior
    /// intercept a click intended for content behind it.
    #[test]
    fn rendered_component_hit_test_respects_outline_whitespace() {
        let mut model = model_with_component_fixtures();
        let component = model
            .insert_component(
                "u53e3",
                Rect::new(Point::new(20.0, 20.0), Point::new(180.0, 180.0)),
            )
            .expect("built-in enclosure component");
        assert_eq!(
            hit_test_rendered(&model, Point::new(48.0, 100.0), 2.0),
            Some(component)
        );
        assert_eq!(
            hit_test_rendered(&model, Point::new(100.0, 100.0), 2.0),
            None
        );
    }

    /// Smooth Mincho mode changes the shared engine geometry used by painting
    /// and record-level interactions without dropping any contours.
    #[test]
    fn smooth_mincho_setting_reaches_all_rendered_outlines() {
        let mut model =
            EditorModel::from_kage("2:0:0:20:100:100:40:180:100").expect("curve fixture");
        let flat = CanvasSnapshot::from_model(&model, 1.0, CanvasOverlay::default());

        let mut settings = *model.settings();
        settings.use_curve = true;
        model.set_settings(settings);
        let smooth = CanvasSnapshot::from_model(&model, 1.0, CanvasOverlay::default());

        assert!(!flat.outlines.is_empty());
        assert!(!smooth.outlines.is_empty());
        assert_ne!(smooth.outlines, flat.outlines);
        assert_eq!(smooth.record_outlines.len(), model.strokes().len());
        assert!(
            smooth
                .record_outlines
                .iter()
                .any(|outlines| !outlines.is_empty())
        );
    }

    /// Marquee selection uses filled geometry rather than a component's full
    /// control frame.
    #[test]
    fn rendered_marquee_ignores_component_whitespace() {
        let mut model = model_with_component_fixtures();
        let component = model
            .insert_component(
                "u53e3",
                Rect::new(Point::new(20.0, 20.0), Point::new(180.0, 180.0)),
            )
            .expect("built-in enclosure component");
        assert!(
            records_intersecting_rendered(
                &model,
                Rect::new(Point::new(90.0, 90.0), Point::new(110.0, 110.0))
            )
            .is_empty()
        );
        assert_eq!(
            records_intersecting_rendered(
                &model,
                Rect::new(Point::new(45.0, 90.0), Point::new(52.0, 110.0))
            ),
            vec![component]
        );
    }

    /// Selected filled records are painted last and therefore also win a hit
    /// over an overlapping, later unselected record.
    #[test]
    fn selected_visual_frontmost_record_is_hit_first() {
        let mut model = EditorModel::from_kage("1:0:0:20:100:180:100$1:0:0:20:100:180:100")
            .expect("overlapping lines");
        let selected = model.strokes()[0].id();
        assert!(model.select(selected, super::super::model::SelectionMode::Replace));

        assert_eq!(
            hit_test_rendered(&model, Point::new(100.0, 100.0), 1.0),
            Some(selected)
        );
    }

    /// A single path exposes point controls, while frame records and a
    /// multi-selection expose only resize handles.
    #[test]
    fn selection_furniture_matches_kage_record_semantics() {
        let mut path = EditorModel::from_kage("1:0:0:20:20:180:180").expect("line");
        let path_id = path.strokes()[0].id();
        assert!(path.select(path_id, super::super::model::SelectionMode::Replace));
        assert!(!selection_uses_resize_handles(&path));
        assert_eq!(
            hit_selected_control_point(&path, Point::new(20.0, 20.0), 1.0),
            Some(ControlPointRef {
                stroke: path_id,
                point: 0,
            })
        );

        let mut frame = EditorModel::from_kage("0:98:0:20:20:180:180").expect("type-0 frame");
        let frame_id = frame.strokes()[0].id();
        assert!(frame.select(frame_id, super::super::model::SelectionMode::Replace));
        assert!(selection_uses_resize_handles(&frame));
        assert_eq!(
            hit_selected_control_point(&frame, Point::new(20.0, 20.0), 1.0),
            None
        );

        let mut multiple =
            EditorModel::from_kage("1:0:0:20:20:180:20$1:0:0:20:180:180:180").expect("two paths");
        multiple.select_all();
        assert!(selection_uses_resize_handles(&multiple));
        assert_eq!(
            hit_selected_control_point(&multiple, Point::new(20.0, 20.0), 1.0),
            None
        );
    }

    /// Curve handles use KAGE's exact off-curve roles rather than treating
    /// every interior point as either an anchor or a handle.
    #[test]
    fn off_curve_controls_match_kage_record_semantics() {
        let demo = EditorModel::demo();
        let demo_off_curve = demo
            .strokes()
            .iter()
            .map(|stroke| {
                (0..stroke.points().len())
                    .filter(|&point| is_off_curve_control(stroke, point))
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        assert_eq!(
            demo_off_curve,
            vec![vec![1], vec![], vec![], vec![], vec![1], vec![1], vec![1]]
        );

        let records = EditorModel::from_kage(
            "2:0:7:0:0:30:60:90:0$6:0:7:0:0:30:60:60:60:90:0$\
             7:0:7:0:0:30:0:60:60:90:90",
        )
        .expect("quadratic, cubic, and sweep fixtures");
        let roles = records
            .strokes()
            .iter()
            .map(|stroke| {
                (0..stroke.points().len())
                    .map(|point| is_off_curve_control(stroke, point))
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        assert_eq!(
            roles,
            vec![
                vec![false, true, false],
                vec![false, true, true, false],
                vec![false, false, true, false],
            ]
        );
    }

    /// A later type-0 operation moves engine polygons without rewriting their
    /// source points, so the canvas exposes only final read-only bounds.
    #[test]
    fn transformed_path_suppresses_stale_control_points() {
        let mut model = EditorModel::from_kage("1:0:0:20:80:60:80$0:98:0:0:0:200:200")
            .expect("line followed by a horizontal flip");
        let line = model.strokes()[0].id();
        assert!(model.select(line, super::super::model::SelectionMode::Replace));

        assert!(selection_has_transformed_geometry(&model));
        assert_eq!(
            hit_selected_control_point(&model, Point::new(20.0, 80.0), 1.0),
            None
        );

        let snapshot = CanvasSnapshot::from_model(&model, 1.0, CanvasOverlay::default());
        let bounds = rendered_record_bounds(&snapshot, line).expect("rendered line bounds");
        assert!(bounds.min.x > 100.0);
        assert!(bounds.max.x >= 180.0);
        let transform = test_transform(1.0);
        assert_eq!(
            hit_control_point(
                &snapshot,
                transform,
                transform.design_to_screen(Point::new(20.0, 80.0)),
            ),
            None
        );

        let mut preceding = EditorModel::from_kage("0:98:0:0:0:200:200$1:0:0:20:80:60:80")
            .expect("flip followed by a line");
        let unaffected = preceding.strokes()[1].id();
        assert!(preceding.select(unaffected, super::super::model::SelectionMode::Replace,));
        assert!(!selection_has_transformed_geometry(&preceding));

        let mut multiple =
            EditorModel::from_kage("1:0:0:20:80:60:80$1:0:0:20:120:60:120$0:98:0:0:0:200:200")
                .expect("two lines followed by a horizontal flip");
        let first = multiple.strokes()[0].id();
        let second = multiple.strokes()[1].id();
        assert!(multiple.select(first, super::super::model::SelectionMode::Replace));
        assert!(multiple.select(second, super::super::model::SelectionMode::Add));
        assert!(selection_has_transformed_geometry(&multiple));
        assert!(!selection_uses_resize_handles(&multiple));

        let snapshot = CanvasSnapshot::from_model(&multiple, 1.0, CanvasOverlay::default());
        let bounds = rendered_selection_bounds(&snapshot).expect("aggregate rendered bounds");
        assert!(bounds.min.x > 100.0);
        let transform = test_transform(1.0);
        assert_eq!(
            hit_resize_handle_screen(
                &snapshot,
                transform,
                transform.design_to_screen(Point::new(20.0, 80.0)),
            ),
            None
        );
    }

    /// A type-99 record also emits polygons that a later type-0 operation can
    /// move away from its source frame. Its stale frame must therefore become
    /// read-only just like the control points of an ordinary transformed path.
    #[test]
    fn transformed_component_suppresses_stale_frame_handles() {
        let mut model = model_with_component_fixtures();
        let component = model
            .insert_component(
                "u53e3",
                Rect::new(Point::new(0.0, 0.0), Point::new(100.0, 200.0)),
            )
            .expect("component fixture");
        model.insert_kage_transform(
            super::super::model::KageTransform::FlipHorizontal,
            Rect::new(Point::new(0.0, 0.0), Point::new(200.0, 200.0)),
        );
        assert!(model.select(component, super::super::model::SelectionMode::Replace,));

        assert!(selection_has_transformed_geometry(&model));
        assert!(!selection_uses_resize_handles(&model));

        let source_bounds = model
            .stroke(component)
            .and_then(Stroke::bounds)
            .expect("component source frame");
        assert!((source_bounds.max.x - 100.0).abs() <= f32::EPSILON);

        let snapshot = CanvasSnapshot::from_model(&model, 1.0, CanvasOverlay::default());
        let final_bounds = rendered_record_bounds(&snapshot, component)
            .expect("transformed component outline bounds");
        assert!(final_bounds.min.x >= 100.0);
    }

    /// An unselected point above the selected record cannot hide the visible
    /// selected control underneath it.
    #[test]
    fn unselected_point_does_not_occlude_selected_control() {
        let mut model = EditorModel::from_kage("1:0:0:20:20:180:20$1:0:0:20:20:20:180")
            .expect("overlapping endpoints");
        let selected = model.strokes()[0].id();
        assert!(model.select(selected, super::super::model::SelectionMode::Replace));

        assert_eq!(
            hit_selected_control_point(&model, Point::new(20.0, 20.0), 1.0),
            Some(ControlPointRef {
                stroke: selected,
                point: 0,
            })
        );
    }

    /// A path endpoint owns the click even though it is also a corner of the
    /// path's control bounds; single-path bounds do not expose resize handles.
    #[test]
    fn single_path_endpoint_hits_control_instead_of_resize() {
        let mut model = EditorModel::from_kage("1:0:0:20:20:180:180").expect("line");
        let selected = model.strokes()[0].id();
        assert!(model.select(selected, super::super::model::SelectionMode::Replace));
        let snapshot = CanvasSnapshot::from_model(&model, 1.0, CanvasOverlay::default());
        let transform = test_transform(1.0);

        assert_eq!(
            hit_test(
                &snapshot,
                transform,
                transform.design_to_screen(Point::new(20.0, 20.0))
            ),
            CanvasHit::ControlPoint(ControlPointRef {
                stroke: selected,
                point: 0,
            })
        );
    }

    /// Skeleton mode selects only its visible centerline, never the wider
    /// invisible Mincho polygon that would otherwise surround it.
    #[test]
    fn skeleton_does_not_hit_invisible_mincho_fill() {
        let mut model = EditorModel::from_kage("1:0:0:20:100:180:100").expect("line");
        let line = model.strokes()[0].id();
        let mut settings = *model.settings();
        settings.typeface = Typeface::Skeleton;
        model.set_settings(settings);

        assert_eq!(
            hit_test_rendered(&model, Point::new(100.0, 100.0), 1.0),
            Some(line)
        );
        assert_eq!(
            hit_test_rendered(&model, Point::new(100.0, 104.0), 1.0),
            None
        );
        assert!(
            records_intersecting_rendered(
                &model,
                Rect::new(Point::new(90.0, 103.0), Point::new(110.0, 105.0))
            )
            .is_empty()
        );
    }

    /// Components expose both a visible frame and recursively expanded ink in
    /// skeleton mode, with matching click and marquee ownership.
    #[test]
    fn skeleton_component_hit_matches_visible_frame() {
        let mut model = model_with_component_fixtures();
        let component = model
            .insert_component(
                "u53e3",
                Rect::new(Point::new(20.0, 20.0), Point::new(180.0, 180.0)),
            )
            .expect("component test fixture");
        let mut settings = *model.settings();
        settings.typeface = Typeface::Skeleton;
        model.set_settings(settings);

        assert_eq!(
            hit_test_rendered(&model, Point::new(20.0, 100.0), 1.0),
            Some(component)
        );
        assert_eq!(
            hit_test_rendered(&model, Point::new(48.0, 100.0), 1.0),
            Some(component)
        );
        assert_eq!(
            hit_test_rendered(&model, Point::new(100.0, 100.0), 1.0),
            None
        );
        assert!(
            records_intersecting_rendered(
                &model,
                Rect::new(Point::new(90.0, 90.0), Point::new(110.0, 110.0))
            )
            .is_empty()
        );
        assert_eq!(
            records_intersecting_rendered(
                &model,
                Rect::new(Point::new(19.0, 90.0), Point::new(21.0, 110.0))
            ),
            vec![component]
        );
        assert_eq!(
            records_intersecting_rendered(
                &model,
                Rect::new(Point::new(47.0, 90.0), Point::new(49.0, 110.0))
            ),
            vec![component]
        );
    }

    /// Endpoint furniture distinguishes exact matches from axis-line contact,
    /// while interior curve controls stay unclassified.
    #[test]
    fn control_states_match_kage_endpoint_connection_semantics() {
        let matched = EditorModel::from_kage("1:0:0:20:40:100:40$1:0:0:100:40:100:100")
            .expect("two joined lines");
        let snapshot = CanvasSnapshot::from_model(&matched, 1.0, CanvasOverlay::default());
        assert_eq!(
            control_state(
                &snapshot,
                ControlPointRef {
                    stroke: matched.strokes()[0].id(),
                    point: 1,
                }
            ),
            ControlState::Matched
        );

        let online = EditorModel::from_kage("1:0:0:60:0:60:40$1:0:0:20:40:100:40")
            .expect("endpoint on a horizontal line");
        let snapshot = CanvasSnapshot::from_model(&online, 1.0, CanvasOverlay::default());
        assert_eq!(
            control_state(
                &snapshot,
                ControlPointRef {
                    stroke: online.strokes()[0].id(),
                    point: 1,
                }
            ),
            ControlState::Online
        );

        let interior = EditorModel::from_kage("2:0:0:20:20:60:40:100:80$1:0:0:60:40:100:40")
            .expect("curve control sharing a line endpoint");
        let snapshot = CanvasSnapshot::from_model(&interior, 1.0, CanvasOverlay::default());
        assert_eq!(
            control_state(
                &snapshot,
                ControlPointRef {
                    stroke: interior.strokes()[0].id(),
                    point: 1,
                }
            ),
            ControlState::Normal
        );
    }

    /// Filled hit and marquee geometry follow aggregate type-0 transforms,
    /// while an intervening zero-polygon type-9 record keeps ownership stable.
    #[test]
    fn rendered_selection_uses_transformed_aggregate_record_outlines() {
        let mut model =
            EditorModel::from_kage("1:0:0:20:80:60:80$9:0:0:0:0:200:200$0:98:0:0:0:200:200")
                .expect("line, type-9 frame, and horizontal transform fixture");
        let line = model.strokes()[0].id();

        assert_eq!(
            hit_test_rendered(&model, Point::new(160.0, 80.0), 2.0),
            Some(line)
        );
        assert_ne!(
            hit_test_rendered(&model, Point::new(40.0, 80.0), 2.0),
            Some(line)
        );

        let transformed = records_intersecting_rendered(
            &model,
            Rect::new(Point::new(150.0, 74.0), Point::new(170.0, 86.0)),
        );
        assert!(transformed.contains(&line));
        let original = records_intersecting_rendered(
            &model,
            Rect::new(Point::new(30.0, 74.0), Point::new(50.0, 86.0)),
        );
        assert!(!original.contains(&line));

        let grouped = render_record_outlines(&model);
        assert_eq!(grouped.len(), 3);
        assert!(grouped[1].is_empty());
        assert!(grouped[2].is_empty());
        assert_eq!(
            grouped.into_iter().flatten().collect::<Vec<_>>(),
            render_source_outlines(&model, &model.to_kage())
        );

        assert!(model.select(line, super::super::model::SelectionMode::Replace));
        let snapshot = CanvasSnapshot::from_model(&model, 1.0, CanvasOverlay::default());
        assert_eq!(record_outlines_for_selection(&snapshot, true).count(), 1);
        assert_eq!(record_outlines_for_selection(&snapshot, false).count(), 2);
        assert_eq!(
            snapshot
                .record_outlines
                .iter()
                .flatten()
                .cloned()
                .collect::<Vec<_>>(),
            snapshot.outlines
        );
    }
}
