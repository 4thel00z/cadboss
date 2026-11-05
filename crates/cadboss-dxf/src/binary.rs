use cadboss_core::{Budget, Report, Result, SkipKind, SkipReason};

use crate::lexer::Builder;
use crate::tape::Tape;

pub const SENTINEL: &[u8; 22] = b"AutoCAD Binary DXF\r\n\x1a\0";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ValueWidth {
    Str,
    F64,
    I16,
    I32,
    I64,
    Bool,
    Chunk,
}

impl ValueWidth {
    fn size(self) -> usize {
        match self {
            ValueWidth::F64 | ValueWidth::I64 => 8,
            ValueWidth::I32 => 4,
            ValueWidth::I16 => 2,
            ValueWidth::Bool => 1,
            ValueWidth::Str | ValueWidth::Chunk => 0,
        }
    }
}

fn value_width(code: i16) -> ValueWidth {
    match code {
        10..=59 | 110..=149 | 210..=239 | 460..=469 | 1010..=1059 => ValueWidth::F64,
        60..=79 | 170..=179 | 270..=289 | 370..=389 | 400..=409 | 1060..=1070 => ValueWidth::I16,
        90..=99 | 420..=429 | 440..=459 | 1071 => ValueWidth::I32,
        160..=169 => ValueWidth::I64,
        290..=299 => ValueWidth::Bool,
        310..=319 | 1004 => ValueWidth::Chunk,
        _ => ValueWidth::Str,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ValueKind {
    Float,
    Integer,
    Text,
}

/// How a binary value of this group code is encoded: fixed-width float, fixed-width integer or bytes.
pub fn value_kind(code: i16) -> ValueKind {
    match value_width(code) {
        ValueWidth::F64 => ValueKind::Float,
        ValueWidth::I16 | ValueWidth::I32 | ValueWidth::I64 | ValueWidth::Bool => {
            ValueKind::Integer
        }
        ValueWidth::Str | ValueWidth::Chunk => ValueKind::Text,
    }
}

struct Span {
    offset: usize,
    len: usize,
    next: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CodeWidth {
    One,
    Two,
}

/// Probes the first pair after the sentinel: `00 'S'` selects one-byte codes, anything else
/// (including the usual `00 00 'S'`) selects two-byte codes.
fn detect_code_width(bytes: &[u8]) -> CodeWidth {
    match bytes.get(SENTINEL.len()..SENTINEL.len() + 3) {
        Some([0, b'S', _]) => CodeWidth::One,
        _ => CodeWidth::Two,
    }
}

/// Reads a group code. Two-byte codes are little-endian i16; with one-byte codes a byte of 255
/// escapes to a following two-byte little-endian code.
pub(crate) fn is_chunk(code: i16) -> bool {
    value_width(code) == ValueWidth::Chunk
}

fn read_code(bytes: &[u8], pos: usize, width: CodeWidth) -> Option<(i16, usize)> {
    let first = *bytes.get(pos)?;
    let wide = width == CodeWidth::Two || first == 255;
    if !wide {
        return Some((i16::from(first), pos + 1));
    }
    let start = pos + usize::from(width == CodeWidth::One);
    let low = *bytes.get(start)?;
    let high = *bytes.get(start + 1)?;
    Some((i16::from_le_bytes([low, high]), start + 2))
}

fn read_value(bytes: &[u8], pos: usize, width: ValueWidth) -> Option<Span> {
    let rest = bytes.get(pos..)?;
    match width {
        ValueWidth::Str => {
            let len = memchr::memchr(0, rest)?;
            Some(Span {
                offset: pos,
                len,
                next: pos + len + 1,
            })
        }
        ValueWidth::Chunk => {
            let len = usize::from(*rest.first()?);
            rest.get(1..1 + len)?;
            Some(Span {
                offset: pos + 1,
                len,
                next: pos + 1 + len,
            })
        }
        fixed => {
            let len = fixed.size();
            rest.get(..len)?;
            Some(Span {
                offset: pos,
                len,
                next: pos + len,
            })
        }
    }
}

pub(crate) fn lex_binary(bytes: &[u8], budget: &Budget) -> Result<(Tape, Report)> {
    let reserve =
        usize::try_from(budget.max_tokens).map_or(bytes.len() / 6, |max| max.min(bytes.len() / 6));
    let mut builder = Builder::new(reserve);
    let width = detect_code_width(bytes);
    let mut pos = SENTINEL.len();
    while pos < bytes.len() {
        if builder.at_limit(budget) {
            break;
        }
        let Some((code, value_pos)) = read_code(bytes, pos, width) else {
            builder.report.note(SkipKind::Token, SkipReason::Truncated);
            break;
        };
        let Some(span) = read_value(bytes, value_pos, value_width(code)) else {
            builder.report.note(SkipKind::Token, SkipReason::Truncated);
            break;
        };
        pos = span.next;
        let Some(index) = builder.push(code, span.offset, span.len) else {
            builder
                .report
                .note(SkipKind::Token, SkipReason::Unsupported);
            break;
        };
        let value = bytes
            .get(span.offset..span.offset + span.len)
            .unwrap_or(&[]);
        if builder.observe(index, code, value) {
            break;
        }
    }
    Ok(builder.finish(match width {
        CodeWidth::One => 1,
        CodeWidth::Two => 2,
    }))
}
