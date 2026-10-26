//! Crawl configuration.
//!
//! [`CrawlConfig`] is plain data: it serialises to JSON, is stored verbatim in
//! the crawl database (so a crawl can be resumed or inspected later), and is
//! what the Flutter UI and the CLI both build. Regexes and other derived state
//! live in [`CompiledConfig`], produced once by [`CrawlConfig::compile`].

use std::time::Duration;

use regex::Regex;
use serde::{Deserialize, Serialize};
use url::Url;

use crate::error::EngineError;
use crate::url_norm::{self, NormalizationPolicy};

/// Default user agent. Points at a public page explaining the bot, as a polite
/// crawler should.
pub const DEFAULT_USER_AGENT: &str = "CrawlStatBot/1.0 (+https://crawlstat.com/bot)";

/// The product token robots.txt rules are matched against (the UA string's
/// leading token, without the parenthetical).
pub const DEFAULT_ROBOTS_TOKEN: &str = "CrawlStatBot";

/// Hard cap on a single response body we will buffer. Anything larger is
/// recorded with its status and size but not parsed — it is not a web page.
pub const DEFAULT_MAX_BODY_BYTES: usize = 10 * 1024 * 1024;

/// URL limit for the free tier. Enforced in the engine, not just the UI.
pub const FREE_TIER_MAX_URLS: u64 = 500;

/// Where a crawl starts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Seed {
    /// Crawl outwards from this page.
    Url { url: String },
    /// Read this XML sitemap (or sitemap index, optionally gzipped) and queue
    /// every URL in it.
    Sitemap { url: String },
    /// Queue exactly these URLs.
    List { urls: Vec<String> },
}

/// How far the crawler follows links.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum CrawlMode {
    /// Follow links discovered on crawled pages (the normal mode).
    #[default]
    Spider,
    /// Fetch only the seeded URLs. Links are still parsed and recorded, but
    /// never queued.
    List,
}

/// Which non-HTML resources are fetched.
///
/// Resources are fetched for their status and size only; they are never parsed
/// for links. Images default to on because "image missing alt" and "oversized
/// image" audits need real byte counts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ResourcePolicy {
    pub images: bool,
    pub css: bool,
    pub javascript: bool,
    /// PDFs and other documents linked with `<a href>`.
    pub documents: bool,
}

impl Default for ResourcePolicy {
    fn default() -> Self {
        Self {
            images: true,
            css: false,
            javascript: false,
            documents: true,
        }
    }
}

/// Everything that defines a crawl.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct CrawlConfig {
    pub seeds: Vec<Seed>,
    pub mode: CrawlMode,

    // --- Politeness -------------------------------------------------------
    /// Simultaneous in-flight requests across the whole crawl.
    pub concurrency: usize,
    /// Simultaneous in-flight requests to any single host.
    pub concurrency_per_host: usize,
    /// Minimum gap between two requests to the same host, in milliseconds.
    pub crawl_delay_ms: u64,
    /// Honour `Crawl-delay` from robots.txt when it is longer than
    /// `crawl_delay_ms`.
    pub respect_robots_crawl_delay: bool,
    pub respect_robots: bool,
    pub user_agent: String,
    /// Token matched against robots.txt `User-agent:` lines. Derived from
    /// `user_agent` when left empty.
    pub robots_token: String,

    // --- Limits -----------------------------------------------------------
    /// Stop after this many URLs have been crawled. `None` means unlimited
    /// (Pro); the engine clamps this for unlicensed installs.
    pub max_urls: Option<u64>,
    /// Maximum link distance from a seed. `None` means unlimited.
    pub max_depth: Option<u32>,
    pub request_timeout_ms: u64,
    pub connect_timeout_ms: u64,
    pub max_retries: u8,
    /// First retry waits this long, doubling each attempt.
    pub retry_backoff_ms: u64,
    pub max_redirects: usize,
    pub max_body_bytes: usize,

    // --- Scope ------------------------------------------------------------
    /// Also crawl `blog.example.com` when the seed is `example.com`.
    pub crawl_subdomains: bool,
    /// Check external links. They are fetched once for their status code and
    /// never followed.
    pub check_external_links: bool,
    /// A URL must match at least one of these to be crawled (empty = allow all).
    pub include_patterns: Vec<String>,
    /// A URL matching any of these is never crawled. Applied after `include`.
    pub exclude_patterns: Vec<String>,
    pub resources: ResourcePolicy,
    /// Queue `rel="nofollow"` links. Off by default, matching search engines.
    pub follow_nofollow: bool,
    /// Queue URLs found in `<link rel="canonical">` and sitemaps even when
    /// nothing links to them. Needed for the orphan-page audit.
    pub follow_sitemap_and_canonical: bool,

    // --- Capture ----------------------------------------------------------
    /// Run the audit pass when the crawl finishes.
    pub run_audits: bool,
    /// Thresholds for the audits that have one.
    pub audits: crate::audit::AuditConfig,

    pub normalization: NormalizationPolicy,
    /// Response headers to persist, lowercase. Empty keeps the default set.
    pub store_headers: Vec<String>,
    /// Keep the raw HTML body in the database. Costly; off by default.
    pub store_html: bool,
    /// Run each page's JavaScript in headless Chrome before parsing it, so
    /// links and content a script adds are seen. Much slower; needs a
    /// Chromium-family browser on the machine.
    pub render_javascript: bool,
}

/// Response headers kept by default: SEO-relevant plus caching basics.
pub const DEFAULT_STORED_HEADERS: &[&str] = &[
    "content-type",
    "content-length",
    "content-encoding",
    "content-language",
    "cache-control",
    "etag",
    "expires",
    "last-modified",
    "location",
    "link",
    "server",
    "vary",
    "x-robots-tag",
    "strict-transport-security",
    "x-frame-options",
    "content-security-policy",
];

impl Default for CrawlConfig {
    fn default() -> Self {
        Self {
            seeds: Vec::new(),
            mode: CrawlMode::default(),

            concurrency: 5,
            concurrency_per_host: 5,
            crawl_delay_ms: 0,
            respect_robots_crawl_delay: true,
            respect_robots: true,
            user_agent: DEFAULT_USER_AGENT.to_string(),
            robots_token: DEFAULT_ROBOTS_TOKEN.to_string(),

            max_urls: None,
            max_depth: None,
            request_timeout_ms: 20_000,
            connect_timeout_ms: 10_000,
            max_retries: 2,
            retry_backoff_ms: 500,
            max_redirects: 10,
            max_body_bytes: DEFAULT_MAX_BODY_BYTES,

            crawl_subdomains: false,
            check_external_links: true,
            include_patterns: Vec::new(),
            exclude_patterns: Vec::new(),
            resources: ResourcePolicy::default(),
            follow_nofollow: false,
            follow_sitemap_and_canonical: true,

            run_audits: true,
            audits: crate::audit::AuditConfig::default(),

            normalization: NormalizationPolicy::default(),
            store_headers: DEFAULT_STORED_HEADERS
                .iter()
                .map(|s| (*s).to_string())
                .collect(),
            store_html: false,
            render_javascript: false,
        }
    }
}

impl CrawlConfig {
    /// A crawl of a single site, starting at `url`.
    pub fn spider(url: impl Into<String>) -> Self {
        Self {
            seeds: vec![Seed::Url { url: url.into() }],
            ..Default::default()
        }
    }

    /// Fetch exactly these URLs and nothing else.
    pub fn list(urls: Vec<String>) -> Self {
        Self {
            seeds: vec![Seed::List { urls }],
            mode: CrawlMode::List,
            ..Default::default()
        }
    }

    pub fn request_timeout(&self) -> Duration {
        Duration::from_millis(self.request_timeout_ms)
    }

    pub fn connect_timeout(&self) -> Duration {
        Duration::from_millis(self.connect_timeout_ms)
    }

    pub fn crawl_delay(&self) -> Duration {
        Duration::from_millis(self.crawl_delay_ms)
    }

    /// Validate and pre-compile everything the crawl loop needs per URL.
    pub fn compile(&self) -> Result<CompiledConfig, EngineError> {
        if self.seeds.is_empty() {
            return Err(EngineError::Config("no seed URLs given".into()));
        }
        if self.concurrency == 0 {
            return Err(EngineError::Config("concurrency must be at least 1".into()));
        }
        if self.concurrency_per_host == 0 {
            return Err(EngineError::Config(
                "concurrency_per_host must be at least 1".into(),
            ));
        }
        if self.max_body_bytes == 0 {
            return Err(EngineError::Config(
                "max_body_bytes must be greater than 0".into(),
            ));
        }
        if self.user_agent.trim().is_empty() {
            return Err(EngineError::Config("user_agent must not be empty".into()));
        }

        // At least one seed has to be a usable http(s) URL. Checking it here
        // rather than when the crawl starts means the CLI and the app's
        // new-crawl dialog both report it next to the field that caused it.
        //
        // Individual bad URLs in a pasted list are *not* fatal: seeding logs a
        // warning for each and carries on with the rest.
        let usable_seeds = self
            .seeds
            .iter()
            .flat_map(|seed| match seed {
                Seed::Url { url } | Seed::Sitemap { url } => std::slice::from_ref(url),
                Seed::List { urls } => urls.as_slice(),
            })
            .filter(|url| url_norm::normalize(url, None, &self.normalization).is_ok())
            .count();
        if usable_seeds == 0 {
            return Err(EngineError::Config(
                "no usable seed URLs: every one is missing a host or is not http(s)".into(),
            ));
        }

        let compile_all = |patterns: &[String]| -> Result<Vec<Regex>, EngineError> {
            patterns
                .iter()
                .map(|p| {
                    Regex::new(p).map_err(|source| EngineError::Regex {
                        pattern: p.clone(),
                        source,
                    })
                })
                .collect()
        };

        let robots_token = if self.robots_token.trim().is_empty() {
            // "CrawlStatBot/1.0 (+https://...)" -> "CrawlStatBot"
            self.user_agent
                .split(['/', ' '])
                .next()
                .unwrap_or(DEFAULT_ROBOTS_TOKEN)
                .to_string()
        } else {
            self.robots_token.clone()
        };

        let store_headers = if self.store_headers.is_empty() {
            DEFAULT_STORED_HEADERS
                .iter()
                .map(|s| (*s).to_string())
                .collect()
        } else {
            self.store_headers
                .iter()
                .map(|h| h.to_ascii_lowercase())
                .collect()
        };

        Ok(CompiledConfig {
            include: compile_all(&self.include_patterns)?,
            exclude: compile_all(&self.exclude_patterns)?,
            robots_token,
            store_headers,
            config: self.clone(),
        })
    }
}

/// A validated [`CrawlConfig`] with its regexes compiled. Cheap to share across
/// worker tasks behind an `Arc`.
#[derive(Debug, Clone)]
pub struct CompiledConfig {
    pub config: CrawlConfig,
    include: Vec<Regex>,
    exclude: Vec<Regex>,
    robots_token: String,
    store_headers: Vec<String>,
}

impl CompiledConfig {
    pub fn robots_token(&self) -> &str {
        &self.robots_token
    }

    /// Should this header be persisted?
    pub fn stores_header(&self, name: &str) -> bool {
        let name = name.to_ascii_lowercase();
        self.store_headers.contains(&name)
    }

    /// Does the URL pass the include/exclude filters?
    ///
    /// `include` is an allowlist when non-empty; `exclude` always wins.
    pub fn matches_filters(&self, url: &str) -> bool {
        if !self.include.is_empty() && !self.include.iter().any(|r| r.is_match(url)) {
            return false;
        }
        !self.exclude.iter().any(|r| r.is_match(url))
    }

    /// Is this URL part of the site being crawled?
    pub fn is_internal(&self, url: &Url, root_host: &str) -> bool {
        url.host_str()
            .is_some_and(|h| url_norm::is_same_site(h, root_host, self.config.crawl_subdomains))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_polite() {
        let c = CrawlConfig::default();
        assert!(c.respect_robots);
        assert_eq!(c.concurrency, 5);
        assert!(!c.follow_nofollow);
        assert!(c.user_agent.contains("crawlstat.com/bot"));
    }

    #[test]
    fn config_round_trips_through_json() {
        let mut c = CrawlConfig::spider("https://example.com");
        c.max_urls = Some(500);
        c.exclude_patterns = vec![r"/cart".into()];
        let json = serde_json::to_string(&c).unwrap();
        let back: CrawlConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(c, back);
    }

    #[test]
    fn empty_seeds_are_rejected() {
        let err = CrawlConfig::default().compile().unwrap_err();
        assert!(matches!(err, EngineError::Config(_)));
    }

    #[test]
    fn bad_regex_is_reported_with_its_pattern() {
        let mut c = CrawlConfig::spider("https://example.com");
        c.include_patterns = vec!["(unclosed".into()];
        match c.compile() {
            Err(EngineError::Regex { pattern, .. }) => assert_eq!(pattern, "(unclosed"),
            other => panic!("expected a regex error, got {other:?}"),
        }
    }

    #[test]
    fn include_is_an_allowlist_and_exclude_wins() {
        let mut c = CrawlConfig::spider("https://example.com");
        c.include_patterns = vec![r"^https://example\.com/blog".into()];
        c.exclude_patterns = vec![r"\?page=".into()];
        let c = c.compile().unwrap();

        assert!(c.matches_filters("https://example.com/blog/post"));
        assert!(!c.matches_filters("https://example.com/shop/item"));
        assert!(!c.matches_filters("https://example.com/blog/post?page=2"));
    }

    #[test]
    fn no_include_patterns_allows_everything_not_excluded() {
        let mut c = CrawlConfig::spider("https://example.com");
        c.exclude_patterns = vec![r"/private/".into()];
        let c = c.compile().unwrap();
        assert!(c.matches_filters("https://example.com/anything"));
        assert!(!c.matches_filters("https://example.com/private/x"));
    }

    #[test]
    fn subdomain_toggle_controls_scope() {
        let url = Url::parse("https://blog.example.com/p").unwrap();

        let c = CrawlConfig::spider("https://example.com")
            .compile()
            .unwrap();
        assert!(!c.is_internal(&url, "example.com"));

        let mut cfg = CrawlConfig::spider("https://example.com");
        cfg.crawl_subdomains = true;
        let c = cfg.compile().unwrap();
        assert!(c.is_internal(&url, "example.com"));
    }

    #[test]
    fn robots_token_is_derived_from_the_user_agent() {
        let mut c = CrawlConfig::spider("https://example.com");
        c.user_agent = "AcmeBot/2.0 (+https://acme.test/bot)".into();
        c.robots_token = String::new();
        assert_eq!(c.compile().unwrap().robots_token(), "AcmeBot");
    }

    #[test]
    fn header_allowlist_is_case_insensitive() {
        let c = CrawlConfig::spider("https://example.com")
            .compile()
            .unwrap();
        assert!(c.stores_header("Content-Type"));
        assert!(c.stores_header("X-Robots-Tag"));
        assert!(!c.stores_header("x-made-up"));
    }
}
