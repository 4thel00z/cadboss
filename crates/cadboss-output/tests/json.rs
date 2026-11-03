use cadboss_core::Report;
use cadboss_model::{Document, Format, Metadata, Text, TextGroups, TextItem, TextSource};
use cadboss_output::{to_json, to_json_compact};

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn bytes_document() -> Document {
    let item = TextItem {
        text: Text::Bytes(vec![0xff, 0x41]),
        source: TextSource::Annotation,
        layer: "0".into(),
        layout: "Model".into(),
        position: None,
        height: None,
    };
    Document {
        metadata: Metadata::new(Format::Dxf, "AC1027".into()),
        layers: vec![],
        blocks: vec![],
        layouts: vec![],
        text: TextGroups {
            annotations: vec![item],
            ..TextGroups::default()
        },
        report: Report::default(),
    }
}

#[test]
fn json_pretty_has_metadata_first_and_bytes_objects_for_invalid_utf8() -> TestResult {
    let json = to_json(&bytes_document());
    assert!(json.starts_with("{\n  \"metadata\": {"));
    let value: serde_json::Value = serde_json::from_str(&json)?;
    assert_eq!(
        value["text"]["annotations"][0]["text"]["_bytes"],
        serde_json::json!("/0E=")
    );
    Ok(())
}

#[test]
fn json_compact_round_trips_through_serde_json_value() -> TestResult {
    let document = cadboss_dxf::load(cadboss_testkit::dxf::simple_drawing())?;
    let compact = to_json_compact(&document);
    assert!(!compact.contains('\n'));
    let value: serde_json::Value = serde_json::from_str(&compact)?;
    let pretty: serde_json::Value = serde_json::from_str(&to_json(&document))?;
    assert_eq!(value, pretty);
    assert_eq!(value["metadata"]["format"], serde_json::json!("Dxf"));
    Ok(())
}
