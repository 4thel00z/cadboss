use cadboss_core::{Budget, Error, Point, Report, SkipKind, SkipReason};
use cadboss_dxf::{decode, decode_unescaped, read_header, unescape, Codepage, SectionName, Tape};
use cadboss_model::{Text, Units};
use cadboss_testkit::dxf::simple_drawing;

fn lex(bytes: &[u8]) -> Result<Tape, Error> {
    Ok(Tape::lex(bytes, &Budget::default())?.0)
}

#[test]
fn header_reads_version_units_and_extents_from_simple_drawing() {
    let bytes = simple_drawing();
    let tape = lex(&bytes).unwrap();
    let mut report = Report::default();
    let header = read_header(&tape, &bytes, &mut report);
    let metadata = header.metadata();
    assert_eq!(metadata.version, Text::from("AC1027"));
    assert_eq!(metadata.units, Units::Millimeters);
    assert_eq!(metadata.author, Some(Text::from("cadboss-testkit")));
    assert_eq!(metadata.created, Some(2460371.5));
    assert_eq!(metadata.updated, Some(2460372.5));
    assert!(metadata.extents.is_some());
    assert!(report.is_empty());
}

#[test]
fn codepage_is_utf8_from_r2007_regardless_of_dwgcodepage() {
    assert_eq!(
        Codepage::from_header(b"AC1021", Some(b"ANSI_1252")),
        Codepage::Utf8
    );
    assert_eq!(
        Codepage::from_header(b"AC1027", Some(b"ISO8859-1")),
        Codepage::Utf8
    );
    assert_eq!(
        Codepage::from_header(b"AC1009", Some(b"ansi_1252")),
        Codepage::Windows1252
    );
    assert_eq!(
        Codepage::from_header(b"AC1015", None),
        Codepage::Windows1252
    );
    assert_eq!(
        Codepage::from_header(b"AC1015", Some(b"Latin1")),
        Codepage::Iso8859_1
    );
    assert_eq!(
        Codepage::from_header(b"AC1015", Some(b"ANSI_932")),
        Codepage::Unsupported
    );
}

#[test]
fn r12_fixture_decodes_latin1_e_acute() {
    let bytes = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/fixtures/r12.dxf"
    ))
    .unwrap();
    let tape = lex(&bytes).unwrap();
    let mut report = Report::default();
    let header = read_header(&tape, &bytes, &mut report);
    let codepage = Codepage::from_header(
        header.version.as_str().unwrap_or("").as_bytes(),
        header
            .codepage
            .as_ref()
            .and_then(Text::as_str)
            .map(str::as_bytes),
    );
    let entities = tape.section(SectionName::Entities).unwrap();
    let value = (entities.start..entities.end)
        .find(|index| tape.code(*index) == 1)
        .map(|index| tape.value(index, &bytes))
        .unwrap();
    assert_eq!(
        decode(codepage, value, &mut report),
        Text::from("caf\u{e9}")
    );
}

#[test]
fn unknown_codepage_passes_bytes_through_and_reports() {
    let mut report = Report::default();
    let text = decode(Codepage::Unsupported, &[0x83, 0x41], &mut report);
    assert_eq!(text, Text::Bytes(vec![0x83, 0x41]));
    assert_eq!(report.skipped.len(), 1);
    assert_eq!(report.skipped[0].kind, SkipKind::Text);
    assert_eq!(report.skipped[0].reason, SkipReason::UnsupportedCodepage);
}

#[test]
fn unescape_turns_percent_codes_and_unicode_escapes_into_chars() {
    assert_eq!(unescape("%%c20 \\U+00B0"), "\u{d8}20 \u{b0}");
    assert_eq!(unescape("%%d%%p%%%"), "\u{b0}\u{b1}%");
    assert_eq!(unescape("%%uA%%oB"), "AB");
    assert_eq!(
        unescape("\\u+00e9 \\M+1FFFF \\U+D800"),
        "\u{e9} \u{fffd} \u{fffd}"
    );
    assert_eq!(unescape("%%065 %%q ^J"), "A %%q ^J");
}

#[test]
fn windows_1252_control_range_maps_the_euro_sign() {
    let mut report = Report::default();
    assert_eq!(
        decode(Codepage::Windows1252, &[0x80], &mut report),
        Text::from("\u{20ac}")
    );
    assert_eq!(
        decode(Codepage::Windows1252, &[0x81], &mut report),
        Text::from("\u{fffd}")
    );
    assert_eq!(
        decode(Codepage::Iso8859_1, &[0x80, 0xe9], &mut report),
        Text::from("\u{80}\u{e9}")
    );
    assert!(report.is_empty());
}

#[test]
fn pure_ascii_bytes_decode_without_the_table() {
    let mut report = Report::default();
    assert_eq!(
        decode(Codepage::Windows1252, b"Plan", &mut report),
        Text::Str("Plan".to_owned())
    );
    assert_eq!(
        decode_unescaped(Codepage::Utf8, b"%%c5", &mut report),
        Text::from("\u{d8}5")
    );
}

#[test]
fn header_with_an_unparsable_extents_value_reports_and_continues() {
    let text = "  0\nSECTION\n  2\nHEADER\n  9\n$EXTMIN\n 10\nabc\n 20\n1\n  9\n$INSUNITS\n 70\n6\n  9\n$EXTMAX\n 10\n5\n 20\n6\n  0\nENDSEC\n  0\nEOF\n";
    let bytes = text.as_bytes();
    let tape = lex(bytes).unwrap();
    let mut report = Report::default();
    let header = read_header(&tape, bytes, &mut report);
    assert_eq!(header.insunits, 6);
    assert!(header.extmin.is_none());
    assert_eq!(header.extmax, Some(Point::new(5.0, 6.0, 0.0)));
    assert!(header.metadata().extents.is_none());
    assert_eq!(report.skipped[0].kind, SkipKind::Table);
    assert_eq!(report.skipped[0].reason, SkipReason::Malformed);
}
