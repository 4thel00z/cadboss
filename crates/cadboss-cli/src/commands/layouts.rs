use std::io::Write;
use std::path::PathBuf;

use cadboss_core::Report;
use cadboss_dxf::DxfDocument;
use clap::Args;

use crate::failure::Failure;

#[derive(Args)]
pub struct LayoutsArgs {
    pub file: PathBuf,
}

pub fn run(doc: &DxfDocument, out: &mut dyn Write) -> Result<Report, Failure> {
    let (document, report) = doc.document_reporting();
    for layout in &document.layouts {
        writeln!(
            out,
            "{}\t{:?}\t{}",
            layout.name, layout.kind, layout.entity_count
        )?;
    }
    Ok(report)
}
