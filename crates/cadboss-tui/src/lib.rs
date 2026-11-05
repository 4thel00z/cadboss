//! Interactive terminal explorer for CAD drawings: an outline of layouts, blocks, layers and text, an inspector, and the selected record as DXF pairs or hex.
//! Only [`run`] touches the terminal; it waits on key events, background results and a 100 ms tick.

pub mod app;
pub mod clipboard;
pub mod input;
mod inspector;
pub mod outline;
pub mod source;
pub mod theme;
pub mod ui;
pub mod yank;

use std::io::{self, IsTerminal};
use std::sync::Arc;
use std::time::Duration;

use cadboss_dxf::DxfDocument;
use crossterm::event::{Event, EventStream};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use futures_util::StreamExt;
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use tokio::sync::mpsc::{self, UnboundedSender};

use crate::app::{App, Cmd, Msg};
use crate::clipboard::Transport;
use crate::outline::Outline;
use crate::theme::Theme;

const TICK: Duration = Duration::from_millis(100);
/// The error [`run`] gives when stdin or stdout is not a terminal.
pub const NO_TTY: &str = "cadboss tui requires an interactive terminal";

/// [`NO_TTY`] for the errors raw mode gives without a terminal: `ENXIO` (6) with no controlling terminal, `ENOTTY` (25) on another device.
fn terminal_error(error: io::Error) -> io::Error {
    if !matches!(error.raw_os_error(), Some(6 | 25)) {
        return error;
    }
    io::Error::new(error.kind(), NO_TTY)
}

/// Leaves raw mode and the alternate screen when dropped, so no early return leaves the shell in raw mode.
struct TerminalGuard;

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        disable_raw_mode().ok();
        execute!(io::stdout(), LeaveAlternateScreen).ok();
    }
}

/// Runs the explorer until the user quits. `title` names the drawing in the header; `target` is the path as given, used in copied shell commands.
/// Needs a tokio runtime with time enabled; a current-thread one will do.
pub async fn run(doc: DxfDocument, title: String, target: String) -> io::Result<()> {
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        return Err(io::Error::other(NO_TTY));
    }
    enable_raw_mode().map_err(terminal_error)?;
    let guard = TerminalGuard;
    execute!(io::stdout(), EnterAlternateScreen)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(io::stdout()))?;
    let size = terminal.size()?;
    let doc = Arc::new(doc);
    let mut app = App::new(
        Arc::clone(&doc),
        title.clone(),
        target,
        (size.width, size.height),
    );
    app.theme = Theme::detect();
    let (tx, mut rx) = mpsc::unbounded_channel::<Msg>();
    spawn_outline(doc, title, tx.clone());
    let mut events = EventStream::new();
    let mut tick = tokio::time::interval(TICK);
    while !app.should_quit {
        terminal.draw(|frame| ui::draw(&mut app, frame))?;
        let msg = tokio::select! {
            event = events.next() => match event {
                Some(Ok(Event::Key(key))) => Msg::Key(key),
                Some(Ok(Event::Resize(width, height))) => Msg::Resize(width, height),
                Some(Ok(_)) => continue,
                Some(Err(error)) => return Err(error),
                None => break,
            },
            Some(msg) = rx.recv() => msg,
            _ = tick.tick() => Msg::Tick,
        };
        let mut pending = app.update(msg);
        while let Some(cmd) = pending.pop() {
            pending.extend(execute_cmd(&mut app, &tx, cmd));
        }
    }
    drop(guard);
    Ok(())
}

/// Builds the outline on the blocking pool and sends it back.
fn spawn_outline(doc: Arc<DxfDocument>, title: String, tx: UnboundedSender<Msg>) {
    tokio::task::spawn_blocking(move || {
        let outline = Outline::build(&doc, &title);
        tx.send(Msg::Loaded(Box::new(outline))).ok();
    });
}

/// Starts what a command asks for; work done in place returns the commands its result leads to.
fn execute_cmd(app: &mut App, tx: &UnboundedSender<Msg>, cmd: Cmd) -> Vec<Cmd> {
    match cmd {
        Cmd::Copy {
            target,
            partial,
            text,
        } => {
            let tx = tx.clone();
            tokio::task::spawn_blocking(move || {
                let msg = match clipboard::copy_native(&text) {
                    Ok(()) => Msg::Copied {
                        target,
                        partial,
                        result: Ok(Transport::Native),
                    },
                    Err(_) => Msg::NoNativeClipboard {
                        target,
                        partial,
                        text,
                    },
                };
                tx.send(msg).ok();
            });
            Vec::new()
        }
        Cmd::Osc52 {
            target,
            partial,
            text,
        } => {
            let result = clipboard::write_osc52(&text)
                .map(|()| Transport::Osc52)
                .map_err(|error| error.to_string());
            app.update(Msg::Copied {
                target,
                partial,
                result,
            })
        }
    }
}
