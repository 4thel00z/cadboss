use std::fs;
use std::path::PathBuf;

use cadboss_testkit::dxf::DxfBuilder;

fn main() -> std::io::Result<()> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets/demo.dxf");
    fs::write(&path, demo_drawing())?;
    println!("wrote {}", path.display());
    Ok(())
}

fn demo_drawing() -> Vec<u8> {
    DxfBuilder::new()
        .version("AC1032")
        .layer("WALLS", 7)
        .layer("EQUIPMENT", 3)
        .layer("PIPING", 5)
        .layer("DIM", 1)
        .layer("NOTES", 2)
        .layer("TITLE", 7)
        .block("PUMP", (0.0, 0.0))
        .circle((0.0, 0.0), 300.0)
        .line((-300.0, 0.0), (300.0, 0.0))
        .text("P", (-80.0, -100.0), 200.0)
        .end()
        .block("TITLEBLOCK", (0.0, 0.0))
        .lwpolyline(
            &[(0.0, 0.0, 0.0), (180.0, 0.0, 0.0), (180.0, 60.0, 0.0), (0.0, 60.0, 0.0)],
            true,
        )
        .line((0.0, 20.0), (180.0, 20.0))
        .line((0.0, 40.0), (180.0, 40.0))
        .line((90.0, 0.0), (90.0, 40.0))
        .text("DRAWN", (4.0, 28.0), 3.0)
        .text("CHECKED", (94.0, 28.0), 3.0)
        .text("SCALE", (4.0, 8.0), 3.0)
        .text("REV", (94.0, 8.0), 3.0)
        .end()
        .xref_block("SITE-PLAN", "../xrefs/site-plan.dxf")
        .on_layer("WALLS")
        .lwpolyline(
            &[
                (0.0, 0.0, 0.0),
                (12000.0, 0.0, 0.0),
                (12000.0, 8000.0, 0.0),
                (0.0, 8000.0, 0.0),
            ],
            true,
        )
        .line((6000.0, 0.0), (6000.0, 3000.0))
        .on_layer("EQUIPMENT")
        .insert_with_attribs("PUMP", (3000.0, 4000.0), &[("TAG", "P-101"), ("DUTY", "DUTY")])
        .insert_with_attribs("PUMP", (9000.0, 4000.0), &[("TAG", "P-102"), ("DUTY", "STANDBY")])
        .on_layer("PIPING")
        .line((3300.0, 4000.0), (8700.0, 4000.0))
        .arc((6000.0, 5500.0), 1500.0, 0.0, 180.0)
        .line((3000.0, 4300.0), (3000.0, 7500.0))
        .hyperlink("https://example.com/specs/pipe-class-1A.pdf")
        .on_layer("DIM")
        .dimension_at("<>", 12000.0, (6000.0, -800.0))
        .dimension_at("<>", 6000.0, (6000.0, 4800.0))
        .dimension_at("{\\H0.7x;<>\\S+0.1^-0.2;}", 300.0, (3000.0, 4400.0))
        .on_layer("NOTES")
        .mtext(
            "NOTES:\\P1. ALL DIMENSIONS IN MILLIMETRES.\\P2. PIPING TO PIPE CLASS 1A.\\P3. P-102 IS THE STANDBY UNIT, SEE DATASHEET \\U+00A7 4.",
            (200.0, 7600.0),
            150.0,
        )
        .text("PUMP ROOM", (5000.0, 7000.0), 250.0)
        .insert("SITE-PLAN", (0.0, 0.0))
        .image("photos/pump-room.png", (12500.0, 0.0), (3000.0, 2000.0))
        .paper_layout("Sheet 1")
        .on_layer("TITLE")
        .insert_with_attribs(
            "TITLEBLOCK",
            (230.0, 10.0),
            &[
                ("TITLE", "PUMP STATION P-101 GENERAL ARRANGEMENT"),
                ("DWG_NO", "PS-101-M-0001"),
                ("REV", "C"),
                ("SCALE", "1:50"),
                ("DRAWN", "M. TAHRIOUI"),
                ("DATE", "2026-11-05"),
                ("CLIENT", "NORDSTADT WATERWORKS"),
            ],
        )
        .end()
        .build()
}
