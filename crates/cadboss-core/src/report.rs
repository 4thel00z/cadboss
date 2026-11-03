use serde::Serialize;

pub const MAX_SKIPPED: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SkipKind {
    Token,
    Section,
    Table,
    Entity,
    Text,
    Object,
}

impl SkipKind {
    pub fn label(self) -> &'static str {
        match self {
            SkipKind::Token => "token",
            SkipKind::Section => "section",
            SkipKind::Table => "table",
            SkipKind::Entity => "entity",
            SkipKind::Text => "text",
            SkipKind::Object => "object",
        }
    }

    fn plural_label(self) -> &'static str {
        match self {
            SkipKind::Token => "tokens",
            SkipKind::Section => "sections",
            SkipKind::Table => "tables",
            SkipKind::Entity => "entities",
            SkipKind::Text => "texts",
            SkipKind::Object => "objects",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SkipReason {
    Malformed,
    Truncated,
    Unknown,
    NestingLimit,
    EntityLimit,
    StringLimit,
    UnsupportedCodepage,
    Unsupported,
    TokenLimit,
}

impl SkipReason {
    pub fn label(self) -> &'static str {
        match self {
            SkipReason::Malformed => "malformed",
            SkipReason::Truncated => "truncated",
            SkipReason::Unknown => "unknown",
            SkipReason::NestingLimit => "nesting_limit",
            SkipReason::EntityLimit => "entity_limit",
            SkipReason::StringLimit => "string_limit",
            SkipReason::UnsupportedCodepage => "unsupported_codepage",
            SkipReason::Unsupported => "unsupported",
            SkipReason::TokenLimit => "token_limit",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Skipped {
    pub kind: SkipKind,
    pub reason: SkipReason,
    pub count: u64,
}

impl Skipped {
    fn describe(&self) -> String {
        let noun = if self.count == 1 {
            self.kind.label()
        } else {
            self.kind.plural_label()
        };
        format!("{} {} skipped ({})", self.count, noun, self.reason.label())
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Report {
    pub skipped: Vec<Skipped>,
    pub unlisted: u64,
}

impl Report {
    pub fn note(&mut self, kind: SkipKind, reason: SkipReason) {
        self.add(kind, reason, 1);
    }

    pub fn merge(&mut self, other: Report) {
        for entry in other.skipped {
            self.add(entry.kind, entry.reason, entry.count);
        }
        self.unlisted = self.unlisted.saturating_add(other.unlisted);
    }

    pub fn is_empty(&self) -> bool {
        self.skipped.is_empty() && self.unlisted == 0
    }

    pub fn summary(&self) -> String {
        let mut parts: Vec<String> = self.skipped.iter().map(Skipped::describe).collect();
        if self.unlisted > 0 {
            parts.push(format!("{} more skipped (unlisted)", self.unlisted));
        }
        parts.join(", ")
    }

    fn add(&mut self, kind: SkipKind, reason: SkipReason, count: u64) {
        if let Some(entry) = self
            .skipped
            .iter_mut()
            .find(|entry| entry.kind == kind && entry.reason == reason)
        {
            entry.count = entry.count.saturating_add(count);
            return;
        }
        if self.skipped.len() >= MAX_SKIPPED {
            self.unlisted = self.unlisted.saturating_add(count);
            return;
        }
        self.skipped.push(Skipped {
            kind,
            reason,
            count,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_deduplicates_by_kind_and_reason() {
        let mut r = Report::default();
        r.note(SkipKind::Entity, SkipReason::Malformed);
        r.note(SkipKind::Entity, SkipReason::Malformed);
        assert_eq!(
            r.skipped,
            vec![Skipped {
                kind: SkipKind::Entity,
                reason: SkipReason::Malformed,
                count: 2
            }]
        );
    }

    #[test]
    fn report_caps_distinct_entries_and_counts_overflow() {
        let full = Skipped {
            kind: SkipKind::Object,
            reason: SkipReason::Unknown,
            count: 1,
        };
        let mut r = Report {
            skipped: vec![full; MAX_SKIPPED],
            unlisted: 0,
        };
        r.note(SkipKind::Entity, SkipReason::Malformed);
        assert_eq!(r.skipped.len(), MAX_SKIPPED);
        assert_eq!(r.unlisted, 1);
    }

    #[test]
    fn report_summary_names_counts_kinds_and_reasons() {
        let mut r = Report::default();
        for _ in 0..3 {
            r.note(SkipKind::Entity, SkipReason::Malformed);
        }
        r.note(SkipKind::Section, SkipReason::Truncated);
        assert_eq!(
            r.summary(),
            "3 entities skipped (malformed), 1 section skipped (truncated)"
        );
    }

    #[test]
    fn report_merge_adds_counts_and_unlisted() {
        let mut a = Report::default();
        a.note(SkipKind::Text, SkipReason::UnsupportedCodepage);
        let mut b = Report::default();
        b.note(SkipKind::Text, SkipReason::UnsupportedCodepage);
        b.note(SkipKind::Token, SkipReason::Truncated);
        b.unlisted = 2;
        a.merge(b);
        assert_eq!(a.skipped.len(), 2);
        assert_eq!(a.skipped[0].count, 2);
        assert_eq!(a.unlisted, 2);
        assert!(!a.is_empty());
        assert!(Report::default().is_empty());
    }
}
