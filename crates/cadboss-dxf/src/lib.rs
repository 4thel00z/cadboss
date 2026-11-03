//! DXF reader for cadboss.

#![deny(unsafe_code)]

mod lexer;
pub mod tape;

pub use tape::{Cursor, Section, SectionName, Tape, Token};
