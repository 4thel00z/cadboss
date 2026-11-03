use std::io::Write;
use std::path::PathBuf;

use cadboss_core::Report;
use cadboss_dxf::DxfDocument;
use cadboss_model::Layer;
use clap::Args;

use crate::failure::Failure;

#[derive(Args)]
pub struct LayersArgs {
    pub file: PathBuf,
}

fn flags(layer: &Layer) -> String {
    let set: Vec<&str> = [
        (layer.frozen, "frozen"),
        (layer.off, "off"),
        (layer.locked, "locked"),
    ]
    .into_iter()
    .filter(|(on, _)| *on)
    .map(|(_, name)| name)
    .collect();
    if set.is_empty() {
        return "-".to_owned();
    }
    set.join(",")
}

pub fn run(doc: &DxfDocument, out: &mut dyn Write) -> Result<Report, Failure> {
    for layer in doc.layers() {
        writeln!(out, "{}\t{}\t{}", layer.name, layer.color, flags(layer))?;
    }
    Ok(doc.report().clone())
}
