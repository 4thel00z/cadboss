//! Full frames on an 80x24 TestBackend over the demo drawing and the binary fixture, and the keys that lead to them.

use std::error::Error;
use std::path::Path;
use std::sync::Arc;

use cadboss_dxf::DxfDocument;
use cadboss_testkit::dxf::DxfBuilder;
use cadboss_tui::app::{App, Msg, Pane};
use cadboss_tui::outline::Outline;
use cadboss_tui::source::View;
use cadboss_tui::ui;
use cadboss_tui::yank::YankTarget;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Modifier;
use ratatui::Terminal;

type TestResult = Result<(), Box<dyn Error>>;

fn repo(path: &str) -> String {
    format!("{}/../../{path}", env!("CARGO_MANIFEST_DIR"))
}

fn loaded(doc: DxfDocument, title: &str, target: &str) -> App {
    let doc = Arc::new(doc);
    let mut app = App::new(
        Arc::clone(&doc),
        title.to_owned(),
        target.to_owned(),
        (80, 24),
    );
    app.update(Msg::Loaded(Box::new(Outline::build(&doc, title))));
    app
}

fn demo() -> Result<App, Box<dyn Error>> {
    let doc = DxfDocument::open(Path::new(&repo("assets/demo.dxf")))?;
    Ok(loaded(doc, "demo.dxf", "assets/demo.dxf"))
}

fn press(app: &mut App, code: KeyCode) {
    press_with(app, code, KeyModifiers::NONE);
}

fn press_with(app: &mut App, code: KeyCode, modifiers: KeyModifiers) {
    app.update(Msg::Key(KeyEvent::new(code, modifiers)));
}

fn type_text(app: &mut App, text: &str) {
    for c in text.chars() {
        press(app, KeyCode::Char(c));
    }
}

fn render_sized(app: &mut App, width: u16, height: u16) -> Result<Buffer, Box<dyn Error>> {
    let mut terminal = Terminal::new(TestBackend::new(width, height))?;
    terminal.draw(|frame| ui::draw(app, frame))?;
    Ok(terminal.backend().buffer().clone())
}

fn render(app: &mut App) -> Result<Buffer, Box<dyn Error>> {
    render_sized(app, 80, 24)
}

fn line(buffer: &Buffer, y: u16) -> String {
    let text: String = (0..buffer.area.width)
        .filter_map(|x| buffer.cell((x, y)).map(|cell| cell.symbol().to_owned()))
        .collect();
    text.trim_end().to_owned()
}

fn frame(app: &mut App) -> Result<Vec<String>, Box<dyn Error>> {
    let buffer = render(app)?;
    Ok((0..buffer.area.height).map(|y| line(&buffer, y)).collect())
}

fn assert_frame(app: &mut App, expected: &[&str]) -> TestResult {
    let lines = frame(app)?;
    let printed = lines.join("\n");
    assert_eq!(lines.len(), expected.len(), "{printed}");
    for (index, (got, want)) in lines.iter().zip(expected).enumerate() {
        assert_eq!(got, want.trim_end(), "line {index}\n{printed}");
    }
    Ok(())
}

fn label(app: &App) -> String {
    app.outline
        .rows
        .get(app.selected)
        .map(|row| row.label.clone())
        .unwrap_or_default()
}

fn contains(lines: &[String], text: &str) -> bool {
    lines.iter().any(|line| line.contains(text))
}

#[test]
fn opening_shows_the_drawing() -> TestResult {
    let mut app = demo()?;
    assert_frame(
        &mut app,
        &[
            " cadboss   demo.dxf   DXF AC1032   15 entities in 3 layouts       35 text items",
            "╭ Outline ───────────────────────╮╭ Drawing ───────────────────────────────────╮",
            "│▌ ▾ demo.dxf                    ││ Format     DXF AC1032                      │",
            "│    ▸ Layouts                 3 ││ Units      Millimeters                     │",
            "│    ▸ Blocks                  6 ││ Codepage   ANSI_1252                       │",
            "│    ▸ Layers                  7 ││ Author     cadboss-testkit                 │",
            "│    ▸ Text                    6 ││ Created    2460371.5                       │",
            "│                                ││ Updated    2460372.5                       │",
            "│                                ││ Extents    (0, 0) to (12500, 8000)         │",
            "│                                ││ Layers     7                               │",
            "│                                ││ Blocks     3                               │",
            "│                                ││ Layouts    3                               │",
            "│                                ││ Entities   15                              │",
            "│                                ││                                            │",
            "│                                │╰────────────────────────────────────────────╯",
            "│                                │╭ DXF ────────────────────────── 0x17..0x10e ╮",
            "│                                ││    9  $ACADVER                             ┃",
            "│                                ││    1  AC1032                               ┃",
            "│                                ││    9  $DWGCODEPAGE                         │",
            "│                                ││    3  ANSI_1252                            │",
            "│                                ││    9  $INSUNITS                            │",
            "│                                ││   70  4                                    │",
            "╰────────────────────────────────╯╰────────────────────────────────────────────╯",
            " browse   / search   ] next layout   x hex   y copy   ? keys   q quit",
        ],
    )
}

#[test]
fn walking_down_to_an_attribute_shows_its_fields_and_record() -> TestResult {
    let mut app = demo()?;
    type_text(&mut app, "Gll");
    assert_eq!(label(&app), "title block");
    type_text(&mut app, "jjjll");
    assert_eq!(label(&app), "TAG=P-101");
    assert_frame(
        &mut app,
        &[
            " cadboss   demo.dxf   DXF AC1032   15 entities in 3 layouts       35 text items",
            "╭ Outline ───────────────────────╮╭ Attribute ─────────────────────────────────╮",
            "│  ▾ demo.dxf                    ┃│ TAG=P-101                                  │",
            "│    ▸ Layouts                 3 ┃│                                            │",
            "│    ▸ Blocks                  6 ┃│ Source     Attribute                       │",
            "│    ▸ Layers                  7 ┃│ Layer      EQUIPMENT                       │",
            "│    ▾ Text                    6 ┃│ Layout     Model                           │",
            "│      ▸ title block           4 ┃│ Position   0, 0                            │",
            "│      ▸ annotations           7 ┃│ Height     2.5                             │",
            "│      ▸ names                10 ┃│ Handle     136                             │",
            "│      ▾ attributes           11 ┃│                                            │",
            "│▌     │   TAG=P-101             ┃│                                            │",
            "│      │   DUTY=DUTY             ┃│                                            │",
            "│      │   TAG=P-102             ┃│                                            │",
            "│      │   DUTY=STANDBY          ┃╰────────────────────────────────────────────╯",
            "│      │   TITLE=PUMP STATION …  ┃╭ DXF ─────────────────────── 0x16ee..0x18a3 ╮",
            "│      │   DWG_NO=PS-101-M-0001  ┃│    0  INSERT                               ┃",
            "│      │   REV=C                 ┃│    5  136                                  │",
            "│      │   SCALE=1:50            ┃│  330  109                                  │",
            "│      │   DRAWN=M. TAHRIOUI     ┃│  100  AcDbEntity                           │",
            "│      │   DATE=2026-11-05       ┃│    8  EQUIPMENT                            │",
            "│      │   CLIENT=NORDSTADT WA…  ││  100  AcDbBlockReference                   │",
            "╰────────────────────────────────╯╰────────────────────────────────────────────╯",
            " browse   / search   ] next layout   x hex   y copy   ? keys   q quit",
        ],
    )
}

#[test]
fn search_marks_hits_and_counts_them() -> TestResult {
    let mut app = demo()?;
    press(&mut app, KeyCode::Char('/'));
    type_text(&mut app, "p-10");
    let prompt = ui::status_text(&app);
    assert!(prompt.starts_with(" search   /p-10█"), "{prompt}");
    assert!(
        prompt.ends_with("4 matches   enter go   esc cancel"),
        "{prompt}"
    );

    press(&mut app, KeyCode::Enter);
    assert!(label(&app).starts_with("NOTES:"));
    let status = ui::status_text(&app);
    assert!(status.ends_with("match 1 of 4   n next"), "{status}");
    press(&mut app, KeyCode::Char('n'));
    assert_eq!(label(&app), "TAG=P-101");

    let buffer = render(&mut app)?;
    let accent = app.theme.accent;
    let y = (0..buffer.area.height)
        .find(|y| line(&buffer, *y).contains("▌     │   TAG=P-101"))
        .unwrap();
    let start = line(&buffer, y).find("TAG=P-101").unwrap();
    let column = |text: &str| u16::try_from(line(&buffer, y)[..start].chars().count() + text.len());
    let tag = buffer.cell((column("")?, y)).unwrap();
    assert!(!tag.modifier.contains(Modifier::UNDERLINED));
    for offset in ["TAG=", "TAG=P", "TAG=P-", "TAG=P-1"] {
        let cell = buffer.cell((column(offset)?, y)).unwrap();
        assert_eq!(cell.fg, accent, "{offset}");
        assert!(cell
            .modifier
            .contains(Modifier::BOLD | Modifier::UNDERLINED));
    }
    let after = buffer.cell((column("TAG=P-10")?, y)).unwrap();
    assert!(!after.modifier.contains(Modifier::UNDERLINED));
    Ok(())
}

#[test]
fn undecodable_text_shows_its_bytes_in_hex() -> TestResult {
    let bytes = DxfBuilder::new()
        .codepage("UNKNOWN")
        .text_bytes(&[0xff, 0x41], (0.0, 0.0), 12.5)
        .build();
    let mut app = loaded(DxfDocument::load(bytes)?, "odd.dxf", "odd.dxf");
    type_text(&mut app, "Gll");
    press(&mut app, KeyCode::Char('j'));
    press(&mut app, KeyCode::Char('l'));
    press(&mut app, KeyCode::Char('l'));
    assert!(label(&app).contains("non-decodable"), "{}", label(&app));
    let lines = frame(&mut app)?;
    assert!(
        contains(&lines, "[non-decodable text, 2 bytes]"),
        "{lines:?}"
    );

    press(&mut app, KeyCode::Char('x'));
    assert_eq!(app.view, View::Hex);
    for _ in 0..9 {
        press_with(&mut app, KeyCode::Left, KeyModifiers::ALT);
        press_with(&mut app, KeyCode::Up, KeyModifiers::ALT);
    }
    let lines = frame(&mut app)?;
    assert!(contains(&lines, "╭ Hex "), "{lines:?}");
    assert!(contains(&lines, "ff 41"), "{lines:?}");
    Ok(())
}

#[test]
fn binary_drawing_opens_in_hex_and_toggles_to_pairs() -> TestResult {
    let doc = DxfDocument::open(Path::new(&repo("tests/fixtures/simple-binary.dxf")))?;
    let mut app = loaded(doc, "simple-binary.dxf", "tests/fixtures/simple-binary.dxf");
    assert_eq!(app.view, View::Hex);
    let lines = frame(&mut app)?;
    assert!(contains(&lines, "╭ Hex "), "{lines:?}");
    assert!(contains(&lines, " 0x29..0x105 ╮"), "{lines:?}");
    assert!(
        contains(&lines, "│ 00000029  09 00 24 41 43 41 44 56"),
        "{lines:?}"
    );

    type_text(&mut app, "jllll");
    assert_eq!(label(&app), "CIRCLE");
    type_text(&mut app, "ll");
    assert_eq!(label(&app), "CIRCLE  DIM");
    let lines = frame(&mut app)?;
    assert!(contains(&lines, "╭ Hex "), "{lines:?}");

    press(&mut app, KeyCode::Char('x'));
    assert_eq!(app.view, View::Pairs);
    let lines = frame(&mut app)?;
    assert!(contains(&lines, "╭ DXF "), "{lines:?}");
    assert!(contains(&lines, "│    0  CIRCLE"), "{lines:?}");
    assert!(contains(&lines, "│    8  DIM"), "{lines:?}");

    press(&mut app, KeyCode::Tab);
    press(&mut app, KeyCode::Tab);
    type_text(&mut app, "jjjj");
    let lines = frame(&mut app)?;
    assert!(contains(&lines, "│   10  60 "), "{lines:?}");
    assert!(contains(&lines, "│   40  5 "), "{lines:?}");
    Ok(())
}

#[test]
fn help_overlay_lists_the_keys_and_any_key_closes_it() -> TestResult {
    let mut app = demo()?;
    press(&mut app, KeyCode::Char('?'));
    assert!(app.help);
    assert!(ui::status_text(&app).starts_with(" keys "));
    let lines = frame(&mut app)?;
    assert!(contains(&lines, "╭ Keys "), "{lines:?}");
    assert!(contains(&lines, "any key closes"), "{lines:?}");
    for help in [
        "↑ ↓  j k      move, or scroll the focused pane",
        "← →  h l      fold, or go to the parent or first child",
        "enter         fold, unfold, or load more entities",
        "g G           first row, last row",
        "tab           focus outline, inspector, source in turn",
        "/             search names, text, layers and handles",
        "n N           next, previous match",
        "] [           next, previous layout",
        "x             source as DXF pairs or hex",
        "y             copy text, layer, handle, DXF, hex, a query",
        "alt arrows    resize the panes",
        "q esc         quit",
    ] {
        assert!(contains(&lines, help), "{help}\n{lines:?}");
    }
    press(&mut app, KeyCode::Char('q'));
    assert!(!app.help);
    assert!(!app.should_quit);
    assert!(!contains(&frame(&mut app)?, "╭ Keys "));
    Ok(())
}

#[test]
fn copy_popup_lists_targets() -> TestResult {
    let mut app = demo()?;
    press(&mut app, KeyCode::Char('y'));
    assert!(ui::status_text(&app).starts_with(" copy "));
    let lines = frame(&mut app)?;
    assert!(contains(&lines, "╭ Copy "), "{lines:?}");
    assert!(contains(&lines, "esc closes"), "{lines:?}");
    assert!(contains(&lines, "t  text           demo.dxf"), "{lines:?}");
    assert!(contains(&lines, "l  layer          none here"), "{lines:?}");
    assert!(contains(&lines, "h  handle         none here"), "{lines:?}");
    assert!(
        contains(&lines, "d  DXF            9 $ACADVER"),
        "{lines:?}"
    );
    assert!(contains(&lines, "x  hex            00000"), "{lines:?}");
    assert!(contains(&lines, "q  jq query       none here"), "{lines:?}");
    assert!(
        contains(&lines, "c  shell command  cadboss info assets/demo.dxf"),
        "{lines:?}"
    );
    press(&mut app, KeyCode::Esc);
    assert!(!app.yanking);

    press(&mut app, KeyCode::Char('/'));
    type_text(&mut app, "tag=p-101");
    press(&mut app, KeyCode::Enter);
    press(&mut app, KeyCode::Char('y'));
    let lines = frame(&mut app)?;
    assert!(contains(&lines, "t  text           TAG=P-101"), "{lines:?}");
    assert!(contains(&lines, "l  layer          EQUIPMENT"), "{lines:?}");
    assert!(contains(&lines, "h  handle         136"), "{lines:?}");
    assert!(
        contains(
            &lines,
            r#"q  jq query       .text.attributes[] | select(.text == "TAG=P-101")"#
        ),
        "{lines:?}"
    );
    Ok(())
}

#[test]
fn tab_cycles_focus_and_scrolls_the_source() -> TestResult {
    let mut app = demo()?;
    press(&mut app, KeyCode::Tab);
    assert_eq!(app.focus, Pane::Inspector);
    press(&mut app, KeyCode::Char('j'));
    assert_eq!(app.inspector_scroll, 1);
    press(&mut app, KeyCode::Char('G'));
    assert_eq!(app.inspector_scroll, u16::MAX);
    frame(&mut app)?;
    let last = app.inspector_scroll;
    assert!(last > 1 && last < u16::MAX, "{last}");
    press(&mut app, KeyCode::Char('k'));
    assert_eq!(app.inspector_scroll, last - 1);

    press(&mut app, KeyCode::Tab);
    assert_eq!(app.focus, Pane::Source);
    press(&mut app, KeyCode::PageDown);
    assert!(app.source_scroll > 0);
    let lines = frame(&mut app)?;
    assert_eq!(app.source_scroll, app.source.pairs.len() - 1);
    assert!(contains(&lines, "╭ DXF "), "{lines:?}");
    assert!(!contains(&lines, "   9  $ACADVER"), "{lines:?}");
    press(&mut app, KeyCode::Tab);
    assert_eq!(app.focus, Pane::Tree);
    Ok(())
}

#[test]
fn tiny_terminals_render_without_panicking() -> TestResult {
    let mut app = demo()?;
    press(&mut app, KeyCode::Char('h'));
    render_sized(&mut app, 0, 24)?;
    render_sized(&mut app, 0, 0)?;
    render_sized(&mut app, 1, 1)?;

    let mut app = demo()?;
    type_text(&mut app, "Glljjjll");
    assert_eq!(label(&app), "TAG=P-101");
    render_sized(&mut app, 1, 24)?;
    for popup in [KeyCode::Char('y'), KeyCode::Char('?')] {
        press(&mut app, popup);
        for (width, height) in [(20, 5), (1, 1), (0, 0)] {
            render_sized(&mut app, width, height)?;
        }
        press(&mut app, KeyCode::Esc);
    }
    assert!(!app.yanking);
    assert!(!app.help);
    Ok(())
}

#[test]
fn status_bar_drops_whole_hints_to_make_room() -> TestResult {
    let mut app = demo()?;
    press(&mut app, KeyCode::Char('/'));
    type_text(&mut app, "p-10");
    press(&mut app, KeyCode::Enter);
    let lines = frame(&mut app)?;
    assert_eq!(
        lines.last().map(String::as_str),
        Some(" browse   / search   ] next layout   x hex   y copy     match 1 of 4   n next")
    );

    app.update(Msg::Copied {
        target: YankTarget::Query,
        result: Err("no native clipboard".to_owned()),
    });
    let lines = frame(&mut app)?;
    assert_eq!(
        lines.last().map(String::as_str),
        Some(" browse   / search                 Could not copy jq query: no native clipboard")
    );
    assert!(ui::status_text(&app).contains("? keys   q quit"));
    Ok(())
}

#[test]
fn a_record_past_the_limit_ends_with_the_count_of_bytes_left_out() -> TestResult {
    let long = "A".repeat(70_000);
    let bytes = DxfBuilder::new().mtext(&long, (0.0, 0.0), 2.5).build();
    let mut app = loaded(DxfDocument::load(bytes)?, "long.dxf", "long.dxf");
    type_text(&mut app, "jllllll");
    assert!(label(&app).starts_with("MTEXT  0  AAAA"), "{}", label(&app));
    press(&mut app, KeyCode::Char('x'));
    assert_eq!(app.view, View::Hex);
    let more = app.source.more;
    assert!(more > 0);
    let note = format!("│ {more} more bytes: cadboss obj shows them");
    assert!(!contains(&frame(&mut app)?, &note));

    let count = app.source.hex.len();
    let height = usize::from(
        ui::panes(Rect::new(0, 0, 80, 24), app.splits)
            .source
            .height
            .saturating_sub(2),
    );
    app.source_scroll = count - height;
    let lines = frame(&mut app)?;
    assert!(!contains(&lines, &note), "{lines:?}");
    app.source_scroll = count - height + 1;
    let lines = frame(&mut app)?;
    assert!(contains(&lines, &note), "{lines:?}");

    press(&mut app, KeyCode::Tab);
    press(&mut app, KeyCode::Tab);
    press(&mut app, KeyCode::End);
    assert_eq!(app.source_scroll, count - 1);
    let lines = frame(&mut app)?;
    assert!(contains(&lines, &note), "{lines:?}");
    Ok(())
}
