use std::io::Write;
use std::path::PathBuf;

use cadboss_core::Report;
use cadboss_dxf::{Cursor, DxfDocument, SectionName};
use clap::Args;

use crate::failure::Failure;

type Pair<'t> = (i16, &'t [u8]);

#[derive(Args)]
pub struct ObjArgs {
    pub file: PathBuf,
    pub handle: String,
}

fn records<'t>(cursor: Cursor<'t>) -> Vec<Vec<Pair<'t>>> {
    let mut records: Vec<Vec<Pair<'t>>> = Vec::new();
    for pair in cursor {
        match records.last_mut() {
            Some(record) if pair.0 != 0 => record.push(pair),
            _ => records.push(vec![pair]),
        }
    }
    records
}

fn find(doc: &DxfDocument, handle: u64) -> Option<Vec<Pair<'_>>> {
    let tape = doc.tape();
    let bytes = doc.bytes();
    [
        SectionName::Blocks,
        SectionName::Entities,
        SectionName::Objects,
    ]
    .into_iter()
    .filter_map(|name| tape.section(name))
    .flat_map(|section| {
        let cursor = Cursor::new(tape, bytes, section.start..section.end);
        let reader = Cursor::new(tape, bytes, section.start..section.end);
        records(cursor).into_iter().filter(move |record| {
            record
                .iter()
                .find(|(code, _)| *code == 5)
                .and_then(|(_, value)| reader.handle(value))
                == Some(handle)
        })
    })
    .next()
}

pub fn run(args: &ObjArgs, doc: &DxfDocument, out: &mut dyn Write) -> Result<Report, Failure> {
    let digits = args.handle.trim_start_matches("0x");
    let handle = u64::from_str_radix(digits, 16)
        .map_err(|_| Failure::usage(format!("invalid handle {}: expected hex", args.handle)))?;
    let record = find(doc, handle).ok_or_else(|| Failure::input("handle not found"))?;
    for (code, value) in record {
        writeln!(out, "{code}\t{}", String::from_utf8_lossy(value))?;
    }
    Ok(doc.report().clone())
}
