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
