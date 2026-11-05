//! Rendering: the pane split percentages the resize keys step through.

use std::ops::RangeInclusive;

/// The two pane dividers as percentages: the tree column's width and the inspector's share of the right column.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Splits {
    pub tree: u16,
    pub right_top: u16,
}

impl Default for Splits {
    fn default() -> Splits {
        Splits {
            tree: 42,
            right_top: 62,
        }
    }
}

pub const SPLIT_STEP: u16 = 5;
pub const TREE_SPLIT: RangeInclusive<u16> = 15..=70;
pub const RIGHT_TOP_SPLIT: RangeInclusive<u16> = 20..=85;
