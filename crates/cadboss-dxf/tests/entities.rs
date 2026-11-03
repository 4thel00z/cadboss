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
    let (tape, _) = Tape::lex(bytes, &Budget::default())?;
    let section = tape.section(SectionName::Entities).ok_or(Error::Empty)?;
    let cursor = cadboss_dxf::Cursor::new(&tape, bytes, section.start..section.end);
    let layout = |_: Option<u64>, _: bool| Text::from("Model");
    let mut decoder = EntityDecoder::new(cursor, Codepage::Utf8, budget, &layout);
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
        EntityKind::Dimension { text, measurement, dim_type, block_name, style }
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
