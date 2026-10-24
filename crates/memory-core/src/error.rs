pub type Result<T> = std::result::Result<T, Error>;

/// Error categories shared by CLI and MCP so both report failures identically.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("invalid argument: {0}")]
    InvalidArgument(String),
    /// Also returned for cross-project reads, so existence is not leaked.
    #[error("not found: {0}")]
    NotFound(String),
    #[error("revision conflict: expected {expected}, current {current}")]
    Conflict { expected: u32, current: u32 },
    /// A job lease that is no longer the caller's: it expired and was reclaimed, or the job is
    /// already finished.
    #[error("stale lease: {0}")]
    StaleLease(String),
    /// An external provider (e.g. the local model server) cannot be reached or refused.
    #[error("provider unavailable: {0}")]
    Unavailable(String),
    #[error("storage error: {0}")]
    Storage(String),
    #[error("internal error: {0}")]
    Internal(String),
}

impl Error {
    /// Stable machine-readable code for `--json` output and MCP error objects.
    pub fn code(&self) -> &'static str {
        match self {
            Error::InvalidArgument(_) => "invalid_argument",
            Error::NotFound(_) => "not_found",
            Error::Conflict { .. } => "conflict",
            Error::StaleLease(_) => "stale_lease",
            Error::Unavailable(_) => "unavailable",
            Error::Storage(_) => "storage",
            Error::Internal(_) => "internal",
        }
    }
}
