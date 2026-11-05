//! BLOCKS section definitions and their entity ranges.

use std::ops::Range;

use cadboss_core::{Point, Report, SkipKind, SkipReason};
use cadboss_model::Block;

use crate::codepage::{decode, Codepage};
use crate::record::Record;
use crate::tape::{Cursor, SectionName, Tape};

const XREF: i64 = 4;

#[derive(Debug, Clone, PartialEq)]
pub struct BlockDef {
    pub block: Block,
    pub handle: u64,
    pub header: Range<u32>,
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
        let header_start = cursor.index() - 1;
        let head = Record::until(&mut cursor, 0);
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
                    cursor.skip_to_code_zero();
                    break end;
                }
                Some(_) => {
                    cursor.next();
                }
            }
        };
        let Some(name) = head.first(2) else {
            report.note(SkipKind::Table, SkipReason::Malformed);
            continue;
        };
        let number = |code: i16| head.first(code).and_then(|value| head.f64(value));
        let flags = head
            .first(70)
            .and_then(|value| head.i64(value))
            .unwrap_or(0);
        let xref_path = head
            .first(1)
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
            handle: head
                .first(5)
                .and_then(|value| head.handle(value))
                .unwrap_or(0),
            header: header_start..start,
            entities: start..end,
        });
    }
    blocks
}

#[cfg(test)]
mod tests {
    use cadboss_testkit::dxf::DxfBuilder;

    use crate::DxfDocument;

    #[test]
    fn block_header_is_the_block_record_before_its_entities() {
        let bytes = DxfBuilder::new()
            .block("PUMP", (0.0, 0.0))
            .circle((0.0, 0.0), 1.0)
            .end()
            .build();
        let doc = DxfDocument::load(bytes).unwrap();
        let def = doc.block_def("pump").unwrap();
        assert_eq!(doc.tape().value(def.header.start, doc.bytes()), b"BLOCK");
        assert_eq!(def.header.end, def.entities.start);
        assert!(def.header.len() > 1);
    }
}
