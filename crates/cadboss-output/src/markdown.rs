use std::collections::HashMap;

use cadboss_core::BBox;
use cadboss_model::{format_number, Document, Layout, Text, TextItem};

pub fn to_markdown(doc: &Document, title: &str) -> String {
    let mut blocks = vec![format!("# {title}"), metadata(doc)];
    blocks.extend(
        doc.layouts
            .iter()
            .flat_map(|layout| layout_blocks(doc, layout)),
    );
    blocks.extend(entities(doc));
    blocks.extend(optional_section("Names", &doc.text.names));
    blocks.extend(optional_section("Attributes", &doc.text.attributes));
    blocks.extend(optional_section("References", &doc.text.references));
    blocks.extend(report(doc));
    let mut out = blocks.join("\n\n");
    out.push('\n');
    out
}

fn metadata(doc: &Document) -> String {
    let meta = &doc.metadata;
    let mut lines = vec![
        "## Metadata".to_owned(),
        String::new(),
        format!("- Format: {} {}", meta.format.label(), meta.version),
        format!("- Units: {}", meta.units.label()),
        format!("- Layers: {}", meta.layer_count),
        format!("- Blocks: {}", meta.block_count),
        format!("- Layouts: {}", meta.layout_count),
    ];
    lines.extend(
        meta.extents
            .map(|extents| format!("- Extents: {}", extents_text(&extents))),
    );
    lines.extend(meta.created.map(|jd| format!("- Created: {}", date(jd))));
    lines.extend(meta.updated.map(|jd| format!("- Updated: {}", date(jd))));
    lines.extend(
        meta.author
            .as_ref()
            .map(|author| format!("- Author: {}", one_line(&author.to_string()))),
    );
    lines.join("\n")
}

fn extents_text(extents: &BBox) -> String {
    format!(
        "({}, {}) to ({}, {})",
        format_number(extents.min.x),
        format_number(extents.min.y),
        format_number(extents.max.x),
        format_number(extents.max.y)
    )
}

fn layout_blocks(doc: &Document, layout: &Layout) -> Vec<String> {
    let in_layout = |item: &&TextItem| item.layout == layout.name;
    let title_block: Vec<&TextItem> = doc.text.title_block.iter().filter(in_layout).collect();
    let annotations: Vec<&TextItem> = doc.text.annotations.iter().filter(in_layout).collect();
    let mut blocks = vec![format!("## Layout: {}", one_line(&layout.name.to_string()))];
    if !title_block.is_empty() {
        blocks.push(bullets(
            "### Title block",
            title_block
                .into_iter()
                .map(|item| one_line(&item.text.to_string())),
        ));
    }
    if annotations.is_empty() {
        return blocks;
    }
    blocks.push("### Text by layer".to_owned());
    blocks.extend(layer_groups(&annotations));
    blocks
}

fn layer_groups(annotations: &[&TextItem]) -> Vec<String> {
    let mut positions: HashMap<&Text, usize> = HashMap::new();
    let mut layers: Vec<Vec<&TextItem>> = Vec::new();
    for item in annotations {
        let position = *positions.entry(&item.layer).or_insert(layers.len());
        let Some(layer) = layers.get_mut(position) else {
            layers.push(vec![*item]);
            continue;
        };
        layer.push(item);
    }
    layers
        .iter()
        .filter_map(|items| {
            let first = items.first()?;
            let heading = format!("#### Layer {}", one_line(&first.layer.to_string()));
            Some(bullets(&heading, items.iter().copied().map(annotation)))
        })
        .collect()
}

fn annotation(item: &TextItem) -> String {
    let text = one_line(&item.text.to_string());
    match item.height {
        Some(height) => format!("{text} (h {})", format_number(height)),
        None => text,
    }
}

fn entities(doc: &Document) -> Option<String> {
    if doc.metadata.entity_counts.is_empty() {
        return None;
    }
    let mut lines = vec![
        "## Entities".to_owned(),
        String::new(),
        "| Type | Count |".to_owned(),
        "| --- | --- |".to_owned(),
    ];
    lines.extend(
        doc.metadata
            .entity_counts
            .iter()
            .map(|(name, count)| format!("| {} | {count} |", cell(name))),
    );
    Some(lines.join("\n"))
}

fn section(heading: &str, items: &[TextItem]) -> String {
    bullets(
        &format!("## {heading}"),
        items.iter().map(|item| one_line(&item.text.to_string())),
    )
}

fn optional_section(heading: &str, items: &[TextItem]) -> Option<String> {
    if items.is_empty() {
        return None;
    }
    Some(section(heading, items))
}

fn report(doc: &Document) -> Option<String> {
    if doc.report.is_empty() {
        return None;
    }
    Some(bullets("## Report", doc.report.lines().into_iter()))
}

fn bullets(heading: &str, items: impl Iterator<Item = String>) -> String {
    let mut lines = vec![heading.to_owned(), String::new()];
    lines.extend(items.map(|item| format!("- {item}")));
    lines.join("\n")
}

/// The text with every line break replaced by a space.
pub fn one_line(text: &str) -> String {
    text.replace("\r\n", " ").replace(['\n', '\r'], " ")
}

fn cell(text: &str) -> String {
    one_line(text).replace('|', "\\|")
}

const MAX_DATE_DAYS: f64 = 1.0e8;

fn date(julian_day: f64) -> String {
    let offset = julian_day - 2_440_587.5;
    if !offset.is_finite() || offset.abs() > MAX_DATE_DAYS {
        return format!("JD {julian_day}");
    }
    let days = offset.floor() as i64;
    let (year, month, day) = civil_from_days(days);
    format!("{year:04}-{month:02}-{day:02}")
}

fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let shifted = days + 719_468;
    let era = shifted.div_euclid(146_097);
    let day_of_era = shifted.rem_euclid(146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_index = (5 * day_of_year + 2) / 153;
    let day = (day_of_year - (153 * month_index + 2) / 5 + 1) as u32;
    let month = ((month_index + 2) % 12 + 1) as u32;
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn julian_day_2440587_5_is_the_unix_epoch() {
        assert_eq!(date(2_440_587.5), "1970-01-01");
    }

    #[test]
    fn julian_day_2460371_5_is_the_second_of_march_2024() {
        assert_eq!(date(2_460_371.5), "2024-03-02");
    }

    #[test]
    fn out_of_range_julian_days_are_printed_as_numbers() {
        assert_eq!(date(f64::MAX), format!("JD {}", f64::MAX));
        assert_eq!(date(f64::NAN), "JD NaN");
        assert_eq!(date(-1.0e300), format!("JD {}", -1.0e300));
    }
}
