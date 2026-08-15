//! Editor domain model for the KAGE Editor example package.
//!
//! The module deliberately contains no GPUI types. A canvas can render the
//! borrowed strokes, turn pointer positions into [`Point`] values, and invoke
//! the transactional editing methods on [`EditorModel`]. KAGE coordinates use
//! the conventional 200-by-200 design space. Document geometry is quantized to
//! integral KAGE coordinates at mutation boundaries so control furniture and
//! the pinned renderer always consume the same values.

#![allow(dead_code)]

use std::collections::{BTreeSet, VecDeque};
use std::error::Error;
use std::fmt;

/// Width and height of the conventional KAGE design space.
pub const DESIGN_SIZE: f32 = 200.0;

/// Maximum number of committed undo states retained by an editor.
pub const HISTORY_LIMIT: usize = 30;

/// Default distance at which two control points are considered connected.
pub const CONNECTION_TOLERANCE: f32 = 0.75;

/// Maximum endpoint-to-endpoint distance recognized as a finishing hook.
const HOOK_CHORD_LIMIT: f32 = 25.0;

/// Maximum distance between a hook start and the preceding path endpoint.
const HOOK_ATTACHMENT_LIMIT: f32 = 10.0;

/// A point in KAGE design-space coordinates.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Point {
    /// Horizontal coordinate.
    pub x: f32,
    /// Vertical coordinate.
    pub y: f32,
}

impl Point {
    /// Creates a point.
    #[must_use]
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }

    /// Returns the Euclidean distance to another point.
    #[must_use]
    pub fn distance(self, other: Self) -> f32 {
        self.distance_squared(other).sqrt()
    }

    /// Returns the squared Euclidean distance to another point.
    #[must_use]
    pub fn distance_squared(self, other: Self) -> f32 {
        let dx = self.x - other.x;
        let dy = self.y - other.y;
        dx.mul_add(dx, dy * dy)
    }

    /// Linearly interpolates between this point and `other`.
    #[must_use]
    pub fn lerp(self, other: Self, amount: f32) -> Self {
        Self::new(
            (other.x - self.x).mul_add(amount, self.x),
            (other.y - self.y).mul_add(amount, self.y),
        )
    }

    /// Returns this point translated by a vector.
    #[must_use]
    pub const fn offset(self, delta: Self) -> Self {
        Self::new(self.x + delta.x, self.y + delta.y)
    }
}

/// An axis-aligned rectangle in design-space coordinates.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    /// Smallest horizontal and vertical values.
    pub min: Point,
    /// Largest horizontal and vertical values.
    pub max: Point,
}

impl Rect {
    /// Creates a normalized rectangle from two arbitrary corners.
    #[must_use]
    pub fn new(first: Point, second: Point) -> Self {
        Self {
            min: Point::new(first.x.min(second.x), first.y.min(second.y)),
            max: Point::new(first.x.max(second.x), first.y.max(second.y)),
        }
    }

    /// Computes a rectangle enclosing every supplied point.
    #[must_use]
    pub fn from_points(points: &[Point]) -> Option<Self> {
        let first = *points.first()?;
        let mut bounds = Self::new(first, first);
        for point in &points[1..] {
            bounds.min.x = bounds.min.x.min(point.x);
            bounds.min.y = bounds.min.y.min(point.y);
            bounds.max.x = bounds.max.x.max(point.x);
            bounds.max.y = bounds.max.y.max(point.y);
        }
        Some(bounds)
    }

    /// Returns this rectangle's width.
    #[must_use]
    pub fn width(self) -> f32 {
        self.max.x - self.min.x
    }

    /// Returns this rectangle's height.
    #[must_use]
    pub fn height(self) -> f32 {
        self.max.y - self.min.y
    }

    /// Returns the center of the rectangle.
    #[must_use]
    pub fn center(self) -> Point {
        self.min.lerp(self.max, 0.5)
    }

    /// Returns whether the rectangle contains a point, including its edge.
    #[must_use]
    pub fn contains(self, point: Point) -> bool {
        point.x >= self.min.x
            && point.x <= self.max.x
            && point.y >= self.min.y
            && point.y <= self.max.y
    }

    /// Returns whether two rectangles overlap, including edge contact.
    #[must_use]
    pub fn intersects(self, other: Self) -> bool {
        self.min.x <= other.max.x
            && self.max.x >= other.min.x
            && self.min.y <= other.max.y
            && self.max.y >= other.min.y
    }

    /// Returns a rectangle expanded equally on every side.
    #[must_use]
    pub fn expand(self, amount: f32) -> Self {
        Self {
            min: Point::new(self.min.x - amount, self.min.y - amount),
            max: Point::new(self.max.x + amount, self.max.y + amount),
        }
    }

    /// Returns the smallest rectangle containing both operands.
    #[must_use]
    pub fn union(self, other: Self) -> Self {
        Self {
            min: Point::new(self.min.x.min(other.min.x), self.min.y.min(other.min.y)),
            max: Point::new(self.max.x.max(other.max.x), self.max.y.max(other.max.y)),
        }
    }
}

/// Stable identity of one editable KAGE record.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct StrokeId(u64);

impl StrokeId {
    /// Returns the numeric identity, useful for GPUI element IDs.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// The supported KAGE record kinds.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StrokeKind {
    /// Type 0 special transform or engine-control record.
    Metadata,
    /// Type 1 straight segment.
    Line,
    /// Type 2 quadratic curve.
    Curve,
    /// Type 3 angular bend.
    Bend,
    /// Type 4 corner or swept bend.
    Corner,
    /// Type 6 cubic Bézier curve.
    Bezier,
    /// Type 7 four-point sweep.
    Sweep,
    /// Type 9 legacy/extension record retained for compatibility.
    Transform,
    /// Type 99 component reference.
    Component,
}

/// Transform encoded by a KAGE type 0 special record.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum KageTransform {
    /// Mirror vertically inside the record frame (`0:97:0`).
    FlipVertical,
    /// Mirror horizontally inside the record frame (`0:98:0`).
    FlipHorizontal,
    /// Rotate 90 degrees inside the record frame (`0:99:1`).
    Rotate90,
    /// Rotate 180 degrees inside the record frame (`0:99:2`).
    Rotate180,
    /// Rotate 270 degrees inside the record frame (`0:99:3`).
    Rotate270,
}

impl KageTransform {
    /// Returns the special record's head and tail parameter pair.
    pub(crate) const fn parameters(self) -> (i32, i32) {
        match self {
            Self::FlipVertical => (97, 0),
            Self::FlipHorizontal => (98, 0),
            Self::Rotate90 => (99, 1),
            Self::Rotate180 => (99, 2),
            Self::Rotate270 => (99, 3),
        }
    }
}

impl StrokeKind {
    /// Converts a numeric KAGE type to a supported kind.
    #[must_use]
    pub const fn from_code(code: i32) -> Option<Self> {
        match code {
            0 => Some(Self::Metadata),
            1 => Some(Self::Line),
            2 => Some(Self::Curve),
            3 => Some(Self::Bend),
            4 => Some(Self::Corner),
            6 => Some(Self::Bezier),
            7 => Some(Self::Sweep),
            9 => Some(Self::Transform),
            99 => Some(Self::Component),
            _ => None,
        }
    }

    /// Returns the numeric KAGE type code.
    #[must_use]
    pub const fn code(self) -> i32 {
        match self {
            Self::Metadata => 0,
            Self::Line => 1,
            Self::Curve => 2,
            Self::Bend => 3,
            Self::Corner => 4,
            Self::Bezier => 6,
            Self::Sweep => 7,
            Self::Transform => 9,
            Self::Component => 99,
        }
    }

    /// Returns the required number of geometry points.
    #[must_use]
    pub const fn point_count(self) -> usize {
        match self {
            Self::Metadata | Self::Line | Self::Transform | Self::Component => 2,
            Self::Curve | Self::Bend | Self::Corner => 3,
            Self::Bezier | Self::Sweep => 4,
        }
    }

    /// Returns whether this kind draws an ordinary editable stroke.
    #[must_use]
    pub const fn is_path(self) -> bool {
        matches!(
            self,
            Self::Line | Self::Curve | Self::Bend | Self::Corner | Self::Bezier | Self::Sweep
        )
    }

    /// Returns the head-shape choices supported by this KAGE stroke family.
    #[must_use]
    pub const fn head_shapes(self) -> &'static [i32] {
        match self {
            Self::Line => &[0, 2, 32, 12, 22],
            Self::Curve | Self::Bezier => &[0, 32, 12, 22, 7, 27],
            Self::Bend | Self::Sweep => &[0, 32, 12, 22],
            Self::Corner => &[0, 22],
            Self::Metadata | Self::Transform | Self::Component => &[],
        }
    }

    /// Returns the tail-shape choices supported by this KAGE stroke family.
    #[must_use]
    pub const fn tail_shapes(self) -> &'static [i32] {
        match self {
            Self::Line => &[0, 2, 32, 13, 23, 4, 313, 413, 24],
            Self::Curve | Self::Bezier => &[7, 0, 8, 4, 5],
            Self::Bend => &[0, 5, 32],
            Self::Corner => &[0, 5],
            Self::Sweep => &[7],
            Self::Metadata | Self::Transform | Self::Component => &[],
        }
    }

    /// Returns a style pair accepted by this ordinary stroke family.
    const fn default_style(self) -> Option<(i32, i32)> {
        match self {
            Self::Line | Self::Bend | Self::Corner => Some((0, 0)),
            Self::Curve | Self::Bezier | Self::Sweep => Some((0, 7)),
            Self::Metadata | Self::Transform | Self::Component => None,
        }
    }
}

/// Piecewise stretch pivots used by a component record.
///
/// In serialized KAGE, the destination x/y values occupy the record's two
/// generic parameter fields, while the source x/y values follow the component
/// name after one reserved zero.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ComponentStretch {
    /// Destination horizontal pivot, expressed relative to 100.
    pub destination_x: i32,
    /// Destination vertical pivot, expressed relative to 100.
    pub destination_y: i32,
    /// Source horizontal pivot, expressed relative to 100.
    pub source_x: i32,
    /// Source vertical pivot, expressed relative to 100.
    pub source_y: i32,
}

impl ComponentStretch {
    /// Creates a complete destination/source pivot pair.
    #[must_use]
    pub const fn new(destination_x: i32, destination_y: i32, source_x: i32, source_y: i32) -> Self {
        Self {
            destination_x,
            destination_y,
            source_x,
            source_y,
        }
    }

    /// Builds an x-only stretch from decoded destination and source pivots.
    ///
    /// Active horizontal destination pivots carry KAGE's `+200` sentinel.
    /// Encoding it here keeps UI presets from disabling their source pivot or
    /// accidentally introducing a y-axis distortion.
    #[must_use]
    pub const fn horizontal_pivots(destination_x: i32, source_x: i32) -> Self {
        Self::new(destination_x + 200, 0, source_x, 0)
    }
}

/// Stretch metadata declared by the first `0:1:0` record of a component.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ComponentStretchGuide {
    /// First metadata x coordinate.
    x0: i32,
    /// First metadata y coordinate.
    y0: i32,
    /// Second metadata x coordinate.
    x1: i32,
    /// Second metadata y coordinate.
    y1: i32,
}

impl ComponentStretchGuide {
    /// Converts a `-10..=10` inspector value into KAGE stretch fields.
    #[must_use]
    pub fn stretch(self, value: i32) -> ComponentStretch {
        let value = value.clamp(-10, 10) as f32;
        let destination_x = self.x0 as f32 + (self.x1 - self.x0) as f32 * value / 20.0 + 100.0;
        let destination_y = self.y0 as f32 + (self.y1 - self.y0) as f32 * value / 20.0 - 100.0;
        ComponentStretch::new(
            js_round_i32(destination_x),
            js_round_i32(destination_y),
            self.x0 - 100,
            self.y0 - 100,
        )
    }

    /// Recovers the nearest inspector value from serialized stretch fields.
    #[must_use]
    pub fn value(self, stretch: Option<ComponentStretch>) -> i32 {
        let (source_x, source_y, destination_x, destination_y) =
            normalize_component_stretch(stretch);
        if source_x == destination_x - 200 && source_y == destination_y
            || self.x0 == self.x1 && self.y0 == self.y1
        {
            return 0;
        }
        let value = if (self.x1 - self.x0).abs() > (self.y1 - self.y0).abs() {
            (destination_x - 100 - self.x0) as f32 / (self.x1 - self.x0) as f32 * 20.0
        } else {
            (destination_y + 100 - self.y0) as f32 / (self.y1 - self.y0) as f32 * 20.0
        };
        js_round_i32(value).clamp(-10, 10)
    }
}

/// A component reference carried by a type 99 record.
#[derive(Clone, Debug, PartialEq)]
pub struct ComponentRef {
    /// Name looked up in the component library or an external KAGE source.
    name: String,
    /// Optional engine destination/source stretch pivots.
    stretch: Option<ComponentStretch>,
    /// Reserved field stored before the source-pivot pair.
    stretch_reserved: i32,
    /// Whether the optional three-field suffix was present in source data.
    stretch_fields_present: bool,
}

impl ComponentRef {
    /// Creates a component reference without stretch values.
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            stretch: None,
            stretch_reserved: 0,
            stretch_fields_present: false,
        }
    }

    /// Returns the KAGE component name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the optional engine destination/source stretch pivots.
    #[must_use]
    pub const fn stretch(&self) -> Option<ComponentStretch> {
        self.stretch
    }
}

/// One KAGE record with stable editor identity.
#[derive(Clone, Debug, PartialEq)]
pub struct Stroke {
    /// Stable identity assigned by an [`EditorModel`].
    id: StrokeId,
    /// KAGE record kind.
    kind: StrokeKind,
    /// KAGE head-shape value (or the first generic parameter).
    head: i32,
    /// KAGE tail-shape value (or the second generic parameter).
    tail: i32,
    /// Geometry/control points.
    points: Vec<Point>,
    /// Component metadata for type 99 records.
    component: Option<ComponentRef>,
    /// Unknown trailing fields retained for forward-compatible serialization.
    extra_fields: Vec<String>,
}

impl Stroke {
    /// Creates an unassigned ordinary record.
    ///
    /// [`EditorModel::insert_stroke`] replaces the zero ID with a stable one.
    #[must_use]
    pub fn new(kind: StrokeKind, head: i32, tail: i32, points: Vec<Point>) -> Self {
        Self {
            id: StrokeId(0),
            kind,
            head,
            tail,
            points: points.into_iter().map(quantize_point).collect(),
            component: None,
            extra_fields: Vec::new(),
        }
    }

    /// Creates an unassigned type 1 line.
    #[must_use]
    pub fn line(start: Point, end: Point) -> Self {
        Self::new(StrokeKind::Line, 0, 0, vec![start, end])
    }

    /// Creates an unassigned type 2 quadratic curve.
    #[must_use]
    pub fn curve(start: Point, control: Point, end: Point) -> Self {
        Self::new(StrokeKind::Curve, 0, 7, vec![start, control, end])
    }

    /// Creates an unassigned type 6 cubic Bézier curve.
    #[must_use]
    pub fn bezier(start: Point, first: Point, second: Point, end: Point) -> Self {
        Self::new(StrokeKind::Bezier, 0, 7, vec![start, first, second, end])
    }

    /// Creates an unassigned type 3 bend.
    #[must_use]
    pub fn bend(start: Point, corner: Point, end: Point) -> Self {
        Self::new(StrokeKind::Bend, 0, 0, vec![start, corner, end])
    }

    /// Returns the stable identity.
    #[must_use]
    pub const fn id(&self) -> StrokeId {
        self.id
    }

    /// Returns the KAGE record kind.
    #[must_use]
    pub const fn kind(&self) -> StrokeKind {
        self.kind
    }

    /// Returns the head-shape value.
    #[must_use]
    pub const fn head(&self) -> i32 {
        self.head
    }

    /// Returns the tail-shape value.
    #[must_use]
    pub const fn tail(&self) -> i32 {
        self.tail
    }

    /// Returns the control points in record order.
    #[must_use]
    pub fn points(&self) -> &[Point] {
        &self.points
    }

    /// Returns component metadata when this is a type 99 record.
    #[must_use]
    pub const fn component(&self) -> Option<&ComponentRef> {
        self.component.as_ref()
    }

    /// Decodes a recognized type 0 KAGE transform record.
    #[must_use]
    pub const fn kage_transform(&self) -> Option<KageTransform> {
        if !matches!(self.kind, StrokeKind::Metadata) {
            return None;
        }
        match (self.head, self.tail) {
            (97, 0) => Some(KageTransform::FlipVertical),
            (98, 0) => Some(KageTransform::FlipHorizontal),
            (99, 1) => Some(KageTransform::Rotate90),
            (99, 2) => Some(KageTransform::Rotate180),
            (99, 3) => Some(KageTransform::Rotate270),
            _ => None,
        }
    }

    /// Returns unrecognized trailing fields retained from the source.
    #[must_use]
    pub fn extra_fields(&self) -> &[String] {
        &self.extra_fields
    }

    /// Returns a conservative control-point bounding rectangle.
    #[must_use]
    pub fn bounds(&self) -> Option<Rect> {
        Rect::from_points(&self.points)
    }

    /// Returns the shortest approximate distance from a point to this record.
    ///
    /// Quadratic and cubic curves are sampled densely enough for pointer hit
    /// testing in a 200-unit canvas. Component references use their frame.
    #[must_use]
    pub fn distance_to(&self, point: Point) -> f32 {
        if self.kind == StrokeKind::Component {
            return self
                .bounds()
                .map_or(f32::INFINITY, |bounds| distance_to_rect(point, bounds));
        }
        let samples = self.sampled_path(24);
        samples
            .windows(2)
            .map(|pair| distance_to_segment(point, pair[0], pair[1]))
            .fold(f32::INFINITY, f32::min)
    }

    /// Returns whether a point is within `tolerance` of the rendered skeleton.
    #[must_use]
    pub fn hit_test(&self, point: Point, tolerance: f32) -> bool {
        self.distance_to(point) <= tolerance
    }

    /// Produces a polyline approximation for painting or hit testing.
    #[must_use]
    pub fn sampled_path(&self, curve_steps: usize) -> Vec<Point> {
        let steps = curve_steps.max(2);
        match (self.kind, self.points.as_slice()) {
            (StrokeKind::Curve, [start, control, end]) => (0..=steps)
                .map(|step| {
                    let t = ratio(step, steps);
                    quadratic(*start, *control, *end, t)
                })
                .collect(),
            (StrokeKind::Bezier, [start, first, second, end]) => (0..=steps)
                .map(|step| {
                    let t = ratio(step, steps);
                    cubic(*start, *first, *second, *end, t)
                })
                .collect(),
            (StrokeKind::Sweep, [start, line_end, control, end]) => {
                let mut samples = Vec::with_capacity(steps + 2);
                samples.push(*start);
                samples.extend((0..=steps).map(|step| {
                    let t = ratio(step, steps);
                    quadratic(*line_end, *control, *end, t)
                }));
                samples
            }
            _ => self.points.clone(),
        }
    }
}

/// A parse failure annotated with its one-based record and field positions.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ParseError {
    /// A record did not contain its type, head, and tail fields.
    TooFewFields {
        /// One-based record number.
        record: usize,
        /// Actual number of fields.
        actual: usize,
    },
    /// An integer header field was malformed.
    InvalidInteger {
        /// One-based record number.
        record: usize,
        /// One-based field number.
        field: usize,
        /// Original field value.
        value: String,
    },
    /// A coordinate was malformed or not finite.
    InvalidCoordinate {
        /// One-based record number.
        record: usize,
        /// One-based field number.
        field: usize,
        /// Original field value.
        value: String,
    },
    /// A syntactically valid but unsupported record type was found.
    UnsupportedType {
        /// One-based record number.
        record: usize,
        /// Unsupported numeric type.
        code: i32,
    },
    /// A record did not provide enough coordinate pairs.
    MissingPoints {
        /// One-based record number.
        record: usize,
        /// Required number of points.
        expected: usize,
        /// Parsed number of points.
        actual: usize,
    },
    /// A variable-width special record had an unmatched coordinate.
    OddCoordinateCount {
        /// One-based record number.
        record: usize,
        /// Number of coordinate fields after the header.
        actual: usize,
    },
    /// A component record omitted its component name.
    MissingComponentName {
        /// One-based record number.
        record: usize,
    },
}

impl fmt::Display for ParseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooFewFields { record, actual } => {
                write!(
                    formatter,
                    "record {record} has {actual} fields; expected at least 3"
                )
            }
            Self::InvalidInteger {
                record,
                field,
                value,
            } => write!(
                formatter,
                "record {record}, field {field} is not an integer: {value:?}"
            ),
            Self::InvalidCoordinate {
                record,
                field,
                value,
            } => write!(
                formatter,
                "record {record}, field {field} is not a finite coordinate: {value:?}"
            ),
            Self::UnsupportedType { record, code } => {
                write!(
                    formatter,
                    "record {record} uses unsupported KAGE type {code}"
                )
            }
            Self::MissingPoints {
                record,
                expected,
                actual,
            } => write!(
                formatter,
                "record {record} has {actual} points; expected {expected}"
            ),
            Self::OddCoordinateCount { record, actual } => write!(
                formatter,
                "record {record} has an odd number ({actual}) of coordinate fields"
            ),
            Self::MissingComponentName { record } => {
                write!(formatter, "component record {record} has no component name")
            }
        }
    }
}

impl Error for ParseError {}

/// Parses KAGE data separated by `$` or line breaks.
///
/// Numeric payload fields follow KAGE's JavaScript importer semantics: finite
/// values are floored, while malformed values become zero. Unsupported record
/// kinds are ignored and unknown trailing fields on fixed-width records are
/// retained.
///
/// # Errors
///
/// Returns [`ParseError`] when a supported record is structurally incomplete.
pub fn parse_kage(source: &str) -> Result<Vec<Stroke>, ParseError> {
    let normalized = source.replace(['\r', '\n'], "$");
    normalized
        .split('$')
        .filter(|record| !record.trim().is_empty())
        .enumerate()
        .filter_map(|(index, record)| {
            let record = record.trim();
            let kind = record.split(':').next().and_then(parse_record_kind)?;
            StrokeKind::from_code(kind)?;
            match parse_record(record, index + 1) {
                Ok(stroke)
                    if stroke.kind == StrokeKind::Metadata && stroke.kage_transform().is_none() =>
                {
                    None
                }
                result => Some(result),
            }
        })
        .collect()
}

/// Serializes records as canonical `$`-separated KAGE data.
#[must_use]
pub fn serialize_kage(strokes: &[Stroke]) -> String {
    strokes
        .iter()
        .map(serialize_record)
        .collect::<Vec<_>>()
        .join("$")
}

/// Parses one colon-separated KAGE record.
fn parse_record(record: &str, record_number: usize) -> Result<Stroke, ParseError> {
    let fields: Vec<&str> = record.split(':').map(str::trim).collect();
    if fields.len() < 3 {
        return Err(ParseError::TooFewFields {
            record: record_number,
            actual: fields.len(),
        });
    }
    let kind_code = parse_record_kind(fields[0]).ok_or_else(|| ParseError::InvalidInteger {
        record: record_number,
        field: 1,
        value: fields[0].to_owned(),
    })?;
    let kind = StrokeKind::from_code(kind_code).ok_or(ParseError::UnsupportedType {
        record: record_number,
        code: kind_code,
    })?;
    let head = parse_number(fields[1]);
    let tail = parse_number(fields[2]);

    if kind == StrokeKind::Component {
        return parse_component_record(&fields, record_number, head, tail);
    }

    let count = kind.point_count();
    let coordinate_fields = count * 2;
    if fields.len() < 3 + coordinate_fields {
        return Err(ParseError::MissingPoints {
            record: record_number,
            expected: count,
            actual: fields.len().saturating_sub(3) / 2,
        });
    }
    let points = parse_points(&fields[3..3 + coordinate_fields]);
    let mut stroke = Stroke::new(kind, head, tail, points);
    stroke.extra_fields = fields[3 + coordinate_fields..]
        .iter()
        .map(ToString::to_string)
        .collect();
    Ok(stroke)
}

/// Parses a type 99 component record and its optional stretch triplet.
fn parse_component_record(
    fields: &[&str],
    record_number: usize,
    head: i32,
    tail: i32,
) -> Result<Stroke, ParseError> {
    if fields.len() < 8 {
        return Err(if fields.len() < 7 {
            ParseError::MissingPoints {
                record: record_number,
                expected: 2,
                actual: fields.len().saturating_sub(3) / 2,
            }
        } else {
            ParseError::MissingComponentName {
                record: record_number,
            }
        });
    }
    let points = parse_points(&fields[3..7]);
    if fields[7].is_empty() {
        return Err(ParseError::MissingComponentName {
            record: record_number,
        });
    }
    let mut component = ComponentRef::new(fields[7]);
    let mut extra_start = 8;
    if fields.len() >= 11 {
        let values = fields[8..11]
            .iter()
            .map(|value| parse_number(value))
            .collect::<Vec<_>>();
        component.stretch = Some(ComponentStretch::new(head, tail, values[1], values[2]));
        component.stretch_reserved = values[0];
        component.stretch_fields_present = true;
        extra_start = 11;
    } else if head != 0 || tail != 0 {
        component.stretch = Some(ComponentStretch::new(head, tail, 0, 0));
    }
    let mut stroke = Stroke::new(StrokeKind::Component, head, tail, points);
    stroke.component = Some(component);
    stroke.extra_fields = fields[extra_start..]
        .iter()
        .map(ToString::to_string)
        .collect();
    Ok(stroke)
}

/// Parses a sequence of alternating x/y coordinate fields.
fn parse_points(fields: &[&str]) -> Vec<Point> {
    fields
        .chunks_exact(2)
        .map(|pair| Point::new(parse_number(pair[0]) as f32, parse_number(pair[1]) as f32))
        .collect()
}

/// Parses the record discriminator without coercing an unknown kind to zero.
fn parse_record_kind(value: &str) -> Option<i32> {
    let value = value.parse::<f64>().ok()?;
    if !value.is_finite()
        || value.fract() != 0.0
        || value < f64::from(i32::MIN)
        || value > f64::from(i32::MAX)
    {
        return None;
    }
    Some(value as i32)
}

/// Floors a finite KAGE payload number and coerces malformed values to zero.
fn parse_number(value: &str) -> i32 {
    let Some(number) = value
        .parse::<f64>()
        .ok()
        .filter(|number| number.is_finite())
    else {
        return 0;
    };
    number
        .floor()
        .clamp(f64::from(i32::MIN), f64::from(i32::MAX)) as i32
}

/// Serializes one record while preserving extension fields.
fn serialize_record(stroke: &Stroke) -> String {
    let mut fields = vec![
        stroke.kind.code().to_string(),
        stroke.head.to_string(),
        stroke.tail.to_string(),
    ];
    for point in &stroke.points {
        fields.push(format_number(point.x));
        fields.push(format_number(point.y));
    }
    if let Some(component) = &stroke.component {
        fields.push(component.name.clone());
        let stretch = component.stretch.unwrap_or_default();
        fields.push(component.stretch_reserved.to_string());
        fields.push(stretch.source_x.to_string());
        fields.push(stretch.source_y.to_string());
    }
    fields.extend(stroke.extra_fields.iter().cloned());
    fields.join(":")
}

/// Formats a coordinate using KAGE's integral document semantics.
fn format_number(value: f32) -> String {
    format!("{:.0}", value.round())
}

/// Converts a sample index to a stable zero-to-one ratio.
#[allow(clippy::cast_precision_loss)]
fn ratio(index: usize, count: usize) -> f32 {
    index as f32 / count as f32
}

/// Evaluates a quadratic Bézier curve.
fn quadratic(start: Point, control: Point, end: Point, t: f32) -> Point {
    let first = start.lerp(control, t);
    let second = control.lerp(end, t);
    first.lerp(second, t)
}

/// Evaluates a cubic Bézier curve.
fn cubic(start: Point, first: Point, second: Point, end: Point, t: f32) -> Point {
    let left = quadratic(start, first, second, t);
    let right = quadratic(first, second, end, t);
    left.lerp(right, t)
}

/// Returns the distance from a point to a finite segment.
fn distance_to_segment(point: Point, start: Point, end: Point) -> f32 {
    let dx = end.x - start.x;
    let dy = end.y - start.y;
    let length_squared = dx.mul_add(dx, dy * dy);
    if length_squared <= f32::EPSILON {
        return point.distance(start);
    }
    let projection = ((point.x - start.x).mul_add(dx, (point.y - start.y) * dy) / length_squared)
        .clamp(0.0, 1.0);
    point.distance(Point::new(
        dx.mul_add(projection, start.x),
        dy.mul_add(projection, start.y),
    ))
}

/// Returns the distance to a rectangle outline, or zero inside it.
fn distance_to_rect(point: Point, bounds: Rect) -> f32 {
    if bounds.contains(point) {
        return 0.0;
    }
    let dx = if point.x < bounds.min.x {
        bounds.min.x - point.x
    } else if point.x > bounds.max.x {
        point.x - bounds.max.x
    } else {
        0.0
    };
    let dy = if point.y < bounds.min.y {
        bounds.min.y - point.y
    } else if point.y > bounds.max.y {
        point.y - bounds.max.y
    } else {
        0.0
    };
    dx.hypot(dy)
}

/// A searchable component definition.
#[derive(Clone, Debug, PartialEq)]
pub struct ComponentDefinition {
    /// Stable KAGE component name.
    name: String,
    /// Human-readable display label.
    label: String,
    /// Search aliases and descriptive terms.
    keywords: Vec<String>,
    /// KAGE source expanded by decomposition.
    source: String,
    /// Optional slider guide extracted from a leading metadata record.
    stretch_guide: Option<ComponentStretchGuide>,
}

impl ComponentDefinition {
    /// Creates a component definition.
    #[must_use]
    pub fn new(
        name: impl Into<String>,
        label: impl Into<String>,
        keywords: impl IntoIterator<Item = impl Into<String>>,
        source: impl Into<String>,
    ) -> Self {
        let (source, stretch_guide) = split_component_metadata(source.into());
        Self {
            name: name.into(),
            label: label.into(),
            keywords: keywords.into_iter().map(Into::into).collect(),
            source,
            stretch_guide,
        }
    }

    /// Returns the stable KAGE name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the human-readable label.
    #[must_use]
    pub fn label(&self) -> &str {
        &self.label
    }

    /// Returns search aliases.
    #[must_use]
    pub fn keywords(&self) -> &[String] {
        &self.keywords
    }

    /// Returns the KAGE source.
    #[must_use]
    pub fn source(&self) -> &str {
        &self.source
    }

    /// Returns the stretch guide declared by this component source.
    #[must_use]
    pub const fn stretch_guide(&self) -> Option<ComponentStretchGuide> {
        self.stretch_guide
    }
}

/// Extracts a leading stretch declaration without exposing it as a type-0 edit.
fn split_component_metadata(source: String) -> (String, Option<ComponentStretchGuide>) {
    let (first, remainder) = source
        .split_once('$')
        .map_or((source.as_str(), ""), |parts| parts);
    let fields = first.split(':').map(str::trim).collect::<Vec<_>>();
    if fields.len() != 7 || fields[..3] != ["0", "1", "0"] {
        return (source, None);
    }
    let guide = ComponentStretchGuide {
        x0: parse_number(fields[3]),
        y0: parse_number(fields[4]),
        x1: parse_number(fields[5]),
        y1: parse_number(fields[6]),
    };
    (remainder.to_owned(), Some(guide))
}

/// Normalizes compact/legacy stretch fields into source/destination pivots.
fn normalize_component_stretch(stretch: Option<ComponentStretch>) -> (i32, i32, i32, i32) {
    let stretch = stretch.unwrap_or_default();
    if stretch.destination_x <= 100 {
        (
            0,
            0,
            stretch.destination_x.saturating_add(200),
            stretch.destination_y,
        )
    } else {
        (
            stretch.source_x,
            stretch.source_y,
            stretch.destination_x,
            stretch.destination_y,
        )
    }
}

/// In-memory searchable collection of reusable KAGE components.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ComponentLibrary {
    /// Definitions in display order.
    entries: Vec<ComponentDefinition>,
}

impl ComponentLibrary {
    /// Creates an empty component library.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    /// Creates deterministic component definitions used only by unit tests.
    #[cfg(test)]
    #[must_use]
    pub fn builtin() -> Self {
        let entries = vec![
            ComponentDefinition::new(
                "u53e3",
                "口 · Enclosure",
                ["mouth", "box", "口", "kou"],
                "1:12:13:35:30:35:170$1:2:2:35:30:165:30$1:22:23:165:30:165:170$1:2:2:35:170:165:170",
            ),
            ComponentDefinition::new(
                "u6728",
                "木 · Tree",
                ["tree", "wood", "木", "ki"],
                "0:1:0:100:100:180:100$1:0:0:28:76:172:76$1:12:13:100:24:100:180$2:0:7:98:79:72:126:30:159$2:0:7:103:79:132:122:174:151",
            ),
            ComponentDefinition::new(
                "u6c35-01",
                "氵 · Water",
                ["water", "three dots", "氵", "sanzui"],
                "0:1:0:100:40:100:160$2:7:8:45:24:76:34:91:50$2:7:8:27:84:62:94:79:109$2:7:8:22:157:67:148:92:120",
            ),
            ComponentDefinition::new(
                "u5fc3",
                "心 · Heart",
                ["heart", "mind", "心", "kokoro"],
                "2:7:8:35:111:25:140:31:164$2:7:8:64:87:77:119:83:153$2:7:8:105:77:121:114:128:150$2:7:8:137:105:163:129:174:157",
            ),
            ComponentDefinition::new(
                "u65e5",
                "日 · Sun",
                ["sun", "day", "日", "hi"],
                "1:12:13:40:24:40:177$1:2:2:40:24:162:24$1:22:23:162:24:162:177$1:0:0:40:96:162:96$1:2:2:40:177:162:177",
            ),
            ComponentDefinition::new(
                "u6c38",
                "永 · Eternity",
                ["eternity", "water", "永", "ei"],
                demo_source(),
            ),
            ComponentDefinition::new(
                "u6797",
                "林 · Grove",
                ["grove", "two trees", "林", "hayashi"],
                "99:0:0:0:0:98:200:u6728$99:0:0:102:0:200:200:u6728",
            ),
            ComponentDefinition::new(
                "u54c1",
                "品 · Goods",
                ["goods", "three mouths", "品", "hin"],
                "99:0:0:55:4:145:96:u53e3$99:0:0:4:104:96:196:u53e3$99:0:0:104:104:196:196:u53e3",
            ),
            ComponentDefinition::new(
                "u6c90",
                "沐 · Wash",
                ["wash", "water tree", "沐", "moku"],
                "99:0:0:0:0:76:200:u6c35-01$99:0:0:66:0:200:200:u6728",
            ),
            ComponentDefinition::new(
                "u68ee",
                "森 · Forest",
                ["forest", "three trees", "森", "mori"],
                "99:0:0:50:0:150:105:u6728$99:0:0:0:92:100:200:u6728$99:0:0:100:92:200:200:u6728",
            ),
            ComponentDefinition::new(
                "u76f8",
                "相 · Mutual",
                ["mutual", "tree eye", "相", "sou"],
                "99:0:0:0:0:104:200:u6728$99:0:0:96:12:200:188:u65e5",
            ),
            ComponentDefinition::new(
                "u60f3",
                "想 · Thought",
                ["thought", "imagine", "想", "sou"],
                "99:0:0:8:0:192:132:u76f8$99:0:0:20:116:180:200:u5fc3",
            ),
        ];
        Self { entries }
    }

    /// Returns all definitions in display order.
    #[must_use]
    pub fn entries(&self) -> &[ComponentDefinition] {
        &self.entries
    }

    /// Finds a component by exact, ASCII-case-insensitive KAGE name.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&ComponentDefinition> {
        self.entries
            .iter()
            .find(|entry| entry.name.eq_ignore_ascii_case(name))
    }

    /// Searches names, labels, and aliases using whitespace-separated terms.
    #[must_use]
    pub fn search(&self, query: &str) -> Vec<&ComponentDefinition> {
        let terms: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
        if terms.is_empty() {
            return self.entries.iter().collect();
        }
        self.entries
            .iter()
            .filter(|entry| {
                let dependencies = self.dependency_names(entry);
                let haystack = format!(
                    "{} {} {} {}",
                    entry.name,
                    entry.label,
                    entry.keywords.join(" "),
                    dependencies.join(" ")
                )
                .to_lowercase();
                terms.iter().all(|term| haystack.contains(term))
            })
            .collect()
    }

    /// Collects direct and transitive type-99 dependency names for search.
    fn dependency_names(&self, entry: &ComponentDefinition) -> Vec<String> {
        let mut names = Vec::new();
        let mut visited = BTreeSet::new();
        self.collect_dependency_names(&entry.source, &mut names, &mut visited);
        names
    }

    /// Traverses one KAGE source with a cycle guard.
    fn collect_dependency_names(
        &self,
        source: &str,
        names: &mut Vec<String>,
        visited: &mut BTreeSet<String>,
    ) {
        for record in source.split('$') {
            let fields = record.split(':').collect::<Vec<_>>();
            if fields.first().copied() != Some("99") {
                continue;
            }
            let Some(name) = fields.get(7).copied().filter(|name| !name.is_empty()) else {
                continue;
            };
            if !visited.insert(name.to_owned()) {
                continue;
            }
            names.push(name.to_owned());
            if let Some(dependency) = self.get(name) {
                self.collect_dependency_names(&dependency.source, names, visited);
            }
        }
    }

    /// Adds or replaces a definition with the same KAGE name.
    pub fn upsert(&mut self, definition: ComponentDefinition) {
        if let Some(existing) = self
            .entries
            .iter_mut()
            .find(|entry| entry.name == definition.name)
        {
            *existing = definition;
        } else {
            self.entries.push(definition);
        }
    }
}

/// Grid display and snapping preferences.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GridSettings {
    /// Whether grid guides are painted.
    pub visible: bool,
    /// Horizontal coordinate of a major-grid origin.
    pub origin_x: f32,
    /// Vertical coordinate of a major-grid origin.
    pub origin_y: f32,
    /// Distance between vertical major grid lines in design units.
    pub spacing_x: f32,
    /// Distance between horizontal major grid lines in design units.
    pub spacing_y: f32,
    /// Whether pointer coordinates snap to the grid.
    pub snap: bool,
    /// Number of minor divisions per major interval.
    pub subdivisions: u8,
}

impl Default for GridSettings {
    fn default() -> Self {
        Self {
            visible: true,
            origin_x: 0.0,
            origin_y: 0.0,
            spacing_x: 20.0,
            spacing_y: 20.0,
            snap: false,
            subdivisions: 2,
        }
    }
}

impl GridSettings {
    /// Snaps a point when snapping is enabled and spacing is valid.
    #[must_use]
    pub fn snap_point(self, point: Point) -> Point {
        if !self.snap
            || self.spacing_x <= f32::EPSILON
            || !self.spacing_x.is_finite()
            || self.spacing_y <= f32::EPSILON
            || !self.spacing_y.is_finite()
            || !self.origin_x.is_finite()
            || !self.origin_y.is_finite()
        {
            return point;
        }
        Point::new(
            ((point.x - self.origin_x) / self.spacing_x)
                .round()
                .mul_add(self.spacing_x, self.origin_x),
            ((point.y - self.origin_y) / self.spacing_y)
                .round()
                .mul_add(self.spacing_y, self.origin_y),
        )
    }
}

/// Preview typeface used by the renderer.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Typeface {
    /// Traditional KAGE Mincho treatment.
    #[default]
    Mincho,
    /// Gothic/sans treatment.
    Gothic,
    /// Hairline skeleton-only treatment.
    Skeleton,
}

/// UI language selection.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum UiLanguage {
    /// English interface.
    English,
    /// Japanese interface.
    Japanese,
    /// Simplified Chinese interface.
    SimplifiedChinese,
    /// Traditional Chinese interface.
    #[default]
    TraditionalChinese,
    /// Korean interface.
    Korean,
}

/// Determines which stroke centerlines are shown above the preview.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum CenterlineMode {
    /// Hide centerlines.
    None,
    /// Show centerlines only for selected records.
    #[default]
    Selection,
    /// Show centerlines for every record.
    Always,
}

/// Shape of the white knockout/mask preview.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum MaskMode {
    /// Disable the mask.
    #[default]
    None,
    /// Circular mask.
    Circle,
    /// Rounded-square mask.
    RoundedSquare,
    /// Square mask.
    Square,
    /// Diamond mask.
    Diamond,
}

/// Persistent view and editor preferences.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EditorSettings {
    /// Grid configuration.
    pub grid: GridSettings,
    /// Preview typeface.
    pub typeface: Typeface,
    /// Whether the KAGE engine emits curve-aware Mincho outlines.
    pub use_curve: bool,
    /// Stroke centerline visibility policy.
    pub centerline: CenterlineMode,
    /// White knockout/mask preview shape.
    pub mask: MaskMode,
    /// Interface language.
    pub language: UiLanguage,
}

impl Default for EditorSettings {
    fn default() -> Self {
        Self {
            grid: GridSettings::default(),
            typeface: Typeface::Mincho,
            use_curve: false,
            centerline: CenterlineMode::Selection,
            mask: MaskMode::None,
            language: UiLanguage::TraditionalChinese,
        }
    }
}

/// How a click modifies the current selection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SelectionMode {
    /// Clear the old selection and select only the target.
    Replace,
    /// Add the target to the selection.
    Add,
    /// Toggle membership of the target.
    Toggle,
    /// Remove the target from the selection.
    Remove,
}

/// Direction used by one-step z-order operations.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OrderDirection {
    /// Move toward the beginning/back of the stroke array.
    Backward,
    /// Move toward the end/front of the stroke array.
    Forward,
}

/// A reference to one control point on one record.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ControlPointRef {
    /// Owning record.
    pub stroke: StrokeId,
    /// Zero-based point index in that record.
    pub point: usize,
}

/// The topmost hit-test result.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hit {
    /// Hit record.
    pub stroke: StrokeId,
    /// Approximate design-space distance from the skeleton.
    pub distance: f32,
}

/// Affine transform using the conventional six-value 2D matrix.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AffineTransform {
    /// Horizontal x basis.
    pub m11: f32,
    /// Vertical x basis.
    pub m12: f32,
    /// Horizontal y basis.
    pub m21: f32,
    /// Vertical y basis.
    pub m22: f32,
    /// Horizontal translation.
    pub tx: f32,
    /// Vertical translation.
    pub ty: f32,
}

impl AffineTransform {
    /// Identity transform.
    pub const IDENTITY: Self = Self {
        m11: 1.0,
        m12: 0.0,
        m21: 0.0,
        m22: 1.0,
        tx: 0.0,
        ty: 0.0,
    };

    /// Creates a translation.
    #[must_use]
    pub const fn translation(delta: Point) -> Self {
        Self {
            tx: delta.x,
            ty: delta.y,
            ..Self::IDENTITY
        }
    }

    /// Creates a scale around a design-space origin.
    #[must_use]
    pub fn scale_about(scale_x: f32, scale_y: f32, origin: Point) -> Self {
        Self {
            m11: scale_x,
            m12: 0.0,
            m21: 0.0,
            m22: scale_y,
            tx: origin.x - origin.x * scale_x,
            ty: origin.y - origin.y * scale_y,
        }
    }

    /// Creates a clockwise rotation in screen/design coordinates.
    #[must_use]
    pub fn rotation_about(radians: f32, origin: Point) -> Self {
        let (sine, cosine) = radians.sin_cos();
        Self {
            m11: cosine,
            m12: sine,
            m21: -sine,
            m22: cosine,
            tx: origin.x - cosine * origin.x + sine * origin.y,
            ty: origin.y - sine * origin.x - cosine * origin.y,
        }
    }

    /// Creates a horizontal mirror around an x coordinate.
    #[must_use]
    pub fn flip_horizontal(axis_x: f32) -> Self {
        Self::scale_about(-1.0, 1.0, Point::new(axis_x, 0.0))
    }

    /// Creates a vertical mirror around a y coordinate.
    #[must_use]
    pub fn flip_vertical(axis_y: f32) -> Self {
        Self::scale_about(1.0, -1.0, Point::new(0.0, axis_y))
    }

    /// Applies the transform to a point.
    #[must_use]
    pub fn apply(self, point: Point) -> Point {
        Point::new(
            self.m11
                .mul_add(point.x, self.m21.mul_add(point.y, self.tx)),
            self.m12
                .mul_add(point.x, self.m22.mul_add(point.y, self.ty)),
        )
    }
}

/// Describes one applied transform for inspectors and activity UI.
#[derive(Clone, Debug, PartialEq)]
pub struct TransformRecord {
    /// User-facing operation label.
    label: String,
    /// Affine transform that was applied.
    transform: AffineTransform,
    /// Records explicitly targeted by the operation.
    targets: Vec<StrokeId>,
}

impl TransformRecord {
    /// Returns the operation label.
    #[must_use]
    pub fn label(&self) -> &str {
        &self.label
    }

    /// Returns the applied affine transform.
    #[must_use]
    pub const fn transform(&self) -> AffineTransform {
        self.transform
    }

    /// Returns explicitly targeted record IDs.
    #[must_use]
    pub fn targets(&self) -> &[StrokeId] {
        &self.targets
    }
}

/// Severity of a model validation finding.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ValidationSeverity {
    /// Data cannot be represented reliably by a KAGE renderer.
    Error,
    /// Data is representable but unusual or likely unintended.
    Warning,
}

/// Stable machine-readable validation category.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ValidationCode {
    /// Geometry point count does not match the stroke kind.
    PointCount,
    /// A coordinate is not finite.
    NonFiniteCoordinate,
    /// A path is degenerate or effectively lengthless.
    DegeneratePath,
    /// A component is missing its name or metadata.
    MissingComponent,
    /// An ordinary path unexpectedly carries component metadata.
    UnexpectedComponent,
    /// Head shape is outside the known KAGE families.
    UnknownHead,
    /// Tail shape is outside the known KAGE families.
    UnknownTail,
    /// Head and tail are individually known but invalid for this stroke.
    InvalidStyleCombination,
    /// A grid preference is invalid.
    InvalidGrid,
}

/// One validation finding suitable for an inspector or problems list.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValidationIssue {
    /// Finding severity.
    pub severity: ValidationSeverity,
    /// Affected record, or `None` for a document setting.
    pub stroke: Option<StrokeId>,
    /// Machine-readable category.
    pub code: ValidationCode,
    /// Concise user-facing explanation.
    pub message: String,
}

/// Error returned by explicit drag transaction operations.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TransactionError {
    /// A transaction is already active.
    AlreadyActive,
    /// No transaction is active.
    NotActive,
}

impl fmt::Display for TransactionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AlreadyActive => formatter.write_str("an editor transaction is already active"),
            Self::NotActive => formatter.write_str("no editor transaction is active"),
        }
    }
}

impl Error for TransactionError {}

/// Error returned by component insertion or decomposition.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ComponentError {
    /// No definition with this name exists in the active library.
    NotFound(String),
    /// The requested stroke does not exist or is not a component.
    NotAComponent(StrokeId),
    /// A cached or user-supplied component source is invalid.
    InvalidSource {
        /// Component name.
        name: String,
        /// Parse failure.
        source: ParseError,
    },
}

impl fmt::Display for ComponentError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFound(name) => write!(formatter, "component {name:?} was not found"),
            Self::NotAComponent(id) => {
                write!(formatter, "stroke {} is not a component", id.get())
            }
            Self::InvalidSource { name, source } => {
                write!(
                    formatter,
                    "component {name:?} has invalid KAGE data: {source}"
                )
            }
        }
    }
}

impl Error for ComponentError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::InvalidSource { source, .. } => Some(source),
            Self::NotFound(_) | Self::NotAComponent(_) => None,
        }
    }
}

/// Serializable editor state captured by history entries.
#[derive(Clone, Debug, PartialEq)]
struct Snapshot {
    /// Ordered KAGE records.
    strokes: Vec<Stroke>,
    /// Selected record identities.
    selection: BTreeSet<StrokeId>,
    /// Applied transform activity.
    transforms: Vec<TransformRecord>,
    /// Next stable identity counter.
    next_id: u64,
}

/// One undo or redo stack entry.
#[derive(Clone, Debug)]
struct HistoryEntry {
    /// User-facing operation label.
    label: String,
    /// State restored by the operation.
    snapshot: Snapshot,
}

/// State held while a pointer gesture groups multiple edits.
#[derive(Clone, Debug)]
struct PendingTransaction {
    /// User-facing operation label.
    label: String,
    /// State before the first gesture edit.
    before: Snapshot,
}

/// Stateful, UI-agnostic KAGE glyph editor model.
///
/// All geometry mutations go through methods so the model can preserve stable
/// IDs, selection, and a bounded 30-step undo history. Selection changes by
/// themselves are intentionally not undoable.
#[derive(Clone, Debug)]
pub struct EditorModel {
    /// Ordered back-to-front KAGE records.
    strokes: Vec<Stroke>,
    /// Current record selection.
    selection: BTreeSet<StrokeId>,
    /// View and editor settings.
    settings: EditorSettings,
    /// Local searchable component library.
    library: ComponentLibrary,
    /// Internal application pasteboard.
    pasteboard: Vec<Stroke>,
    /// Old states, newest at the back.
    undo: VecDeque<HistoryEntry>,
    /// States made available by undo, newest at the back.
    redo: VecDeque<HistoryEntry>,
    /// Optional in-progress pointer gesture transaction.
    transaction: Option<PendingTransaction>,
    /// Recent transform activity retained in snapshots.
    transforms: Vec<TransformRecord>,
    /// Next stable record identity.
    next_id: u64,
    /// Monotonic value used by GPUI callers to detect changes.
    revision: u64,
}

impl Default for EditorModel {
    fn default() -> Self {
        Self::new()
    }
}

impl EditorModel {
    /// Creates an empty editor with an empty component library.
    #[must_use]
    pub fn new() -> Self {
        Self {
            strokes: Vec::new(),
            selection: BTreeSet::new(),
            settings: EditorSettings::default(),
            library: ComponentLibrary::new(),
            pasteboard: Vec::new(),
            undo: VecDeque::new(),
            redo: VecDeque::new(),
            transaction: None,
            transforms: Vec::new(),
            next_id: 1,
            revision: 0,
        }
    }

    /// Creates a populated model showing `GlyphWiki`'s seven-stroke “永”.
    #[must_use]
    pub fn demo() -> Self {
        Self::from_kage(demo_source()).unwrap_or_default()
    }

    /// Creates a model by parsing KAGE data.
    ///
    /// # Errors
    ///
    /// Returns [`ParseError`] if any source record is invalid.
    pub fn from_kage(source: &str) -> Result<Self, ParseError> {
        let parsed = parse_kage(source)?;
        let mut model = Self::new();
        for stroke in parsed {
            model.push_assigned(stroke);
        }
        Ok(model)
    }

    /// Replaces the document with parsed KAGE data as one undoable action.
    ///
    /// # Errors
    ///
    /// Returns [`ParseError`] without changing the model when parsing fails.
    pub fn load_kage(&mut self, source: &str) -> Result<(), ParseError> {
        let parsed = parse_kage(source)?;
        self.edit("Load KAGE", move |model| {
            model.strokes.clear();
            model.selection.clear();
            model.transforms.clear();
            for stroke in parsed {
                model.push_assigned(stroke);
            }
        });
        Ok(())
    }

    /// Serializes the current ordered records to KAGE data.
    #[must_use]
    pub fn to_kage(&self) -> String {
        serialize_kage(&self.strokes)
    }

    /// Recursively expands available type-99 records for renderer consumption.
    ///
    /// The pinned Rust KAGE engine drops type-0 operations found inside a
    /// component while performing its own expansion. Flattening through the
    /// editor's decomposition rules preserves those records in their original
    /// order, along with KAGE stretch and ordered-frame mapping. Missing,
    /// malformed, cyclic, or excessively deep components remain as type-99
    /// records so the renderer can safely ignore them.
    #[must_use]
    pub(crate) fn flattened_render_source(&self, source: &str) -> String {
        const MAX_COMPONENT_EXPANSIONS: usize = 256;
        const MAX_RENDER_RECORDS: usize = 16_384;

        let Ok(mut expanded) = Self::from_kage(source) else {
            return source.to_owned();
        };
        expanded.library = self.library.clone();

        let mut expansions = 0;
        let mut index = 0;
        while index < expanded.strokes.len()
            && expansions < MAX_COMPONENT_EXPANSIONS
            && expanded.strokes.len() <= MAX_RENDER_RECORDS
        {
            if expanded.strokes[index].kind != StrokeKind::Component {
                index += 1;
                continue;
            }

            let id = expanded.strokes[index].id;
            let Ok(children) = expanded.prepare_decomposed_children(id) else {
                index += 1;
                continue;
            };
            if children.is_empty()
                || expanded.strokes.len() - 1 + children.len() > MAX_RENDER_RECORDS
            {
                index += 1;
                continue;
            }

            expanded.install_decomposed_children(id, children);
            expansions += 1;
            // Deliberately revisit this index: the first replacement can itself
            // be a component, and component order is significant to type 0.
        }

        expanded.to_kage()
    }

    /// Serializes the main 200-unit glyph while omitting pasteboard records.
    ///
    /// Records whose complete bounding box begins at or beyond `x = 200` are
    /// editor staging data. They stay in [`Self::to_kage`] so the wide native
    /// canvas can render and manipulate them, but must not leave the editor as
    /// finished glyph data.
    #[must_use]
    pub fn to_export_kage(&self) -> String {
        let records = self
            .strokes
            .iter()
            .filter(|stroke| {
                stroke
                    .bounds()
                    .is_none_or(|bounds| bounds.min.x < DESIGN_SIZE)
            })
            .cloned()
            .collect::<Vec<_>>();
        serialize_kage(&records)
    }

    /// Returns records in back-to-front order.
    #[must_use]
    pub fn strokes(&self) -> &[Stroke] {
        &self.strokes
    }

    /// Returns a record by stable identity.
    #[must_use]
    pub fn stroke(&self, id: StrokeId) -> Option<&Stroke> {
        self.strokes.iter().find(|stroke| stroke.id == id)
    }

    /// Returns the selected identities in stable numeric order.
    #[must_use]
    pub const fn selection(&self) -> &BTreeSet<StrokeId> {
        &self.selection
    }

    /// Iterates selected records in document order.
    pub fn selected_strokes(&self) -> impl Iterator<Item = &Stroke> {
        self.strokes
            .iter()
            .filter(|stroke| self.selection.contains(&stroke.id))
    }

    /// Returns whether a record is selected.
    #[must_use]
    pub fn is_selected(&self, id: StrokeId) -> bool {
        self.selection.contains(&id)
    }

    /// Returns persistent editor settings.
    #[must_use]
    pub const fn settings(&self) -> &EditorSettings {
        &self.settings
    }

    /// Returns the searchable component library.
    #[must_use]
    pub const fn component_library(&self) -> &ComponentLibrary {
        &self.library
    }

    /// Returns a selected component's `-10..=10` metadata-backed stretch value.
    #[must_use]
    pub fn component_stretch_value(&self, id: StrokeId) -> Option<i32> {
        let component = self.stroke(id)?.component()?;
        let guide = self.library.get(component.name())?.stretch_guide()?;
        Some(guide.value(component.stretch()))
    }

    /// Returns applied transform activity, oldest first.
    #[must_use]
    pub fn transform_records(&self) -> &[TransformRecord] {
        &self.transforms
    }

    /// Returns the model revision, incremented after every visible change.
    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    /// Replaces display settings without consuming glyph undo history.
    pub fn set_settings(&mut self, settings: EditorSettings) {
        if self.settings != settings {
            self.settings = settings;
            self.revision = self.revision.wrapping_add(1);
        }
    }

    /// Replaces the component library without affecting glyph history.
    pub fn set_component_library(&mut self, library: ComponentLibrary) {
        if self.library != library {
            self.library = library;
            self.revision = self.revision.wrapping_add(1);
        }
    }

    /// Snaps a pointer coordinate according to current grid settings.
    #[must_use]
    pub fn snap_point(&self, point: Point) -> Point {
        self.settings.grid.snap_point(point)
    }

    /// Snaps freehand samples to nearby endpoints and axis-aligned stroke runs.
    ///
    /// Grid snapping is applied first. Geometry snapping then uses a
    /// screen-independent KAGE-unit tolerance supplied by the caller.
    #[must_use]
    pub fn snap_freehand_point(&self, point: Point, tolerance: f32) -> Point {
        let point = self.snap_point(point);
        let tolerance = tolerance.max(0.0);
        if let Some(endpoint) = self
            .strokes
            .iter()
            .filter(|stroke| stroke.kind.is_path())
            .flat_map(|stroke| [stroke.points.first(), stroke.points.last()])
            .flatten()
            .filter_map(|endpoint| {
                let distance = endpoint.distance(point);
                (distance <= tolerance).then_some((*endpoint, distance))
            })
            .min_by(|left, right| left.1.total_cmp(&right.1))
            .map(|(endpoint, _)| endpoint)
        {
            return endpoint;
        }

        let mut snapped = point;
        let mut closest_x = tolerance;
        let mut closest_y = tolerance;
        for (start, end) in self.strokes.iter().flat_map(straight_snap_segments) {
            if (start.x - end.x).abs() <= f32::EPSILON
                && point.y >= start.y.min(end.y) - tolerance
                && point.y <= start.y.max(end.y) + tolerance
            {
                let distance = (point.x - start.x).abs();
                if distance <= closest_x {
                    closest_x = distance;
                    snapped.x = start.x;
                }
            }
            if (start.y - end.y).abs() <= f32::EPSILON
                && point.x >= start.x.min(end.x) - tolerance
                && point.x <= start.x.max(end.x) + tolerance
            {
                let distance = (point.y - start.y).abs();
                if distance <= closest_y {
                    closest_y = distance;
                    snapped.y = start.y;
                }
            }
        }
        quantize_point(snapped)
    }

    /// Returns the union bounds of every record.
    #[must_use]
    pub fn bounds(&self) -> Option<Rect> {
        bounds_for(self.strokes.iter())
    }

    /// Returns the union bounds of selected records.
    #[must_use]
    pub fn selection_bounds(&self) -> Option<Rect> {
        bounds_for(self.selected_strokes())
    }

    /// Returns whether the current selection can be transformed as geometry.
    ///
    /// A type-99 component stores only an axis-aligned frame, while type-0 and
    /// type-9 records describe engine operations rather than ordinary path
    /// geometry. Rotating their two frame corners would encode negative scale
    /// or a broken operation frame, not a rotation. Paths whose visible ink is
    /// already changed by a later type-0 record are likewise kept read-only
    /// because their raw control points no longer describe what is painted.
    #[must_use]
    pub fn can_affine_transform_selection(&self) -> bool {
        !self.selection.is_empty()
            && self.strokes.iter().enumerate().all(|(index, stroke)| {
                if !self.selection.contains(&stroke.id) {
                    return true;
                }
                stroke.kind.is_path()
                    && !self.strokes[index + 1..]
                        .iter()
                        .any(|later| later.kage_transform().is_some())
            })
    }

    /// Finds the topmost skeleton/component frame under a pointer.
    #[must_use]
    pub fn hit_test(&self, point: Point, tolerance: f32) -> Option<Hit> {
        self.strokes.iter().rev().find_map(|stroke| {
            let distance = stroke.distance_to(point);
            (distance <= tolerance).then_some(Hit {
                stroke: stroke.id,
                distance,
            })
        })
    }

    /// Finds the topmost control point under a pointer.
    #[must_use]
    pub fn hit_control_point(&self, point: Point, tolerance: f32) -> Option<ControlPointRef> {
        let tolerance_squared = tolerance * tolerance;
        self.strokes.iter().rev().find_map(|stroke| {
            stroke
                .points
                .iter()
                .enumerate()
                .rev()
                .find(|(_, candidate)| candidate.distance_squared(point) <= tolerance_squared)
                .map(|(point, _)| ControlPointRef {
                    stroke: stroke.id,
                    point,
                })
        })
    }

    /// Returns every point connected to `origin` within a tolerance.
    #[must_use]
    pub fn connected_points(
        &self,
        origin: ControlPointRef,
        _tolerance: f32,
    ) -> Vec<ControlPointRef> {
        let Some(origin_stroke) = self.stroke(origin.stroke) else {
            return Vec::new();
        };
        if origin_stroke.points.get(origin.point).is_none() {
            return Vec::new();
        }
        let Some(anchor) = connection_anchor(origin_stroke, origin.point) else {
            return vec![origin];
        };
        let mut connected = self
            .strokes
            .iter()
            .flat_map(|stroke| {
                stroke
                    .points
                    .iter()
                    .enumerate()
                    .filter(move |(index, _)| {
                        stroke.id == origin.stroke && *index == origin.point
                            || connection_anchor(stroke, *index)
                                .is_some_and(|candidate| connections_match(anchor, candidate))
                    })
                    .map(move |(point, _)| ControlPointRef {
                        stroke: stroke.id,
                        point,
                    })
            })
            .collect::<Vec<_>>();
        connected.sort_unstable_by_key(|point| (point.stroke, point.point));
        connected
    }

    /// Applies a click-like selection operation.
    ///
    /// Returns `false` when the identity does not exist or membership did not
    /// change.
    pub fn select(&mut self, id: StrokeId, mode: SelectionMode) -> bool {
        if self.stroke(id).is_none() {
            return false;
        }
        let before = self.selection.clone();
        match mode {
            SelectionMode::Replace => {
                self.selection.clear();
                self.selection.insert(id);
            }
            SelectionMode::Add => {
                self.selection.insert(id);
            }
            SelectionMode::Toggle => {
                if !self.selection.remove(&id) {
                    self.selection.insert(id);
                }
            }
            SelectionMode::Remove => {
                self.selection.remove(&id);
            }
        }
        self.finish_selection_change(&before)
    }

    /// Clears the current selection.
    pub fn clear_selection(&mut self) -> bool {
        if self.selection.is_empty() {
            return false;
        }
        self.selection.clear();
        self.revision = self.revision.wrapping_add(1);
        true
    }

    /// Selects every record.
    pub fn select_all(&mut self) -> bool {
        let before = self.selection.clone();
        self.selection = self.strokes.iter().map(|stroke| stroke.id).collect();
        self.finish_selection_change(&before)
    }

    /// Inverts selection membership for every record.
    pub fn invert_selection(&mut self) -> bool {
        let before = self.selection.clone();
        self.selection = self
            .strokes
            .iter()
            .filter_map(|stroke| (!before.contains(&stroke.id)).then_some(stroke.id))
            .collect();
        self.finish_selection_change(&before)
    }

    /// Selects records whose control bounds intersect a marquee.
    pub fn select_in_rect(&mut self, rect: Rect, mode: SelectionMode) -> bool {
        let hits: Vec<StrokeId> = self
            .strokes
            .iter()
            .filter(|stroke| {
                stroke
                    .bounds()
                    .is_some_and(|bounds| bounds.intersects(rect))
            })
            .map(|stroke| stroke.id)
            .collect();
        let before = self.selection.clone();
        if mode == SelectionMode::Replace {
            self.selection.clear();
        }
        for id in hits {
            match mode {
                SelectionMode::Replace | SelectionMode::Add => {
                    self.selection.insert(id);
                }
                SelectionMode::Toggle => {
                    if !self.selection.remove(&id) {
                        self.selection.insert(id);
                    }
                }
                SelectionMode::Remove => {
                    self.selection.remove(&id);
                }
            }
        }
        self.finish_selection_change(&before)
    }

    /// Replaces selection with the preceding record, wrapping at the start.
    pub fn select_previous(&mut self) -> Option<StrokeId> {
        self.select_adjacent(OrderDirection::Backward)
    }

    /// Replaces selection with the following record, wrapping at the end.
    pub fn select_next(&mut self) -> Option<StrokeId> {
        self.select_adjacent(OrderDirection::Forward)
    }

    /// Begins a grouped transaction, typically on pointer-down.
    ///
    /// # Errors
    ///
    /// Returns [`TransactionError::AlreadyActive`] if a transaction is already
    /// open.
    pub fn begin_transaction(&mut self, label: impl Into<String>) -> Result<(), TransactionError> {
        if self.transaction.is_some() {
            return Err(TransactionError::AlreadyActive);
        }
        self.transaction = Some(PendingTransaction {
            label: label.into(),
            before: self.snapshot(),
        });
        Ok(())
    }

    /// Commits a grouped transaction as at most one undo step.
    ///
    /// Returns whether the transaction actually changed document state.
    ///
    /// # Errors
    ///
    /// Returns [`TransactionError::NotActive`] when no transaction is open.
    pub fn commit_transaction(&mut self) -> Result<bool, TransactionError> {
        let pending = self.transaction.take().ok_or(TransactionError::NotActive)?;
        if pending.before == self.snapshot() {
            return Ok(false);
        }
        self.push_undo(pending.label, pending.before);
        Ok(true)
    }

    /// Restores the state captured at transaction start.
    ///
    /// # Errors
    ///
    /// Returns [`TransactionError::NotActive`] when no transaction is open.
    pub fn cancel_transaction(&mut self) -> Result<(), TransactionError> {
        let pending = self.transaction.take().ok_or(TransactionError::NotActive)?;
        self.restore(pending.before);
        self.revision = self.revision.wrapping_add(1);
        Ok(())
    }

    /// Returns whether a grouped transaction is active.
    #[must_use]
    pub const fn transaction_active(&self) -> bool {
        self.transaction.is_some()
    }

    /// Returns whether undo is currently available.
    #[must_use]
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty() && self.transaction.is_none()
    }

    /// Returns whether redo is currently available.
    #[must_use]
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty() && self.transaction.is_none()
    }

    /// Returns the next undo operation label.
    #[must_use]
    pub fn undo_label(&self) -> Option<&str> {
        self.undo.back().map(|entry| entry.label.as_str())
    }

    /// Returns the next redo operation label.
    #[must_use]
    pub fn redo_label(&self) -> Option<&str> {
        self.redo.back().map(|entry| entry.label.as_str())
    }

    /// Restores the preceding committed document state.
    pub fn undo(&mut self) -> bool {
        if self.transaction.is_some() {
            return false;
        }
        let Some(entry) = self.undo.pop_back() else {
            return false;
        };
        let current = self.snapshot();
        self.redo.push_back(HistoryEntry {
            label: entry.label,
            snapshot: current,
        });
        trim_history(&mut self.redo);
        self.restore(entry.snapshot);
        self.selection.clear();
        self.revision = self.revision.wrapping_add(1);
        true
    }

    /// Reapplies the most recently undone document state.
    pub fn redo(&mut self) -> bool {
        if self.transaction.is_some() {
            return false;
        }
        let Some(entry) = self.redo.pop_back() else {
            return false;
        };
        let current = self.snapshot();
        self.undo.push_back(HistoryEntry {
            label: entry.label,
            snapshot: current,
        });
        trim_history(&mut self.undo);
        self.restore(entry.snapshot);
        self.selection.clear();
        self.revision = self.revision.wrapping_add(1);
        true
    }

    /// Inserts a record, assigns an identity, and selects only the new record.
    pub fn insert_stroke(&mut self, stroke: Stroke) -> StrokeId {
        self.edit("Insert stroke", |model| {
            let id = model.push_assigned(stroke);
            model.selection.clear();
            model.selection.insert(id);
            id
        })
    }

    /// Inserts a serializable KAGE type 0 transform record.
    ///
    /// KAGE renderers apply the transform to previously emitted outlines fully
    /// contained by `frame`, so z-order placement of this record is meaningful.
    pub fn insert_kage_transform(&mut self, transform: KageTransform, frame: Rect) -> StrokeId {
        let (head, tail) = transform.parameters();
        self.insert_stroke(Stroke::new(
            StrokeKind::Metadata,
            head,
            tail,
            vec![frame.min, frame.max],
        ))
    }

    /// Deletes all selected records as one action.
    pub fn delete_selected(&mut self) -> usize {
        if self.selection.is_empty() {
            return 0;
        }
        self.edit("Delete", |model| {
            let before = model.strokes.len();
            model
                .strokes
                .retain(|stroke| !model.selection.contains(&stroke.id));
            model.selection.clear();
            before - model.strokes.len()
        })
    }

    /// Copies selected records to the model-local pasteboard at `(230, 20)`.
    ///
    /// This operation does not touch glyph history or the operating system
    /// clipboard.
    pub fn copy_selected(&mut self) -> usize {
        let mut copied: Vec<Stroke> = self.selected_strokes().cloned().collect();
        if let Some(bounds) = bounds_for(copied.iter()) {
            let delta = Point::new(230.0 - bounds.min.x, 20.0 - bounds.min.y);
            for stroke in &mut copied {
                for point in &mut stroke.points {
                    *point = quantize_point(point.offset(delta));
                }
            }
        }
        self.pasteboard = copied;
        self.pasteboard.len()
    }

    /// Cuts selected records while preserving their original coordinates.
    pub fn cut_selected(&mut self) -> usize {
        self.pasteboard = self.selected_strokes().cloned().collect();
        let count = self.pasteboard.len();
        if count != 0 {
            self.delete_selected();
        }
        count
    }

    /// Pastes internal records at their stored pasteboard coordinates.
    pub fn paste(&mut self) -> Vec<StrokeId> {
        if self.pasteboard.is_empty() {
            return Vec::new();
        }
        let pasted = self.pasteboard.clone();
        self.edit("Paste", move |model| {
            model.selection.clear();
            let mut ids = Vec::with_capacity(pasted.len());
            for stroke in pasted {
                let id = model.push_assigned(stroke);
                model.selection.insert(id);
                ids.push(id);
            }
            ids
        })
    }

    /// Returns the number of records on the internal pasteboard.
    #[must_use]
    pub fn pasteboard_len(&self) -> usize {
        self.pasteboard.len()
    }

    /// Swaps the z-order positions of two records.
    pub fn swap_order(&mut self, first: StrokeId, second: StrokeId) -> bool {
        let Some(first_index) = self.index_of(first) else {
            return false;
        };
        let Some(second_index) = self.index_of(second) else {
            return false;
        };
        if first_index == second_index {
            return false;
        }
        self.edit("Reorder", |model| {
            model.strokes.swap(first_index, second_index);
        });
        true
    }

    /// Moves selected records one step backward or forward without splitting
    /// contiguous selected groups.
    pub fn move_selected_in_order(&mut self, direction: OrderDirection) -> bool {
        if self.selection.is_empty() {
            return false;
        }
        let before = self.strokes.clone();
        self.edit("Reorder", |model| match direction {
            OrderDirection::Backward => {
                for index in 1..model.strokes.len() {
                    let selected = model.selection.contains(&model.strokes[index].id);
                    let prior_selected = model
                        .selection
                        .contains(&model.strokes[index.saturating_sub(1)].id);
                    if selected && !prior_selected {
                        model.strokes.swap(index, index - 1);
                    }
                }
            }
            OrderDirection::Forward => {
                for index in (0..model.strokes.len().saturating_sub(1)).rev() {
                    let selected = model.selection.contains(&model.strokes[index].id);
                    let next_selected = model.selection.contains(&model.strokes[index + 1].id);
                    if selected && !next_selected {
                        model.strokes.swap(index, index + 1);
                    }
                }
            }
        });
        before != self.strokes
    }

    /// Moves selected records, optionally carrying coincident unselected points.
    pub fn move_selected(&mut self, delta: Point, propagate_connected: bool) -> bool {
        let delta = quantize_point(delta);
        if self.selection.is_empty() || (delta.x == 0.0 && delta.y == 0.0) {
            return false;
        }
        let anchors = self.selected_connection_anchors();
        let selection = self.selection.clone();
        self.edit("Move", |model| {
            for stroke in &mut model.strokes {
                let selected = selection.contains(&stroke.id);
                for index in 0..stroke.points.len() {
                    let connected = !selected
                        && propagate_connected
                        && connection_anchor(stroke, index).is_some_and(|candidate| {
                            anchors
                                .iter()
                                .any(|anchor| connections_match(*anchor, candidate))
                        });
                    if selected || connected {
                        stroke.points[index] = quantize_point(stroke.points[index].offset(delta));
                    }
                }
            }
            model.record_transform(
                "Move",
                AffineTransform::translation(delta),
                selection.iter().copied().collect(),
            );
        });
        true
    }

    /// Moves one control point and optionally every coincident point.
    pub fn move_control_point(
        &mut self,
        control: ControlPointRef,
        destination: Point,
        propagate_connected: bool,
    ) -> bool {
        let Some(origin_stroke) = self.stroke(control.stroke) else {
            return false;
        };
        let Some(origin) = origin_stroke.points.get(control.point).copied() else {
            return false;
        };
        let origin_anchor = connection_anchor(origin_stroke, control.point);
        let destination = quantize_point(destination);
        if origin == destination {
            return false;
        }
        self.edit("Move control point", |model| {
            for stroke in &mut model.strokes {
                for index in 0..stroke.points.len() {
                    let is_target = stroke.id == control.stroke && index == control.point;
                    let is_connected = propagate_connected
                        && stroke.id != control.stroke
                        && origin_anchor.is_some_and(|anchor| {
                            connection_anchor(stroke, index)
                                .is_some_and(|candidate| connections_match(anchor, candidate))
                        });
                    if is_target || is_connected {
                        stroke.points[index] = quantize_point(stroke.points[index].offset(
                            Point::new(destination.x - origin.x, destination.y - origin.y),
                        ));
                    }
                }
            }
        });
        true
    }

    /// Resizes the selected records into `target` and can carry attached points.
    pub fn resize_selected(&mut self, target: Rect, propagate_connected: bool) -> bool {
        let Some(source) = self.selection_bounds() else {
            return false;
        };
        if source == target {
            return false;
        }
        self.edit("Resize", |model| {
            model.apply_selection_resize(source, target, propagate_connected);
        });
        true
    }

    /// Previews a selection resize from the active transaction's start state.
    ///
    /// Pointer drags can publish many intermediate targets. Rebuilding each
    /// preview from the transaction snapshot prevents integral KAGE rounding
    /// from accumulating differently according to the number of pointer-move
    /// events. Without an active transaction this behaves like
    /// [`Self::resize_selected`].
    pub fn preview_resize_selected(&mut self, target: Rect, propagate_connected: bool) -> bool {
        let Some(baseline) = self
            .transaction
            .as_ref()
            .map(|transaction| transaction.before.clone())
        else {
            return self.resize_selected(target, propagate_connected);
        };
        let Some(source) = bounds_for(
            baseline
                .strokes
                .iter()
                .filter(|stroke| baseline.selection.contains(&stroke.id)),
        ) else {
            return false;
        };
        let current = self.snapshot();
        self.edit("Resize", move |model| {
            model.restore(baseline);
            if source != target {
                model.apply_selection_resize(source, target, propagate_connected);
            }
        });
        current != self.snapshot()
    }

    /// Applies one resize from an explicit, unmodified selection rectangle.
    fn apply_selection_resize(&mut self, source: Rect, target: Rect, propagate_connected: bool) {
        let scale_x = if source.width().abs() <= f32::EPSILON {
            1.0
        } else {
            target.width() / source.width()
        };
        let scale_y = if source.height().abs() <= f32::EPSILON {
            1.0
        } else {
            target.height() / source.height()
        };
        let transform = AffineTransform {
            m11: scale_x,
            m12: 0.0,
            m21: 0.0,
            m22: scale_y,
            tx: target.min.x - source.min.x * scale_x,
            ty: target.min.y - source.min.y * scale_y,
        };
        let anchors = self.selected_connection_anchors();
        let selection = self.selection.clone();
        for stroke in &mut self.strokes {
            let selected = selection.contains(&stroke.id);
            for index in 0..stroke.points.len() {
                let connected = !selected
                    && propagate_connected
                    && connection_anchor(stroke, index).is_some_and(|candidate| {
                        anchors
                            .iter()
                            .any(|anchor| connections_match(*anchor, candidate))
                    });
                if selected || connected {
                    stroke.points[index] = quantize_point(transform.apply(stroke.points[index]));
                }
            }
        }
        self.record_transform("Resize", transform, selection.iter().copied().collect());
    }

    /// Replaces the diagonal of one type 0, 9, or 99 frame record.
    ///
    /// Components and type-9 extension records preserve their ordered points,
    /// allowing a crossed component frame to encode negative scale. KAGE
    /// type-0 operations, however, select polygons with an ordered rectangle;
    /// a crossed type-0 frame selects nothing, so those points are normalized.
    pub fn resize_frame_record(&mut self, id: StrokeId, first: Point, second: Point) -> bool {
        let Some(index) = self.index_of(id) else {
            return false;
        };
        if !matches!(
            self.strokes[index].kind,
            StrokeKind::Metadata | StrokeKind::Transform | StrokeKind::Component
        ) || self.strokes[index].points.len() != 2
        {
            return false;
        }
        let first = quantize_point(first);
        let second = quantize_point(second);
        let points = if self.strokes[index].kind == StrokeKind::Metadata {
            let frame = Rect::new(first, second);
            [frame.min, frame.max]
        } else {
            [first, second]
        };
        if self.strokes[index].points == points {
            return false;
        }
        self.edit("Resize frame", |model| {
            model.strokes[index].points = points.to_vec();
        });
        true
    }

    /// Applies an arbitrary affine transform to selected records.
    pub fn transform_selected(
        &mut self,
        label: impl Into<String>,
        transform: AffineTransform,
        propagate_connected: bool,
    ) -> bool {
        if !self.can_affine_transform_selection() {
            return false;
        }
        let label = label.into();
        let anchors = self.selected_connection_anchors();
        let selection = self.selection.clone();
        let history_label = label.clone();
        self.edit(&history_label, move |model| {
            for stroke in &mut model.strokes {
                let selected = selection.contains(&stroke.id);
                let mut geometry_changed = false;
                for index in 0..stroke.points.len() {
                    let connected = !selected
                        && propagate_connected
                        && connection_anchor(stroke, index).is_some_and(|candidate| {
                            anchors
                                .iter()
                                .any(|anchor| connections_match(*anchor, candidate))
                        });
                    if selected || connected {
                        stroke.points[index] =
                            quantize_point(transform.apply(stroke.points[index]));
                        geometry_changed = true;
                    }
                }
                if geometry_changed
                    && stroke.kind.is_path()
                    && !valid_style_combination(stroke)
                    && let Some((head, tail)) = stroke.kind.default_style()
                {
                    stroke.head = head;
                    stroke.tail = tail;
                }
            }
            model.record_transform(label, transform, selection.iter().copied().collect());
        });
        true
    }

    /// Updates a record's KAGE type and resamples its geometry as necessary.
    pub fn set_stroke_kind(&mut self, id: StrokeId, kind: StrokeKind) -> bool {
        let Some(index) = self.index_of(id) else {
            return false;
        };
        if self.strokes[index].kind == kind {
            return false;
        }
        self.edit("Change stroke type", |model| {
            let stroke = &mut model.strokes[index];
            stroke.points = convert_point_count(&stroke.points, kind.point_count());
            stroke.kind = kind;
            if let Some(default) = kind.head_shapes().first().copied()
                && !kind.head_shapes().contains(&stroke.head)
            {
                stroke.head = default;
            }
            if let Some(default) = kind.tail_shapes().first().copied()
                && !kind.tail_shapes().contains(&stroke.tail)
            {
                stroke.tail = default;
            }
            if let Some((head, tail)) = kind.default_style()
                && !valid_style_combination(stroke)
            {
                stroke.head = head;
                stroke.tail = tail;
            }
            if kind == StrokeKind::Component {
                stroke
                    .component
                    .get_or_insert_with(|| ComponentRef::new("component"));
            } else {
                stroke.component = None;
            }
        });
        true
    }

    /// Changes the type of every selected record.
    pub fn set_selected_kind(&mut self, kind: StrokeKind) -> usize {
        let ids: Vec<StrokeId> = self.selection.iter().copied().collect();
        if ids.is_empty() {
            return 0;
        }
        let mut changed = 0;
        let transaction_started = self.begin_implicit_transaction("Change stroke type");
        for id in ids {
            changed += usize::from(self.set_stroke_kind(id, kind));
        }
        self.finish_implicit_transaction(transaction_started);
        changed
    }

    /// Updates one record's KAGE head-shape value.
    pub fn set_stroke_head(&mut self, id: StrokeId, head: i32) -> bool {
        let Some(index) = self.index_of(id) else {
            return false;
        };
        if self.strokes[index].head == head {
            return false;
        }
        self.edit("Change stroke head", |model| {
            let stroke = &mut model.strokes[index];
            stroke.head = head;
            if let Some(stretch) = stroke
                .component
                .as_mut()
                .and_then(|component| component.stretch.as_mut())
            {
                stretch.destination_x = head;
            }
        });
        true
    }

    /// Updates one record's KAGE tail-shape value.
    pub fn set_stroke_tail(&mut self, id: StrokeId, tail: i32) -> bool {
        let Some(index) = self.index_of(id) else {
            return false;
        };
        if self.strokes[index].tail == tail {
            return false;
        }
        self.edit("Change stroke tail", |model| {
            let stroke = &mut model.strokes[index];
            stroke.tail = tail;
            if let Some(stretch) = stroke
                .component
                .as_mut()
                .and_then(|component| component.stretch.as_mut())
            {
                stretch.destination_y = tail;
            }
        });
        true
    }

    /// Updates head and tail values for every selected record as one action.
    pub fn set_selected_style(&mut self, head: i32, tail: i32) -> usize {
        if self.selection.is_empty() {
            return 0;
        }
        let selection = self.selection.clone();
        self.edit("Change stroke style", |model| {
            let mut changed = 0;
            for stroke in &mut model.strokes {
                if selection.contains(&stroke.id) && (stroke.head != head || stroke.tail != tail) {
                    stroke.head = head;
                    stroke.tail = tail;
                    if let Some(stretch) = stroke
                        .component
                        .as_mut()
                        .and_then(|component| component.stretch.as_mut())
                    {
                        stretch.destination_x = head;
                        stretch.destination_y = tail;
                    }
                    changed += 1;
                }
            }
            changed
        })
    }

    /// Inserts a type 99 component reference using the supplied frame.
    ///
    /// # Errors
    ///
    /// Returns [`ComponentError::NotFound`] when the active library has no
    /// matching definition.
    pub fn insert_component(
        &mut self,
        name: &str,
        frame: Rect,
    ) -> Result<StrokeId, ComponentError> {
        if self.library.get(name).is_none() {
            return Err(ComponentError::NotFound(name.to_owned()));
        }
        let mut stroke = Stroke::new(StrokeKind::Component, 0, 0, vec![frame.min, frame.max]);
        stroke.component = Some(ComponentRef::new(name));
        Ok(self.insert_stroke(stroke))
    }

    /// Updates a component frame and optional engine stretch parameters.
    ///
    /// # Errors
    ///
    /// Returns [`ComponentError::NotAComponent`] when `id` is missing or does
    /// not refer to a type 99 record.
    pub fn stretch_component(
        &mut self,
        id: StrokeId,
        frame: Rect,
        stretch: Option<ComponentStretch>,
    ) -> Result<bool, ComponentError> {
        let frame = Rect::new(quantize_point(frame.min), quantize_point(frame.max));
        let Some(index) = self.index_of(id) else {
            return Err(ComponentError::NotAComponent(id));
        };
        if self.strokes[index].kind != StrokeKind::Component {
            return Err(ComponentError::NotAComponent(id));
        }
        let frame_points = if self.strokes[index].bounds() == Some(frame) {
            self.strokes[index].points.clone()
        } else {
            vec![frame.min, frame.max]
        };
        let unchanged = self.strokes[index].points == frame_points
            && self.strokes[index]
                .component
                .as_ref()
                .is_some_and(|component| component.stretch == stretch);
        if unchanged {
            return Ok(false);
        }
        self.edit("Stretch component", move |model| {
            let stroke = &mut model.strokes[index];
            stroke.points = frame_points;
            if let Some(component) = &mut stroke.component {
                component.stretch = stretch;
                component.stretch_reserved = 0;
                component.stretch_fields_present = stretch.is_some();
            }
            if let Some(stretch) = stretch {
                stroke.head = stretch.destination_x;
                stroke.tail = stretch.destination_y;
            } else {
                stroke.head = 0;
                stroke.tail = 0;
            }
        });
        Ok(true)
    }

    /// Updates one component using its source-declared stretch guide.
    ///
    /// # Errors
    ///
    /// Returns [`ComponentError::NotAComponent`] for a missing/non-component
    /// record and [`ComponentError::NotFound`] when its source is unavailable.
    pub fn set_component_stretch_value(
        &mut self,
        id: StrokeId,
        value: i32,
    ) -> Result<bool, ComponentError> {
        let stroke = self
            .stroke(id)
            .filter(|stroke| stroke.kind == StrokeKind::Component)
            .ok_or(ComponentError::NotAComponent(id))?;
        let component = stroke
            .component
            .as_ref()
            .ok_or(ComponentError::NotAComponent(id))?;
        let name = component.name.clone();
        let frame = stroke.bounds().ok_or(ComponentError::NotAComponent(id))?;
        let definition = self
            .library
            .get(&name)
            .ok_or_else(|| ComponentError::NotFound(name.clone()))?;
        let Some(guide) = definition.stretch_guide() else {
            return Ok(false);
        };
        self.stretch_component(id, frame, Some(guide.stretch(value)))
    }

    /// Expands one component into editable child records in the same z-order
    /// position.
    ///
    /// Child coordinates first receive KAGE's piecewise source/destination
    /// pivot stretch and are then mapped from the conventional 0–200 component
    /// design space into the reference frame.
    ///
    /// # Errors
    ///
    /// Returns [`ComponentError`] when the identity is not a component, its
    /// definition is unavailable, or its source cannot be parsed.
    pub fn decompose_component(&mut self, id: StrokeId) -> Result<Vec<StrokeId>, ComponentError> {
        let children = self.prepare_decomposed_children(id)?;
        Ok(self.edit("Decompose component", move |model| {
            model.install_decomposed_children(id, children)
        }))
    }

    /// Expands every selected type-99 record as one atomic undoable action.
    ///
    /// Selected ordinary strokes remain selected alongside all newly created
    /// child records.
    ///
    /// # Errors
    ///
    /// Returns the first component lookup or parse error without changing the
    /// document.
    pub fn decompose_selected_components(&mut self) -> Result<Vec<StrokeId>, ComponentError> {
        let component_ids = self
            .strokes
            .iter()
            .filter(|stroke| {
                stroke.kind == StrokeKind::Component && self.selection.contains(&stroke.id)
            })
            .map(|stroke| stroke.id)
            .collect::<Vec<_>>();
        let mut prepared = Vec::with_capacity(component_ids.len());
        for id in component_ids {
            prepared.push((id, self.prepare_decomposed_children(id)?));
        }
        if prepared.is_empty() {
            return Ok(Vec::new());
        }
        Ok(self.edit("Decompose components", move |model| {
            let mut ids = Vec::new();
            for (id, children) in prepared {
                ids.extend(model.install_decomposed_children(id, children));
            }
            ids
        }))
    }

    /// Produces transformed child records without mutating the document.
    fn prepare_decomposed_children(&self, id: StrokeId) -> Result<Vec<Stroke>, ComponentError> {
        let Some(stroke) = self.stroke(id) else {
            return Err(ComponentError::NotAComponent(id));
        };
        let Some(component) = stroke.component.as_ref() else {
            return Err(ComponentError::NotAComponent(id));
        };
        let name = component.name.clone();
        let stretch = component.stretch;
        let definition = self
            .library
            .get(&name)
            .ok_or_else(|| ComponentError::NotFound(name.clone()))?;
        let mut children =
            parse_kage(&definition.source).map_err(|source| ComponentError::InvalidSource {
                name: name.clone(),
                source,
            })?;
        let (frame_first, frame_second) = match stroke.points.as_slice() {
            [first, second] => (*first, *second),
            _ => (Point::new(0.0, 0.0), Point::new(DESIGN_SIZE, DESIGN_SIZE)),
        };
        if let Some(stretch) = stretch {
            apply_component_stretch(&mut children, stretch);
            let normalized = normalize_component_stretch(Some(stretch));
            if normalized.0 != normalized.2 - 200 || normalized.1 != normalized.3 {
                compose_nested_component_stretch(&mut children, normalized);
            }
        }
        for child in &mut children {
            for point in &mut child.points {
                *point = quantize_point(Point::new(
                    (frame_second.x - frame_first.x).mul_add(point.x / DESIGN_SIZE, frame_first.x),
                    (frame_second.y - frame_first.y).mul_add(point.y / DESIGN_SIZE, frame_first.y),
                ));
            }
        }
        Ok(children)
    }

    /// Replaces one component with already prepared child records.
    fn install_decomposed_children(
        &mut self,
        id: StrokeId,
        children: Vec<Stroke>,
    ) -> Vec<StrokeId> {
        let index = self
            .index_of(id)
            .expect("a prepared component must remain in the document");
        self.strokes.remove(index);
        self.selection.remove(&id);
        let mut ids = Vec::with_capacity(children.len());
        for (offset, mut child) in children.into_iter().enumerate() {
            let child_id = self.assign_id(&mut child);
            self.strokes.insert(index + offset, child);
            self.selection.insert(child_id);
            ids.push(child_id);
        }
        ids
    }

    /// Validates stroke structure, common style families, coordinates, and grid
    /// settings without mutating the model.
    #[must_use]
    pub fn validate(&self) -> Vec<ValidationIssue> {
        let mut issues = Vec::new();
        for stroke in &self.strokes {
            let expected = stroke.kind.point_count();
            if stroke.points.len() != expected {
                issues.push(issue(
                    ValidationSeverity::Error,
                    Some(stroke.id),
                    ValidationCode::PointCount,
                    format!(
                        "type {} needs {expected} control points, found {}",
                        stroke.kind.code(),
                        stroke.points.len()
                    ),
                ));
            }
            if stroke
                .points
                .iter()
                .any(|point| !point.x.is_finite() || !point.y.is_finite())
            {
                issues.push(issue(
                    ValidationSeverity::Error,
                    Some(stroke.id),
                    ValidationCode::NonFiniteCoordinate,
                    "record contains a non-finite coordinate".to_owned(),
                ));
            }
            if stroke.kind.is_path()
                && stroke
                    .points
                    .windows(2)
                    .all(|pair| points_near(pair[0], pair[1]))
            {
                issues.push(issue(
                    ValidationSeverity::Warning,
                    Some(stroke.id),
                    ValidationCode::DegeneratePath,
                    "stroke has no appreciable length".to_owned(),
                ));
            }
            if stroke.kind == StrokeKind::Component {
                match stroke.component.as_ref() {
                    None => issues.push(issue(
                        ValidationSeverity::Error,
                        Some(stroke.id),
                        ValidationCode::MissingComponent,
                        "type 99 record has no component name".to_owned(),
                    )),
                    Some(component) if component.name.trim().is_empty() => {
                        issues.push(issue(
                            ValidationSeverity::Error,
                            Some(stroke.id),
                            ValidationCode::MissingComponent,
                            "type 99 record has no component name".to_owned(),
                        ));
                    }
                    Some(component) if self.library.get(&component.name).is_none() => {
                        issues.push(issue(
                            ValidationSeverity::Error,
                            Some(stroke.id),
                            ValidationCode::MissingComponent,
                            format!("component definition {} is unavailable", component.name),
                        ));
                    }
                    Some(_) => {}
                }
            } else if stroke.component.is_some() {
                issues.push(issue(
                    ValidationSeverity::Error,
                    Some(stroke.id),
                    ValidationCode::UnexpectedComponent,
                    "non-component record carries component metadata".to_owned(),
                ));
            }
            let known_head_for_kind = stroke.kind.head_shapes().contains(&stroke.head);
            let known_tail_for_kind = stroke.kind.tail_shapes().contains(&stroke.tail);
            if stroke.kind.is_path() && !known_head_for_kind {
                issues.push(issue(
                    ValidationSeverity::Warning,
                    Some(stroke.id),
                    ValidationCode::UnknownHead,
                    format!(
                        "head shape {} is unavailable for type {}",
                        stroke.head,
                        stroke.kind.code()
                    ),
                ));
            }
            if stroke.kind.is_path() && !known_tail_for_kind {
                issues.push(issue(
                    ValidationSeverity::Warning,
                    Some(stroke.id),
                    ValidationCode::UnknownTail,
                    format!(
                        "tail shape {} is unavailable for type {}",
                        stroke.tail,
                        stroke.kind.code()
                    ),
                ));
            }
            if stroke.kind.is_path()
                && known_head_for_kind
                && known_tail_for_kind
                && !valid_style_combination(stroke)
            {
                issues.push(issue(
                    ValidationSeverity::Warning,
                    Some(stroke.id),
                    ValidationCode::InvalidStyleCombination,
                    format!(
                        "head {} and tail {} are incompatible with type {} geometry",
                        stroke.head,
                        stroke.tail,
                        stroke.kind.code()
                    ),
                ));
            }
        }
        if !self.settings.grid.origin_x.is_finite()
            || !self.settings.grid.origin_y.is_finite()
            || !self.settings.grid.spacing_x.is_finite()
            || self.settings.grid.spacing_x <= 0.0
            || !self.settings.grid.spacing_y.is_finite()
            || self.settings.grid.spacing_y <= 0.0
            || self.settings.grid.subdivisions == 0
        {
            issues.push(issue(
                ValidationSeverity::Error,
                None,
                ValidationCode::InvalidGrid,
                "grid origins must be finite; spacing and subdivisions must be positive".to_owned(),
            ));
        }
        issues
    }

    /// Applies an attached finishing hook or inserts a recognized freehand stroke.
    pub fn insert_gesture(&mut self, samples: &[Point]) -> Option<RecognizedStroke> {
        if let Some(hook) = recognize_hook_gesture(&self.strokes, samples) {
            self.edit("Add hook", |model| {
                let index = model
                    .index_of(hook.target)
                    .expect("the recognized hook target must still exist");
                let stroke = &mut model.strokes[index];
                stroke.head = hook.head;
                stroke.tail = hook.tail;
            });
            let stroke = self
                .stroke(hook.target)
                .expect("the recognized hook target must still exist")
                .clone();
            return Some(RecognizedStroke {
                kind: hook.kind,
                stroke,
                confidence: hook.confidence,
            });
        }

        let mut recognized = recognize_gesture(samples)?;
        snap_stroke_endpoints(self, &mut recognized.stroke, 10.0);
        align_near_axis(&mut recognized.stroke, 10.0);
        apply_freehand_connection_styles(&self.strokes, &mut recognized.stroke);
        self.insert_stroke(recognized.stroke.clone());
        Some(recognized)
    }

    /// Groups a multi-record helper only when no caller transaction is active.
    fn begin_implicit_transaction(&mut self, label: &str) -> bool {
        if self.transaction.is_none() {
            self.begin_transaction(label)
                .expect("the transaction was just checked as inactive");
            true
        } else {
            false
        }
    }

    /// Commits a helper-owned transaction, leaving caller transactions intact.
    fn finish_implicit_transaction(&mut self, started: bool) {
        if started {
            self.commit_transaction()
                .expect("the helper-owned transaction must remain active");
        }
    }

    /// Runs a mutation and records one undo state when not inside a transaction.
    fn edit<ResultValue>(
        &mut self,
        label: &str,
        operation: impl FnOnce(&mut Self) -> ResultValue,
    ) -> ResultValue {
        let before = self.snapshot();
        let result = operation(self);
        if before != self.snapshot() {
            self.revision = self.revision.wrapping_add(1);
            if self.transaction.is_none() {
                self.push_undo(label.to_owned(), before);
            }
        }
        result
    }

    /// Captures state included in undo and redo.
    fn snapshot(&self) -> Snapshot {
        Snapshot {
            strokes: self.strokes.clone(),
            selection: self.selection.clone(),
            transforms: self.transforms.clone(),
            next_id: self.next_id,
        }
    }

    /// Restores state from undo, redo, or transaction cancellation.
    fn restore(&mut self, snapshot: Snapshot) {
        self.strokes = snapshot.strokes;
        self.selection = snapshot.selection;
        self.transforms = snapshot.transforms;
        self.next_id = snapshot.next_id;
    }

    /// Adds an old state to undo and invalidates redo.
    fn push_undo(&mut self, label: String, snapshot: Snapshot) {
        self.undo.push_back(HistoryEntry { label, snapshot });
        trim_history(&mut self.undo);
        self.redo.clear();
    }

    /// Assigns and appends a record without creating an independent history step.
    fn push_assigned(&mut self, mut stroke: Stroke) -> StrokeId {
        let id = self.assign_id(&mut stroke);
        self.strokes.push(stroke);
        id
    }

    /// Replaces a record's identity with the next editor-local value.
    fn assign_id(&mut self, stroke: &mut Stroke) -> StrokeId {
        let id = StrokeId(self.next_id);
        self.next_id = self.next_id.saturating_add(1);
        stroke.id = id;
        id
    }

    /// Finds a record's current z-order index.
    fn index_of(&self, id: StrokeId) -> Option<usize> {
        self.strokes.iter().position(|stroke| stroke.id == id)
    }

    /// Collects selected endpoints that participate in KAGE connection pairs.
    fn selected_connection_anchors(&self) -> Vec<ConnectionAnchor> {
        self.selected_strokes()
            .flat_map(|stroke| {
                stroke
                    .points
                    .iter()
                    .enumerate()
                    .filter_map(|(index, _)| connection_anchor(stroke, index))
            })
            .collect()
    }

    /// Applies a selection change revision when the set changed.
    fn finish_selection_change(&mut self, before: &BTreeSet<StrokeId>) -> bool {
        let changed = before != &self.selection;
        if changed {
            self.revision = self.revision.wrapping_add(1);
        }
        changed
    }

    /// Selects one neighbor according to document order.
    fn select_adjacent(&mut self, direction: OrderDirection) -> Option<StrokeId> {
        if self.strokes.is_empty() {
            return None;
        }
        let current = match direction {
            OrderDirection::Backward => self
                .strokes
                .iter()
                .position(|stroke| self.selection.contains(&stroke.id))
                .unwrap_or(0),
            OrderDirection::Forward => self
                .strokes
                .iter()
                .rposition(|stroke| self.selection.contains(&stroke.id))
                .unwrap_or(self.strokes.len() - 1),
        };
        let next = match direction {
            OrderDirection::Backward => current.checked_sub(1).unwrap_or(self.strokes.len() - 1),
            OrderDirection::Forward => (current + 1) % self.strokes.len(),
        };
        let id = self.strokes[next].id;
        let before = self.selection.clone();
        self.selection.clear();
        self.selection.insert(id);
        self.finish_selection_change(&before);
        Some(id)
    }

    /// Appends a bounded transform activity record.
    fn record_transform(
        &mut self,
        label: impl Into<String>,
        transform: AffineTransform,
        targets: Vec<StrokeId>,
    ) {
        self.transforms.push(TransformRecord {
            label: label.into(),
            transform,
            targets,
        });
        if self.transforms.len() > HISTORY_LIMIT {
            self.transforms.remove(0);
        }
    }
}

/// Carries an active parent stretch into neutral nested component references.
fn compose_nested_component_stretch(
    children: &mut [Stroke],
    (source_x, source_y, destination_x, destination_y): (i32, i32, i32, i32),
) {
    for child in children {
        if child.kind != StrokeKind::Component {
            continue;
        }
        let Some(component) = child.component.as_mut() else {
            continue;
        };
        let normalized_child = normalize_component_stretch(component.stretch);
        if normalized_child.0 != normalized_child.2 - 200
            || normalized_child.1 != normalized_child.3
        {
            continue;
        }
        let [first, second] = child.points.as_slice() else {
            continue;
        };
        if (first.x - second.x).abs() <= f32::EPSILON || (first.y - second.y).abs() <= f32::EPSILON
        {
            continue;
        }
        let reverse_x = |value: f32| (value - first.x) / (second.x - first.x) * DESIGN_SIZE;
        let reverse_y = |value: f32| (value - first.y) / (second.y - first.y) * DESIGN_SIZE;
        let nested = ComponentStretch::new(
            js_round_i32(reverse_x((destination_x - 100) as f32) + 100.0),
            js_round_i32(reverse_y((destination_y + 100) as f32) - 100.0),
            js_round_i32(reverse_x((source_x + 100) as f32) - 100.0),
            js_round_i32(reverse_y((source_y + 100) as f32) - 100.0),
        );
        component.stretch = Some(nested);
        component.stretch_reserved = 0;
        component.stretch_fields_present = true;
        child.head = nested.destination_x;
        child.tail = nested.destination_y;
    }
}

/// Returns the canonical seven-stroke “永” used by [`EditorModel::demo`].
fn demo_source() -> &'static str {
    concat!(
        "2:7:8:66:13:102:23:120:43$",
        "1:0:2:34:60:100:60$",
        "1:22:4:100:60:100:183$",
        "1:0:2:16:93:71:93$",
        "2:22:7:71:93:61:145:13:174$",
        "2:0:7:171:64:152:81:119:104$",
        "2:7:0:104:67:121:135:180:166",
    )
}

/// Computes union bounds over borrowed records.
fn bounds_for<'a>(strokes: impl IntoIterator<Item = &'a Stroke>) -> Option<Rect> {
    strokes
        .into_iter()
        .filter_map(Stroke::bounds)
        .reduce(Rect::union)
}

/// Keeps a history deque within the documented limit.
fn trim_history(history: &mut VecDeque<HistoryEntry>) {
    while history.len() > HISTORY_LIMIT {
        history.pop_front();
    }
}

/// Rounds one document point to the coordinate semantics used by KAGE.
fn quantize_point(point: Point) -> Point {
    let quantize = |value: f32| {
        if value.is_finite() {
            js_round_i32(value) as f32
        } else {
            value
        }
    };
    Point::new(quantize(point.x), quantize(point.y))
}

/// Reproduces JavaScript `Math.round` and saturates to KAGE's integer range.
fn js_round_i32(value: f32) -> i32 {
    (value + 0.5)
        .floor()
        .clamp(i32::MIN as f32, i32::MAX as f32) as i32
}

/// One endpoint role in KAGE's four legal head/tail connection pairs.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ConnectionRole {
    /// Type-1 start with head 2.
    LeftStemStart,
    /// Any path start with head 12.
    UpperLeftStart,
    /// Type-1 end with tail 13, 313, or 413.
    LowerLeftEnd,
    /// Type-1 end with tail 2.
    RightStemEnd,
    /// Any path start with head 22 or 27.
    UpperRightStart,
    /// Type-1 end with tail 23 or 24.
    LowerRightEnd,
}

/// Coordinate and semantic role captured before a connected edit.
#[derive(Clone, Copy, Debug, PartialEq)]
struct ConnectionAnchor {
    /// Exact quantized document coordinate.
    point: Point,
    /// Style-dependent endpoint category.
    role: ConnectionRole,
}

/// Classifies one endpoint according to KAGE's connection style matrix.
fn connection_role(stroke: &Stroke, point: usize) -> Option<ConnectionRole> {
    if !stroke.kind.is_path() || stroke.points.is_empty() {
        return None;
    }
    if point == 0 {
        return match (stroke.kind, stroke.head) {
            (StrokeKind::Line, 2) => Some(ConnectionRole::LeftStemStart),
            (_, 12) => Some(ConnectionRole::UpperLeftStart),
            (_, 22 | 27) => Some(ConnectionRole::UpperRightStart),
            _ => None,
        };
    }
    if point + 1 != stroke.points.len() || stroke.kind != StrokeKind::Line {
        return None;
    }
    match stroke.tail {
        2 => Some(ConnectionRole::RightStemEnd),
        13 | 313 | 413 => Some(ConnectionRole::LowerLeftEnd),
        23 | 24 => Some(ConnectionRole::LowerRightEnd),
        _ => None,
    }
}

/// Captures a legal endpoint and its exact coordinate.
fn connection_anchor(stroke: &Stroke, point: usize) -> Option<ConnectionAnchor> {
    Some(ConnectionAnchor {
        point: *stroke.points.get(point)?,
        role: connection_role(stroke, point)?,
    })
}

/// Returns whether two endpoint anchors form one legal, undirected pair.
fn connections_match(first: ConnectionAnchor, second: ConnectionAnchor) -> bool {
    if first.point != second.point {
        return false;
    }
    matches!(
        (first.role, second.role),
        (
            ConnectionRole::LeftStemStart,
            ConnectionRole::UpperLeftStart | ConnectionRole::LowerLeftEnd
        ) | (
            ConnectionRole::UpperLeftStart | ConnectionRole::LowerLeftEnd,
            ConnectionRole::LeftStemStart
        ) | (
            ConnectionRole::RightStemEnd,
            ConnectionRole::UpperRightStart | ConnectionRole::LowerRightEnd
        ) | (
            ConnectionRole::UpperRightStart | ConnectionRole::LowerRightEnd,
            ConnectionRole::RightStemEnd
        )
    )
}

/// Returns whether two points are indistinguishable for degeneracy warnings.
fn points_near(first: Point, second: Point) -> bool {
    first.distance_squared(second) <= CONNECTION_TOLERANCE * CONNECTION_TOLERANCE
}

/// Returns straight runs eligible for freehand axis snapping.
fn straight_snap_segments(stroke: &Stroke) -> Vec<(Point, Point)> {
    let count = match stroke.kind {
        StrokeKind::Line | StrokeKind::Sweep => 1,
        StrokeKind::Bend | StrokeKind::Corner => 2,
        _ => 0,
    };
    stroke
        .points
        .windows(2)
        .take(count)
        .map(|pair| (pair[0], pair[1]))
        .collect()
}

/// Straightens near-horizontal/vertical runs after gesture recognition.
fn align_near_axis(stroke: &mut Stroke, tolerance: f32) {
    let segment_count = match stroke.kind {
        StrokeKind::Line | StrokeKind::Sweep => 1,
        StrokeKind::Bend | StrokeKind::Corner => 2,
        _ => 0,
    };
    for index in 0..segment_count.min(stroke.points.len().saturating_sub(1)) {
        let start = stroke.points[index];
        let end = &mut stroke.points[index + 1];
        if (end.x - start.x).abs() <= tolerance {
            end.x = start.x;
        }
        if (end.y - start.y).abs() <= tolerance {
            end.y = start.y;
        }
        *end = quantize_point(*end);
    }
}

/// Snaps only the on-curve endpoints of a newly recognized freehand record.
fn snap_stroke_endpoints(model: &EditorModel, stroke: &mut Stroke, tolerance: f32) {
    if !stroke.kind.is_path() || stroke.points.is_empty() {
        return;
    }
    let last = stroke.points.len() - 1;
    stroke.points[0] = model.snap_freehand_point(stroke.points[0], tolerance);
    stroke.points[last] = model.snap_freehand_point(stroke.points[last], tolerance);
}

/// Chooses compatible KAGE head/tail shapes after freehand endpoint snapping.
fn apply_freehand_connection_styles(existing: &[Stroke], stroke: &mut Stroke) {
    if !stroke.kind.is_path() || stroke.points.is_empty() {
        return;
    }
    let start = stroke.points[0];
    let start_head = existing.iter().rev().find_map(|candidate| {
        candidate
            .points
            .iter()
            .enumerate()
            .filter_map(|(index, point)| {
                if point.distance_squared(start) <= f32::EPSILON {
                    connection_role(candidate, index)
                } else {
                    None
                }
            })
            .find_map(|role| match role {
                ConnectionRole::LeftStemStart => Some(12),
                ConnectionRole::UpperLeftStart | ConnectionRole::LowerLeftEnd
                    if stroke.kind == StrokeKind::Line =>
                {
                    Some(2)
                }
                ConnectionRole::RightStemEnd => Some(22),
                _ => None,
            })
    });
    if let Some(head) = start_head.filter(|head| stroke.kind.head_shapes().contains(head)) {
        let previous = stroke.head;
        stroke.head = head;
        if !valid_style_combination(stroke) {
            stroke.head = previous;
        }
    }

    if stroke.kind != StrokeKind::Line {
        return;
    }
    let end = *stroke.points.last().expect("a path has an endpoint");
    let end_tail = existing.iter().rev().find_map(|candidate| {
        candidate
            .points
            .iter()
            .enumerate()
            .filter_map(|(index, point)| {
                if point.distance_squared(end) <= f32::EPSILON {
                    connection_role(candidate, index)
                } else {
                    None
                }
            })
            .find_map(|role| match role {
                ConnectionRole::LeftStemStart => Some(13),
                ConnectionRole::UpperRightStart | ConnectionRole::LowerRightEnd => Some(2),
                ConnectionRole::RightStemEnd => Some(23),
                _ => None,
            })
    });
    if let Some(tail) = end_tail.filter(|tail| stroke.kind.tail_shapes().contains(tail)) {
        let previous = stroke.tail;
        stroke.tail = tail;
        if !valid_style_combination(stroke) {
            stroke.tail = previous;
        }
    }
}

/// Builds a validation issue without repeating field labels.
fn issue(
    severity: ValidationSeverity,
    stroke: Option<StrokeId>,
    code: ValidationCode,
    message: String,
) -> ValidationIssue {
    ValidationIssue {
        severity,
        stroke,
        code,
        message,
    }
}

/// Checks the engine-supported head/tail pair and the type-1 direction rule.
fn valid_style_combination(stroke: &Stroke) -> bool {
    let pair_supported = match stroke.kind {
        StrokeKind::Line => match stroke.head {
            0 => true,
            2 => matches!(stroke.tail, 0 | 2),
            32 | 22 => stroke.tail != 2,
            12 => !matches!(stroke.tail, 2 | 4),
            _ => false,
        },
        StrokeKind::Curve | StrokeKind::Bezier => match stroke.head {
            0 => matches!(stroke.tail, 7 | 5),
            32 | 22 => matches!(stroke.tail, 7 | 4 | 5),
            12 => stroke.tail == 7,
            7 => matches!(stroke.tail, 0 | 8 | 4),
            27 => stroke.tail == 0,
            _ => false,
        },
        StrokeKind::Bend
        | StrokeKind::Corner
        | StrokeKind::Metadata
        | StrokeKind::Transform
        | StrokeKind::Component => true,
        StrokeKind::Sweep => stroke.tail == 7,
    };
    if !pair_supported || stroke.kind != StrokeKind::Line {
        return pair_supported;
    }
    let [start, end] = stroke.points.as_slice() else {
        return true;
    };
    let vertical = if (start.y - end.y).abs() <= f32::EPSILON {
        (start.x - end.x).abs() <= f32::EPSILON
    } else {
        end.x - start.x <= (start.y - end.y).abs()
    };
    if vertical {
        stroke.head != 2 && stroke.tail != 2
    } else {
        matches!(stroke.head, 0 | 2) && matches!(stroke.tail, 0 | 2)
    }
}

/// Applies KAGE's two-segment component stretch before frame scaling.
#[allow(clippy::cast_precision_loss)]
fn apply_component_stretch(strokes: &mut [Stroke], stretch: ComponentStretch) {
    let mut destination_x = stretch.destination_x as f32;
    let destination_y = stretch.destination_y as f32;
    let mut source_x = stretch.source_x as f32;
    let mut source_y = stretch.source_y as f32;
    if destination_x == 0.0 && destination_y == 0.0 {
        return;
    }
    if destination_x > 100.0 {
        destination_x -= 200.0;
    } else {
        source_x = 0.0;
        source_y = 0.0;
    }
    let Some(bounds) = bounds_for(strokes.iter()) else {
        return;
    };
    for stroke in strokes {
        for point in &mut stroke.points {
            point.x =
                stretch_coordinate(destination_x, source_x, point.x, bounds.min.x, bounds.max.x);
            point.y =
                stretch_coordinate(destination_y, source_y, point.y, bounds.min.y, bounds.max.y);
        }
    }
}

/// Maps one coordinate through KAGE's piecewise-linear pivot function.
fn stretch_coordinate(destination: f32, source: f32, value: f32, min: f32, max: f32) -> f32 {
    let (input_min, input_max, output_min, output_max) = if value < source + 100.0 {
        (min, source + 100.0, min, destination + 100.0)
    } else {
        (source + 100.0, max, destination + 100.0, max)
    };
    let denominator = input_max - input_min;
    if denominator.abs() <= f32::EPSILON {
        return value;
    }
    ((value - input_min) / denominator * (output_max - output_min) + output_min).floor()
}

/// Converts KAGE control-point arity without changing equivalent curve shape.
///
/// The two-to-four and three-to-four cases use the canonical line/quadratic
/// Bézier degree elevation formulas. The reverse three-point conversion uses
/// the midpoint of the cubic controls, matching KAGE Editor's record switcher.
fn convert_point_count(points: &[Point], count: usize) -> Vec<Point> {
    if points.len() == count {
        return points.to_vec();
    }
    let converted = match points {
        [start, end] if count == 3 => vec![*start, js_round_point(start.lerp(*end, 0.5)), *end],
        [start, end] if count == 4 => vec![
            *start,
            js_round_point(start.lerp(*end, 1.0 / 3.0)),
            js_round_point(start.lerp(*end, 2.0 / 3.0)),
            *end,
        ],
        [start, _, end] if count == 2 => vec![*start, *end],
        [start, middle, end] if count == 4 => vec![
            *start,
            js_round_point(Point::new(
                (start.x + 2.0 * middle.x) / 3.0,
                (start.y + 2.0 * middle.y) / 3.0,
            )),
            js_round_point(Point::new(
                (end.x + 2.0 * middle.x) / 3.0,
                (end.y + 2.0 * middle.y) / 3.0,
            )),
            *end,
        ],
        [start, _, _, end] if count == 2 => vec![*start, *end],
        [start, first, second, end] if count == 3 => {
            vec![*start, js_round_point(first.lerp(*second, 0.5)), *end]
        }
        _ => resample_points(points, count),
    };
    converted.into_iter().map(quantize_point).collect()
}

/// Reproduces JavaScript's `Math.round` for negative half-unit coordinates.
fn js_round_point(point: Point) -> Point {
    Point::new(js_round_i32(point.x) as f32, js_round_i32(point.y) as f32)
}

/// Resamples malformed extension geometry as a defensive editing fallback.
fn resample_points(polyline: &[Point], count: usize) -> Vec<Point> {
    if count == 0 || polyline.is_empty() {
        return Vec::new();
    }
    if count == 1 {
        return vec![polyline[0]];
    }
    let lengths: Vec<f32> = polyline
        .windows(2)
        .map(|pair| pair[0].distance(pair[1]))
        .collect();
    let total: f32 = lengths.iter().sum();
    if total <= f32::EPSILON {
        return vec![polyline[0]; count];
    }
    (0..count)
        .map(|index| {
            let target = total * ratio(index, count - 1);
            point_at_distance(polyline, &lengths, target)
        })
        .collect()
}

/// Finds a linearly interpolated point at an accumulated polyline distance.
fn point_at_distance(polyline: &[Point], lengths: &[f32], target: f32) -> Point {
    let mut traversed = 0.0;
    for (index, length) in lengths.iter().copied().enumerate() {
        if traversed + length >= target && length > f32::EPSILON {
            return polyline[index].lerp(polyline[index + 1], (target - traversed) / length);
        }
        traversed += length;
    }
    *polyline.last().unwrap_or(&Point::default())
}

/// High-level shape inferred from a freehand pointer gesture.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GestureKind {
    /// Approximately straight gesture mapped to KAGE type 1.
    Line,
    /// Smooth curved gesture mapped to KAGE type 2.
    Curve,
    /// Gesture with one sharp corner mapped to KAGE type 3.
    Bend,
    /// Long descending left sweep mapped to KAGE type 7.
    Sweep,
    /// Short leftward finishing hook applied to the preceding path.
    LeftHook,
    /// Short rising-right finishing hook applied to the preceding curve.
    RightHook,
    /// Short upward finishing hook applied to the preceding bend or corner.
    UpHook,
}

/// Result of recognizing a freehand pointer gesture.
#[derive(Clone, Debug, PartialEq)]
pub struct RecognizedStroke {
    /// Inferred semantic shape.
    kind: GestureKind,
    /// Resulting inserted stroke or the existing stroke modified by a hook.
    stroke: Stroke,
    /// Heuristic zero-to-one recognition confidence.
    confidence: f32,
}

impl RecognizedStroke {
    /// Returns the inferred semantic shape.
    #[must_use]
    pub const fn kind(&self) -> GestureKind {
        self.kind
    }

    /// Returns the inserted or hook-modified KAGE stroke.
    #[must_use]
    pub const fn stroke(&self) -> &Stroke {
        &self.stroke
    }

    /// Returns a heuristic confidence between zero and one.
    #[must_use]
    pub const fn confidence(&self) -> f32 {
        self.confidence
    }
}

/// A short gesture resolved against the endpoint of an existing path.
#[derive(Clone, Copy, Debug, PartialEq)]
struct RecognizedHook {
    /// Existing path record to update.
    target: StrokeId,
    /// Direction-specific hook classification.
    kind: GestureKind,
    /// Resulting KAGE head style.
    head: i32,
    /// Resulting KAGE tail style.
    tail: i32,
    /// Heuristic confidence based on length and attachment distance.
    confidence: f32,
}

/// Recognizes a short hook attached to the most recent path endpoint.
fn recognize_hook_gesture(strokes: &[Stroke], samples: &[Point]) -> Option<RecognizedHook> {
    let filtered = filter_gesture_samples(samples);
    let start = *filtered.first()?;
    let end = *filtered.last()?;
    if filtered.len() < 2 {
        return None;
    }

    let chord = start.distance(end);
    if chord >= HOOK_CHORD_LIMIT {
        return None;
    }
    let stroke = strokes.iter().rfind(|stroke| stroke.kind.is_path())?;
    let endpoint = *stroke.points.last()?;
    let attachment = start.distance(endpoint);
    if attachment >= HOOK_ATTACHMENT_LIMIT {
        return None;
    }

    let delta = Point::new(end.x - start.x, end.y - start.y);
    let mut head = stroke.head;
    let (kind, tail) = if matches!(
        stroke.kind,
        StrokeKind::Line | StrokeKind::Curve | StrokeKind::Bezier
    ) && delta.x < 0.0
    {
        if head == 27 {
            head = 22;
        }
        (GestureKind::LeftHook, 4)
    } else if matches!(stroke.kind, StrokeKind::Curve | StrokeKind::Bezier)
        && delta.x >= 0.0
        && delta.y < 0.0
    {
        head = match head {
            7 => 0,
            27 => 22,
            _ => head,
        };
        (GestureKind::RightHook, 5)
    } else if matches!(stroke.kind, StrokeKind::Bend | StrokeKind::Corner) && delta.y < 0.0 {
        (GestureKind::UpHook, 5)
    } else {
        return None;
    };
    let confidence = ((1.0 - chord / HOOK_CHORD_LIMIT)
        .mul_add(0.5, (1.0 - attachment / HOOK_ATTACHMENT_LIMIT) * 0.5))
    .clamp(0.55, 1.0);

    Some(RecognizedHook {
        target: stroke.id,
        kind,
        head,
        tail,
        confidence,
    })
}

/// Recognizes a freehand gesture as a line, smooth curve, bend, or descending sweep.
///
/// The recognizer is deterministic and deliberately lightweight: it filters
/// duplicate pointer samples, uses path/chord deviation for lines, detects a
/// localized direction discontinuity for bends, recognizes the conventional
/// long vertical-left KAGE sweep, and fits a quadratic control point to all
/// remaining gestures. It is suitable for live desktop input and does not
/// attempt handwriting recognition.
#[must_use]
pub fn recognize_gesture(samples: &[Point]) -> Option<RecognizedStroke> {
    let filtered = filter_gesture_samples(samples);
    if filtered.len() < 2 {
        return None;
    }
    let start = filtered[0];
    let end = *filtered.last()?;
    let chord = start.distance(end);
    let path_length: f32 = filtered
        .windows(2)
        .map(|pair| pair[0].distance(pair[1]))
        .sum();
    if path_length < 3.0 {
        return None;
    }
    let max_deviation = filtered
        .iter()
        .map(|point| distance_to_segment(*point, start, end))
        .fold(0.0, f32::max);
    let straight_tolerance = 1.5_f32.max(chord * 0.045);
    let detour = if chord <= f32::EPSILON {
        f32::INFINITY
    } else {
        path_length / chord
    };
    if max_deviation <= straight_tolerance && detour <= 1.1 {
        let confidence = (1.0 - max_deviation / (straight_tolerance * 2.0)).clamp(0.55, 1.0);
        return Some(RecognizedStroke {
            kind: GestureKind::Line,
            stroke: Stroke::line(start, end),
            confidence,
        });
    }

    let inverse_sample_count = ratio(1, filtered.len());
    let centroid = Point::new(
        filtered.iter().map(|point| point.x).sum::<f32>() * inverse_sample_count,
        filtered.iter().map(|point| point.y).sum::<f32>() * inverse_sample_count,
    );
    let midpoint = start.lerp(end, 0.5);
    let gesture_control = Point::new(
        midpoint.x + (centroid.x - midpoint.x) * 3.0,
        midpoint.y + (centroid.y - midpoint.y) * 3.0,
    );
    let delta = Point::new(end.x - start.x, end.y - start.y);
    let signed_deviation = if chord <= f32::EPSILON {
        0.0
    } else {
        (delta.x.mul_add(
            gesture_control.y,
            -delta.y * gesture_control.x + start.x * end.y - start.y * end.x,
        )) / chord
    };
    if delta.x < 0.0 && delta.y >= 50.0 && signed_deviation < 0.0 && -delta.x * 3.0 < delta.y {
        let first = Point::new(start.x, start.y + delta.y / 3.0);
        let second = Point::new(start.x, start.y + delta.y * 2.0 / 3.0);
        return Some(RecognizedStroke {
            kind: GestureKind::Sweep,
            stroke: Stroke::new(StrokeKind::Sweep, 0, 7, vec![start, first, second, end]),
            confidence: (delta.y / path_length).clamp(0.55, 0.96),
        });
    }

    let simplified = simplify_polyline(&filtered, 1.25);
    if let Some((corner, turn)) = sharp_corner(&simplified, path_length) {
        let confidence = (turn / std::f32::consts::FRAC_PI_2).clamp(0.55, 1.0);
        return Some(RecognizedStroke {
            kind: GestureKind::Bend,
            stroke: Stroke::bend(start, corner, end),
            confidence,
        });
    }

    let control = fit_quadratic_control(&filtered, start, end, path_length);
    let confidence = (max_deviation / (path_length * 0.25).max(1.0)).clamp(0.5, 0.95);
    Some(RecognizedStroke {
        kind: GestureKind::Curve,
        stroke: Stroke::curve(start, control, end),
        confidence,
    })
}

/// Drops non-finite and near-duplicate pointer samples.
fn filter_gesture_samples(samples: &[Point]) -> Vec<Point> {
    let mut filtered = Vec::with_capacity(samples.len());
    for sample in samples.iter().copied() {
        if !sample.x.is_finite() || !sample.y.is_finite() {
            continue;
        }
        if filtered
            .last()
            .is_none_or(|previous: &Point| previous.distance(sample) >= 0.5)
        {
            filtered.push(sample);
        }
    }
    filtered
}

/// Simplifies a pointer trace with the Ramer-Douglas-Peucker algorithm.
fn simplify_polyline(points: &[Point], epsilon: f32) -> Vec<Point> {
    if points.len() <= 2 {
        return points.to_vec();
    }
    let first = points[0];
    let last = *points.last().unwrap_or(&first);
    let (index, distance) = points[1..points.len() - 1]
        .iter()
        .enumerate()
        .map(|(index, point)| (index + 1, distance_to_segment(*point, first, last)))
        .max_by(|left, right| left.1.total_cmp(&right.1))
        .unwrap_or((0, 0.0));
    if distance <= epsilon {
        return vec![first, last];
    }
    let mut left = simplify_polyline(&points[..=index], epsilon);
    let right = simplify_polyline(&points[index..], epsilon);
    left.pop();
    left.extend(right);
    left
}

/// Locates a sufficiently sharp, well-supported interior corner.
fn sharp_corner(points: &[Point], total_length: f32) -> Option<(Point, f32)> {
    if points.len() < 3 {
        return None;
    }
    points[1..points.len() - 1]
        .iter()
        .enumerate()
        .filter_map(|(offset, point)| {
            let index = offset + 1;
            let before = points[index - 1];
            let after = points[index + 1];
            let incoming = Point::new(point.x - before.x, point.y - before.y);
            let outgoing = Point::new(after.x - point.x, after.y - point.y);
            let first_length = before.distance(*point);
            let second_length = point.distance(after);
            if first_length < total_length * 0.08 || second_length < total_length * 0.08 {
                return None;
            }
            let cosine = (incoming.x.mul_add(outgoing.x, incoming.y * outgoing.y)
                / (first_length * second_length))
                .clamp(-1.0, 1.0);
            let turn = cosine.acos();
            (turn >= 0.82).then_some((*point, turn))
        })
        .max_by(|left, right| left.1.total_cmp(&right.1))
}

/// Fits a quadratic Bézier control point by least squares over arc-length time.
fn fit_quadratic_control(samples: &[Point], start: Point, end: Point, total_length: f32) -> Point {
    let mut travelled = 0.0;
    let mut numerator_x = 0.0;
    let mut numerator_y = 0.0;
    let mut denominator = 0.0;
    for (index, sample) in samples.iter().copied().enumerate() {
        if index != 0 {
            travelled += samples[index - 1].distance(sample);
        }
        let t = (travelled / total_length).clamp(0.0, 1.0);
        let one_minus = 1.0 - t;
        let weight = 2.0 * one_minus * t;
        if weight <= f32::EPSILON {
            continue;
        }
        let base_x = one_minus * one_minus * start.x + t * t * end.x;
        let base_y = one_minus * one_minus * start.y + t * t * end.y;
        numerator_x += weight * (sample.x - base_x);
        numerator_y += weight * (sample.y - base_y);
        denominator += weight * weight;
    }
    if denominator <= f32::EPSILON {
        return start.lerp(end, 0.5);
    }
    Point::new(numerator_x / denominator, numerator_y / denominator)
}

/// Unit coverage for parsing, editing invariants, history, components, and
/// gesture recognition.
#[cfg(test)]
mod tests {
    use super::*;

    /// Returns a two-line document used by editing tests.
    fn two_lines() -> EditorModel {
        EditorModel::from_kage("1:0:0:10:10:50:10$1:0:0:50:10:50:60").expect("fixture should parse")
    }

    /// Creates a model whose component cache is populated explicitly for tests
    /// that exercise insertion and decomposition without network access.
    fn model_with_component_fixtures() -> EditorModel {
        let mut model = EditorModel::new();
        model.set_component_library(ComponentLibrary::builtin());
        model
    }

    /// Asserts approximate equality for geometry values.
    fn assert_close(actual: f32, expected: f32) {
        assert!(
            (actual - expected).abs() < 0.01,
            "expected {expected}, found {actual}"
        );
    }

    #[test]
    fn demo_uses_the_canonical_seven_record_yong() {
        let model = EditorModel::demo();
        assert_eq!(model.strokes().len(), 7);
        assert_eq!(model.to_kage(), demo_source());
        assert_eq!(model.to_export_kage(), demo_source());
        assert!(model.validate().is_empty());
        assert!(model.selection().is_empty());
        assert!(!model.can_undo());
    }

    /// Runtime models start without approximate bundled component data; the UI
    /// fills this cache from `GlyphWiki` instead.
    #[test]
    fn new_models_have_an_empty_component_library() {
        assert!(EditorModel::new().component_library().entries().is_empty());
        assert!(EditorModel::demo().component_library().entries().is_empty());
    }

    /// Every requested KAGE type parses and serializes in order.
    #[test]
    fn parses_and_serializes_all_supported_types() {
        let source = concat!(
            "0:99:1:10:20:100:120$",
            "1:0:0:0:0:200:0$",
            "2:7:8:0:0:100:40:200:100$",
            "3:0:0:0:0:100:0:100:100$",
            "4:0:0:0:0:80:20:100:100$",
            "6:0:0:0:0:40:0:160:200:200:200$",
            "7:32:7:100:20:100:80:100:150:20:180$",
            "9:1:0:0:0:200:200$",
            "99:150:0:5:6:190:194:u6728:0:-10:50:future"
        );
        let strokes = parse_kage(source).expect("all supported records should parse");
        assert_eq!(strokes.len(), 9);
        assert_eq!(strokes[0].kage_transform(), Some(KageTransform::Rotate90));
        assert_eq!(
            strokes.iter().map(Stroke::kind).collect::<Vec<_>>(),
            vec![
                StrokeKind::Metadata,
                StrokeKind::Line,
                StrokeKind::Curve,
                StrokeKind::Bend,
                StrokeKind::Corner,
                StrokeKind::Bezier,
                StrokeKind::Sweep,
                StrokeKind::Transform,
                StrokeKind::Component,
            ]
        );
        let serialized = serialize_kage(&strokes);
        assert!(serialized.contains("99:150:0:5:6:190:194:u6728:0:-10:50:future"));
        assert_eq!(
            strokes[8]
                .component()
                .expect("component metadata")
                .stretch(),
            Some(ComponentStretch::new(150, 0, -10, 50))
        );
        assert_eq!(
            parse_kage(&serialized).expect("round trip should parse"),
            strokes
        );
    }

    /// Import floors KAGE numbers, coerces malformed payloads, and skips
    /// unsupported record kinds without discarding surrounding valid records.
    #[test]
    fn parser_matches_kage_numeric_coercion_and_unknown_filtering() {
        let lines = "1:0:0:1:2:3:4\n2:0:0:1:2:3:4:5:6\r\n";
        assert_eq!(parse_kage(lines).expect("line input should parse").len(), 2);

        let parsed = parse_kage(concat!(
            "5:0:0:1:2$",
            "0:1:0:50:100:150:100$",
            "1:2.9:oops:10.9:-1.2:NaN:4.99$",
            "not-a-kind:0:0:1:2:3:4$",
            "1.5:0:0:1:2:3:4"
        ))
        .expect("bad payload values are coerced and unknown records are skipped");
        assert_eq!(serialize_kage(&parsed), "1:2:0:10:-2:0:4");

        let mut manually_created = EditorModel::new();
        manually_created.insert_stroke(Stroke::line(
            Point::new(f32::NAN, 0.0),
            Point::new(20.0, 20.0),
        ));
        assert!(
            manually_created
                .validate()
                .iter()
                .any(|issue| issue.code == ValidationCode::NonFiniteCoordinate)
        );
        assert!(matches!(
            parse_kage("99:0:0:0:0:200:200:"),
            Err(ParseError::MissingComponentName { .. })
        ));
    }

    /// Type 9 has exactly two geometry points; forward-compatible fields stay
    /// trailing data instead of being reinterpreted as additional coordinates.
    #[test]
    fn type_nine_uses_two_points_and_preserves_extensions() {
        let source = "9:1:2:0:0:200:200:future:extension";
        let strokes = parse_kage(source).expect("type 9 should parse");
        assert_eq!(strokes[0].points().len(), 2);
        assert_eq!(strokes[0].extra_fields, ["future", "extension"]);
        assert_eq!(serialize_kage(&strokes), source);

        let mut model = EditorModel::new();
        model.insert_stroke(Stroke::new(
            StrokeKind::Transform,
            0,
            0,
            vec![
                Point::new(0.0, 0.0),
                Point::new(100.0, 100.0),
                Point::new(200.0, 200.0),
            ],
        ));
        assert!(
            model
                .validate()
                .iter()
                .any(|issue| issue.code == ValidationCode::PointCount)
        );
    }

    /// Neutral type 99 records always emit the canonical three-field stretch
    /// suffix, whether imported from compact data or created by the editor.
    #[test]
    fn neutral_components_always_serialize_eleven_columns() {
        let compact = parse_kage("99:0:0:0:0:200:200:u53e3")
            .expect("compact component should remain import-compatible");
        let canonical = serialize_kage(&compact);
        assert_eq!(canonical, "99:0:0:0:0:200:200:u53e3:0:0:0");
        assert_eq!(canonical.split(':').count(), 11);

        let mut model = model_with_component_fixtures();
        model
            .insert_component(
                "u53e3",
                Rect::new(Point::new(0.0, 0.0), Point::new(200.0, 200.0)),
            )
            .expect("component test fixture");
        assert_eq!(model.to_kage(), canonical);
    }

    /// Imported and edited document geometry stays integral for engine parity.
    #[test]
    fn document_geometry_is_quantized_before_rendering() {
        let mut model = EditorModel::from_kage("1:0:0:10.49:20.5:90.51:80.49")
            .expect("fractional source should parse");
        assert_eq!(model.to_kage(), "1:0:0:10:20:90:80");
        let id = model.strokes()[0].id();
        model.select(id, SelectionMode::Replace);
        assert!(model.move_control_point(
            ControlPointRef {
                stroke: id,
                point: 0,
            },
            Point::new(11.6, 22.4),
            false,
        ));
        assert_eq!(model.to_kage(), "1:0:0:12:22:90:80");
        assert!(!model.to_kage().contains('.'));
    }

    /// Bounds and hit testing cover straight, curved, and component records.
    #[test]
    fn geometry_bounds_and_hit_tests_are_canvas_ready() {
        let line = Stroke::line(Point::new(10.0, 10.0), Point::new(90.0, 10.0));
        assert!(line.hit_test(Point::new(50.0, 12.0), 3.0));
        assert!(!line.hit_test(Point::new(50.0, 20.0), 3.0));

        let curve = Stroke::curve(
            Point::new(0.0, 0.0),
            Point::new(50.0, 100.0),
            Point::new(100.0, 0.0),
        );
        assert!(curve.hit_test(Point::new(50.0, 50.0), 2.0));
        assert_eq!(
            curve.bounds(),
            Some(Rect::new(Point::new(0.0, 0.0), Point::new(100.0, 100.0)))
        );

        let mut component = Stroke::new(
            StrokeKind::Component,
            0,
            0,
            vec![Point::new(20.0, 30.0), Point::new(80.0, 90.0)],
        );
        component.component = Some(ComponentRef::new("u53e3"));
        assert!(component.hit_test(Point::new(50.0, 50.0), 0.1));
    }

    /// Replace, additive, toggle, inversion, range, and adjacent selection work.
    #[test]
    fn selection_operations_cover_editor_shortcuts() {
        let mut model = EditorModel::demo();
        let ids: Vec<StrokeId> = model.strokes().iter().map(Stroke::id).collect();
        assert!(model.select(ids[0], SelectionMode::Replace));
        assert!(model.select(ids[1], SelectionMode::Add));
        assert_eq!(model.selection().len(), 2);
        assert!(model.select(ids[0], SelectionMode::Toggle));
        assert!(!model.is_selected(ids[0]));
        assert!(model.invert_selection());
        assert!(model.is_selected(ids[0]));
        assert!(!model.is_selected(ids[1]));
        assert!(model.select_all());
        assert_eq!(model.selection().len(), ids.len());
        assert_eq!(model.select_next(), Some(ids[0]));
        assert_eq!(
            model.select_previous(),
            Some(*ids.last().expect("nonempty IDs"))
        );
        assert!(model.select_in_rect(
            Rect::new(Point::new(0.0, 0.0), Point::new(80.0, 60.0)),
            SelectionMode::Replace
        ));
        assert!(!model.selection().is_empty());
    }

    /// Multiple pointer updates within a transaction create exactly one undo.
    #[test]
    fn drag_transaction_is_one_undo_and_can_cancel() {
        let mut model = two_lines();
        let id = model.strokes()[0].id();
        model.select(id, SelectionMode::Replace);
        let original = model.to_kage();
        model.begin_transaction("Drag").expect("begin drag");
        assert!(model.move_selected(Point::new(1.0, 0.0), false));
        assert!(model.move_selected(Point::new(2.0, 0.0), false));
        assert!(model.commit_transaction().expect("commit drag"));
        assert_eq!(model.undo_label(), Some("Drag"));
        assert!(model.undo());
        assert_eq!(model.to_kage(), original);
        assert!(model.selection().is_empty());
        assert!(model.redo());
        assert!(model.selection().is_empty());

        let moved = model.to_kage();
        model.select(id, SelectionMode::Replace);
        model
            .begin_transaction("Cancelled drag")
            .expect("begin drag");
        model.move_selected(Point::new(50.0, 50.0), false);
        model.cancel_transaction().expect("cancel drag");
        assert_eq!(model.to_kage(), moved);
    }

    /// Resize previews always derive from pointer-down geometry, so their
    /// integral result cannot depend on how many mouse-move events arrived.
    #[test]
    fn resize_transaction_preview_is_path_independent() {
        let source = "2:0:7:0:0:1:2:3:6";
        let target = Rect::new(Point::new(0.0, 0.0), Point::new(5.0, 6.0));

        let mut expected = EditorModel::from_kage(source).expect("curve fixture");
        expected.select_all();
        assert!(expected.resize_selected(target, false));

        let mut actual = EditorModel::from_kage(source).expect("curve fixture");
        actual.select_all();
        actual
            .begin_transaction("Resize selection")
            .expect("begin resize");
        assert!(
            actual.preview_resize_selected(
                Rect::new(Point::new(0.0, 0.0), Point::new(4.0, 6.0)),
                false,
            )
        );
        assert!(actual.preview_resize_selected(target, false));
        assert_eq!(actual.to_kage(), expected.to_kage());
        assert_eq!(actual.strokes()[0].points()[1], Point::new(2.0, 2.0));

        assert!(actual.commit_transaction().expect("commit resize"));
        assert_eq!(actual.undo_label(), Some("Resize selection"));
        assert!(actual.undo());
        assert_eq!(actual.to_kage(), source);
    }

    /// Returning a resize drag to its starting bounds restores the exact
    /// transaction snapshot and does not create an empty undo entry.
    #[test]
    fn resize_transaction_preview_can_return_to_baseline() {
        let source = "2:0:7:0:0:1:2:3:6";
        let mut model = EditorModel::from_kage(source).expect("curve fixture");
        model.select_all();
        let original_bounds = model.selection_bounds().expect("selection bounds");
        model
            .begin_transaction("Resize selection")
            .expect("begin resize");
        assert!(
            model.preview_resize_selected(
                Rect::new(Point::new(0.0, 0.0), Point::new(5.0, 6.0)),
                false,
            )
        );
        assert!(model.preview_resize_selected(original_bounds, false));
        assert_eq!(model.to_kage(), source);
        assert!(!model.commit_transaction().expect("commit resize"));
        assert!(!model.can_undo());
    }

    /// Undo retention is capped at the documented thirty committed edits.
    #[test]
    fn undo_history_is_bounded_to_thirty() {
        let mut model = EditorModel::new();
        for offset in 0_i16..35 {
            model.insert_stroke(Stroke::line(
                Point::new(f32::from(offset), 0.0),
                Point::new(f32::from(offset), 10.0),
            ));
        }
        let mut undo_count = 0;
        while model.undo() {
            undo_count += 1;
        }
        assert_eq!(undo_count, HISTORY_LIMIT);
        assert_eq!(model.strokes().len(), 5);
    }

    /// Copy targets the visible pasteboard while cut preserves source geometry.
    #[test]
    fn pasteboard_operations_are_internal_and_undoable() {
        let mut model = two_lines();
        let first = model.strokes()[0].id();
        model.select(first, SelectionMode::Replace);
        assert_eq!(model.copy_selected(), 1);
        let pasted = model.paste();
        assert_eq!(pasted.len(), 1);
        assert_eq!(
            model.stroke(pasted[0]).expect("pasted stroke").points()[0],
            Point::new(230.0, 20.0)
        );
        assert!(!model.to_kage().is_empty());
        assert_eq!(model.to_export_kage(), two_lines().to_kage());
        let repeated = model.paste();
        assert_eq!(
            model.stroke(repeated[0]).expect("repeated paste").points(),
            model.stroke(pasted[0]).expect("first paste").points()
        );
        assert!(model.undo());
        assert!(model.undo());
        assert_eq!(model.strokes().len(), 2);
        model.select(first, SelectionMode::Replace);
        assert_eq!(model.cut_selected(), 1);
        assert_eq!(model.strokes().len(), 1);
        let cut_paste = model.paste();
        assert_eq!(
            model.stroke(cut_paste[0]).expect("cut paste").points()[0],
            Point::new(10.0, 10.0)
        );
        assert!(model.undo());
        assert!(model.undo());
        assert_eq!(model.strokes().len(), 2);
    }

    /// Z-order helpers move groups and swap arbitrary records.
    #[test]
    fn z_order_operations_preserve_group_order() {
        let mut model = EditorModel::demo();
        let ids: Vec<StrokeId> = model.strokes().iter().map(Stroke::id).collect();
        model.select(ids[1], SelectionMode::Replace);
        model.select(ids[2], SelectionMode::Add);
        assert!(model.move_selected_in_order(OrderDirection::Forward));
        let reordered: Vec<StrokeId> = model.strokes().iter().map(Stroke::id).collect();
        assert_eq!(reordered[2], ids[1]);
        assert_eq!(reordered[3], ids[2]);
        assert!(model.swap_order(ids[0], ids[4]));
        assert_eq!(model.strokes()[0].id(), ids[4]);
    }

    /// Moving one shared point can propagate to the neighboring stroke.
    #[test]
    fn connected_control_points_propagate() {
        let mut model = EditorModel::from_kage("1:0:2:10:10:50:10$1:22:0:50:10:50:60")
            .expect("compatible right-side connection");
        let first = model.strokes()[0].id();
        let second = model.strokes()[1].id();
        let connections = model.connected_points(
            ControlPointRef {
                stroke: first,
                point: 1,
            },
            CONNECTION_TOLERANCE,
        );
        assert_eq!(connections.len(), 2);
        assert!(model.move_control_point(
            ControlPointRef {
                stroke: first,
                point: 1,
            },
            Point::new(60.0, 20.0),
            true,
        ));
        assert_eq!(
            model.stroke(first).expect("first").points()[1],
            Point::new(60.0, 20.0)
        );
        assert_eq!(
            model.stroke(second).expect("second").points()[0],
            Point::new(60.0, 20.0)
        );
    }

    /// Coincident endpoints only propagate for KAGE's style-compatible pairs.
    #[test]
    fn incompatible_or_merely_near_endpoints_do_not_propagate() {
        let mut incompatible = EditorModel::from_kage("1:0:0:10:10:50:10$1:0:0:50:10:50:60")
            .expect("coincident but unconnected lines");
        let first = incompatible.strokes()[0].id();
        let second = incompatible.strokes()[1].id();
        assert!(incompatible.move_control_point(
            ControlPointRef {
                stroke: first,
                point: 1,
            },
            Point::new(60.0, 20.0),
            true,
        ));
        assert_eq!(
            incompatible.stroke(second).expect("second").points()[0],
            Point::new(50.0, 10.0)
        );

        let mut near = EditorModel::from_kage("1:0:2:10:10:50:10$1:22:0:50:9:50:60")
            .expect("nearby compatible styles");
        let first = near.strokes()[0].id();
        let second = near.strokes()[1].id();
        assert!(near.move_control_point(
            ControlPointRef {
                stroke: first,
                point: 1,
            },
            Point::new(60.0, 20.0),
            true,
        ));
        assert_eq!(
            near.stroke(second).expect("near second").points()[0],
            Point::new(50.0, 9.0)
        );
    }

    /// Off-curve handles never create accidental cross-stroke connections.
    #[test]
    fn only_on_curve_endpoints_propagate() {
        let mut model = EditorModel::from_kage("2:0:7:10:10:50:50:90:10$2:0:7:50:50:80:80:120:50")
            .expect("fixture should parse");
        let first = model.strokes()[0].id();
        let second = model.strokes()[1].id();
        assert!(model.move_control_point(
            ControlPointRef {
                stroke: first,
                point: 1,
            },
            Point::new(60.0, 60.0),
            true,
        ));
        assert_eq!(
            model.stroke(second).expect("second").points()[0],
            Point::new(50.0, 50.0)
        );
        assert_eq!(
            model.connected_points(
                ControlPointRef {
                    stroke: first,
                    point: 1,
                },
                CONNECTION_TOLERANCE,
            ),
            vec![ControlPointRef {
                stroke: first,
                point: 1,
            }]
        );
    }

    /// Selection resize and arbitrary transforms update geometry and activity.
    #[test]
    fn resize_and_transform_records_are_exposed() {
        let mut model = two_lines();
        model.select_all();
        assert!(model.resize_selected(
            Rect::new(Point::new(0.0, 0.0), Point::new(100.0, 100.0)),
            false,
        ));
        assert_eq!(
            model.selection_bounds(),
            Some(Rect::new(Point::new(0.0, 0.0), Point::new(100.0, 100.0)))
        );
        assert!(model.transform_selected("Mirror", AffineTransform::flip_horizontal(50.0), false,));
        assert_eq!(
            model.transform_records().last().expect("transform").label(),
            "Mirror"
        );
    }

    /// Positive angles rotate clockwise in KAGE's downward-positive canvas,
    /// while negative angles rotate counter-clockwise around the same center.
    #[test]
    fn affine_rotations_match_y_down_design_coordinates() {
        let source = "2:0:0:10:20:70:40:30:120";
        let center = Point::new(40.0, 70.0);
        let mut clockwise = EditorModel::from_kage(source).expect("asymmetric path");
        clockwise.select_all();
        assert!(clockwise.transform_selected(
            "Rotate right",
            AffineTransform::rotation_about(std::f32::consts::FRAC_PI_2, center),
            false,
        ));
        assert_eq!(
            clockwise.strokes()[0].points(),
            [
                Point::new(90.0, 40.0),
                Point::new(70.0, 100.0),
                Point::new(-10.0, 60.0),
            ]
        );

        let mut counter_clockwise = EditorModel::from_kage(source).expect("asymmetric path");
        counter_clockwise.select_all();
        assert!(counter_clockwise.transform_selected(
            "Rotate left",
            AffineTransform::rotation_about(-std::f32::consts::FRAC_PI_2, center),
            false,
        ));
        assert_eq!(
            counter_clockwise.strokes()[0].points(),
            [
                Point::new(-10.0, 100.0),
                Point::new(10.0, 40.0),
                Point::new(90.0, 80.0),
            ]
        );
    }

    /// Axis-aligned component and operation frames cannot encode an arbitrary
    /// affine rotation, so mixed selections are rejected without history.
    #[test]
    fn affine_transform_rejects_non_path_selection_atomically() {
        let mut model = EditorModel::from_kage("1:0:0:10:20:70:40$99:0:0:10:20:70:120:u53e3")
            .expect("mixed path and component");
        model.select_all();
        let before = model.to_kage();
        let revision = model.revision();
        let undo_label = model.undo_label().map(str::to_owned);

        assert!(!model.can_affine_transform_selection());
        assert!(!model.transform_selected(
            "Rotate right",
            AffineTransform::rotation_about(std::f32::consts::FRAC_PI_2, Point::new(40.0, 70.0),),
            true,
        ));
        assert_eq!(model.to_kage(), before);
        assert_eq!(model.revision(), revision);
        assert_eq!(model.undo_label(), undo_label.as_deref());
    }

    /// Raw controls stay immutable when a later type-0 record has already
    /// moved their rendered polygons away from those source coordinates.
    #[test]
    fn affine_transform_rejects_paths_behind_type_zero_operations() {
        let mut model = EditorModel::from_kage("1:0:0:20:80:60:80$0:98:0:0:0:200:200")
            .expect("path followed by reflection");
        let path = model.strokes()[0].id();
        model.select(path, SelectionMode::Replace);
        let before = model.to_kage();

        assert!(!model.can_affine_transform_selection());
        assert!(
            !model.transform_selected("Mirror", AffineTransform::flip_horizontal(100.0), true,)
        );
        assert_eq!(model.to_kage(), before);
    }

    /// Geometry transforms preserve KAGE-compatible endpoints outside the
    /// selected set when the inspector advertises connected geometry.
    #[test]
    fn affine_transform_propagates_compatible_connected_endpoints() {
        let mut model = EditorModel::from_kage("1:0:2:10:10:50:10$1:22:0:50:10:50:60")
            .expect("compatible right-side connection");
        let first = model.strokes()[0].id();
        let second = model.strokes()[1].id();
        model.select(first, SelectionMode::Replace);

        assert!(model.transform_selected("Mirror", AffineTransform::flip_horizontal(30.0), true,));
        assert_eq!(
            model.stroke(first).expect("selected").points(),
            [Point::new(50.0, 10.0), Point::new(10.0, 10.0)]
        );
        assert_eq!(
            model.stroke(second).expect("connected").points()[0],
            Point::new(10.0, 10.0)
        );
    }

    /// Geometry rotation keeps valid KAGE styles and safely resets a style
    /// whose horizontal-only pair no longer fits the rotated path.
    #[test]
    fn affine_transform_repairs_direction_dependent_styles() {
        let mut model =
            EditorModel::from_kage("1:2:2:10:20:70:20").expect("horizontal styled line");
        model.select_all();

        assert!(model.transform_selected(
            "Rotate right",
            AffineTransform::rotation_about(std::f32::consts::FRAC_PI_2, Point::new(40.0, 20.0),),
            false,
        ));
        let stroke = &model.strokes()[0];
        assert_eq!((stroke.head(), stroke.tail()), (0, 0));
        assert!(
            model
                .validate()
                .iter()
                .all(|issue| issue.code != ValidationCode::InvalidStyleCombination)
        );
    }

    /// Transform mutations use JavaScript's `Math.round` semantics at negative
    /// half units, matching the reference editor rather than Rust's `round`.
    #[test]
    fn affine_transform_uses_javascript_rounding() {
        let mut model = EditorModel::from_kage("1:0:0:0:0:2:0").expect("line");
        model.select_all();
        assert!(model.transform_selected(
            "Move half",
            AffineTransform::translation(Point::new(-1.5, 0.0)),
            false,
        ));
        assert_eq!(
            model.strokes()[0].points(),
            [Point::new(-1.0, 0.0), Point::new(1.0, 0.0)]
        );
    }

    /// Type-0 ranges must remain ordered because crossed bounds select no
    /// polygons in the KAGE engine.
    #[test]
    fn type_zero_frame_resize_normalizes_crossed_diagonal() {
        let mut model =
            EditorModel::from_kage("0:98:0:20:30:180:170").expect("horizontal reflection");
        let transform = model.strokes()[0].id();
        assert!(model.resize_frame_record(
            transform,
            Point::new(190.0, 180.0),
            Point::new(10.0, 20.0),
        ));
        assert_eq!(
            model.stroke(transform).expect("type zero").points(),
            [Point::new(10.0, 20.0), Point::new(190.0, 180.0)]
        );
        assert_eq!(model.to_kage(), "0:98:0:10:20:190:180");
    }

    /// Component frames retain crossed diagonals to encode negative scale,
    /// while ordinary paths reject frame editing.
    #[test]
    fn frame_resize_can_flip_special_records() {
        let mut model = model_with_component_fixtures();
        let component = model
            .insert_component(
                "u53e3",
                Rect::new(Point::new(20.0, 30.0), Point::new(180.0, 170.0)),
            )
            .expect("component");
        assert!(model.resize_frame_record(
            component,
            Point::new(180.4, 170.4),
            Point::new(19.6, 29.6),
        ));
        assert_eq!(
            model.stroke(component).expect("component").points(),
            [Point::new(180.0, 170.0), Point::new(20.0, 30.0)]
        );
        assert!(model.to_kage().contains("180:170:20:30:u53e3"));
        let children = model
            .decompose_component(component)
            .expect("flipped decomposition");
        assert_eq!(
            model.stroke(children[0]).expect("first child").points(),
            [Point::new(152.0, 149.0), Point::new(152.0, 51.0)]
        );

        let line =
            model.insert_stroke(Stroke::line(Point::new(0.0, 0.0), Point::new(100.0, 100.0)));
        assert!(!model.resize_frame_record(line, Point::new(100.0, 100.0), Point::new(0.0, 0.0),));
    }

    /// Type resampling, style changes, and validation report structural issues.
    #[test]
    fn style_editing_and_validation_are_typed() {
        let mut model = two_lines();
        let id = model.strokes()[0].id();
        assert!(model.set_stroke_kind(id, StrokeKind::Bezier));
        assert_eq!(model.stroke(id).expect("stroke").points().len(), 4);
        assert!(model.set_stroke_head(id, 999));
        assert!(model.set_stroke_tail(id, 998));
        let issues = model.validate();
        assert!(
            issues
                .iter()
                .any(|issue| issue.code == ValidationCode::UnknownHead)
        );
        assert!(
            issues
                .iter()
                .any(|issue| issue.code == ValidationCode::UnknownTail)
        );
    }

    /// Stroke-family changes use KAGE's explicit degree conversion formulas.
    #[test]
    fn stroke_kind_conversion_preserves_standard_control_geometry() {
        let mut line = EditorModel::from_kage("1:0:0:-2:-2:7:7").expect("line");
        let id = line.strokes()[0].id();
        assert!(line.set_stroke_kind(id, StrokeKind::Bezier));
        assert_eq!(
            line.stroke(id).expect("converted line").points(),
            [
                Point::new(-2.0, -2.0),
                Point::new(1.0, 1.0),
                Point::new(4.0, 4.0),
                Point::new(7.0, 7.0),
            ]
        );

        let mut quadratic = EditorModel::from_kage("2:0:7:0:0:30:60:90:0").expect("quadratic");
        let id = quadratic.strokes()[0].id();
        assert!(quadratic.set_stroke_kind(id, StrokeKind::Bezier));
        assert_eq!(
            quadratic.stroke(id).expect("elevated curve").points(),
            [
                Point::new(0.0, 0.0),
                Point::new(20.0, 40.0),
                Point::new(50.0, 40.0),
                Point::new(90.0, 0.0),
            ]
        );
        assert!(quadratic.set_stroke_kind(id, StrokeKind::Curve));
        assert_eq!(
            quadratic.stroke(id).expect("reduced curve").points(),
            [
                Point::new(0.0, 0.0),
                Point::new(35.0, 40.0),
                Point::new(90.0, 0.0),
            ]
        );
    }

    /// Curve constructors begin with an engine-supported style pair.
    #[test]
    fn curve_constructors_use_valid_default_styles() {
        let curve = Stroke::curve(
            Point::new(10.0, 20.0),
            Point::new(80.0, 10.0),
            Point::new(160.0, 120.0),
        );
        let bezier = Stroke::bezier(
            Point::new(10.0, 20.0),
            Point::new(50.0, 10.0),
            Point::new(120.0, 180.0),
            Point::new(180.0, 120.0),
        );

        for stroke in [&curve, &bezier] {
            assert_eq!((stroke.head(), stroke.tail()), (0, 7));
            assert!(valid_style_combination(stroke));
        }
    }

    /// Kind changes repair incompatible pairs even when both individual values
    /// are accepted by the destination family.
    #[test]
    fn kind_changes_leave_every_path_with_a_valid_style_pair() {
        for kind in [
            StrokeKind::Line,
            StrokeKind::Curve,
            StrokeKind::Bend,
            StrokeKind::Corner,
            StrokeKind::Bezier,
            StrokeKind::Sweep,
        ] {
            let mut model = EditorModel::new();
            let id = model.insert_stroke(Stroke::new(
                StrokeKind::Transform,
                0,
                0,
                vec![Point::new(20.0, 100.0), Point::new(180.0, 100.0)],
            ));

            assert!(model.set_stroke_kind(id, kind));
            let stroke = model.stroke(id).expect("changed stroke");
            assert!(kind.head_shapes().contains(&stroke.head()));
            assert!(kind.tail_shapes().contains(&stroke.tail()));
            assert!(
                valid_style_combination(stroke),
                "type {} retained invalid style ({}, {})",
                kind.code(),
                stroke.head(),
                stroke.tail()
            );
        }

        let mut model =
            EditorModel::from_kage("2:7:0:20:100:100:40:180:100").expect("curve fixture");
        let id = model.strokes()[0].id();
        assert!(model.set_stroke_kind(id, StrokeKind::Bezier));
        let stroke = model.stroke(id).expect("changed stroke");
        assert_eq!((stroke.head(), stroke.tail()), (7, 0));
        assert!(valid_style_combination(stroke));
    }

    /// Stroke-family choices, pair compatibility, and the type-1 direction
    /// rule are validated independently.
    #[test]
    fn style_validation_matches_kage_family_rules() {
        assert_eq!(StrokeKind::Curve.head_shapes(), &[0, 32, 12, 22, 7, 27]);
        assert_eq!(StrokeKind::Sweep.tail_shapes(), &[7]);

        let curve = EditorModel::from_kage("2:0:0:20:100:100:40:180:100").expect("curve fixture");
        assert!(
            curve
                .validate()
                .iter()
                .any(|issue| { issue.code == ValidationCode::InvalidStyleCombination })
        );

        let horizontal =
            EditorModel::from_kage("1:12:0:20:100:180:100").expect("horizontal line fixture");
        assert!(
            horizontal
                .validate()
                .iter()
                .any(|issue| { issue.code == ValidationCode::InvalidStyleCombination })
        );

        let mut changed = EditorModel::from_kage("1:2:2:20:100:180:100").expect("line fixture");
        let id = changed.strokes()[0].id();
        assert!(changed.set_stroke_kind(id, StrokeKind::Sweep));
        let stroke = changed.stroke(id).expect("changed stroke");
        assert_eq!((stroke.head(), stroke.tail()), (0, 7));
    }

    /// Explicit test fixtures can be searched, inserted, stretched, and decomposed.
    #[test]
    fn component_workflow_is_searchable_with_explicit_fixtures() {
        let mut model = model_with_component_fixtures();
        let results = model.component_library().search("tree wood");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name(), "u6728");
        let component = model
            .insert_component(
                "u6728",
                Rect::new(Point::new(20.0, 30.0), Point::new(180.0, 170.0)),
            )
            .expect("insert component");
        assert!(
            model
                .stretch_component(
                    component,
                    Rect::new(Point::new(10.0, 20.0), Point::new(190.0, 180.0)),
                    Some(ComponentStretch::new(150, 0, -10, 50)),
                )
                .expect("stretch component")
        );
        assert!(
            model
                .to_kage()
                .contains("99:150:0:10:20:190:180:u6728:0:-10:50")
        );
        let children = model
            .decompose_component(component)
            .expect("decompose component");
        assert_eq!(children.len(), 4);
        assert!(model.strokes().iter().all(|stroke| stroke.kind().is_path()));
        assert_eq!(model.selection().len(), 4);
    }

    /// Source metadata drives the exact `-10..=10` stretch inspector contract.
    #[test]
    fn component_stretch_metadata_is_extracted_and_round_trips_slider_values() {
        let mut model = model_with_component_fixtures();
        let tree = model
            .component_library()
            .get("u6728")
            .expect("built-in tree");
        assert!(!tree.source().starts_with("0:1:0"));
        let guide = tree.stretch_guide().expect("tree stretch guide");
        assert_eq!(guide.stretch(0), ComponentStretch::new(200, 0, 0, 0));
        assert_eq!(guide.value(Some(guide.stretch(-10))), -10);
        assert_eq!(guide.value(Some(guide.stretch(10))), 10);

        let component = model
            .insert_component(
                "u6728",
                Rect::new(Point::new(0.0, 0.0), Point::new(200.0, 200.0)),
            )
            .expect("insert tree");
        assert_eq!(model.component_stretch_value(component), Some(0));
        assert!(
            model
                .set_component_stretch_value(component, 10)
                .expect("set stretch")
        );
        assert_eq!(model.component_stretch_value(component), Some(10));
        assert!(model.to_kage().contains("99:240:0:0:0:200:200:u6728:0:0:0"));
        assert_eq!(
            model
                .decompose_component(component)
                .expect("metadata is not decomposed as a record")
                .len(),
            4
        );
    }

    /// Decomposition carries an active parent stretch into neutral nested parts.
    #[test]
    fn nested_component_decomposition_composes_parent_stretch() {
        let mut model = model_with_component_fixtures();
        let grove = model
            .insert_component(
                "u6797",
                Rect::new(Point::new(0.0, 0.0), Point::new(200.0, 200.0)),
            )
            .expect("insert nested component");
        assert!(
            model
                .stretch_component(
                    grove,
                    Rect::new(Point::new(0.0, 0.0), Point::new(200.0, 200.0)),
                    Some(ComponentStretch::new(200, 0, -50, 0)),
                )
                .expect("stretch parent")
        );
        let children = model.decompose_component(grove).expect("decompose parent");
        assert_eq!(children.len(), 2);
        for child in children {
            let component = model
                .stroke(child)
                .and_then(Stroke::component)
                .expect("nested tree remains a component");
            let stretch = component.stretch().expect("composed child stretch");
            let normalized = normalize_component_stretch(Some(stretch));
            assert!(normalized.0 != normalized.2 - 200 || normalized.1 != normalized.3);
        }
    }

    /// Component-name search includes direct and transitive dependants so the
    /// Shift-click discovery workflow is useful without a network service.
    #[test]
    fn component_search_discovers_nested_dependants() {
        let library = ComponentLibrary::builtin();
        let tree_dependants = library
            .search("u6728")
            .into_iter()
            .map(ComponentDefinition::name)
            .collect::<Vec<_>>();
        assert!(tree_dependants.contains(&"u6797"));
        assert!(tree_dependants.contains(&"u68ee"));
        assert!(tree_dependants.contains(&"u76f8"));
        assert!(tree_dependants.contains(&"u60f3"));

        let mutual_dependants = library
            .search("u76f8")
            .into_iter()
            .map(ComponentDefinition::name)
            .collect::<Vec<_>>();
        assert!(mutual_dependants.contains(&"u60f3"));
    }

    /// Missing component definitions are surfaced instead of rendering silently.
    #[test]
    fn validation_reports_unavailable_component_sources() {
        let model = EditorModel::from_kage("99:0:0:0:0:200:200:u-does-not-exist")
            .expect("component reference syntax");
        assert!(model.validate().iter().any(|issue| {
            issue.severity == ValidationSeverity::Error
                && issue.code == ValidationCode::MissingComponent
                && issue.message.contains("u-does-not-exist")
        }));
    }

    /// Every selected component decomposes atomically into one undo entry.
    #[test]
    fn selected_components_decompose_as_one_action() {
        let mut model = model_with_component_fixtures();
        let ordinary =
            model.insert_stroke(Stroke::line(Point::new(10.0, 10.0), Point::new(30.0, 10.0)));
        let first = model
            .insert_component(
                "u53e3",
                Rect::new(Point::new(0.0, 0.0), Point::new(90.0, 90.0)),
            )
            .expect("first component");
        let second = model
            .insert_component(
                "u6728",
                Rect::new(Point::new(100.0, 100.0), Point::new(200.0, 200.0)),
            )
            .expect("second component");
        model.select(ordinary, SelectionMode::Add);
        model.select(first, SelectionMode::Add);
        let before = model.to_kage();

        let children = model
            .decompose_selected_components()
            .expect("decompose selection");
        assert_eq!(children.len(), 8);
        assert!(model.is_selected(ordinary));
        assert!(model.stroke(second).is_none());
        assert_eq!(model.undo_label(), Some("Decompose components"));
        assert!(model.undo());
        assert_eq!(model.to_kage(), before);
    }

    /// Component pivot stretching follows KAGE's two linear segments.
    #[test]
    fn piecewise_component_stretch_uses_source_and_destination_pivots() {
        assert_close(stretch_coordinate(20.0, 0.0, 50.0, 0.0, 200.0), 60.0);
        assert_close(stretch_coordinate(20.0, 0.0, 150.0, 0.0, 200.0), 160.0);
        let preset = ComponentStretch::horizontal_pivots(-30, 0);
        assert_eq!(preset, ComponentStretch::new(170, 0, 0, 0));
        let mut strokes = vec![
            Stroke::line(Point::new(0.0, 0.0), Point::new(200.0, 200.0)),
            Stroke::line(Point::new(50.0, 25.0), Point::new(150.0, 175.0)),
        ];
        apply_component_stretch(&mut strokes, preset);
        assert_eq!(strokes[1].points()[0], Point::new(35.0, 25.0));
        assert_eq!(strokes[1].points()[1], Point::new(135.0, 175.0));
        let mut model = EditorModel::new();
        let id = model.insert_kage_transform(
            KageTransform::FlipHorizontal,
            Rect::new(Point::new(10.0, 20.0), Point::new(190.0, 180.0)),
        );
        assert_eq!(
            model.stroke(id).and_then(Stroke::kage_transform),
            Some(KageTransform::FlipHorizontal)
        );
        assert_eq!(model.to_kage(), "0:98:0:10:20:190:180");
    }

    /// Display settings persist independently from glyph undo history.
    #[test]
    fn settings_express_all_editor_display_modes() {
        let mut model = EditorModel::new();
        assert!(!model.settings().use_curve);
        model.insert_stroke(Stroke::line(Point::new(0.0, 0.0), Point::new(20.0, 0.0)));
        let settings = EditorSettings {
            grid: GridSettings {
                visible: true,
                origin_x: 3.0,
                origin_y: 7.0,
                spacing_x: 10.0,
                spacing_y: 25.0,
                snap: true,
                subdivisions: 4,
            },
            typeface: Typeface::Gothic,
            use_curve: true,
            centerline: CenterlineMode::Always,
            mask: MaskMode::Diamond,
            language: UiLanguage::Korean,
        };
        model.set_settings(settings);
        assert_eq!(*model.settings(), settings);
        assert_eq!(
            model.snap_point(Point::new(27.0, 46.0)),
            Point::new(23.0, 57.0)
        );
        assert!(model.undo());
        assert_eq!(*model.settings(), settings);
        assert!(model.strokes().is_empty());
    }

    /// Type 7 follows its straight-first, quadratic-second KAGE geometry.
    #[test]
    fn sweep_sampling_uses_line_then_quadratic() {
        let sweep = Stroke::new(
            StrokeKind::Sweep,
            0,
            7,
            vec![
                Point::new(0.0, 0.0),
                Point::new(0.0, 100.0),
                Point::new(100.0, 100.0),
                Point::new(100.0, 0.0),
            ],
        );
        let samples = sweep.sampled_path(4);
        assert_eq!(samples.len(), 6);
        assert_eq!(samples[0], Point::new(0.0, 0.0));
        assert_eq!(samples[1], Point::new(0.0, 100.0));
        assert_eq!(samples[3], Point::new(75.0, 75.0));
        assert_eq!(samples[5], Point::new(100.0, 0.0));
    }

    /// Builds an ordinary path ending at the shared hook-test anchor.
    fn hook_target(kind: StrokeKind, head: i32, tail: i32) -> Stroke {
        let points = match kind {
            StrokeKind::Line => vec![Point::new(100.0, 20.0), Point::new(100.0, 100.0)],
            StrokeKind::Curve | StrokeKind::Bend | StrokeKind::Corner => vec![
                Point::new(20.0, 100.0),
                Point::new(60.0, 40.0),
                Point::new(100.0, 100.0),
            ],
            StrokeKind::Bezier => vec![
                Point::new(20.0, 100.0),
                Point::new(40.0, 40.0),
                Point::new(80.0, 40.0),
                Point::new(100.0, 100.0),
            ],
            _ => panic!("type {} is not a hook-test path", kind.code()),
        };
        Stroke::new(kind, head, tail, points)
    }

    /// A short left gesture attaches to line and curve-family endpoints.
    #[test]
    fn left_hook_updates_the_last_line_or_curve_path() {
        for kind in [StrokeKind::Line, StrokeKind::Curve, StrokeKind::Bezier] {
            let mut model = EditorModel::new();
            let id = model.insert_stroke(hook_target(kind, 27, 0));
            let count = model.strokes().len();
            let recognized = model
                .insert_gesture(&[Point::new(100.0, 100.0), Point::new(80.0, 100.0)])
                .expect("left hook");

            assert_eq!(recognized.kind(), GestureKind::LeftHook);
            assert_eq!(recognized.stroke().id(), id);
            assert_eq!(model.strokes().len(), count);
            let stroke = model.stroke(id).expect("hook target");
            assert_eq!((stroke.head(), stroke.tail()), (22, 4));
            assert!(valid_style_combination(stroke));
        }
    }

    /// A rising-right gesture applies the curve-family head conversions.
    #[test]
    fn right_hook_updates_curve_and_bezier_styles() {
        for (kind, head, expected_head) in [(StrokeKind::Curve, 7, 0), (StrokeKind::Bezier, 27, 22)]
        {
            let mut model = EditorModel::new();
            let id = model.insert_stroke(hook_target(kind, head, 0));
            let recognized = model
                .insert_gesture(&[Point::new(100.0, 100.0), Point::new(116.0, 88.0)])
                .expect("right hook");

            assert_eq!(recognized.kind(), GestureKind::RightHook);
            assert_eq!(model.strokes().len(), 1);
            let stroke = model.stroke(id).expect("hook target");
            assert_eq!((stroke.head(), stroke.tail()), (expected_head, 5));
            assert!(valid_style_combination(stroke));
        }
    }

    /// An upward gesture applies a hook to bend and corner records.
    #[test]
    fn up_hook_updates_bend_and_corner_styles() {
        for kind in [StrokeKind::Bend, StrokeKind::Corner] {
            let mut model = EditorModel::new();
            let id = model.insert_stroke(hook_target(kind, 0, 0));
            let recognized = model
                .insert_gesture(&[Point::new(100.0, 100.0), Point::new(100.0, 80.0)])
                .expect("up hook");

            assert_eq!(recognized.kind(), GestureKind::UpHook);
            assert_eq!(model.strokes().len(), 1);
            let stroke = model.stroke(id).expect("hook target");
            assert_eq!(stroke.tail(), 5);
            assert!(valid_style_combination(stroke));
        }
    }

    /// A hook is one undoable mutation, while a detached short trace retains
    /// ordinary gesture recognition and inserts a new record.
    #[test]
    fn hook_is_one_undo_without_inserting_and_detached_trace_falls_back() {
        let mut model = EditorModel::new();
        let id = model.insert_stroke(Stroke::curve(
            Point::new(20.0, 100.0),
            Point::new(60.0, 40.0),
            Point::new(100.0, 100.0),
        ));
        let before = (
            model.stroke(id).expect("curve").head(),
            model.stroke(id).expect("curve").tail(),
        );
        let recognized = model
            .insert_gesture(&[Point::new(100.0, 100.0), Point::new(116.0, 88.0)])
            .expect("attached hook");

        assert_eq!(recognized.kind(), GestureKind::RightHook);
        assert_eq!(model.strokes().len(), 1);
        assert_eq!(model.undo_label(), Some("Add hook"));
        assert!(model.undo());
        assert_eq!(model.strokes().len(), 1);
        let restored = model.stroke(id).expect("restored curve");
        assert_eq!((restored.head(), restored.tail()), before);
        assert_eq!(model.undo_label(), Some("Insert stroke"));

        let recognized = model
            .insert_gesture(&[Point::new(110.0, 100.0), Point::new(90.0, 100.0)])
            .expect("detached line");
        assert_eq!(recognized.kind(), GestureKind::Line);
        assert_eq!(model.strokes().len(), 2);
    }

    /// Straight, smooth, cornered, and swept gestures choose distinct KAGE types.
    #[test]
    fn freehand_recognizes_line_curve_bend_and_sweep() {
        let line: Vec<Point> = (0_i16..=10)
            .map(|index| Point::new(f32::from(index) * 10.0, f32::from(index % 2) * 0.2))
            .collect();
        let recognized = recognize_gesture(&line).expect("line gesture");
        assert_eq!(recognized.kind(), GestureKind::Line);
        assert_eq!(recognized.stroke().kind(), StrokeKind::Line);

        let curve: Vec<Point> = (0_i16..=16)
            .map(|index| {
                let t = f32::from(index) / 16.0;
                quadratic(
                    Point::new(10.0, 10.0),
                    Point::new(100.0, 150.0),
                    Point::new(190.0, 20.0),
                    t,
                )
            })
            .collect();
        let recognized = recognize_gesture(&curve).expect("curve gesture");
        assert_eq!(recognized.kind(), GestureKind::Curve);
        assert_eq!(recognized.stroke().kind(), StrokeKind::Curve);

        let bend = vec![
            Point::new(10.0, 20.0),
            Point::new(40.0, 20.0),
            Point::new(70.0, 20.0),
            Point::new(100.0, 20.0),
            Point::new(100.0, 50.0),
            Point::new(100.0, 80.0),
            Point::new(100.0, 110.0),
        ];
        let recognized = recognize_gesture(&bend).expect("bend gesture");
        assert_eq!(recognized.kind(), GestureKind::Bend);
        assert_eq!(recognized.stroke().kind(), StrokeKind::Bend);

        let sweep: Vec<Point> = (0_i16..=20)
            .map(|index| {
                let t = f32::from(index) / 20.0;
                cubic(
                    Point::new(120.0, 20.0),
                    Point::new(122.0, 72.0),
                    Point::new(118.0, 135.0),
                    Point::new(80.0, 178.0),
                    t,
                )
            })
            .collect();
        let recognized = recognize_gesture(&sweep).expect("sweep gesture");
        assert_eq!(recognized.kind(), GestureKind::Sweep);
        assert_eq!(recognized.stroke().kind(), StrokeKind::Sweep);
    }

    /// Freehand samples snap to nearby endpoints and horizontal/vertical runs.
    #[test]
    fn freehand_geometry_snap_uses_ten_unit_axis_tolerance() {
        let model = EditorModel::from_kage("1:0:0:20:40:180:40$1:0:0:100:60:100:180")
            .expect("axis fixture");
        assert_eq!(
            model.snap_freehand_point(Point::new(70.0, 47.0), 10.0),
            Point::new(70.0, 40.0)
        );
        assert_eq!(
            model.snap_freehand_point(Point::new(94.0, 120.0), 10.0),
            Point::new(100.0, 120.0)
        );
        assert_eq!(
            model.snap_freehand_point(Point::new(174.0, 44.0), 10.0),
            Point::new(180.0, 40.0)
        );
        assert_eq!(
            model.snap_freehand_point(Point::new(70.0, 52.0), 10.0),
            Point::new(70.0, 52.0)
        );

        let connected =
            EditorModel::from_kage("1:0:2:20:20:100:20").expect("connectable endpoint fixture");
        let mut new_stroke = Stroke::line(Point::new(103.0, 23.0), Point::new(160.0, 90.0));
        snap_stroke_endpoints(&connected, &mut new_stroke, 10.0);
        apply_freehand_connection_styles(connected.strokes(), &mut new_stroke);
        assert_eq!(new_stroke.points()[0], Point::new(100.0, 20.0));
        assert_eq!(new_stroke.head(), 22);
        assert!(connections_match(
            connection_anchor(&connected.strokes()[0], 1).expect("existing endpoint"),
            connection_anchor(&new_stroke, 0).expect("new endpoint"),
        ));
    }
}
