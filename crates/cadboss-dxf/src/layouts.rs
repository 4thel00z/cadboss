//! LAYOUT objects resolved against the block records.

use std::collections::HashMap;

use cadboss_core::{Budget, Report, SkipKind, SkipReason};
use cadboss_model::{Layout, LayoutKind, Text};

use crate::codepage::{decode, Codepage};
use crate::record::Record;
use crate::tables::BlockRecord;
use crate::tape::{Cursor, SectionName, Tape};

const MODEL_SPACE: &str = "*Model_Space";
const PAPER_SPACE: &str = "*Paper_Space";

fn is_named(name: &Text, expected: &str) -> bool {
    name.as_str()
        .is_some_and(|name| name.eq_ignore_ascii_case(expected))
}

fn layout(name: Text, block_name: Text) -> Layout {
    let mut kind = LayoutKind::Paper;
    if is_named(&block_name, MODEL_SPACE) {
        kind = LayoutKind::Model;
    }
    Layout {
        name,
        kind,
        block_name,
        extents: None,
        entity_count: 0,
    }
}

fn space_block(block_records: &[BlockRecord], space: &str) -> Option<Text> {
    block_records
        .iter()
        .find(|record| is_named(&record.name, space))
        .map(|record| record.name.clone())
}

/// Reads LAYOUT objects sorted by tab order. Paper layouts are capped at `max_layouts`, keeping the lowest tab orders; the model layout is always present and not counted.
/// `Layout1` is synthesized when no paper layout exists but a paper space block record does.
pub fn read_layouts(
    tape: &Tape,
    bytes: &[u8],
    cp: Codepage,
    block_records: &[BlockRecord],
    budget: &Budget,
    report: &mut Report,
) -> Vec<Layout> {
    let by_handle: HashMap<u64, &BlockRecord> = block_records
        .iter()
        .map(|record| (record.handle, record))
        .collect();
    let max = budget.max_layouts as usize;
    let mut model: Option<(i64, Layout)> = None;
    let mut papers: Vec<(i64, Layout)> = Vec::new();
    let mut dropped = false;
    if let Some(section) = tape.section(SectionName::Objects) {
        let mut cursor = Cursor::new(tape, bytes, section.start..section.end);
        while let Some((code, value)) = cursor.next() {
            if code != 0 || value != b"LAYOUT" {
                continue;
            }
            let pairs = Record::until(&mut cursor, 0);
            let record = pairs
                .last(330)
                .and_then(|value| pairs.handle(value))
                .and_then(|handle| by_handle.get(&handle));
            let (Some(name), Some(record)) = (pairs.last(1), record) else {
                report.note(SkipKind::Object, SkipReason::Malformed);
                continue;
            };
            let tab = pairs
                .first(71)
                .and_then(|value| pairs.i64(value))
                .unwrap_or(0);
            let layout = layout(decode(cp, name, report), record.name.clone());
            if layout.kind == LayoutKind::Model {
                model.get_or_insert((tab, layout));
                continue;
            }
            papers.push((tab, layout));
            if papers.len() <= max {
                continue;
            }
            dropped = true;
            let highest = papers
                .iter()
                .enumerate()
                .max_by_key(|(_, (tab, _))| *tab)
                .map(|(index, _)| index);
            if let Some(index) = highest {
                papers.remove(index);
            }
        }
    }
    if dropped {
        report.note(SkipKind::Object, SkipReason::EntityLimit);
    }
    let model = model.unwrap_or_else(|| {
        let block =
            space_block(block_records, MODEL_SPACE).unwrap_or_else(|| Text::from(MODEL_SPACE));
        (i64::MIN, layout(Text::from("Model"), block))
    });
    if papers.is_empty() && max > 0 {
        if let Some(block) = space_block(block_records, PAPER_SPACE) {
            papers.push((i64::MAX, layout(Text::from("Layout1"), block)));
        }
    }
    papers.push(model);
    papers.sort_by_key(|(tab, _)| *tab);
    papers.into_iter().map(|(_, layout)| layout).collect()
}
