use cadboss_core::{Error, Point, SkipKind, SkipReason, Skipped};
use cadboss_dxf::{DxfDocument, Options};
use cadboss_model::{Entity, EntityKind, LayoutKind, Text};
use cadboss_testkit::dxf::{multi_layout, nested_inserts, simple_drawing, DxfBuilder};

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-9
}

fn names(texts: Vec<&Text>) -> Vec<String> {
    texts.iter().map(|text| text.to_string()).collect()
}

#[test]
fn document_opens_the_simple_fixture_and_lists_layers_blocks_and_layouts() -> Result<(), Error> {
    let document = DxfDocument::load(simple_drawing())?;
    assert_eq!(document.layers().len(), 2);
    let blocks: Vec<&Text> = document.blocks().map(|block| &block.name).collect();
    assert!(names(blocks).contains(&"TITLEBLOCK".to_string()));
    let layouts: Vec<&Text> = document
        .layouts()
        .iter()
        .map(|layout| &layout.name)
        .collect();
    assert_eq!(names(layouts), ["Model", "Layout1"]);
    assert_eq!(document.layouts()[0].kind, LayoutKind::Model);
    assert_eq!(document.layouts()[1].kind, LayoutKind::Paper);
    assert!(document.report().is_empty());
    Ok(())
}

#[test]
fn entities_of_model_space_exclude_paper_entities() -> Result<(), Error> {
    let document = DxfDocument::load(simple_drawing())?;
    let model: Vec<Entity> = document.entities("Model").collect();
    let paper: Vec<Entity> = document.entities("Layout1").collect();
    assert_eq!(model.len(), 3);
    assert!(model
        .iter()
        .all(|entity| entity.common.layout == Text::from("Model")));
    assert_eq!(paper.len(), 1);
    assert!(matches!(paper[0].kind, EntityKind::Insert { .. }));
    assert_eq!(document.entities("Nowhere").count(), 0);
    Ok(())
}

#[test]
fn block_entities_cover_exactly_the_block_body() -> Result<(), Error> {
    let document = DxfDocument::load(simple_drawing())?;
    let body: Vec<Entity> = document
        .block_entities("TITLEBLOCK")
        .ok_or(Error::Empty)?
        .collect();
    assert_eq!(body.len(), 2);
    assert!(body
        .iter()
        .all(|entity| !matches!(entity.kind, EntityKind::Unknown { .. })));
    assert!(document.block_entities("Missing").is_none());
    Ok(())
}

#[test]
fn walk_expands_inserts_with_the_block_transform() -> Result<(), Error> {
    let bytes = DxfBuilder::new()
        .block("A", (0.0, 0.0))
        .line((0.0, 0.0), (1.0, 0.0))
        .end()
        .insert_transformed("A", (10.0, 10.0), (1.0, 1.0, 1.0), 90.0)
        .build();
    let document = DxfDocument::load(bytes)?;
    let mut ends = Vec::new();
    let report = document.walk("Model", &mut |entity, transform, depth| {
        if let EntityKind::Line { end, .. } = &entity.kind {
            ends.push((transform.apply(*end), depth));
        }
    });
    assert!(report.is_empty());
    assert_eq!(ends.len(), 1);
    assert!(close(ends[0].0.x, 10.0) && close(ends[0].0.y, 11.0));
    assert_eq!(ends[0].1, 1);
    Ok(())
}

#[test]
fn walk_subtracts_the_block_base_point() -> Result<(), Error> {
    let bytes = DxfBuilder::new()
        .block("A", (5.0, 5.0))
        .line((5.0, 5.0), (6.0, 5.0))
        .end()
        .insert("A", (100.0, 0.0))
        .build();
    let document = DxfDocument::load(bytes)?;
    let extents = document.layout_extents("Model").ok_or(Error::Empty)?;
    assert!(close(extents.min.x, 100.0) && close(extents.max.x, 101.0));
    assert!(close(extents.min.y, 0.0) && close(extents.max.y, 0.0));
    Ok(())
}

#[test]
fn walk_stops_at_the_nesting_limit_and_reports() -> Result<(), Error> {
    let document = DxfDocument::load(nested_inserts(70))?;
    let mut deepest = 0;
    let report = document.walk("Model", &mut |_, _, depth| deepest = deepest.max(depth));
    assert_eq!(deepest, 63);
    assert!(report.skipped.contains(&Skipped {
        kind: SkipKind::Entity,
        reason: SkipReason::NestingLimit,
        count: 1
    }));
    Ok(())
}

#[test]
fn walk_detects_a_self_inserting_block() -> Result<(), Error> {
    let bytes = DxfBuilder::new()
        .block("A", (0.0, 0.0))
        .insert("A", (1.0, 1.0))
        .end()
        .insert("A", (0.0, 0.0))
        .build();
    let document = DxfDocument::load(bytes)?;
    let mut visited = 0;
    let report = document.walk("Model", &mut |_, _, _| visited += 1);
    assert_eq!(visited, 2);
    assert!(report.skipped.contains(&Skipped {
        kind: SkipKind::Entity,
        reason: SkipReason::Malformed,
        count: 1
    }));
    Ok(())
}

#[test]
fn walk_reports_an_insert_of_an_unknown_block() -> Result<(), Error> {
    let bytes = DxfBuilder::new().insert("Ghost", (0.0, 0.0)).build();
    let document = DxfDocument::load(bytes)?;
    let report = document.walk("Model", &mut |_, _, _| {});
    assert!(report.skipped.contains(&Skipped {
        kind: SkipKind::Entity,
        reason: SkipReason::Unknown,
        count: 1
    }));
    Ok(())
}

#[test]
fn layout_extents_include_text_positions_and_none_for_an_empty_layout() -> Result<(), Error> {
    let bytes = DxfBuilder::new().text("Only", (3.0, 4.0), 2.0).build();
    let document = DxfDocument::load(bytes)?;
    let extents = document.layout_extents("Model").ok_or(Error::Empty)?;
    assert_eq!(extents.min, Point::xy(3.0, 4.0));
    assert!(document.layout_extents("Layout1").is_none());
    Ok(())
}

#[test]
fn xref_blocks_carry_their_path_and_anonymous_blocks_are_flagged() -> Result<(), Error> {
    let bytes = DxfBuilder::new()
        .xref_block("EXT", "other.dwg")
        .block("*U1", (0.0, 0.0))
        .end()
        .build();
    let document = DxfDocument::load(bytes)?;
    let blocks: Vec<_> = document.blocks().collect();
    let external = blocks
        .iter()
        .find(|block| block.name == Text::from("EXT"))
        .ok_or(Error::Empty)?;
    assert_eq!(external.xref_path, Some(Text::from("other.dwg")));
    let anonymous = blocks
        .iter()
        .find(|block| block.name == Text::from("*U1"))
        .ok_or(Error::Empty)?;
    assert!(anonymous.anonymous);
    assert_eq!(document.block_index("ext"), document.block_index("EXT"));
    assert!(document.block_index("EXT").is_some());
    Ok(())
}

#[test]
fn non_first_paper_layouts_read_their_entities_from_their_block() -> Result<(), Error> {
    let document = DxfDocument::load(multi_layout(3))?;
    let sheet = document.entities("Sheet2").collect::<Vec<Entity>>();
    assert_eq!(sheet.len(), 1);
    assert_eq!(sheet[0].common.layout, Text::from("Sheet2"));
    Ok(())
}

#[test]
fn load_and_open_produce_equal_layouts() -> Result<(), Error> {
    let bytes = simple_drawing();
    let path = std::env::temp_dir().join(format!("cadboss-document-{}.dxf", std::process::id()));
    std::fs::write(&path, &bytes)?;
    let loaded = DxfDocument::load(bytes)?;
    let opened = DxfDocument::open(&path);
    let read = DxfDocument::read(&path);
    std::fs::remove_file(&path)?;
    assert_eq!(opened?.layouts(), loaded.layouts());
    assert_eq!(read?.layouts(), loaded.layouts());
    Ok(())
}

#[test]
fn reporting_constructors_return_a_copy_of_the_document_report() -> Result<(), Error> {
    let (document, report) = DxfDocument::load_reporting(simple_drawing())?;
    assert_eq!(document.report(), &report);
    let options = Options::default();
    assert_eq!(options.threads, 1);
    let (again, again_report) =
        DxfDocument::load_with_options_reporting(simple_drawing(), &options)?;
    assert_eq!(again.layouts().len(), 2);
    assert!(again_report.is_empty());
    Ok(())
}

#[test]
fn an_empty_file_is_an_empty_error() {
    assert!(matches!(
        DxfDocument::load(Vec::<u8>::new()),
        Err(Error::Empty)
    ));
}

#[test]
fn layout_limit_truncates_layouts_with_a_report() -> Result<(), Error> {
    let document = DxfDocument::load(multi_layout(300))?;
    assert_eq!(document.layouts().len(), 200);
    assert!(document.report().skipped.contains(&Skipped {
        kind: SkipKind::Object,
        reason: SkipReason::EntityLimit,
        count: 1
    }));
    Ok(())
}

#[test]
fn a_document_without_layout_objects_synthesizes_model_and_layout1() -> Result<(), Error> {
    let bytes = simple_drawing();
    let text = String::from_utf8_lossy(&bytes).replace("LAYOUT", "LAYOUX");
    let document = DxfDocument::load(text.into_bytes())?;
    let layouts: Vec<&Text> = document
        .layouts()
        .iter()
        .map(|layout| &layout.name)
        .collect();
    assert_eq!(names(layouts), ["Model", "Layout1"]);
    Ok(())
}
