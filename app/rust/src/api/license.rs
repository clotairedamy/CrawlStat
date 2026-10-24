//! Licensing, as the UI sees it.
//!
//! The licence key and the signed token never cross this boundary: Dart asks a
//! question and gets a status. That means a UI bug cannot leak either secret,
//! and neither appears in a Flutter crash report.

use crawlstat_engine::license::{self, client};
use flutter_rust_bridge::frb;

/// Everything the Settings screen and the upgrade prompts need.
#[derive(Debug, Clone)]
pub struct LicenseInfo {
    /// `free`, `active`, `grace`, `expired` or `invalid`.
    pub status: String,
    /// `free` or `pro`.
    pub plan: String,
    /// Days until the cached entitlement lapses. Zero on the free tier.
    pub days_remaining: i64,
    /// Which key is active, shown so the user can tell two apart. Not the key.
    pub key_fingerprint: Option<String>,
    pub features: Vec<String>,
    /// URLs allowed per crawl. `None` means unlimited.
    pub max_urls_per_crawl: Option<i64>,
    /// Why a licence was refused or could not be read.
    pub reason: Option<String>,
}

impl From<license::LicenseStatus> for LicenseInfo {
    fn from(status: license::LicenseStatus) -> Self {
        let entitlement = status.entitlement();
        let (name, days, reason) = match &status {
            license::LicenseStatus::Free => ("free", 0, None),
            license::LicenseStatus::Active { days_remaining, .. } => {
                ("active", *days_remaining, None)
            }
            license::LicenseStatus::Grace { days_remaining, .. } => {
                ("grace", *days_remaining, None)
            }
            license::LicenseStatus::Expired { .. } => ("expired", 0, None),
            license::LicenseStatus::Invalid { reason } => ("invalid", 0, Some(reason.clone())),
        };

        Self {
            status: name.to_string(),
            plan: entitlement.plan.as_str().to_string(),
            days_remaining: days,
            key_fingerprint: (!entitlement.key_fingerprint.is_empty())
                .then(|| entitlement.key_fingerprint.clone()),
            features: entitlement.features.clone(),
            max_urls_per_crawl: entitlement.max_urls_per_crawl().map(|cap| cap as i64),
            reason,
        }
    }
}

/// Read the stored licence and make it current. Called once, at startup.
pub fn load_license() -> LicenseInfo {
    LicenseInfo::from(license::load_current())
}

/// The licence in force right now, with no network call.
#[frb(sync)]
pub fn license_status() -> LicenseInfo {
    LicenseInfo::from(license::current())
}

/// Activate a licence key on this machine.
///
/// The user is waiting, so a failure comes back immediately with the service's
/// own wording rather than something generic.
pub fn activate_license(
    license_key: String,
    api_base: Option<String>,
) -> Result<LicenseInfo, String> {
    let base = api_base.unwrap_or_else(|| client::DEFAULT_API_BASE.to_string());
    client::activate(&base, &license_key)
        .map(LicenseInfo::from)
        .map_err(|error| error.to_string())
}

/// Re-check the licence now, whether or not a refresh is due.
pub fn refresh_license(api_base: Option<String>) -> Result<LicenseInfo, String> {
    let base = api_base.unwrap_or_else(|| client::DEFAULT_API_BASE.to_string());
    match stored_key() {
        Some(key) => client::refresh_now(&base, &key)
            .map(LicenseInfo::from)
            .map_err(|error| error.to_string()),
        None => Err("no licence is activated on this machine".to_string()),
    }
}

/// Refresh only if one is due, honouring the backoff.
///
/// Called on launch. A failure here is normal and not worth showing: the cached
/// token covers a fortnight.
pub fn refresh_license_if_due(api_base: Option<String>) -> LicenseInfo {
    let base = api_base.unwrap_or_else(|| client::DEFAULT_API_BASE.to_string());
    match client::refresh_if_due(&base) {
        Ok(status) => LicenseInfo::from(status),
        // Keep whatever is cached; the grace period is what this is for.
        Err(_) => LicenseInfo::from(license::current()),
    }
}

/// Remove the licence from this machine.
pub fn deactivate_license() -> Result<LicenseInfo, String> {
    client::deactivate().map_err(|error| error.to_string())?;
    Ok(LicenseInfo::from(license::current()))
}

/// The stored licence key, for a manual refresh.
///
/// Deliberately private: the key is read inside this crate and handed to the
/// engine, never returned to Dart.
fn stored_key() -> Option<String> {
    crawlstat_engine::license::client::stored_license_key()
}
