//! Clipboard transport for the copy popup: the native clipboard via arboard,
//! else the OSC 52 escape sequence, which reaches the local clipboard over
//! SSH but cannot confirm delivery.

use std::io::{self, Write};
use std::sync::{Mutex, OnceLock, PoisonError};

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Transport {
    Native,
    Osc52,
}

/// The OSC 52 sequence putting `text` on the clipboard of the terminal that
/// renders it.
pub fn osc52(text: &str) -> String {
    format!("\x1b]52;c;{}\x07", BASE64.encode(text))
}

/// One clipboard handle for the session: on X11 the selection belongs to the
/// connection that set it, so a dropped handle would clear the copy.
fn native() -> &'static Mutex<Option<arboard::Clipboard>> {
    static NATIVE: OnceLock<Mutex<Option<arboard::Clipboard>>> = OnceLock::new();
    NATIVE.get_or_init(|| Mutex::new(arboard::Clipboard::new().ok()))
}

/// Puts `text` on the native clipboard, failing when there is none to reach.
/// Can block on X11 and Wayland, so the loop runs it on the blocking pool.
pub fn copy_native(text: &str) -> Result<(), String> {
    let mut guard = native().lock().unwrap_or_else(PoisonError::into_inner);
    let Some(clip) = guard.as_mut() else {
        return Err("no native clipboard".to_string());
    };
    clip.set_text(text.to_string())
        .map_err(|error| error.to_string())
}

/// Writes the OSC 52 sequence to stdout, between frames.
pub fn write_osc52(text: &str) -> io::Result<()> {
    let mut stdout = io::stdout();
    stdout.write_all(osc52(text).as_bytes())?;
    stdout.flush()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn osc52_wraps_base64() {
        assert_eq!(osc52("hi"), "\x1b]52;c;aGk=\x07");
        assert_eq!(osc52(""), "\x1b]52;c;\x07");
    }
}
