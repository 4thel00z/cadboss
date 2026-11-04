//! Format-independent drawing model for cadboss.

#![deny(unsafe_code)]

pub mod document;
pub mod entity;
pub mod metadata;
pub mod number;
pub mod text;

pub use document::{Document, TextGroups, TextItem, TextSource};
pub use entity::{Attribute, Entity, EntityCommon, EntityKind};
pub use metadata::{Block, Format, Layer, Layout, LayoutKind, Metadata, Units};
pub use number::format_number;
pub use text::Text;
