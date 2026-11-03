//! BLOCKS section definitions and their entity ranges.

use std::ops::Range;

use cadboss_core::{Point, Report, SkipKind, SkipReason};
use cadboss_model::Block;

use crate::codepage::{decode, Codepage};
use crate::record::{first, pairs_until, without_groups};
use crate::tape::{Cursor, SectionName, Tape};

const XREF: i64 = 4;

#[derive(Debug, Clone, PartialEq)]
pub struct BlockDef {
    pub block: Block,
    pub handle: u64,
    pub entities: Range<u32>,
}

pub fn read_blocks(tape: &Tape, bytes: &[u8], cp: Codepage, report: &mut Report) -> Vec<BlockDef> {
    let mut blocks = Vec::new();
    let Some(section) = tape.section(SectionName::Blocks) else {
        return blocks;
    };
    let mut cursor = Cursor::new(tape, bytes, section.start..section.end);
    while let Some((code, value)) = cursor.next() {
        if code != 0 || value != b"BLOCK" {
            continue;
        }
        let head = without_groups(pairs_until(&mut cursor, 0));
        let start = cursor.index();
        let end = loop {
            match cursor.peek() {
                None | Some((0, b"BLOCK")) => {
                    report.note(SkipKind::Section, SkipReason::Truncated);
                    break cursor.index();
                }
                Some((0, b"ENDBLK")) => {
                    let end = cursor.index();
                    cursor.next();
                    pairs_until(&mut cursor, 0);
                    break end;
                }
                Some(_) => {
                    cursor.next();
                }
            }
        };
        let Some(name) = first(&head, 2) else {
            report.note(SkipKind::Table, SkipReason::Malformed);
            continue;
        };
        let number = |code: i16| first(&head, code).and_then(|value| cursor.f64(value));
        let flags = first(&head, 70)
            .and_then(|value| cursor.i64(value))
            .unwrap_or(0);
        let xref_path = first(&head, 1)
            .filter(|path| flags & XREF != 0 && !path.is_empty())
            .map(|path| decode(cp, path, report));
        let name = decode(cp, name, report);
        blocks.push(BlockDef {
            block: Block {
                anonymous: name.as_str().is_some_and(|name| name.starts_with('*')),
                name,
                base: Point::new(
                    number(10).unwrap_or(0.0),
                    number(20).unwrap_or(0.0),
                    number(30).unwrap_or(0.0),
                ),
                xref_path,
                entity_count: 0,
            },
            handle: first(&head, 5)
                .and_then(|value| cursor.handle(value))
                .unwrap_or(0),
            entities: start..end,
        });
    }
    blocks
}
