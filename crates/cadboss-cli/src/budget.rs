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
    match key {
        "max_bytes" => budget.max_bytes = number,
        "max_entities" => budget.max_entities = number,
        "max_tokens" => budget.max_tokens = number,
        "max_layouts" => budget.max_layouts = narrow(entry, number)?,
        "max_nesting" => budget.max_nesting = narrow(entry, number)?,
        "max_string_bytes" => budget.max_string_bytes = narrow(entry, number)?,
        other => {
            return Err(Failure::usage(format!(
                "unknown budget key {other}: expected max_bytes, max_entities, max_layouts, max_nesting, max_string_bytes or max_tokens"
            )))
        }
    }
    Ok(())
}

fn narrow(entry: &str, number: u64) -> Result<u32, Failure> {
    u32::try_from(number)
        .map_err(|_| Failure::usage(format!("invalid --budget {entry}: value too large")))
}
