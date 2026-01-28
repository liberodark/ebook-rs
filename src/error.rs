use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use thiserror::Error;

/// Main error type for the application.
#[derive(Error, Debug)]
pub enum AppError {
    /// Resource not found error.
    #[error("Book not found: {0}")]
    NotFound(String),

    /// Invalid format error.
    #[error("Invalid format: {0}")]
    InvalidFormat(String),

    /// I/O error.
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// ZIP archive error.
    #[error("ZIP error: {0}")]
    Zip(#[from] zip::result::ZipError),

    /// PDF processing error.
    #[error("PDF error: {0}")]
    Pdf(String),

    /// XML parsing error.
    #[error("XML parsing error: {0}")]
    Xml(#[from] roxmltree::Error),

    /// Image processing error.
    #[error("Image processing error: {0}")]
    Image(#[from] image::ImageError),

    /// Configuration error.
    #[error("Configuration error: {0}")]
    Config(String),

    /// Internal server error.
    #[error("Internal error: {0}")]
    Internal(String),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let status = match &self {
            AppError::NotFound(_) => StatusCode::NOT_FOUND,
            AppError::InvalidFormat(_) => StatusCode::BAD_REQUEST,
            AppError::Config(_) => StatusCode::INTERNAL_SERVER_ERROR,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        };

        tracing::error!(error = %self, "Request error");

        (status, self.to_string()).into_response()
    }
}

/// Result type alias for the application.
pub type Result<T> = std::result::Result<T, AppError>;
