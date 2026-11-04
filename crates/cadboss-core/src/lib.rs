//! Shared budgets, errors and reporting for cadboss readers.

pub mod budget;
pub mod error;
pub mod geom;
pub mod report;
pub mod source;

pub use budget::Budget;
pub use error::{Error, Result};
pub use geom::{arc_bbox, bulge_arc, BBox, Point, Transform};
pub use report::{Report, SkipKind, SkipReason, Skipped, MAX_SKIPPED};
pub use source::Source;
