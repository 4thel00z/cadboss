//! DXF reader for cadboss.

#![deny(unsafe_code)]

mod binary;
mod lexer;
pub mod tape;

pub use binary::SENTINEL;
pub use tape::{Cursor, Section, SectionName, Tape, Token};
