//! Rendering adapter between the editor package and `wcshds/kage`.
//!
//! The current engine deliberately keeps its polygon point type behind a
//! private module.  Its public SVG exporter is therefore used as the stable
//! boundary and the simple polygon form is decoded back into canvas points.

use kage_rs::{Typeface, kage::Kage, polygons::Polygons};

use super::model::{ComponentLibrary, EditorModel};

/// One filled outline returned by the KAGE engine in its native 200×200 space.
pub(crate) type Outline = Vec<(f32, f32)>;

/// Renders KAGE source into filled outlines.
///
/// Component references are flattened before reaching the pinned KAGE engine
/// because its native component expansion discards nested type-0 operations.
/// A fresh engine is cheap for the small demonstration glyphs and makes this
/// function deterministic and side-effect free.
pub(crate) fn render_outlines(
    source: &str,
    components: &ComponentLibrary,
    gothic: bool,
    use_curve: bool,
) -> Vec<Outline> {
    let typeface = if gothic {
        Typeface::Gothic
    } else {
        Typeface::Ming
    };
    let engine = Kage::new(typeface, use_curve);
    let mut model = EditorModel::new();
    model.set_component_library(components.clone());
    let source = model.flattened_render_source(source);

    let mut polygons = Polygons::new();
    engine.make_glyph_with_data(&mut polygons, &source);
    let svg = polygons.generate_svg(use_curve);
    if use_curve {
        parse_path_svg(&svg)
    } else {
        parse_polygon_svg(&svg)
    }
}

/// Converts the engine's public, non-curve SVG representation into points.
fn parse_polygon_svg(svg: &str) -> Vec<Outline> {
    const ATTRIBUTE: &str = "points=\"";

    let mut outlines = Vec::new();
    let mut remaining = svg;
    while let Some(attribute_start) = remaining.find(ATTRIBUTE) {
        remaining = &remaining[attribute_start + ATTRIBUTE.len()..];
        let Some(attribute_end) = remaining.find('"') else {
            break;
        };

        let outline = remaining[..attribute_end]
            .split_ascii_whitespace()
            .filter_map(|pair| {
                let (x, y) = pair.split_once(',')?;
                Some((x.parse::<f32>().ok()?, y.parse::<f32>().ok()?))
            })
            .collect::<Vec<_>>();
        if outline.len() >= 3 {
            outlines.push(outline);
        }
        remaining = &remaining[attribute_end + 1..];
    }
    outlines
}

/// Converts KAGE's curve SVG paths into polygonal outlines for the canvas.
///
/// KAGE emits only absolute `M`, `L`, `Q`, and `Z` commands. Quadratic curves
/// are flattened here because the canvas consumes filled point lists rather
/// than SVG path commands.
fn parse_path_svg(svg: &str) -> Vec<Outline> {
    const PATH_TAG: &str = "<path";
    const ATTRIBUTE: &str = "d=\"";

    let mut outlines = Vec::new();
    let mut remaining = svg;
    while let Some(path_start) = remaining.find(PATH_TAG) {
        remaining = &remaining[path_start + PATH_TAG.len()..];
        let Some(tag_end) = remaining.find('>') else {
            break;
        };
        let tag = &remaining[..tag_end];

        if let Some(attribute_start) = tag.find(ATTRIBUTE) {
            let value = &tag[attribute_start + ATTRIBUTE.len()..];
            if let Some(attribute_end) = value.find('"')
                && let Some(outline) = parse_path_data(&value[..attribute_end])
            {
                outlines.push(outline);
            }
        }

        remaining = &remaining[tag_end + 1..];
    }
    outlines
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum PathToken {
    Command(u8),
    Number(f32),
}

fn tokenize_path_data(data: &str) -> Option<Vec<PathToken>> {
    let bytes = data.as_bytes();
    let mut tokens = Vec::new();
    let mut index = 0;

    while index < bytes.len() {
        let byte = bytes[index];
        if byte.is_ascii_whitespace() || byte == b',' {
            index += 1;
            continue;
        }
        if matches!(byte, b'M' | b'L' | b'Q' | b'Z') {
            tokens.push(PathToken::Command(byte));
            index += 1;
            continue;
        }
        if byte >= 0x80 {
            return None;
        }

        let start = index;
        if matches!(bytes[index], b'+' | b'-') {
            index += 1;
        }

        let integer_start = index;
        while index < bytes.len() && bytes[index].is_ascii_digit() {
            index += 1;
        }
        let mut has_digit = index > integer_start;

        if index < bytes.len() && bytes[index] == b'.' {
            index += 1;
            let fraction_start = index;
            while index < bytes.len() && bytes[index].is_ascii_digit() {
                index += 1;
            }
            has_digit |= index > fraction_start;
        }
        if !has_digit {
            return None;
        }

        if index < bytes.len() && matches!(bytes[index], b'e' | b'E') {
            index += 1;
            if index < bytes.len() && matches!(bytes[index], b'+' | b'-') {
                index += 1;
            }
            let exponent_start = index;
            while index < bytes.len() && bytes[index].is_ascii_digit() {
                index += 1;
            }
            if index == exponent_start {
                return None;
            }
        }

        let number = data[start..index].parse::<f32>().ok()?;
        if !number.is_finite() {
            return None;
        }
        tokens.push(PathToken::Number(number));
    }

    Some(tokens)
}

fn take_number(tokens: &[PathToken], index: &mut usize) -> Option<f32> {
    match tokens.get(*index)? {
        PathToken::Number(number) => {
            *index += 1;
            Some(*number)
        }
        PathToken::Command(_) => None,
    }
}

fn take_point(tokens: &[PathToken], index: &mut usize) -> Option<(f32, f32)> {
    Some((take_number(tokens, index)?, take_number(tokens, index)?))
}

fn append_quadratic(
    outline: &mut Outline,
    start: (f32, f32),
    control: (f32, f32),
    end: (f32, f32),
) {
    const STEPS: usize = 24;

    for step in 1..=STEPS {
        let t = step as f32 / STEPS as f32;
        let one_minus_t = 1.0 - t;
        outline.push((
            one_minus_t * one_minus_t * start.0 + 2.0 * one_minus_t * t * control.0 + t * t * end.0,
            one_minus_t * one_minus_t * start.1 + 2.0 * one_minus_t * t * control.1 + t * t * end.1,
        ));
    }
}

fn parse_path_data(data: &str) -> Option<Outline> {
    let tokens = tokenize_path_data(data)?;
    let mut outline = Vec::new();
    let mut index = 0;
    let mut command = None;
    let mut current = None;
    let mut start = None;
    let mut closed = false;

    while index < tokens.len() {
        if let PathToken::Command(next_command) = tokens[index] {
            index += 1;
            if next_command == b'Z' {
                if start.is_none() || closed {
                    return None;
                }
                closed = true;
                if index != tokens.len() {
                    return None;
                }
                continue;
            }
            if closed {
                return None;
            }
            command = Some(next_command);
        }

        match command? {
            b'M' => {
                if !outline.is_empty() {
                    return None;
                }
                let point = take_point(&tokens, &mut index)?;
                outline.push(point);
                current = Some(point);
                start = Some(point);
                // Further coordinate pairs after a moveto are implicit lines.
                command = Some(b'L');
            }
            b'L' => {
                current?;
                let point = take_point(&tokens, &mut index)?;
                outline.push(point);
                current = Some(point);
            }
            b'Q' => {
                let from = current?;
                let control = take_point(&tokens, &mut index)?;
                let end = if matches!(tokens.get(index), Some(PathToken::Number(_))) {
                    take_point(&tokens, &mut index)?
                } else if matches!(tokens.get(index), Some(PathToken::Command(b'Z'))) {
                    // KAGE can end a closed contour with an off-curve control
                    // point. Its SVG spells that as `Q control-x,control-y Z`,
                    // with the initial moveto point serving as the endpoint.
                    start?
                } else {
                    return None;
                };
                append_quadratic(&mut outline, from, control, end);
                current = Some(end);
            }
            _ => return None,
        }
    }

    if closed && outline.len() >= 3 {
        Some(outline)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ComponentDefinition;

    fn empty_components() -> ComponentLibrary {
        ComponentLibrary::new()
    }

    fn outline_bounds(outlines: &[Outline]) -> Option<(f32, f32, f32, f32)> {
        let mut points = outlines.iter().flatten().copied();
        let (first_x, first_y) = points.next()?;
        Some(points.fold(
            (first_x, first_y, first_x, first_y),
            |(min_x, min_y, max_x, max_y), (x, y)| {
                (min_x.min(x), min_y.min(y), max_x.max(x), max_y.max(y))
            },
        ))
    }

    /// The adapter keeps every polygon and all coordinates finite.
    #[test]
    fn renders_a_real_kage_stroke() {
        let outlines = render_outlines("1:0:0:20:100:180:100", &empty_components(), false, false);
        assert!(!outlines.is_empty());
        assert!(
            outlines
                .iter()
                .flatten()
                .all(|(x, y)| x.is_finite() && y.is_finite())
        );
    }

    /// The five asymmetric type-0 operations follow KAGE's ordered formulas,
    /// including the swapped extent of quarter turns in the y-down design
    /// coordinate system.
    #[test]
    fn type_zero_operations_map_asymmetric_outline_in_engine_order() {
        const PATH: &str = "1:0:0:20:80:60:80";
        let components = empty_components();
        let cases = [
            ("0:98:0:0:0:200:200", (140.0, 69.0, 180.0, 82.0)),
            ("0:97:0:0:0:200:200", (20.0, 118.0, 60.0, 131.0)),
            ("0:99:1:0:0:200:200", (118.0, 20.0, 131.0, 60.0)),
            ("0:99:2:0:0:200:200", (140.0, 118.0, 180.0, 131.0)),
            ("0:99:3:0:0:200:200", (69.0, 140.0, 82.0, 180.0)),
        ];

        for (operation, expected) in cases {
            let source = format!("{PATH}${operation}");
            let outlines = render_outlines(&source, &components, false, false);
            assert_eq!(
                outline_bounds(&outlines),
                Some(expected),
                "unexpected bounds after {operation}"
            );
        }
    }

    /// KAGE does not normalize an imported operation frame and applies type-0
    /// only to polygons emitted before that record.
    #[test]
    fn type_zero_keeps_ordered_frame_and_record_order_semantics() {
        const PATH: &str = "1:0:0:20:80:60:80";
        const FLIP: &str = "0:98:0:0:0:200:200";
        const REVERSED_FLIP: &str = "0:98:0:200:0:0:200";
        let components = empty_components();
        let base = render_outlines(PATH, &components, false, false);

        assert_eq!(
            render_outlines(&format!("{FLIP}${PATH}"), &components, false, false),
            base,
            "an operation before a path cannot affect it"
        );
        assert_eq!(
            render_outlines(
                &format!("{PATH}${REVERSED_FLIP}"),
                &components,
                false,
                false
            ),
            base,
            "a crossed operation frame selects no polygons"
        );
        assert_ne!(
            render_outlines(&format!("{PATH}${FLIP}"), &components, false, false),
            base,
            "an ordered operation after the path must affect it"
        );
    }

    /// Component flattening must retain a nested type-0 record. The pinned
    /// engine's native expansion drops that record, so the full-frame
    /// reference would otherwise render on the opposite side of the canvas.
    #[test]
    fn full_frame_component_preserves_nested_type_zero_outline() {
        const DIRECT: &str = "1:0:0:20:80:60:80$0:98:0:0:0:200:200";
        const REFERENCE: &str = "99:0:0:0:0:200:200:fixture:0:0:0";
        let mut components = empty_components();
        components.upsert(ComponentDefinition::new(
            "fixture",
            "fixture",
            std::iter::empty::<&str>(),
            DIRECT,
        ));

        let direct = render_outlines(DIRECT, &components, false, false);
        let referenced = render_outlines(REFERENCE, &components, false, false);

        assert_eq!(referenced, direct);
        assert_eq!(
            outline_bounds(&referenced),
            Some((140.0, 69.0, 180.0, 82.0))
        );
    }

    /// Malformed or unrelated SVG text is ignored instead of panicking.
    #[test]
    fn parser_ignores_incomplete_attributes() {
        assert!(parse_polygon_svg("<svg><polygon points=\"1,2 3\"").is_empty());
        assert!(parse_polygon_svg("not svg").is_empty());
    }

    #[test]
    fn curve_parser_flattens_quadratics_and_implicit_coordinates() {
        let outlines =
            parse_path_svg(r#"<svg><path d="M0,0 L10 0 10,10 Q15,20 20,10 25,0,30,10 Z" /></svg>"#);

        assert_eq!(outlines.len(), 1);
        assert_eq!(outlines[0].len(), 51);
        assert_eq!(outlines[0][0], (0.0, 0.0));
        assert_eq!(outlines[0][2], (10.0, 10.0));
        assert_eq!(outlines[0][14], (15.0, 15.0));
        assert_eq!(outlines[0][26], (20.0, 10.0));
        assert_eq!(outlines[0][50], (30.0, 10.0));
    }

    #[test]
    fn curve_parser_closes_kage_trailing_control_point() {
        let outlines = parse_path_svg(r#"<path d="M 0,0 L 10,0 Q 5,10 Z" />"#);

        assert_eq!(outlines.len(), 1);
        assert_eq!(outlines[0].len(), 26);
        assert_eq!(outlines[0].last(), Some(&(0.0, 0.0)));
    }

    #[test]
    fn curve_parser_ignores_malformed_paths() {
        let outlines = parse_path_svg(
            r#"<path d="M 0,0 Q 1,2" /><path d="M 0,0 C 1,2 3,4 5,6 Z" />
               <path d="M 0,0 L 10,0 0,10 Z" />"#,
        );

        assert_eq!(outlines, vec![vec![(0.0, 0.0), (10.0, 0.0), (0.0, 10.0)]]);
    }

    #[test]
    fn ming_curve_mode_changes_the_rendered_outline() {
        let source = "2:0:7:20:100:100:40:180:100";
        let components = empty_components();
        let polygonal = render_outlines(source, &components, false, false);
        let smooth = render_outlines(source, &components, false, true);

        assert!(!smooth.is_empty());
        assert_ne!(smooth, polygonal);
    }

    #[test]
    fn gothic_ignores_curve_mode() {
        let components = empty_components();
        let polygonal = render_outlines("1:0:0:20:100:180:100", &components, true, false);
        let smooth = render_outlines("1:0:0:20:100:180:100", &components, true, true);

        assert!(!smooth.is_empty());
        assert_eq!(smooth, polygonal);
    }
}
