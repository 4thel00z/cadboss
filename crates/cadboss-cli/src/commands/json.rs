use std::collections::HashSet;
use std::io::Write;
use std::path::PathBuf;

use cadboss_core::Report;
use cadboss_dxf::DxfDocument;
use cadboss_model::Document;
use clap::Args;
use serde::ser::{SerializeMap, Serializer};
use serde::Serialize;

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

/// The unified document, optionally followed by an `entities` map whose lists are decoded while they are serialized.
#[derive(Serialize)]
pub struct Export<'a> {
    #[serde(flatten)]
    document: &'a Document,
    #[serde(skip_serializing_if = "Option::is_none")]
    entities: Option<Layouts<'a>>,
}

impl<'a> Export<'a> {
    pub fn new(
        doc: &'a DxfDocument,
        document: &'a Document,
        entities: bool,
        layout: Option<&str>,
    ) -> Export<'a> {
        let mut seen = HashSet::new();
        let names = document
            .layouts
            .iter()
            .map(|entry| entry.name.to_string())
            .filter(|name| layout.is_none_or(|wanted| wanted == name))
            .filter(|name| seen.insert(name.clone()))
            .collect();
        Export {
            document,
            entities: entities.then_some(Layouts { doc, names }),
        }
    }
}

pub struct Layouts<'a> {
    doc: &'a DxfDocument,
    names: Vec<String>,
}

impl Serialize for Layouts<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(Some(self.names.len()))?;
        for name in &self.names {
            map.serialize_entry(
                name,
                &Entities {
                    doc: self.doc,
                    layout: name,
                },
            )?;
        }
        map.end()
    }
}

struct Entities<'a> {
    doc: &'a DxfDocument,
    layout: &'a str,
}

impl Serialize for Entities<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_seq(self.doc.entities(self.layout))
    }
}

fn write(out: &mut dyn Write, export: &Export, compact: bool) -> Result<(), Failure> {
    if compact {
        serde_json::to_writer(&mut *out, export)?;
        return Ok(());
    }
    serde_json::to_writer_pretty(&mut *out, export)?;
    Ok(())
}

pub fn run(args: &JsonArgs, doc: &DxfDocument, out: &mut dyn Write) -> Result<Report, Failure> {
    let (document, report) = doc.document_reporting();
    let export = Export::new(doc, &document, args.entities, args.layout.as_deref());
    write(out, &export, args.compact)?;
    writeln!(out)?;
    Ok(report)
}
