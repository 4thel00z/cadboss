use std::io::Write;
use std::path::PathBuf;

use cadboss_core::Report;
use cadboss_dxf::DxfDocument;
use cadboss_model::{Text, TextItem};
use clap::{Args, ValueEnum};

use crate::failure::Failure;

#[derive(Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Group {
    TitleBlock,
    Annotations,
    Names,
    Attributes,
    References,
    All,
}

#[derive(Args)]
pub struct TextArgs {
    pub file: PathBuf,
    #[arg(long)]
    pub layout: Option<String>,
    #[arg(long, value_enum, default_value = "all")]
    pub group: Group,
}

pub fn run(args: &TextArgs, doc: &DxfDocument, out: &mut dyn Write) -> Result<Report, Failure> {
    let (groups, report) = doc.text_reporting();
    let entries: [(Option<Group>, &str, &[TextItem]); 6] = [
        (Some(Group::TitleBlock), "title-block", &groups.title_block),
        (Some(Group::Annotations), "annotations", &groups.annotations),
        (Some(Group::Names), "names", &groups.names),
        (Some(Group::Attributes), "attributes", &groups.attributes),
        (Some(Group::References), "references", &groups.references),
        (None, "properties", &groups.properties),
    ];
    let layout = args.layout.as_deref().map(Text::from);
    for (group, label, items) in entries {
        let selected = args.group == Group::All || group == Some(args.group);
        let scoped = layout.is_none() || !matches!(group, None | Some(Group::Names));
        if !selected || !scoped {
            continue;
        }
        let kept: Vec<&TextItem> = items
            .iter()
            .filter(|item| layout.as_ref().is_none_or(|name| item.layout == *name))
            .collect();
        if kept.is_empty() {
            continue;
        }
        if args.group == Group::All {
            writeln!(out, "# {label}")?;
        }
        for item in kept {
            writeln!(out, "{}", item.text)?;
        }
    }
    Ok(report)
}
