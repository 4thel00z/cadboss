use std::io::Write;
use std::path::PathBuf;

use cadboss_core::Report;
use cadboss_dxf::DxfDocument;
use clap::Args;
use jaq_core::load::{Arena, File, Loader};
use jaq_core::{data, unwrap_valr, Compiler, Ctx, Vars};
use jaq_json::{read, Val};

use crate::commands::json::build;
use crate::failure::Failure;

#[derive(Args)]
pub struct QArgs {
    pub file: PathBuf,
    pub program: String,
    #[arg(short = 'r', long)]
    pub raw: bool,
    #[arg(long)]
    pub entities: bool,
    #[arg(long)]
    pub layout: Option<String>,
}

fn render(value: &Val, raw: bool) -> Vec<u8> {
    if let (true, Val::TStr(bytes)) = (raw, value) {
        return bytes.to_vec();
    }
    let compact = value.to_string();
    let pretty = serde_json::from_str::<serde_json::Value>(&compact)
        .ok()
        .filter(|parsed| parsed.is_object() || parsed.is_array())
        .and_then(|parsed| serde_json::to_string_pretty(&parsed).ok());
    pretty.unwrap_or(compact).into_bytes()
}

pub fn run(args: &QArgs, doc: &DxfDocument, out: &mut dyn Write) -> Result<Report, Failure> {
    let (value, report) = build(doc, args.entities, args.layout.as_deref())?;
    let text = serde_json::to_string(&value)?;
    let input = read::parse_single(text.as_bytes())
        .map_err(|error| Failure::input(format!("{error:?}")))?;
    let defs = jaq_core::defs()
        .chain(jaq_std::defs())
        .chain(jaq_json::defs());
    let funs = jaq_core::funs()
        .chain(jaq_std::funs())
        .chain(jaq_json::funs());
    let arena = Arena::default();
    let program = File {
        code: args.program.as_str(),
        path: (),
    };
    let modules = Loader::new(defs)
        .load(&arena, program)
        .map_err(|errors| Failure::usage(format!("invalid jq program: {errors:?}")))?;
    let filter = Compiler::default()
        .with_funs(funs)
        .compile(modules)
        .map_err(|errors| Failure::usage(format!("invalid jq program: {errors:?}")))?;
    let ctx = Ctx::<data::JustLut<Val>>::new(&filter.lut, Vars::new([]));
    for result in filter.id.run((ctx, input)).map(unwrap_valr) {
        let output = result.map_err(|error| Failure::input(format!("{error}")))?;
        out.write_all(&render(&output, args.raw))?;
        out.write_all(b"\n")?;
    }
    Ok(report)
}
