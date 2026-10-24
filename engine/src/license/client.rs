//! Talking to the licence API.
//!
//! This lives in the engine, not in the app, for the same reason the keychain
//! does: the licence key and the token never cross the FFI boundary. Dart asks
//! "activate this key" and gets back a status; it never holds either secret.
//!
//! ## What happens when the API is unreachable
//!
//! Nothing urgent. A refresh failure is expected — laptops close, networks
//! drop, our own deploy might be mid-flight — and the cached token covers a
//! fortnight. So failures are retried with an exponential backoff and never
//! surfaced as an error to the user until the token is actually close to
//! lapsing.
//!
//! Activation is different: the user is sitting there having just pasted a key,
//! so a failure there is reported immediately and verbatim.

use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use serde::{Deserialize, Serialize};

use super::{evaluate, now_seconds, storage, LicenseStatus, SIGNING_PUBLIC_KEY_HEX};

/// Where the licence API lives.
pub const DEFAULT_API_BASE: &str = "https://crawlstat.com/api";

/// How long to wait on the licence API.
///
/// Short on purpose: this runs at app startup, and a slow licence check must
/// never hold up the UI. A timeout is just another refresh failure.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

/// The first retry waits this long, doubling each time, capped at a day.
const BACKOFF_BASE: Duration = Duration::from_secs(60);
const BACKOFF_CAP: Duration = Duration::from_secs(86_400);

#[derive(Debug, thiserror::Error)]
pub enum ClientError {
    #[error("could not reach the licence service: {0}")]
    Unreachable(String),
    #[error("the licence service returned an unexpected response: {0}")]
    BadResponse(String),
    #[error("{0}")]
    Refused(String),
    #[error("the licence service sent a token this build cannot verify: {0}")]
    BadToken(String),
    #[error("could not save the licence: {0}")]
    Storage(#[from] storage::StorageError),
    #[error("too many failed attempts; next retry in {seconds}s")]
    BackingOff { seconds: u64 },
}

#[derive(Serialize)]
struct LicenseRequest<'a> {
    license_key: &'a str,
    instance_id: &'a str,
}

#[derive(Deserialize)]
struct LicenseResponse {
    valid: bool,
    token: Option<String>,
    reason: Option<String>,
}

/// Retry state for refreshes. Per-process: a restart is itself a retry.
#[derive(Debug, Default)]
struct Backoff {
    failures: u32,
    /// Unix seconds of the last attempt.
    last_attempt: i64,
}

fn backoff() -> &'static Mutex<Backoff> {
    static BACKOFF: OnceLock<Mutex<Backoff>> = OnceLock::new();
    BACKOFF.get_or_init(|| Mutex::new(Backoff::default()))
}

/// Seconds to wait after `failures` consecutive failures.
fn backoff_seconds(failures: u32) -> u64 {
    if failures == 0 {
        return 0;
    }
    let doubled = BACKOFF_BASE
        .as_secs()
        .saturating_mul(1u64 << failures.min(16).saturating_sub(1));
    doubled.min(BACKOFF_CAP.as_secs())
}

/// Activate a licence key on this installation.
///
/// Called once, when the user pastes their key. On success the token and the
/// key are saved to the keychain and the licence becomes current.
pub fn activate(api_base: &str, license_key: &str) -> Result<LicenseStatus, ClientError> {
    let license_key = license_key.trim();
    if license_key.is_empty() {
        return Err(ClientError::Refused("enter your licence key".into()));
    }

    let instance_id = storage::instance_id()?;
    let response = post(
        &format!("{}/license/activate", api_base.trim_end_matches('/')),
        &LicenseRequest {
            license_key,
            instance_id: &instance_id,
        },
    )?;

    if !response.valid {
        return Err(ClientError::Refused(
            response
                .reason
                .unwrap_or_else(|| "that licence key was not recognised".into()),
        ));
    }
    let token = response
        .token
        .ok_or_else(|| ClientError::BadResponse("no token in the response".into()))?;

    // Verify before storing: a token this build cannot read is useless, and
    // finding that out now is far better than on the next launch.
    let status = evaluate(&token, SIGNING_PUBLIC_KEY_HEX, now_seconds());
    if let LicenseStatus::Invalid { reason } = &status {
        return Err(ClientError::BadToken(reason.clone()));
    }

    storage::store_token(&token, license_key)?;
    super::set_current(status.clone());
    reset_backoff();
    Ok(status)
}

/// Refresh the token if it is due, honouring the backoff.
///
/// Returns the current status either way, so the caller can use the result
/// without caring whether a request was made.
pub fn refresh_if_due(api_base: &str) -> Result<LicenseStatus, ClientError> {
    let current = super::current();
    let entitlement = current.entitlement();

    // Nothing to refresh on the free tier, and nothing due yet otherwise.
    if entitlement.plan == super::Plan::Free {
        return Ok(current);
    }
    // A developer entitlement was never issued by the licence service, so
    // asking it to reissue one would only ever fail. Leave it alone.
    if entitlement.key_fingerprint == "dev" {
        return Ok(current);
    }
    if !entitlement.needs_refresh(now_seconds()) {
        return Ok(current);
    }

    let now = now_seconds();
    {
        let state = backoff()
            .lock()
            .map_err(|_| ClientError::Unreachable("the licence backoff lock is poisoned".into()))?;
        let wait = backoff_seconds(state.failures) as i64;
        if state.failures > 0 && now - state.last_attempt < wait {
            return Err(ClientError::BackingOff {
                seconds: (wait - (now - state.last_attempt)) as u64,
            });
        }
    }

    let Some(license_key) = storage::load_key()? else {
        // A token with no key cannot be refreshed; it will lapse on its own.
        return Ok(current);
    };

    match refresh_now(api_base, &license_key) {
        Ok(status) => {
            reset_backoff();
            Ok(status)
        }
        Err(error) => {
            record_failure(now);
            Err(error)
        }
    }
}

/// Refresh unconditionally. Used by the Settings screen's "check now" button.
pub fn refresh_now(api_base: &str, license_key: &str) -> Result<LicenseStatus, ClientError> {
    let instance_id = storage::instance_id()?;
    let response = post(
        &format!("{}/license/validate", api_base.trim_end_matches('/')),
        &LicenseRequest {
            license_key: license_key.trim(),
            instance_id: &instance_id,
        },
    )?;

    if !response.valid {
        // The licence is genuinely gone — refunded, cancelled, expired. Clear
        // it rather than leaving a stale token to keep working for a fortnight.
        let reason = response
            .reason
            .unwrap_or_else(|| "this licence is no longer valid".into());
        storage::clear_stored()?;
        super::set_current(LicenseStatus::Free);
        return Err(ClientError::Refused(reason));
    }

    let token = response
        .token
        .ok_or_else(|| ClientError::BadResponse("no token in the response".into()))?;
    let status = evaluate(&token, SIGNING_PUBLIC_KEY_HEX, now_seconds());
    if let LicenseStatus::Invalid { reason } = &status {
        return Err(ClientError::BadToken(reason.clone()));
    }

    storage::store_refreshed_token(&token)?;
    super::set_current(status.clone());
    Ok(status)
}

/// The stored licence key, for a manual refresh.
///
/// Returns `None` when nothing is activated. Kept here rather than exported
/// from `storage` so the only way to read the key is through this module,
/// which is the one that knows what to do with it.
pub fn stored_license_key() -> Option<String> {
    storage::load_key().ok().flatten()
}

/// Forget the licence on this machine.
pub fn deactivate() -> Result<(), ClientError> {
    storage::clear_stored()?;
    super::set_current(LicenseStatus::Free);
    reset_backoff();
    Ok(())
}

fn reset_backoff() {
    if let Ok(mut state) = backoff().lock() {
        *state = Backoff::default();
    }
}

fn record_failure(now: i64) {
    if let Ok(mut state) = backoff().lock() {
        state.failures = state.failures.saturating_add(1);
        state.last_attempt = now;
    }
}

/// POST a licence request and read the answer.
///
/// A blocking client built per call: this runs at most twice a day, so a
/// long-lived client would be holding connections open for nothing.
fn post<T: Serialize>(url: &str, body: &T) -> Result<LicenseResponse, ClientError> {
    let client = reqwest::blocking::Client::builder()
        .timeout(REQUEST_TIMEOUT)
        .user_agent(format!("CrawlStat/{} (licence client)", crate::VERSION))
        .build()
        .map_err(|error| ClientError::Unreachable(error.to_string()))?;

    let response = client
        .post(url)
        .json(body)
        .send()
        .map_err(|error| ClientError::Unreachable(error.to_string()))?;

    let status = response.status();
    if status.as_u16() == 429 {
        return Err(ClientError::Unreachable(
            "rate limited by the licence service".into(),
        ));
    }
    // 400 and 422 carry a readable reason in the body; anything else does not.
    if !status.is_success() && status.as_u16() != 400 && status.as_u16() != 422 {
        return Err(ClientError::BadResponse(format!("HTTP {status}")));
    }

    response
        .json::<LicenseResponse>()
        .map_err(|error| ClientError::BadResponse(error.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_backoff_doubles_and_then_stops_growing() {
        assert_eq!(backoff_seconds(0), 0);
        assert_eq!(backoff_seconds(1), 60);
        assert_eq!(backoff_seconds(2), 120);
        assert_eq!(backoff_seconds(3), 240);
        assert_eq!(backoff_seconds(4), 480);

        // Capped at a day, however many times it has failed — including at
        // absurd failure counts, which must not overflow.
        assert_eq!(backoff_seconds(20), BACKOFF_CAP.as_secs());
        assert_eq!(backoff_seconds(u32::MAX), BACKOFF_CAP.as_secs());
    }

    #[test]
    fn an_empty_key_is_refused_without_a_request() {
        // No network, no keychain: just a clear message for an empty field.
        let error = activate("https://example.invalid/api", "   ").unwrap_err();
        assert!(matches!(error, ClientError::Refused(_)));
        assert!(error.to_string().contains("licence key"));
    }

    #[test]
    fn a_free_install_never_tries_to_refresh() {
        super::super::set_current(LicenseStatus::Free);
        // An unreachable host would fail if a request were actually made.
        let status = refresh_if_due("https://unreachable.invalid/api").unwrap();
        assert_eq!(status, LicenseStatus::Free);
    }
}
