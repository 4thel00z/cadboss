//! Walking the group-code pairs of one record in place on the tape.

use std::ops::Range;

use crate::tape::{Cursor, Tape};

/// The pairs of one record as a token range of the tape; every query scans the tape, nothing is collected.
/// Pairs inside a `102 {` ... `102 }` group are hidden, the markers included.
#[derive(Clone, Copy)]
pub(crate) struct Record<'t> {
    tape: &'t Tape,
    bytes: &'t [u8],
    start: u32,
    end: u32,
}

impl<'t> Record<'t> {
    pub(crate) fn new(tape: &'t Tape, bytes: &'t [u8], range: Range<u32>) -> Record<'t> {
        Record {
            tape,
            bytes,
            start: range.start,
            end: range.end.min(tape.len()),
        }
    }

    /// Consumes pairs up to, but not including, the next pair whose code is `terminator`.
    pub(crate) fn until(cursor: &mut Cursor<'t>, terminator: i16) -> Record<'t> {
        let start = cursor.index();
        cursor.skip_to_code(terminator);
        Record::new(cursor.tape(), cursor.bytes(), start..cursor.index())
    }

    /// Number of tokens in the record, hidden groups included.
    pub(crate) fn len(&self) -> u32 {
        self.end.saturating_sub(self.start)
    }

    pub(crate) fn pairs(&self) -> impl Iterator<Item = (i16, &'t [u8])> + 't {
        let (tape, bytes) = (self.tape, self.bytes);
        self.codes()
            .map(move |(code, index)| (code, tape.value(index, bytes)))
    }

    pub(crate) fn first(&self, code: i16) -> Option<&'t [u8]> {
        let (_, index) = self.codes().find(|(candidate, _)| *candidate == code)?;
        Some(self.tape.value(index, self.bytes))
    }

    pub(crate) fn last(&self, code: i16) -> Option<&'t [u8]> {
        let (_, index) = self
            .codes()
            .filter(|(candidate, _)| *candidate == code)
            .last()?;
        Some(self.tape.value(index, self.bytes))
    }

    pub(crate) fn f64(&self, value: &[u8]) -> Option<f64> {
        self.tape.f64(value)
    }

    pub(crate) fn i64(&self, value: &[u8]) -> Option<i64> {
        self.tape.i64(value)
    }

    pub(crate) fn handle(&self, value: &[u8]) -> Option<u64> {
        crate::tape::handle(value)
    }

    fn codes(&self) -> Codes<'t> {
        Codes {
            tape: self.tape,
            bytes: self.bytes,
            index: self.start,
            end: self.end,
            depth: 0,
        }
    }
}

struct Codes<'t> {
    tape: &'t Tape,
    bytes: &'t [u8],
    index: u32,
    end: u32,
    depth: u32,
}

impl Iterator for Codes<'_> {
    type Item = (i16, u32);

    fn next(&mut self) -> Option<(i16, u32)> {
        while self.index < self.end {
            let index = self.index;
            self.index += 1;
            let code = self.tape.code(index);
            if code == 102 {
                let marker = self.tape.value(index, self.bytes);
                if marker.starts_with(b"{") {
                    self.depth += 1;
                    continue;
                }
                if marker.starts_with(b"}") {
                    self.depth = self.depth.saturating_sub(1);
                    continue;
                }
            }
            if self.depth == 0 {
                return Some((code, index));
            }
        }
        None
    }
}

/// The code-0 records of a token range in order, each as its name and its pairs.
pub(crate) struct Records<'t> {
    tape: &'t Tape,
    bytes: &'t [u8],
    index: u32,
    end: u32,
}

impl<'t> Records<'t> {
    pub(crate) fn new(tape: &'t Tape, bytes: &'t [u8], range: Range<u32>) -> Records<'t> {
        Records {
            tape,
            bytes,
            index: range.start,
            end: range.end.min(tape.len()),
        }
    }
}

impl<'t> Iterator for Records<'t> {
    type Item = (&'t [u8], Record<'t>);

    fn next(&mut self) -> Option<Self::Item> {
        while self.index < self.end && self.tape.code(self.index) != 0 {
            self.index += 1;
        }
        if self.index >= self.end {
            return None;
        }
        let name = self.tape.value(self.index, self.bytes);
        let start = self.index + 1;
        self.index = start;
        while self.index < self.end && self.tape.code(self.index) != 0 {
            self.index += 1;
        }
        Some((name, Record::new(self.tape, self.bytes, start..self.index)))
    }
}
