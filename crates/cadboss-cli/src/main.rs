#![deny(unsafe_code)]

mod budget;
mod commands;
mod failure;
mod input;

use std::io::{BufWriter, Write};
use std::path::Path;
use std::process::ExitCode;

use cadboss_core::Report;
use cadboss_dxf::{DxfDocument, Options};
use clap::{Args, Parser, Subcommand};

use crate::commands::info::InfoArgs;
use crate::commands::json::JsonArgs;
use crate::commands::layers::LayersArgs;
use crate::commands::layouts::LayoutsArgs;
use crate::commands::md::MdArgs;
use crate::commands::obj::ObjArgs;
use crate::commands::q::QArgs;
use crate::commands::skill::SkillArgs;
use crate::commands::text::TextArgs;
use crate::failure::Failure;

#[derive(Args)]
struct Common {
    #[arg(long, value_name = "KEY=VALUE", global = true, action = clap::ArgAction::Append)]
    budget: Vec<String>,
    /// Reserved for internal parallelism; currently unused.
    #[arg(long, value_name = "N", global = true, default_value_t = 1)]
    threads: usize,
}

#[derive(Parser)]
#[command(
    name = "cadboss",
    version,
    about = "Reads CAD files and prints their text, metadata and structure",
    long_about = "Reads CAD files (DXF today) and prints their text, metadata and structure.\n\nThe input file is memory-mapped. Truncating or rewriting it while cadboss runs can crash the process, so do not point cadboss at a file another program is still writing; copy it first."
)]
struct Cli {
    #[command(flatten)]
    common: Common,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    Info(InfoArgs),
    Text(TextArgs),
    Md(MdArgs),
    Json(JsonArgs),
    Q(QArgs),
    Layouts(LayoutsArgs),
    Layers(LayersArgs),
    Obj(ObjArgs),
    Skill(SkillArgs),
}

fn open(file: &Path, common: &Common) -> Result<DxfDocument, Failure> {
    let options = Options {
        budget: budget::parse(&common.budget)?,
        threads: common.threads,
    };
    input::open(file, &options)
}

fn execute(cli: &Cli, out: &mut dyn Write) -> Result<Report, Failure> {
    let common = &cli.common;
    match &cli.command {
        Command::Info(args) => commands::info::run(&open(&args.file, common)?, out),
        Command::Text(args) => commands::text::run(args, &open(&args.file, common)?, out),
        Command::Md(args) => commands::md::run(args, &open(&args.file, common)?, out),
        Command::Json(args) => commands::json::run(args, &open(&args.file, common)?, out),
        Command::Q(args) => commands::q::run(args, &open(&args.file, common)?, out),
        Command::Layouts(args) => commands::layouts::run(&open(&args.file, common)?, out),
        Command::Layers(args) => commands::layers::run(&open(&args.file, common)?, out),
        Command::Obj(args) => commands::obj::run(args, &open(&args.file, common)?, out),
        Command::Skill(args) => {
            budget::parse(&common.budget)?;
            commands::skill::run(args, out)?;
            Ok(Report::default())
        }
    }
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let stdout = std::io::stdout();
    let mut out = BufWriter::new(stdout.lock());
    let result = execute(&cli, &mut out).and_then(|report| {
        out.flush()?;
        Ok(report)
    });
    drop(out);
    let mut err = std::io::stderr();
    match result {
        Ok(report) => {
            if !report.is_empty() {
                writeln!(err, "warning: {}", report.summary()).ok();
            }
            ExitCode::SUCCESS
        }
        Err(failure) if failure.broken_pipe => ExitCode::SUCCESS,
        Err(failure) => {
            writeln!(err, "cadboss: {}", failure.message).ok();
            ExitCode::from(u8::try_from(failure.code).unwrap_or(1))
        }
    }
}
