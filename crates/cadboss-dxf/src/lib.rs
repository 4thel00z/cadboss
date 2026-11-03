//! DXF reader for cadboss.

#![deny(unsafe_code)]

mod binary;
pub mod blocks;
mod codepage;
pub mod document;
mod entity;
mod header;
pub mod layouts;
mod lexer;
mod mtext;
mod reader;
mod record;
mod tables;
pub mod tape;
mod textgroups;

pub use binary::SENTINEL;
pub use blocks::{read_blocks, BlockDef};
pub use codepage::{decode, decode_unescaped, unescape, Codepage};
pub use document::{DxfDocument, Options};
pub use entity::EntityDecoder;
pub use header::{read_header, Header};
pub use layouts::read_layouts;
pub use mtext::strip_mtext;
pub use reader::{
    load, load_reporting, load_with_options, load_with_options_reporting, open, open_reporting,
    open_with_options, open_with_options_reporting, read, read_reporting, read_with_options,
    read_with_options_reporting,
};
pub use tables::{read_block_records, read_layers, BlockRecord};
pub use tape::{Cursor, Section, SectionName, Tape, Token};
