use crate::error::{Error, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Budget {
    pub max_bytes: u64,
    pub max_entities: u64,
    /// Cap on paper layouts; the model layout is always kept and not counted.
    pub max_layouts: u32,
    /// Number of insert levels, the model-space level counted as level 0, so the deepest depth value a walk reports is `max_nesting - 1`.
    pub max_nesting: u32,
    pub max_string_bytes: u32,
    pub max_tokens: u64,
}

impl Default for Budget {
    fn default() -> Self {
        Budget {
            max_bytes: 500 * 1024 * 1024,
            max_entities: 2_000_000,
            max_layouts: 200,
            max_nesting: 64,
            max_string_bytes: 1024 * 1024,
            max_tokens: 64_000_000,
        }
    }
}

impl Budget {
    /// Names accepted by `set`.
    pub const KEYS: [&'static str; 6] = [
        "max_bytes",
        "max_entities",
        "max_layouts",
        "max_nesting",
        "max_string_bytes",
        "max_tokens",
    ];

    /// Sets the budget named `key`; fails for a name outside `KEYS` and for a value a 32-bit budget cannot hold.
    pub fn set(&mut self, key: &str, value: u64) -> std::result::Result<(), &'static str> {
        let narrow = || u32::try_from(value).map_err(|_| "value too large");
        match key {
            "max_bytes" => self.max_bytes = value,
            "max_entities" => self.max_entities = value,
            "max_tokens" => self.max_tokens = value,
            "max_layouts" => self.max_layouts = narrow()?,
            "max_nesting" => self.max_nesting = narrow()?,
            "max_string_bytes" => self.max_string_bytes = narrow()?,
            _ => return Err("unknown budget key"),
        }
        Ok(())
    }

    pub fn check_bytes(&self, len: u64) -> Result<()> {
        if len > self.max_bytes {
            return Err(Error::BudgetExceeded {
                budget: "max_bytes",
                limit: self.max_bytes,
            });
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_budget_matches_the_documented_values() {
        let budget = Budget::default();
        assert_eq!(budget.max_bytes, 500 * 1024 * 1024);
        assert_eq!(budget.max_entities, 2_000_000);
        assert_eq!(budget.max_layouts, 200);
        assert_eq!(budget.max_nesting, 64);
        assert_eq!(budget.max_string_bytes, 1024 * 1024);
        assert_eq!(budget.max_tokens, 64_000_000);
    }

    #[test]
    fn set_writes_every_key_and_rejects_the_rest() {
        let mut budget = Budget::default();
        for (value, key) in (1..).zip(Budget::KEYS) {
            assert_eq!(budget.set(key, value), Ok(()));
        }
        assert_eq!(
            budget,
            Budget {
                max_bytes: 1,
                max_entities: 2,
                max_layouts: 3,
                max_nesting: 4,
                max_string_bytes: 5,
                max_tokens: 6,
            }
        );
        assert_eq!(budget.set("max_nothing", 1), Err("unknown budget key"));
        assert_eq!(budget.set("max_layouts", u64::MAX), Err("value too large"));
        assert_eq!(budget.max_layouts, 3);
    }

    #[test]
    fn check_bytes_returns_budget_exceeded_past_max_bytes() {
        let budget = Budget::default();
        assert!(budget.check_bytes(budget.max_bytes).is_ok());
        assert!(matches!(
            budget.check_bytes(budget.max_bytes + 1),
            Err(crate::Error::BudgetExceeded {
                budget: "max_bytes",
                ..
            })
        ));
    }
}
