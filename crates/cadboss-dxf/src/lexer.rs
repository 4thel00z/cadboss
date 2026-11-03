use cadboss_core::{Budget, Error, Report, Result, SkipKind, SkipReason};

use crate::tape::{Section, SectionName, Tape, Token};

const BOM: &[u8] = b"\xef\xbb\xbf";

struct Line {
    start: usize,
    end: usize,
    next: usize,
}

fn next_line(bytes: &[u8], pos: usize) -> Option<Line> {
    let rest = bytes.get(pos..)?;
    if rest.is_empty() {
        return None;
    }
    let (end, next) = match memchr::memchr(b'\n', rest) {
        Some(at) => (pos + at, pos + at + 1),
        None => (bytes.len(), bytes.len()),
    };
    let end = match end > pos && bytes.get(end - 1) == Some(&b'\r') {
        true => end - 1,
        false => end,
    };
    Some(Line {
        start: pos,
        end,
        next,
    })
}

fn trim_blanks(mut line: &[u8]) -> &[u8] {
    while let [b' ' | b'\t', rest @ ..] = line {
        line = rest;
    }
    while let [rest @ .., b' ' | b'\t'] = line {
        line = rest;
    }
    line
}

fn parse_integer(line: &[u8]) -> Option<i32> {
    let line = trim_blanks(line);
    let (negative, digits) = match line {
        [b'-', rest @ ..] => (true, rest),
        rest => (false, rest),
    };
    if digits.is_empty() {
        return None;
    }
    let mut value: i32 = 0;
    for &byte in digits {
        if !byte.is_ascii_digit() {
            return None;
        }
        value = value.checked_mul(10)?.checked_add(i32::from(byte - b'0'))?;
    }
    Some(if negative { -value } else { value })
}

fn parse_code(line: &[u8]) -> Option<i16> {
    i16::try_from(parse_integer(line)?).ok()
}

fn looks_like_dxf(bytes: &[u8], mut pos: usize) -> bool {
    while let Some(line) = next_line(bytes, pos) {
        pos = line.next;
        let text = bytes.get(line.start..line.end).unwrap_or(&[]);
        if trim_blanks(text).is_empty() {
            continue;
        }
        return parse_integer(text).is_some();
    }
    false
}

struct Builder {
    tokens: Vec<Token>,
    long_values: Vec<(u32, u64, u32)>,
    sections: Vec<Section>,
    open: Option<(SectionName, u32)>,
    awaiting_name: bool,
    report: Report,
}

impl Builder {
    fn push(&mut self, code: i16, offset: usize, len: usize) -> Option<u32> {
        let index = u32::try_from(self.tokens.len()).ok()?;
        let inline = match (u32::try_from(offset), u16::try_from(len)) {
            (Ok(offset), Ok(len)) if len < u16::MAX => Some((offset, len)),
            _ => None,
        };
        let token = match inline {
            Some((offset, len)) => Token { code, len, offset },
            None => {
                self.long_values
                    .push((index, offset as u64, u32::try_from(len).ok()?));
                Token {
                    code,
                    len: u16::MAX,
                    offset: 0,
                }
            }
        };
        self.tokens.push(token);
        Some(index)
    }

    fn close(&mut self, end: u32, reason: Option<SkipReason>) {
        let Some((name, start)) = self.open.take() else {
            return;
        };
        self.sections.push(Section { name, start, end });
        if let Some(reason) = reason {
            self.report.note(SkipKind::Section, reason);
        }
    }

    fn observe(&mut self, index: u32, code: i16, value: &[u8]) -> bool {
        if self.awaiting_name {
            self.awaiting_name = false;
            if code == 2 {
                self.open = Some((SectionName::from_bytes(value), index + 1));
                return false;
            }
            self.report.note(SkipKind::Section, SkipReason::Malformed);
            self.open = Some((SectionName::Other, index));
        }
        if code != 0 {
            return false;
        }
        match value {
            b"SECTION" => {
                self.close(index, Some(SkipReason::Truncated));
                self.awaiting_name = true;
                false
            }
            b"ENDSEC" => {
                if self.open.is_none() {
                    self.report.note(SkipKind::Section, SkipReason::Malformed);
                    return false;
                }
                self.close(index, None);
                false
            }
            b"EOF" => true,
            _ => false,
        }
    }
}

pub(crate) fn lex_ascii(bytes: &[u8], budget: &Budget) -> Result<(Tape, Report)> {
    budget.check_bytes(bytes.len() as u64)?;
    let mut pos = if bytes.starts_with(BOM) { BOM.len() } else { 0 };
    if !looks_like_dxf(bytes, pos) {
        return Err(Error::NotDxf);
    }
    let mut builder = Builder {
        tokens: Vec::with_capacity(bytes.len() / 12),
        long_values: Vec::new(),
        sections: Vec::new(),
        open: None,
        awaiting_name: false,
        report: Report::default(),
    };
    let mut previous_malformed = false;
    while let Some(code_line) = next_line(bytes, pos) {
        let code_text = bytes.get(code_line.start..code_line.end).unwrap_or(&[]);
        let Some(code) = parse_code(code_text) else {
            if !previous_malformed {
                builder.report.note(SkipKind::Token, SkipReason::Malformed);
            }
            previous_malformed = true;
            pos = code_line.next;
            continue;
        };
        previous_malformed = false;
        let Some(value_line) = next_line(bytes, code_line.next) else {
            builder.report.note(SkipKind::Token, SkipReason::Truncated);
            break;
        };
        pos = value_line.next;
        let len = value_line.end - value_line.start;
        let Some(index) = builder.push(code, value_line.start, len) else {
            builder
                .report
                .note(SkipKind::Token, SkipReason::Unsupported);
            break;
        };
        let value = bytes.get(value_line.start..value_line.end).unwrap_or(&[]);
        if builder.observe(index, code, value) {
            break;
        }
    }
    if builder.awaiting_name {
        builder
            .report
            .note(SkipKind::Section, SkipReason::Malformed);
    }
    let end = u32::try_from(builder.tokens.len()).unwrap_or(u32::MAX);
    builder.close(end, Some(SkipReason::Truncated));
    let tape = Tape {
        tokens: builder.tokens,
        long_values: builder.long_values,
        sections: builder.sections,
        binary: false,
    };
    Ok((tape, builder.report))
}
