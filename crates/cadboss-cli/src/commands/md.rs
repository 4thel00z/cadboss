use std::io::Write;
use std::path::PathBuf;

use cadboss_core::Report;
use cadboss_dxf::DxfDocument;
use cadboss_output::to_markdown;
use clap::Args;

use crate::failure::Failure;

#[derive(Args)]
pub struct MdArgs {
    pub file: PathBuf,
    #[arg(long)]
    pub title: Option<String>,
}

pub fn run(args: &MdArgs, doc: &DxfDocument, out: &mut dyn Write) -> Result<Report, Failure> {
    let title = args.title.clone().unwrap_or_else(|| {
        args.file
            .file_stem()
            .map(|stem| stem.to_string_lossy().into_owned())
            .unwrap_or_default()
    });
    let (document, report) = doc.document_reporting();
    out.write_all(to_markdown(&document, &title).as_bytes())?;
    Ok(report)
}
