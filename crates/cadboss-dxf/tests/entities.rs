use cadboss_core::{Budget, Error, Point, Report, SkipKind, SkipReason, Skipped};
use cadboss_dxf::{Codepage, EntityDecoder, SectionName, Tape};
use cadboss_model::{Entity, EntityKind, Text};
use cadboss_testkit::dxf::{large_drawing, DxfBuilder};

macro_rules! variant {
    ($entity:expr, $pattern:pat => $value:expr) => {
        match &$entity.kind {
            $pattern => Some($value),
            _ => None,
        }
        .unwrap()
    };
}

fn decode_with(bytes: &[u8], budget: &Budget) -> Result<(Vec<Entity>, Report), Error> {
    decode_laid_out(bytes, budget, &|_, _| Text::from("Model"))
}

fn decode_laid_out(
    bytes: &[u8],
    budget: &Budget,
    layout: &dyn Fn(Option<u64>, bool) -> Text,
) -> Result<(Vec<Entity>, Report), Error> {
    let (tape, _) = Tape::lex(bytes, &Budget::default())?;
    let section = tape.section(SectionName::Entities).ok_or(Error::Empty)?;
    let cursor = cadboss_dxf::Cursor::new(&tape, bytes, section.start..section.end);
    let mut decoder = EntityDecoder::new(cursor, Codepage::Utf8, budget, layout);
    let entities: Vec<Entity> = decoder.by_ref().collect();
    Ok((entities, decoder.into_report()))
}

fn decode_all(bytes: &[u8]) -> Result<(Vec<Entity>, Report), Error> {
    decode_with(bytes, &Budget::default())
}

#[test]
fn decoder_reads_line_circle_arc_and_polyline_from_builder_output() {
    let bytes = DxfBuilder::new()
        .line((1.0, 2.0), (3.0, 4.0))
        .circle((5.0, 6.0), 7.0)
        .arc((0.0, 0.0), 2.0, 10.0, 80.0)
        .lwpolyline(&[(0.0, 0.0, 0.0), (4.0, 0.0, 0.5), (4.0, 3.0, 0.0)], true)
        .build();
    let (entities, report) = decode_all(&bytes).unwrap();
    assert!(report.is_empty());
    assert_eq!(entities.len(), 4);
    assert_eq!(
        entities[0].kind,
        EntityKind::Line {
            start: Point::xy(1.0, 2.0),
            end: Point::xy(3.0, 4.0)
        }
    );
    assert_eq!(
        entities[1].kind,
        EntityKind::Circle {
            center: Point::xy(5.0, 6.0),
            radius: 7.0
        }
    );
    assert_eq!(
        entities[2].kind,
        EntityKind::Arc {
            center: Point::xy(0.0, 0.0),
            radius: 2.0,
            start: 10.0,
            end: 80.0
        }
    );
    assert_eq!(
        entities[3].kind,
        EntityKind::Polyline {
            points: vec![
                Point::xy(0.0, 0.0),
                Point::xy(4.0, 0.0),
                Point::xy(4.0, 3.0)
            ],
            bulges: vec![0.0, 0.5, 0.0],
            closed: true
        }
    );
    assert_eq!(entities[0].common.layer, Text::from("0"));
    assert_eq!(entities[0].common.layout, Text::from("Model"));
    assert_eq!(entities[0].common.color, 256);
    assert!(entities[0].common.handle.is_some());
}

#[test]
fn decoder_joins_mtext_chunks_in_order() {
    let original: String = (0..300)
        .map(|index| char::from(b'a' + u8::try_from(index % 26).unwrap()))
        .collect();
    let bytes = DxfBuilder::new().mtext(&original, (0.0, 0.0), 2.5).build();
    let (entities, _) = decode_all(&bytes).unwrap();
    let (text, height) =
        variant!(entities[0], EntityKind::MText { text, height, .. } => (text, *height));
    assert_eq!(text, &Text::from(original));
    assert_eq!(height, 2.5);
}

#[test]
fn mtext_strips_format_codes_and_keeps_text() {
    let bytes = DxfBuilder::new()
        .mtext("{\\fArial|b1;Hello} \\PWorld %%d", (0.0, 0.0), 2.5)
        .build();
    let (entities, _) = decode_all(&bytes).unwrap();
    let (text,) = variant!(entities[0], EntityKind::MText { text, .. } => (text,));
    assert_eq!(text, &Text::from("Hello \nWorld \u{b0}"));
}

#[test]
fn mtext_with_unbalanced_braces_still_yields_text() {
    let bytes = DxfBuilder::new()
        .mtext("{\\fArial|b1; unterminated", (0.0, 0.0), 2.5)
        .build();
    let (entities, _) = decode_all(&bytes).unwrap();
    let (text,) = variant!(entities[0], EntityKind::MText { text, .. } => (text,));
    assert_eq!(text, &Text::from(" unterminated"));
}

#[test]
fn insert_collects_attribs_until_seqend() {
    let bytes = DxfBuilder::new()
        .insert_with_attribs("TITLE", (10.0, 20.0), &[("NAME", "Pump"), ("REV", "B")])
        .line((0.0, 0.0), (1.0, 1.0))
        .build();
    let (entities, report) = decode_all(&bytes).unwrap();
    assert!(report.is_empty());
    assert_eq!(entities.len(), 2);
    let (block_name, position, scale, attributes) = variant!(
        entities[0],
        EntityKind::Insert { block_name, position, scale, attributes, .. }
            => (block_name, *position, *scale, attributes)
    );
    assert_eq!(block_name, &Text::from("TITLE"));
    assert_eq!(position, Point::xy(10.0, 20.0));
    assert_eq!(scale, Point::new(1.0, 1.0, 1.0));
    assert_eq!(attributes.len(), 2);
    assert_eq!(attributes[0].tag, Text::from("NAME"));
    assert_eq!(attributes[0].value, Text::from("Pump"));
    assert_eq!(attributes[1].tag, Text::from("REV"));
    assert_eq!(attributes[1].value, Text::from("B"));
    assert_eq!(attributes[0].height, 2.5);
    assert!(matches!(entities[1].kind, EntityKind::Line { .. }));
}

#[test]
fn dimension_keeps_override_text_and_measurement() {
    let bytes = DxfBuilder::new()
        .dimension("<>mm", 42.5)
        .dimension("", 7.0)
        .build();
    let (entities, _) = decode_all(&bytes).unwrap();
    let (text, measurement, dim_type, block_name, style) = variant!(
        entities[0],
        EntityKind::Dimension { text, measurement, dim_type, block_name, style, .. }
            => (text, *measurement, *dim_type, block_name, style)
    );
    assert_eq!(text, &Text::from("<>mm"));
    assert_eq!(measurement, Some(42.5));
    assert_eq!(dim_type, 0);
    assert_eq!(block_name, &Text::from("*D1"));
    assert_eq!(style, &Text::from("Standard"));
    let (text,) = variant!(entities[1], EntityKind::Dimension { text, .. } => (text,));
    assert!(text.is_empty());
}

#[test]
fn unknown_entity_is_kept_as_unknown_and_reported_once_per_type() {
    let bytes = DxfBuilder::new()
        .raw(0, "WIPEOUT")
        .raw(0, "WIPEOUT")
        .build();
    let (entities, report) = decode_all(&bytes).unwrap();
    assert_eq!(entities.len(), 2);
    for entity in &entities {
        assert_eq!(
            entity.kind,
            EntityKind::Unknown {
                type_name: Text::from("WIPEOUT")
            }
        );
    }
    assert_eq!(
        report.skipped,
        vec![Skipped {
            kind: SkipKind::Entity,
            reason: SkipReason::Unknown,
            count: 2
        }]
    );
}

#[test]
fn line_without_endpoint_is_skipped_as_malformed() {
    let bytes = DxfBuilder::new()
        .raw(0, "LINE")
        .raw(10, "1.0")
        .raw(20, "1.0")
        .circle((0.0, 0.0), 1.0)
        .build();
    let (entities, report) = decode_all(&bytes).unwrap();
    assert_eq!(entities.len(), 1);
    assert!(matches!(entities[0].kind, EntityKind::Circle { .. }));
    assert_eq!(
        report.skipped,
        vec![Skipped {
            kind: SkipKind::Entity,
            reason: SkipReason::Malformed,
            count: 1
        }]
    );
}

#[test]
fn entity_limit_ends_iteration_with_a_report() {
    let bytes = large_drawing(100);
    let budget = Budget {
        max_entities: 10,
        ..Budget::default()
    };
    let (entities, report) = decode_with(&bytes, &budget).unwrap();
    assert_eq!(entities.len(), 10);
    assert_eq!(
        report.skipped,
        vec![Skipped {
            kind: SkipKind::Entity,
            reason: SkipReason::EntityLimit,
            count: 1
        }]
    );
}

#[test]
fn string_past_the_limit_is_truncated_and_reported() {
    let bytes = DxfBuilder::new()
        .text("abcdefghijklmnopqrstuvwxyz", (0.0, 0.0), 2.5)
        .build();
    let budget = Budget {
        max_string_bytes: 8,
        ..Budget::default()
    };
    let (entities, report) = decode_with(&bytes, &budget).unwrap();
    let (text,) = variant!(entities[0], EntityKind::Text { text, .. } => (text,));
    assert_eq!(text, &Text::from("abcdefgh"));
    assert_eq!(
        report.skipped,
        vec![Skipped {
            kind: SkipKind::Text,
            reason: SkipReason::StringLimit,
            count: 1
        }]
    );
}

#[test]
fn polyline_collects_vertices_until_seqend() {
    let bytes = DxfBuilder::new()
        .raw(0, "POLYLINE")
        .raw(70, "1")
        .raw(0, "VERTEX")
        .raw(10, "0.0")
        .raw(20, "0.0")
        .raw(42, "0.5")
        .raw(0, "VERTEX")
        .raw(10, "2.0")
        .raw(20, "1.0")
        .raw(0, "SEQEND")
        .circle((0.0, 0.0), 1.0)
        .build();
    let (entities, report) = decode_all(&bytes).unwrap();
    assert!(report.is_empty());
    assert_eq!(entities.len(), 2);
    assert_eq!(
        entities[0].kind,
        EntityKind::Polyline {
            points: vec![Point::xy(0.0, 0.0), Point::xy(2.0, 1.0)],
            bulges: vec![0.5, 0.0],
            closed: true
        }
    );
}

fn only(report: &Report, kind: SkipKind, reason: SkipReason, count: u64) -> bool {
    report.skipped
        == vec![Skipped {
            kind,
            reason,
            count,
        }]
}

#[test]
fn reactor_group_does_not_leak_into_the_owner_handle() {
    let bytes = DxfBuilder::new()
        .raw(0, "LINE")
        .raw(5, "1A")
        .raw(102, "{ACAD_REACTORS")
        .raw(330, "AB")
        .raw(102, "}")
        .raw(330, "1F")
        .raw(67, "1")
        .raw(10, "0.0")
        .raw(20, "0.0")
        .raw(11, "1.0")
        .raw(21, "1.0")
        .build();
    let seen = std::cell::RefCell::new(Vec::new());
    let layout = |owner: Option<u64>, paper: bool| {
        seen.borrow_mut().push((owner, paper));
        Text::from("Layout1")
    };
    let (entities, _) = decode_laid_out(&bytes, &Budget::default(), &layout).unwrap();
    assert_eq!(entities.len(), 1);
    assert_eq!(entities[0].common.handle, Some(0x1A));
    assert_eq!(*seen.borrow(), vec![(Some(0x1F), true)]);
}

#[test]
fn text_value_cut_at_the_limit_reports_once_and_keeps_the_limit_length() {
    let bytes = DxfBuilder::new()
        .mtext(&"x".repeat(300), (0.0, 0.0), 2.5)
        .build();
    let budget = Budget {
        max_string_bytes: 18,
        ..Budget::default()
    };
    let (entities, report) = decode_with(&bytes, &budget).unwrap();
    let text = variant!(entities[0], EntityKind::MText { text, .. } => text);
    assert_eq!(text, &Text::from("x".repeat(18)));
    assert!(only(&report, SkipKind::Text, SkipReason::StringLimit, 1));
}

#[test]
fn multibyte_text_is_cut_on_a_character_boundary() {
    let bytes = DxfBuilder::new()
        .text("\u{e9}\u{e9}\u{e9}", (0.0, 0.0), 2.5)
        .build();
    let budget = Budget {
        max_string_bytes: 3,
        ..Budget::default()
    };
    let (entities, report) = decode_with(&bytes, &budget).unwrap();
    let text = variant!(entities[0], EntityKind::Text { text, .. } => text);
    assert_eq!(text, &Text::from("\u{e9}"));
    assert!(only(&report, SkipKind::Text, SkipReason::StringLimit, 1));
}

#[test]
fn unknown_type_name_goes_through_the_string_limit() {
    let bytes = DxfBuilder::new().raw(0, "ABCDEFGHIJKL").build();
    let budget = Budget {
        max_string_bytes: 4,
        ..Budget::default()
    };
    let (entities, _) = decode_with(&bytes, &budget).unwrap();
    let name = variant!(entities[0], EntityKind::Unknown { type_name } => type_name);
    assert_eq!(name, &Text::from("ABCD"));
}

#[test]
fn bad_vertex_coordinate_drops_that_vertex_only() {
    let bytes = DxfBuilder::new()
        .raw(0, "LWPOLYLINE")
        .raw(42, "0.9")
        .raw(10, "0.0")
        .raw(20, "0.0")
        .raw(10, "oops")
        .raw(20, "5.0")
        .raw(42, "0.7")
        .raw(10, "2.0")
        .raw(20, "3.0")
        .raw(42, "0.25")
        .build();
    let (entities, report) = decode_all(&bytes).unwrap();
    let (points, bulges) =
        variant!(entities[0], EntityKind::Polyline { points, bulges, .. } => (points, bulges));
    assert_eq!(points, &vec![Point::xy(0.0, 0.0), Point::xy(2.0, 3.0)]);
    assert_eq!(bulges, &vec![0.0, 0.25]);
    assert!(only(&report, SkipKind::Entity, SkipReason::Malformed, 1));
}

#[test]
fn stray_vertex_attrib_and_seqend_are_reported_as_malformed() {
    let bytes = DxfBuilder::new()
        .raw(0, "VERTEX")
        .raw(0, "ATTRIB")
        .raw(0, "SEQEND")
        .circle((0.0, 0.0), 1.0)
        .build();
    let (entities, report) = decode_all(&bytes).unwrap();
    assert_eq!(entities.len(), 1);
    assert!(only(&report, SkipKind::Entity, SkipReason::Malformed, 3));
}

#[test]
fn attribs_attach_without_the_attribs_follow_flag() {
    let bytes = DxfBuilder::new()
        .raw(0, "INSERT")
        .raw(2, "B")
        .raw(10, "0.0")
        .raw(20, "0.0")
        .raw(0, "ATTRIB")
        .raw(2, "TAG")
        .raw(1, "value")
        .raw(0, "SEQEND")
        .build();
    let (entities, _) = decode_all(&bytes).unwrap();
    let attributes = variant!(entities[0], EntityKind::Insert { attributes, .. } => attributes);
    assert_eq!(attributes.len(), 1);
}

#[test]
fn ellipse_spline_hatch_and_point_decode_their_fields() {
    let bytes = DxfBuilder::new()
        .raw(0, "ELLIPSE")
        .raw(10, "1.0")
        .raw(20, "2.0")
        .raw(11, "3.0")
        .raw(21, "0.0")
        .raw(40, "0.5")
        .raw(0, "SPLINE")
        .raw(70, "1")
        .raw(71, "2")
        .raw(10, "0.0")
        .raw(20, "0.0")
        .raw(10, "1.0")
        .raw(20, "1.0")
        .raw(11, "0.5")
        .raw(21, "0.5")
        .raw(0, "HATCH")
        .raw(2, "SOLID")
        .raw(70, "1")
        .raw(91, "2")
        .raw(0, "POINT")
        .raw(10, "7.0")
        .raw(20, "8.0")
        .build();
    let (entities, report) = decode_all(&bytes).unwrap();
    assert!(report.is_empty());
    assert_eq!(
        entities[0].kind,
        EntityKind::Ellipse {
            center: Point::xy(1.0, 2.0),
            major: Point::xy(3.0, 0.0),
            ratio: 0.5,
            start: 0.0,
            end: std::f64::consts::TAU
        }
    );
    assert_eq!(
        entities[1].kind,
        EntityKind::Spline {
            control: vec![Point::xy(0.0, 0.0), Point::xy(1.0, 1.0)],
            fit: vec![Point::xy(0.5, 0.5)],
            degree: 2,
            closed: true
        }
    );
    assert_eq!(
        entities[2].kind,
        EntityKind::Hatch {
            pattern: Text::from("SOLID"),
            solid: true,
            loops: 2
        }
    );
    assert_eq!(
        entities[3].kind,
        EntityKind::Point {
            position: Point::xy(7.0, 8.0)
        }
    );
}

#[test]
fn solid_image_and_viewport_decode_their_fields() {
    let bytes = DxfBuilder::new()
        .raw(0, "SOLID")
        .raw(10, "0.0")
        .raw(20, "0.0")
        .raw(11, "1.0")
        .raw(21, "0.0")
        .raw(12, "0.0")
        .raw(22, "1.0")
        .raw(13, "1.0")
        .raw(23, "1.0")
        .raw(0, "IMAGE")
        .raw(10, "5.0")
        .raw(20, "6.0")
        .raw(13, "640.0")
        .raw(23, "480.0")
        .raw(340, "2b")
        .raw(0, "VIEWPORT")
        .raw(10, "1.0")
        .raw(20, "2.0")
        .raw(40, "10.0")
        .raw(41, "5.0")
        .raw(12, "3.0")
        .raw(22, "4.0")
        .raw(45, "20.0")
        .raw(331, "0a")
        .raw(331, "FF")
        .build();
    let (entities, report) = decode_all(&bytes).unwrap();
    assert!(report.is_empty());
    let points = variant!(entities[0], EntityKind::Solid { points } => points);
    assert_eq!(points.len(), 4);
    assert_eq!(
        entities[1].kind,
        EntityKind::Image {
            path: None,
            image_def: Some(0x2B),
            position: Point::xy(5.0, 6.0),
            size: Point::xy(640.0, 480.0)
        }
    );
    assert_eq!(
        entities[2].kind,
        EntityKind::Viewport {
            center: Point::xy(1.0, 2.0),
            width: 10.0,
            height: 5.0,
            view_center: Point::xy(3.0, 4.0),
            view_height: 20.0,
            frozen_layers: vec![Text::from("A"), Text::from("FF")]
        }
    );
}

#[test]
fn dimension_type_is_masked_to_its_low_nibble() {
    let bytes = DxfBuilder::new().raw(0, "DIMENSION").raw(70, "33").build();
    let (entities, _) = decode_all(&bytes).unwrap();
    let dim_type = variant!(entities[0], EntityKind::Dimension { dim_type, .. } => *dim_type);
    assert_eq!(dim_type, 1);
}

#[test]
fn polyline_without_seqend_does_not_swallow_the_next_entity() {
    let bytes = DxfBuilder::new()
        .raw(0, "POLYLINE")
        .raw(0, "VERTEX")
        .raw(10, "1.0")
        .raw(20, "1.0")
        .raw(0, "LINE")
        .raw(10, "0.0")
        .raw(20, "0.0")
        .raw(11, "1.0")
        .raw(21, "1.0")
        .build();
    let (entities, _) = decode_all(&bytes).unwrap();
    assert_eq!(entities.len(), 2);
    assert!(matches!(entities[1].kind, EntityKind::Line { .. }));
}

#[test]
fn insert_announcing_attribs_without_any_keeps_the_next_line() {
    let bytes = DxfBuilder::new()
        .raw(0, "INSERT")
        .raw(66, "1")
        .raw(2, "B")
        .raw(10, "0.0")
        .raw(20, "0.0")
        .line((0.0, 0.0), (1.0, 1.0))
        .build();
    let (entities, report) = decode_all(&bytes).unwrap();
    assert!(report.is_empty());
    assert_eq!(entities.len(), 2);
    assert!(matches!(entities[1].kind, EntityKind::Line { .. }));
}

#[test]
fn mtext_with_only_chunk_codes_still_decodes() {
    let bytes = DxfBuilder::new()
        .raw(0, "MTEXT")
        .raw(10, "0.0")
        .raw(20, "0.0")
        .raw(3, "abc")
        .raw(3, "def")
        .build();
    let (entities, _) = decode_all(&bytes).unwrap();
    let text = variant!(entities[0], EntityKind::MText { text, .. } => text);
    assert_eq!(text, &Text::from("abcdef"));
}

#[test]
fn hyperlink_xdata_is_kept_on_the_entity() {
    let bytes = DxfBuilder::new()
        .text("Spec", (0.0, 0.0), 2.5)
        .hyperlink("https://example.com/spec")
        .line((0.0, 0.0), (1.0, 1.0))
        .build();
    let (entities, _) = decode_all(&bytes).unwrap();
    assert_eq!(
        entities[0].common.hyperlink,
        Some(Text::from("https://example.com/spec"))
    );
    assert_eq!(entities[1].common.hyperlink, None);
}

#[test]
fn foreign_xdata_without_a_url_leaves_the_hyperlink_empty() {
    let bytes = DxfBuilder::new()
        .line((0.0, 0.0), (1.0, 1.0))
        .raw(1001, "ACAD")
        .build();
    let (entities, _) = decode_all(&bytes).unwrap();
    assert_eq!(entities[0].common.hyperlink, None);
}

#[test]
fn lwpolyline_with_200_000_vertices_fills_exactly_sized_point_lists() {
    let corners: Vec<(f64, f64, f64)> = (0..200_000)
        .map(|index| (f64::from(index), f64::from(index % 7), 0.0))
        .collect();
    let bytes = DxfBuilder::new().lwpolyline(&corners, false).build();
    let (entities, report) = decode_all(&bytes).unwrap();
    assert!(report.is_empty());
    let (points, bulges) =
        variant!(entities[0], EntityKind::Polyline { points, bulges, .. } => (points, bulges));
    assert_eq!(points.len(), 200_000);
    assert_eq!(points.capacity(), 200_000);
    assert_eq!(bulges.capacity(), 200_000);
    assert_eq!(points[199_999], Point::xy(199_999.0, 2.0));
}

#[test]
fn lwpolyline_vertex_count_cannot_reserve_past_the_record() {
    let bytes = DxfBuilder::new()
        .raw(0, "LWPOLYLINE")
        .raw(90, "2000000000")
        .raw(10, "0.0")
        .raw(20, "0.0")
        .raw(10, "1.0")
        .raw(20, "1.0")
        .build();
    let (entities, _) = decode_all(&bytes).unwrap();
    let points = variant!(entities[0], EntityKind::Polyline { points, .. } => points);
    assert_eq!(points.len(), 2);
    assert!(points.capacity() <= 3);
}

#[test]
fn polyline_vertices_inside_a_reactor_group_are_read_in_place() {
    let bytes = DxfBuilder::new()
        .raw(0, "POLYLINE")
        .raw(102, "{ACAD_REACTORS")
        .raw(330, "AB")
        .raw(102, "}")
        .raw(0, "VERTEX")
        .raw(102, "{ACAD_REACTORS")
        .raw(10, "9.0")
        .raw(102, "}")
        .raw(10, "1.0")
        .raw(20, "2.0")
        .raw(0, "SEQEND")
        .build();
    let (entities, report) = decode_all(&bytes).unwrap();
    assert!(report.is_empty());
    let points = variant!(entities[0], EntityKind::Polyline { points, .. } => points);
    assert_eq!(points, &vec![Point::xy(1.0, 2.0)]);
}

#[test]
fn non_finite_coordinates_count_as_missing() {
    for builder in [DxfBuilder::new(), DxfBuilder::new().binary()] {
        let bytes = builder
            .raw(0, "LINE")
            .raw(10, "inf")
            .raw(20, "0")
            .raw(11, "1")
            .raw(21, "1")
            .raw(0, "CIRCLE")
            .raw(10, "0")
            .raw(20, "0")
            .raw(40, "NaN")
            .raw(0, "POINT")
            .raw(10, "1e400")
            .raw(20, "0")
            .raw(0, "POINT")
            .raw(10, "-inf")
            .raw(20, "-inf")
            .build();
        let (entities, report) = decode_all(&bytes).unwrap();
        assert!(entities.is_empty());
        assert!(only(&report, SkipKind::Entity, SkipReason::Malformed, 4));
    }
}
