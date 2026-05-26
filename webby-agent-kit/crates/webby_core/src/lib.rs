//! Shared primitives and structured errors used across Webby crates.

use std::path::PathBuf;

/// Convenient result alias for Webby operations.
pub type WebbyResult<T> = Result<T, WebbyError>;

/// Structured error type for the browser pipeline.
#[derive(Debug, thiserror::Error)]
pub enum WebbyError {
    /// The caller supplied input that cannot be handled.
    #[error("invalid input: {message}")]
    InvalidInput { message: String },
    /// URL parsing or resolution failed.
    #[error("url error: {message}")]
    Url { message: String },
    /// Network loading failed.
    #[error("network error: {message}")]
    Network { message: String },
    /// Local I/O failed.
    #[error("io error at {path:?}: {source}")]
    Io {
        path: Option<PathBuf>,
        #[source]
        source: std::io::Error,
    },
    /// HTML, CSS, or other text parsing failed.
    #[error("parse error: {message}")]
    Parse { message: String },
    /// Layout computation failed.
    #[error("layout error: {message}")]
    Layout { message: String },
    /// Rendering failed.
    #[error("render error: {message}")]
    Render { message: String },
    /// A future Webby feature was requested before implementation.
    #[error("unsupported operation: {message}")]
    Unsupported { message: String },
}

impl WebbyError {
    /// Creates an invalid-input error.
    pub fn invalid_input(message: impl Into<String>) -> Self {
        Self::InvalidInput {
            message: message.into(),
        }
    }

    /// Creates an unsupported-operation error.
    pub fn unsupported(message: impl Into<String>) -> Self {
        Self::Unsupported {
            message: message.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{WebbyError, WebbyResult};

    #[test]
    fn invalid_input_error_is_readable() {
        let error = WebbyError::invalid_input("address bar input was empty");

        assert_eq!(
            error.to_string(),
            "invalid input: address bar input was empty"
        );
    }

    #[test]
    fn result_alias_uses_webby_error() {
        fn fail() -> WebbyResult<()> {
            Err(WebbyError::unsupported("not wired yet"))
        }

        assert!(matches!(fail(), Err(WebbyError::Unsupported { .. })));
    }
}
