//! Text that is either valid UTF-8 or raw undecodable bytes.

use std::fmt;

use serde::ser::SerializeMap;
use serde::{Serialize, Serializer};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Text {
    Str(String),
    Bytes(Vec<u8>),
}

impl Text {
    /// `Str` when `bytes` is valid UTF-8, otherwise `Bytes` holding a copy.
    pub fn from_utf8_lossy_or_bytes(bytes: &[u8]) -> Text {
        match std::str::from_utf8(bytes) {
            Ok(text) => Text::Str(text.to_owned()),
            Err(_) => Text::Bytes(bytes.to_vec()),
        }
    }

    /// The UTF-8 bytes of a string or the raw bytes.
    pub fn as_bytes(&self) -> &[u8] {
        match self {
            Text::Str(text) => text.as_bytes(),
            Text::Bytes(bytes) => bytes,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Text::Str(text) => Some(text),
            Text::Bytes(_) => None,
        }
    }

    pub fn is_empty(&self) -> bool {
        match self {
            Text::Str(text) => text.is_empty(),
            Text::Bytes(bytes) => bytes.is_empty(),
        }
    }
}

impl Default for Text {
    fn default() -> Text {
        Text::Str(String::new())
    }
}

impl From<&str> for Text {
    fn from(text: &str) -> Text {
        Text::Str(text.to_owned())
    }
}

impl From<String> for Text {
    fn from(text: String) -> Text {
        Text::Str(text)
    }
}

impl fmt::Display for Text {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Text::Str(text) => f.write_str(text),
            Text::Bytes(bytes) => write!(f, "[non-decodable text, {} bytes]", bytes.len()),
        }
    }
}

impl Serialize for Text {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Text::Str(text) => serializer.serialize_str(text),
            Text::Bytes(bytes) => {
                let mut map = serializer.serialize_map(Some(1))?;
                map.serialize_entry("_bytes", &base64(bytes))?;
                map.end()
            }
        }
    }
}

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn base64(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b0 = u32::from(chunk[0]);
        let b1 = u32::from(chunk.get(1).copied().unwrap_or(0));
        let b2 = u32::from(chunk.get(2).copied().unwrap_or(0));
        let word = (b0 << 16) | (b1 << 8) | b2;
        let sextet = |shift: u32| char::from(ALPHABET[((word >> shift) & 63) as usize]);
        out.push(sextet(18));
        out.push(sextet(12));
        out.push(if chunk.len() > 1 { sextet(6) } else { '=' });
        out.push(if chunk.len() > 2 { sextet(0) } else { '=' });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_with_valid_utf8_serializes_as_a_string() {
        let json = serde_json::to_string(&Text::from_utf8_lossy_or_bytes(b"Plan")).unwrap();
        assert_eq!(json, "\"Plan\"");
    }

    #[test]
    fn text_with_invalid_utf8_serializes_as_bytes_object() {
        let text = Text::from_utf8_lossy_or_bytes(&[0xff, 0x41]);
        assert_eq!(text.as_str(), None);
        assert_eq!(
            serde_json::to_string(&text).unwrap(),
            "{\"_bytes\":\"/0E=\"}"
        );
    }

    #[test]
    fn text_displays_strings_and_byte_counts() {
        assert_eq!(Text::from("a").to_string(), "a");
        assert_eq!(
            Text::Bytes(vec![1, 2, 3]).to_string(),
            "[non-decodable text, 3 bytes]"
        );
        assert!(Text::default().is_empty());
    }

    #[test]
    fn base64_pads_every_remainder() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
    }
}
