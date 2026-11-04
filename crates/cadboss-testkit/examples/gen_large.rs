use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use cadboss_testkit::dxf::realistic_batch;

const BATCH: usize = 10_000;
const PRELUDE_FIRST_HANDLE: u64 = 0x100;

struct Args {
    entities: usize,
    out: PathBuf,
}

fn parse(mut args: impl Iterator<Item = String>) -> Result<Args, String> {
    let mut entities = None;
    let mut out = None;
    while let Some(flag) = args.next() {
        let value = args.next().ok_or(format!("{flag} needs a value"))?;
        match flag.as_str() {
            "--entities" => {
                entities = Some(value.parse::<usize>().map_err(|error| error.to_string())?)
            }
            "--out" => out = Some(PathBuf::from(value)),
            other => return Err(format!("unknown flag {other}")),
        }
    }
    Ok(Args {
        entities: entities.ok_or("missing --entities")?,
        out: out.ok_or("missing --out")?,
    })
}

fn write(args: &Args) -> std::io::Result<()> {
    let (head, tail) = realistic_batch(0, 0)
        .with_first_handle(PRELUDE_FIRST_HANDLE)
        .build_around_entities();
    let mut writer = BufWriter::new(File::create(&args.out)?);
    writer.write_all(&head)?;
    let mut start = 0;
    while start < args.entities {
        let count = BATCH.min(args.entities - start);
        writer.write_all(&realistic_batch(start, count).entities_only())?;
        start += count;
    }
    writer.write_all(&tail)?;
    writer.flush()
}

fn main() -> ExitCode {
    let args = match parse(std::env::args().skip(1)) {
        Ok(args) => args,
        Err(message) => {
            eprintln!("gen_large: {message}");
            return ExitCode::from(2);
        }
    };
    match write(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("gen_large: {error}");
            ExitCode::FAILURE
        }
    }
}
