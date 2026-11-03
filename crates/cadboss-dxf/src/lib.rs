//! DXF reader for cadboss.

#![deny(unsafe_code)]

mod binary;
mod codepage;
mod entity;
mod header;
mod lexer;
mod mtext;
mod record;
mod tables;
pub mod tape;

pub use binary::SENTINEL;
pub use codepage::{decode, decode_unescaped, unescape, Codepage};
pub use entity::EntityDecoder;
pub use header::{read_header, Header};
pub use mtext::strip_mtext;
pub use tables::{read_block_records, read_layers, BlockRecord};
pub use tape::{Cursor, Section, SectionName, Tape, Token};
