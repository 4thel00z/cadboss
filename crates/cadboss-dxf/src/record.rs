//! Walking the group-code pairs of one record.

use crate::tape::Cursor;

pub(crate) type Pair<'t> = (i16, &'t [u8]);

/// Consumes pairs up to, but not including, the next pair whose code is `terminator`.
pub(crate) fn pairs_until<'t>(cursor: &mut Cursor<'t>, terminator: i16) -> Vec<Pair<'t>> {
    let mut pairs = Vec::new();
    while cursor.peek_code().is_some_and(|code| code != terminator) {
        pairs.extend(cursor.next());
    }
    pairs
}

pub(crate) fn first<'t>(pairs: &[Pair<'t>], code: i16) -> Option<&'t [u8]> {
    pairs
        .iter()
        .find(|(candidate, _)| *candidate == code)
        .map(|(_, value)| *value)
}

pub(crate) fn last<'t>(pairs: &[Pair<'t>], code: i16) -> Option<&'t [u8]> {
    pairs
        .iter()
        .rev()
        .find(|(candidate, _)| *candidate == code)
        .map(|(_, value)| *value)
}

/// Drops every pair inside a `102 {` ... `102 }` group, the markers included.
pub(crate) fn without_groups(pairs: Vec<Pair<'_>>) -> Vec<Pair<'_>> {
    let mut depth = 0u32;
    pairs
        .into_iter()
        .filter(|(code, value)| {
            if *code != 102 {
                return depth == 0;
            }
            if value.starts_with(b"{") {
                depth += 1;
                return false;
            }
            if value.starts_with(b"}") {
                depth = depth.saturating_sub(1);
                return false;
            }
            depth == 0
        })
        .collect()
}
