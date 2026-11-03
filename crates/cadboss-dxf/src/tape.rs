use std::ops::Range;

use cadboss_core::{Budget, Error, Report, Result};

use crate::binary::{lex_binary, SENTINEL};
use crate::lexer::lex_ascii;

/// One group-code/value pair: the value bytes live in the input at `offset..offset + len`.
/// A `len` of `u16::MAX` marks a value stored in `Tape::long_values`.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct Token {
    pub code: i16,
    pub len: u16,
    pub offset: u32,
}

const _: () = assert!(std::mem::size_of::<Token>() == 8);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SectionName {
    Header,
    Classes,
    Tables,
    Blocks,
    Entities,
    Objects,
    Thumbnail,
    Other,
}

impl SectionName {
    pub(crate) fn from_bytes(name: &[u8]) -> SectionName {
        match name {
            b"HEADER" => SectionName::Header,
            b"CLASSES" => SectionName::Classes,
            b"TABLES" => SectionName::Tables,
            b"BLOCKS" => SectionName::Blocks,
            b"ENTITIES" => SectionName::Entities,
            b"OBJECTS" => SectionName::Objects,
            b"THUMBNAILIMAGE" => SectionName::Thumbnail,
            _ => SectionName::Other,
        }
    }
}

/// Token range of a section body: `start` follows the name token, `end` is the `ENDSEC` token.
#[derive(Clone, Copy, Debug)]
pub struct Section {
    pub name: SectionName,
    pub start: u32,
    pub end: u32,
}

#[derive(Debug)]
pub struct Tape {
    pub(crate) tokens: Vec<Token>,
    pub(crate) long_values: Vec<(u32, u64, u32)>,
    pub(crate) sections: Vec<Section>,
    pub(crate) binary: bool,
}

impl Tape {
    pub fn lex(bytes: &[u8], budget: &Budget) -> Result<(Tape, Report)> {
        budget.check_bytes(bytes.len() as u64)?;
        if bytes.is_empty() {
            return Err(Error::Empty);
        }
        if bytes.starts_with(SENTINEL) {
            return lex_binary(bytes, budget);
        }
        lex_ascii(bytes, budget)
    }

    pub fn len(&self) -> u32 {
        u32::try_from(self.tokens.len()).unwrap_or(u32::MAX)
    }

    pub fn is_empty(&self) -> bool {
        self.tokens.is_empty()
    }

    pub fn is_binary(&self) -> bool {
        self.binary
    }

    /// Group code at `index`, or `i16::MIN` when out of range.
    pub fn code(&self, index: u32) -> i16 {
        self.tokens
            .get(index as usize)
            .map_or(i16::MIN, |token| token.code)
    }

    /// Value bytes at `index`, or an empty slice when out of range.
    pub fn value<'a>(&'a self, index: u32, bytes: &'a [u8]) -> &'a [u8] {
        let Some(token) = self.tokens.get(index as usize) else {
            return &[];
        };
        let (offset, len) = match token.len {
            u16::MAX => match self
                .long_values
                .binary_search_by_key(&index, |entry| entry.0)
            {
                Ok(at) => match self.long_values.get(at) {
                    Some(&(_, offset, len)) => (offset, u64::from(len)),
                    None => return &[],
                },
                Err(_) => return &[],
            },
            len => (u64::from(token.offset), u64::from(len)),
        };
        let (Ok(start), Ok(end)) = (
            usize::try_from(offset),
            usize::try_from(offset.saturating_add(len)),
        ) else {
            return &[];
        };
        bytes.get(start..end).unwrap_or(&[])
    }

    pub fn section(&self, name: SectionName) -> Option<&Section> {
        self.sections.iter().find(|section| section.name == name)
    }

    /// Decodes a numeric value: fixed-width little-endian bytes when the tape is binary, text otherwise; infinities and NaN count as missing.
    pub(crate) fn f64(&self, value: &[u8]) -> Option<f64> {
        self.float(value).filter(|number| number.is_finite())
    }

    fn float(&self, value: &[u8]) -> Option<f64> {
        if self.binary {
            return Some(f64::from_le_bytes(value.try_into().ok()?));
        }
        std::str::from_utf8(value).ok()?.trim().parse().ok()
    }

    /// Decodes an integer value by width (1, 2, 4 or 8 bytes) when the tape is binary, text otherwise.
    pub(crate) fn i64(&self, value: &[u8]) -> Option<i64> {
        if self.binary {
            return match *value {
                [byte] => Some(i64::from(byte)),
                [a, b] => Some(i64::from(i16::from_le_bytes([a, b]))),
                [a, b, c, d] => Some(i64::from(i32::from_le_bytes([a, b, c, d]))),
                [a, b, c, d, e, f, g, h] => Some(i64::from_le_bytes([a, b, c, d, e, f, g, h])),
                _ => None,
            };
        }
        std::str::from_utf8(value).ok()?.trim().parse().ok()
    }

    pub fn pairs(&self, range: Range<u32>) -> impl Iterator<Item = (u32, i16)> + '_ {
        range.filter_map(|index| {
            self.tokens
                .get(index as usize)
                .map(|token| (index, token.code))
        })
    }
}

pub struct Cursor<'t> {
    tape: &'t Tape,
    bytes: &'t [u8],
    index: u32,
    end: u32,
}

impl<'t> Cursor<'t> {
    pub fn new(tape: &'t Tape, bytes: &'t [u8], range: Range<u32>) -> Cursor<'t> {
        Cursor {
            tape,
            bytes,
            index: range.start,
            end: range.end.min(tape.len()),
        }
    }

    pub fn index(&self) -> u32 {
        self.index
    }

    pub(crate) fn seek(&mut self, index: u32) {
        self.index = index.min(self.end);
    }

    pub(crate) fn tape(&self) -> &'t Tape {
        self.tape
    }

    pub(crate) fn bytes(&self) -> &'t [u8] {
        self.bytes
    }

    pub fn peek_code(&self) -> Option<i16> {
        if self.index >= self.end {
            return None;
        }
        Some(self.tape.code(self.index))
    }

    pub fn peek(&self) -> Option<(i16, &'t [u8])> {
        if self.index >= self.end {
            return None;
        }
        Some((
            self.tape.code(self.index),
            self.tape.value(self.index, self.bytes),
        ))
    }

    fn advance(&mut self) -> Option<(i16, &'t [u8])> {
        let item = self.peek()?;
        self.index += 1;
        Some(item)
    }

    pub fn skip_to_code_zero(&mut self) {
        self.skip_to_code(0);
    }

    pub(crate) fn skip_to_code(&mut self, code: i16) {
        while self.peek_code().is_some_and(|candidate| candidate != code) {
            self.index += 1;
        }
    }

    /// Decodes a numeric value: fixed-width little-endian bytes when the tape is binary, text otherwise; infinities and NaN count as missing.
    pub fn f64(&self, value: &[u8]) -> Option<f64> {
        self.tape.f64(value)
    }

    /// Decodes an integer value by width (1, 2, 4 or 8 bytes) when the tape is binary, text otherwise.
    pub fn i64(&self, value: &[u8]) -> Option<i64> {
        self.tape.i64(value)
    }

    pub fn handle(&self, value: &[u8]) -> Option<u64> {
        handle(value)
    }
}

pub(crate) fn handle(value: &[u8]) -> Option<u64> {
    u64::from_str_radix(std::str::from_utf8(value).ok()?.trim(), 16).ok()
}

impl<'t> Iterator for Cursor<'t> {
    type Item = (i16, &'t [u8]);

    fn next(&mut self) -> Option<Self::Item> {
        self.advance()
    }
}
