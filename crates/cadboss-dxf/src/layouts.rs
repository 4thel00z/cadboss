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
    Layout {
        name,
        kind: match is_named(&block_name, MODEL_SPACE) {
            true => LayoutKind::Model,
            false => LayoutKind::Paper,
        },
        block_name,
        extents: None,
        entity_count: 0,
    }
}

fn synthesized(block_records: &[BlockRecord]) -> Vec<Layout> {
    [(MODEL_SPACE, "Model"), (PAPER_SPACE, "Layout1")]
        .into_iter()
        .filter_map(|(block, name)| {
            let record = block_records
                .iter()
                .find(|record| is_named(&record.name, block))?;
            Some(layout(Text::from(name), record.name.clone()))
        })
        .collect()
}

/// Reads LAYOUT objects sorted by tab order; without any, derives `Model` and `Layout1` from the space block records.
pub fn read_layouts(
    tape: &Tape,
    bytes: &[u8],
    cp: Codepage,
    block_records: &[BlockRecord],
    budget: &Budget,
    report: &mut Report,
) -> Vec<Layout> {
    let Some(section) = tape.section(SectionName::Objects) else {
        return synthesized(block_records);
    };
    let mut cursor = Cursor::new(tape, bytes, section.start..section.end);
    let mut found = Vec::new();
    let mut seen = 0u32;
    while let Some((code, value)) = cursor.next() {
        if code != 0 || value != b"LAYOUT" {
            continue;
        }
        seen += 1;
        let pairs = without_groups(pairs_until(&mut cursor, 0));
        if found.len() >= budget.max_layouts as usize {
            report.note(SkipKind::Object, SkipReason::EntityLimit);
            break;
        }
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
    if seen == 0 {
        return synthesized(block_records);
    }
    found.sort_by_key(|(tab, _)| *tab);
    found.into_iter().map(|(_, layout)| layout).collect()
}
