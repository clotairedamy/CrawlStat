//! Audit tests.
//!
//! Each test builds the smallest crawl that should produce a given finding,
//! runs the full pass, and asserts on both the issue and its per-URL detail.
//! Pages are inserted directly rather than crawled, so a test can state exactly
//! the field values it is about.

use rusqlite::params;

use super::*;
use crate::config::CrawlConfig;
use crate::model::{ResourceKind, UrlState};
use crate::store::queries::{self, UrlFilter};

/// A page to insert. Defaults to a healthy, indexable HTML page, so each test
/// only states what it is testing.
#[derive(Clone)]
struct Page<'a> {
    url: &'a str,
    status: Option<i64>,
    state: UrlState,
    kind: ResourceKind,
    is_internal: bool,
    depth: i64,
    title: Option<&'a str>,
    description: Option<&'a str>,
    h1_count: i64,
    word_count: i64,
    text_hash: Option<String>,
    simhash: Option<u64>,
    canonical: Option<&'a str>,
    canonical_is_self: Option<bool>,
    meta_robots: Option<&'a str>,
    html_lang: Option<&'a str>,
    x_robots_tag: Option<&'a str>,
    redirect_count: i64,
    redirect_loop: bool,
    response_ms: Option<i64>,
    body_bytes: Option<i64>,
    error_kind: Option<&'a str>,
    /// A URL the crawl was pointed at, rather than one it discovered. Seeds are
    /// never orphans, however few links point at them.
    is_seed: bool,
}

impl Default for Page<'_> {
    fn default() -> Self {
        Self {
            url: "https://a.test/",
            status: Some(200),
            state: UrlState::Done,
            kind: ResourceKind::Html,
            is_internal: true,
            depth: 1,
            // Long enough to pass the length audits, so they stay quiet unless
            // a test asks for them.
            title: Some("A perfectly reasonable page title for testing"),
            description: Some(
                "A meta description of a sensible length, long enough to satisfy the \
                 minimum and short enough to stay under the maximum.",
            ),
            h1_count: 1,
            word_count: 400,
            text_hash: None,
            simhash: None,
            canonical: None,
            canonical_is_self: None,
            meta_robots: None,
            html_lang: Some("en"),
            x_robots_tag: None,
            redirect_count: 0,
            redirect_loop: false,
            response_ms: Some(120),
            body_bytes: Some(20_000),
            error_kind: None,
            is_seed: false,
        }
    }
}

struct TestSite {
    store: Store,
}

impl TestSite {
    fn new() -> Self {
        let store = Store::open_in_memory().unwrap();
        store
            .initialise_crawl(&CrawlConfig::spider("https://a.test/"), "a.test")
            .unwrap();
        Self { store }
    }

    /// Insert a page and return its id.
    fn add(&mut self, page: Page<'_>) -> i64 {
        let canonical_is_self = page
            .canonical_is_self
            .or_else(|| page.canonical.map(|canonical| canonical == page.url));
        self.store
            .connection()
            .execute(
                "INSERT INTO urls (
                     url, dedup_key, depth, state, kind, is_internal, status_code,
                     response_time_ms, body_bytes, title, title_length, meta_description,
                     meta_description_length, meta_robots, x_robots_tag, canonical,
                     canonical_is_self, h1_count, word_count, text_hash, text_simhash,
                     redirect_count, redirect_loop, error_kind, discovered_via, html_lang,
                     content_type
                 ) VALUES (
                     ?1, ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15,
                     ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25, 'text/html'
                 )",
                params![
                    page.url,
                    page.depth,
                    page.state as i64,
                    page.kind as i64,
                    page.is_internal as i64,
                    page.status,
                    page.response_ms,
                    page.body_bytes,
                    page.title,
                    page.title.map(|t| t.chars().count() as i64),
                    page.description,
                    page.description.map(|d| d.chars().count() as i64),
                    page.meta_robots,
                    page.x_robots_tag,
                    page.canonical,
                    canonical_is_self.map(i64::from),
                    page.h1_count,
                    page.word_count,
                    page.text_hash,
                    page.simhash.map(|h| h as i64),
                    page.redirect_count,
                    page.redirect_loop as i64,
                    page.error_kind,
                    // NULL marks a seed; anything else marks a discovery.
                    (!page.is_seed).then_some(crate::model::LinkSource::Anchor as i64),
                    page.html_lang,
                ],
            )
            .unwrap();
        self.store.connection().last_insert_rowid()
    }

    /// A followed `<a href>` link from `from` to `to`.
    fn link(&mut self, from: i64, to: &str) {
        self.link_with(from, to, crate::model::LinkSource::Anchor, false);
    }

    fn link_with(&mut self, from: i64, to: &str, source: crate::model::LinkSource, nofollow: bool) {
        let is_internal = to.contains("a.test");
        self.store
            .connection()
            .execute(
                "INSERT INTO links
                     (source_id, target_key, target_url, raw_href, anchor_text, position,
                      link_source, nofollow, is_internal)
                 VALUES (?1, ?2, ?2, ?2, 'anchor', 0, ?3, ?4, ?5)",
                params![from, to, source as i64, nofollow as i64, is_internal as i64],
            )
            .unwrap();
    }

    fn hreflang(&mut self, url_id: i64, lang: &str, href: &str) {
        self.store
            .connection()
            .execute(
                "INSERT INTO hreflang (url_id, lang, raw_href, href, href_key)
                 VALUES (?1, ?2, ?3, ?3, ?3)",
                params![url_id, lang, href],
            )
            .unwrap();
    }

    fn image(&mut self, url_id: i64, src: &str, alt: Option<&str>) {
        self.store
            .connection()
            .execute(
                "INSERT INTO images (url_id, raw_src, src, src_key, alt, alt_missing)
                 VALUES (?1, ?2, ?2, ?2, ?3, ?4)",
                params![url_id, src, alt, alt.is_none() as i64],
            )
            .unwrap();
    }

    fn sitemap_entry(&mut self, url: &str) {
        self.store
            .connection()
            .execute(
                "INSERT INTO sitemap_urls (url, dedup_key, sitemap_url)
                 VALUES (?1, ?1, 'https://a.test/sitemap.xml')",
                params![url],
            )
            .unwrap();
    }

    /// Recompute inlink counts, as a finished crawl would, then audit.
    fn audit(&mut self) -> AuditReport {
        self.audit_with(&AuditConfig::default())
    }

    fn audit_with(&mut self, config: &AuditConfig) -> AuditReport {
        self.store.finalize("finished").unwrap();
        self.store.audit(config).unwrap()
    }

    /// URLs affected by an issue, by address.
    fn affected(&self, issue_id: &str) -> Vec<String> {
        queries::issue_details(self.store.connection(), issue_id, 100)
            .unwrap()
            .into_iter()
            .map(|(_, url, _)| url)
            .collect()
    }

    fn detail(&self, issue_id: &str, url: &str) -> Option<String> {
        queries::issue_details(self.store.connection(), issue_id, 100)
            .unwrap()
            .into_iter()
            .find(|(_, found, _)| found == url)
            .and_then(|(_, _, detail)| detail)
    }
}

// ─── status and connectivity ─────────────────────────────────────────────

#[test]
fn error_statuses_and_fetch_failures_are_reported_separately() {
    let mut site = TestSite::new();
    site.add(Page {
        url: "https://a.test/ok",
        ..Default::default()
    });
    site.add(Page {
        url: "https://a.test/gone",
        status: Some(404),
        ..Default::default()
    });
    site.add(Page {
        url: "https://a.test/broken",
        status: Some(503),
        ..Default::default()
    });
    site.add(Page {
        url: "https://a.test/unreachable",
        status: None,
        state: UrlState::Failed,
        error_kind: Some("timeout"),
        ..Default::default()
    });

    let report = site.audit();

    assert_eq!(report.issue("status_4xx").unwrap().url_count, 1);
    assert_eq!(report.issue("status_5xx").unwrap().url_count, 1);
    assert_eq!(report.issue("fetch_failed").unwrap().url_count, 1);
    assert_eq!(
        site.detail("status_4xx", "https://a.test/gone").as_deref(),
        Some("HTTP 404")
    );
    assert_eq!(
        site.detail("fetch_failed", "https://a.test/unreachable")
            .as_deref(),
        Some("timeout")
    );

    // All three are errors, which is what the UI sorts on.
    for id in ["status_4xx", "status_5xx", "fetch_failed"] {
        assert_eq!(report.issue(id).unwrap().severity, Severity::Error);
    }
}

#[test]
fn broken_internal_links_name_the_page_that_needs_editing() {
    let mut site = TestSite::new();
    let home = site.add(Page {
        url: "https://a.test/",
        depth: 0,
        is_seed: true,
        ..Default::default()
    });
    site.add(Page {
        url: "https://a.test/gone",
        status: Some(404),
        ..Default::default()
    });
    site.link(home, "https://a.test/gone");

    let report = site.audit();

    // The 404 itself is one issue; the page linking to it is another.
    assert_eq!(report.issue("status_4xx").unwrap().url_count, 1);
    assert_eq!(
        site.affected("broken_internal_link"),
        vec!["https://a.test/"]
    );
    assert!(site
        .detail("broken_internal_link", "https://a.test/")
        .unwrap()
        .contains("/gone"));
}

#[test]
fn redirect_chains_and_loops_are_distinguished() {
    let mut site = TestSite::new();
    site.add(Page {
        url: "https://a.test/one-hop",
        status: Some(301),
        redirect_count: 1,
        ..Default::default()
    });
    site.add(Page {
        url: "https://a.test/chain",
        status: Some(301),
        redirect_count: 4,
        ..Default::default()
    });
    site.add(Page {
        url: "https://a.test/loop",
        status: Some(301),
        redirect_count: 2,
        redirect_loop: true,
        ..Default::default()
    });

    let report = site.audit();

    // A single hop is normal and must not be reported.
    assert_eq!(
        site.affected("redirect_chain"),
        vec!["https://a.test/chain"]
    );
    assert_eq!(site.affected("redirect_loop"), vec!["https://a.test/loop"]);
    assert_eq!(
        report.issue("redirect_loop").unwrap().severity,
        Severity::Error
    );
    assert_eq!(
        report.issue("redirect_chain").unwrap().severity,
        Severity::Warning
    );
}

#[test]
fn internal_links_to_redirects_are_flagged_at_the_source() {
    let mut site = TestSite::new();
    let home = site.add(Page {
        url: "https://a.test/",
        depth: 0,
        is_seed: true,
        ..Default::default()
    });
    site.add(Page {
        url: "https://a.test/old",
        status: Some(301),
        redirect_count: 1,
        ..Default::default()
    });
    site.link(home, "https://a.test/old");

    site.audit();
    assert_eq!(
        site.affected("internal_link_to_redirect"),
        vec!["https://a.test/"]
    );
}

// ─── titles and descriptions ─────────────────────────────────────────────

#[test]
fn title_problems() {
    let mut site = TestSite::new();
    site.add(Page {
        url: "https://a.test/none",
        title: None,
        ..Default::default()
    });
    site.add(Page {
        url: "https://a.test/empty",
        title: Some("   "),
        ..Default::default()
    });
    site.add(Page {
        url: "https://a.test/short",
        title: Some("Hi"),
        ..Default::default()
    });
    site.add(Page {
        url: "https://a.test/long",
        title: Some(
            "An extremely long page title that goes well past sixty characters and will be cut",
        ),
        ..Default::default()
    });
    site.add(Page {
        url: "https://a.test/dup-1",
        title: Some("Shared title"),
        ..Default::default()
    });
    site.add(Page {
        url: "https://a.test/dup-2",
        title: Some("Shared title"),
        ..Default::default()
    });

    site.audit();

    let missing = site.affected("title_missing");
    assert_eq!(missing.len(), 2, "absent and whitespace-only both count");
    assert!(missing.contains(&"https://a.test/none".to_string()));
    assert!(missing.contains(&"https://a.test/empty".to_string()));

    // "Shared title" is 12 characters, so the duplicates are also short.
    assert_eq!(site.affected("title_too_long"), vec!["https://a.test/long"]);
    assert!(site
        .affected("title_too_short")
        .contains(&"https://a.test/short".to_string()));

    let duplicates = site.affected("title_duplicate");
    assert_eq!(
        duplicates.len(),
        2,
        "both sides of a duplicate are reported"
    );
    assert_eq!(
        site.detail("title_duplicate", "https://a.test/dup-1")
            .as_deref(),
        Some("shared with 1 other page(s)")
    );
}

#[test]
fn description_problems() {
    let mut site = TestSite::new();
    site.add(Page {
        url: "https://a.test/none",
        description: None,
        ..Default::default()
    });
    site.add(Page {
        url: "https://a.test/short",
        description: Some("Too short."),
        ..Default::default()
    });
    site.add(Page {
        url: "https://a.test/long",
        description: Some(&"x".repeat(200)),
        ..Default::default()
    });
    site.add(Page {
        url: "https://a.test/d1",
        description: Some("Same description used twice over here."),
        ..Default::default()
    });
    site.add(Page {
        url: "https://a.test/d2",
        description: Some("Same description used twice over here."),
        ..Default::default()
    });

    site.audit();

    assert_eq!(
        site.affected("description_missing"),
        vec!["https://a.test/none"]
    );
    assert_eq!(
        site.affected("description_too_long"),
        vec!["https://a.test/long"]
    );
    assert!(site
        .affected("description_too_short")
        .contains(&"https://a.test/short".to_string()));
    assert_eq!(site.affected("description_duplicate").len(), 2);
}

#[test]
fn length_thresholds_are_configurable() {
    let mut site = TestSite::new();
    site.add(Page {
        url: "https://a.test/p",
        title: Some("Nine char"),
        ..Default::default()
    });

    let strict = AuditConfig {
        title_min_chars: 5,
        title_max_chars: 8,
        ..Default::default()
    };
    site.audit_with(&strict);
    assert_eq!(site.affected("title_too_long"), vec!["https://a.test/p"]);
    assert!(site.affected("title_too_short").is_empty());

    let relaxed = AuditConfig {
        title_min_chars: 20,
        title_max_chars: 100,
        ..Default::default()
    };
    let report = site.audit_with(&relaxed);
    assert!(report.issue("title_too_long").is_none());
    assert_eq!(site.affected("title_too_short"), vec!["https://a.test/p"]);

    // The thresholds appear in the text the user reads.
    assert!(report
        .issue("title_too_short")
        .unwrap()
        .description
        .contains("20"));
}

#[test]
fn headings() {
    let mut site = TestSite::new();
    site.add(Page {
        url: "https://a.test/none",
        h1_count: 0,
        ..Default::default()
    });
    site.add(Page {
        url: "https://a.test/two",
        h1_count: 2,
        ..Default::default()
    });
    site.add(Page {
        url: "https://a.test/one",
        h1_count: 1,
        ..Default::default()
    });

    let report = site.audit();
    assert_eq!(site.affected("h1_missing"), vec!["https://a.test/none"]);
    assert_eq!(site.affected("h1_multiple"), vec!["https://a.test/two"]);
    assert_eq!(
        site.detail("h1_multiple", "https://a.test/two").as_deref(),
        Some("2 H1 tags")
    );
    assert_eq!(
        report.issue("h1_multiple").unwrap().severity,
        Severity::Notice
    );
}

// ─── canonicals, robots and indexability ─────────────────────────────────

#[test]
fn canonical_problems() {
    let mut site = TestSite::new();
    site.add(Page {
        url: "https://a.test/none",
        canonical: None,
        ..Default::default()
    });
    site.add(Page {
        url: "https://a.test/self",
        canonical: Some("https://a.test/self"),
        ..Default::default()
    });
    site.add(Page {
        url: "https://a.test/elsewhere",
        canonical: Some("https://a.test/self"),
        ..Default::default()
    });
    site.add(Page {
        url: "https://a.test/dead",
        status: Some(404),
        ..Default::default()
    });
    site.add(Page {
        url: "https://a.test/points-at-dead",
        canonical: Some("https://a.test/dead"),
        ..Default::default()
    });

    let report = site.audit();

    assert_eq!(
        site.affected("canonical_missing"),
        vec!["https://a.test/none"]
    );
    let canonicalised = site.affected("canonicalised");
    assert!(canonicalised.contains(&"https://a.test/elsewhere".to_string()));
    assert!(!canonicalised.contains(&"https://a.test/self".to_string()));

    assert_eq!(
        site.affected("canonical_non_200"),
        vec!["https://a.test/points-at-dead"]
    );
    assert_eq!(
        report.issue("canonical_non_200").unwrap().severity,
        Severity::Error
    );
}

#[test]
fn page_quality_audits_skip_pages_that_cannot_be_indexed() {
    let mut site = TestSite::new();
    // Each of these has no title, but none of them can be indexed, so none
    // should be nagged about.
    site.add(Page {
        url: "https://a.test/noindex",
        title: None,
        meta_robots: Some("noindex"),
        ..Default::default()
    });
    site.add(Page {
        url: "https://a.test/header-noindex",
        title: None,
        x_robots_tag: Some("noindex, nofollow"),
        ..Default::default()
    });
    site.add(Page {
        url: "https://a.test/canonicalised",
        title: None,
        canonical: Some("https://a.test/"),
        ..Default::default()
    });
    site.add(Page {
        url: "https://a.test/404",
        title: None,
        status: Some(404),
        ..Default::default()
    });
    site.add(Page {
        url: "https://a.test/blocked",
        title: None,
        state: UrlState::BlockedByRobots,
        status: None,
        ..Default::default()
    });
    site.add(Page {
        url: "https://external.test/x",
        title: None,
        is_internal: false,
        ..Default::default()
    });
    site.add(Page {
        url: "https://a.test/image.png",
        title: None,
        kind: ResourceKind::Image,
        ..Default::default()
    });
    // ...and one that can.
    site.add(Page {
        url: "https://a.test/real",
        title: None,
        ..Default::default()
    });

    let report = site.audit();
    assert_eq!(
        site.affected("title_missing"),
        vec!["https://a.test/real"],
        "only indexable pages are judged on their title"
    );
    assert_eq!(report.indexable_pages, 1);
    assert!(report.non_indexable_pages >= 5);
}

#[test]
fn indexability_records_the_reason() {
    let mut site = TestSite::new();
    site.add(Page {
        url: "https://a.test/ok",
        ..Default::default()
    });
    site.add(Page {
        url: "https://a.test/noindex",
        meta_robots: Some("noindex,follow"),
        ..Default::default()
    });
    site.add(Page {
        url: "https://a.test/gone",
        status: Some(404),
        ..Default::default()
    });
    site.add(Page {
        url: "https://a.test/canon",
        canonical: Some("https://a.test/ok"),
        ..Default::default()
    });
    site.add(Page {
        url: "https://a.test/blocked",
        state: UrlState::BlockedByRobots,
        status: None,
        ..Default::default()
    });
    site.add(Page {
        url: "https://a.test/pic.png",
        kind: ResourceKind::Image,
        ..Default::default()
    });

    site.audit();
    let conn = site.store.connection();
    let reason = |url: &str| -> (Option<i64>, Option<String>) {
        conn.query_row(
            "SELECT indexable, indexability_reason FROM urls WHERE url = ?1",
            [url],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap()
    };

    assert_eq!(reason("https://a.test/ok"), (Some(1), None));
    assert_eq!(
        reason("https://a.test/noindex").1.as_deref(),
        Some("noindex (meta robots)")
    );
    assert_eq!(reason("https://a.test/gone").1.as_deref(), Some("HTTP 404"));
    assert_eq!(
        reason("https://a.test/canon").1.as_deref(),
        Some("Canonicalised")
    );
    assert_eq!(
        reason("https://a.test/blocked").1.as_deref(),
        Some("Blocked by robots.txt")
    );
    // Indexability does not apply to an image.
    assert_eq!(reason("https://a.test/pic.png"), (None, None));
}

#[test]
fn noindex_and_robots_blocked_pages_that_are_linked_to() {
    let mut site = TestSite::new();
    let home = site.add(Page {
        url: "https://a.test/",
        depth: 0,
        is_seed: true,
        ..Default::default()
    });
    site.add(Page {
        url: "https://a.test/noindex",
        meta_robots: Some("noindex"),
        ..Default::default()
    });
    site.add(Page {
        url: "https://a.test/blocked",
        state: UrlState::BlockedByRobots,
        status: None,
        ..Default::default()
    });
    site.link(home, "https://a.test/noindex");
    site.link(home, "https://a.test/blocked");

    let report = site.audit();

    assert_eq!(
        site.affected("noindex_internal_links"),
        vec!["https://a.test/noindex"]
    );
    assert_eq!(
        site.affected("robots_blocked_but_linked"),
        vec!["https://a.test/blocked"]
    );
    assert_eq!(
        site.detail("noindex_internal_links", "https://a.test/noindex")
            .as_deref(),
        Some("1 internal link(s)")
    );
    // The noindex page is also listed informationally, as a notice.
    assert_eq!(
        report.issue("noindex_page").unwrap().severity,
        Severity::Notice
    );
}

// ─── hreflang ────────────────────────────────────────────────────────────

#[test]
fn hreflang_return_links_must_be_reciprocal() {
    let mut site = TestSite::new();
    let en = site.add(Page {
        url: "https://a.test/en",
        ..Default::default()
    });
    let fr = site.add(Page {
        url: "https://a.test/fr",
        ..Default::default()
    });
    let de = site.add(Page {
        url: "https://a.test/de",
        ..Default::default()
    });

    // en ↔ fr is a correct pair.
    site.hreflang(en, "en", "https://a.test/en");
    site.hreflang(en, "fr", "https://a.test/fr");
    site.hreflang(fr, "en", "https://a.test/en");
    site.hreflang(fr, "fr", "https://a.test/fr");
    // de points at en, but en does not point back at de.
    site.hreflang(de, "en", "https://a.test/en");

    site.audit();

    assert_eq!(
        site.affected("hreflang_missing_return"),
        vec!["https://a.test/de"]
    );
    assert!(site
        .detail("hreflang_missing_return", "https://a.test/de")
        .unwrap()
        .contains("/en"));
}

#[test]
fn an_hreflang_block_must_include_the_page_itself() {
    let mut site = TestSite::new();
    let complete = site.add(Page {
        url: "https://a.test/en",
        ..Default::default()
    });
    let no_self = site.add(Page {
        url: "https://a.test/fr",
        ..Default::default()
    });
    site.add(Page {
        url: "https://a.test/plain",
        ..Default::default()
    });

    // A complete block names every page in the group, itself included.
    site.hreflang(complete, "en", "https://a.test/en");
    site.hreflang(complete, "fr", "https://a.test/fr");
    // This one names its sibling but forgets itself.
    site.hreflang(no_self, "en", "https://a.test/en");

    site.audit();

    assert_eq!(
        site.affected("hreflang_missing_self"),
        vec!["https://a.test/fr"]
    );
    // A page with no hreflang at all is not expected to have a self-reference.
    assert!(!site
        .affected("hreflang_missing_self")
        .contains(&"https://a.test/plain".to_string()));
}

#[test]
fn a_near_duplicate_of_an_exact_duplicate_pair_is_still_reported() {
    let mut site = TestSite::new();
    let body = "Delivery is free on orders over fifty pounds and takes two to three \
                working days. Orders placed before midday are dispatched the same \
                afternoon. Tracking details are emailed as soon as the parcel leaves \
                our warehouse. "
        .repeat(4);
    let variant = format!("{body} Next-day delivery is available at checkout.");

    // Two pages are byte-identical...
    let exact_hash = crate::hash::text_hash(&body);
    for url in ["https://a.test/delivery", "https://a.test/shipping"] {
        site.add(Page {
            url,
            text_hash: Some(exact_hash.clone()),
            simhash: crate::hash::simhash(&body),
            word_count: 400,
            ..Default::default()
        });
    }
    // ...and a third nearly matches them.
    site.add(Page {
        url: "https://a.test/delivery-uk",
        text_hash: Some(crate::hash::text_hash(&variant)),
        simhash: crate::hash::simhash(&variant),
        word_count: 406,
        ..Default::default()
    });

    site.audit();

    // The identical pair is an exact duplicate, and only that.
    let exact = site.affected("duplicate_content");
    assert_eq!(exact.len(), 2);
    // The third page is reported as a near-duplicate of one of them: being an
    // exact duplicate excludes a page from being *reported* as a near one, but
    // not from being another page's match.
    assert_eq!(
        site.affected("near_duplicate_content"),
        vec!["https://a.test/delivery-uk"]
    );
    assert!(site
        .detail("near_duplicate_content", "https://a.test/delivery-uk")
        .unwrap()
        .contains("% similar"));
}

#[test]
fn invalid_hreflang_codes_are_reported_with_an_explanation() {
    let mut site = TestSite::new();
    let good = site.add(Page {
        url: "https://a.test/good",
        ..Default::default()
    });
    let bad = site.add(Page {
        url: "https://a.test/bad",
        ..Default::default()
    });
    site.hreflang(good, "en-GB", "https://a.test/good");
    site.hreflang(good, "x-default", "https://a.test/good");
    site.hreflang(bad, "en-UK", "https://a.test/bad");

    site.audit();

    assert_eq!(
        site.affected("hreflang_invalid_code"),
        vec!["https://a.test/bad"]
    );
    assert!(site
        .detail("hreflang_invalid_code", "https://a.test/bad")
        .unwrap()
        .contains("GB"));
}

// ─── duplication ─────────────────────────────────────────────────────────

#[test]
fn exact_duplicates_are_reported_and_near_duplicates_are_not_double_counted() {
    let mut site = TestSite::new();
    let body = "The returns policy allows any item to be sent back within thirty days of \
                delivery for a full refund, provided it is unused and in its original \
                packaging. Contact support to start a return and we will email a prepaid \
                label. Refunds are processed within five working days of receipt. "
        .repeat(4);
    let variant = format!("{body} Last updated in March.");

    let exact_hash = crate::hash::text_hash(&body);
    for url in ["https://a.test/dup-a", "https://a.test/dup-b"] {
        site.add(Page {
            url,
            text_hash: Some(exact_hash.clone()),
            simhash: crate::hash::simhash(&body),
            word_count: 400,
            ..Default::default()
        });
    }
    site.add(Page {
        url: "https://a.test/near",
        text_hash: Some(crate::hash::text_hash(&variant)),
        simhash: crate::hash::simhash(&variant),
        word_count: 404,
        ..Default::default()
    });
    site.add(Page {
        url: "https://a.test/unique",
        text_hash: Some(crate::hash::text_hash(
            "Something else entirely, about bicycles.",
        )),
        simhash: crate::hash::simhash(
            "Something else entirely, about bicycles and the weather and nothing at all to \
             do with returns or refunds or packaging of any kind whatsoever.",
        ),
        word_count: 400,
        ..Default::default()
    });

    site.audit();

    let exact = site.affected("duplicate_content");
    assert_eq!(exact.len(), 2);
    assert!(exact.contains(&"https://a.test/dup-a".to_string()));
    assert!(!exact.contains(&"https://a.test/near".to_string()));

    // The near-duplicate is only reported as such: the exact pair is excluded
    // from the simhash comparison, so nothing appears in both issues.
    let near = site.affected("near_duplicate_content");
    assert!(
        near.is_empty() || !near.iter().any(|url| exact.contains(url)),
        "a page must not be both an exact and a near duplicate"
    );
    assert!(!site
        .affected("duplicate_content")
        .contains(&"https://a.test/unique".to_string()));
}

#[test]
fn near_duplicates_are_found_between_unique_but_similar_pages() {
    let mut site = TestSite::new();
    let body = "Our opening hours are nine until five on weekdays and ten until four on \
                Saturdays. The shop is closed on Sundays and on public holidays. Parking is \
                available behind the building, and the entrance is step-free throughout. "
        .repeat(4);

    for (url, suffix) in [
        ("https://a.test/branch-a", "Branch A."),
        ("https://a.test/branch-b", "Branch B."),
    ] {
        let text = format!("{body} {suffix}");
        site.add(Page {
            url,
            text_hash: Some(crate::hash::text_hash(&text)),
            simhash: crate::hash::simhash(&text),
            word_count: 400,
            ..Default::default()
        });
    }

    site.audit();

    let near = site.affected("near_duplicate_content");
    assert_eq!(near.len(), 2, "both sides of the pair are reported");
    let detail = site
        .detail("near_duplicate_content", "https://a.test/branch-a")
        .unwrap();
    assert!(detail.contains("% similar"), "got {detail}");
    assert!(detail.contains("branch-b"));
}

#[test]
fn thin_pages_are_not_compared_by_simhash() {
    let mut site = TestSite::new();
    // Two short pages with near-identical text. Simhash is unreliable at this
    // length, so the audit must leave them alone.
    for (url, text) in [
        ("https://a.test/thin-a", "Call us on 01234 567890."),
        ("https://a.test/thin-b", "Call us on 01234 567891."),
    ] {
        site.add(Page {
            url,
            text_hash: Some(crate::hash::text_hash(text)),
            simhash: crate::hash::simhash(text),
            word_count: 5,
            ..Default::default()
        });
    }

    let report = site.audit();
    assert!(report.issue("near_duplicate_content").is_none());
}

// ─── images, speed, scheme, depth ────────────────────────────────────────

#[test]
fn images_missing_alt_text_are_counted_per_page() {
    let mut site = TestSite::new();
    let page = site.add(Page {
        url: "https://a.test/gallery",
        ..Default::default()
    });
    site.image(page, "https://a.test/1.png", Some("Described"));
    site.image(page, "https://a.test/2.png", None);
    site.image(page, "https://a.test/3.png", None);
    // An empty alt is a deliberate choice for a decorative image, not a fault.
    site.image(page, "https://a.test/4.png", Some(""));

    site.audit();
    assert_eq!(
        site.affected("image_missing_alt"),
        vec!["https://a.test/gallery"]
    );
    assert_eq!(
        site.detail("image_missing_alt", "https://a.test/gallery")
            .as_deref(),
        Some("2 image(s)")
    );
}

#[test]
fn oversized_images_and_slow_responses_use_their_thresholds() {
    let mut site = TestSite::new();
    site.add(Page {
        url: "https://a.test/huge.png",
        kind: ResourceKind::Image,
        body_bytes: Some(400 * 1024),
        ..Default::default()
    });
    site.add(Page {
        url: "https://a.test/small.png",
        kind: ResourceKind::Image,
        body_bytes: Some(10 * 1024),
        ..Default::default()
    });
    site.add(Page {
        url: "https://a.test/slow",
        response_ms: Some(2_500),
        ..Default::default()
    });

    site.audit();
    assert_eq!(
        site.affected("image_oversized"),
        vec!["https://a.test/huge.png"]
    );
    assert_eq!(
        site.detail("image_oversized", "https://a.test/huge.png")
            .as_deref(),
        Some("400 KB")
    );
    assert_eq!(site.affected("slow_response"), vec!["https://a.test/slow"]);

    // Raise the bar and both go quiet.
    let relaxed = AuditConfig {
        large_image_bytes: 1024 * 1024,
        slow_response_ms: 5_000,
        ..Default::default()
    };
    let report = site.audit_with(&relaxed);
    assert!(report.issue("image_oversized").is_none());
    assert!(report.issue("slow_response").is_none());
}

#[test]
fn insecure_references_on_an_https_site() {
    let mut site = TestSite::new();
    let page = site.add(Page {
        url: "https://a.test/",
        depth: 0,
        is_seed: true,
        ..Default::default()
    });
    site.add(Page {
        url: "http://a.test/legacy",
        ..Default::default()
    });
    site.link_with(
        page,
        "http://cdn.test/pic.png",
        crate::model::LinkSource::Image,
        false,
    );

    site.audit();

    assert_eq!(site.affected("mixed_content"), vec!["https://a.test/"]);
    assert!(site
        .detail("mixed_content", "https://a.test/")
        .unwrap()
        .contains("cdn.test"));
    assert_eq!(
        site.affected("http_url_on_https_site"),
        vec!["http://a.test/legacy"]
    );
}

#[test]
fn buried_pages_are_a_notice() {
    let mut site = TestSite::new();
    site.add(Page {
        url: "https://a.test/shallow",
        depth: 2,
        ..Default::default()
    });
    site.add(Page {
        url: "https://a.test/deep",
        depth: 9,
        ..Default::default()
    });

    let report = site.audit();
    assert_eq!(site.affected("deep_page"), vec!["https://a.test/deep"]);
    assert_eq!(
        site.detail("deep_page", "https://a.test/deep").as_deref(),
        Some("9 clicks deep")
    );
    assert_eq!(report.depth_distribution, vec![(2, 1), (9, 1)]);
}

// ─── orphans and link score ──────────────────────────────────────────────

#[test]
fn orphan_pages_say_whether_they_are_in_the_sitemap() {
    let mut site = TestSite::new();
    let home = site.add(Page {
        url: "https://a.test/",
        depth: 0,
        is_seed: true,
        ..Default::default()
    });
    site.add(Page {
        url: "https://a.test/linked",
        ..Default::default()
    });
    site.add(Page {
        url: "https://a.test/orphan-sitemap",
        ..Default::default()
    });
    site.add(Page {
        url: "https://a.test/orphan-plain",
        ..Default::default()
    });
    site.link(home, "https://a.test/linked");
    site.sitemap_entry("https://a.test/orphan-sitemap");

    site.audit();

    let orphans = site.affected("orphan_page");
    assert_eq!(orphans.len(), 2);
    assert!(!orphans.contains(&"https://a.test/linked".to_string()));
    // The start URL is never an orphan, however few links point at it.
    assert!(!orphans.contains(&"https://a.test/".to_string()));
    assert_eq!(
        site.detail("orphan_page", "https://a.test/orphan-sitemap")
            .as_deref(),
        Some("in sitemap, no internal links")
    );
    assert_eq!(
        site.detail("orphan_page", "https://a.test/orphan-plain")
            .as_deref(),
        Some("no internal links")
    );
}

#[test]
fn link_score_follows_the_internal_link_graph() {
    let mut site = TestSite::new();
    let home = site.add(Page {
        url: "https://a.test/",
        depth: 0,
        is_seed: true,
        ..Default::default()
    });
    let hub = site.add(Page {
        url: "https://a.test/hub",
        ..Default::default()
    });
    let leaf_a = site.add(Page {
        url: "https://a.test/leaf-a",
        ..Default::default()
    });
    let leaf_b = site.add(Page {
        url: "https://a.test/leaf-b",
        ..Default::default()
    });
    let ignored = site.add(Page {
        url: "https://a.test/nofollowed",
        ..Default::default()
    });

    // Everything links to the hub; the hub links to the leaves.
    for source in [home, leaf_a, leaf_b] {
        site.link(source, "https://a.test/hub");
    }
    site.link(hub, "https://a.test/leaf-a");
    site.link(hub, "https://a.test/leaf-b");
    // A nofollowed link passes nothing.
    site.link_with(
        home,
        "https://a.test/nofollowed",
        crate::model::LinkSource::Anchor,
        true,
    );
    let _ = ignored;

    site.audit();

    let score = |url: &str| -> Option<f64> {
        site.store
            .connection()
            .query_row("SELECT link_score FROM urls WHERE url = ?1", [url], |row| {
                row.get(0)
            })
            .unwrap()
    };

    let hub_score = score("https://a.test/hub").unwrap();
    let leaf_score = score("https://a.test/leaf-a").unwrap();
    let nofollowed = score("https://a.test/nofollowed").unwrap();

    assert_eq!(hub_score, 100.0, "the strongest page is rescaled to 100");
    assert!(
        leaf_score < hub_score,
        "{leaf_score} should be below {hub_score}"
    );
    assert!(
        nofollowed < leaf_score,
        "a nofollowed page ({nofollowed}) should score below a linked one ({leaf_score})"
    );
    // Symmetric pages score the same.
    assert!((leaf_score - score("https://a.test/leaf-b").unwrap()).abs() < 0.001);
}

#[test]
fn link_score_handles_a_site_with_no_links_at_all() {
    let mut site = TestSite::new();
    site.add(Page {
        url: "https://a.test/",
        depth: 0,
        is_seed: true,
        ..Default::default()
    });
    site.audit();

    // Every page gets an equal share, so the only page is the strongest.
    let score: Option<f64> = site
        .store
        .connection()
        .query_row(
            "SELECT link_score FROM urls WHERE url = 'https://a.test/'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(score, Some(100.0));
}

// ─── the report and its storage ──────────────────────────────────────────

#[test]
fn the_report_groups_by_severity_and_carries_the_fix_text() {
    let mut site = TestSite::new();
    site.add(Page {
        url: "https://a.test/gone",
        status: Some(404),
        ..Default::default()
    });
    site.add(Page {
        url: "https://a.test/no-title",
        title: None,
        ..Default::default()
    });

    let report = site.audit();

    assert!(report.errors >= 1);
    assert!(report.warnings >= 1);
    assert!(report.total_findings() >= 2);
    assert!(report.elapsed_ms < 60_000);

    // Issues arrive worst-first.
    let severities: Vec<Severity> = report.issues.iter().map(|issue| issue.severity).collect();
    assert!(
        severities.windows(2).all(|pair| pair[0] <= pair[1]),
        "{severities:?}"
    );

    let issue = report.issue("status_4xx").unwrap();
    assert!(!issue.how_to_fix.is_empty());
    assert_eq!(issue.docs_slug, "client-errors-4xx");
    assert_eq!(
        report.issues_of(Severity::Error).count(),
        report.errors as usize
    );
}

#[test]
fn clicking_an_issue_filters_the_url_table() {
    let mut site = TestSite::new();
    site.add(Page {
        url: "https://a.test/ok",
        ..Default::default()
    });
    site.add(Page {
        url: "https://a.test/gone",
        status: Some(404),
        ..Default::default()
    });
    site.audit();

    let page = queries::list_urls(
        site.store.connection(),
        &UrlFilter::for_issue("status_4xx"),
        queries::SortBy::Url,
        false,
        50,
        0,
    )
    .unwrap();
    assert_eq!(page.total, 1);
    assert_eq!(page.rows[0].url, "https://a.test/gone");
}

#[test]
fn a_urls_own_issues_are_listed_for_the_detail_panel() {
    let mut site = TestSite::new();
    let id = site.add(Page {
        url: "https://a.test/bad",
        title: None,
        h1_count: 0,
        canonical: None,
        ..Default::default()
    });
    site.audit();

    let issues = queries::issues_for_url(site.store.connection(), id).unwrap();
    let ids: Vec<&str> = issues.iter().map(|(id, _)| id.as_str()).collect();
    assert!(ids.contains(&"title_missing"));
    assert!(ids.contains(&"h1_missing"));
    assert!(ids.contains(&"canonical_missing"));
}

#[test]
fn re_running_the_audit_replaces_the_previous_findings() {
    let mut site = TestSite::new();
    site.add(Page {
        url: "https://a.test/gone",
        status: Some(404),
        ..Default::default()
    });
    let first = site.audit();
    assert_eq!(first.issue("status_4xx").unwrap().url_count, 1);

    // Fix the page and re-run.
    site.store
        .connection()
        .execute(
            "UPDATE urls SET status_code = 200 WHERE url = 'https://a.test/gone'",
            [],
        )
        .unwrap();
    let second = site.audit();

    assert!(
        second.issue("status_4xx").is_none(),
        "the stale finding must be gone"
    );
    assert_eq!(
        queries::issue_details(site.store.connection(), "status_4xx", 10)
            .unwrap()
            .len(),
        0
    );
}

#[test]
fn a_report_can_be_read_back_without_re_running_the_audit() {
    let mut site = TestSite::new();
    site.add(Page {
        url: "https://a.test/gone",
        status: Some(404),
        ..Default::default()
    });
    let fresh = site.audit();

    let config = AuditConfig::default();
    let reread = site.store.audit_report(&config).unwrap();
    assert_eq!(reread.issues.len(), fresh.issues.len());
    assert_eq!(reread.issue("status_4xx").unwrap().url_count, 1);
    assert_eq!(reread.elapsed_ms, 0, "a re-read did not time anything");
}

#[test]
fn a_page_with_no_declared_language_is_an_accessibility_error() {
    let mut site = TestSite::new();
    site.add(Page {
        url: "https://a.test/named",
        html_lang: Some("en-GB"),
        ..Default::default()
    });
    site.add(Page {
        url: "https://a.test/silent",
        html_lang: None,
        ..Default::default()
    });
    site.add(Page {
        url: "https://a.test/blank",
        html_lang: Some("  "),
        ..Default::default()
    });

    let report = site.audit();
    let affected = site.affected("a11y_page_without_lang");
    assert_eq!(affected.len(), 2, "absent and whitespace-only both count");
    assert!(affected.contains(&"https://a.test/silent".to_string()));
    assert!(!affected.contains(&"https://a.test/named".to_string()));
    assert_eq!(
        report.issue("a11y_page_without_lang").unwrap().severity,
        Severity::Error
    );
}

#[test]
fn a_clean_site_produces_no_issues() {
    let mut site = TestSite::new();
    let home = site.add(Page {
        url: "https://a.test/",
        depth: 0,
        is_seed: true,
        canonical: Some("https://a.test/"),
        description: Some(
            "The home page of the test site, describing what the company sells and \
             linking through to the rest of the sections.",
        ),
        ..Default::default()
    });
    let about = site.add(Page {
        url: "https://a.test/about",
        title: Some("About us, the company and what we actually do"),
        canonical: Some("https://a.test/about"),
        description: Some(
            "Who we are, when the company was founded, and the people who work here \
             today across our two offices.",
        ),
        ..Default::default()
    });
    site.link(home, "https://a.test/about");
    site.link(about, "https://a.test/");

    let report = site.audit();
    assert_eq!(report.errors, 0, "{:?}", report.issues);
    assert_eq!(
        report.issues.len(),
        0,
        "a healthy site should be silent, found: {:?}",
        report.issues.iter().map(|i| &i.id).collect::<Vec<_>>()
    );
}

#[test]
fn audit_config_round_trips_through_json() {
    let config = AuditConfig {
        title_max_chars: 70,
        slow_response_ms: 250,
        ..Default::default()
    };
    let json = serde_json::to_string(&config).unwrap();
    assert_eq!(serde_json::from_str::<AuditConfig>(&json).unwrap(), config);
}

#[test]
fn substitution_only_touches_known_placeholders() {
    let config = AuditConfig::default();
    let text = config.apply("up to {title_max_chars} chars, {large_image_kb} KB", true);
    assert_eq!(text, "up to 60 chars, 100 KB");
    // An unknown marker is left alone rather than silently emptied.
    assert_eq!(config.apply("{not_a_threshold}", true), "{not_a_threshold}");
}
