use cadboss_core::Point;
use cadboss_dxf::DxfDocument;
use cadboss_model::{Text, TextItem, TextSource};
use cadboss_testkit::dxf::{cp1251_attribute_drawing, simple_drawing, DxfBuilder, CP1251_PUMP};

fn texts(items: &[TextItem]) -> Vec<String> {
    items.iter().map(|item| item.text.to_string()).collect()
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-9
}

#[test]
fn simple_drawing_puts_pump_title_into_title_block_group() {
    let groups = DxfDocument::load(simple_drawing()).unwrap().text();
    assert_eq!(texts(&groups.title_block), ["TITLE: PUMP P-101"]);
    assert_eq!(groups.title_block[0].layout.to_string(), "Layout1");
    assert_eq!(groups.title_block[0].position, Some(Point::xy(205.0, 15.0)));
    assert_eq!(texts(&groups.annotations), ["Hello"]);
}

#[test]
fn annotations_carry_layer_layout_and_transformed_position() {
    let bytes = DxfBuilder::new()
        .block("NOTE", (0.0, 0.0))
        .on_layer("NOTES")
        .text("inside", (1.0, 1.0), 2.0)
        .end()
        .on_layer("0")
        .text("Hello", (10.0, 40.0), 2.5)
        .insert_transformed("NOTE", (10.0, 20.0), (2.0, 2.0, 1.0), 0.0)
        .build();
    let groups = DxfDocument::load(bytes).unwrap().text();
    assert_eq!(groups.annotations.len(), 2);
    let hello = &groups.annotations[0];
    assert_eq!(hello.source, TextSource::Annotation);
    assert_eq!(hello.layer.to_string(), "0");
    assert_eq!(hello.layout.to_string(), "Model");
    assert_eq!(hello.position, Some(Point::xy(10.0, 40.0)));
    assert_eq!(hello.height, Some(2.5));
    let inside = &groups.annotations[1];
    assert_eq!(inside.layer.to_string(), "NOTES");
    assert_eq!(inside.layout.to_string(), "Model");
    let position = inside.position.unwrap();
    assert!(close(position.x, 12.0) && close(position.y, 22.0));
}

#[test]
fn dimension_override_with_angle_brackets_substitutes_the_measurement() {
    let bytes = DxfBuilder::new()
        .dimension("<> mm", 12.5)
        .dimension("", 12.0)
        .dimension("<>", 3.456)
        .dimension("fixed", 1.0)
        .build();
    let groups = DxfDocument::load(bytes).unwrap().text();
    assert_eq!(
        texts(&groups.annotations),
        ["12.5 mm", "12", "3.46", "fixed"]
    );
    assert!(groups
        .annotations
        .iter()
        .all(|item| item.source == TextSource::Dimension));
}

#[test]
fn attributes_are_tag_equals_value() {
    let bytes = DxfBuilder::new()
        .block("TAGBLK", (0.0, 0.0))
        .end()
        .insert_with_attribs("TAGBLK", (0.0, 0.0), &[("PUMP", "P-101"), ("SIZE", "DN50")])
        .build();
    let groups = DxfDocument::load(bytes).unwrap().text();
    assert_eq!(texts(&groups.attributes), ["PUMP=P-101", "SIZE=DN50"]);
    assert!(groups
        .attributes
        .iter()
        .all(|item| item.source == TextSource::Attribute));
}

#[test]
fn xref_and_imagedef_paths_land_in_references() {
    let bytes = DxfBuilder::new()
        .xref_block("BASE", "C:/refs/base.dwg")
        .image("maps/plan.png", (0.0, 0.0), (10.0, 10.0))
        .image("maps/plan.png", (20.0, 0.0), (10.0, 10.0))
        .imagedef("maps/unused.png")
        .build();
    let groups = DxfDocument::load(bytes).unwrap().text();
    let found: Vec<(String, TextSource)> = groups
        .references
        .iter()
        .map(|item| (item.text.to_string(), item.source))
        .collect();
    assert_eq!(
        found,
        [
            ("maps/plan.png".to_string(), TextSource::ImagePath),
            ("C:/refs/base.dwg".to_string(), TextSource::Xref),
            ("maps/unused.png".to_string(), TextSource::ImagePath),
        ]
    );
}

#[test]
fn hyperlink_xdata_is_extracted() {
    let bytes = DxfBuilder::new()
        .text("Spec", (0.0, 0.0), 2.5)
        .hyperlink("https://example.com/spec")
        .build();
    let groups = DxfDocument::load(bytes).unwrap().text();
    assert_eq!(texts(&groups.references), ["https://example.com/spec"]);
    assert_eq!(groups.references[0].source, TextSource::Hyperlink);
}

#[test]
fn names_list_every_layer_and_named_block() {
    let groups = DxfDocument::load(simple_drawing()).unwrap().text();
    let layers: Vec<String> = groups
        .names
        .iter()
        .filter(|item| item.source == TextSource::LayerName)
        .map(|item| item.text.to_string())
        .collect();
    let blocks: Vec<String> = groups
        .names
        .iter()
        .filter(|item| item.source == TextSource::BlockName)
        .map(|item| item.text.to_string())
        .collect();
    assert_eq!(layers, ["0", "DIM"]);
    assert_eq!(blocks, ["TITLEBLOCK"]);
    assert!(groups.names.iter().all(|item| item.position.is_none()));
}

#[test]
fn text_items_are_ordered_top_down_within_a_layer() {
    let bytes = DxfBuilder::new()
        .on_layer("B")
        .text("b", (0.0, 50.0), 1.0)
        .on_layer("A")
        .text("low", (0.0, 10.0), 1.0)
        .text("right", (5.0, 10.0), 1.0)
        .text("top", (0.0, 20.0), 1.0)
        .build();
    let groups = DxfDocument::load(bytes).unwrap().text();
    assert_eq!(texts(&groups.annotations), ["top", "low", "right", "b"]);
}

#[test]
fn bottom_right_paper_text_without_a_title_block_insert_is_a_title_block_item() {
    let bytes = DxfBuilder::new()
        .paper_layout("Layout1")
        .text("frame", (0.0, 0.0), 1.0)
        .text("top left", (0.0, 100.0), 1.0)
        .text("stamp", (100.0, 0.0), 1.0)
        .build();
    let groups = DxfDocument::load(bytes).unwrap().text();
    assert_eq!(texts(&groups.title_block), ["stamp"]);
    assert_eq!(groups.annotations.len(), 2);
}

#[test]
fn dimensions_without_a_measurement_and_without_text_are_skipped() {
    let bytes = DxfBuilder::new()
        .dimension_unmeasured("<>")
        .dimension_unmeasured("")
        .dimension_unmeasured("manual")
        .build();
    let groups = DxfDocument::load(bytes).unwrap().text();
    assert_eq!(texts(&groups.annotations), ["manual"]);
}

#[test]
fn bottom_right_quarter_boundaries_on_a_paper_layout() {
    let bytes = DxfBuilder::new()
        .paper_layout("Layout1")
        .text("min", (0.0, 0.0), 1.0)
        .text("max", (100.0, 100.0), 1.0)
        .text("in", (80.0, 10.0), 1.0)
        .text("high", (80.0, 30.0), 1.0)
        .text("left", (70.0, 10.0), 1.0)
        .build();
    let groups = DxfDocument::load(bytes).unwrap().text();
    assert_eq!(texts(&groups.title_block), ["in"]);
    assert_eq!(groups.annotations.len(), 4);
}

#[test]
fn text_height_is_scaled_by_the_insert_transform() {
    let bytes = DxfBuilder::new()
        .block("NOTE", (0.0, 0.0))
        .text("inside", (1.0, 1.0), 2.5)
        .end()
        .insert_transformed("NOTE", (0.0, 0.0), (2.0, 2.0, 2.0), 0.0)
        .build();
    let groups = DxfDocument::load(bytes).unwrap().text();
    assert_eq!(groups.annotations[0].height, Some(5.0));
}

#[test]
fn dimension_items_use_the_text_midpoint_as_position() {
    let bytes = DxfBuilder::new().dimension_at("", 4.0, (7.0, 9.0)).build();
    let groups = DxfDocument::load(bytes).unwrap().text();
    assert_eq!(groups.annotations[0].position, Some(Point::xy(7.0, 9.0)));
}

#[test]
fn layout_tab_order_ranks_model_before_paper_layouts() {
    let bytes = DxfBuilder::new()
        .text("model", (0.0, 0.0), 1.0)
        .paper_layout("Layout1")
        .text("paper", (0.0, 100.0), 1.0)
        .build();
    let groups = DxfDocument::load(bytes).unwrap().text();
    assert_eq!(texts(&groups.annotations), ["model", "paper"]);
}

#[test]
fn dimension_override_formatting_codes_are_stripped_before_substitution() {
    let bytes = DxfBuilder::new()
        .dimension("{\\H0.7x;<>\\S+0.1^-0.2;}", 25.0)
        .dimension("<>\\XTYP", 10.0)
        .build();
    let groups = DxfDocument::load(bytes).unwrap().text();
    assert_eq!(texts(&groups.annotations), ["25+0.1/-0.2", "10\nTYP"]);
    assert!(groups
        .annotations
        .iter()
        .all(|item| !item.text.to_string().contains(['\\', '{'])));
}

#[test]
fn mtext_column_breaks_become_newlines() {
    let bytes = DxfBuilder::new()
        .mtext("Line 1\\NLine 2", (0.0, 0.0), 1.0)
        .build();
    let groups = DxfDocument::load(bytes).unwrap().text();
    assert_eq!(texts(&groups.annotations), ["Line 1\nLine 2"]);
}

#[test]
fn an_undecodable_attribute_value_keeps_its_bytes() {
    let groups = DxfDocument::load(cp1251_attribute_drawing())
        .unwrap()
        .text();
    let expected = [b"PUMP=".as_slice(), CP1251_PUMP].concat();
    assert_eq!(groups.attributes.len(), 1);
    assert_eq!(groups.attributes[0].text, Text::Bytes(expected));
}

#[test]
fn text_items_carry_the_handle_of_their_entity() {
    let bytes = DxfBuilder::new()
        .block("B", (0.0, 0.0))
        .text("INSIDE", (0.0, 0.0), 1.0)
        .end()
        .text("HELLO", (0.0, 0.0), 2.5)
        .insert_with_attribs("B", (1.0, 1.0), &[("TAG", "X")])
        .build();
    let doc = DxfDocument::load(bytes).unwrap();
    let groups = doc.text();
    let handles: Vec<Option<u64>> = doc
        .entities("Model")
        .map(|entity| entity.common.handle)
        .collect();
    let hello = groups
        .annotations
        .iter()
        .find(|item| item.text.to_string() == "HELLO")
        .unwrap();
    assert_eq!(hello.handle, handles[0]);
    assert!(hello.handle.is_some());
    let inside = groups
        .annotations
        .iter()
        .find(|item| item.text.to_string() == "INSIDE")
        .unwrap();
    assert!(inside.handle.is_some());
    assert_ne!(inside.handle, handles[1]);
    assert_eq!(groups.attributes[0].handle, handles[1]);
    assert!(groups.names.iter().all(|item| item.handle.is_none()));
}
