use cadboss_core::Budget;

use crate::failure::Failure;

pub fn parse(entries: &[String]) -> Result<Budget, Failure> {
    entries
        .iter()
        .try_fold(Budget::default(), |mut budget, entry| {
            apply(&mut budget, entry)?;
            Ok(budget)
        })
}

fn apply(budget: &mut Budget, entry: &str) -> Result<(), Failure> {
    let Some((key, value)) = entry.split_once('=') else {
        return Err(Failure::usage(format!(
            "invalid --budget {entry}: expected KEY=VALUE"
        )));
    };
    let number: u64 = value.trim().parse().map_err(|_| {
        Failure::usage(format!(
            "invalid --budget {entry}: value must be an integer"
        ))
    })?;
    budget.set(key, number).map_err(|error| {
        Failure::usage(format!(
            "invalid --budget {entry}: {error} (keys: {})",
            Budget::KEYS.join(", ")
        ))
    })
}
