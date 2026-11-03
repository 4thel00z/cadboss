use std::io::Write;
use std::ops::Range;
use std::path::PathBuf;

use cadboss_core::Report;
use cadboss_dxf::{value_kind, Cursor, DxfDocument, SectionName, ValueKind};
use clap::Args;

use crate::failure::Failure;

#[derive(Args)]
pub struct ObjArgs {
    pub file: PathBuf,
    pub handle: String,
}

/// Token range from the code-0 pair of the first record whose first handle pair is `handle` to the end of its section.
fn locate(doc: &DxfDocument, handle: u64) -> Option<Range<u32>> {
    let tape = doc.tape();
    let reader = Cursor::new(tape, doc.bytes(), 0..0);
    [
        SectionName::Tables,
        SectionName::Blocks,
        SectionName::Entities,
        SectionName::Objects,
    ]
    .into_iter()
    .filter_map(|name| tape.section(name))
    .find_map(|section| {
        let mut start = section.start;
        let mut checked = false;
        for (index, code) in tape.pairs(section.start..section.end) {
            if code == 0 {
                start = index;
                checked = false;
                continue;
            }
            if code != 5 || checked {
                continue;
            }
            checked = true;
            if reader.handle(tape.value(index, doc.bytes())) == Some(handle) {
                return Some(start..section.end);
            }
        }
        None
    })
}

fn display(cursor: &Cursor<'_>, binary: bool, code: i16, value: &[u8]) -> String {
    let typed = match (binary, value_kind(code)) {
        (false, _) | (_, ValueKind::Text) => None,
        (true, ValueKind::Float) => cursor.f64(value).map(|number| number.to_string()),
        (true, ValueKind::Integer) => cursor.i64(value).map(|number| number.to_string()),
    };
    typed.unwrap_or_else(|| String::from_utf8_lossy(value).into_owned())
}

pub fn run(args: &ObjArgs, doc: &DxfDocument, out: &mut dyn Write) -> Result<Report, Failure> {
    let digits = args.handle.trim_start_matches("0x");
    let handle = u64::from_str_radix(digits, 16)
        .map_err(|_| Failure::usage(format!("invalid handle {}: expected hex", args.handle)))?;
    let range = locate(doc, handle).ok_or_else(|| Failure::input("handle not found"))?;
    let mut record = Cursor::new(doc.tape(), doc.bytes(), range);
    let reader = Cursor::new(doc.tape(), doc.bytes(), 0..0);
    let binary = doc.tape().is_binary();
    let head = record.next();
    for (code, value) in head
        .into_iter()
        .chain(record.take_while(|(code, _)| *code != 0))
    {
        writeln!(out, "{code}\t{}", display(&reader, binary, code, value))?;
    }
    Ok(doc.report().clone())
}
