use std::path::{Path, PathBuf};
use std::process::{Command, Output};

type TestResult = Result<(), Box<dyn std::error::Error>>;

struct TempFile(PathBuf);

impl Drop for TempFile {
    fn drop(&mut self) {
        std::fs::remove_file(&self.0).ok();
    }
}

fn fixture(name: &str) -> String {
    format!("{}/../../tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"))
}

fn temp_file(name: &str, bytes: &[u8]) -> Result<TempFile, std::io::Error> {
    let path = std::env::temp_dir().join(format!("cadboss-cli-{}-{name}", std::process::id()));
    std::fs::write(&path, bytes)?;
    Ok(TempFile(path))
}

fn cadboss(args: &[&str]) -> Result<Output, std::io::Error> {
    Command::new(env!("CARGO_BIN_EXE_cadboss"))
        .args(args)
        .env("NO_COLOR", "1")
        .output()
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn path_text(path: &Path) -> String {
    path.display().to_string()
}

#[test]
fn info_prints_format_and_counts() -> TestResult {
    let output = cadboss(&["info", &fixture("simple.dxf")])?;
    assert_eq!(output.status.code(), Some(0));
    let text = stdout(&output);
    assert!(text.starts_with("Format: DXF AC1027\nUnits: Millimeters\nLayers: 2\n"));
    assert!(text.contains("\nLayouts: 2 (Model, Layout1)\nEntities: "));
    assert!(text.contains("\nExtents: ("));
    Ok(())
}

#[test]
fn text_lists_the_title_block_item() -> TestResult {
    let output = cadboss(&["text", &fixture("simple.dxf")])?;
    assert_eq!(output.status.code(), Some(0));
    let text = stdout(&output);
    assert!(text.contains("# title-block\nTITLE: PUMP P-101\n"));
    let single = cadboss(&[
        "text",
        &fixture("simple.dxf"),
        "--group",
        "title-block",
        "--layout",
        "Layout1",
    ])?;
    assert_eq!(stdout(&single), "TITLE: PUMP P-101\n");
    Ok(())
}

#[test]
fn md_output_equals_the_output_crate() -> TestResult {
    let path = fixture("simple.dxf");
    let output = cadboss(&["md", &path])?;
    let document = cadboss_dxf::open(Path::new(&path))?;
    assert_eq!(
        stdout(&output),
        cadboss_output::to_markdown(&document, "simple")
    );
    let titled = cadboss(&["md", &path, "--title", "Pump"])?;
    assert!(stdout(&titled).starts_with("# Pump\n"));
    Ok(())
}

#[test]
fn json_is_valid_and_has_metadata_first() -> TestResult {
    let output = cadboss(&["json", &fixture("simple.dxf"), "--entities"])?;
    let value: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    let object = value.as_object().ok_or("not an object")?;
    assert_eq!(object.keys().next().map(String::as_str), Some("metadata"));
    let entities = object
        .get("entities")
        .and_then(|entities| entities.get("Model"))
        .and_then(serde_json::Value::as_array)
        .ok_or("no model entities")?;
    assert!(!entities.is_empty());
    let compact = cadboss(&["json", &fixture("simple.dxf"), "--compact"])?;
    assert_eq!(stdout(&compact).lines().count(), 1);
    Ok(())
}

#[test]
fn q_extracts_the_version_with_a_jq_program() -> TestResult {
    let output = cadboss(&["q", &fixture("simple.dxf"), ".metadata.version"])?;
    assert_eq!(stdout(&output), "\"AC1027\"\n");
    let raw = cadboss(&["q", &fixture("simple.dxf"), ".metadata.version", "-r"])?;
    assert_eq!(stdout(&raw), "AC1027\n");
    let broken = cadboss(&["q", &fixture("simple.dxf"), ".metadata |"])?;
    assert_eq!(broken.status.code(), Some(2));
    Ok(())
}

#[test]
fn obj_dumps_the_pairs_of_a_handle() -> TestResult {
    let output = cadboss(&["obj", &fixture("simple.dxf"), "0x112"])?;
    assert_eq!(output.status.code(), Some(0));
    assert!(stdout(&output).starts_with("0\tLINE\n5\t112\n"));
    let layer = cadboss(&["obj", &fixture("simple.dxf"), "100"])?;
    assert_eq!(layer.status.code(), Some(0));
    assert!(stdout(&layer).starts_with("0\t"));
    let missing = cadboss(&["obj", &fixture("simple.dxf"), "ffff"])?;
    assert_eq!(missing.status.code(), Some(1));
    assert!(stderr(&missing).contains("handle not found"));
    Ok(())
}

#[test]
fn png_input_exits_1_with_not_dxf() -> TestResult {
    let path = temp_file("image.png", b"\x89PNG\r\n\x1a\nrest")?;
    let output = cadboss(&["info", &path_text(&path.0)])?;
    assert_eq!(output.status.code(), Some(1));
    assert!(stderr(&output).contains("not a dxf file"));
    Ok(())
}

#[test]
fn empty_file_exits_1_with_empty() -> TestResult {
    let path = temp_file("empty.dxf", b"")?;
    let output = cadboss(&["info", &path_text(&path.0)])?;
    assert_eq!(output.status.code(), Some(1));
    assert!(stderr(&output).contains("empty input"));
    Ok(())
}

#[test]
fn nested_70_exits_0_and_warns_about_the_nesting_limit() -> TestResult {
    let output = cadboss(&["text", &fixture("nested-70.dxf")])?;
    assert_eq!(output.status.code(), Some(0));
    assert!(stderr(&output).contains("nesting_limit"));
    Ok(())
}

#[test]
fn budget_flag_caps_entities() -> TestResult {
    let path = temp_file("large.dxf", &cadboss_testkit::dxf::large_drawing(10))?;
    let output = cadboss(&["info", &path_text(&path.0), "--budget", "max_entities=3"])?;
    assert!(stdout(&output).contains("Entities: 3"));
    assert!(stderr(&output).contains("entity_limit"));
    let bad = cadboss(&["info", &path_text(&path.0), "--budget", "bogus=3"])?;
    assert_eq!(bad.status.code(), Some(2));
    assert!(stderr(&bad)
        .starts_with("cadboss: invalid --budget bogus=3: unknown budget key (keys: max_bytes,"));
    let large = cadboss(&[
        "info",
        &path_text(&path.0),
        "--budget",
        "max_nesting=4294967296",
    ])?;
    assert_eq!(large.status.code(), Some(2));
    assert!(stderr(&large).contains("value too large"));
    Ok(())
}

#[test]
fn layouts_and_layers_list_one_per_line() -> TestResult {
    let layouts = cadboss(&["layouts", &fixture("simple.dxf")])?;
    assert!(stdout(&layouts).starts_with("Model\tModel\t"));
    let layers = cadboss(&["layers", &fixture("simple.dxf")])?;
    assert!(stdout(&layers)
        .lines()
        .all(|line| line.split('\t').count() == 3));
    Ok(())
}

#[test]
fn skill_show_prints_front_matter() -> TestResult {
    let output = cadboss(&["skill", "show"])?;
    assert!(stdout(&output).starts_with("---\nname: cadboss\n"));
    Ok(())
}

#[test]
fn skill_copies_are_identical() -> TestResult {
    let packaged = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/skill/SKILL.md"))?;
    let published = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../skills/cadboss/SKILL.md"
    ))?;
    assert_eq!(packaged, published);
    assert!(!packaged.contains('\u{2014}'));
    Ok(())
}

#[test]
fn text_with_a_layout_hides_names_and_properties() -> TestResult {
    let path = fixture("simple.dxf");
    let everything = stdout(&cadboss(&["text", &path])?);
    assert!(everything.contains("# names"));
    let scoped = stdout(&cadboss(&["text", &path, "--layout", "Model"])?);
    assert!(scoped.contains("Hello"));
    assert!(!scoped.contains("# names"));
    assert!(!scoped.contains("# properties"));
    let names = cadboss(&["text", &path, "--layout", "Model", "--group", "names"])?;
    assert_eq!(stdout(&names), "");
    Ok(())
}

#[test]
fn obj_prints_typed_values_for_binary_dxf() -> TestResult {
    let output = cadboss(&["obj", &fixture("simple-binary.dxf"), "110"])?;
    assert_eq!(output.status.code(), Some(0));
    assert!(stdout(&output).contains("\n10\t0\n"));
    Ok(())
}

#[test]
fn q_reports_readable_compile_and_runtime_errors() -> TestResult {
    let path = fixture("simple.dxf");
    let broken = cadboss(&["q", &path, ".metadata |"])?;
    assert_eq!(broken.status.code(), Some(2));
    let message = stderr(&broken);
    assert!(message.starts_with("cadboss: invalid jq program:"));
    assert!(!message.contains("File {"));
    let undefined = cadboss(&["q", &path, "nope"])?;
    assert_eq!(undefined.status.code(), Some(2));
    let runtime = cadboss(&["q", &path, ".metadata.version | error"])?;
    assert_eq!(runtime.status.code(), Some(1));
    let raw = cadboss(&["q", &path, ".metadata.layer_count", "-r"])?;
    assert_eq!(stdout(&raw), "2\n");
    Ok(())
}

#[test]
fn json_entities_stream_the_same_document_as_q() -> TestResult {
    let path = fixture("multi-layout-5.dxf");
    let streamed = cadboss(&["json", &path, "--entities", "--compact"])?;
    let value: serde_json::Value = serde_json::from_slice(&streamed.stdout)?;
    let entities = value
        .get("entities")
        .and_then(serde_json::Value::as_object)
        .ok_or("no entities")?;
    let names: Vec<&str> = entities.keys().map(String::as_str).collect();
    assert_eq!(names.first(), Some(&"Model"));
    assert_eq!(names.len(), 7);
    let queried = cadboss(&["q", &path, "--entities", ".entities | keys | length"])?;
    assert_eq!(stdout(&queried), "7\n");
    let scoped = cadboss(&["json", &path, "--entities", "--layout", "Sheet2"])?;
    let value: serde_json::Value = serde_json::from_slice(&scoped.stdout)?;
    let only = value
        .get("entities")
        .and_then(serde_json::Value::as_object)
        .ok_or("no entities")?;
    assert_eq!(only.keys().collect::<Vec<_>>(), ["Sheet2"]);
    Ok(())
}

#[test]
fn text_prints_each_item_on_one_line() -> TestResult {
    let bytes = cadboss_testkit::dxf::DxfBuilder::new()
        .mtext("first\\Psecond", (0.0, 0.0), 1.0)
        .build();
    let path = temp_file("lines.dxf", &bytes)?;
    let output = cadboss(&["text", &path_text(&path.0), "--group", "annotations"])?;
    assert_eq!(stdout(&output), "first second\n");
    Ok(())
}

#[test]
fn an_unknown_layout_exits_2_and_lists_the_layouts() -> TestResult {
    let path = fixture("simple.dxf");
    for command in ["text", "json", "q"] {
        let mut args = vec![command, path.as_str(), "--layout", "Nope"];
        if command == "q" {
            args.push(".");
        }
        let output = cadboss(&args)?;
        assert_eq!(output.status.code(), Some(2), "{command}");
        assert_eq!(
            stderr(&output),
            "cadboss: unknown layout: Nope (available: Model, Layout1)\n",
            "{command}"
        );
    }
    Ok(())
}
