//! LAYER and BLOCK_RECORD symbol tables.

use cadboss_core::{Report, SkipKind, SkipReason};
use cadboss_model::{Layer, Text};

use crate::codepage::{decode, Codepage};
use crate::record::Record;
use crate::tape::{Cursor, SectionName, Tape};

const DEFAULT_COLOR: i16 = 7;
const FROZEN: i64 = 1;
const LOCKED: i64 = 4;

#[derive(Debug, Clone, PartialEq)]
pub struct BlockRecord {
    pub handle: u64,
    pub name: Text,
    pub layout_handle: Option<u64>,
}

fn table_records<'a>(
    tape: &'a Tape,
    bytes: &'a [u8],
    table: &[u8],
    record: &[u8],
) -> Vec<Record<'a>> {
    let mut records = Vec::new();
    let Some(section) = tape.section(SectionName::Tables) else {
        return records;
    };
    let mut cursor = Cursor::new(tape, bytes, section.start..section.end);
    let mut inside = false;
    while let Some((code, value)) = cursor.next() {
        if code != 0 {
            continue;
        }
        if value == b"TABLE" {
            let named = cursor.peek_code() == Some(2);
            inside = named && cursor.next().is_some_and(|(_, name)| name == table);
            continue;
        }
        if value == b"ENDTAB" {
            inside = false;
            continue;
        }
        if inside && value == record {
            records.push(Record::until(&mut cursor, 0));
        }
    }
    records
}

pub fn read_layers(tape: &Tape, bytes: &[u8], cp: Codepage, report: &mut Report) -> Vec<Layer> {
    let records = table_records(tape, bytes, b"LAYER", b"LAYER");
    let mut layers = Vec::with_capacity(records.len());
    for pairs in &records {
        let Some(name) = pairs.first(2) else {
            report.note(SkipKind::Table, SkipReason::Malformed);
            continue;
        };
        let signed = pairs
            .first(62)
            .and_then(|value| pairs.i64(value))
            .and_then(|value| i16::try_from(value).ok())
            .unwrap_or(DEFAULT_COLOR);
        let flags = pairs
            .first(70)
            .and_then(|value| pairs.i64(value))
            .unwrap_or(0);
        layers.push(Layer {
            name: decode(cp, name, report),
            color: signed.saturating_abs(),
            frozen: flags & FROZEN != 0,
            off: signed < 0,
            locked: flags & LOCKED != 0,
        });
    }
    layers
}

pub fn read_block_records(
    tape: &Tape,
    bytes: &[u8],
    cp: Codepage,
    report: &mut Report,
) -> Vec<BlockRecord> {
    let records = table_records(tape, bytes, b"BLOCK_RECORD", b"BLOCK_RECORD");
    let mut block_records = Vec::with_capacity(records.len());
    for pairs in &records {
        let Some(name) = pairs.first(2) else {
            report.note(SkipKind::Table, SkipReason::Malformed);
            continue;
        };
        block_records.push(BlockRecord {
            handle: pairs
                .first(5)
                .and_then(|value| pairs.handle(value))
                .unwrap_or(0),
            name: decode(cp, name, report),
            layout_handle: pairs.first(340).and_then(|value| pairs.handle(value)),
        });
    }
    block_records
}
