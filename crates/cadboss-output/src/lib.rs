//! JSON and Markdown output for cadboss models.

#![deny(unsafe_code)]

mod json;
mod markdown;

pub use cadboss_model::format_number;
pub use json::{to_json, to_json_compact};
pub use markdown::{one_line, to_markdown};
