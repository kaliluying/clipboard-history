use serde::Serialize;
use std::fmt::Display;

#[derive(Debug)]
pub struct AppError(anyhow::Error);

impl Serialize for AppError {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        // Serialize the error using its Display implementation, which retains the anyhow context format.
        serializer.serialize_str(&format!("{:#}", self.0))
    }
}

impl<E> From<E> for AppError
where
    E: Into<anyhow::Error>,
{
    fn from(err: E) -> Self {
        AppError(err.into())
    }
}

impl Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:#}", self.0)
    }
}

pub type Result<T> = std::result::Result<T, AppError>;
