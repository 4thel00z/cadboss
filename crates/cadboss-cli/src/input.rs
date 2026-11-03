use std::path::Path;

use cadboss_dxf::{DxfDocument, Options};

use crate::failure::Failure;

pub fn open(path: &Path, options: &Options) -> Result<DxfDocument, Failure> {
    DxfDocument::open_with_options(path, options)
        .map_err(|error| Failure::input(format!("{}: {error}", path.display())))
}
