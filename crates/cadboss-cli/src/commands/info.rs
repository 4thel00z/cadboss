use std::io::Write;
use std::path::PathBuf;

use cadboss_core::Report;
use cadboss_dxf::DxfDocument;
use cadboss_output::format_number;
use clap::Args;

use crate::failure::Failure;

#[derive(Args)]
pub struct InfoArgs {
    pub file: PathBuf,
}

pub fn run(doc: &DxfDocument, out: &mut dyn Write) -> Result<Report, Failure> {
    let (document, report) = doc.document_reporting();
    let meta = &document.metadata;
    let names = document
        .layouts
        .iter()
        .map(|layout| layout.name.to_string())
        .collect::<Vec<_>>()
        .join(", ");
    let entities: u64 = meta.entity_counts.iter().map(|(_, count)| count).sum();
    writeln!(out, "Format: {} {}", meta.format.label(), meta.version)?;
    writeln!(out, "Units: {}", meta.units.label())?;
    writeln!(out, "Layers: {}", meta.layer_count)?;
    writeln!(out, "Blocks: {}", meta.block_count)?;
    writeln!(out, "Layouts: {} ({names})", meta.layout_count)?;
    writeln!(out, "Entities: {entities}")?;
    if let Some(extents) = meta.extents {
        writeln!(
            out,
            "Extents: ({}, {}) to ({}, {})",
            format_number(extents.min.x),
            format_number(extents.min.y),
            format_number(extents.max.x),
            format_number(extents.max.y)
        )?;
    }
    if !report.is_empty() {
        writeln!(out, "Report: {}", report.summary())?;
    }
    Ok(report)
}
