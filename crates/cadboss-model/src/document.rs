//! The unified document and its grouped text items.

use cadboss_core::{Point, Report};
use serde::Serialize;

use crate::metadata::{Block, Layer, Layout, Metadata};
use crate::text::Text;

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub enum TextSource {
    TitleBlock,
    Annotation,
    Dimension,
    Leader,
    BlockName,
    LayerName,
    Attribute,
    Xref,
    ImagePath,
    Hyperlink,
    Property,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TextItem {
    pub text: Text,
    pub source: TextSource,
    pub layer: Text,
    pub layout: Text,
    pub position: Option<Point>,
    pub height: Option<f64>,
    pub handle: Option<u64>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct TextGroups {
    pub title_block: Vec<TextItem>,
    pub annotations: Vec<TextItem>,
    pub names: Vec<TextItem>,
    pub attributes: Vec<TextItem>,
    pub references: Vec<TextItem>,
    pub properties: Vec<TextItem>,
}

impl TextGroups {
    pub fn len(&self) -> usize {
        self.title_block.len()
            + self.annotations.len()
            + self.names.len()
            + self.attributes.len()
            + self.references.len()
            + self.properties.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn iter(&self) -> impl Iterator<Item = &TextItem> {
        self.title_block
            .iter()
            .chain(&self.annotations)
            .chain(&self.names)
            .chain(&self.attributes)
            .chain(&self.references)
            .chain(&self.properties)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Document {
    pub metadata: Metadata,
    pub layers: Vec<Layer>,
    pub blocks: Vec<Block>,
    pub layouts: Vec<Layout>,
    pub text: TextGroups,
    pub report: Report,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Format, Metadata};
    use cadboss_core::Report;

    #[test]
    fn document_serializes_with_stable_key_order() {
        let document = Document {
            metadata: Metadata::new(Format::Dxf, "AC1027".into()),
            layers: vec![],
            blocks: vec![],
            layouts: vec![],
            text: TextGroups {
                annotations: vec![TextItem {
                    text: "a".into(),
                    source: TextSource::Annotation,
                    layer: "0".into(),
                    layout: "Model".into(),
                    position: None,
                    height: None,
                    handle: Some(7),
                }],
                ..TextGroups::default()
            },
            report: Report::default(),
        };
        let value = serde_json::to_value(&document).unwrap();
        let keys: Vec<&str> = value
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(
            keys,
            ["metadata", "layers", "blocks", "layouts", "text", "report"]
        );
        let item_keys: Vec<&str> = value["text"]["annotations"][0]
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(
            item_keys,
            ["text", "source", "layer", "layout", "position", "height", "handle"]
        );
    }

    #[test]
    fn text_groups_count_and_iterate_in_group_order() {
        let item = |label: &str| TextItem {
            text: label.into(),
            source: TextSource::Annotation,
            layer: "0".into(),
            layout: "Model".into(),
            position: None,
            height: None,
            handle: None,
        };
        let groups = TextGroups {
            title_block: vec![item("a")],
            annotations: vec![item("b")],
            names: vec![item("c")],
            attributes: vec![item("d")],
            references: vec![item("e")],
            properties: vec![item("f")],
        };
        assert_eq!(groups.len(), 6);
        let order: Vec<String> = groups.iter().map(|i| i.text.to_string()).collect();
        assert_eq!(order, ["a", "b", "c", "d", "e", "f"]);
        assert!(!groups.is_empty());
    }
}
