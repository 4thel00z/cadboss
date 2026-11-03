#[test]
fn workspace_denies_every_panic_path_in_lib_code() {
    let manifest =
        std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../Cargo.toml")).unwrap();
    for lint in [
        "unwrap_used",
        "expect_used",
        "panic",
        "unreachable",
        "todo",
        "unimplemented",
    ] {
        assert!(
            manifest.contains(&format!("{lint} = \"deny\"")),
            "{lint} missing"
        );
    }
    let clippy =
        std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../clippy.toml")).unwrap();
    assert!(clippy.contains("allow-unwrap-in-tests = true"));
}
