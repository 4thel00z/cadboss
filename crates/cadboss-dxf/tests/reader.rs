use cadboss_core::{Error, SkipKind, SkipReason, Skipped};
use cadboss_dxf::{load, load_reporting, open};
use cadboss_testkit::dxf::simple_drawing;

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
        document.blocks.len()
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
    Ok(())
}

#[test]
fn open_and_load_give_equal_documents_for_the_simple_fixture() -> Result<(), Error> {
    let directory = std::env::temp_dir().join("cadboss-reader-test");
    std::fs::create_dir_all(&directory)?;
    let path = directory.join("simple.dxf");
    std::fs::write(&path, simple_drawing())?;
    let opened = open(&path)?;
    let loaded = load(simple_drawing())?;
    assert_eq!(opened, loaded);
    Ok(())
}
