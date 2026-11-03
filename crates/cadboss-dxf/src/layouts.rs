//! LAYOUT objects resolved against the block records.

use cadboss_core::{Budget, Report, SkipKind, SkipReason};
use cadboss_model::{Layout, LayoutKind, Text};

use crate::codepage::{decode, Codepage};
use crate::record::{first, last, pairs_until, without_groups};
use crate::tables::BlockRecord;
use crate::tape::{Cursor, SectionName, Tape};

const MODEL_SPACE: &str = "*Model_Space";
const PAPER_SPACE: &str = "*Paper_Space";

fn is_named(name: &Text, expected: &str) -> bool {
    name.as_str()
        .is_some_and(|name| name.eq_ignore_ascii_case(expected))
}

fn layout(name: Text, block_name: Text) -> Layout {
    let model = is_named(&block_name, MODEL_SPACE);
    Layout {
        name,
        kind: if model {
            LayoutKind::Model
        } else {
            LayoutKind::Paper
        },
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

/// Reads LAYOUT objects sorted by tab order and capped at `max_layouts`.
/// A `Model` layout is always present; `Layout1` is synthesized when no paper layout exists but a paper space block record does.
pub fn read_layouts(
    tape: &Tape,
    bytes: &[u8],
    cp: Codepage,
    block_records: &[BlockRecord],
    budget: &Budget,
    report: &mut Report,
) -> Vec<Layout> {
    let mut found = Vec::new();
    if let Some(section) = tape.section(SectionName::Objects) {
        let mut cursor = Cursor::new(tape, bytes, section.start..section.end);
        while let Some((code, value)) = cursor.next() {
            if code != 0 || value != b"LAYOUT" {
                continue;
            }
            let pairs = without_groups(pairs_until(&mut cursor, 0));
            let record = last(&pairs, 330)
                .and_then(|value| cursor.handle(value))
                .and_then(|handle| block_records.iter().find(|record| record.handle == handle));
            let (Some(name), Some(record)) = (last(&pairs, 1), record) else {
                report.note(SkipKind::Object, SkipReason::Malformed);
                continue;
            };
            let tab = first(&pairs, 71)
                .and_then(|value| cursor.i64(value))
                .unwrap_or(0);
            found.push((tab, layout(decode(cp, name, report), record.name.clone())));
        }
    }
    found.sort_by_key(|(tab, _)| *tab);
    let mut layouts: Vec<Layout> = found.into_iter().map(|(_, layout)| layout).collect();
    let max = budget.max_layouts as usize;
    if layouts.len() > max {
        report.note(SkipKind::Object, SkipReason::EntityLimit);
        let model = layouts
            .iter()
            .position(|layout| layout.kind == LayoutKind::Model)
            .filter(|index| *index >= max)
            .map(|index| layouts.swap_remove(index));
        layouts.truncate(max);
        if let Some(model) = model {
            layouts.pop();
            layouts.insert(0, model);
        }
    }
    if !layouts
        .iter()
        .any(|layout| layout.kind == LayoutKind::Model)
    {
        let block =
            space_block(block_records, MODEL_SPACE).unwrap_or_else(|| Text::from(MODEL_SPACE));
        layouts.insert(0, layout(Text::from("Model"), block));
    }
    if layouts
        .iter()
        .any(|layout| layout.kind == LayoutKind::Paper)
    {
        return layouts;
    }
    if let Some(block) = space_block(block_records, PAPER_SPACE) {
        layouts.push(layout(Text::from("Layout1"), block));
    }
    layouts
}
