use cadboss_core::{Error, Point, Report, SkipKind, SkipReason, Skipped};
use cadboss_dxf::{DxfDocument, Options};

fn file(sections: &[(&str, &[(i16, &str)])]) -> Vec<u8> {
    let mut text = String::new();
    for (name, pairs) in sections {
        text.push_str(&format!("0\nSECTION\n2\n{name}\n"));
        for (code, value) in *pairs {
            text.push_str(&format!("{code}\n{value}\n"));
        }
        text.push_str("0\nENDSEC\n");
    }
    text.push_str("0\nEOF\n");
    text.into_bytes()
}

fn noted(report: &Report, kind: SkipKind, reason: SkipReason) -> u64 {
    report
        .skipped
        .iter()
        .filter(|entry| entry.kind == kind && entry.reason == reason)
        .map(|entry| entry.count)
        .sum()
}
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
    let opened = DxfDocument::open(&path)?;
    let read = DxfDocument::read(&path)?;
    std::fs::remove_file(&path)?;
    assert_eq!(opened.layouts(), loaded.layouts());
    assert_eq!(read.layouts(), loaded.layouts());
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
fn open_and_read_reject_a_file_past_max_bytes_from_its_length() -> Result<(), Error> {
    let bytes = simple_drawing();
    let path = std::env::temp_dir().join(format!("cadboss-budget-{}.dxf", std::process::id()));
    std::fs::write(&path, &bytes)?;
    let mut options = Options::default();
    options.budget.max_bytes = bytes.len() as u64 - 1;
    let opened = DxfDocument::open_with_options(&path, &options);
    let read = DxfDocument::read_with_options(&path, &options);
    options.budget.max_bytes = bytes.len() as u64;
    let fits = DxfDocument::read_with_options(&path, &options);
    std::fs::remove_file(&path)?;
    for result in [opened, read] {
        assert!(matches!(
            result,
            Err(Error::BudgetExceeded {
                budget: "max_bytes",
                ..
            })
        ));
    }
    assert_eq!(fits?.bytes().len(), bytes.len());
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
    assert_eq!(document.layouts().len(), 201);
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

#[test]
fn walk_stops_at_the_entity_budget_on_doubling_inserts() -> Result<(), Error> {
    let mut builder = DxfBuilder::new();
    for level in 0..=20 {
        builder = builder.block(&format!("B{level}"), (0.0, 0.0));
        if level < 20 {
            let next = format!("B{}", level + 1);
            builder = builder.insert(&next, (0.0, 0.0)).insert(&next, (0.0, 0.0));
        }
        builder = builder.line((0.0, 0.0), (1.0, 1.0)).end();
    }
    let bytes = builder.insert("B0", (0.0, 0.0)).build();
    let mut options = Options::default();
    options.budget.max_entities = 1000;
    let document = DxfDocument::load_with_options(bytes, &options)?;
    let mut visits = 0;
    let report = document.walk("Model", &mut |_, _, _| visits += 1);
    assert!(visits <= 1000);
    assert_eq!(noted(&report, SkipKind::Entity, SkipReason::EntityLimit), 1);
    Ok(())
}

#[test]
fn entities_only_file_still_has_a_model_layout() -> Result<(), Error> {
    let bytes = file(&[(
        "ENTITIES",
        &[
            (0, "LINE"),
            (8, "0"),
            (10, "0"),
            (20, "0"),
            (11, "1"),
            (21, "1"),
        ],
    )]);
    let document = DxfDocument::load(bytes)?;
    let layouts: Vec<&Text> = document
        .layouts()
        .iter()
        .map(|layout| &layout.name)
        .collect();
    assert_eq!(names(layouts), ["Model"]);
    assert_eq!(document.entities("Model").count(), 1);
    Ok(())
}

#[test]
fn filtered_entities_add_no_notes() -> Result<(), Error> {
    let bytes = file(&[
        (
            "TABLES",
            &[
                (0, "TABLE"),
                (2, "BLOCK_RECORD"),
                (0, "BLOCK_RECORD"),
                (5, "A"),
                (2, "*Model_Space"),
                (0, "BLOCK_RECORD"),
                (5, "B"),
                (2, "*Paper_Space"),
                (0, "ENDTAB"),
            ],
        ),
        (
            "ENTITIES",
            &[(0, "WIPEOUT"), (5, "1F"), (330, "A"), (8, "0")],
        ),
    ]);
    let document = DxfDocument::load(bytes)?;
    let mut paper = document.entities("Layout1");
    assert_eq!(paper.by_ref().count(), 0);
    assert!(paper.report().is_empty());
    let mut model = document.entities("Model");
    assert_eq!(model.by_ref().count(), 1);
    assert_eq!(
        noted(model.report(), SkipKind::Entity, SkipReason::Unknown),
        1
    );
    Ok(())
}

#[test]
fn a_block_without_endblk_is_truncated_and_reported() -> Result<(), Error> {
    let bytes = file(&[(
        "BLOCKS",
        &[
            (0, "BLOCK"),
            (2, "A"),
            (0, "LINE"),
            (8, "0"),
            (10, "0"),
            (20, "0"),
            (11, "1"),
            (21, "1"),
            (0, "BLOCK"),
            (2, "B"),
            (0, "ENDBLK"),
        ],
    )]);
    let document = DxfDocument::load(bytes)?;
    assert_eq!(document.blocks().count(), 2);
    assert_eq!(document.block_entities("A").ok_or(Error::Empty)?.count(), 1);
    assert_eq!(
        noted(document.report(), SkipKind::Section, SkipReason::Truncated),
        1
    );
    Ok(())
}

#[test]
fn a_nameless_block_is_a_malformed_table_entry() -> Result<(), Error> {
    let bytes = file(&[("BLOCKS", &[(0, "BLOCK"), (0, "ENDBLK")])]);
    let document = DxfDocument::load(bytes)?;
    assert_eq!(document.blocks().count(), 0);
    assert_eq!(
        noted(document.report(), SkipKind::Table, SkipReason::Malformed),
        1
    );
    Ok(())
}

#[test]
fn a_layout_with_an_unresolved_block_record_is_skipped() -> Result<(), Error> {
    let bytes = file(&[(
        "OBJECTS",
        &[(0, "LAYOUT"), (1, "X"), (71, "1"), (330, "FFF")],
    )]);
    let document = DxfDocument::load(bytes)?;
    let layouts: Vec<&Text> = document
        .layouts()
        .iter()
        .map(|layout| &layout.name)
        .collect();
    assert_eq!(names(layouts), ["Model"]);
    assert_eq!(
        noted(document.report(), SkipKind::Object, SkipReason::Malformed),
        1
    );
    Ok(())
}

#[test]
fn a_file_without_an_entities_section_has_no_entities() -> Result<(), Error> {
    let document = DxfDocument::load(file(&[("HEADER", &[(9, "$ACADVER"), (1, "AC1027")])]))?;
    assert_eq!(document.entities("Model").count(), 0);
    Ok(())
}

#[test]
fn walk_applies_base_point_and_rotation_together() -> Result<(), Error> {
    let bytes = DxfBuilder::new()
        .block("A", (5.0, 5.0))
        .line((5.0, 5.0), (6.0, 5.0))
        .end()
        .insert_transformed("A", (10.0, 10.0), (1.0, 1.0, 1.0), 90.0)
        .build();
    let document = DxfDocument::load(bytes)?;
    let mut ends = Vec::new();
    document.walk("Model", &mut |entity, transform, _| {
        if let EntityKind::Line { end, .. } = &entity.kind {
            ends.push(transform.apply(*end));
        }
    });
    assert_eq!(ends.len(), 1);
    assert!(close(ends[0].x, 10.0) && close(ends[0].y, 11.0));
    Ok(())
}

#[test]
fn sibling_inserts_at_the_nesting_limit_note_once() -> Result<(), Error> {
    let bytes = DxfBuilder::new()
        .block("B", (0.0, 0.0))
        .line((0.0, 0.0), (1.0, 1.0))
        .end()
        .block("A", (0.0, 0.0))
        .insert("B", (0.0, 0.0))
        .insert("B", (1.0, 0.0))
        .end()
        .insert("A", (0.0, 0.0))
        .build();
    let mut options = Options::default();
    options.budget.max_nesting = 2;
    let document = DxfDocument::load_with_options(bytes, &options)?;
    let report = document.walk("Model", &mut |_, _, _| {});
    assert_eq!(
        noted(&report, SkipKind::Entity, SkipReason::NestingLimit),
        1
    );
    Ok(())
}

#[test]
fn walk_detects_a_two_block_cycle() -> Result<(), Error> {
    let bytes = DxfBuilder::new()
        .block("A", (0.0, 0.0))
        .insert("B", (0.0, 0.0))
        .end()
        .block("B", (0.0, 0.0))
        .insert("A", (0.0, 0.0))
        .end()
        .insert("A", (0.0, 0.0))
        .build();
    let document = DxfDocument::load(bytes)?;
    let mut visited = 0;
    let report = document.walk("Model", &mut |_, _, _| visited += 1);
    assert_eq!(visited, 3);
    assert_eq!(noted(&report, SkipKind::Entity, SkipReason::Malformed), 1);
    Ok(())
}

#[test]
fn layout_cap_keeps_model_and_the_lowest_tab_paper_layouts() -> Result<(), Error> {
    let bytes = file(&[
        (
            "TABLES",
            &[
                (0, "TABLE"),
                (2, "BLOCK_RECORD"),
                (0, "BLOCK_RECORD"),
                (5, "A"),
                (2, "*Model_Space"),
                (0, "BLOCK_RECORD"),
                (5, "B"),
                (2, "*Paper_Space"),
                (0, "BLOCK_RECORD"),
                (5, "C"),
                (2, "*Paper_Space1"),
                (0, "BLOCK_RECORD"),
                (5, "D"),
                (2, "*Paper_Space2"),
                (0, "ENDTAB"),
            ],
        ),
        (
            "OBJECTS",
            &[
                (0, "LAYOUT"),
                (1, "Sheet3"),
                (71, "3"),
                (330, "D"),
                (0, "LAYOUT"),
                (1, "Sheet1"),
                (71, "1"),
                (330, "B"),
                (0, "LAYOUT"),
                (1, "Sheet2"),
                (71, "2"),
                (330, "C"),
                (0, "LAYOUT"),
                (1, "Model"),
                (71, "0"),
                (330, "A"),
            ],
        ),
    ]);
    let mut options = Options::default();
    options.budget.max_layouts = 2;
    let document = DxfDocument::load_with_options(bytes, &options)?;
    let layouts: Vec<&Text> = document
        .layouts()
        .iter()
        .map(|layout| &layout.name)
        .collect();
    assert_eq!(names(layouts), ["Model", "Sheet1", "Sheet2"]);
    assert_eq!(
        noted(document.report(), SkipKind::Object, SkipReason::EntityLimit),
        1
    );
    Ok(())
}

#[test]
fn walk_report_has_no_entity_decode_notes() -> Result<(), Error> {
    let bytes = DxfBuilder::new().raw(0, "WIPEOUT").build();
    let document = DxfDocument::load(bytes)?;
    let report = document.walk("Model", &mut |_, _, _| {});
    assert!(report.is_empty());
    Ok(())
}
