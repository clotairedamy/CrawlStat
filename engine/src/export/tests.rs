//! CSV export tests.
//!
//! The point of these is the file *format*: a header that spreadsheets can
//! read, correct quoting for titles containing commas and quotes, and a row
//! count that matches the table the user was looking at.

use std::path::Path;

use super::*;
use crate::config::CrawlConfig;
use crate::model::{
    CrawledUrl, DiscoveredLink, FrontierUrl, LinkPosition, LinkSource, ParsedPage, ResourceKind,
    UrlState,
};
use crate::store::WriteBatch;

/// A finished, audited crawl with two good pages and a 404.
fn store_with_rows() -> (tempfile::TempDir, Store) {
    // A comma and a quote, to prove the writer escapes rather than corrupts
    // the row.
    store_with_title(r#"Home, "the" page"#)
}

/// As [`store_with_rows`], with `title` on both good pages.
fn store_with_title(title: &str) -> (tempfile::TempDir, Store) {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(dir.path().join("crawl.db")).unwrap();
    store
        .initialise_crawl(&CrawlConfig::spider("https://a.test/"), "a.test")
        .unwrap();

    let seeds: Vec<FrontierUrl> = [
        "https://a.test/",
        "https://a.test/about",
        "https://a.test/gone",
    ]
    .iter()
    .map(|url| FrontierUrl {
        url: (*url).to_string(),
        key: (*url).to_string(),
        depth: 0,
        discovered_from: None,
        via: None,
        kind: ResourceKind::Html,
        is_internal: true,
        state: UrlState::Queued,
    })
    .collect();
    store.enqueue(&seeds).unwrap();

    let claimed = store.claim(3).unwrap();
    let mut results = Vec::new();
    for page in &claimed {
        let mut result = CrawledUrl {
            url_id: page.id,
            url: page.url.clone(),
            state: UrlState::Done,
            kind: ResourceKind::Html,
            status_code: Some(200),
            final_status_code: None,
            response_time: Some(std::time::Duration::from_millis(80)),
            ttfb: None,
            content_type: Some("text/html".into()),
            content_length: Some(1_000),
            body_bytes: Some(1_000),
            content_encoding: None,
            x_robots_tag: None,
            final_url: None,
            redirect_chain: Vec::new(),
            redirect_loop: false,
            headers: vec![("content-type".into(), "text/html".into())],
            content_hash: None,
            truncated: false,
            html: None,
            parsed: Some(ParsedPage {
                title: Some(title.into()),
                h1: vec!["Heading".into()],
                word_count: 120,
                links: vec![DiscoveredLink {
                    raw_href: "/about".into(),
                    target: Some("https://a.test/about".into()),
                    target_key: Some("https://a.test/about".into()),
                    anchor_text: "About".into(),
                    rel: None,
                    position: LinkPosition::Content,
                    nofollow: false,
                    source: LinkSource::Anchor,
                    is_internal: true,
                }],
                ..Default::default()
            }),
            internal_links: 1,
            external_links: 0,
            error_kind: None,
            error_message: None,
            attempts: 1,
            fetched_at: 0,
        };
        if page.url.ends_with("/gone") {
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
    store.finalize("finished").unwrap();
    store.audit(&AuditConfig::default()).unwrap();
    (dir, store)
}

/// The file as text, without the BOM; that has its own test.
fn read(path: &Path) -> String {
    let text = std::fs::read_to_string(path).unwrap();
    text.strip_prefix('\u{feff}').unwrap_or(&text).to_string()
}

/// The title column of a URL export, as a CSV reader sees it.
fn titles(path: &Path) -> Vec<String> {
    let mut reader = csv::Reader::from_path(path).unwrap();
    reader
        .records()
        .map(|record| record.unwrap().get(9).unwrap_or_default().to_string())
        .collect()
}

#[test]
fn urls_export_has_a_header_and_one_row_per_url() {
    let (dir, store) = store_with_rows();
    let out = dir.path().join("urls.csv");
    let written = urls_to_csv(&store, &UrlFilter::default(), SortBy::Url, false, &out).unwrap();

    assert_eq!(written, 3);
    let text = read(&out);
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), 4, "a header plus three rows");
    assert!(lines[0].starts_with("URL,Status Code,Status,Indexable"));
    assert!(text.contains("https://a.test/gone,404"));
}

#[test]
fn fields_containing_commas_and_quotes_are_escaped() {
    let (dir, store) = store_with_rows();
    let out = dir.path().join("urls.csv");
    urls_to_csv(&store, &UrlFilter::default(), SortBy::Url, false, &out).unwrap();

    // Inner quotes are doubled and the field is wrapped.
    assert!(
        read(&out).contains(r#""Home, ""the"" page""#),
        "title was not escaped"
    );

    // And it reads back as the original value.
    assert!(titles(&out).contains(&r#"Home, "the" page"#.to_string()));
}

#[test]
fn a_filter_narrows_the_export() {
    let (dir, store) = store_with_rows();
    let out = dir.path().join("errors.csv");
    let written = urls_to_csv(
        &store,
        &UrlFilter::status_class(4),
        SortBy::Url,
        false,
        &out,
    )
    .unwrap();

    assert_eq!(written, 1);
    assert_eq!(read(&out).lines().count(), 2);
}

#[test]
fn exporting_nothing_still_writes_the_header() {
    let (dir, store) = store_with_rows();
    let out = dir.path().join("none.csv");
    let filter = UrlFilter {
        url_contains: Some("nothing-matches".into()),
        ..Default::default()
    };
    let written = urls_to_csv(&store, &filter, SortBy::Url, false, &out).unwrap();

    assert_eq!(written, 0);
    // A spreadsheet opening this should still see the columns.
    assert_eq!(read(&out).lines().count(), 1);
}

#[test]
fn issues_export_carries_the_fix_text_and_a_docs_link() {
    let (dir, store) = store_with_rows();
    let out = dir.path().join("issues.csv");
    let written = issues_to_csv(&store, &AuditConfig::default(), &out).unwrap();

    assert!(written > 0);
    let text = read(&out);
    assert!(text.starts_with("Issue,Severity,URLs,Description,How To Fix,Documentation"));
    assert!(text.contains("https://crawlstat.com/docs/audits/"));
    assert!(text.contains("error"));
}

#[test]
fn an_issues_url_export_leads_with_the_per_url_detail() {
    let (dir, store) = store_with_rows();
    let out = dir.path().join("status_4xx.csv");
    let written = issue_urls_to_csv(&store, "status_4xx", SortBy::Url, false, &out).unwrap();

    assert_eq!(written, 1);
    let text = read(&out);
    assert!(text.starts_with("Detail,URL,"));
    assert!(
        text.contains("HTTP 404"),
        "the detail column should be filled: {text}"
    );
}

#[test]
fn the_link_graph_exports_with_resolved_target_statuses() {
    let (dir, store) = store_with_rows();
    let out = dir.path().join("links.csv");
    let written = links_to_csv(&store, &out).unwrap();

    assert!(written >= 1);
    let text = read(&out);
    assert!(text.starts_with("Source,Target,Anchor Text,Type,Position,Rel,Nofollow,Target Status"));
    assert!(text.contains("Hyperlink"));
    assert!(text.contains("https://a.test/about"));
}

#[test]
fn every_export_starts_with_a_utf8_bom() {
    let (dir, store) = store_with_rows();
    let urls = dir.path().join("urls.csv");
    let issues = dir.path().join("issues.csv");
    let issue_urls = dir.path().join("status_4xx.csv");
    let links = dir.path().join("links.csv");
    urls_to_csv(&store, &UrlFilter::default(), SortBy::Url, false, &urls).unwrap();
    issues_to_csv(&store, &AuditConfig::default(), &issues).unwrap();
    issue_urls_to_csv(&store, "status_4xx", SortBy::Url, false, &issue_urls).unwrap();
    links_to_csv(&store, &links).unwrap();

    for path in [&urls, &issues, &issue_urls, &links] {
        let bytes = std::fs::read(path).unwrap();
        assert!(bytes.starts_with(UTF8_BOM), "{} has no BOM", path.display());
    }

    // The BOM must not leak into the first heading.
    let mut reader = csv::Reader::from_path(&urls).unwrap();
    let headers = reader.headers().unwrap();
    assert_eq!(headers.get(0), Some("URL"));
}

#[test]
fn a_formula_in_a_crawled_title_is_neutralised() {
    let (dir, store) = store_with_title(r#"=HYPERLINK("https://evil.test","x")"#);
    let out = dir.path().join("urls.csv");
    urls_to_csv(&store, &UrlFilter::default(), SortBy::Url, false, &out).unwrap();

    assert!(titles(&out).contains(&r#"'=HYPERLINK("https://evil.test","x")"#.to_string()));
}

#[test]
fn an_ordinary_title_is_written_untouched() {
    let (dir, store) = store_with_title("Café & crème — about us");
    let out = dir.path().join("urls.csv");
    urls_to_csv(&store, &UrlFilter::default(), SortBy::Url, false, &out).unwrap();

    assert!(titles(&out).contains(&"Café & crème — about us".to_string()));
}

#[test]
fn cell_prefixes_only_formula_triggers() {
    for risky in ["=1+1", "+1", "@SUM(A1)", "\tx", "\rx", "-1+1", "-x", "-"] {
        assert_eq!(cell(risky), format!("'{risky}"), "{risky:?}");
    }
    for safe in ["", "Home", "-5", "-1.5", "a=b", "https://a.test/"] {
        assert_eq!(cell(safe), safe, "{safe:?}");
    }
}
