use cadboss_core::{Error, SkipKind, SkipReason, Skipped};
use cadboss_dxf::{load, load_reporting, open};
use cadboss_testkit::dxf::{simple_drawing, DxfBuilder};

fn expected_counts() -> Vec<(String, u64)> {
    ["CIRCLE", "INSERT", "LINE", "TEXT"]
        .into_iter()
        .map(|name| (name.to_owned(), 1))
        .collect()
}

#[test]
fn document_counts_entities_per_type() -> Result<(), Error> {
    let document = load(simple_drawing())?;
    assert_eq!(document.metadata.entity_counts, expected_counts());
    assert_eq!(document.metadata.layout_count, 2);
    assert_eq!(
        document.metadata.layer_count as usize,
        document.layers.len()
    );
    assert_eq!(
        document.metadata.block_count as usize,
        document
            .blocks
            .iter()
            .filter(|block| !block.anonymous)
            .count()
    );
    Ok(())
}

#[test]
fn document_fills_layout_extents_and_counts() -> Result<(), Error> {
    let document = load(simple_drawing())?;
    let model = document
        .layouts
        .iter()
        .find(|layout| layout.name.as_str() == Some("Model"))
        .ok_or(Error::Empty)?;
    let extents = model.extents.ok_or(Error::Empty)?;
    assert!(extents.min.x <= 0.0 && extents.min.y <= 0.0);
    assert!(extents.max.x >= 65.0 && extents.max.y >= 40.0);
    assert_eq!(model.entity_count, 3);
    let paper = document
        .layouts
        .iter()
        .find(|layout| layout.name.as_str() == Some("Layout1"))
        .ok_or(Error::Empty)?;
    assert_eq!(paper.entity_count, 1);
    let titleblock = document
        .blocks
        .iter()
        .find(|block| block.name.as_str() == Some("TITLEBLOCK"))
        .ok_or(Error::Empty)?;
    assert_eq!(titleblock.entity_count, 2);
    assert!(document.metadata.extents.is_some());
    Ok(())
}

#[test]
fn document_report_includes_lexer_and_entity_skips() -> Result<(), Error> {
    let bytes = b"  0\nSECTION\n  2\nENTITIES\n  1O\nfoo\n  0\nWIPEOUT\n  0\nENDSEC\n  0\nEOF\n";
    let (document, report) = load_reporting(bytes.to_vec())?;
    for kind in [SkipKind::Token, SkipKind::Entity] {
        assert!(report
            .skipped
            .iter()
            .any(|entry: &Skipped| entry.kind == kind));
    }
    assert!(report
        .skipped
        .iter()
        .any(|entry| entry.kind == SkipKind::Entity && entry.reason == SkipReason::Unknown));
    assert_eq!(document.report, report);
    let unknown = report
        .skipped
        .iter()
        .find(|entry| entry.kind == SkipKind::Entity && entry.reason == SkipReason::Unknown)
        .ok_or(Error::Empty)?;
    assert_eq!(unknown.count, 1);
    Ok(())
}

#[test]
fn open_and_load_give_equal_documents_for_the_simple_fixture() -> Result<(), Error> {
    let path =
        std::env::temp_dir().join(format!("cadboss-reader-{}-simple.dxf", std::process::id()));
    std::fs::write(&path, simple_drawing())?;
    let opened = open(&path)?;
    let loaded = load(simple_drawing())?;
    std::fs::remove_file(&path)?;
    assert_eq!(opened, loaded);
    Ok(())
}

#[test]
fn entity_counts_sort_by_count_descending_then_name() -> Result<(), Error> {
    let bytes = DxfBuilder::new()
        .line((0.0, 0.0), (1.0, 1.0))
        .line((1.0, 1.0), (2.0, 2.0))
        .circle((0.0, 0.0), 1.0)
        .build();
    let document = load(bytes)?;
    let expected = vec![("LINE".to_owned(), 2), ("CIRCLE".to_owned(), 1)];
    assert_eq!(document.metadata.entity_counts, expected);
    Ok(())
}

#[test]
fn malformed_block_body_entity_is_reported_once() -> Result<(), Error> {
    let bytes = DxfBuilder::new()
        .block("B", (0.0, 0.0))
        .raw(0, "LINE")
        .raw(10, "1.0")
        .end()
        .build();
    let report = load_reporting(bytes)?.1;
    let malformed = report
        .skipped
        .iter()
        .find(|entry| entry.kind == SkipKind::Entity && entry.reason == SkipReason::Malformed)
        .ok_or(Error::Empty)?;
    assert_eq!(malformed.count, 1);
    Ok(())
}

fn noted(report: &cadboss_core::Report, kind: SkipKind, reason: SkipReason) -> u64 {
    report
        .skipped
        .iter()
        .filter(|entry| entry.kind == kind && entry.reason == reason)
        .map(|entry| entry.count)
        .sum()
}

#[test]
fn document_decodes_the_entities_section_once_across_twenty_layouts() -> Result<(), Error> {
    let mut builder = DxfBuilder::new()
        .raw(0, "WIPEOUT")
        .raw(8, "0")
        .raw(0, "SEQEND")
        .line((0.0, 0.0), (1.0, 1.0));
    for sheet in 1..=18 {
        builder = builder
            .paper_layout(&format!("Sheet{sheet}"))
            .text(&format!("Sheet {sheet}"), (10.0, 10.0), 2.5)
            .end();
    }
    let document = load(builder.build())?;
    assert_eq!(document.layouts.len(), 20);
    assert_eq!(
        noted(&document.report, SkipKind::Entity, SkipReason::Unknown),
        1
    );
    assert_eq!(
        noted(&document.report, SkipKind::Entity, SkipReason::Malformed),
        1
    );
    let model = document
        .layouts
        .iter()
        .find(|layout| layout.name.as_str() == Some("Model"))
        .ok_or(Error::Empty)?;
    assert_eq!(model.entity_count, 2);
    assert!(model.extents.is_some());
    Ok(())
}

#[test]
fn a_paper_layout_body_is_decoded_once_for_the_report() -> Result<(), Error> {
    let bytes = DxfBuilder::new()
        .paper_layout("Sheet2")
        .raw(0, "WIPEOUT")
        .raw(8, "0")
        .raw(0, "LINE")
        .raw(8, "0")
        .raw(10, "0.0")
        .raw(20, "0.0")
        .end()
        .build();
    let document = load(bytes)?;
    assert_eq!(
        noted(&document.report, SkipKind::Entity, SkipReason::Unknown),
        1
    );
    assert_eq!(
        noted(&document.report, SkipKind::Entity, SkipReason::Malformed),
        1
    );
    let sheet = document
        .layouts
        .iter()
        .find(|layout| layout.name.as_str() == Some("Sheet2"))
        .ok_or(Error::Empty)?;
    assert_eq!(sheet.entity_count, 1);
    let body = document
        .blocks
        .iter()
        .find(|block| block.name.as_str() == Some("*Paper_Space0"))
        .ok_or(Error::Empty)?;
    assert_eq!(body.entity_count, 1);
    Ok(())
}

#[test]
fn a_stray_seqend_is_reported_once_with_two_layouts() -> Result<(), Error> {
    let bytes = DxfBuilder::new()
        .raw(0, "SEQEND")
        .raw(8, "0")
        .line((0.0, 0.0), (1.0, 1.0))
        .build();
    let document = load(bytes)?;
    assert_eq!(document.layouts.len(), 2);
    assert_eq!(
        noted(&document.report, SkipKind::Entity, SkipReason::Malformed),
        1
    );
    Ok(())
}

#[test]
fn polylines_and_lightweight_polylines_count_under_their_own_names() -> Result<(), Error> {
    let bytes = DxfBuilder::new()
        .lwpolyline(&[(0.0, 0.0, 0.0), (1.0, 1.0, 0.0)], false)
        .raw(0, "POLYLINE")
        .raw(8, "0")
        .raw(0, "VERTEX")
        .raw(10, "0.0")
        .raw(20, "0.0")
        .raw(0, "VERTEX")
        .raw(10, "2.0")
        .raw(20, "1.0")
        .raw(0, "SEQEND")
        .build();
    let document = load(bytes)?;
    assert_eq!(
        document.metadata.entity_counts,
        vec![("LWPOLYLINE".to_owned(), 1), ("POLYLINE".to_owned(), 1)]
    );
    Ok(())
}

#[test]
fn missing_header_extents_fall_back_to_the_model_layout_by_kind() -> Result<(), Error> {
    let pairs: [(i16, &str); 24] = [
        (0, "SECTION"),
        (2, "TABLES"),
        (0, "TABLE"),
        (2, "BLOCK_RECORD"),
        (0, "BLOCK_RECORD"),
        (5, "A"),
        (2, "*Model_Space"),
        (0, "ENDTAB"),
        (0, "ENDSEC"),
        (0, "SECTION"),
        (2, "ENTITIES"),
        (0, "LINE"),
        (10, "2"),
        (20, "3"),
        (11, "7"),
        (21, "9"),
        (0, "ENDSEC"),
        (0, "SECTION"),
        (2, "OBJECTS"),
        (0, "LAYOUT"),
        (1, "Modell"),
        (330, "A"),
        (0, "ENDSEC"),
        (0, "EOF"),
    ];
    let text: String = pairs
        .iter()
        .map(|(code, value)| format!("{code}\n{value}\n"))
        .collect();
    let document = load(text.into_bytes())?;
    assert_eq!(document.layouts[0].name.as_str(), Some("Modell"));
    let extents = document.metadata.extents.ok_or(Error::Empty)?;
    assert_eq!((extents.min.x, extents.max.y), (2.0, 9.0));
    Ok(())
}
