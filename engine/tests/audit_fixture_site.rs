//! End-to-end audits: crawl the live fixture site, then audit what was found.
//!
//! The unit tests in `src/audit/tests.rs` insert rows directly, which makes
//! them precise but means they never prove that a *real* crawl produces the
//! field values the audits read. These tests close that gap: everything here
//! comes from real HTTP responses parsed by the real parser.

mod fixture;

use std::sync::Arc;

use crawlstat_engine::audit::{AuditConfig, Severity};
use crawlstat_engine::config::CrawlConfig;
use crawlstat_engine::engine::Crawler;
use crawlstat_engine::fetch::HttpFetcher;
use crawlstat_engine::store::queries::{self, SortBy, UrlFilter};
use crawlstat_engine::store::Store;

/// A crawled fixture site, audited, with helpers to ask about the findings.
struct Audited {
    site: fixture::FixtureSite,
    store: Store,
    report: crawlstat_engine::audit::AuditReport,
}

impl Audited {
    /// Crawl the whole fixture site except the generated pages, then audit.
    async fn new(tune: impl FnOnce(&mut AuditConfig)) -> Self {
        let site = fixture::spawn().await;

        let mut config = CrawlConfig::spider(site.url("/"));
        config.concurrency = 8;
        config.concurrency_per_host = 8;
        // The generated binary tree is for the performance tests; it would
        // swamp the findings here.
        config.exclude_patterns = vec![r"/page/".into()];
        config.check_external_links = false;
        // The audit pass is driven explicitly below, so thresholds can be
        // tuned per test.
        config.run_audits = false;
        tune(&mut config.audits);
        let audit_config = config.audits.clone();

        let compiled = config.compile().unwrap();
        let fetcher = Arc::new(HttpFetcher::new(&compiled).unwrap());
        let (crawler, _handle) =
            Crawler::start_with_fetcher(config, Store::open_in_memory().unwrap(), fetcher, None)
                .await
                .unwrap();
        let (_, mut store) = crawler.run_and_close().await.unwrap();
        let report = store.audit(&audit_config).unwrap();

        Self {
            site,
            store,
            report,
        }
    }

    fn url(&self, path: &str) -> String {
        self.site.url(path)
    }

    /// Paths affected by an issue, relative to the fixture's base URL.
    fn affected(&self, issue_id: &str) -> Vec<String> {
        let base = self.site.base.clone();
        let mut paths: Vec<String> = queries::issue_details(self.store.connection(), issue_id, 500)
            .unwrap()
            .into_iter()
            .map(|(_, url, _)| url.replace(&base, ""))
            .collect();
        paths.sort();
        paths
    }

    fn detail(&self, issue_id: &str, path: &str) -> Option<String> {
        let wanted = self.url(path);
        queries::issue_details(self.store.connection(), issue_id, 500)
            .unwrap()
            .into_iter()
            .find(|(_, url, _)| *url == wanted)
            .and_then(|(_, _, detail)| detail)
    }

    fn has(&self, issue_id: &str, path: &str) -> bool {
        self.affected(issue_id).contains(&path.to_string())
    }
}

#[tokio::test]
async fn error_statuses_and_the_pages_linking_to_them() {
    let audited = Audited::new(|_| {}).await;

    assert!(audited.has("status_4xx", "/missing"));
    assert!(audited.has("status_5xx", "/server-error"));
    assert_eq!(
        audited.detail("status_5xx", "/server-error").as_deref(),
        Some("HTTP 500")
    );

    // The home page links to both, so it is the page that needs editing.
    assert!(audited.has("broken_internal_link", "/"));
    let detail = audited.detail("broken_internal_link", "/").unwrap();
    assert!(
        detail.contains("/missing") || detail.contains("/server-error"),
        "got {detail}"
    );

    for id in ["status_4xx", "status_5xx", "broken_internal_link"] {
        assert_eq!(audited.report.issue(id).unwrap().severity, Severity::Error);
    }
}

#[tokio::test]
async fn redirect_chains_loops_and_links_into_them() {
    let audited = Audited::new(|_| {}).await;

    // /redirect/1 → /redirect/2 → /redirect/3 → /about is three hops.
    assert!(audited.has("redirect_chain", "/redirect/1"));
    assert_eq!(
        audited.detail("redirect_chain", "/redirect/1").as_deref(),
        Some("3 hops")
    );
    // The last hop in the chain is a single redirect, which is normal.
    assert!(!audited.has("redirect_chain", "/redirect/3"));

    assert!(audited.has("redirect_loop", "/loop/a"));
    // The home page links straight into the chain and the loop.
    assert!(audited.has("internal_link_to_redirect", "/"));
}

#[tokio::test]
async fn title_and_description_problems_found_in_real_markup() {
    let audited = Audited::new(|_| {}).await;

    assert!(audited.has("title_missing", "/no-title"));

    // /dup-a, /dup-b and /near-dup all serve <title>Duplicate content</title>.
    let duplicates = audited.affected("title_duplicate");
    assert!(duplicates.contains(&"/dup-a".to_string()));
    assert!(duplicates.contains(&"/dup-b".to_string()));
    assert_eq!(
        audited.detail("title_duplicate", "/dup-a").as_deref(),
        Some("shared with 2 other page(s)")
    );

    // Most fixture pages have no meta description, including this one.
    assert!(audited.has("description_missing", "/contact"));
    // ...but the home page and /about do, so they are not reported.
    assert!(!audited.has("description_missing", "/"));
}

#[tokio::test]
async fn heading_problems() {
    let audited = Audited::new(|_| {}).await;
    assert!(audited.has("h1_multiple", "/multi-h1"));
    assert_eq!(
        audited.detail("h1_multiple", "/multi-h1").as_deref(),
        Some("2 H1 tags")
    );
    assert!(audited.has("h1_missing", "/no-title"));
}

#[tokio::test]
async fn canonical_and_robots_directives() {
    let audited = Audited::new(|_| {}).await;

    // /canonicalised names the home page as canonical.
    assert!(audited.has("canonicalised", "/canonicalised"));
    assert_eq!(
        audited.detail("canonicalised", "/canonicalised").unwrap(),
        format!("canonical → {}", audited.url("/"))
    );
    // The home page canonicalises to itself, so it is not reported.
    assert!(!audited.has("canonicalised", "/"));
    assert!(audited.has("canonical_missing", "/contact"));

    // Both forms of noindex are found, from the tag and from the header.
    let noindex = audited.affected("noindex_page");
    assert!(noindex.contains(&"/noindex".to_string()));
    assert!(noindex.contains(&"/x-robots".to_string()));
    assert_eq!(
        audited.detail("noindex_page", "/x-robots").as_deref(),
        Some("X-Robots-Tag")
    );
    // Both are linked from the home page.
    assert!(audited.has("noindex_internal_links", "/noindex"));

    // robots.txt disallows /secret/, and the home page links there anyway.
    assert!(audited.has("robots_blocked_but_linked", "/secret/hidden"));
}

#[tokio::test]
async fn hreflang_problems_across_a_real_language_group() {
    let audited = Audited::new(|_| {}).await;

    // The English page declares `de`, which declares nothing back.
    assert!(audited.has("hreflang_missing_return", "/hreflang/en"));
    assert!(audited
        .detail("hreflang_missing_return", "/hreflang/en")
        .unwrap()
        .contains("/hreflang/de"));

    // The French page lists its sibling but not itself.
    assert!(audited.has("hreflang_missing_self", "/hreflang/fr"));

    // `en-UK` is not a country code.
    assert!(audited.has("hreflang_invalid_code", "/hreflang/en"));
    assert!(audited
        .detail("hreflang_invalid_code", "/hreflang/en")
        .unwrap()
        .contains("GB"));
}

#[tokio::test]
async fn duplicate_and_near_duplicate_pages() {
    let audited = Audited::new(|_| {}).await;

    // /dup-a and /dup-b are byte-identical.
    let exact = audited.affected("duplicate_content");
    assert!(exact.contains(&"/dup-a".to_string()), "got {exact:?}");
    assert!(exact.contains(&"/dup-b".to_string()));

    // /near-dup is the same text plus a sentence, so it is reported as a near
    // duplicate — and not as an exact one.
    assert!(audited.has("near_duplicate_content", "/near-dup"));
    assert!(!exact.contains(&"/near-dup".to_string()));
    let detail = audited
        .detail("near_duplicate_content", "/near-dup")
        .unwrap();
    assert!(detail.contains("% similar"), "got {detail}");
    assert!(detail.contains("dup-"), "should name its partner: {detail}");
}

#[tokio::test]
async fn images_and_response_times() {
    // /slow sleeps for 400ms, so the threshold has to be below that to see it.
    let audited = Audited::new(|config| config.slow_response_ms = 200).await;

    // The home page has one image with alt text and one without.
    assert!(audited.has("image_missing_alt", "/"));
    assert_eq!(
        audited.detail("image_missing_alt", "/").as_deref(),
        Some("1 image(s)")
    );

    // /image.png is ~120 KB, over the 100 KB default.
    assert!(audited.has("image_oversized", "/image.png"));

    assert!(audited.has("slow_response", "/slow"));
    let detail = audited.detail("slow_response", "/slow").unwrap();
    assert!(detail.ends_with(" ms"), "got {detail}");
}

#[tokio::test]
async fn orphan_pages_are_found_through_the_sitemap() {
    let audited = Audited::new(|_| {}).await;

    // /orphan is in the sitemap and nothing links to it.
    assert!(audited.has("orphan_page", "/orphan"));
    assert_eq!(
        audited.detail("orphan_page", "/orphan").as_deref(),
        Some("in sitemap, no internal links")
    );
    // The start URL is never an orphan.
    assert!(!audited.has("orphan_page", "/"));
}

#[tokio::test]
async fn indexability_is_computed_for_every_crawled_page() {
    let audited = Audited::new(|_| {}).await;

    let reason = |path: &str| -> (Option<i64>, Option<String>) {
        audited
            .store
            .connection()
            .query_row(
                "SELECT indexable, indexability_reason FROM urls WHERE url = ?1",
                [audited.url(path)],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap()
    };

    assert_eq!(reason("/"), (Some(1), None), "the home page is indexable");
    assert_eq!(
        reason("/noindex").1.as_deref(),
        Some("noindex (meta robots)")
    );
    assert_eq!(
        reason("/x-robots").1.as_deref(),
        Some("noindex (X-Robots-Tag)")
    );
    assert_eq!(reason("/missing").1.as_deref(), Some("HTTP 404"));
    assert_eq!(reason("/canonicalised").1.as_deref(), Some("Canonicalised"));
    assert_eq!(reason("/redirect/1").1.as_deref(), Some("Redirected"));
    assert_eq!(
        reason("/secret/hidden").1.as_deref(),
        Some("Blocked by robots.txt")
    );
    // Not a question that applies to an image.
    assert_eq!(reason("/image.png"), (None, None));

    assert!(audited.report.indexable_pages > 5);
    assert!(audited.report.non_indexable_pages > 5);
}

#[tokio::test]
async fn link_score_ranks_the_most_linked_page_highest() {
    let audited = Audited::new(|_| {}).await;

    let score = |path: &str| -> Option<f64> {
        audited
            .store
            .connection()
            .query_row(
                "SELECT link_score FROM urls WHERE url = ?1",
                [audited.url(path)],
                |row| row.get(0),
            )
            .unwrap()
    };

    // Every page in the fixture links back to the home page.
    let home = score("/").expect("the home page should have a score");
    let contact = score("/contact").expect("a leaf page should have a score");
    assert!(
        home > contact,
        "home ({home}) should outrank /contact ({contact})"
    );

    let max: f64 = audited
        .store
        .connection()
        .query_row("SELECT MAX(link_score) FROM urls", [], |row| row.get(0))
        .unwrap();
    assert_eq!(
        max, 100.0,
        "scores are rescaled so the strongest page is 100"
    );

    // And the column is sortable, which is what the UI needs.
    let page = queries::list_urls(
        audited.store.connection(),
        &UrlFilter::html_pages(),
        SortBy::LinkScore,
        true,
        5,
        0,
    )
    .unwrap();
    let scores: Vec<f64> = page.rows.iter().filter_map(|row| row.link_score).collect();
    assert!(
        scores.windows(2).all(|pair| pair[0] >= pair[1]),
        "not sorted: {scores:?}"
    );
}

#[tokio::test]
async fn the_report_is_coherent_and_the_issue_filter_works() {
    let audited = Audited::new(|_| {}).await;
    let report = &audited.report;

    assert!(report.errors >= 3, "{:?}", report.issues);
    assert!(report.warnings >= 5);
    assert!(report.notices >= 2);
    assert!(report.total_findings() >= 20);
    assert!(!report.depth_distribution.is_empty());

    // Worst first.
    let severities: Vec<Severity> = report.issues.iter().map(|issue| issue.severity).collect();
    assert!(
        severities.windows(2).all(|pair| pair[0] <= pair[1]),
        "{severities:?}"
    );

    // Every reported issue has the same count in the table as in the report,
    // and its URLs are reachable through the filter the UI uses.
    for issue in &report.issues {
        let page = queries::list_urls(
            audited.store.connection(),
            &UrlFilter::for_issue(&issue.id),
            SortBy::Url,
            false,
            1_000,
            0,
        )
        .unwrap();
        assert_eq!(
            page.total, issue.url_count,
            "{} reports {} URLs but the filter returns {}",
            issue.id, issue.url_count, page.total
        );
        assert!(!issue.how_to_fix.is_empty());
        assert!(
            !issue.how_to_fix.contains('{'),
            "{} has an unsubstituted placeholder",
            issue.id
        );
        assert!(
            !issue.description.contains('{'),
            "{} has an unsubstituted placeholder",
            issue.id
        );
    }
}

#[tokio::test]
async fn an_audit_runs_automatically_at_the_end_of_a_crawl() {
    let site = fixture::spawn().await;

    let mut config = CrawlConfig::spider(site.url("/"));
    config.exclude_patterns = vec![r"/page/".into()];
    config.check_external_links = false;
    // The default, stated explicitly because it is what this test is about.
    config.run_audits = true;

    let compiled = config.compile().unwrap();
    let fetcher = Arc::new(HttpFetcher::new(&compiled).unwrap());
    let (crawler, _handle) =
        Crawler::start_with_fetcher(config, Store::open_in_memory().unwrap(), fetcher, None)
            .await
            .unwrap();
    let (summary, store) = crawler.run_and_close().await.unwrap();

    let audit = summary
        .audit
        .expect("the crawl summary should carry an audit summary");
    assert!(audit.errors > 0);
    assert!(audit.findings > 0);

    // The findings are in the file, readable without re-running the audit.
    let reread = store.audit_report(&AuditConfig::default()).unwrap();
    assert_eq!(reread.errors, audit.errors);
    assert!(!queries::issues(store.connection()).unwrap().is_empty());
}

#[tokio::test]
async fn skipping_the_audit_leaves_no_findings() {
    let site = fixture::spawn().await;

    let mut config = CrawlConfig::spider(site.url("/"));
    config.exclude_patterns = vec![r"/page/".into()];
    config.check_external_links = false;
    config.run_audits = false;

    let compiled = config.compile().unwrap();
    let fetcher = Arc::new(HttpFetcher::new(&compiled).unwrap());
    let (crawler, _handle) =
        Crawler::start_with_fetcher(config, Store::open_in_memory().unwrap(), fetcher, None)
            .await
            .unwrap();
    let (summary, store) = crawler.run_and_close().await.unwrap();

    assert!(summary.audit.is_none());
    assert!(queries::issues(store.connection()).unwrap().is_empty());
}
