//! Shared budgets, errors and reporting for cadboss readers.

pub mod budget;
pub mod error;
pub mod report;
pub mod source;

pub use budget::Budget;
pub use error::{Error, Result};
pub use report::{Report, SkipKind, SkipReason, Skipped, MAX_SKIPPED};
pub use source::Source;
