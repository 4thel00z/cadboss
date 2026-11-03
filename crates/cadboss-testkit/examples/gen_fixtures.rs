use std::fs;
use std::path::PathBuf;

use cadboss_testkit::dxf::{
    multi_layout, nested_inserts, simple_builder, simple_drawing, DxfBuilder,
};

fn main() -> std::io::Result<()> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures");
    fs::create_dir_all(&dir)?;
    let simple_binary = simple_builder().binary().build();
    let r12 = DxfBuilder::new()
        .version("AC1009")
        .codepage("ANSI_1252")
        .text_bytes(b"caf\xE9", (0.0, 0.0), 2.5)
        .build();
    let fixtures = [
        ("simple.dxf", simple_drawing()),
        ("simple-binary.dxf", simple_binary),
        ("nested-70.dxf", nested_inserts(70)),
        ("multi-layout-5.dxf", multi_layout(5)),
        ("r12.dxf", r12),
    ];
    for (name, bytes) in fixtures {
        fs::write(dir.join(name), bytes)?;
    }
    Ok(())
}
