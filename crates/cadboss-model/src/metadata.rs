//! Document-level metadata, layers, blocks and layouts.

use cadboss_core::{BBox, Point};
use serde::Serialize;

use crate::text::Text;

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub enum Format {
    Dxf,
    Dwg,
    Step,
    Ifc,
}

impl Format {
    pub fn label(self) -> &'static str {
        match self {
            Format::Dxf => "DXF",
            Format::Dwg => "DWG",
            Format::Step => "STEP",
            Format::Ifc => "IFC",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub enum Units {
    Unitless,
    Inches,
    Feet,
    Millimeters,
    Centimeters,
    Meters,
    Other(u16),
}

impl Units {
    pub fn from_insunits(code: u16) -> Units {
        match code {
            0 => Units::Unitless,
            1 => Units::Inches,
            2 => Units::Feet,
            4 => Units::Millimeters,
            5 => Units::Centimeters,
            6 => Units::Meters,
            other => Units::Other(other),
        }
    }

    pub fn label(&self) -> String {
        match self {
            Units::Unitless => "Unitless".to_owned(),
            Units::Inches => "Inches".to_owned(),
            Units::Feet => "Feet".to_owned(),
            Units::Millimeters => "Millimeters".to_owned(),
            Units::Centimeters => "Centimeters".to_owned(),
            Units::Meters => "Meters".to_owned(),
            Units::Other(code) => format!("Other({code})"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Metadata {
    pub format: Format,
    pub version: Text,
    pub codepage: Option<Text>,
    pub units: Units,
    pub author: Option<Text>,
    pub created: Option<f64>,
    pub updated: Option<f64>,
    pub extents: Option<BBox>,
    pub entity_counts: Vec<(String, u64)>,
    pub layer_count: u32,
    pub block_count: u32,
    pub layout_count: u32,
}

impl Metadata {
    pub fn new(format: Format, version: Text) -> Metadata {
        Metadata {
            format,
            version,
            codepage: None,
            units: Units::Unitless,
            author: None,
            created: None,
            updated: None,
            extents: None,
            entity_counts: Vec::new(),
            layer_count: 0,
            block_count: 0,
            layout_count: 0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Layer {
    pub name: Text,
    pub color: i16,
    pub frozen: bool,
    pub off: bool,
    pub locked: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Block {
    pub name: Text,
    pub base: Point,
    pub xref_path: Option<Text>,
    pub anonymous: bool,
    pub entity_count: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub enum LayoutKind {
    Model,
    Paper,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Layout {
    pub name: Text,
    pub kind: LayoutKind,
    pub block_name: Text,
    pub extents: Option<BBox>,
    pub entity_count: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn units_from_insunits_maps_the_documented_codes() {
        assert_eq!(Units::from_insunits(0), Units::Unitless);
        assert_eq!(Units::from_insunits(1), Units::Inches);
        assert_eq!(Units::from_insunits(2), Units::Feet);
        assert_eq!(Units::from_insunits(4), Units::Millimeters);
        assert_eq!(Units::from_insunits(5), Units::Centimeters);
        assert_eq!(Units::from_insunits(6), Units::Meters);
        assert_eq!(Units::from_insunits(7), Units::Other(7));
        assert_eq!(Units::Other(7).label(), "Other(7)");
        assert_eq!(Units::Meters.label(), "Meters");
    }

    #[test]
    fn format_labels_are_uppercase() {
        assert_eq!(Format::Dxf.label(), "DXF");
        assert_eq!(Format::Ifc.label(), "IFC");
    }
}
