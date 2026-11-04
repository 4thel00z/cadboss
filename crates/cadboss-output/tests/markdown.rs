use cadboss_core::{Report, SkipKind, SkipReason};
use cadboss_model::{
    Document, Format, Layout, LayoutKind, Metadata, Text, TextGroups, TextItem, TextSource,
};
use cadboss_output::to_markdown;

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn simple_document() -> Result<Document, Box<dyn std::error::Error>> {
    Ok(cadboss_dxf::load(cadboss_testkit::dxf::simple_drawing())?)
}

#[test]
fn markdown_of_simple_drawing_matches_the_golden_file() -> TestResult {
    let markdown = to_markdown(&simple_document()?, "simple.dxf");
    assert_eq!(markdown, include_str!("golden/simple.md"));
    Ok(())
}

#[test]
fn markdown_omits_empty_sections() {
    let document = Document {
        metadata: Metadata::new(Format::Dxf, "AC1027".into()),
        layers: vec![],
        blocks: vec![],
        layouts: vec![],
        text: TextGroups::default(),
        report: Report::default(),
    };
    let markdown = to_markdown(&document, "empty");
    assert!(markdown.starts_with("# empty\n\n## Metadata\n\n"));
    assert!(markdown.ends_with('\n'));
    assert!(!markdown.ends_with("\n\n"));
    assert!(!markdown.contains("## Names"));
    assert!(!markdown.contains("## Attributes"));
    assert!(!markdown.contains("## References"));
    assert!(!markdown.contains("## Report"));
}

fn item(text: Text, layer: &str, height: Option<f64>) -> TextItem {
    TextItem {
        text,
        source: TextSource::Annotation,
        layer: layer.into(),
        layout: "Model".into(),
        position: None,
        height,
    }
}

fn model_document(annotations: Vec<TextItem>) -> Document {
    let mut document = Document {
        metadata: Metadata::new(Format::Dxf, "AC1027".into()),
        layers: vec![],
        blocks: vec![],
        layouts: vec![Layout {
            name: "Model".into(),
            kind: LayoutKind::Model,
            block_name: "*Model_Space".into(),
            extents: None,
            entity_count: 0,
        }],
        text: TextGroups::default(),
        report: Report::default(),
    };
    document.text.annotations = annotations;
    document
}

#[test]
fn markdown_report_has_one_bullet_per_skip() {
    let mut document = model_document(vec![]);
    document
        .report
        .note(SkipKind::Entity, SkipReason::Malformed);
    document
        .report
        .note(SkipKind::Text, SkipReason::UnsupportedCodepage);
    let markdown = to_markdown(&document, "t");
    assert!(markdown.contains(
        "## Report\n\n- 1 entity skipped (malformed)\n- 1 text skipped (unsupported_codepage)\n"
    ));
}

#[test]
fn markdown_escapes_pipes_in_entity_type_cells() {
    let mut document = model_document(vec![]);
    document.metadata.entity_counts = vec![("A|B".to_owned(), 2)];
    assert!(to_markdown(&document, "t").contains("| A\\|B | 2 |"));
}

#[test]
fn markdown_replaces_newlines_in_bullets_with_spaces() {
    let document = model_document(vec![item("one\ntwo\r\nthree".into(), "0", None)]);
    assert!(to_markdown(&document, "t").contains("- one two three\n"));
}

#[test]
fn markdown_renders_undecodable_text_as_a_byte_count() {
    let document = model_document(vec![item(Text::Bytes(vec![0xff, 0x41]), "0", None)]);
    assert!(to_markdown(&document, "t").contains("- [non-decodable text, 2 bytes]\n"));
}

#[test]
fn markdown_omits_the_height_when_it_is_none() {
    let document = model_document(vec![item("Hi".into(), "0", None)]);
    let markdown = to_markdown(&document, "t");
    assert!(markdown.contains("- Hi\n"));
    assert!(!markdown.contains("(h "));
}

#[test]
fn markdown_formats_height_12_345_as_12_35() {
    let document = model_document(vec![item("Hi".into(), "0", Some(12.345))]);
    assert!(to_markdown(&document, "t").contains("(h 12.35)"));
}

#[test]
fn markdown_groups_fifty_thousand_distinct_layers_in_under_two_seconds() {
    let annotations = (0..50_000)
        .map(|index| item(format!("t{index}").into(), &format!("L{index}"), Some(1.0)))
        .collect();
    let document = model_document(annotations);
    let started = std::time::Instant::now();
    let markdown = to_markdown(&document, "layers");
    let elapsed = started.elapsed();
    assert!(elapsed.as_secs_f64() < 2.0, "took {elapsed:?}");
    assert_eq!(markdown.matches("#### Layer ").count(), 50_000);
    assert!(markdown.contains("#### Layer L49999\n\n- t49999 (h 1)"));
}

#[test]
fn markdown_keeps_first_seen_layer_order_and_item_order_within_a_layer() {
    let document = model_document(vec![
        item("b1".into(), "B", None),
        item("a1".into(), "A", None),
        item("b2".into(), "B", None),
    ]);
    let markdown = to_markdown(&document, "t");
    assert!(markdown.contains("#### Layer B\n\n- b1\n- b2\n\n#### Layer A\n\n- a1"));
}
