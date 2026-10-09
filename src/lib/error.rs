use serde::Serialize;
use std::{error::Error as StdError, fmt};

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Serialize)]
pub struct Error {
    pub ok: bool,
    pub code: &'static str,
    pub error: String,
    pub uncertain: bool,
}

impl Error {
    pub fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            ok: false,
            code,
            error: message.into(),
            uncertain: false,
        }
    }
    pub fn uncertain(mut self) -> Self {
        self.uncertain = true;
        self
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.error)
    }
}
impl StdError for Error {}
impl From<std::io::Error> for Error {
    fn from(value: std::io::Error) -> Self {
        Self::new("io", value.to_string())
    }
}
impl From<serde_json::Error> for Error {
    fn from(value: serde_json::Error) -> Self {
        Self::new("invalid-json", value.to_string())
    }
}
