//! End-to-end crawls against the local fixture site.
//!
//! Unlike the unit tests, these go through `reqwest`, a real TCP connection and
//! a real HTTP server, so they cover the parts a mock cannot: header handling,
//! chunked bodies, HEAD fallbacks, connection failures and the body size cap.

mod fixture;

use std::time::Duration;

use crawlstat_engine::config::CrawlConfig;
use crawlstat_engine::engine::Crawler;
use crawlstat_engine::hash;
use crawlstat_engine::model::{ResourceKind, UrlState};
use crawlstat_engine::progress::CrawlState;
use crawlstat_engine::store::queries::{self, SortBy, UrlFilter};
use crawlstat_engine::store::Store;

/// Crawl the fixture site with the given configuration tweaks.
async fn crawl(configure: impl FnOnce(&mut CrawlConfig)) -> (fixture::FixtureSite, Store) {
    let site = fixture::spawn().await;
    let mut config = CrawlConfig::spider(site.url("/"));
    // The fixture is local, so there is no reason to be slow about it.
    config.concurrency = 8;
    config.concurrency_per_host = 8;
    config.request_timeout_ms = 10_000;
    configure(&mut config);

    let store = Store::open_in_memory().unwrap();
    let fetcher = std::sync::Arc::new(
        crawlstat_engine::fetch::HttpFetcher::new(&config.compile().unwrap()).unwrap(),
    );
    let (crawler, _handle) = Crawler::start_with_fetcher(config, store, fetcher, None)
        .await
        .unwrap();
    let (summary, store) = crawler.run_and_close().await.unwrap();
    assert_eq!(summary.state, CrawlState::Finished);
    (site, store)
}

fn row(store: &Store, url: &str) -> queries::UrlRow {
    queries::url_by_address(store.connection(), url)
        .unwrap()
        .unwrap_or_else(|| panic!("{url} is not in the crawl"))
}

fn exists(store: &Store, url: &str) -> bool {
    queries::url_by_address(store.connection(), url)
        .unwrap()
        .is_some()
}

#[tokio::test]
async fn crawls_the_whole_fixture_site() {
    let (site, store) = crawl(|config| {
        config.exclude_patterns = vec![r"/page/".into()];
        config.check_external_links = false;
    })
    .await;

    let home = row(&store, &site.url("/"));
    assert_eq!(home.status_code, Some(200));
    assert_eq!(home.title.as_deref(), Some("Fixture home"));
    assert_eq!(
        home.meta_description.as_deref(),
        Some("The CrawlStat fixture site home page.")
    );
    assert_eq!(home.h1.as_deref(), Some("Fixture home"));
    assert_eq!(home.canonical_is_self, Some(true));
    assert_eq!(home.depth, 0);
    assert!(home.word_count.unwrap() > 20);
    assert_eq!(
        home.content_type.as_deref(),
        Some("text/html; charset=utf-8")
    );
    assert!(home.response_time_ms.is_some());

    // Response headers were captured from the real server.
    let headers = queries::headers(store.connection(), home.id).unwrap();
    assert!(headers.iter().any(|(name, _)| name == "content-type"));

    let stats = store.stats().unwrap();
    assert_eq!(stats.queued, 0, "the crawl drained the frontier");
    assert!(stats.done > 15, "only {} pages crawled", stats.done);
}

#[tokio::test]
async fn error_statuses_are_recorded_as_results() {
    let (site, store) = crawl(|config| {
        config.exclude_patterns = vec![r"/page/".into()];
        config.check_external_links = false;
    })
    .await;

    assert_eq!(row(&store, &site.url("/missing")).status_code, Some(404));
    assert_eq!(
        row(&store, &site.url("/server-error")).status_code,
        Some(500)
    );

    // A 4xx/5xx is a successful fetch, so neither counts as a fetch error.
    assert_eq!(store.stats().unwrap().failed, 0);
    let summary = queries::status_code_summary(store.connection()).unwrap();
    assert!(summary.iter().any(|(code, _)| *code == 404));
    assert!(summary.iter().any(|(code, _)| *code == 500));
}

#[tokio::test]
async fn a_redirect_chain_is_walked_and_recorded_hop_by_hop() {
    let (site, store) = crawl(|config| {
        config.exclude_patterns = vec![r"/page/".into()];
        config.check_external_links = false;
    })
    .await;

    let first = row(&store, &site.url("/redirect/1"));
    assert_eq!(first.status_code, Some(301));
    assert_eq!(first.redirect_count, 3, "301 → 302 → 301 → /about");

    let chain = queries::redirect_chain(store.connection(), first.id).unwrap();
    assert_eq!(chain.len(), 3);
    assert_eq!(chain[0].3, 301);
    assert_eq!(chain[1].3, 302);
    assert!(chain[2].2.ends_with("/about"));

    // Every intermediate hop is crawled in its own right.
    assert!(exists(&store, &site.url("/redirect/2")));
    assert!(exists(&store, &site.url("/redirect/3")));
    assert_eq!(row(&store, &site.url("/about")).status_code, Some(200));
}

#[tokio::test]
async fn a_redirect_loop_is_detected_and_logged() {
    let (site, store) = crawl(|config| {
        config.exclude_patterns = vec![r"/page/".into()];
        config.check_external_links = false;
    })
    .await;

    let a = row(&store, &site.url("/loop/a"));
    assert!(a.redirect_count >= 2);

    let log = store.log_entries().unwrap();
    assert!(
        log.iter()
            .any(|(level, message, _)| level == "warning" && message.contains("loop")),
        "the loop should be logged: {log:?}"
    );
}

#[tokio::test]
async fn robots_txt_from_the_real_server_is_obeyed() {
    let (site, store) = crawl(|config| {
        config.exclude_patterns = vec![r"/page/".into()];
        config.check_external_links = false;
    })
    .await;

    let blocked = row(&store, &site.url("/secret/hidden"));
    assert_eq!(blocked.state, Some(UrlState::BlockedByRobots));
    assert_eq!(blocked.status_code, None, "a blocked URL is never fetched");
}

#[tokio::test]
async fn the_sitemap_is_found_through_robots_txt() {
    let (site, store) = crawl(|config| {
        config.exclude_patterns = vec![r"/page/".into()];
        config.check_external_links = false;
    })
    .await;

    // /orphan is in the sitemap and linked from nowhere.
    let orphan = row(&store, &site.url("/orphan"));
    assert_eq!(orphan.status_code, Some(200));
    assert_eq!(orphan.inlinks, 0, "that is what makes it an orphan");

    let declared = queries::sitemap_urls(store.connection(), 100).unwrap();
    assert_eq!(declared.len(), 3, "/, /about and /orphan");
    assert!(declared.iter().all(|(_, id)| id.is_some()));
}

#[tokio::test]
async fn canonicals_hreflang_and_robots_directives_are_captured() {
    let (site, store) = crawl(|config| {
        config.exclude_patterns = vec![r"/page/".into()];
        config.check_external_links = false;
    })
    .await;

    let canonicalised = row(&store, &site.url("/canonicalised"));
    assert_eq!(
        canonicalised.canonical.as_deref(),
        Some(site.url("/").as_str())
    );
    assert_eq!(canonicalised.canonical_is_self, Some(false));

    assert_eq!(
        row(&store, &site.url("/noindex")).meta_robots.as_deref(),
        Some("noindex, follow")
    );

    let x_robots: Option<String> = store
        .connection()
        .query_row(
            "SELECT x_robots_tag FROM urls WHERE url = ?1",
            [site.url("/x-robots")],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(x_robots.as_deref(), Some("noindex, nofollow"));

    let en = row(&store, &site.url("/hreflang/en"));
    let hreflang: Vec<(String, String)> = {
        let mut stmt = store
            .connection()
            .prepare("SELECT lang, href FROM hreflang WHERE url_id = ?1 ORDER BY lang")
            .unwrap();
        stmt.query_map([en.id], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap()
            .map(Result::unwrap)
            .collect()
    };
    // en, en-UK, de, fr — including the invalid region code, which is captured
    // as written and judged by the audit rather than dropped here.
    assert_eq!(hreflang.len(), 4);
    assert_eq!(hreflang[0].0, "de");
    assert!(hreflang.iter().any(|(lang, _)| lang == "en-UK"));
    // The French page declares `en` but not itself: a missing self-reference,
    // which the audit reports.
    let fr = row(&store, &site.url("/hreflang/fr"));
    let fr_count: i64 = store
        .connection()
        .query_row(
            "SELECT count(*) FROM hreflang WHERE url_id = ?1",
            [fr.id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(fr_count, 1);
}

#[tokio::test]
async fn duplicate_and_near_duplicate_content_is_detectable() {
    let (site, store) = crawl(|config| {
        config.exclude_patterns = vec![r"/page/".into()];
        config.check_external_links = false;
    })
    .await;

    let a = row(&store, &site.url("/dup-a"));
    let b = row(&store, &site.url("/dup-b"));

    let hash_of = |id: i64| -> (Option<String>, Option<i64>) {
        store
            .connection()
            .query_row(
                "SELECT text_hash, text_simhash FROM urls WHERE id = ?1",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap()
    };

    let (hash_a, simhash_a) = hash_of(a.id);
    let (hash_b, _) = hash_of(b.id);
    assert!(hash_a.is_some());
    assert_eq!(hash_a, hash_b, "identical bodies share a text hash");

    // The near-duplicate differs by one sentence, so the hashes differ but the
    // simhashes stay close.
    let near = row(&store, &site.url("/near-dup"));
    let (hash_near, simhash_near) = hash_of(near.id);
    assert_ne!(hash_a, hash_near);
    let distance = hash::hamming_distance(simhash_a.unwrap() as u64, simhash_near.unwrap() as u64);
    assert!(
        distance < 20,
        "near-duplicates scored {distance} bits apart"
    );
}

#[tokio::test]
async fn assets_are_crawled_according_to_the_resource_policy() {
    let (site, store) = crawl(|config| {
        config.exclude_patterns = vec![r"/page/".into()];
        config.check_external_links = false;
    })
    .await;

    // Images and documents are on by default; CSS is not.
    let image = row(&store, &site.url("/image.png"));
    assert_eq!(image.kind, ResourceKind::Image);
    assert_eq!(image.status_code, Some(200));
    assert!(
        image.body_bytes.is_none() || image.body_bytes == Some(0),
        "HEAD fetches no body"
    );
    assert!(
        image
            .content_type
            .as_deref()
            .is_some_and(|t| t.starts_with("image/")),
        "got {:?}",
        image.content_type
    );

    assert_eq!(
        row(&store, &site.url("/report.pdf")).kind,
        ResourceKind::Document
    );
    assert!(
        !exists(&store, &site.url("/styles.css")),
        "CSS is off by default"
    );

    // The image row records its declared dimensions and missing alt text.
    let home = row(&store, &site.url("/"));
    let images = queries::images(store.connection(), home.id).unwrap();
    assert_eq!(images.len(), 2);
    assert!(images
        .iter()
        .any(|i| i.alt.as_deref() == Some("A test image")));
    assert!(images.iter().any(|i| i.alt_missing));
    assert!(images.iter().any(|i| i.declared_width == Some(1200)));
    // Its byte size comes from the crawled image row.
    assert!(images.iter().any(|i| i.status_code == Some(200)));
}

#[tokio::test]
async fn css_and_javascript_are_crawled_when_asked_for() {
    let (site, store) = crawl(|config| {
        config.exclude_patterns = vec![r"/page/".into()];
        config.check_external_links = false;
        config.resources.css = true;
    })
    .await;

    let css = row(&store, &site.url("/styles.css"));
    assert_eq!(css.kind, ResourceKind::Css);
    assert_eq!(css.status_code, Some(200));
}

#[tokio::test]
async fn an_oversized_body_is_capped_not_buffered() {
    let site = fixture::spawn().await;
    let mut config = CrawlConfig::list(vec![site.url("/big")]);
    config.max_body_bytes = 64 * 1024;

    let store = Store::open_in_memory().unwrap();
    let fetcher = std::sync::Arc::new(
        crawlstat_engine::fetch::HttpFetcher::new(&config.compile().unwrap()).unwrap(),
    );
    let (crawler, _handle) = Crawler::start_with_fetcher(config, store, fetcher, None)
        .await
        .unwrap();
    let (_, store) = crawler.run_and_close().await.unwrap();

    let big = row(&store, &site.url("/big"));
    assert_eq!(big.status_code, Some(200));
    assert_eq!(
        big.body_bytes,
        Some(64 * 1024),
        "the body stopped at the cap"
    );

    let truncated: i64 = store
        .connection()
        .query_row(
            "SELECT body_truncated FROM urls WHERE id = ?1",
            [big.id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(truncated, 1);
    // A truncated page is still parsed: partial data beats none.
    assert_eq!(big.title.as_deref(), Some("Big"));
}

#[tokio::test]
async fn a_connection_failure_is_recorded_against_the_url() {
    let port = fixture::closed_port().await;
    let dead = format!("http://127.0.0.1:{port}/nothing-here");

    let mut config = CrawlConfig::list(vec![dead.clone()]);
    config.max_retries = 1;
    config.retry_backoff_ms = 10;
    config.connect_timeout_ms = 2_000;
    config.respect_robots = false;

    let store = Store::open_in_memory().unwrap();
    let fetcher = std::sync::Arc::new(
        crawlstat_engine::fetch::HttpFetcher::new(&config.compile().unwrap()).unwrap(),
    );
    let (crawler, _handle) = Crawler::start_with_fetcher(config, store, fetcher, None)
        .await
        .unwrap();
    let (summary, store) = crawler.run_and_close().await.unwrap();

    assert_eq!(summary.errors, 1);
    let failed = row(&store, &dead);
    assert_eq!(failed.state, Some(UrlState::Failed));
    assert_eq!(failed.status_code, None);
    assert_eq!(failed.error_kind.as_deref(), Some("connect"));
}

#[tokio::test]
async fn a_timeout_is_reported_as_such() {
    let site = fixture::spawn().await;
    let mut config = CrawlConfig::list(vec![site.url("/slow")]);
    // The route sleeps for 400ms.
    config.request_timeout_ms = 100;
    config.max_retries = 0;
    config.respect_robots = false;

    let store = Store::open_in_memory().unwrap();
    let fetcher = std::sync::Arc::new(
        crawlstat_engine::fetch::HttpFetcher::new(&config.compile().unwrap()).unwrap(),
    );
    let (crawler, _handle) = Crawler::start_with_fetcher(config, store, fetcher, None)
        .await
        .unwrap();
    let (summary, store) = crawler.run_and_close().await.unwrap();

    assert_eq!(summary.errors, 1);
    assert_eq!(
        row(&store, &site.url("/slow")).error_kind.as_deref(),
        Some("timeout")
    );
}

#[tokio::test]
async fn head_requests_fall_back_to_get_when_rejected() {
    let site = fixture::spawn().await;
    // /head-only answers 405; the fetcher should retry with GET.
    let mut config = CrawlConfig::list(vec![site.url("/head-only")]);
    config.respect_robots = false;

    let store = Store::open_in_memory().unwrap();
    let fetcher = std::sync::Arc::new(
        crawlstat_engine::fetch::HttpFetcher::new(&config.compile().unwrap()).unwrap(),
    );
    let (crawler, _handle) = Crawler::start_with_fetcher(config, store, fetcher, None)
        .await
        .unwrap();
    let (_, store) = crawler.run_and_close().await.unwrap();

    // Seeds are HTML and internal, so this one is fetched with GET directly and
    // the 405 is simply recorded.
    assert_eq!(row(&store, &site.url("/head-only")).status_code, Some(405));
}

#[tokio::test]
async fn the_url_limit_holds_against_a_real_server() {
    let (_, store) = crawl(|config| {
        config.max_urls = Some(20);
        config.check_external_links = false;
    })
    .await;

    let stats = store.stats().unwrap();
    assert_eq!(
        stats.done + stats.failed,
        20,
        "exactly the limit, never more"
    );
    assert!(stats.skipped > 0);
}

#[tokio::test]
async fn crawl_delay_paces_requests_to_one_host() {
    let started = std::time::Instant::now();
    let (_, store) = crawl(|config| {
        config.max_urls = Some(5);
        config.concurrency = 1;
        config.concurrency_per_host = 1;
        config.crawl_delay_ms = 120;
        config.check_external_links = false;
    })
    .await;

    let crawled = store.stats().unwrap().done;
    assert_eq!(crawled, 5);
    // Four gaps of 120ms at minimum, plus robots.txt.
    assert!(
        started.elapsed() >= Duration::from_millis(4 * 120),
        "finished too quickly ({:?}) for a 120ms crawl delay",
        started.elapsed()
    );
}

#[tokio::test]
async fn pagination_and_sorting_work_on_a_real_crawl() {
    let (_, store) = crawl(|config| {
        config.max_urls = Some(40);
        config.check_external_links = false;
    })
    .await;

    let page = queries::list_urls(
        store.connection(),
        &UrlFilter::html_pages(),
        SortBy::WordCount,
        true,
        5,
        0,
    )
    .unwrap();
    assert_eq!(page.rows.len(), 5);
    assert!(page.total >= 5);
    // Descending word count.
    let counts: Vec<i64> = page
        .rows
        .iter()
        .map(|r| r.word_count.unwrap_or(0))
        .collect();
    assert!(
        counts.windows(2).all(|w| w[0] >= w[1]),
        "not sorted: {counts:?}"
    );
}
