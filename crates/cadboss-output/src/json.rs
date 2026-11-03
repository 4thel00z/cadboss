use cadboss_model::Document;

/// Pretty JSON with two-space indent; serializing the model cannot fail, so an empty string is never returned in practice.
pub fn to_json(doc: &Document) -> String {
    serde_json::to_string_pretty(doc).unwrap_or_default()
}

/// Single-line JSON; serializing the model cannot fail, so an empty string is never returned in practice.
pub fn to_json_compact(doc: &Document) -> String {
    serde_json::to_string(doc).unwrap_or_default()
}
