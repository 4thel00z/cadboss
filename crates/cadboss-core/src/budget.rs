use crate::error::{Error, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Budget {
    pub max_bytes: u64,
    pub max_entities: u64,
    pub max_layouts: u32,
    pub max_nesting: u32,
    pub max_string_bytes: u32,
}

impl Default for Budget {
    fn default() -> Self {
        Budget {
            max_bytes: 500 * 1024 * 1024,
            max_entities: 2_000_000,
            max_layouts: 200,
            max_nesting: 64,
            max_string_bytes: 1024 * 1024,
        }
    }
}

impl Budget {
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
