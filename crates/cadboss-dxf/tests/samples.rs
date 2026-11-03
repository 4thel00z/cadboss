use std::error::Error;
use std::path::{Path, PathBuf};

use cadboss_dxf::open;
use toml::Table;

fn samples_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/samples")
}

fn manifest() -> Result<Table, Box<dyn Error>> {
    let text = std::fs::read_to_string(samples_dir().join("manifest.toml"))?;
    Ok(text.parse::<Table>()?)
}

#[test]
fn every_downloaded_sample_opens_with_its_declared_version() -> Result<(), Box<dyn Error>> {
    let manifest = manifest()?;
    let entries = manifest
        .get("sample")
        .and_then(|value| value.as_array())
        .cloned()
        .unwrap_or_default();
    for entry in &entries {
        let name = entry
            .get("name")
            .and_then(|value| value.as_str())
            .ok_or("sample without a name")?;
        let path = samples_dir().join(name);
        if !path.exists() {
            println!("skipping {name}: not downloaded");
            continue;
        }
        let document = open(&path).map_err(|error| format!("{name}: {error}"))?;
        let version = entry
            .get("version")
            .and_then(|value| value.as_str())
            .ok_or("sample without a version")?;
        assert_eq!(
            document.metadata.version.as_str(),
            Some(version),
            "{name}: version"
        );
        let has_text = entry
            .get("has_text")
            .and_then(|value| value.as_bool())
            .unwrap_or(false);
        let placed = document.text.annotations.len() + document.text.attributes.len();
        assert!(
            !has_text || placed > 0,
            "{name}: no annotation or attribute found"
        );
        let leaked = document
            .text
            .iter()
            .filter_map(|item| item.text.as_str())
            .find(|text| text.contains("\\N") || text.contains("\\P"));
        assert_eq!(leaked, None, "{name}: MTEXT break code left in text");
        println!(
            "{name}: {} text items, {placed} annotations and attributes, {} skipped",
            document.text.len(),
            document.report.skipped.len()
        );
    }
    Ok(())
}
