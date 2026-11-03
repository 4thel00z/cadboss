//! Legacy codepage decoding and DXF text escape expansion.

use cadboss_core::{Report, SkipKind, SkipReason};
use cadboss_model::Text;

const FIRST_UTF8_VERSION: &[u8] = b"AC1021";

const WINDOWS_1252_NAMES: [&str; 3] = ["ANSI_1252", "1252", "CP1252"];

const ISO_8859_1_NAMES: [&str; 6] = [
    "ISO8859-1",
    "ISO-8859-1",
    "8859_1",
    "ISO8859_1",
    "LATIN1",
    "ISO_8859-1",
];

const WINDOWS_1252_HIGH: [char; 32] = [
    '\u{20ac}', '\u{fffd}', '\u{201a}', '\u{192}', '\u{201e}', '\u{2026}', '\u{2020}', '\u{2021}',
    '\u{2c6}', '\u{2030}', '\u{160}', '\u{2039}', '\u{152}', '\u{fffd}', '\u{17d}', '\u{fffd}',
    '\u{fffd}', '\u{2018}', '\u{2019}', '\u{201c}', '\u{201d}', '\u{2022}', '\u{2013}', '\u{2014}',
    '\u{2dc}', '\u{2122}', '\u{161}', '\u{203a}', '\u{153}', '\u{fffd}', '\u{17e}', '\u{178}',
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Codepage {
    Utf8,
    Windows1252,
    Iso8859_1,
    Unsupported,
}

impl Codepage {
    pub fn from_header(version: &[u8], codepage: Option<&[u8]>) -> Codepage {
        if version >= FIRST_UTF8_VERSION {
            return Codepage::Utf8;
        }
        let Some(name) = codepage else {
            return Codepage::Windows1252;
        };
        let name = name.trim_ascii();
        let matches = |names: &[&str]| {
            names
                .iter()
                .any(|known| name.eq_ignore_ascii_case(known.as_bytes()))
        };
        if matches(&WINDOWS_1252_NAMES) {
            return Codepage::Windows1252;
        }
        if matches(&ISO_8859_1_NAMES) {
            return Codepage::Iso8859_1;
        }
        Codepage::Unsupported
    }
}

pub fn decode(cp: Codepage, bytes: &[u8], report: &mut Report) -> Text {
    if bytes.is_ascii() {
        return Text::from_utf8_lossy_or_bytes(bytes);
    }
    match cp {
        Codepage::Utf8 => Text::from_utf8_lossy_or_bytes(bytes),
        Codepage::Windows1252 => Text::Str(bytes.iter().map(|byte| windows_1252(*byte)).collect()),
        Codepage::Iso8859_1 => Text::Str(bytes.iter().map(|byte| char::from(*byte)).collect()),
        Codepage::Unsupported => {
            report.note(SkipKind::Text, SkipReason::UnsupportedCodepage);
            Text::Bytes(bytes.to_vec())
        }
    }
}

pub fn decode_unescaped(cp: Codepage, bytes: &[u8], report: &mut Report) -> Text {
    match decode(cp, bytes, report) {
        Text::Str(text) => Text::Str(unescape(&text)),
        other => other,
    }
}

fn windows_1252(byte: u8) -> char {
    let Some(offset) = byte.checked_sub(0x80).filter(|offset| *offset < 0x20) else {
        return char::from(byte);
    };
    WINDOWS_1252_HIGH
        .get(usize::from(offset))
        .copied()
        .unwrap_or('\u{fffd}')
}

pub fn unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(next) = rest.chars().next() {
        if let Some((replacement, used)) = escape(rest) {
            out.extend(replacement);
            rest = rest.get(used..).unwrap_or("");
            continue;
        }
        out.push(next);
        rest = rest.get(next.len_utf8()..).unwrap_or("");
    }
    out
}

fn escape(rest: &str) -> Option<(Option<char>, usize)> {
    let bytes = rest.as_bytes();
    match *bytes {
        [b'\\', marker, b'+', ..] if marker.eq_ignore_ascii_case(&b'U') => {
            let code = hex4(rest.get(3..7))?;
            Some((Some(char::from_u32(code).unwrap_or('\u{fffd}')), 7))
        }
        [b'\\', marker, b'+', digit, ..]
            if marker.eq_ignore_ascii_case(&b'M') && digit.is_ascii_digit() =>
        {
            hex4(rest.get(4..8))?;
            Some((Some('\u{fffd}'), 8))
        }
        [b'%', b'%', code, ..] => percent(rest, code),
        _ => None,
    }
}

fn percent(rest: &str, code: u8) -> Option<(Option<char>, usize)> {
    match code.to_ascii_lowercase() {
        b'd' => Some((Some('\u{b0}'), 3)),
        b'c' => Some((Some('\u{d8}'), 3)),
        b'p' => Some((Some('\u{b1}'), 3)),
        b'%' => Some((Some('%'), 3)),
        b'u' | b'o' => Some((None, 3)),
        digit if digit.is_ascii_digit() => {
            let digits = rest
                .get(2..5)
                .filter(|digits| digits.bytes().all(|b| b.is_ascii_digit()))?;
            let value = u8::try_from(digits.parse::<u32>().ok()?).ok()?;
            Some((Some(char::from(value)), 5))
        }
        _ => None,
    }
}

fn hex4(digits: Option<&str>) -> Option<u32> {
    let digits = digits.filter(|digits| digits.bytes().all(|byte| byte.is_ascii_hexdigit()))?;
    u32::from_str_radix(digits, 16).ok()
}
