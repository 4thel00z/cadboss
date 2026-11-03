//! Text, JSON and rendering output for cadboss models.

#![deny(unsafe_code)]

mod json;
mod markdown;

pub use json::{to_json, to_json_compact};
pub use markdown::{format_number, to_markdown};
