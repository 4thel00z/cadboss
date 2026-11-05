use std::path::PathBuf;

use cadboss_dxf::DxfDocument;
use clap::Args;

use crate::failure::Failure;

#[derive(Args)]
pub struct TuiArgs {
    /// Path to the DXF file.
    pub file: PathBuf,
}

pub fn run(args: &TuiArgs, doc: DxfDocument) -> Result<(), Failure> {
    let title = args
        .file
        .file_name()
        .map_or_else(
            || args.file.to_string_lossy(),
            |name| name.to_string_lossy(),
        )
        .into_owned();
    let target = args.file.to_string_lossy().into_owned();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .map_err(|error| Failure::input(error.to_string()))?;
    let result = runtime.block_on(cadboss_tui::run(doc, title, target));
    runtime.shutdown_background();
    result.map_err(|error| Failure::input(error.to_string()))
}
