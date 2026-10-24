//! `robots.txt` fetching, caching and evaluation.
//!
//! One `robots.txt` is fetched per origin (scheme + host + port) and cached for
//! the life of the crawl. The file also gives us two useful extras: a
//! `Crawl-delay` to obey, and `Sitemap:` lines we can seed from.
//!
//! ## What happens when robots.txt cannot be read
//!
//! RFC 9309 says a 5xx should be treated as "disallow everything". We do not do
//! that, and the choice is deliberate: CrawlStat is a desktop tool pointed at a
//! site by a person who usually owns it, and refusing to crawl because their
//! server hiccuped on `/robots.txt` would be useless behaviour. Instead a
//! fetch failure or 5xx allows the crawl and raises
//! [`RobotsStatus::Unavailable`], which the UI shows as a warning. A clean 404
//! means "no rules", which is also allow-all — that part is per spec.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use texting_robots::Robot;
use tokio::sync::Mutex;
use url::Url;

use crate::fetch::{FetchRequest, Fetcher};

/// How the fetch of a `robots.txt` went.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RobotsStatus {
    /// Fetched and parsed.
    Found,
    /// A clean 404/410: the site has no rules.
    NotFound,
    /// A 5xx, a network failure, or a body we could not parse. Treated as
    /// allow-all, with a warning.
    Unavailable,
    /// The user turned robots.txt off.
    Ignored,
}

/// Parsed rules for one origin.
#[derive(Debug)]
pub struct RobotsRules {
    robot: Option<Robot>,
    pub status: RobotsStatus,
    /// `Crawl-delay` for our user agent, if any.
    pub crawl_delay: Option<Duration>,
    /// `Sitemap:` lines, which the engine may seed from.
    pub sitemaps: Vec<String>,
}

impl RobotsRules {
    /// Rules that allow everything.
    pub fn allow_all(status: RobotsStatus) -> Self {
        Self {
            robot: None,
            status,
            crawl_delay: None,
            sitemaps: Vec::new(),
        }
    }

    /// Parse a `robots.txt` body for the given user-agent token.
    pub fn parse(body: &[u8], agent: &str) -> Self {
        match Robot::new(agent, body) {
            Ok(robot) => {
                let crawl_delay = robot
                    .delay
                    .filter(|d| d.is_finite() && *d > 0.0)
                    // Cap at a minute: a few sites publish absurd values that
                    // would otherwise stall the crawl indefinitely.
                    .map(|d| Duration::from_secs_f32(d.min(60.0)));
                let sitemaps = robot.sitemaps.clone();
                Self {
                    robot: Some(robot),
                    status: RobotsStatus::Found,
                    crawl_delay,
                    sitemaps,
                }
            }
            // An unparseable robots.txt is not a reason to refuse to crawl.
            Err(_) => Self::allow_all(RobotsStatus::Unavailable),
        }
    }

    /// May we fetch this URL?
    pub fn allows(&self, url: &str) -> bool {
        match &self.robot {
            Some(robot) => robot.allowed(url),
            None => true,
        }
    }
}

/// Per-origin `robots.txt` cache.
///
/// `get` is safe to call from every worker: concurrent callers for the same
/// origin wait on one fetch rather than each making their own.
pub struct RobotsCache {
    /// Keyed by origin, e.g. `https://example.com` or `http://host:8080`.
    cache: Mutex<HashMap<String, Arc<RobotsRules>>>,
    agent: String,
    enabled: bool,
}

impl RobotsCache {
    pub fn new(agent: impl Into<String>, enabled: bool) -> Self {
        Self {
            cache: Mutex::new(HashMap::new()),
            agent: agent.into(),
            enabled,
        }
    }

    /// Origin key for a URL: scheme, host and (non-default) port.
    pub fn origin_of(url: &Url) -> String {
        let mut origin = format!("{}://{}", url.scheme(), url.host_str().unwrap_or_default());
        if let Some(port) = url.port() {
            origin.push(':');
            origin.push_str(&port.to_string());
        }
        origin
    }

    /// Rules for the URL's origin, fetching `robots.txt` the first time.
    pub async fn rules_for(&self, url: &Url, fetcher: &dyn Fetcher) -> Arc<RobotsRules> {
        if !self.enabled {
            return Arc::new(RobotsRules::allow_all(RobotsStatus::Ignored));
        }

        let origin = Self::origin_of(url);
        if let Some(cached) = self.cache.lock().await.get(&origin) {
            return Arc::clone(cached);
        }

        let rules = Arc::new(self.fetch_rules(&origin, fetcher).await);

        // Another worker may have inserted while we were fetching; either entry
        // is equally valid, so keep whichever landed first.
        let mut cache = self.cache.lock().await;
        Arc::clone(cache.entry(origin).or_insert(rules))
    }

    async fn fetch_rules(&self, origin: &str, fetcher: &dyn Fetcher) -> RobotsRules {
        let Ok(robots_url) = Url::parse(&format!("{origin}/robots.txt")) else {
            return RobotsRules::allow_all(RobotsStatus::Unavailable);
        };

        let mut request = FetchRequest::get(robots_url);
        // robots.txt beyond 512 KiB is not a real robots.txt; Google uses the
        // same cut-off.
        request.max_body_bytes = 512 * 1024;

        match fetcher.fetch(request).await {
            Ok(response) if response.is_success() => {
                RobotsRules::parse(&response.body, &self.agent)
            }
            // 404/410 and other 4xx: no rules published.
            Ok(response) if (400..500).contains(&response.status) => {
                RobotsRules::allow_all(RobotsStatus::NotFound)
            }
            // 5xx or anything else: see the module docs.
            Ok(_) | Err(_) => RobotsRules::allow_all(RobotsStatus::Unavailable),
        }
    }

    /// Origins whose `robots.txt` has already been fetched.
    pub async fn known_origins(&self) -> Vec<String> {
        self.cache.lock().await.keys().cloned().collect()
    }

    /// Every `Sitemap:` line seen so far, across all origins.
    pub async fn discovered_sitemaps(&self) -> Vec<String> {
        let cache = self.cache.lock().await;
        let mut sitemaps: Vec<String> = cache.values().flat_map(|r| r.sitemaps.clone()).collect();
        sitemaps.sort();
        sitemaps.dedup();
        sitemaps
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::FetchError;
    use crate::fetch::mock::{MockFetcher, MockResponse};

    const ROBOTS: &str = "User-agent: *\n\
                          Disallow: /private/\n\
                          Disallow: /*.pdf$\n\
                          Crawl-delay: 2\n\
                          \n\
                          User-agent: CrawlStatBot\n\
                          Disallow: /no-crawlstat/\n\
                          Allow: /private/public-corner/\n\
                          \n\
                          Sitemap: https://a.test/sitemap.xml\n";

    fn robots_response(body: &str) -> MockResponse {
        MockResponse {
            status: 200,
            content_type: "text/plain".into(),
            body: body.to_string(),
            redirect_to: None,
            headers: Vec::new(),
            error: None,
        }
    }

    #[test]
    fn our_own_group_wins_over_the_wildcard() {
        let rules = RobotsRules::parse(ROBOTS.as_bytes(), "CrawlStatBot");
        assert_eq!(rules.status, RobotsStatus::Found);
        assert!(!rules.allows("https://a.test/no-crawlstat/x"));
        // The wildcard group's rules do not apply once a named group matches.
        assert!(rules.allows("https://a.test/private/secret"));
        assert_eq!(rules.sitemaps, vec!["https://a.test/sitemap.xml"]);
    }

    #[test]
    fn wildcard_group_applies_to_other_agents() {
        let rules = RobotsRules::parse(ROBOTS.as_bytes(), "SomeOtherBot");
        assert!(!rules.allows("https://a.test/private/secret"));
        assert!(
            !rules.allows("https://a.test/brochure.pdf"),
            "pattern with $ anchor"
        );
        assert!(rules.allows("https://a.test/public"));
        assert_eq!(rules.crawl_delay, Some(Duration::from_secs(2)));
    }

    #[test]
    fn absurd_crawl_delays_are_capped() {
        let rules = RobotsRules::parse(b"User-agent: *\nCrawl-delay: 86400\n", "AnyBot");
        assert_eq!(rules.crawl_delay, Some(Duration::from_secs(60)));
    }

    #[test]
    fn empty_robots_allows_everything() {
        let rules = RobotsRules::parse(b"", "AnyBot");
        assert!(rules.allows("https://a.test/anything"));
        assert_eq!(rules.crawl_delay, None);
    }

    #[tokio::test]
    async fn rules_are_fetched_once_per_origin() {
        let fetcher = MockFetcher::new();
        fetcher.insert("https://a.test/robots.txt", robots_response(ROBOTS));
        let cache = RobotsCache::new("CrawlStatBot", true);

        let url = Url::parse("https://a.test/page").unwrap();
        for _ in 0..5 {
            assert!(cache.rules_for(&url, &fetcher).await.allows(url.as_str()));
        }
        assert_eq!(fetcher.times_requested("https://a.test/robots.txt"), 1);
    }

    #[tokio::test]
    async fn each_origin_is_cached_separately() {
        let fetcher = MockFetcher::new();
        fetcher.insert(
            "https://a.test/robots.txt",
            robots_response("User-agent: *\nDisallow: /x"),
        );
        fetcher.insert(
            "https://b.test/robots.txt",
            robots_response("User-agent: *\nAllow: /"),
        );
        let cache = RobotsCache::new("CrawlStatBot", true);

        let a = Url::parse("https://a.test/x").unwrap();
        let b = Url::parse("https://b.test/x").unwrap();
        assert!(!cache.rules_for(&a, &fetcher).await.allows(a.as_str()));
        assert!(cache.rules_for(&b, &fetcher).await.allows(b.as_str()));
        assert_eq!(cache.known_origins().await.len(), 2);
    }

    #[tokio::test]
    async fn a_missing_robots_txt_allows_everything() {
        let fetcher = MockFetcher::new(); // unknown URLs 404
        let cache = RobotsCache::new("CrawlStatBot", true);
        let url = Url::parse("https://a.test/page").unwrap();
        let rules = cache.rules_for(&url, &fetcher).await;
        assert_eq!(rules.status, RobotsStatus::NotFound);
        assert!(rules.allows(url.as_str()));
    }

    #[tokio::test]
    async fn a_broken_robots_txt_allows_the_crawl_with_a_warning() {
        for failure in [
            MockResponse::status(500),
            MockResponse::failure(FetchError::Connect("refused".into())),
        ] {
            let fetcher = MockFetcher::new();
            fetcher.insert("https://a.test/robots.txt", failure);
            let cache = RobotsCache::new("CrawlStatBot", true);
            let url = Url::parse("https://a.test/page").unwrap();
            let rules = cache.rules_for(&url, &fetcher).await;
            assert_eq!(rules.status, RobotsStatus::Unavailable);
            assert!(
                rules.allows(url.as_str()),
                "a broken robots.txt must not block the crawl"
            );
        }
    }

    #[tokio::test]
    async fn disabling_robots_skips_the_fetch_entirely() {
        let fetcher = MockFetcher::new();
        fetcher.insert(
            "https://a.test/robots.txt",
            robots_response("User-agent: *\nDisallow: /"),
        );
        let cache = RobotsCache::new("CrawlStatBot", false);

        let url = Url::parse("https://a.test/private/x").unwrap();
        let rules = cache.rules_for(&url, &fetcher).await;
        assert_eq!(rules.status, RobotsStatus::Ignored);
        assert!(rules.allows(url.as_str()));
        assert_eq!(fetcher.request_count(), 0, "no robots.txt request at all");
    }

    #[test]
    fn origin_keys_include_a_non_default_port() {
        let origin = |u: &str| RobotsCache::origin_of(&Url::parse(u).unwrap());
        assert_eq!(origin("https://a.test/x?y=1"), "https://a.test");
        assert_eq!(origin("http://a.test:8080/x"), "http://a.test:8080");
        assert_eq!(origin("https://a.test:443/x"), "https://a.test");
        // Scheme is part of the origin: http and https have separate rules.
        assert_ne!(origin("http://a.test/"), origin("https://a.test/"));
    }
}
