//! Error types for the engine.
//!
//! The engine distinguishes three kinds of failure:
//!
//! * [`FetchError`] — something went wrong talking to a single URL. These are
//!   *expected*: they get recorded against the URL row and the crawl carries on.
//! * [`EngineError`] — the crawl itself cannot continue (bad config, unwritable
//!   database, ...).
//! * Parse problems are never errors. A page that cannot be parsed still has a
//!   status code and headers worth keeping, so the parser is infallible and
//!   simply yields empty fields.

use std::time::Duration;

/// A crawl-level failure. Anything that makes the whole crawl unable to proceed.
#[derive(Debug, thiserror::Error)]
pub enum EngineError {
    #[error("invalid crawl configuration: {0}")]
    Config(String),

    #[error("invalid regex `{pattern}`: {source}")]
    Regex {
        pattern: String,
        #[source]
        source: regex::Error,
    },

    #[error("invalid URL `{url}`: {source}")]
    Url {
        url: String,
        #[source]
        source: url::ParseError,
    },

    #[error("storage error: {0}")]
    Store(#[from] rusqlite::Error),

    #[error("database `{path}` was written by CrawlStat schema v{found}, this build expects v{expected}")]
    SchemaMismatch {
        path: String,
        found: i64,
        expected: i64,
    },

    #[error("could not build HTTP client: {0}")]
    HttpClient(String),

    #[error("i/o error: {0}")]
    Io(#[from] std::io::Error),

    #[error("the crawl was cancelled")]
    Cancelled,

    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

/// Why a single URL could not be fetched.
///
/// Every variant maps onto a stable [`FetchError::kind`] string that is stored
/// in the database, so the UI can group failures without string-matching on
/// human-readable messages.
#[derive(Debug, Clone, thiserror::Error)]
pub enum FetchError {
    #[error("connection failed: {0}")]
    Connect(String),

    #[error("timed out after {0:?}")]
    Timeout(Duration),

    #[error("TLS error: {0}")]
    Tls(String),

    #[error("DNS lookup failed: {0}")]
    Dns(String),

    #[error("too many redirects ({0})")]
    TooManyRedirects(usize),

    #[error("response body exceeded the {limit} byte limit")]
    BodyTooLarge { limit: usize },

    #[error("unsupported URL scheme `{0}`")]
    UnsupportedScheme(String),

    #[error("request failed: {0}")]
    Request(String),
}

impl FetchError {
    /// Short, stable identifier stored in `urls.error_kind`.
    pub fn kind(&self) -> &'static str {
        match self {
            FetchError::Connect(_) => "connect",
            FetchError::Timeout(_) => "timeout",
            FetchError::Tls(_) => "tls",
            FetchError::Dns(_) => "dns",
            FetchError::TooManyRedirects(_) => "too_many_redirects",
            FetchError::BodyTooLarge { .. } => "body_too_large",
            FetchError::UnsupportedScheme(_) => "unsupported_scheme",
            FetchError::Request(_) => "request",
        }
    }

    /// Whether retrying this URL could plausibly succeed.
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            FetchError::Connect(_) | FetchError::Timeout(_) | FetchError::Request(_)
        )
    }
}

pub type Result<T, E = EngineError> = std::result::Result<T, E>;
