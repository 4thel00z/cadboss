pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("empty input")]
    Empty,
    #[error("not a dxf file")]
    NotDxf,
    #[error("{budget} budget exceeded (limit {limit})")]
    BudgetExceeded { budget: &'static str, limit: u64 },
    #[error("syntax error at byte {offset}: {msg}")]
    Syntax { offset: usize, msg: String },
    #[error("unsupported: {0}")]
    Unsupported(&'static str),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_messages_state_their_fields() {
        let exceeded = Error::BudgetExceeded {
            budget: "max_bytes",
            limit: 7,
        };
        assert_eq!(exceeded.to_string(), "max_bytes budget exceeded (limit 7)");
        let syntax = Error::Syntax {
            offset: 12,
            msg: "bad group code".to_string(),
        };
        assert_eq!(
            syntax.to_string(),
            "syntax error at byte 12: bad group code"
        );
        assert_eq!(Error::Empty.to_string(), "empty input");
        assert_eq!(Error::NotDxf.to_string(), "not a dxf file");
        assert_eq!(Error::Unsupported("dwg").to_string(), "unsupported: dwg");
    }

    #[test]
    fn io_errors_convert_with_question_mark() {
        fn open() -> Result<()> {
            std::fs::read("/nonexistent/cadboss/path")?;
            Ok(())
        }
        assert!(matches!(open(), Err(Error::Io(_))));
    }
}
