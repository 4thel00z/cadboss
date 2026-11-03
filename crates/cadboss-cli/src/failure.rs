use cadboss_core::Error;

#[derive(Debug)]
pub struct Failure {
    pub message: String,
    pub code: i32,
}

impl Failure {
    pub fn input(message: impl Into<String>) -> Failure {
        Failure {
            message: message.into(),
            code: 1,
        }
    }

    pub fn usage(message: impl Into<String>) -> Failure {
        Failure {
            message: message.into(),
            code: 2,
        }
    }
}

impl From<Error> for Failure {
    fn from(error: Error) -> Failure {
        Failure::input(error.to_string())
    }
}

impl From<std::io::Error> for Failure {
    fn from(error: std::io::Error) -> Failure {
        Failure::input(error.to_string())
    }
}

impl From<serde_json::Error> for Failure {
    fn from(error: serde_json::Error) -> Failure {
        Failure::input(error.to_string())
    }
}
