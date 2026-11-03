use cadboss_core::{Budget, Error, Report, SkipKind, SkipReason};
use cadboss_dxf::{read_block_records, read_layers, Codepage, Tape};
use cadboss_model::Text;
use cadboss_testkit::dxf::{simple_drawing, DxfBuilder};

fn lex(bytes: &[u8]) -> Result<Tape, Error> {
    Ok(Tape::lex(bytes, &Budget::default())?.0)
}

#[test]
fn layers_carry_color_frozen_and_off_flags() {
    let bytes = DxfBuilder::new()
        .layer("HIDDEN", -3)
        .layer("DIM", 1)
        .build();
    let tape = lex(&bytes).unwrap();
    let mut report = Report::default();
    let layers = read_layers(&tape, &bytes, Codepage::Utf8, &mut report);
    let hidden = layers
        .iter()
        .find(|l| l.name == Text::from("HIDDEN"))
        .unwrap();
    assert!(hidden.off);
    assert_eq!(hidden.color, 3);
    let dim = layers.iter().find(|l| l.name == Text::from("DIM")).unwrap();
    assert!(!dim.off && !dim.frozen && !dim.locked);
    assert_eq!(dim.color, 1);
    assert!(report.is_empty());
}

#[test]
fn layer_flags_bit_one_freezes_and_bit_four_locks() {
    let text = "  0\nSECTION\n  2\nTABLES\n  0\nTABLE\n  2\nLAYER\n  0\nLAYER\n  2\nA\n 70\n5\n 62\n7\n  0\nENDTAB\n  0\nENDSEC\n  0\nEOF\n";
    let bytes = text.as_bytes();
    let tape = lex(bytes).unwrap();
    let mut report = Report::default();
    let layers = read_layers(&tape, bytes, Codepage::Utf8, &mut report);
    assert_eq!(layers.len(), 1);
    assert!(layers[0].frozen && layers[0].locked && !layers[0].off);
}

#[test]
fn block_records_map_names_to_layout_handles() {
    let bytes = simple_drawing();
    let tape = lex(&bytes).unwrap();
    let mut report = Report::default();
    let records = read_block_records(&tape, &bytes, Codepage::Utf8, &mut report);
    let model = records
        .iter()
        .find(|r| r.name == Text::from("*Model_Space"))
        .unwrap();
    assert!(model.layout_handle.is_some());
    let title = records
        .iter()
        .find(|r| r.name == Text::from("TITLEBLOCK"))
        .unwrap();
    assert!(title.layout_handle.is_none());
    assert_ne!(title.handle, model.handle);
}

#[test]
fn block_record_without_a_name_is_skipped() {
    let text = "  0\nSECTION\n  2\nTABLES\n  0\nTABLE\n  2\nBLOCK_RECORD\n  0\nBLOCK_RECORD\n  5\nA1\n  0\nBLOCK_RECORD\n  5\nA2\n  2\nB\n  0\nENDTAB\n  0\nENDSEC\n  0\nEOF\n";
    let bytes = text.as_bytes();
    let tape = lex(bytes).unwrap();
    let mut report = Report::default();
    let records = read_block_records(&tape, bytes, Codepage::Utf8, &mut report);
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].handle, 0xA2);
    assert_eq!(report.skipped[0].kind, SkipKind::Table);
    assert_eq!(report.skipped[0].reason, SkipReason::Malformed);
}
