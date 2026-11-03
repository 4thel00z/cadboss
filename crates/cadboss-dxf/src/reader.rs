//! One-shot assembly of the unified document and the free reading functions.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use cadboss_core::{Report, Result};
use cadboss_model::Document;

use crate::document::{DxfDocument, Options};

pub(crate) fn assemble(doc: &DxfDocument) -> (Document, Report) {
    let mut report = doc.report().clone();
    let mut totals: HashMap<String, u64> = HashMap::new();
    let mut layouts = doc.layouts().to_vec();
    for layout in &mut layouts {
        let Some(name) = layout.name.as_str() else {
            continue;
        };
        let mut decoder = doc.entities(name);
        let mut count = 0;
        for entity in decoder.by_ref() {
            *totals.entry(entity.display_type().to_owned()).or_default() += 1;
            count += 1;
        }
        report.merge(decoder.into_report());
        layout.entity_count = count;
        layout.extents = doc.layout_extents(name);
    }
    let mut entity_counts: Vec<(String, u64)> = totals.into_iter().collect();
    entity_counts.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));

    let blocks = doc
        .blocks()
        .cloned()
        .map(|mut block| {
            let Some(mut decoder) = block
                .name
                .as_str()
                .and_then(|name| doc.block_entities(name))
            else {
                return block;
            };
            block.entity_count = decoder.by_ref().count() as u64;
            report.merge(decoder.into_report());
            block
        })
        .collect::<Vec<_>>();

    let (text, text_report) = doc.text_reporting();
    report.merge(text_report);

    let mut metadata = doc.header().metadata();
    metadata.extents = metadata.extents.or_else(|| doc.layout_extents("Model"));
    metadata.entity_counts = entity_counts;
    metadata.layer_count = doc.layers().len() as u32;
    metadata.block_count = blocks.iter().filter(|block| !block.anonymous).count() as u32;
    metadata.layout_count = layouts.len() as u32;

    let document = Document {
        metadata,
        layers: doc.layers().to_vec(),
        blocks,
        layouts,
        text,
        report: report.clone(),
    };
    (document, report)
}

impl DxfDocument {
    pub fn document(&self) -> Document {
        assemble(self).0
    }

    pub fn document_reporting(&self) -> (Document, Report) {
        assemble(self)
    }
}

pub fn open(path: &Path) -> Result<Document> {
    Ok(DxfDocument::open(path)?.document())
}

pub fn open_with_options(path: &Path, options: &Options) -> Result<Document> {
    Ok(DxfDocument::open_with_options(path, options)?.document())
}

pub fn open_reporting(path: &Path) -> Result<(Document, Report)> {
    Ok(DxfDocument::open(path)?.document_reporting())
}

pub fn open_with_options_reporting(path: &Path, options: &Options) -> Result<(Document, Report)> {
    Ok(DxfDocument::open_with_options(path, options)?.document_reporting())
}

pub fn read(path: &Path) -> Result<Document> {
    Ok(DxfDocument::read(path)?.document())
}

pub fn read_with_options(path: &Path, options: &Options) -> Result<Document> {
    Ok(DxfDocument::read_with_options(path, options)?.document())
}

pub fn read_reporting(path: &Path) -> Result<(Document, Report)> {
    Ok(DxfDocument::read(path)?.document_reporting())
}

pub fn read_with_options_reporting(path: &Path, options: &Options) -> Result<(Document, Report)> {
    Ok(DxfDocument::read_with_options(path, options)?.document_reporting())
}

pub fn load(bytes: impl Into<Arc<[u8]>>) -> Result<Document> {
    Ok(DxfDocument::load(bytes)?.document())
}

pub fn load_with_options(bytes: impl Into<Arc<[u8]>>, options: &Options) -> Result<Document> {
    Ok(DxfDocument::load_with_options(bytes, options)?.document())
}

pub fn load_reporting(bytes: impl Into<Arc<[u8]>>) -> Result<(Document, Report)> {
    Ok(DxfDocument::load(bytes)?.document_reporting())
}

pub fn load_with_options_reporting(
    bytes: impl Into<Arc<[u8]>>,
    options: &Options,
) -> Result<(Document, Report)> {
    Ok(DxfDocument::load_with_options(bytes, options)?.document_reporting())
}
