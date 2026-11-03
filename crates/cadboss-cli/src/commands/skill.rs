use std::io::Write;
use std::path::PathBuf;

use clap::{Args, Subcommand};

use crate::failure::Failure;

const SKILL: &str = include_str!("../../skill/SKILL.md");

#[derive(Args)]
pub struct SkillArgs {
    #[command(subcommand)]
    pub action: SkillAction,
}

#[derive(Subcommand)]
pub enum SkillAction {
    Install {
        #[arg(short = 'g', long)]
        global: bool,
    },
    Show,
}

fn home() -> Result<PathBuf, Failure> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .ok_or_else(|| Failure::usage("cannot find the home directory (HOME or USERPROFILE)"))
}

pub fn run(args: &SkillArgs, out: &mut dyn Write) -> Result<(), Failure> {
    match args.action {
        SkillAction::Show => out.write_all(SKILL.as_bytes())?,
        SkillAction::Install { global } => {
            let base = if global { home()? } else { PathBuf::from(".") };
            let directory = base.join(".claude").join("skills").join("cadboss");
            std::fs::create_dir_all(&directory)?;
            let path = directory.join("SKILL.md");
            std::fs::write(&path, SKILL)?;
            writeln!(out, "installed {}", path.display())?;
        }
    }
    Ok(())
}
