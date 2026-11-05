//! The source pane's two views of a record: its group-code pairs and a hex dump of its bytes.

use std::ops::Range;

use cadboss_dxf::DxfDocument;

/// Bytes of a record the source pane lays out; the rest is only counted.
pub const SOURCE_LIMIT: usize = 64 * 1024;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum View {
    Pairs,
    Hex,
}

/// A record's first `SOURCE_LIMIT` bytes as decoded pairs and as hex lines; `more` counts the bytes past the limit.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Source {
    pub tokens: Option<Range<u32>>,
    pub bytes: Range<usize>,
    pub pairs: Vec<(i16, String)>,
    pub hex: Vec<String>,
    pub more: usize,
}

impl Source {
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

/// The record of `tokens`, its pairs printed as `cadboss obj` prints them; no record gives an empty source.
pub fn source(doc: &DxfDocument, tokens: Option<Range<u32>>) -> Source {
    let Some(tokens) = tokens else {
        return Source::default();
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
    let hex = hex_lines(
        doc.bytes().get(bytes.start..end).unwrap_or(&[]),
        bytes.start,
    );
    Source {
        tokens: Some(tokens),
        more: bytes.end.saturating_sub(end),
        bytes,
        pairs,
        hex,
    }
}

/// Sixteen bytes per line: the absolute offset, two groups of eight hex bytes and the printable ASCII.
pub fn hex_lines(bytes: &[u8], base: usize) -> Vec<String> {
    bytes
        .chunks(16)
        .enumerate()
        .map(|(line, chunk)| {
            let offset = base.saturating_add(line * 16);
            let (left, right) = chunk.split_at(chunk.len().min(8));
            let ascii: String = chunk.iter().copied().map(printable).collect();
            format!(
                "{offset:08x}  {:<23}  {:<23}  |{ascii}|",
                hex_bytes(left),
                hex_bytes(right)
            )
        })
        .collect()
}

fn hex_bytes(bytes: &[u8]) -> String {
    bytes
        .iter()
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

/// The hex line holding `offset` in a dump starting at `base`.
pub fn hex_line_of(offset: usize, base: usize) -> usize {
    offset.saturating_sub(base) / 16
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
            hex_lines(b"ABC", 0x10),
            [format!("00000010  {:<23}  {:<23}  |ABC|", "41 42 43", "")]
        );
        let bytes: Vec<u8> = (1..=17).collect();
        let lines = hex_lines(&bytes, 0x10);
        assert_eq!(lines.len(), 2);
        assert_eq!(
            lines[0],
            "00000010  01 02 03 04 05 06 07 08  09 0a 0b 0c 0d 0e 0f 10  |................|"
        );
        assert!(lines[1].starts_with("00000020  11"));
        assert_eq!(lines[1], format!("00000020  {:<23}  {:<23}  |.|", "11", ""));
        assert_eq!(
            hex_lines(b" ~\x7f\x1f", 0),
            [format!(
                "00000000  {:<23}  {:<23}  | ~..|",
                "20 7e 7f 1f", ""
            )]
        );
        assert!(hex_lines(&[], 0).is_empty());
    }

    #[test]
    fn offsets_map_to_hex_lines_and_to_the_pair_at_or_after_them() {
        assert_eq!(hex_line_of(0x10, 0x10), 0);
        assert_eq!(hex_line_of(0x1f, 0x10), 0);
        assert_eq!(hex_line_of(0x20, 0x10), 1);
        assert_eq!(hex_line_of(0x05, 0x10), 0);
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
        let shown = source(&doc, Some(tokens.clone()));
        assert_eq!(shown.tokens, Some(tokens.clone()));
        assert_eq!(shown.bytes, bytes);
        assert!(bytes.len() > SOURCE_LIMIT);
        assert_eq!(shown.hex.len(), SOURCE_LIMIT / 16);
        assert_eq!(shown.more, bytes.len() - SOURCE_LIMIT);
        assert!(shown.hex[0].starts_with(&format!("{:08x}  20 20 30 0a", bytes.start)));
        assert_eq!(
            shown.hex.last().unwrap()[..8],
            format!("{:08x}", bytes.start + SOURCE_LIMIT - 16)
        );

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
        assert_eq!(shown.line_count(View::Hex), SOURCE_LIMIT / 16);
        assert_eq!(shown.line_count(View::Pairs), kept);
    }

    #[test]
    fn short_record_shows_every_pair_and_byte() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets/demo.dxf");
        let doc = DxfDocument::open(Path::new(path)).unwrap();
        let tape = doc.tape();
        let tokens = Outline::build(&doc, "demo.dxf").handles[&0x136].clone();
        let shown = source(&doc, Some(tokens.clone()));
        assert_eq!(shown.more, 0);
        assert_eq!(shown.pairs.len(), tokens.len());
        assert_eq!(shown.pairs.len(), 42);
        assert_eq!(shown.hex.len(), shown.bytes.len().div_ceil(16));
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
        assert_eq!(source(&doc, None), Source::default());
        assert_eq!(Source::default().line_count(View::Pairs), 0);
    }

    #[test]
    fn binary_pairs_print_numbers_as_obj_does() {
        let bytes = DxfBuilder::new()
            .line((0.0, 0.5), (12.25, 3.0))
            .binary()
            .build();
        let doc = DxfDocument::load(bytes).unwrap();
        let shown = source(&doc, Some(record(&doc, b"LINE")));
        assert_eq!(shown.pairs[0], (0, "LINE".to_owned()));
        assert!(shown.pairs.contains(&(8, "0".to_owned())));
        assert!(shown.pairs.contains(&(10, "0".to_owned())));
        assert!(shown.pairs.contains(&(20, "0.5".to_owned())));
        assert!(shown.pairs.contains(&(11, "12.25".to_owned())));
        assert!(shown.pairs.contains(&(21, "3".to_owned())));
    }
}
