//! Where the licence token and key are kept.
//!
//! The OS keychain — macOS Keychain, Windows Credential Manager, the Secret
//! Service on Linux — through the `keyring` crate.
//!
//! ## Why this is in Rust rather than in Flutter
//!
//! The brief suggested `flutter_secure_storage`, which would work. Doing it
//! here is better for two reasons:
//!
//! * **The secret never crosses the FFI boundary.** Dart asks "am I licensed?"
//!   and gets a status; it never holds the key or the token, so a UI bug cannot
//!   leak either, and neither appears in a Flutter crash dump.
//! * **No native plugin.** The app stays buildable with nothing but the Flutter
//!   SDK — no CocoaPods, no per-platform plugin registration.
//!
//! The licence key is stored alongside the token because refreshing needs it:
//! the app re-presents the key to the API to get a new token.

use keyring::Entry;

/// The service name every entry is filed under in the keychain.
const SERVICE: &str = "com.crawlstat.app";

/// The signed entitlement token.
const TOKEN_ACCOUNT: &str = "license-token";
/// The licence key, needed to refresh the token.
const KEY_ACCOUNT: &str = "license-key";
/// This installation's identifier, so seat counting is stable across restarts.
const INSTANCE_ACCOUNT: &str = "instance-id";

#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    #[error("the system keychain is not available: {0}")]
    Unavailable(String),
    #[error("could not write to the keychain: {0}")]
    Write(String),
}

fn entry(account: &str) -> Result<Entry, StorageError> {
    Entry::new(SERVICE, account).map_err(|error| StorageError::Unavailable(error.to_string()))
}

/// Read a value, treating "not there" as `None` rather than as an error.
fn read(account: &str) -> Result<Option<String>, StorageError> {
    match entry(account)?.get_password() {
        Ok(value) => Ok(Some(value)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(error) => Err(StorageError::Unavailable(error.to_string())),
    }
}

fn write(account: &str, value: &str) -> Result<(), StorageError> {
    entry(account)?
        .set_password(value)
        .map_err(|error| StorageError::Write(error.to_string()))
}

fn delete(account: &str) -> Result<(), StorageError> {
    match entry(account)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(error) => Err(StorageError::Write(error.to_string())),
    }
}

/// The stored entitlement token, if this install has been activated.
pub fn load_stored() -> Result<Option<String>, StorageError> {
    read(TOKEN_ACCOUNT)
}

/// The stored licence key, needed to refresh the token.
pub fn load_key() -> Result<Option<String>, StorageError> {
    read(KEY_ACCOUNT)
}

/// Save a token and the key that produced it.
pub fn store_token(token: &str, license_key: &str) -> Result<(), StorageError> {
    write(TOKEN_ACCOUNT, token)?;
    write(KEY_ACCOUNT, license_key)?;
    Ok(())
}

/// Replace just the token, after a refresh.
pub fn store_refreshed_token(token: &str) -> Result<(), StorageError> {
    write(TOKEN_ACCOUNT, token)
}

/// Forget everything, on deactivation.
///
/// Both entries are attempted even if the first fails, so a half-cleared
/// keychain cannot leave a key behind with no token.
pub fn clear_stored() -> Result<(), StorageError> {
    let token = delete(TOKEN_ACCOUNT);
    let key = delete(KEY_ACCOUNT);
    token.and(key)
}

/// This installation's identifier, created on first use.
///
/// A random value, not a machine fingerprint: it identifies the install for
/// seat counting without being a hardware identifier the user did not consent
/// to share.
pub fn instance_id() -> Result<String, StorageError> {
    if let Some(existing) = read(INSTANCE_ACCOUNT)? {
        if !existing.trim().is_empty() {
            return Ok(existing);
        }
    }
    let generated = generate_instance_id();
    write(INSTANCE_ACCOUNT, &generated)?;
    Ok(generated)
}

/// A random 128-bit identifier, hex-encoded.
fn generate_instance_id() -> String {
    // Hashing a few local entropy sources is enough for an identifier that only
    // needs to be unique, not unguessable.
    let mut hasher = blake3::Hasher::new();
    hasher.update(&super::now_seconds().to_le_bytes());
    hasher.update(&std::process::id().to_le_bytes());
    if let Ok(elapsed) = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
        hasher.update(&elapsed.subsec_nanos().to_le_bytes());
    }
    // A heap address varies per run thanks to ASLR.
    let boxed = Box::new(0u8);
    hasher.update(&(boxed.as_ref() as *const u8 as usize).to_le_bytes());
    hasher.finalize().to_hex()[..32].to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_instance_ids_are_distinct_and_well_formed() {
        let first = generate_instance_id();
        let second = generate_instance_id();
        assert_eq!(first.len(), 32);
        assert!(first.chars().all(|c| c.is_ascii_hexdigit()));
        assert_ne!(first, second, "two installs must not share an id");
    }

    /// Keychain access is skipped in CI: a headless Linux runner has no Secret
    /// Service, and prompting for macOS Keychain access in a test would hang.
    /// The round trip is covered by the app's own activation flow.
    #[test]
    #[ignore = "touches the real system keychain"]
    fn a_token_round_trips_through_the_keychain() {
        store_token("test-token", "TEST-KEY-1234").unwrap();
        assert_eq!(load_stored().unwrap().as_deref(), Some("test-token"));
        assert_eq!(load_key().unwrap().as_deref(), Some("TEST-KEY-1234"));

        store_refreshed_token("fresher-token").unwrap();
        assert_eq!(load_stored().unwrap().as_deref(), Some("fresher-token"));
        // The key survives a token refresh, because the next one needs it.
        assert_eq!(load_key().unwrap().as_deref(), Some("TEST-KEY-1234"));

        clear_stored().unwrap();
        assert_eq!(load_stored().unwrap(), None);
        assert_eq!(load_key().unwrap(), None);
    }
}
