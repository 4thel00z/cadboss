//! The source pane's two views of a record: its group-code pairs and a hex dump of its bytes.

use std::ops::Range;

use cadboss_dxf::DxfDocument;

/// Bytes of a record the source pane lays out; the rest is only counted.
pub const SOURCE_LIMIT: usize = 64 * 1024;
/// Bytes a hex line holds in a pane wide enough for it, and in every copied dump.
pub const WIDE_HEX: usize = 16;
/// Bytes per hex line and the columns such a line takes, widest first; a pane takes the first that fits, else the last.
const HEX_LAYOUTS: [(usize, usize); 3] = [(WIDE_HEX, 78), (8, 45), (4, 29)];
/// Most bytes in a group of hex digits; groups are two spaces apart.
const GROUP: usize = 8;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum View {
    Pairs,
    Hex,
}

/// A record's first `SOURCE_LIMIT` bytes as decoded pairs and as hex lines of `bytes_per_line` bytes; `more` counts the bytes past the limit.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Source {
    pub tokens: Option<Range<u32>>,
    pub bytes: Range<usize>,
    pub pairs: Vec<(i16, String)>,
    pub hex: Vec<String>,
    pub bytes_per_line: usize,
    pub more: usize,
}

impl Source {
    /// The shown bytes as hex lines of `bytes_per_line` bytes.
    pub fn hex_dump(&self, doc: &DxfDocument, bytes_per_line: usize) -> Vec<String> {
        let shown = self.bytes.start..self.bytes.end.saturating_sub(self.more);
        hex_lines(
            doc.bytes().get(shown).unwrap_or(&[]),
            self.bytes.start,
            bytes_per_line,
        )
    }

    /// Lays the hex lines out again with `bytes_per_line` bytes each.
    pub fn refit(&mut self, doc: &DxfDocument, bytes_per_line: usize) {
        self.hex = self.hex_dump(doc, bytes_per_line);
        self.bytes_per_line = bytes_per_line;
    }

    /// Lines the view lays out, the line counting the bytes past the limit excluded.
    pub fn line_count(&self, view: View) -> usize {
        match view {
            View::Pairs => self.pairs.len(),
            View::Hex => self.hex.len(),
        }
    }

    /// File offset where each shown pair begins, code bytes included.
    pub fn offsets(&self, doc: &DxfDocument) -> Vec<usize> {
        let Some(tokens) = self.tokens.clone() else {
            return Vec::new();
        };
        tokens
            .take(self.pairs.len())
            .map(|index| doc.tape().token_start(index, doc.bytes()))
            .collect()
    }
}

/// The record of `tokens`, its pairs printed as `cadboss obj` prints them and its hex lines `bytes_per_line` bytes long; no record gives an empty source.
pub fn source(doc: &DxfDocument, tokens: Option<Range<u32>>, bytes_per_line: usize) -> Source {
    let Some(tokens) = tokens else {
        return Source {
            bytes_per_line,
            ..Source::default()
        };
    };
    let tape = doc.tape();
    let bytes = tape.byte_range(tokens.clone(), doc.bytes());
    let end = bytes.end.min(bytes.start.saturating_add(SOURCE_LIMIT));
    let pairs = tape
        .pairs(tokens.clone())
        .take_while(|(index, _)| tape.token_start(*index, doc.bytes()) < end)
        .map(|(index, code)| {
            (
                code,
                tape.display_value(code, tape.value(index, doc.bytes())),
            )
        })
        .collect();
    let mut shown = Source {
        tokens: Some(tokens),
        more: bytes.end.saturating_sub(end),
        bytes,
        pairs,
        hex: Vec::new(),
        bytes_per_line,
    };
    shown.refit(doc, bytes_per_line);
    shown
}

/// Bytes per hex line in a pane `width` columns wide: the largest of sixteen, eight and four whose line fits, four when none does.
pub fn bytes_per_line(width: usize) -> usize {
    HEX_LAYOUTS
        .iter()
        .find(|(_, columns)| *columns <= width)
        .or(HEX_LAYOUTS.last())
        .map_or(WIDE_HEX, |(bytes, _)| *bytes)
}

/// `bytes_per_line` bytes per line: the absolute offset, groups of up to eight hex bytes and the printable ASCII.
pub fn hex_lines(bytes: &[u8], base: usize, bytes_per_line: usize) -> Vec<String> {
    let width = bytes_per_line.max(1);
    let group = width.min(GROUP);
    let digits_width = group * 3 - 1;
    bytes
        .chunks(width)
        .enumerate()
        .map(|(line, chunk)| {
            let offset = base.saturating_add(line.saturating_mul(width));
            let groups: Vec<String> = (0..width.div_ceil(group))
                .map(|index| {
                    let digits = hex_bytes(chunk.iter().skip(index * group).take(group));
                    format!("{digits:<digits_width$}")
                })
                .collect();
            let ascii: String = chunk.iter().copied().map(printable).collect();
            format!("{offset:08x}  {}  |{ascii}|", groups.join("  "))
        })
        .collect()
}

fn hex_bytes<'b>(bytes: impl Iterator<Item = &'b u8>) -> String {
    bytes
        .map(|byte| format!("{byte:02x}"))
        .collect::<Vec<_>>()
        .join(" ")
}

fn printable(byte: u8) -> char {
    if !(0x20..=0x7e).contains(&byte) {
        return '.';
    }
    char::from(byte)
}

/// The hex line holding `offset` in a dump starting at `base` with `bytes_per_line` bytes per line.
pub fn hex_line_of(offset: usize, base: usize, bytes_per_line: usize) -> usize {
    offset
        .saturating_sub(base)
        .checked_div(bytes_per_line)
        .unwrap_or(0)
}

/// The first pair starting at or after `offset`, the last pair when none does.
pub fn pair_line_of(pairs_offsets: &[usize], offset: usize) -> usize {
    pairs_offsets
        .partition_point(|start| *start < offset)
        .min(pairs_offsets.len().saturating_sub(1))
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use cadboss_testkit::dxf::DxfBuilder;

    use super::*;
    use crate::outline::Outline;

    fn record(doc: &DxfDocument, kind: &[u8]) -> Range<u32> {
        let tape = doc.tape();
        let start = (0..tape.len())
            .find(|index| tape.code(*index) == 0 && tape.value(*index, doc.bytes()) == kind)
            .unwrap();
        let end = (start + 1..tape.len())
            .find(|index| tape.code(*index) == 0)
            .unwrap();
        start..end
    }

    #[test]
    fn hex_lines_format_sixteen_bytes_per_line() {
        assert_eq!(
            hex_lines(b"ABC", 0x10, WIDE_HEX),
            [format!("00000010  {:<23}  {:<23}  |ABC|", "41 42 43", "")]
        );
        let bytes: Vec<u8> = (1..=17).collect();
        let lines = hex_lines(&bytes, 0x10, WIDE_HEX);
        assert_eq!(lines.len(), 2);
        assert_eq!(
            lines[0],
            "00000010  01 02 03 04 05 06 07 08  09 0a 0b 0c 0d 0e 0f 10  |................|"
        );
        assert_eq!(lines[0].len(), 78);
        assert!(lines[1].starts_with("00000020  11"));
        assert_eq!(lines[1], format!("00000020  {:<23}  {:<23}  |.|", "11", ""));
        assert_eq!(
            hex_lines(b" ~\x7f\x1f", 0, WIDE_HEX),
            [format!(
                "00000000  {:<23}  {:<23}  | ~..|",
                "20 7e 7f 1f", ""
            )]
        );
        assert!(hex_lines(&[], 0, WIDE_HEX).is_empty());
    }

    #[test]
    fn hex_lines_format_eight_bytes_per_line_in_one_group() {
        let bytes: Vec<u8> = (0x41..=0x4a).collect();
        assert_eq!(
            hex_lines(&bytes, 0x29, 8),
            [
                "00000029  41 42 43 44 45 46 47 48  |ABCDEFGH|".to_owned(),
                format!("00000031  {:<23}  |IJ|", "49 4a"),
            ]
        );
    }

    #[test]
    fn hex_lines_format_four_bytes_per_line_in_one_narrow_group() {
        assert_eq!(
            hex_lines(b"ABCDEF", 0x29, 4),
            [
                "00000029  41 42 43 44  |ABCD|".to_owned(),
                format!("0000002d  {:<11}  |EF|", "45 46"),
            ]
        );
        assert_eq!(
            hex_lines(b"AB", 0, 0),
            ["00000000  41  |A|", "00000001  42  |B|"]
        );
    }

    #[test]
    fn each_hex_layout_names_the_columns_its_full_line_takes() {
        for (bytes, columns) in HEX_LAYOUTS {
            let line = &hex_lines(&vec![0x41; bytes], 0, bytes)[0];
            assert_eq!(line.len(), columns, "{line}");
            assert!(line.ends_with('|'), "{line}");
        }
    }

    #[test]
    fn a_pane_takes_the_widest_hex_line_that_fits_down_to_four_bytes() {
        assert_eq!(bytes_per_line(200), 16);
        assert_eq!(bytes_per_line(78), 16);
        assert_eq!(bytes_per_line(77), 8);
        assert_eq!(bytes_per_line(45), 8);
        assert_eq!(bytes_per_line(44), 4);
        assert_eq!(bytes_per_line(29), 4);
        assert_eq!(bytes_per_line(0), 4);
    }

    #[test]
    fn offsets_map_to_hex_lines_and_to_the_pair_at_or_after_them() {
        assert_eq!(hex_line_of(0x10, 0x10, 16), 0);
        assert_eq!(hex_line_of(0x1f, 0x10, 16), 0);
        assert_eq!(hex_line_of(0x20, 0x10, 16), 1);
        assert_eq!(hex_line_of(0x05, 0x10, 16), 0);
        assert_eq!(hex_line_of(0x17, 0x10, 8), 0);
        assert_eq!(hex_line_of(0x18, 0x10, 8), 1);
        assert_eq!(hex_line_of(0x20, 0x10, 8), 2);
        assert_eq!(hex_line_of(0x20, 0x10, 0), 0);
        let offsets = [100, 110, 130, 160];
        assert_eq!(pair_line_of(&offsets, 0), 0);
        assert_eq!(pair_line_of(&offsets, 100), 0);
        assert_eq!(pair_line_of(&offsets, 101), 1);
        assert_eq!(pair_line_of(&offsets, 130), 2);
        assert_eq!(pair_line_of(&offsets, 500), 3);
        assert_eq!(pair_line_of(&[], 500), 0);
    }

    #[test]
    fn source_caps_at_the_limit_and_counts_the_rest() {
        let long = "A".repeat(70_000);
        let doc =
            DxfDocument::load(DxfBuilder::new().mtext(&long, (0.0, 0.0), 2.5).build()).unwrap();
        let tape = doc.tape();
        let tokens = record(&doc, b"MTEXT");
        let bytes = tape.byte_range(tokens.clone(), doc.bytes());
        let mut shown = source(&doc, Some(tokens.clone()), WIDE_HEX);
        assert_eq!(shown.tokens, Some(tokens.clone()));
        assert_eq!(shown.bytes, bytes);
        assert!(bytes.len() > SOURCE_LIMIT);
        assert_eq!(shown.bytes_per_line, WIDE_HEX);
        assert_eq!(shown.hex.len(), SOURCE_LIMIT / 16);
        assert_eq!(shown.more, bytes.len() - SOURCE_LIMIT);
        assert!(shown.hex[0].starts_with(&format!("{:08x}  20 20 30 0a", bytes.start)));
        assert_eq!(
            shown.hex.last().unwrap()[..8],
            format!("{:08x}", bytes.start + SOURCE_LIMIT - 16)
        );
        assert_eq!(shown.hex_dump(&doc, WIDE_HEX), shown.hex);

        let wide = shown.clone();
        shown.refit(&doc, 8);
        assert_eq!(shown.bytes_per_line, 8);
        assert_eq!(shown.hex.len(), SOURCE_LIMIT / 8);
        assert_eq!(
            shown.hex.last().unwrap()[..8],
            format!("{:08x}", bytes.start + SOURCE_LIMIT - 8)
        );
        assert_eq!(shown.hex_dump(&doc, WIDE_HEX), wide.hex);
        assert_eq!(shown.pairs, wide.pairs);
        assert_eq!(shown.more, wide.more);

        let limit = bytes.start + SOURCE_LIMIT;
        let kept = tokens
            .clone()
            .filter(|index| tape.token_start(*index, doc.bytes()) < limit)
            .count();
        assert!(kept < tokens.len());
        assert_eq!(shown.pairs.len(), kept);
        assert_eq!(shown.pairs[0], (0, "MTEXT".to_owned()));
        assert!(shown.pairs.iter().skip(1).all(|(code, _)| *code != 0));
        assert_eq!(shown.offsets(&doc).len(), kept);
        assert!(shown.offsets(&doc).iter().all(|offset| *offset < limit));
        assert_eq!(shown.offsets(&doc)[0], bytes.start);
        assert_eq!(shown.line_count(View::Hex), SOURCE_LIMIT / 8);
        assert_eq!(shown.line_count(View::Pairs), kept);
    }

    #[test]
    fn short_record_shows_every_pair_and_byte() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets/demo.dxf");
        let doc = DxfDocument::open(Path::new(path)).unwrap();
        let tape = doc.tape();
        let tokens = Outline::build(&doc, "demo.dxf").handles[&0x136].clone();
        let shown = source(&doc, Some(tokens.clone()), 8);
        assert_eq!(shown.more, 0);
        assert_eq!(shown.pairs.len(), tokens.len());
        assert_eq!(shown.pairs.len(), 42);
        assert_eq!(shown.hex.len(), shown.bytes.len().div_ceil(8));
        assert_eq!(shown.pairs[0], (0, "INSERT".to_owned()));
        assert_eq!(shown.pairs[1], (5, "136".to_owned()));
        assert!(shown.pairs.contains(&(2, "PUMP".to_owned())));
        assert!(shown.pairs.contains(&(1, "P-101".to_owned())));
        let records: Vec<&str> = shown
            .pairs
            .iter()
            .filter(|(code, _)| *code == 0)
            .map(|(_, value)| value.as_str())
            .collect();
        assert_eq!(records, ["INSERT", "ATTRIB", "ATTRIB", "SEQEND"]);
        let offsets = shown.offsets(&doc);
        let starts: Vec<usize> = tokens
            .map(|index| tape.token_start(index, doc.bytes()))
            .collect();
        assert_eq!(offsets, starts);
        let empty = source(&doc, None, 8);
        assert_eq!(empty.bytes_per_line, 8);
        assert_eq!(
            empty,
            Source {
                bytes_per_line: 8,
                ..Source::default()
            }
        );
        assert!(empty.hex_dump(&doc, WIDE_HEX).is_empty());
        assert_eq!(Source::default().line_count(View::Pairs), 0);
    }

    #[test]
    fn binary_pairs_print_numbers_as_obj_does() {
        let bytes = DxfBuilder::new()
            .line((0.0, 0.5), (12.25, 3.0))
            .binary()
            .build();
        let doc = DxfDocument::load(bytes).unwrap();
        let shown = source(&doc, Some(record(&doc, b"LINE")), WIDE_HEX);
        assert_eq!(shown.pairs[0], (0, "LINE".to_owned()));
        assert!(shown.pairs.contains(&(8, "0".to_owned())));
        assert!(shown.pairs.contains(&(10, "0".to_owned())));
        assert!(shown.pairs.contains(&(20, "0.5".to_owned())));
        assert!(shown.pairs.contains(&(11, "12.25".to_owned())));
        assert!(shown.pairs.contains(&(21, "3".to_owned())));
    }
}
