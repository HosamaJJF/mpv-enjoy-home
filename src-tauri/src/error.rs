use std::fmt::{Display, Formatter};

#[derive(Debug)]
pub struct AppError(pub String);

impl AppError {
    pub fn message(message: impl Into<String>) -> Self {
        Self(message.into())
    }
}

impl Display for AppError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for AppError {}

impl From<std::io::Error> for AppError {
    fn from(error: std::io::Error) -> Self {
        Self(error.to_string())
    }
}

impl From<rusqlite::Error> for AppError {
    fn from(error: rusqlite::Error) -> Self {
        Self(error.to_string())
    }
}

pub type AppResult<T> = Result<T, AppError>;
