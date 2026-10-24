//! Crawl-loop tests.
//!
//! These use [`MockFetcher`] rather than a real server, so they are fast and
//! deterministic and can assert on exactly which URLs were requested. The
//! fixture site in `tests/` covers the HTTP layer end to end.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use futures::future::BoxFuture;
use tokio::sync::mpsc;

use super::*;
use crate::config::{CrawlConfig, Seed};
use crate::error::FetchError;
use crate::fetch::mock::{MockFetcher, MockResponse};
use crate::fetch::{FetchRequest, FetchedResponse};
use crate::model::UrlState;
use crate::store::queries::{self, SortBy, UrlFilter};
use crate::store::Store;

/// A four-page site with a nav menu, an image, an external link and a 404.
fn site() -> Arc<MockFetcher> {
    let fetcher = Arc::new(MockFetcher::new());
    fetcher.page(
        "https://a.test/",
        r#"<html><head><title>Home</title></head><body>
             <nav><a href="/about">About</a><a href="/blog/">Blog</a></nav>
             <p>Welcome to the site, it has words on it.</p>
             <img src="/logo.png" alt="Logo">
             <a href="https://external.test/page">External</a>
             <a href="/gone">Broken</a>
           </body></html>"#,
    );
    fetcher.page(
        "https://a.test/about",
        r#"<html><head><title>About</title>
             <meta name="description" content="About us"></head>
           <body><h1>About</h1><a href="/">Home</a><a href="/team">Team</a></body></html>"#,
    );
    fetcher.page(
        "https://a.test/blog/",
        r#"<html><head><title>Blog</title></head>
           <body><a href="/">Home</a><a href="/blog/post-1">Post</a></body></html>"#,
    );
    fetcher.page(
        "https://a.test/blog/post-1",
        r#"<html><head><title>Post 1</title></head><body><h1>Post</h1><p>Words.</p></body></html>"#,
    );
    fetcher.page(
        "https://a.test/team",
        r#"<html><head><title>Team</title></head><body><h1>Team</h1></body></html>"#,
    );
    fetcher.insert("https://a.test/gone", MockResponse::status(404));
    fetcher.insert(
        "https://a.test/logo.png",
        MockResponse {
            status: 200,
            content_type: "image/png".into(),
            body: "binarydata".into(),
            redirect_to: None,
            headers: vec![],
            error: None,
        },
    );
    fetcher.insert(
        "https://external.test/page",
        MockResponse::html("<p>external</p>"),
    );
    fetcher
}

/// Widen a mock fetcher to the trait object the crawler takes.
fn dynamic(fetcher: &Arc<MockFetcher>) -> Arc<dyn crate::fetch::Fetcher> {
    let cloned: Arc<MockFetcher> = Arc::clone(fetcher);
    cloned
}

/// Run a crawl against a mock fetcher and hand back the store for queries.
async fn run(config: CrawlConfig, fetcher: Arc<MockFetcher>) -> (CrawlSummary, Store) {
    let store = Store::open_in_memory().unwrap();
    let (crawler, _handle) = Crawler::start_with_fetcher(config, store, fetcher, None)
        .await
        .unwrap();
    crawler.run_and_close().await.unwrap()
}

fn urls_in(store: &Store, filter: &UrlFilter) -> Vec<String> {
    let page = queries::list_urls(store.connection(), filter, SortBy::Url, false, 1000, 0).unwrap();
    page.rows.into_iter().map(|r| r.url).collect()
}

#[tokio::test]
async fn crawls_a_small_site_and_records_every_url_once() {
    let fetcher = site();
    let (summary, store) = run(CrawlConfig::spider("https://a.test/"), Arc::clone(&fetcher)).await;

    assert_eq!(summary.state, CrawlState::Finished);
    assert!(!summary.hit_limit);

    let crawled = urls_in(&store, &UrlFilter::default());
    assert!(crawled.contains(&"https://a.test/".to_string()));
    assert!(
        crawled.contains(&"https://a.test/blog/post-1".to_string()),
        "two levels deep"
    );
    assert!(crawled.contains(&"https://a.test/team".to_string()));
    assert!(
        crawled.contains(&"https://a.test/logo.png".to_string()),
        "images are crawled"
    );
    assert!(
        crawled.contains(&"https://external.test/page".to_string()),
        "external is checked"
    );

    // The nav links from four pages all point at "/", which must be fetched once.
    assert_eq!(fetcher.times_requested("https://a.test/"), 1);

    // The 404 is a successful fetch with an error status, not a failure.
    assert_eq!(summary.status_4xx, 1);
    assert_eq!(summary.errors, 0);

    let home = queries::url_by_address(store.connection(), "https://a.test/")
        .unwrap()
        .unwrap();
    assert_eq!(home.title.as_deref(), Some("Home"));
    assert_eq!(home.depth, 0);
    assert_eq!(home.status_code, Some(200));
    assert!(home.word_count.unwrap() > 5);

    let post = queries::url_by_address(store.connection(), "https://a.test/blog/post-1")
        .unwrap()
        .unwrap();
    assert_eq!(post.depth, 2, "home → blog → post");
    assert_eq!(post.inlinks, 1);
}

#[tokio::test]
async fn external_urls_are_checked_but_never_followed() {
    let fetcher = site();
    fetcher.page(
        "https://external.test/page",
        r#"<html><body><a href="/deeper">more</a></body></html>"#,
    );
    let (_, store) = run(CrawlConfig::spider("https://a.test/"), Arc::clone(&fetcher)).await;

    assert!(fetcher
        .requested()
        .iter()
        .any(|u| u == "https://external.test/page"));
    assert!(
        !fetcher.requested().iter().any(|u| u.contains("/deeper")),
        "links found on an external page must not be queued"
    );

    let external = urls_in(
        &store,
        &UrlFilter {
            internal: Some(false),
            ..Default::default()
        },
    );
    assert_eq!(external, vec!["https://external.test/page"]);
}

#[tokio::test]
async fn external_link_checking_can_be_turned_off() {
    let mut config = CrawlConfig::spider("https://a.test/");
    config.check_external_links = false;
    let fetcher = site();
    let (_, store) = run(config, Arc::clone(&fetcher)).await;

    assert!(!fetcher
        .requested()
        .iter()
        .any(|u| u.contains("external.test")));
    assert!(urls_in(
        &store,
        &UrlFilter {
            internal: Some(false),
            ..Default::default()
        }
    )
    .is_empty());
}

#[tokio::test]
async fn max_depth_stops_the_crawl_going_deeper() {
    let mut config = CrawlConfig::spider("https://a.test/");
    config.max_depth = Some(1);
    config.check_external_links = false;
    let (_, store) = run(config, site()).await;

    let crawled = urls_in(&store, &UrlFilter::default());
    assert!(
        crawled.contains(&"https://a.test/about".to_string()),
        "depth 1 is crawled"
    );
    assert!(
        !crawled.contains(&"https://a.test/team".to_string()),
        "depth 2 is never queued"
    );
}

#[tokio::test]
async fn the_url_limit_is_enforced_and_the_rest_marked_skipped() {
    let mut config = CrawlConfig::spider("https://a.test/");
    config.max_urls = Some(3);
    config.concurrency = 1;
    let fetcher = site();
    let (summary, store) = run(config, Arc::clone(&fetcher)).await;

    assert!(summary.hit_limit);
    assert_eq!(summary.crawled, 3, "the limit is exact, never overshot");

    let stats = store.stats().unwrap();
    assert_eq!(stats.done + stats.failed, 3);
    assert!(
        stats.skipped > 0,
        "URLs found but not crawled stay visible as skipped"
    );
    assert_eq!(stats.queued, 0);
}

#[tokio::test]
async fn robots_txt_is_obeyed_and_the_blocked_url_is_recorded() {
    let fetcher = site();
    fetcher.insert(
        "https://a.test/robots.txt",
        MockResponse {
            status: 200,
            content_type: "text/plain".into(),
            body: "User-agent: *\nDisallow: /blog/\n".into(),
            redirect_to: None,
            headers: vec![],
            error: None,
        },
    );
    let (_, store) = run(CrawlConfig::spider("https://a.test/"), Arc::clone(&fetcher)).await;

    assert!(
        !fetcher
            .requested()
            .iter()
            .any(|u| u.starts_with("https://a.test/blog")),
        "a disallowed URL must never be fetched"
    );

    let blocked = queries::list_urls(
        store.connection(),
        &UrlFilter {
            state: Some(UrlState::BlockedByRobots),
            ..Default::default()
        },
        SortBy::Url,
        false,
        10,
        0,
    )
    .unwrap();
    assert_eq!(blocked.rows.len(), 1);
    assert_eq!(blocked.rows[0].url, "https://a.test/blog/");
    assert_eq!(
        blocked.rows[0].error_kind.as_deref(),
        Some("blocked_by_robots")
    );
}

#[tokio::test]
async fn robots_txt_can_be_ignored_on_request() {
    let fetcher = site();
    fetcher.insert(
        "https://a.test/robots.txt",
        MockResponse {
            status: 200,
            content_type: "text/plain".into(),
            body: "User-agent: *\nDisallow: /\n".into(),
            redirect_to: None,
            headers: vec![],
            error: None,
        },
    );
    let mut config = CrawlConfig::spider("https://a.test/");
    config.respect_robots = false;
    let (_, store) = run(config, Arc::clone(&fetcher)).await;

    assert!(
        !fetcher
            .requested()
            .iter()
            .any(|u| u.ends_with("/robots.txt")),
        "robots.txt is not even fetched when disabled"
    );
    assert!(store.stats().unwrap().done > 3);
}

#[tokio::test]
async fn nofollow_links_are_not_followed_by_default() {
    let fetcher = Arc::new(MockFetcher::new());
    fetcher.page(
        "https://a.test/",
        r#"<a href="/followed">yes</a><a href="/skipped" rel="nofollow">no</a>"#,
    );
    fetcher.page("https://a.test/followed", "<p>ok</p>");
    fetcher.page("https://a.test/skipped", "<p>ok</p>");

    let mut config = CrawlConfig::spider("https://a.test/");
    config.check_external_links = false;
    let (_, store) = run(config.clone(), Arc::clone(&fetcher)).await;
    let crawled = urls_in(&store, &UrlFilter::default());
    assert!(crawled.contains(&"https://a.test/followed".to_string()));
    assert!(!crawled.contains(&"https://a.test/skipped".to_string()));

    // The link itself is still recorded, so the UI can show it.
    let home = queries::url_by_address(store.connection(), "https://a.test/")
        .unwrap()
        .unwrap();
    let outlinks = queries::outlinks(store.connection(), home.id, 10).unwrap();
    assert!(outlinks.iter().any(|l| l.nofollow));

    // And the toggle works.
    config.follow_nofollow = true;
    let (_, store) = run(config, Arc::clone(&fetcher)).await;
    assert!(urls_in(&store, &UrlFilter::default()).contains(&"https://a.test/skipped".to_string()));
}

#[tokio::test]
async fn include_and_exclude_patterns_shape_the_crawl() {
    let mut config = CrawlConfig::spider("https://a.test/");
    config.exclude_patterns = vec![r"/blog".into()];
    config.check_external_links = false;
    let (_, store) = run(config, site()).await;

    let crawled = urls_in(&store, &UrlFilter::default());
    assert!(!crawled.iter().any(|u| u.contains("/blog")));
    assert!(crawled.contains(&"https://a.test/about".to_string()));
}

#[tokio::test]
async fn the_resource_policy_decides_which_assets_are_fetched() {
    let fetcher = Arc::new(MockFetcher::new());
    fetcher.page(
        "https://a.test/",
        r#"<link rel="stylesheet" href="/site.css">
           <script src="/app.js"></script>
           <img src="/pic.png" alt="x">
           <a href="/doc.pdf">Download</a>"#,
    );
    for asset in ["/site.css", "/app.js", "/pic.png", "/doc.pdf"] {
        fetcher.insert(&format!("https://a.test{asset}"), MockResponse::status(200));
    }

    let mut config = CrawlConfig::spider("https://a.test/");
    config.check_external_links = false;
    let (_, store) = run(config, Arc::clone(&fetcher)).await;

    let crawled = urls_in(&store, &UrlFilter::default());
    // Defaults: images and documents yes, CSS and JS no.
    assert!(crawled.contains(&"https://a.test/pic.png".to_string()));
    assert!(crawled.contains(&"https://a.test/doc.pdf".to_string()));
    assert!(!crawled.contains(&"https://a.test/site.css".to_string()));
    assert!(!crawled.contains(&"https://a.test/app.js".to_string()));
}

#[tokio::test]
async fn redirect_chains_are_recorded_and_their_target_queued() {
    let fetcher = Arc::new(MockFetcher::new());
    fetcher.page("https://a.test/", r#"<a href="/old">Old page</a>"#);
    fetcher.insert("https://a.test/old", MockResponse::redirect(301, "/middle"));
    fetcher.insert("https://a.test/middle", MockResponse::redirect(302, "/new"));
    fetcher.page("https://a.test/new", "<h1>New</h1>");

    let mut config = CrawlConfig::spider("https://a.test/");
    config.check_external_links = false;
    let (_, store) = run(config, Arc::clone(&fetcher)).await;

    let old = queries::url_by_address(store.connection(), "https://a.test/old")
        .unwrap()
        .unwrap();
    assert_eq!(old.redirect_count, 2);
    let chain = queries::redirect_chain(store.connection(), old.id).unwrap();
    assert_eq!(chain.len(), 2);
    assert_eq!(chain[0].3, 301);

    // The destination is crawled in its own right...
    let new = queries::url_by_address(store.connection(), "https://a.test/new")
        .unwrap()
        .unwrap();
    assert_eq!(new.status_code, Some(200));
    // ...at the same depth as the redirect, because a redirect is not a hop.
    assert_eq!(old.depth, 1);
    assert_eq!(new.depth, 1);
}

#[tokio::test]
async fn a_redirect_loop_is_detected_rather_than_followed_forever() {
    let fetcher = Arc::new(MockFetcher::new());
    fetcher.insert("https://a.test/", MockResponse::redirect(301, "/b"));
    fetcher.insert("https://a.test/b", MockResponse::redirect(301, "/"));

    let (summary, store) = run(CrawlConfig::spider("https://a.test/"), fetcher).await;
    assert_eq!(summary.state, CrawlState::Finished);

    let entries = store.log_entries().unwrap();
    assert!(
        entries
            .iter()
            .any(|(level, message, _)| level == "warning" && message.contains("loop")),
        "the loop should be logged: {entries:?}"
    );
}

#[tokio::test]
async fn list_mode_fetches_only_the_given_urls() {
    let fetcher = site();
    let config = CrawlConfig::list(vec![
        "https://a.test/about".into(),
        "https://a.test/team".into(),
    ]);
    let (summary, store) = run(config, Arc::clone(&fetcher)).await;

    assert_eq!(summary.crawled, 2);
    let crawled = urls_in(&store, &UrlFilter::default());
    assert_eq!(crawled, vec!["https://a.test/about", "https://a.test/team"]);

    // Links are still parsed and stored, just not followed.
    let about = queries::url_by_address(store.connection(), "https://a.test/about")
        .unwrap()
        .unwrap();
    assert!(!queries::outlinks(store.connection(), about.id, 10)
        .unwrap()
        .is_empty());
}

#[tokio::test]
async fn a_sitemap_seed_queues_every_url_it_declares() {
    let fetcher = site();
    fetcher.insert(
        "https://a.test/sitemap.xml",
        MockResponse {
            status: 200,
            content_type: "application/xml".into(),
            body: r#"<?xml version="1.0"?><sitemapindex>
                       <sitemap><loc>https://a.test/sitemap-pages.xml</loc></sitemap>
                     </sitemapindex>"#
                .into(),
            redirect_to: None,
            headers: vec![],
            error: None,
        },
    );
    fetcher.insert(
        "https://a.test/sitemap-pages.xml",
        MockResponse {
            status: 200,
            content_type: "application/xml".into(),
            body: r#"<?xml version="1.0"?><urlset>
                       <url><loc>https://a.test/team</loc><lastmod>2026-01-01</lastmod></url>
                       <url><loc>https://a.test/orphan</loc></url>
                     </urlset>"#
                .into(),
            redirect_to: None,
            headers: vec![],
            error: None,
        },
    );
    fetcher.page("https://a.test/orphan", "<h1>Orphan</h1>");

    let config = CrawlConfig {
        seeds: vec![Seed::Sitemap {
            url: "https://a.test/sitemap.xml".into(),
        }],
        check_external_links: false,
        ..Default::default()
    };
    let (summary, store) = run(config, Arc::clone(&fetcher)).await;

    assert_eq!(summary.crawled, 2, "both sitemap URLs, via the index");
    let crawled = urls_in(&store, &UrlFilter::default());
    assert!(crawled.contains(&"https://a.test/orphan".to_string()));

    // Sitemap membership is recorded for the orphan-page audit.
    let declared = queries::sitemap_urls(store.connection(), 100).unwrap();
    assert_eq!(declared.len(), 2);
    assert!(declared.iter().all(|(_, id)| id.is_some()));
}

#[tokio::test]
async fn a_broken_sitemap_warns_but_does_not_stop_the_crawl() {
    let fetcher = site();
    fetcher.insert(
        "https://a.test/sitemap.xml",
        MockResponse {
            status: 200,
            content_type: "application/xml".into(),
            body: "<rss><channel/></rss>".into(),
            redirect_to: None,
            headers: vec![],
            error: None,
        },
    );
    let mut config = CrawlConfig::spider("https://a.test/");
    config.seeds.push(Seed::Sitemap {
        url: "https://a.test/sitemap.xml".into(),
    });
    let (summary, store) = run(config, fetcher).await;

    assert_eq!(summary.state, CrawlState::Finished);
    assert!(summary.crawled > 3, "the start URL was still crawled");
    assert!(store
        .log_entries()
        .unwrap()
        .iter()
        .any(|(level, message, _)| level == "warning" && message.contains("sitemap")));
}

#[tokio::test]
async fn seeds_that_cannot_be_used_are_rejected_before_the_crawl_starts() {
    let store = Store::open_in_memory().unwrap();
    let config = CrawlConfig {
        seeds: vec![Seed::List {
            urls: vec!["mailto:someone@a.test".into()],
        }],
        ..Default::default()
    };

    // Fail fast: a crawl with no usable seed never reaches the loop.
    let err = Crawler::start_with_fetcher(config, store, Arc::new(MockFetcher::new()), None)
        .await
        .unwrap_err();
    assert!(err.to_string().contains("no usable seed"), "got: {err}");
}

#[tokio::test]
async fn a_seed_that_is_unreachable_still_produces_a_finished_crawl() {
    // The seed parses but 404s: that is a result, not a configuration error.
    let fetcher = Arc::new(MockFetcher::new());
    let mut config = CrawlConfig::spider("https://a.test/missing");
    config.check_external_links = false;
    let (summary, store) = run(config, fetcher).await;

    assert_eq!(summary.state, CrawlState::Finished);
    assert_eq!(summary.status_4xx, 1);
    assert_eq!(store.stats().unwrap().done, 1);
}

#[tokio::test]
async fn progress_events_describe_the_crawl() {
    let (tx, mut rx) = mpsc::channel(1000);
    let store = Store::open_in_memory().unwrap();
    let mut config = CrawlConfig::spider("https://a.test/");
    config.check_external_links = false;

    let (crawler, _handle) = Crawler::start_with_fetcher(config, store, site(), Some(tx))
        .await
        .unwrap();
    let summary = crawler.run().await.unwrap();

    let mut events = Vec::new();
    while let Ok(event) = rx.try_recv() {
        events.push(event);
    }

    assert!(matches!(
        events.first(),
        Some(ProgressEvent::StateChanged { .. })
    ));
    assert!(events
        .iter()
        .any(|e| matches!(e, ProgressEvent::Started { .. })));

    let url_events: Vec<&CrawledUrlEvent> = events
        .iter()
        .filter_map(|e| match e {
            ProgressEvent::Url(url) => Some(url),
            _ => None,
        })
        .collect();
    assert_eq!(url_events.len() as u64, summary.crawled);
    assert!(url_events
        .iter()
        .any(|u| u.title.as_deref() == Some("Home")));
    assert!(url_events.iter().any(|u| u.status_code == Some(404)));
    assert!(url_events.iter().any(|u| u.depth == 2));

    match events.last() {
        Some(ProgressEvent::Finished { summary: finished }) => {
            assert_eq!(finished.state, CrawlState::Finished);
            assert_eq!(finished.crawled, summary.crawled);
        }
        other => panic!("expected Finished last, got {other:?}"),
    }
}

/// Fails the first `fail_times` requests to a URL, then serves the real page.
struct FlakyFetcher {
    inner: Arc<MockFetcher>,
    fail_times: usize,
    seen: AtomicUsize,
}

impl crate::fetch::Fetcher for FlakyFetcher {
    fn fetch<'a>(
        &'a self,
        request: FetchRequest,
    ) -> BoxFuture<'a, std::result::Result<FetchedResponse, FetchError>> {
        Box::pin(async move {
            if request.url.path() == "/flaky"
                && self.seen.fetch_add(1, Ordering::SeqCst) < self.fail_times
            {
                return Err(FetchError::Connect("connection reset".into()));
            }
            self.inner.fetch(request).await
        })
    }
}

#[tokio::test]
async fn a_transient_failure_is_retried() {
    let inner = Arc::new(MockFetcher::new());
    inner.page("https://a.test/", r#"<a href="/flaky">Flaky</a>"#);
    inner.page("https://a.test/flaky", "<h1>Eventually fine</h1>");

    let fetcher = Arc::new(FlakyFetcher {
        inner: Arc::clone(&inner),
        fail_times: 2,
        seen: AtomicUsize::new(0),
    });

    let mut config = CrawlConfig::spider("https://a.test/");
    config.max_retries = 2;
    config.retry_backoff_ms = 1;
    config.check_external_links = false;

    let store = Store::open_in_memory().unwrap();
    let (crawler, _handle) = Crawler::start_with_fetcher(config, store, fetcher, None)
        .await
        .unwrap();
    let (summary, store) = crawler.run_and_close().await.unwrap();

    assert_eq!(summary.errors, 0, "the retry should have succeeded");
    let flaky = queries::url_by_address(store.connection(), "https://a.test/flaky")
        .unwrap()
        .unwrap();
    assert_eq!(flaky.status_code, Some(200));
}

#[tokio::test]
async fn a_permanent_failure_is_recorded_after_its_retries() {
    let inner = Arc::new(MockFetcher::new());
    inner.page("https://a.test/", r#"<a href="/flaky">Flaky</a>"#);
    let fetcher = Arc::new(FlakyFetcher {
        inner,
        fail_times: usize::MAX,
        seen: AtomicUsize::new(0),
    });

    let mut config = CrawlConfig::spider("https://a.test/");
    config.max_retries = 1;
    config.retry_backoff_ms = 1;
    config.check_external_links = false;

    let store = Store::open_in_memory().unwrap();
    let (crawler, _handle) = Crawler::start_with_fetcher(config, store, fetcher, None)
        .await
        .unwrap();
    let (summary, store) = crawler.run_and_close().await.unwrap();

    assert_eq!(summary.errors, 1);
    let failed = queries::url_by_address(store.connection(), "https://a.test/flaky")
        .unwrap()
        .unwrap();
    assert_eq!(failed.error_kind.as_deref(), Some("connect"));
    assert_eq!(failed.status_code, None);
}

#[tokio::test]
async fn cancelling_stops_the_crawl_and_leaves_it_resumable() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("crawl.db");

    let fetcher = site();
    let mut config = CrawlConfig::spider("https://a.test/");
    config.concurrency = 1;
    // Slow the crawl down so there is a middle to cancel in.
    config.crawl_delay_ms = 50;

    let store = Store::open(&path).unwrap();
    let (crawler, handle) = Crawler::start_with_fetcher(config, store, dynamic(&fetcher), None)
        .await
        .unwrap();

    let task = tokio::spawn(crawler.run());
    // Let the first page come back, then stop.
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    handle.cancel();
    let summary = task.await.unwrap().unwrap();

    assert_eq!(summary.state, CrawlState::Cancelled);
    assert!(summary.crawled >= 1);

    // Queued URLs are untouched, so the crawl can be picked up again.
    let store = Store::open(&path).unwrap();
    assert_eq!(store.meta("state").unwrap().as_deref(), Some("cancelled"));
    let before = store.stats().unwrap();
    assert!(
        before.queued > 0,
        "cancelling must not discard the frontier"
    );
    drop(store);

    let store = Store::open(&path).unwrap();
    let (crawler, _handle) = Crawler::resume_with_fetcher(store, dynamic(&fetcher), None)
        .await
        .unwrap();
    let (resumed, store) = crawler.run_and_close().await.unwrap();

    assert_eq!(resumed.state, CrawlState::Finished);
    assert_eq!(store.stats().unwrap().queued, 0);
    // Nothing was fetched twice.
    assert_eq!(fetcher.times_requested("https://a.test/"), 1);
}

#[tokio::test]
async fn pausing_halts_claiming_until_resumed() {
    let fetcher = site();
    let mut config = CrawlConfig::spider("https://a.test/");
    config.concurrency = 1;
    config.crawl_delay_ms = 30;

    let store = Store::open_in_memory().unwrap();
    let (tx, mut rx) = mpsc::channel(1000);
    let (crawler, handle) = Crawler::start_with_fetcher(config, store, dynamic(&fetcher), Some(tx))
        .await
        .unwrap();

    let task = tokio::spawn(crawler.run());
    tokio::time::sleep(std::time::Duration::from_millis(40)).await;
    handle.pause();
    assert!(handle.is_paused());

    // Wait for the pause to take effect, then confirm the crawl has stopped
    // making requests.
    tokio::time::sleep(std::time::Duration::from_millis(80)).await;
    let during_pause = fetcher.request_count();
    tokio::time::sleep(std::time::Duration::from_millis(80)).await;
    assert_eq!(
        fetcher.request_count(),
        during_pause,
        "a paused crawl makes no requests"
    );

    handle.resume();
    let summary = task.await.unwrap().unwrap();
    assert_eq!(summary.state, CrawlState::Finished);
    assert!(fetcher.request_count() > during_pause);

    let mut events = Vec::new();
    while let Ok(event) = rx.try_recv() {
        events.push(event);
    }
    assert!(events.iter().any(|e| matches!(
        e,
        ProgressEvent::StateChanged {
            state: CrawlState::Paused
        }
    )));
}

#[tokio::test]
async fn an_interrupted_crawl_requeues_what_was_in_flight() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("crawl.db");

    // Simulate a crash: queue three URLs, claim two, then drop the store.
    {
        let mut store = Store::open(&path).unwrap();
        store
            .initialise_crawl(&CrawlConfig::spider("https://a.test/"), "a.test")
            .unwrap();
        let seeds: Vec<_> = [
            "https://a.test/",
            "https://a.test/about",
            "https://a.test/team",
        ]
        .iter()
        .map(|u| crate::model::FrontierUrl {
            url: (*u).to_string(),
            key: (*u).to_string(),
            depth: 0,
            discovered_from: None,
            via: None,
            kind: crate::model::ResourceKind::Html,
            is_internal: true,
            state: UrlState::Queued,
        })
        .collect();
        store.enqueue(&seeds).unwrap();
        store.claim(2).unwrap();
        assert_eq!(store.stats().unwrap().in_progress, 2);
    }

    let store = Store::open(&path).unwrap();
    let fetcher = site();
    let (crawler, _handle) = Crawler::resume_with_fetcher(store, dynamic(&fetcher), None)
        .await
        .unwrap();
    let (summary, store) = crawler.run_and_close().await.unwrap();

    assert_eq!(summary.state, CrawlState::Finished);
    let stats = store.stats().unwrap();
    assert_eq!(stats.in_progress, 0);
    assert_eq!(stats.queued, 0);
    // All three originally queued URLs were visited.
    for url in [
        "https://a.test/",
        "https://a.test/about",
        "https://a.test/team",
    ] {
        assert_eq!(fetcher.times_requested(url), 1, "{url}");
    }
}

#[tokio::test]
async fn subdomains_are_out_of_scope_unless_asked_for() {
    let fetcher = Arc::new(MockFetcher::new());
    fetcher.page(
        "https://a.test/",
        r#"<a href="https://blog.a.test/post">Blog</a>"#,
    );
    fetcher.page("https://blog.a.test/post", r#"<a href="/deeper">more</a>"#);
    fetcher.page("https://blog.a.test/deeper", "<p>deep</p>");

    // Off: the subdomain is an external link, checked but not followed.
    let (_, store) = run(CrawlConfig::spider("https://a.test/"), Arc::clone(&fetcher)).await;
    let crawled = urls_in(&store, &UrlFilter::default());
    assert!(crawled.contains(&"https://blog.a.test/post".to_string()));
    assert!(!crawled.contains(&"https://blog.a.test/deeper".to_string()));

    // On: it is part of the site.
    let mut config = CrawlConfig::spider("https://a.test/");
    config.crawl_subdomains = true;
    let (_, store) = run(config, Arc::clone(&fetcher)).await;
    assert!(
        urls_in(&store, &UrlFilter::default()).contains(&"https://blog.a.test/deeper".to_string())
    );
}

#[tokio::test]
async fn tracking_parameters_do_not_create_duplicate_crawls() {
    let fetcher = Arc::new(MockFetcher::new());
    fetcher.page(
        "https://a.test/",
        r#"<a href="/p?utm_source=nav">P</a>
           <a href="/p?utm_source=footer">P again</a>
           <a href="/p">P plain</a>"#,
    );
    fetcher.page("https://a.test/p?utm_source=nav", "<h1>P</h1>");
    let mut config = CrawlConfig::spider("https://a.test/");
    config.check_external_links = false;
    let (summary, _) = run(config, Arc::clone(&fetcher)).await;

    assert_eq!(summary.crawled, 2, "the home page and one /p");
    // The first-seen form is the one fetched, verbatim.
    assert_eq!(
        fetcher.times_requested("https://a.test/p?utm_source=nav"),
        1
    );
}

#[tokio::test]
async fn finalised_crawls_expose_link_counts_for_the_ui() {
    let mut config = CrawlConfig::spider("https://a.test/");
    config.check_external_links = false;
    let (_, store) = run(config, site()).await;

    let home = queries::url_by_address(store.connection(), "https://a.test/")
        .unwrap()
        .unwrap();
    // Anchors only: /about, /blog/ and /gone. The <img> is not a link.
    assert_eq!(home.internal_links, Some(3));
    assert_eq!(home.external_links, Some(1));
    assert_eq!(home.inlinks, 2, "/about and /blog/ both link home");

    assert!(!queries::depth_distribution(store.connection())
        .unwrap()
        .is_empty());
    assert!(!queries::status_code_summary(store.connection())
        .unwrap()
        .is_empty());
}

// ─── licence enforcement ─────────────────────────────────────────────────

#[tokio::test]
async fn an_unlicensed_crawl_is_capped_by_the_engine_not_by_the_ui() {
    // Nothing in the configuration asks for a limit, and nothing in the UI is
    // involved: the engine applies the free-tier cap itself.
    let _guard = crate::license::testing::guard().await;
    crate::license::set_current(crate::license::LicenseStatus::Free);

    let mut config = CrawlConfig::spider("https://a.test/");
    config.max_urls = None;

    let store = Store::open_in_memory().unwrap();
    let (crawler, _handle) = Crawler::start_with_fetcher(config, store, site(), None)
        .await
        .unwrap();
    let (_, store) = crawler.run_and_close().await.unwrap();

    // The clamp is recorded with the crawl, so resuming it stays limited too.
    assert_eq!(
        store.stored_config().unwrap().unwrap().max_urls,
        Some(crate::config::FREE_TIER_MAX_URLS)
    );
}

#[tokio::test]
async fn a_smaller_requested_limit_is_still_honoured_on_the_free_tier() {
    let _guard = crate::license::testing::guard().await;
    crate::license::set_current(crate::license::LicenseStatus::Free);

    let mut config = CrawlConfig::spider("https://a.test/");
    config.max_urls = Some(2);
    config.concurrency = 1;

    let store = Store::open_in_memory().unwrap();
    let (crawler, _handle) = Crawler::start_with_fetcher(config, store, site(), None)
        .await
        .unwrap();
    let (summary, store) = crawler.run_and_close().await.unwrap();

    assert_eq!(summary.crawled, 2, "the user's own smaller limit applies");
    assert_eq!(store.stored_config().unwrap().unwrap().max_urls, Some(2));
}

#[tokio::test]
async fn a_licensed_crawl_is_not_capped() {
    let _guard = crate::license::testing::guard().await;

    // A Pro entitlement granting unlimited URLs.
    let now = crate::license::now_seconds();
    crate::license::set_current(crate::license::LicenseStatus::Active {
        entitlement: crate::license::Entitlement {
            version: 1,
            plan: crate::license::Plan::Pro,
            issued_at: now,
            expires_at: now + 14 * 86_400,
            key_fingerprint: "abcd1234".into(),
            instance_id: "test".into(),
            features: vec![crate::license::features::UNLIMITED_URLS.to_string()],
        },
        days_remaining: 14,
    });

    let config = CrawlConfig::spider("https://a.test/");
    let store = Store::open_in_memory().unwrap();
    let (crawler, _handle) = Crawler::start_with_fetcher(config, store, site(), None)
        .await
        .unwrap();
    let (summary, store) = crawler.run_and_close().await.unwrap();

    assert_eq!(store.stored_config().unwrap().unwrap().max_urls, None);
    assert!(!summary.hit_limit);

    crate::license::set_current(crate::license::LicenseStatus::Free);
}
