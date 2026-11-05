//! The copy popup's targets, the key that picks each, and the text each copies from the selected row.

use cadboss_model::{Entity, Text, TextItem};
use crossterm::event::KeyCode;

use crate::app::App;
use crate::outline::{entity_text, hex, Group, Outline, Row, RowKind};
use crate::source::WIDE_HEX;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum YankTarget {
    Text,
    Layer,
    Handle,
    Dxf,
    Hex,
    Query,
    Command,
}

pub const TARGETS: [YankTarget; 7] = [
    YankTarget::Text,
    YankTarget::Layer,
    YankTarget::Handle,
    YankTarget::Dxf,
    YankTarget::Hex,
    YankTarget::Query,
    YankTarget::Command,
];

impl YankTarget {
    pub fn key(self) -> char {
        match self {
            YankTarget::Text => 't',
            YankTarget::Layer => 'l',
            YankTarget::Handle => 'h',
            YankTarget::Dxf => 'd',
            YankTarget::Hex => 'x',
            YankTarget::Query => 'q',
            YankTarget::Command => 'c',
        }
    }

    pub fn from_key(code: KeyCode) -> Option<YankTarget> {
        let KeyCode::Char(c) = code else {
            return None;
        };
        TARGETS.into_iter().find(|target| target.key() == c)
    }

    pub fn name(self) -> &'static str {
        match self {
            YankTarget::Text => "text",
            YankTarget::Layer => "layer",
            YankTarget::Handle => "handle",
            YankTarget::Dxf => "DXF",
            YankTarget::Hex => "hex",
            YankTarget::Query => "jq query",
            YankTarget::Command => "shell command",
        }
    }
}

/// Quotes `text` for a POSIX shell when it holds anything but safe characters.
pub fn shell_quote(text: &str) -> String {
    let safe = |c: char| c.is_ascii_alphanumeric() || "-_./:@%+=,".contains(c);
    if !text.is_empty() && text.chars().all(safe) {
        return text.to_owned();
    }
    format!("'{}'", text.replace('\'', r"'\''"))
}

/// What copying `target` takes from the selected row, empty when the row has none.
/// DXF is every pair of the record as `cadboss obj` prints them, hex the shown bytes sixteen to a line.
pub fn yank_text(app: &App, target: YankTarget) -> String {
    let outline = &app.outline;
    let Some(row) = outline.rows.get(app.selected) else {
        return String::new();
    };
    match target {
        YankTarget::Text => text(outline, row),
        YankTarget::Layer => layer(outline, &row.kind)
            .map(Text::to_string)
            .unwrap_or_default(),
        YankTarget::Handle => hex(handle(outline, &row.kind)),
        YankTarget::Dxf => dxf(app),
        YankTarget::Hex => app
            .source
            .hex_dump(&app.doc, WIDE_HEX)
            .into_iter()
            .map(|line| line + "\n")
            .collect(),
        YankTarget::Query => query(outline, &row.kind).unwrap_or_default(),
        YankTarget::Command => command(outline, &row.kind, &app.target).unwrap_or_default(),
    }
}

/// Every pair of the selected record, past the source limit too, one `{code}\t{value}` line each.
fn dxf(app: &App) -> String {
    let Some(tokens) = app.source.tokens.clone() else {
        return String::new();
    };
    let tape = app.doc.tape();
    tape.pairs(tokens)
        .map(|(index, code)| {
            let value = tape.display_value(code, tape.value(index, app.doc.bytes()));
            format!("{code}\t{value}\n")
        })
        .collect()
}

fn item<'o>(outline: &'o Outline, kind: &RowKind) -> Option<&'o TextItem> {
    let RowKind::TextItem(group, index) = kind else {
        return None;
    };
    group.items(&outline.document.text).get(*index)
}

fn entity<'o>(outline: &'o Outline, kind: &RowKind) -> Option<&'o Entity> {
    let RowKind::Entity { slot } = kind else {
        return None;
    };
    outline.entities.get(*slot).map(|located| &located.entity)
}

/// A text item's text, an entity's own text, or the row's label.
fn text(outline: &Outline, row: &Row) -> String {
    if let Some(item) = item(outline, &row.kind) {
        return item.text.to_string();
    }
    entity(outline, &row.kind)
        .and_then(entity_text)
        .filter(|text| !text.is_empty())
        .map_or_else(|| row.label.clone(), Text::to_string)
}

fn layer<'o>(outline: &'o Outline, kind: &RowKind) -> Option<&'o Text> {
    if let RowKind::Layer(index) = kind {
        return outline.document.layers.get(*index).map(|layer| &layer.name);
    }
    item(outline, kind)
        .map(|item| &item.layer)
        .or_else(|| entity(outline, kind).map(|entity| &entity.common.layer))
}

fn handle(outline: &Outline, kind: &RowKind) -> Option<u64> {
    item(outline, kind)
        .map(|item| item.handle)
        .or_else(|| entity(outline, kind).map(|entity| entity.common.handle))
        .flatten()
}

/// The jq program selecting this row in `cadboss json` output.
fn query(outline: &Outline, kind: &RowKind) -> Option<String> {
    let document = &outline.document;
    match kind {
        RowKind::TextItem(group, _) => {
            let text = json(&item(outline, kind)?.text)?;
            Some(format!(
                ".text.{}[] | select(.text == {text})",
                group.json_key()
            ))
        }
        RowKind::Layout(position) => by_name("layouts", &document.layouts.get(*position)?.name),
        RowKind::Layer(index) => by_name("layers", &document.layers.get(*index)?.name),
        RowKind::Block(index) => by_name("blocks", &document.blocks.get(*index)?.name),
        _ => None,
    }
}

fn by_name(list: &str, name: &Text) -> Option<String> {
    Some(format!(".{list}[] | select(.name == {})", json(name)?))
}

/// The JSON literal `cadboss json` writes for `text`: a string, or an object holding undecodable bytes.
fn json(text: &Text) -> Option<String> {
    serde_json::to_string(text).ok()
}

/// The `cadboss` command printing this row; a text item names its layout only when it has one.
fn command(outline: &Outline, kind: &RowKind, target: &str) -> Option<String> {
    let file = shell_quote(target);
    match kind {
        RowKind::Entity { .. } => {
            let handle = entity(outline, kind)?.common.handle?;
            Some(format!("cadboss obj {file} {handle:X}"))
        }
        RowKind::TextItem(Group::Properties, _) => None,
        RowKind::TextItem(group, _) => {
            let item = item(outline, kind)?;
            let line = format!("cadboss text {file} --group {}", group.cli_name());
            if item.layout.is_empty() {
                return Some(line);
            }
            Some(format!(
                "{line} --layout {}",
                shell_quote(&item.layout.to_string())
            ))
        }
        RowKind::Layout(position) => {
            let name = outline.document.layouts.get(*position)?.name.to_string();
            Some(format!(
                "cadboss json {file} --entities --layout {}",
                shell_quote(&name)
            ))
        }
        RowKind::Layer(_) => Some(format!("cadboss layers {file}")),
        RowKind::Block(_) => Some(format!("cadboss json {file}")),
        RowKind::File => Some(format!("cadboss info {file}")),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::sync::Arc;

    use cadboss_dxf::DxfDocument;
    use cadboss_model::{Text, TextItem, TextSource};
    use cadboss_testkit::dxf::DxfBuilder;
    use crossterm::event::{KeyEvent, KeyModifiers};

    use super::*;
    use crate::app::{App, Cmd, Msg};
    use crate::clipboard::Transport;
    use crate::outline::{Group, Outline, RowKind, ROOT};
    use crate::source::SOURCE_LIMIT;

    fn demo() -> App {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets/demo.dxf");
        let doc = Arc::new(DxfDocument::open(Path::new(path)).unwrap());
        let mut app = App::new(
            Arc::clone(&doc),
            "demo.dxf".to_owned(),
            "assets/demo.dxf".to_owned(),
            (80, 24),
        );
        app.update(Msg::Loaded(Box::new(Outline::build(&doc, "demo.dxf"))));
        app
    }

    fn row(app: &App, path: &[&str]) -> usize {
        path.iter().fold(ROOT, |row, label| {
            *app.outline.rows[row]
                .children
                .iter()
                .find(|child| app.outline.rows[**child].label == *label)
                .unwrap()
        })
    }

    fn yanks(app: &mut App, path: &[&str]) -> [String; 7] {
        app.select(row(app, path));
        TARGETS.map(|target| yank_text(app, target))
    }

    #[test]
    fn yank_text_builds_queries_and_commands() {
        let mut app = demo();

        let [text, layer, handle, dxf, hex, query, command] =
            yanks(&mut app, &["Text", "attributes", "TAG=P-101"]);
        assert_eq!(text, "TAG=P-101");
        assert_eq!(layer, "EQUIPMENT");
        assert_eq!(handle, "136");
        assert!(dxf.starts_with("0\tINSERT\n5\t136\n"));
        assert!(dxf.contains("\n2\tPUMP\n"));
        assert!(dxf.ends_with('\n'));
        assert_eq!(dxf.lines().count(), app.source.pairs.len());
        assert_eq!(app.source.bytes_per_line, 4);
        let start = app.source.bytes.start;
        let lines: Vec<&str> = hex.lines().collect();
        assert_eq!(lines.len(), app.source.bytes.len().div_ceil(16));
        assert_eq!(
            lines[0],
            format!(
                "{start:08x}  20 20 30 0a 49 4e 53 45  52 54 0a 20 20 35 0a 31  |  0.INSERT.  5.1|"
            )
        );
        assert!(lines[1].starts_with(&format!("{:08x}  ", start + 16)));
        assert!(hex.ends_with("|\n"));
        assert_eq!(
            query,
            r#".text.attributes[] | select(.text == "TAG=P-101")"#
        );
        assert_eq!(
            command,
            "cadboss text assets/demo.dxf --group attributes --layout Model"
        );

        let [text, _, _, _, _, query, command] = yanks(&mut app, &["Text", "title block", "DRAWN"]);
        assert_eq!(text, "DRAWN");
        assert_eq!(query, r#".text.title_block[] | select(.text == "DRAWN")"#);
        assert_eq!(
            command,
            "cadboss text assets/demo.dxf --group title-block --layout 'Sheet 1'"
        );

        let notes = app.outline.rows[row(&app, &["Text", "annotations"])]
            .children
            .iter()
            .copied()
            .find(|child| app.outline.rows[*child].label.starts_with("NOTES:"))
            .unwrap();
        app.select(notes);
        assert!(yank_text(&app, YankTarget::Text).starts_with("NOTES:\n1. ALL DIMENSIONS"));
        assert!(yank_text(&app, YankTarget::Query)
            .starts_with(r#".text.annotations[] | select(.text == "NOTES:\n1. ALL DIMENSIONS"#));

        let [text, layer, handle, dxf, hex, query, command] =
            yanks(&mut app, &["Text", "names", "WALLS"]);
        assert_eq!(text, "WALLS");
        assert_eq!([layer, handle, dxf, hex], ["", "", "", ""]);
        assert_eq!(query, r#".text.names[] | select(.text == "WALLS")"#);
        assert_eq!(command, "cadboss text assets/demo.dxf --group names");

        let [text, _, _, _, _, query, command] = yanks(&mut app, &["Layouts", "Sheet 1"]);
        assert_eq!(text, "Sheet 1");
        assert_eq!(query, r#".layouts[] | select(.name == "Sheet 1")"#);
        assert_eq!(
            command,
            "cadboss json assets/demo.dxf --entities --layout 'Sheet 1'"
        );

        let [text, layer, _, dxf, _, query, command] = yanks(&mut app, &["Layers", "WALLS"]);
        assert_eq!([text, layer], ["WALLS", "WALLS"]);
        assert!(dxf.starts_with("0\tLAYER\n"));
        assert_eq!(query, r#".layers[] | select(.name == "WALLS")"#);
        assert_eq!(command, "cadboss layers assets/demo.dxf");

        let [_, _, _, dxf, _, query, command] = yanks(&mut app, &["Blocks", "PUMP"]);
        assert!(dxf.starts_with("0\tBLOCK\n"));
        assert_eq!(query, r#".blocks[] | select(.name == "PUMP")"#);
        assert_eq!(command, "cadboss json assets/demo.dxf");

        app.select(ROOT);
        assert_eq!(yank_text(&app, YankTarget::Text), "demo.dxf");
        assert_eq!(yank_text(&app, YankTarget::Query), "");
        assert_eq!(
            yank_text(&app, YankTarget::Command),
            "cadboss info assets/demo.dxf"
        );
        assert!(yank_text(&app, YankTarget::Dxf).starts_with("9\t$ACADVER\n"));

        let [text, layer, handle, dxf, hex, query, command] = yanks(&mut app, &["Layouts"]);
        assert_eq!(text, "Layouts");
        assert_eq!(
            [layer, handle, dxf, hex, query, command],
            ["", "", "", "", "", ""]
        );

        app.select(row(&app, &["Layouts", "Model", "INSERT"]));
        app.update(Msg::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)));
        let [text, layer, handle, dxf, _, query, command] = yanks(
            &mut app,
            &["Layouts", "Model", "INSERT", "INSERT  EQUIPMENT  PUMP"],
        );
        assert!(matches!(
            app.outline.rows[app.selected].kind,
            RowKind::Entity { .. }
        ));
        assert_eq!([text, layer, handle], ["PUMP", "EQUIPMENT", "136"]);
        assert!(dxf.starts_with("0\tINSERT\n5\t136\n"));
        assert_eq!(query, "");
        assert_eq!(command, "cadboss obj assets/demo.dxf 136");
    }

    #[test]
    fn dxf_copy_of_a_record_past_the_limit_holds_every_pair_and_the_hex_toast_notes_the_cut() {
        let long = "A".repeat(70_000);
        let bytes = DxfBuilder::new().mtext(&long, (0.0, 0.0), 2.5).build();
        let doc = Arc::new(DxfDocument::load(bytes).unwrap());
        let mut app = App::new(
            Arc::clone(&doc),
            "long.dxf".to_owned(),
            "long.dxf".to_owned(),
            (80, 24),
        );
        app.update(Msg::Loaded(Box::new(Outline::build(&doc, "long.dxf"))));
        let group = row(&app, &["Layouts", "Model", "MTEXT"]);
        app.select(group);
        app.update(Msg::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)));
        app.select(app.outline.rows[group].children[0]);
        assert!(app.source.more > 0);
        let tokens = app.source.tokens.clone().unwrap();
        assert!(app.source.pairs.len() < tokens.len());

        let dxf = yank_text(&app, YankTarget::Dxf);
        let lines: Vec<&str> = dxf.lines().collect();
        assert_eq!(lines.len(), tokens.len());
        assert_eq!(lines[0], "0\tMTEXT");
        let text: String = lines
            .iter()
            .filter_map(|line| {
                line.strip_prefix("3\t")
                    .or_else(|| line.strip_prefix("1\t"))
            })
            .collect();
        assert_eq!(text, long);
        assert!(dxf.ends_with('\n'));

        let hex = yank_text(&app, YankTarget::Hex);
        assert_eq!(hex.lines().count(), SOURCE_LIMIT / 16);
        assert_eq!(app.source.hex.len(), SOURCE_LIMIT / 4);

        let press = |app: &mut App, c: char| {
            app.update(Msg::Key(KeyEvent::new(
                KeyCode::Char(c),
                KeyModifiers::NONE,
            )))
        };
        press(&mut app, 'y');
        assert_eq!(
            press(&mut app, 'x'),
            [Cmd::Copy {
                target: YankTarget::Hex,
                partial: true,
                text: hex,
            }]
        );
        let toast = |app: &mut App, transport: Transport| {
            app.update(Msg::Copied {
                target: YankTarget::Hex,
                partial: true,
                result: Ok(transport),
            });
            app.toast.as_ref().unwrap().text.clone()
        };
        assert_eq!(
            toast(&mut app, Transport::Native),
            "Copied hex (first 64 KiB)"
        );
        assert_eq!(
            toast(&mut app, Transport::Osc52),
            "Copied hex (first 64 KiB) through the terminal"
        );

        press(&mut app, 'y');
        assert_eq!(
            press(&mut app, 'd'),
            [Cmd::Copy {
                target: YankTarget::Dxf,
                partial: false,
                text: dxf,
            }]
        );
    }

    #[test]
    fn properties_items_have_no_shell_command() {
        let mut app = demo();
        app.outline.document.text.properties.push(TextItem {
            text: Text::from("Client=Nordstadt"),
            source: TextSource::Property,
            layer: Text::default(),
            layout: Text::from("Model"),
            position: None,
            height: None,
            handle: None,
        });
        let kind = RowKind::TextItem(Group::Properties, 0);
        assert_eq!(command(&app.outline, &kind, "assets/demo.dxf"), None);
        assert_eq!(
            query(&app.outline, &kind),
            Some(r#".text.properties[] | select(.text == "Client=Nordstadt")"#.to_owned())
        );
    }

    #[test]
    fn shell_quote_wraps_special_characters() {
        assert_eq!(shell_quote("assets/demo.dxf"), "assets/demo.dxf");
        assert_eq!(shell_quote("Model"), "Model");
        assert_eq!(shell_quote("Sheet 1"), "'Sheet 1'");
        assert_eq!(shell_quote("$HOME"), "'$HOME'");
        assert_eq!(shell_quote("it's"), r"'it'\''s'");
        assert_eq!(shell_quote(""), "''");
    }
}
