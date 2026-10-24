//! Licensing: what this install is entitled to, and how that is proved offline.
//!
//! ## The shape of it
//!
//! The payment provider is the source of truth for who has paid. CrawlStat's
//! own API ([`site/server/api/license/`]) is a thin proxy in front of it that
//! signs a short-lived **entitlement token** with an Ed25519 key. The app
//! embeds the matching public key and verifies tokens itself, so:
//!
//! * There is **no database** to run. The provider holds the licences.
//! * The app can verify entitlement **offline**, with no round trip.
//! * A token cannot be forged without the private key, which never leaves
//!   Vercel's environment variables.
//!
//! ## Why a grace period rather than a daily check
//!
//! A token is valid for [`GRACE_PERIOD`]. The app refreshes it about once a
//! day; if it cannot — no network, a flight, our API down — the cached token
//! keeps working until it expires. Only after a fortnight of failed refreshes
//! does the install fall back to the free tier.
//!
//! The rule this exists to serve is simple: **a network failure must never
//! interrupt someone's work.** A crawl already running is never re-checked, and
//! a paying customer on a plane keeps their features.
//!
//! ## Why enforcement lives here
//!
//! The URL limit is applied by the crawl engine reading [`current`], not by the
//! UI hiding a button. A hand-built `CrawlConfig`, the CLI, or a future CI
//! runner all go through the same check, so there is one place to get right and
//! no way to route around it.

use std::sync::{OnceLock, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};

use base64::Engine as _;
use ed25519_dalek::{Signature, VerifyingKey};
use serde::{Deserialize, Serialize};

pub mod client;
mod storage;

pub use storage::{clear_stored, instance_id, load_stored, store_token, StorageError};

/// How long a token stays valid without being refreshed.
pub const GRACE_PERIOD_DAYS: i64 = 14;

/// How often the app tries to refresh. Well inside the grace period, so a few
/// failures in a row are invisible to the user.
pub const REFRESH_INTERVAL_HOURS: i64 = 24;

/// URLs per crawl without a licence.
pub const FREE_TIER_MAX_URLS: u64 = crate::config::FREE_TIER_MAX_URLS;

/// The Ed25519 public key that signs entitlement tokens, hex-encoded.
///
/// **This placeholder is the RFC 8032 §7.1 test vector**, whose private half is
/// published in the standard. It is deliberately a key everybody has: nobody
/// can mistake it for a secret, and a development build therefore verifies only
/// tokens signed with a key the whole world knows — never a real one. The
/// release build substitutes the production public key, so a development binary
/// cannot honour a production token, or the reverse.
pub const SIGNING_PUBLIC_KEY_HEX: &str =
    "d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a";

/// What the user has paid for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Plan {
    #[default]
    Free,
    Pro,
}

impl Plan {
    pub fn as_str(self) -> &'static str {
        match self {
            Plan::Free => "free",
            Plan::Pro => "pro",
        }
    }
}

/// A Pro capability. Strings rather than an enum so the server can grant
/// something a given build does not know about yet without breaking it.
pub mod features {
    pub const UNLIMITED_URLS: &str = "unlimited_urls";
    pub const CRAWL_COMPARISON: &str = "crawl_comparison";
    pub const CUSTOM_EXTRACTION: &str = "custom_extraction";
    pub const SCHEDULED_CRAWLS: &str = "scheduled_crawls";
    pub const XLSX_EXPORT: &str = "xlsx_export";
    pub const HTML_REPORT: &str = "html_report";
    pub const BYOK_INTEGRATIONS: &str = "byok_integrations";

    /// Everything a Pro licence grants in this build.
    pub const PRO: &[&str] = &[
        UNLIMITED_URLS,
        CRAWL_COMPARISON,
        CUSTOM_EXTRACTION,
        SCHEDULED_CRAWLS,
        XLSX_EXPORT,
        HTML_REPORT,
        BYOK_INTEGRATIONS,
    ];
}

/// The signed body of an entitlement token.
///
/// Deliberately small, and deliberately free of anything personal: no email, no
/// name, no provider customer id. A token on a stolen laptop should reveal
/// nothing about who owns it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entitlement {
    /// Token format version, so a future change can be told apart.
    pub version: u8,
    pub plan: Plan,
    /// Unix seconds when the token was signed.
    pub issued_at: i64,
    /// Unix seconds after which the token no longer proves anything.
    pub expires_at: i64,
    /// First eight characters of the licence key's hash. Enough to show the
    /// user which key is active and to tell two keys apart; not enough to
    /// reconstruct the key.
    pub key_fingerprint: String,
    /// This installation, so the server can count seats.
    pub instance_id: String,
    pub features: Vec<String>,
}

impl Entitlement {
    /// The entitlement of an install that has never been activated.
    pub fn free() -> Self {
        Self {
            version: 1,
            plan: Plan::Free,
            issued_at: 0,
            expires_at: 0,
            key_fingerprint: String::new(),
            instance_id: String::new(),
            features: Vec::new(),
        }
    }

    pub fn is_pro(&self) -> bool {
        self.plan == Plan::Pro
    }

    pub fn has(&self, feature: &str) -> bool {
        self.features.iter().any(|granted| granted == feature)
    }

    /// URLs allowed per crawl. `None` means unlimited.
    pub fn max_urls_per_crawl(&self) -> Option<u64> {
        if self.has(features::UNLIMITED_URLS) {
            None
        } else {
            Some(FREE_TIER_MAX_URLS)
        }
    }

    /// Seconds until the token expires; negative once it has.
    pub fn seconds_remaining(&self, now: i64) -> i64 {
        self.expires_at - now
    }

    pub fn days_remaining(&self, now: i64) -> i64 {
        // Round up, so "expires in 20 hours" reads as one day left rather than
        // none.
        (self.seconds_remaining(now) + 86_399) / 86_400
    }

    /// Whether the app should try to refresh this token.
    pub fn needs_refresh(&self, now: i64) -> bool {
        if self.plan == Plan::Free {
            return false;
        }
        now - self.issued_at >= REFRESH_INTERVAL_HOURS * 3_600
    }
}

/// What the UI shows about the licence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum LicenseStatus {
    /// No licence: the free tier, which is a product, not an error.
    Free,
    /// A valid licence, refreshed recently.
    Active {
        entitlement: Entitlement,
        days_remaining: i64,
    },
    /// Valid, but we have not been able to refresh it for a while. Everything
    /// keeps working; the UI mentions it once it is close to lapsing.
    Grace {
        entitlement: Entitlement,
        days_remaining: i64,
    },
    /// The token ran out. Features are off until it is refreshed.
    Expired { entitlement: Entitlement },
    /// A stored token that will not verify. Treated exactly like Free, and
    /// surfaced so the user can re-activate rather than wonder.
    Invalid { reason: String },
}

impl LicenseStatus {
    /// The entitlement to enforce. An expired or invalid licence falls back to
    /// the free tier rather than to nothing.
    pub fn entitlement(&self) -> Entitlement {
        match self {
            LicenseStatus::Active { entitlement, .. }
            | LicenseStatus::Grace { entitlement, .. } => entitlement.clone(),
            _ => Entitlement::free(),
        }
    }

    pub fn is_pro(&self) -> bool {
        self.entitlement().is_pro()
    }
}

/// Why a token was rejected.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TokenError {
    #[error("the token is not in the expected format")]
    Malformed,
    #[error("the token's signature does not match")]
    BadSignature,
    #[error("the token's contents could not be read: {0}")]
    BadPayload(String),
    #[error("this build cannot read a version {0} token")]
    UnsupportedVersion(u8),
    #[error("the embedded signing key is not usable: {0}")]
    BadVerifyingKey(String),
}

/// Encode an entitlement as a token body. Used by the tests and by the signing
/// service; the app only ever verifies.
pub fn encode_payload(entitlement: &Entitlement) -> String {
    let json = serde_json::to_vec(entitlement).expect("an entitlement is serialisable");
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(json)
}

/// Verify a token and return what it grants.
///
/// The token is `<base64url payload>.<base64url signature>`, signed over the
/// payload's bytes exactly as they appear — so re-encoding the JSON cannot
/// change what was signed.
pub fn verify_token(token: &str, public_key_hex: &str) -> Result<Entitlement, TokenError> {
    let (payload_b64, signature_b64) = token.trim().split_once('.').ok_or(TokenError::Malformed)?;

    let payload = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload_b64)
        .map_err(|_| TokenError::Malformed)?;
    let signature_bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(signature_b64)
        .map_err(|_| TokenError::Malformed)?;

    let key_bytes: [u8; 32] = decode_hex32(public_key_hex)
        .ok_or_else(|| TokenError::BadVerifyingKey("not 32 hex-encoded bytes".into()))?;
    let verifying_key = VerifyingKey::from_bytes(&key_bytes)
        .map_err(|error| TokenError::BadVerifyingKey(error.to_string()))?;

    let signature_bytes: [u8; 64] = signature_bytes
        .try_into()
        .map_err(|_| TokenError::Malformed)?;
    let signature = Signature::from_bytes(&signature_bytes);

    // Signature first: nothing in an unverified payload is worth parsing.
    verifying_key
        .verify_strict(payload_b64.as_bytes(), &signature)
        .map_err(|_| TokenError::BadSignature)?;

    let entitlement: Entitlement = serde_json::from_slice(&payload)
        .map_err(|error| TokenError::BadPayload(error.to_string()))?;
    if entitlement.version != 1 {
        return Err(TokenError::UnsupportedVersion(entitlement.version));
    }
    Ok(entitlement)
}

/// Classify a verified entitlement against the clock.
pub fn classify(entitlement: Entitlement, now: i64) -> LicenseStatus {
    if entitlement.plan == Plan::Free {
        return LicenseStatus::Free;
    }
    let days = entitlement.days_remaining(now);
    if days <= 0 {
        return LicenseStatus::Expired { entitlement };
    }
    // Inside the refresh window everything is normal; past it we are running on
    // a stale token, which is worth saying so the user can get online.
    if entitlement.needs_refresh(now) {
        LicenseStatus::Grace {
            entitlement,
            days_remaining: days,
        }
    } else {
        LicenseStatus::Active {
            entitlement,
            days_remaining: days,
        }
    }
}

/// Verify a stored token and classify it in one step.
pub fn evaluate(token: &str, public_key_hex: &str, now: i64) -> LicenseStatus {
    match verify_token(token, public_key_hex) {
        Ok(entitlement) => classify(entitlement, now),
        Err(error) => LicenseStatus::Invalid {
            reason: error.to_string(),
        },
    }
}

/// Unix seconds now.
pub fn now_seconds() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs() as i64)
        .unwrap_or(0)
}

/// A licence key's fingerprint: the first eight characters of its hash.
pub fn fingerprint(license_key: &str) -> String {
    crate::hash::content_hash(license_key.trim().as_bytes())[..8].to_string()
}

fn decode_hex32(hex: &str) -> Option<[u8; 32]> {
    let hex = hex.trim();
    if hex.len() != 64 {
        return None;
    }
    let mut out = [0u8; 32];
    for (index, chunk) in hex.as_bytes().chunks(2).enumerate() {
        let pair = std::str::from_utf8(chunk).ok()?;
        out[index] = u8::from_str_radix(pair, 16).ok()?;
    }
    Some(out)
}

// ─── process-wide licence state ──────────────────────────────────────────

fn state() -> &'static RwLock<LicenseStatus> {
    static STATE: OnceLock<RwLock<LicenseStatus>> = OnceLock::new();
    STATE.get_or_init(|| RwLock::new(LicenseStatus::Free))
}

/// The licence this process is running under.
///
/// Read by the crawl engine to apply limits. Defaults to the free tier, so a
/// build that never calls [`set_current`] is correctly limited rather than
/// accidentally unlimited — the safe direction for a mistake to fail in.
pub fn current() -> LicenseStatus {
    state()
        .read()
        .map(|status| status.clone())
        .unwrap_or(LicenseStatus::Free)
}

/// Replace the licence state. Called after activation and after each refresh.
pub fn set_current(status: LicenseStatus) {
    if let Ok(mut slot) = state().write() {
        *slot = status;
    }
}

/// Load the stored token, verify it, and make it current.
///
/// Called once at startup. A missing or unreadable token is not an error: it
/// means the free tier.
pub fn load_current() -> LicenseStatus {
    let status = match load_stored() {
        Ok(Some(token)) => evaluate(&token, SIGNING_PUBLIC_KEY_HEX, now_seconds()),
        Ok(None) => LicenseStatus::Free,
        Err(error) => {
            // A keychain that will not open is worth reporting, but it must not
            // stop the app: the free tier still works.
            tracing::warn!("could not read the stored licence: {error}");
            LicenseStatus::Free
        }
    };
    set_current(status.clone());
    status
}

/// The URL limit to apply to a crawl, given the current licence.
pub fn max_urls_for_crawl(requested: Option<u64>) -> Option<u64> {
    match current().entitlement().max_urls_per_crawl() {
        // Pro: whatever the user asked for, including no limit at all.
        None => requested,
        // Free: the lower of the two, so a smaller explicit limit still works.
        Some(cap) => Some(requested.map_or(cap, |asked| asked.min(cap))),
    }
}

/// Test-only helpers for the process-wide licence state.
///
/// Enforcement reads a global, which is the point — nothing can route around
/// it — but it makes every test that touches it share one resource. This lock
/// is in the library rather than in one test module so tests in *any* module
/// serialise against each other.
#[cfg(test)]
pub(crate) mod testing {
    use super::{set_current, LicenseStatus};

    /// A tokio mutex rather than a `std` one, so an async test can hold it
    /// across an await — which is exactly what a test that runs a whole crawl
    /// needs to do.
    static LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

    /// Run a synchronous `test` with `status` in force, then restore the free
    /// tier.
    pub(crate) fn with_license<T>(status: LicenseStatus, test: impl FnOnce() -> T) -> T {
        let _guard = LOCK.blocking_lock();
        set_current(status);
        let result = test();
        set_current(LicenseStatus::Free);
        result
    }

    /// Hold the lock for the duration of an async test.
    pub(crate) async fn guard() -> tokio::sync::MutexGuard<'static, ()> {
        LOCK.lock().await
    }
}

#[cfg(test)]
mod tests;
