use cadboss_core::Report;
use cadboss_model::{Document, Format, Metadata, TextGroups};
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
    assert!(!markdown.contains("## Attributes"));
    assert!(!markdown.contains("## References"));
    assert!(!markdown.contains("## Report"));
}
