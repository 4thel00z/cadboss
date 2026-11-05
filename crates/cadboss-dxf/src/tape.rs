use std::ops::Range;

use cadboss_core::{Budget, Error, Report, Result};

use crate::binary::{is_chunk, lex_binary, SENTINEL};
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
    pub(crate) code_width: u8,
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
        self.code_width != 0
    }

    /// Group code at `index`, or `i16::MIN` when out of range.
    pub fn code(&self, index: u32) -> i16 {
        self.tokens
            .get(index as usize)
            .map_or(i16::MIN, |token| token.code)
    }

    fn span(&self, index: u32) -> Option<(usize, usize)> {
        let token = self.tokens.get(index as usize)?;
        let (offset, len) = match token.len {
            u16::MAX => {
                let at = self
                    .long_values
                    .binary_search_by_key(&index, |entry| entry.0)
                    .ok()?;
                let &(_, offset, len) = self.long_values.get(at)?;
                (offset, u64::from(len))
            }
            len => (u64::from(token.offset), u64::from(len)),
        };
        let start = usize::try_from(offset).ok()?;
        let end = usize::try_from(offset.saturating_add(len)).ok()?;
        Some((start, end - start))
    }

    /// Value bytes at `index`, or an empty slice when out of range.
    pub fn value<'a>(&'a self, index: u32, bytes: &'a [u8]) -> &'a [u8] {
        let Some((start, len)) = self.span(index) else {
            return &[];
        };
        bytes.get(start..start.saturating_add(len)).unwrap_or(&[])
    }

    /// Byte offset where the record at `index` begins, code bytes included; `bytes.len()` when out of range.
    pub fn token_start(&self, index: u32, bytes: &[u8]) -> usize {
        let (Some(token), Some((offset, _))) = (self.tokens.get(index as usize), self.span(index))
        else {
            return bytes.len();
        };
        if self.is_binary() {
            let code_bytes = match (self.code_width, token.code) {
                (2, _) => 2,
                (_, code) if !(0..=254).contains(&code) => 3,
                _ => 1,
            };
            return offset
                .saturating_sub(code_bytes)
                .saturating_sub(usize::from(is_chunk(token.code)));
        }
        let before = bytes.get(..offset).unwrap_or(bytes);
        let value_line = memchr::memrchr(b'\n', before).map_or(0, |at| at + 1);
        let Some(code_end) = value_line.checked_sub(1) else {
            return 0;
        };
        memchr::memrchr(b'\n', &bytes[..code_end]).map_or(0, |at| at + 1)
    }

    /// Byte range covering the records in `tokens`, from the first code byte to the start of the next record.
    pub fn byte_range(&self, tokens: Range<u32>, bytes: &[u8]) -> Range<usize> {
        let start = self.token_start(tokens.start, bytes);
        if tokens.end <= tokens.start {
            return start..start;
        }
        if tokens.end >= self.len() {
            return start..bytes.len();
        }
        start..self.token_start(tokens.end, bytes)
    }

    pub fn section(&self, name: SectionName) -> Option<&Section> {
        self.sections.iter().find(|section| section.name == name)
    }

    /// Decodes a numeric value: fixed-width little-endian bytes when the tape is binary, text otherwise; infinities and NaN count as missing.
    pub(crate) fn f64(&self, value: &[u8]) -> Option<f64> {
        self.float(value).filter(|number| number.is_finite())
    }

    fn float(&self, value: &[u8]) -> Option<f64> {
        if self.is_binary() {
            return Some(f64::from_le_bytes(value.try_into().ok()?));
        }
        std::str::from_utf8(value).ok()?.trim().parse().ok()
    }

    /// Decodes an integer value by width (1, 2, 4 or 8 bytes) when the tape is binary, text otherwise.
    pub(crate) fn i64(&self, value: &[u8]) -> Option<i64> {
        if self.is_binary() {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn byte_range_spans_the_code_lines_of_an_ascii_record() {
        let bytes =
            b"  0\nSECTION\n  2\nENTITIES\n  0\nLINE\n  8\n0\n 10\n1.5\n  0\nENDSEC\n  0\nEOF\n";
        let (tape, _) = Tape::lex(bytes, &Budget::default()).unwrap();
        assert_eq!(tape.byte_range(2..5, bytes), 25..48);
        assert_eq!(
            &bytes[tape.byte_range(2..5, bytes)],
            b"  0\nLINE\n  8\n0\n 10\n1.5\n"
        );
        assert_eq!(tape.byte_range(5..tape.len(), bytes).end, bytes.len());
        assert_eq!(tape.token_start(0, bytes), 0);
    }

    #[test]
    fn byte_range_keeps_carriage_returns() {
        let bytes = b"  0\r\nSECTION\r\n  2\r\nENTITIES\r\n  0\r\nLINE\r\n  8\r\n0\r\n  0\r\nENDSEC\r\n  0\r\nEOF\r\n";
        let (tape, _) = Tape::lex(bytes, &Budget::default()).unwrap();
        assert_eq!(
            &bytes[tape.byte_range(2..4, bytes)],
            b"  0\r\nLINE\r\n  8\r\n0\r\n"
        );
    }

    #[test]
    fn byte_range_starts_at_the_code_bytes_of_a_binary_record() {
        let bytes = cadboss_testkit::dxf::DxfBuilder::new()
            .line((0.0, 0.0), (1.0, 1.0))
            .binary()
            .build();
        let (tape, _) = Tape::lex(&bytes, &Budget::default()).unwrap();
        assert!(tape.is_binary());
        let start = (0..tape.len())
            .find(|index| tape.code(*index) == 0 && tape.value(*index, &bytes) == b"LINE")
            .unwrap();
        let end = (start + 1..tape.len())
            .find(|index| tape.code(*index) == 0)
            .unwrap();
        let slice = &bytes[tape.byte_range(start..end, &bytes)];
        assert!(matches!(
            slice.windows(5).position(|w| w == b"LINE\0"),
            Some(1 | 2)
        ));
        assert_eq!(
            tape.byte_range(start..end, &bytes).end,
            tape.token_start(end, &bytes)
        );
    }
}
