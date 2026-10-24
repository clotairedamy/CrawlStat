//! Everything Dart can call.
//!
//! Functions here are deliberately coarse: one call per thing a screen needs,
//! rather than many small calls, because every call crosses the FFI boundary
//! and the UI should never make several to paint one frame.

pub mod crawl;
pub mod license;
pub mod project;
pub mod types;

/// A crawl request carrying the engine's own defaults.
///
/// One source of truth for "what does CrawlStat do if you change nothing":
/// the engine's `CrawlConfig::default`, not a second copy in Dart.
#[flutter_rust_bridge::frb(sync)]
pub fn default_crawl_request(start_url: String) -> types::CrawlRequest {
    types::CrawlRequest::new(start_url)
}

/// The engine version this app is built against. Shown in Settings and sent
/// with crash reports.
#[flutter_rust_bridge::frb(sync)]
pub fn engine_version() -> String {
    crawlstat_engine::VERSION.to_string()
}

/// The default user agent, so the Settings screen can show and reset it.
#[flutter_rust_bridge::frb(sync)]
pub fn default_user_agent() -> String {
    crawlstat_engine::config::DEFAULT_USER_AGENT.to_string()
}

/// URLs per crawl on the free tier. The engine enforces this; the UI only
/// displays it.
#[flutter_rust_bridge::frb(sync)]
pub fn free_tier_max_urls() -> i64 {
    crawlstat_engine::config::FREE_TIER_MAX_URLS as i64
}
