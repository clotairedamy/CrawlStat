//! Store tests. These exercise the write path a crawl actually uses: queue,
//! claim, write results with children, finalise, then read back through the
//! same queries the UI will use.

use std::time::Duration;

use super::queries::{self, SortBy, UrlFilter};
use super::*;
use crate::hash;
use crate::model::{
    DiscoveredImage, DiscoveredLink, HreflangEntry, LinkPosition, LinkSource, RedirectHop,
    SocialTag, StructuredDataBlock,
};
use crate::url_norm::{normalize, NormalizationPolicy};

fn frontier(url: &str, depth: u32) -> FrontierUrl {
    let normalized = normalize(url, None, &NormalizationPolicy::default()).unwrap();
    FrontierUrl {
        url: normalized.url.to_string(),
        key: normalized.key,
        depth,
        discovered_from: None,
        via: None,
        kind: ResourceKind::Html,
        is_internal: true,
        state: UrlState::Queued,
    }
}

/// A fully populated result, so every child table gets exercised.
fn result_for(url_id: i64, url: &str) -> CrawledUrl {
    let link = |href: &str, source| DiscoveredLink {
        raw_href: href.to_string(),
        target: Some(format!("https://a.test{href}")),
        target_key: Some(format!("https://a.test{href}")),
        anchor_text: format!("to {href}"),
        rel: None,
        position: LinkPosition::Content,
        nofollow: false,
        source,
        is_internal: true,
    };

    let parsed = ParsedPage {
        title: Some("A page title".into()),
        meta_description: Some("A description".into()),
        meta_robots: Some("index,follow".into()),
        canonical: Some(url.to_string()),
        canonical_key: Some(url.to_string()),
        html_lang: Some("en".into()),
        charset: Some("utf-8".into()),
        has_viewport: true,
        h1: vec!["Heading one".into()],
        h2: vec!["Sub one".into(), "Sub two".into()],
        word_count: 321,
        text_hash: Some(hash::text_hash("body text")),
        text_simhash: hash::simhash("body text here with several words"),
        links: vec![
            link("/other", LinkSource::Anchor),
            link("/img.png", LinkSource::Image),
        ],
        images: vec![DiscoveredImage {
            raw_src: "/img.png".into(),
            src: Some("https://a.test/img.png".into()),
            src_key: Some("https://a.test/img.png".into()),
            alt: None,
            title: None,
            declared_width: Some(1200),
            declared_height: Some(800),
            loading_lazy: false,
        }],
        hreflang: vec![HreflangEntry {
            lang: "fr".into(),
            raw_href: "/fr".into(),
            href: Some("https://a.test/fr".into()),
            href_key: Some("https://a.test/fr".into()),
        }],
        social: vec![SocialTag {
            namespace: "og".into(),
            property: "title".into(),
            content: "A page title".into(),
        }],
        structured_data: vec![StructuredDataBlock {
            format: "json-ld".into(),
            types: vec!["Article".into()],
            valid: true,
            error: None,
        }],
        ..Default::default()
    };

    CrawledUrl {
        url_id,
        url: url.to_string(),
        state: UrlState::Done,
        kind: ResourceKind::Html,
        status_code: Some(200),
        final_status_code: None,
        response_time: Some(Duration::from_millis(123)),
        ttfb: Some(Duration::from_millis(45)),
        content_type: Some("text/html; charset=utf-8".into()),
        content_length: Some(4096),
        body_bytes: Some(4096),
        content_encoding: Some("gzip".into()),
        x_robots_tag: Some("noarchive".into()),
        final_url: None,
        redirect_chain: vec![],
        redirect_loop: false,
        headers: vec![
            ("content-type".into(), "text/html; charset=utf-8".into()),
            ("x-robots-tag".into(), "noarchive".into()),
        ],
        content_hash: Some(hash::content_hash(b"<html></html>")),
        truncated: false,
        html: None,
        internal_links: 1,
        external_links: 0,
        parsed: Some(parsed),
        error_kind: None,
        error_message: None,
        attempts: 1,
        fetched_at: 1,
    }
}

fn store_with_one_page() -> Store {
    let mut store = Store::open_in_memory().unwrap();
    store
        .initialise_crawl(&CrawlConfig::spider("https://a.test/"), "a.test")
        .unwrap();
    store.enqueue(&[frontier("https://a.test/", 0)]).unwrap();
    let claimed = store.claim(1).unwrap();
    let batch = WriteBatch {
        results: vec![result_for(claimed[0].id, &claimed[0].url)],
        ..Default::default()
    };
    store.apply(&batch).unwrap();
    store
}

#[test]
fn a_new_store_records_its_config_and_can_be_read_back() {
    let store = Store::open_in_memory().unwrap();
    let mut config = CrawlConfig::spider("https://a.test/");
    config.max_urls = Some(500);
    store.initialise_crawl(&config, "a.test").unwrap();

    assert_eq!(store.meta("root_host").unwrap().as_deref(), Some("a.test"));
    assert_eq!(store.meta("state").unwrap().as_deref(), Some("running"));
    assert_eq!(store.stored_config().unwrap().unwrap().max_urls, Some(500));
}

#[test]
fn enqueue_dedups_on_the_normalised_key() {
    let mut store = Store::open_in_memory().unwrap();
    // Same page, three ways of writing it: tracking parameter, reordered
    // query, explicit default port.
    let added = store
        .enqueue(&[
            frontier("https://a.test/p?id=1", 0),
            frontier("https://a.test/p?id=1&utm_source=x", 0),
            frontier("https://a.test:443/p?id=1", 0),
        ])
        .unwrap();
    assert_eq!(added, 1, "all three share a dedup key");
    assert_eq!(store.stats().unwrap().queued, 1);
}

#[test]
fn claim_is_breadth_first_and_marks_rows_in_progress() {
    let mut store = Store::open_in_memory().unwrap();
    store
        .enqueue(&[
            frontier("https://a.test/deep", 3),
            frontier("https://a.test/", 0),
            frontier("https://a.test/mid", 1),
        ])
        .unwrap();

    let claimed = store.claim(2).unwrap();
    assert_eq!(
        claimed.iter().map(|c| c.url.as_str()).collect::<Vec<_>>(),
        vec!["https://a.test/", "https://a.test/mid"]
    );
    let stats = store.stats().unwrap();
    assert_eq!((stats.in_progress, stats.queued), (2, 1));

    // Claiming again never hands out the same URL twice.
    let again = store.claim(10).unwrap();
    assert_eq!(again.len(), 1);
    assert_eq!(again[0].url, "https://a.test/deep");
    assert!(store.claim(10).unwrap().is_empty());
}

#[test]
fn a_result_and_all_its_children_are_written() {
    let store = store_with_one_page();
    let conn = store.connection();

    let row = queries::url_by_address(conn, "https://a.test/")
        .unwrap()
        .unwrap();
    assert_eq!(row.status_code, Some(200));
    assert_eq!(row.state, Some(UrlState::Done));
    assert_eq!(row.title.as_deref(), Some("A page title"));
    assert_eq!(row.title_length, Some(12));
    assert_eq!(row.meta_description_length, Some(13));
    assert_eq!(row.word_count, Some(321));
    assert_eq!(row.h1_count, Some(1));
    assert_eq!(row.response_time_ms, Some(123));
    assert_eq!(
        row.canonical_is_self,
        Some(true),
        "canonical equals the page's own key"
    );

    assert_eq!(queries::headers(conn, row.id).unwrap().len(), 2);
    assert_eq!(queries::outlinks(conn, row.id, 100).unwrap().len(), 2);

    let images = queries::images(conn, row.id).unwrap();
    assert_eq!(images.len(), 1);
    assert!(
        images[0].alt_missing,
        "a missing alt attribute is flagged in the row"
    );
    assert_eq!(images[0].declared_width, Some(1200));

    let counts = |table: &str| -> i64 {
        conn.query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))
            .unwrap()
    };
    assert_eq!(counts("headings"), 3, "one h1 and two h2s");
    assert_eq!(counts("hreflang"), 1);
    assert_eq!(counts("social_tags"), 1);
    assert_eq!(counts("structured_data"), 1);
}

#[test]
fn a_canonical_pointing_elsewhere_is_not_self() {
    let mut store = Store::open_in_memory().unwrap();
    store.enqueue(&[frontier("https://a.test/dup", 0)]).unwrap();
    let claimed = store.claim(1).unwrap();
    let mut result = result_for(claimed[0].id, &claimed[0].url);
    if let Some(parsed) = result.parsed.as_mut() {
        parsed.canonical = Some("https://a.test/original".into());
        parsed.canonical_key = Some("https://a.test/original".into());
    }
    store
        .apply(&WriteBatch {
            results: vec![result],
            ..Default::default()
        })
        .unwrap();

    let row = queries::url_by_id(store.connection(), claimed[0].id)
        .unwrap()
        .unwrap();
    assert_eq!(row.canonical_is_self, Some(false));
    assert_eq!(row.canonical.as_deref(), Some("https://a.test/original"));
}

#[test]
fn rewriting_a_url_replaces_its_children_rather_than_duplicating_them() {
    let mut store = store_with_one_page();
    let id = queries::url_by_address(store.connection(), "https://a.test/")
        .unwrap()
        .unwrap()
        .id;

    // Simulate a retry writing the same URL a second time.
    store
        .apply(&WriteBatch {
            results: vec![result_for(id, "https://a.test/")],
            ..Default::default()
        })
        .unwrap();

    assert_eq!(queries::headers(store.connection(), id).unwrap().len(), 2);
    assert_eq!(
        queries::outlinks(store.connection(), id, 100)
            .unwrap()
            .len(),
        2
    );
}

#[test]
fn redirect_chains_are_stored_in_order() {
    let mut store = Store::open_in_memory().unwrap();
    store.enqueue(&[frontier("https://a.test/1", 0)]).unwrap();
    let claimed = store.claim(1).unwrap();
    let mut result = result_for(claimed[0].id, &claimed[0].url);
    result.parsed = None;
    result.status_code = Some(301);
    result.final_url = Some("https://a.test/3".into());
    result.redirect_chain = vec![
        RedirectHop {
            hop: 1,
            from_url: "https://a.test/1".into(),
            to_url: "https://a.test/2".into(),
            status: 301,
        },
        RedirectHop {
            hop: 2,
            from_url: "https://a.test/2".into(),
            to_url: "https://a.test/3".into(),
            status: 302,
        },
    ];
    store
        .apply(&WriteBatch {
            results: vec![result],
            ..Default::default()
        })
        .unwrap();

    let chain = queries::redirect_chain(store.connection(), claimed[0].id).unwrap();
    assert_eq!(chain.len(), 2);
    assert_eq!(
        chain[0],
        (1, "https://a.test/1".into(), "https://a.test/2".into(), 301)
    );
    assert_eq!(chain[1].3, 302);

    let row = queries::url_by_id(store.connection(), claimed[0].id)
        .unwrap()
        .unwrap();
    assert_eq!(row.redirect_count, 2);
    // Page fields stay empty for a redirect: there was no document.
    assert_eq!(row.word_count, None);
}

#[test]
fn a_fetch_failure_is_recorded_against_the_url() {
    let mut store = Store::open_in_memory().unwrap();
    store
        .enqueue(&[frontier("https://a.test/gone", 0)])
        .unwrap();
    let claimed = store.claim(1).unwrap();
    let mut result = result_for(claimed[0].id, &claimed[0].url);
    result.parsed = None;
    result.state = UrlState::Failed;
    result.status_code = None;
    result.error_kind = Some("timeout".into());
    result.error_message = Some("timed out after 20s".into());
    store
        .apply(&WriteBatch {
            results: vec![result],
            ..Default::default()
        })
        .unwrap();

    let stats = store.stats().unwrap();
    assert_eq!(stats.failed, 1);
    let row = queries::url_by_id(store.connection(), claimed[0].id)
        .unwrap()
        .unwrap();
    assert_eq!(row.error_kind.as_deref(), Some("timeout"));
    assert_eq!(row.status_code, None);
}

#[test]
fn interrupted_urls_are_requeued_on_resume() {
    let mut store = Store::open_in_memory().unwrap();
    store
        .enqueue(&[
            frontier("https://a.test/a", 0),
            frontier("https://a.test/b", 0),
        ])
        .unwrap();
    store.claim(2).unwrap(); // in flight when the process "died"

    assert_eq!(store.reset_in_progress().unwrap(), 2);
    let stats = store.stats().unwrap();
    assert_eq!((stats.queued, stats.in_progress), (2, 0));
}

#[test]
fn remaining_urls_are_marked_skipped_when_a_limit_is_hit() {
    let mut store = Store::open_in_memory().unwrap();
    store
        .enqueue(&[
            frontier("https://a.test/a", 0),
            frontier("https://a.test/b", 0),
        ])
        .unwrap();
    assert_eq!(store.skip_remaining_queued().unwrap(), 2);

    let stats = store.stats().unwrap();
    assert_eq!(stats.skipped, 2);
    assert_eq!(stats.queued, 0);
    // Skipped rows stay visible: "found but not crawled" is useful information.
    assert_eq!(stats.total, 2);
}

#[test]
fn finalize_counts_inlinks_from_followed_anchors_only() {
    let mut store = Store::open_in_memory().unwrap();
    // Two pages both linking to /target, plus an image reference to it.
    store
        .enqueue(&[
            frontier("https://a.test/one", 0),
            frontier("https://a.test/two", 0),
            frontier("https://a.test/target", 1),
        ])
        .unwrap();
    let claimed = store.claim(2).unwrap();

    let mut results = Vec::new();
    for (index, page) in claimed.iter().enumerate() {
        let mut result = result_for(page.id, &page.url);
        let parsed = result.parsed.as_mut().unwrap();
        parsed.links = vec![
            DiscoveredLink {
                raw_href: "/target".into(),
                target: Some("https://a.test/target".into()),
                target_key: Some("https://a.test/target".into()),
                anchor_text: "target".into(),
                rel: None,
                position: LinkPosition::Content,
                nofollow: index == 1, // the second page's link is nofollow
                source: LinkSource::Anchor,
                is_internal: true,
            },
            DiscoveredLink {
                raw_href: "/target".into(),
                target: Some("https://a.test/target".into()),
                target_key: Some("https://a.test/target".into()),
                anchor_text: String::new(),
                rel: None,
                position: LinkPosition::Unknown,
                nofollow: false,
                source: LinkSource::Image,
                is_internal: true,
            },
        ];
        results.push(result);
    }
    store
        .apply(&WriteBatch {
            results,
            ..Default::default()
        })
        .unwrap();
    store.finalize("finished").unwrap();

    let target = queries::url_by_address(store.connection(), "https://a.test/target")
        .unwrap()
        .unwrap();
    assert_eq!(
        target.inlinks, 2,
        "both anchors count, the image reference does not"
    );

    let internal: i64 = store
        .connection()
        .query_row(
            "SELECT internal_inlinks FROM urls WHERE id = ?1",
            [target.id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(internal, 1, "the nofollow anchor is excluded");

    assert_eq!(store.meta("state").unwrap().as_deref(), Some("finished"));
    // The inlinks panel lists every reference, typed: two hyperlinks and two
    // image references here.
    let references = queries::inlinks(store.connection(), target.id, 10).unwrap();
    assert_eq!(references.len(), 4);
    assert_eq!(
        references
            .iter()
            .filter(|l| l.source == LinkSource::Anchor)
            .count(),
        2
    );
    assert_eq!(
        references
            .iter()
            .filter(|l| l.source == LinkSource::Image)
            .count(),
        2
    );
}

#[test]
fn pagination_is_stable_and_reports_the_total() {
    let mut store = Store::open_in_memory().unwrap();
    let urls: Vec<FrontierUrl> = (0..25)
        .map(|i| frontier(&format!("https://a.test/p{i:02}"), 0))
        .collect();
    store.enqueue(&urls).unwrap();

    let page = |offset| {
        queries::list_urls(
            store.connection(),
            &UrlFilter::default(),
            SortBy::Url,
            false,
            10,
            offset,
        )
        .unwrap()
    };

    let first = page(0);
    assert_eq!(first.total, 25);
    assert_eq!(first.rows.len(), 10);
    assert_eq!(first.rows[0].url, "https://a.test/p00");

    let second = page(10);
    assert_eq!(second.rows[0].url, "https://a.test/p10");
    let last = page(20);
    assert_eq!(last.rows.len(), 5, "the final partial page");

    // No row appears on two pages.
    let mut seen: Vec<String> = [first, second, last]
        .iter()
        .flat_map(|p| p.rows.iter().map(|r| r.url.clone()))
        .collect();
    seen.sort();
    seen.dedup();
    assert_eq!(seen.len(), 25);
}

#[test]
fn filters_and_sorts_narrow_the_table() {
    let mut store = Store::open_in_memory().unwrap();
    store
        .enqueue(&[
            frontier("https://a.test/ok", 0),
            frontier("https://a.test/missing", 1),
            frontier("https://external.test/x", 1),
        ])
        .unwrap();
    store
        .connection()
        .execute(
            "UPDATE urls SET is_internal = 0 WHERE url LIKE 'https://external%'",
            [],
        )
        .unwrap();

    let claimed = store.claim(3).unwrap();
    let mut results = Vec::new();
    for page in &claimed {
        let mut result = result_for(page.id, &page.url);
        if page.url.contains("missing") {
            result.status_code = Some(404);
            result.parsed = None;
        }
        results.push(result);
    }
    store
        .apply(&WriteBatch {
            results,
            ..Default::default()
        })
        .unwrap();

    let conn = store.connection();
    let list =
        |filter: &UrlFilter| queries::list_urls(conn, filter, SortBy::Url, false, 100, 0).unwrap();

    assert_eq!(list(&UrlFilter::status_class(4)).total, 1);
    assert_eq!(list(&UrlFilter::status_class(2)).total, 2);
    assert_eq!(list(&UrlFilter::internal()).total, 2);
    assert_eq!(
        list(&UrlFilter {
            url_contains: Some("missing".into()),
            ..Default::default()
        })
        .total,
        1
    );
    assert_eq!(
        list(&UrlFilter {
            status_code: Some(404),
            ..Default::default()
        })
        .rows[0]
            .url,
        "https://a.test/missing"
    );
    assert_eq!(
        list(&UrlFilter {
            max_depth: Some(0),
            ..Default::default()
        })
        .total,
        1
    );

    // Descending sort on a nullable column still returns every row.
    let by_words =
        queries::list_urls(conn, &UrlFilter::default(), SortBy::WordCount, true, 100, 0).unwrap();
    assert_eq!(by_words.rows.len(), 3);
    assert_eq!(by_words.rows[0].word_count, Some(321));

    assert_eq!(queries::status_code_summary(conn).unwrap().len(), 2);
    assert_eq!(
        queries::depth_distribution(conn).unwrap(),
        vec![(0, 1), (1, 1)]
    );
}

#[test]
fn sitemap_entries_are_kept_for_the_orphan_audit() {
    let mut store = Store::open_in_memory().unwrap();
    store
        .enqueue(&[frontier("https://a.test/linked", 0)])
        .unwrap();

    let entry = |loc: &str| SitemapEntry {
        loc: loc.to_string(),
        lastmod: Some("2026-01-01".into()),
        changefreq: None,
        priority: Some(0.8),
    };
    store
        .apply(&WriteBatch {
            sitemap_entries: vec![
                (
                    "https://a.test/sitemap.xml".into(),
                    entry("https://a.test/linked"),
                    "https://a.test/linked".into(),
                ),
                (
                    "https://a.test/sitemap.xml".into(),
                    entry("https://a.test/orphan"),
                    "https://a.test/orphan".into(),
                ),
            ],
            ..Default::default()
        })
        .unwrap();

    let rows = queries::sitemap_urls(store.connection(), 100).unwrap();
    assert_eq!(rows.len(), 2);
    // The orphan is in the sitemap but has no matching crawled URL row.
    let orphan = rows
        .iter()
        .find(|(url, _)| url.ends_with("orphan"))
        .unwrap();
    assert!(orphan.1.is_none());
    let linked = rows
        .iter()
        .find(|(url, _)| url.ends_with("linked"))
        .unwrap();
    assert!(linked.1.is_some());
}

#[test]
fn log_entries_are_recorded_with_their_level() {
    let store = Store::open_in_memory().unwrap();
    store
        .log(
            LogLevel::Warning,
            "robots.txt unavailable",
            Some("https://a.test/robots.txt"),
        )
        .unwrap();
    store.log(LogLevel::Info, "crawl started", None).unwrap();

    let entries = store.log_entries().unwrap();
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].0, "warning");
    assert_eq!(entries[0].2.as_deref(), Some("https://a.test/robots.txt"));
}

#[test]
fn a_simhash_survives_the_round_trip_through_sqlites_signed_integers() {
    let mut store = Store::open_in_memory().unwrap();
    store.enqueue(&[frontier("https://a.test/", 0)]).unwrap();
    let claimed = store.claim(1).unwrap();
    let mut result = result_for(claimed[0].id, &claimed[0].url);
    // A value with the top bit set, which SQLite stores as a negative integer.
    let original = u64::MAX - 12345;
    result.parsed.as_mut().unwrap().text_simhash = Some(original);
    store
        .apply(&WriteBatch {
            results: vec![result],
            ..Default::default()
        })
        .unwrap();

    let stored: i64 = store
        .connection()
        .query_row(
            "SELECT text_simhash FROM urls WHERE id = ?1",
            [claimed[0].id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(stored as u64, original);
}

#[test]
fn opening_a_database_from_a_newer_build_fails_loudly() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("crawl.db");
    {
        let store = Store::open(&path).unwrap();
        store.set_meta("schema_version", "999").unwrap();
    }
    match Store::open(&path) {
        Err(EngineError::SchemaMismatch { found, .. }) => assert_eq!(found, 999),
        other => panic!("expected a schema mismatch, got {other:?}"),
    }
}

#[test]
fn a_crawl_survives_being_closed_and_reopened() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("crawl.db");

    {
        let mut store = Store::open(&path).unwrap();
        store
            .initialise_crawl(&CrawlConfig::spider("https://a.test/"), "a.test")
            .unwrap();
        store
            .enqueue(&[
                frontier("https://a.test/", 0),
                frontier("https://a.test/x", 1),
            ])
            .unwrap();
        let claimed = store.claim(1).unwrap();
        store
            .apply(&WriteBatch {
                results: vec![result_for(claimed[0].id, &claimed[0].url)],
                ..Default::default()
            })
            .unwrap();
        store.checkpoint().unwrap();
    }

    let store = Store::open(&path).unwrap();
    let stats = store.stats().unwrap();
    assert_eq!((stats.total, stats.done, stats.queued), (2, 1, 1));
    assert_eq!(store.stored_config().unwrap().unwrap().seeds.len(), 1);
}
