use std::io::Write;
use std::path::PathBuf;

use cadboss_core::Report;
use cadboss_dxf::DxfDocument;
use clap::Args;
use serde_json::Value;

use crate::failure::Failure;

#[derive(Args)]
pub struct JsonArgs {
    pub file: PathBuf,
    #[arg(long)]
    pub compact: bool,
    #[arg(long)]
    pub entities: bool,
    #[arg(long)]
    pub layout: Option<String>,
}

pub fn build(
    doc: &DxfDocument,
    entities: bool,
    layout: Option<&str>,
) -> Result<(Value, Report), Failure> {
    let (document, report) = doc.document_reporting();
    let mut value = serde_json::to_value(&document)?;
    if !entities {
        return Ok((value, report));
    }
    let mut by_layout = serde_json::Map::new();
    for entry in &document.layouts {
        let name = entry.name.to_string();
        if layout.is_some_and(|wanted| wanted != name) {
            continue;
        }
        let list = doc.entities(&name).collect::<Vec<_>>();
        by_layout.insert(name, serde_json::to_value(list)?);
    }
    if let Value::Object(map) = &mut value {
        map.insert("entities".to_owned(), Value::Object(by_layout));
    }
    Ok((value, report))
}

pub fn run(args: &JsonArgs, doc: &DxfDocument, out: &mut dyn Write) -> Result<Report, Failure> {
    let (value, report) = build(doc, args.entities, args.layout.as_deref())?;
    let text = if args.compact {
        serde_json::to_string(&value)?
    } else {
        serde_json::to_string_pretty(&value)?
    };
    writeln!(out, "{text}")?;
    Ok(report)
}
