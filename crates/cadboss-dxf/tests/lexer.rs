use cadboss_core::{Budget, Error, Report, SkipKind, SkipReason, Skipped};
use cadboss_dxf::{Cursor, SectionName, Tape, Token};

fn lex(bytes: &[u8]) -> Result<(Tape, Report), Error> {
    Tape::lex(bytes, &Budget::default())
}

#[test]
fn lexer_builds_section_index_from_a_minimal_file() {
    let bytes = b"  0\nSECTION\n  2\nHEADER\n  9\n$ACADVER\n  1\nAC1027\n  0\nENDSEC\n  0\nEOF\n";
    let (tape, report) = lex(bytes).unwrap();
    assert!(report.is_empty());
    let h = tape.section(SectionName::Header).unwrap();
    assert_eq!(tape.code(h.start), 9);
    assert_eq!(tape.value(h.start + 1, bytes), b"AC1027");
    assert_eq!(tape.code(h.end), 0);
    assert_eq!(tape.value(h.end, bytes), b"ENDSEC");
}

#[test]
fn tape_skips_a_section_in_constant_time() {
    let mut text = String::from("  0\nSECTION\n  2\nENTITIES\n");
    for i in 0..10_000 {
        text.push_str(&format!("  0\nLINE\n  8\nL{i}\n 10\n0.0\n 20\n0.0\n"));
    }
    text.push_str("  0\nENDSEC\n  0\nSECTION\n  2\nOBJECTS\n  0\nENDSEC\n  0\nEOF\n");
    let (tape, report) = lex(text.as_bytes()).unwrap();
    assert!(report.is_empty());
    let entities = tape.section(SectionName::Entities).unwrap();
    let objects = tape.section(SectionName::Objects).unwrap();
    assert_eq!(objects.start, entities.end + 3);
    assert_eq!(entities.end - entities.start, 40_000);
}

#[test]
fn lexer_resynchronizes_after_a_bad_group_code() {
    let bytes = b"999\ncomment\n  1O\nfoo\n  0\nLINE\n";
    let (tape, report) = lex(bytes).unwrap();
    assert_eq!(
        report.skipped,
        vec![Skipped {
            kind: SkipKind::Token,
            reason: SkipReason::Malformed,
            count: 1
        }]
    );
    assert_eq!(tape.len(), 2);
    assert_eq!(tape.code(1), 0);
    assert_eq!(tape.value(1, bytes), b"LINE");
}

#[test]
fn lexer_reports_a_truncated_value() {
    let bytes = b"  0\nSECTION\n  2\nENTITIES\n  0\n";
    let (tape, report) = lex(bytes).unwrap();
    assert!(report.skipped.contains(&Skipped {
        kind: SkipKind::Token,
        reason: SkipReason::Truncated,
        count: 1
    }));
    assert_eq!(tape.len(), 2);
}

#[test]
fn lexer_reports_a_truncated_section() {
    let bytes = b"  0\nSECTION\n  2\nENTITIES\n  0\nLINE\n  8\n0\n";
    let (tape, report) = lex(bytes).unwrap();
    assert_eq!(
        report.skipped,
        vec![Skipped {
            kind: SkipKind::Section,
            reason: SkipReason::Truncated,
            count: 1
        }]
    );
    let entities = tape.section(SectionName::Entities).unwrap();
    assert_eq!(entities.end, tape.len());
}

#[test]
fn lexer_closes_an_open_section_when_another_begins() {
    let bytes = b"  0\nSECTION\n  2\nTABLES\n  0\nSECTION\n  2\nBLOCKS\n  0\nENDSEC\n  0\nEOF\n";
    let (tape, report) = lex(bytes).unwrap();
    assert_eq!(
        report.skipped,
        vec![Skipped {
            kind: SkipKind::Section,
            reason: SkipReason::Truncated,
            count: 1
        }]
    );
    assert_eq!(tape.section(SectionName::Tables).unwrap().end, 2);
    assert!(tape.section(SectionName::Blocks).is_some());
}

#[test]
fn lexer_reports_endsec_without_a_section() {
    let (tape, report) = lex(b"  0\nENDSEC\n  0\nEOF\n").unwrap();
    assert_eq!(
        report.skipped,
        vec![Skipped {
            kind: SkipKind::Section,
            reason: SkipReason::Malformed,
            count: 1
        }]
    );
    assert!(tape.section(SectionName::Other).is_none());
}

#[test]
fn lexer_names_unknown_sections_other() {
    let bytes = b"  0\nSECTION\n  2\nWHATEVER\n  0\nENDSEC\n  0\nEOF\n";
    let (tape, report) = lex(bytes).unwrap();
    assert!(report.is_empty());
    assert!(tape.section(SectionName::Other).is_some());
}

#[test]
fn lexer_ignores_bytes_after_eof() {
    let bytes = b"  0\nSECTION\n  2\nHEADER\n  0\nENDSEC\n  0\nEOF\ngarbage\nmore\n";
    let (tape, report) = lex(bytes).unwrap();
    assert!(report.is_empty());
    assert_eq!(tape.len(), 4);
}

#[test]
fn lexer_accepts_crlf_and_a_leading_bom() {
    let bytes = b"\xef\xbb\xbf  0\r\nSECTION\r\n  2\r\nHEADER\r\n  9\r\n$ACADVER\r\n  1\r\nAC 1027\r\n  0\r\nENDSEC\r\n  0\r\nEOF\r\n";
    let (tape, report) = lex(bytes).unwrap();
    assert!(report.is_empty());
    let h = tape.section(SectionName::Header).unwrap();
    assert_eq!(tape.value(h.start, bytes), b"$ACADVER");
    assert_eq!(tape.value(h.start + 1, bytes), b"AC 1027");
}

#[test]
fn lexer_stores_oversized_values_out_of_line() {
    let long = "x".repeat(70_000);
    let text =
        format!("  0\nSECTION\n  2\nHEADER\n  1\n{long}\n  1\nshort\n  0\nENDSEC\n  0\nEOF\n");
    let bytes = text.as_bytes();
    let (tape, report) = lex(bytes).unwrap();
    assert!(report.is_empty());
    let h = tape.section(SectionName::Header).unwrap();
    assert_eq!(tape.value(h.start, bytes).len(), 70_000);
    assert_eq!(tape.value(h.start + 1, bytes), b"short");
}

#[test]
fn cursor_walks_pairs_and_parses_numbers() {
    let bytes = b"  0\nLINE\n 10\n1.5\n 70\n-3\n  5\n1F\n  0\nEOF\n";
    let (tape, _) = lex(bytes).unwrap();
    let mut cursor = Cursor::new(&tape, bytes, 0..tape.len());
    assert_eq!(cursor.peek_code(), Some(0));
    assert_eq!(cursor.next(), Some((0, &b"LINE"[..])));
    let (code, value) = cursor.next().unwrap();
    assert_eq!((code, cursor.f64(value)), (10, Some(1.5)));
    let (_, value) = cursor.next().unwrap();
    assert_eq!(cursor.i64(value), Some(-3));
    let (_, value) = cursor.next().unwrap();
    assert_eq!(cursor.handle(value), Some(0x1F));
    assert_eq!(cursor.index(), 4);
    cursor.skip_to_code_zero();
    assert_eq!(cursor.peek_code(), Some(0));
    assert_eq!(cursor.next().unwrap().0, 0);
    assert_eq!(cursor.next(), None);
}

#[test]
fn cursor_skips_to_the_end_without_a_code_zero() {
    let bytes = b" 10\n1\n 20\n2\n";
    let (tape, _) = lex(bytes).unwrap();
    let mut cursor = Cursor::new(&tape, bytes, 0..tape.len());
    cursor.skip_to_code_zero();
    assert_eq!(cursor.peek_code(), None);
}

#[test]
fn pairs_yields_index_and_code() {
    let bytes = b"  0\nLINE\n 10\n1\n 20\n2\n";
    let (tape, _) = lex(bytes).unwrap();
    let pairs: Vec<(u32, i16)> = tape.pairs(1..3).collect();
    assert_eq!(pairs, vec![(1, 10), (2, 20)]);
}

#[test]
fn token_is_eight_bytes() {
    assert_eq!(std::mem::size_of::<Token>(), 8);
}

#[test]
fn lexer_rejects_a_png_as_not_dxf() {
    let result = Tape::lex(b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR", &Budget::default());
    assert!(matches!(result, Err(Error::NotDxf)));
}

#[test]
fn lexer_rejects_empty_input() {
    assert!(matches!(
        Tape::lex(b"", &Budget::default()),
        Err(Error::Empty)
    ));
}

#[test]
fn lexer_rejects_input_over_the_byte_budget() {
    let budget = Budget {
        max_bytes: 4,
        ..Budget::default()
    };
    assert!(matches!(
        Tape::lex(b"  0\nEOF\n", &budget),
        Err(Error::BudgetExceeded { .. })
    ));
}

#[test]
fn binary_sentinel_is_recognized_but_unsupported_for_now() {
    let result = Tape::lex(
        b"AutoCAD Binary DXF\r\n\x1a\0\x00SECTION\0",
        &Budget::default(),
    );
    assert!(matches!(result, Err(Error::Unsupported("binary dxf"))));
}

#[test]
fn lexer_stops_at_the_token_budget_and_reports() {
    let bytes = b"  0\nSECTION\n  2\nHEADER\n  9\n$ACADVER\n  1\nAC1027\n  0\nENDSEC\n  0\nEOF\n";
    let budget = Budget {
        max_tokens: 4,
        ..Budget::default()
    };
    let (tape, report) = Tape::lex(bytes, &budget).unwrap();
    assert_eq!(tape.len(), 4);
    assert!(report.skipped.contains(&Skipped {
        kind: SkipKind::Token,
        reason: SkipReason::TokenLimit,
        count: 1
    }));
    assert!(report.skipped.contains(&Skipped {
        kind: SkipKind::Section,
        reason: SkipReason::Truncated,
        count: 1
    }));
    assert_eq!(tape.section(SectionName::Header).unwrap().end, 4);
}

#[test]
fn a_section_open_at_eof_ends_at_the_eof_token() {
    let bytes = b"  0\nSECTION\n  2\nENTITIES\n  0\nLINE\n  0\nEOF\n";
    let (tape, report) = lex(bytes).unwrap();
    assert_eq!(
        report.skipped,
        vec![Skipped {
            kind: SkipKind::Section,
            reason: SkipReason::Truncated,
            count: 1
        }]
    );
    assert_eq!(tape.section(SectionName::Entities).unwrap().end, 3);
}

#[test]
fn lexer_matches_padded_structure_values() {
    let bytes = b"  0\nSECTION \n  2\nHEADER\n  0\n ENDSEC\n  0\nEOF \n";
    let (tape, report) = lex(bytes).unwrap();
    assert!(report.is_empty());
    assert_eq!(tape.section(SectionName::Header).unwrap().end, 2);
}

#[test]
fn a_first_line_beyond_i16_is_not_dxf() {
    assert!(matches!(lex(b"99999\nfoo\n"), Err(Error::NotDxf)));
}

#[test]
fn two_separate_malformed_runs_count_twice() {
    let bytes = b"999\nc\nxx\nyy\n  0\nLINE\nzz\nww\n";
    let (_, report) = lex(bytes).unwrap();
    assert_eq!(
        report.skipped,
        vec![Skipped {
            kind: SkipKind::Token,
            reason: SkipReason::Malformed,
            count: 2
        }]
    );
}
