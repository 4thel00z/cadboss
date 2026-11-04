use cadboss_core::{Budget, Report, SkipKind, SkipReason, Skipped};
use cadboss_dxf::{Cursor, SectionName, Tape, SENTINEL};

fn pair_str(code: i16, text: &str) -> Vec<u8> {
    let mut out = code.to_le_bytes().to_vec();
    out.extend_from_slice(text.as_bytes());
    out.push(0);
    out
}

fn pair_f64(code: i16, value: f64) -> Vec<u8> {
    let mut out = code.to_le_bytes().to_vec();
    out.extend_from_slice(&value.to_le_bytes());
    out
}

fn pair_i16(code: i16, value: i16) -> Vec<u8> {
    let mut out = code.to_le_bytes().to_vec();
    out.extend_from_slice(&value.to_le_bytes());
    out
}

fn pair_i32(code: i16, value: i32) -> Vec<u8> {
    let mut out = code.to_le_bytes().to_vec();
    out.extend_from_slice(&value.to_le_bytes());
    out
}

fn file(parts: &[Vec<u8>]) -> Vec<u8> {
    let mut out = SENTINEL.to_vec();
    parts.iter().for_each(|part| out.extend_from_slice(part));
    out
}

fn header_file() -> Vec<u8> {
    file(&[
        pair_str(0, "SECTION"),
        pair_str(2, "HEADER"),
        pair_str(9, "$ACADVER"),
        pair_str(1, "AC1015"),
        pair_str(0, "ENDSEC"),
        pair_str(0, "EOF"),
    ])
}

#[test]
fn binary_sentinel_switches_to_binary_lexing() {
    let bytes = header_file();
    let (tape, report) = Tape::lex(&bytes, &Budget::default()).unwrap();
    assert!(tape.is_binary());
    assert!(report.is_empty());
    let h = tape.section(SectionName::Header).unwrap();
    assert_eq!(tape.code(h.start), 9);
    assert_eq!(tape.value(h.start + 1, &bytes), b"AC1015");
    assert_eq!(tape.value(h.end, &bytes), b"ENDSEC");
}

#[test]
fn binary_f64_values_round_trip_through_cursor() {
    let bytes = file(&[
        pair_str(0, "SECTION"),
        pair_str(2, "ENTITIES"),
        pair_f64(10, 1.5),
        pair_i16(70, -7),
        pair_i32(90, 70_000),
        pair_str(0, "ENDSEC"),
        pair_str(0, "EOF"),
    ]);
    let (tape, _) = Tape::lex(&bytes, &Budget::default()).unwrap();
    let section = tape.section(SectionName::Entities).unwrap();
    let mut cursor = Cursor::new(&tape, &bytes, section.start..section.end);
    let (code, value) = cursor.next().unwrap();
    assert_eq!(code, 10);
    assert_eq!(cursor.f64(value), Some(1.5));
    let (_, value) = cursor.next().unwrap();
    assert_eq!(cursor.i64(value), Some(-7));
    let (_, value) = cursor.next().unwrap();
    assert_eq!(cursor.i64(value), Some(70_000));
}

#[test]
fn binary_truncated_mid_value_reports_truncated_section() {
    let mut bytes = file(&[
        pair_str(0, "SECTION"),
        pair_str(2, "ENTITIES"),
        pair_f64(10, 1.5),
    ]);
    bytes.truncate(bytes.len() - 3);
    let (tape, report) = Tape::lex(&bytes, &Budget::default()).unwrap();
    assert!(tape.section(SectionName::Entities).is_some());
    assert!(report.skipped.contains(&Skipped {
        kind: SkipKind::Token,
        reason: SkipReason::Truncated,
        count: 1,
    }));
    assert!(report.skipped.contains(&Skipped {
        kind: SkipKind::Section,
        reason: SkipReason::Truncated,
        count: 1,
    }));
}

fn r12_str(code: u8, text: &str) -> Vec<u8> {
    let mut out = vec![code];
    out.extend_from_slice(text.as_bytes());
    out.push(0);
    out
}

fn r12_entities_start() -> Vec<Vec<u8>> {
    vec![r12_str(0, "SECTION"), r12_str(2, "ENTITIES")]
}

#[test]
fn binary_r12_one_byte_codes_lex_the_header() {
    let bytes = file(&[
        r12_str(0, "SECTION"),
        r12_str(2, "HEADER"),
        r12_str(9, "$ACADVER"),
        r12_str(1, "AC1009"),
        r12_str(0, "ENDSEC"),
        r12_str(0, "EOF"),
    ]);
    let (tape, report) = Tape::lex(&bytes, &Budget::default()).unwrap();
    assert!(report.is_empty());
    let h = tape.section(SectionName::Header).unwrap();
    assert_eq!(tape.code(h.start), 9);
    assert_eq!(tape.value(h.start + 1, &bytes), b"AC1009");
    assert_eq!(tape.value(h.end, &bytes), b"ENDSEC");
}

#[test]
fn binary_r12_escape_reads_a_two_byte_code() {
    let mut escaped = vec![255, 10, 0];
    escaped.extend_from_slice(&1.5f64.to_le_bytes());
    let mut parts = r12_entities_start();
    parts.push(escaped);
    parts.push(r12_str(0, "ENDSEC"));
    let bytes = file(&parts);
    let (tape, report) = Tape::lex(&bytes, &Budget::default()).unwrap();
    assert!(report.is_empty());
    let section = tape.section(SectionName::Entities).unwrap();
    assert_eq!(section.end - section.start, 1);
    assert_eq!(tape.code(section.start), 10);
    let cursor = Cursor::new(&tape, &bytes, section.start..section.end);
    assert_eq!(cursor.f64(tape.value(section.start, &bytes)), Some(1.5));
}

#[test]
fn binary_r12_escaped_code_in_plain_range_is_not_misread() {
    let mut parts = r12_entities_start();
    parts.push(vec![255, 1, 0, b'x', 0]);
    parts.push(r12_str(0, "ENDSEC"));
    let bytes = file(&parts);
    let (tape, report) = Tape::lex(&bytes, &Budget::default()).unwrap();
    assert!(report.is_empty());
    let section = tape.section(SectionName::Entities).unwrap();
    assert_eq!(tape.code(section.start), 1);
    assert_eq!(tape.value(section.start, &bytes), b"x");
}

#[test]
fn binary_chunk_longer_than_the_input_reports_truncated() {
    let mut bytes = file(&[pair_str(0, "SECTION"), pair_str(2, "OBJECTS")]);
    bytes.extend_from_slice(&310i16.to_le_bytes());
    bytes.push(200);
    bytes.extend_from_slice(&[7; 10]);
    let (tape, report) = Tape::lex(&bytes, &Budget::default()).unwrap();
    assert_eq!(tape.len(), 2);
    assert!(report.skipped.contains(&Skipped {
        kind: SkipKind::Token,
        reason: SkipReason::Truncated,
        count: 1,
    }));
}

#[test]
fn binary_bool_values_decode_through_cursor() {
    let mut bytes = file(&[pair_str(0, "SECTION"), pair_str(2, "OBJECTS")]);
    bytes.extend_from_slice(&290i16.to_le_bytes());
    bytes.push(1);
    bytes.extend_from_slice(&pair_str(0, "ENDSEC"));
    let (tape, _) = Tape::lex(&bytes, &Budget::default()).unwrap();
    let section = tape.section(SectionName::Objects).unwrap();
    let cursor = Cursor::new(&tape, &bytes, section.start..section.end);
    assert_eq!(cursor.i64(tape.value(section.start, &bytes)), Some(1));
}

#[test]
fn binary_f64_with_seven_bytes_reports_truncated() {
    let mut bytes = file(&[pair_str(0, "SECTION"), pair_str(2, "ENTITIES")]);
    bytes.extend_from_slice(&10i16.to_le_bytes());
    bytes.extend_from_slice(&[0; 7]);
    let (tape, report) = Tape::lex(&bytes, &Budget::default()).unwrap();
    assert_eq!(tape.len(), 2);
    assert!(report.skipped.contains(&Skipped {
        kind: SkipKind::Token,
        reason: SkipReason::Truncated,
        count: 1,
    }));
}

#[test]
fn binary_string_without_nul_reports_truncated() {
    let mut bytes = file(&[pair_str(0, "SECTION"), pair_str(2, "HEADER")]);
    bytes.extend_from_slice(&1i16.to_le_bytes());
    bytes.extend_from_slice(b"AC10");
    let (tape, report): (Tape, Report) = Tape::lex(&bytes, &Budget::default()).unwrap();
    assert_eq!(tape.len(), 2);
    assert!(report.skipped.contains(&Skipped {
        kind: SkipKind::Token,
        reason: SkipReason::Truncated,
        count: 1,
    }));
}

#[test]
fn binary_tape_respects_the_token_budget() {
    let bytes = header_file();
    let budget = Budget {
        max_tokens: 3,
        ..Budget::default()
    };
    let (tape, report) = Tape::lex(&bytes, &budget).unwrap();
    assert_eq!(tape.len(), 3);
    assert!(report.skipped.contains(&Skipped {
        kind: SkipKind::Token,
        reason: SkipReason::TokenLimit,
        count: 1,
    }));
}
