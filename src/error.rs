use thiserror::Error;

/// Define a convenient Result type
pub type AppResult<T> = Result<T, AppError>;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("I/O Error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Protobuf decode error: {0}")]
    ProtobufDecode(#[from] prost::DecodeError),

    #[error("Log parsing error: {0}")]
    LogParsing(String),

    #[error("Analysis error: {0}")]
    Analysis(String),
}

impl From<anyhow::Error> for AppError {
    fn from(err: anyhow::Error) -> Self {
        AppError::Analysis(err.to_string())
    }
}