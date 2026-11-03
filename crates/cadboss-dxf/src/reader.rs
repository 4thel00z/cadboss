//! One-shot assembly of the unified document and the free reading functions.

use std::collections::HashMap;
use std::path::Path;

use cadboss_core::{Report, Result};
use cadboss_model::{Document, Entity, LayoutKind};

use crate::document::{DxfDocument, Options};
use crate::textgroups::survey;

fn count(totals: &mut HashMap<String, u64>, entity: &Entity) {
    let name = entity.display_type();
    if let Some(total) = totals.get_mut(name) {
        *total += 1;
        return;
    }
    totals.insert(name.to_owned(), 1);
}

/// One walk per layout yields counts, extents and text together.
pub(crate) fn assemble(doc: &DxfDocument) -> (Document, Report) {
    let mut report = doc.report().clone();
    let mut totals: HashMap<String, u64> = HashMap::new();
    let survey = survey(doc, &mut |entity| count(&mut totals, entity));
    let mut layouts = doc.layouts().to_vec();
    for (layout, pass) in layouts.iter_mut().zip(survey.passes) {
        let Some(pass) = pass else {
            continue;
        };
        layout.entity_count = pass.entity_count;
        layout.extents = pass.extents;
        report.merge(pass.decoded);
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

    report.merge(survey.report);

    let mut metadata = doc.header().metadata();
    metadata.extents = metadata.extents.or_else(|| {
        layouts
            .iter()
            .find(|layout| layout.kind == LayoutKind::Model)
            .and_then(|layout| layout.extents)
    });
    metadata.entity_counts = entity_counts;
    metadata.layer_count = doc.layers().len() as u32;
    metadata.block_count = blocks.iter().filter(|block| !block.anonymous).count() as u32;
    metadata.layout_count = layouts.len() as u32;

    let document = Document {
        metadata,
        layers: doc.layers().to_vec(),
        blocks,
        layouts,
        text: survey.groups,
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

pub fn load(bytes: impl Into<Vec<u8>>) -> Result<Document> {
    Ok(DxfDocument::load(bytes)?.document())
}

pub fn load_with_options(bytes: impl Into<Vec<u8>>, options: &Options) -> Result<Document> {
    Ok(DxfDocument::load_with_options(bytes, options)?.document())
}

pub fn load_reporting(bytes: impl Into<Vec<u8>>) -> Result<(Document, Report)> {
    Ok(DxfDocument::load(bytes)?.document_reporting())
}

pub fn load_with_options_reporting(
    bytes: impl Into<Vec<u8>>,
    options: &Options,
) -> Result<(Document, Report)> {
    Ok(DxfDocument::load_with_options(bytes, options)?.document_reporting())
}
