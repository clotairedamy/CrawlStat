//! Keys the user brings, kept where the operating system keeps secrets.
//!
//! Phase 5 is "bring your own key": PageSpeed Insights, Search Console, an
//! Ollama endpoint. CrawlStat never has an account to tie these to and never
//! sees them — they are the user's credentials with the user's own providers,
//! and the only machine they are ever sent from is this one.
//!
//! Stored exactly like the licence token, and for the same reasons: the OS
//! keychain, written and read in Rust, so no secret crosses into Dart where a
//! UI bug or a crash dump could leak it. Dart asks "is a key set?" and gets a
//! yes or no. See [`crate::license::storage`] for the longer argument.

use keyring::Entry;

pub use crate::license::storage::StorageError;

/// Filed under the same service as the licence, with its own accounts.
const SERVICE: &str = "com.crawlstat.app";

/// A credential CrawlStat knows how to store.
///
/// An enum rather than free-form strings so a typo cannot quietly write a
/// secret to an account nothing ever reads again.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Secret {
    /// Google PageSpeed Insights API key.
    PageSpeedKey,
    /// Google OAuth refresh token for Search Console.
    SearchConsoleToken,
}

impl Secret {
    pub fn account(self) -> &'static str {
        match self {
            Secret::PageSpeedKey => "pagespeed-key",
            Secret::SearchConsoleToken => "search-console-token",
        }
    }

    /// Parse the name the UI passes across the bridge.
    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "pagespeed" => Some(Secret::PageSpeedKey),
            "search_console" => Some(Secret::SearchConsoleToken),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Secret::PageSpeedKey => "pagespeed",
            Secret::SearchConsoleToken => "search_console",
        }
    }
}

fn entry(account: &str) -> Result<Entry, StorageError> {
    Entry::new(SERVICE, account).map_err(|error| StorageError::Unavailable(error.to_string()))
}

/// Store a key. An empty value clears it, which is what an emptied text field
/// in the UI should mean.
pub fn store(secret: Secret, value: &str) -> Result<(), StorageError> {
    if value.trim().is_empty() {
        return clear(secret);
    }
    entry(secret.account())?
        .set_password(value.trim())
        .map_err(|error| StorageError::Write(error.to_string()))
}

/// Read a key, for the code that is about to make the call.
///
/// Deliberately not exposed across the bridge: nothing in Dart ever needs the
/// value, only [`is_set`].
pub fn load(secret: Secret) -> Result<Option<String>, StorageError> {
    match entry(secret.account())?.get_password() {
        Ok(value) => Ok(Some(value)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(error) => Err(StorageError::Unavailable(error.to_string())),
    }
}

/// Whether a key is stored. This is all the UI is told.
pub fn is_set(secret: Secret) -> bool {
    matches!(load(secret), Ok(Some(value)) if !value.is_empty())
}

pub fn clear(secret: Secret) -> Result<(), StorageError> {
    match entry(secret.account())?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(error) => Err(StorageError::Write(error.to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_round_trip_through_the_bridge_form() {
        for secret in [Secret::PageSpeedKey, Secret::SearchConsoleToken] {
            assert_eq!(Secret::parse(secret.as_str()), Some(secret));
        }
    }

    #[test]
    fn an_unknown_name_is_refused_rather_than_guessed() {
        assert_eq!(Secret::parse("openai"), None);
        assert_eq!(Secret::parse(""), None);
    }

    #[test]
    fn every_secret_has_its_own_account() {
        let accounts = [Secret::PageSpeedKey, Secret::SearchConsoleToken]
            .map(|secret| secret.account());
        let mut sorted = accounts.to_vec();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), accounts.len(), "two secrets share an account");
    }
}
