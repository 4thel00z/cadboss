use std::path::Path;

use cadboss_dxf::{DxfDocument, Options};

use crate::failure::Failure;

/// Fails with a usage error listing the available layouts when `layout` names none of them.
pub fn known_layout(doc: &DxfDocument, layout: Option<&str>) -> Result<(), Failure> {
    let Some(wanted) = layout else {
        return Ok(());
    };
    if doc
        .layouts()
        .iter()
        .any(|candidate| candidate.name.as_str() == Some(wanted))
    {
        return Ok(());
    }
    let names = doc
        .layouts()
        .iter()
        .map(|candidate| candidate.name.to_string())
        .collect::<Vec<_>>()
        .join(", ");
    Err(Failure::usage(format!(
        "unknown layout: {wanted} (available: {names})"
    )))
}

pub fn open(path: &Path, options: &Options) -> Result<DxfDocument, Failure> {
    DxfDocument::open_with_options(path, options)
        .map_err(|error| Failure::input(format!("{}: {error}", path.display())))
}
