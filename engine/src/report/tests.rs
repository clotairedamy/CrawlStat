use std::sync::Arc;

use super::*;
use crate::config::CrawlConfig;
use crate::engine::Crawler;
use crate::fetch::mock::{MockFetcher, MockResponse};

/// A small site with something to report: a missing title, a 404 and an
/// image without alt text.
async fn crawl(with_404: bool) -> Store {
    let fetcher = MockFetcher::new();
    fetcher.page(
        "https://shop.test/",
        r#"<html lang="en"><head><title>Shop & "friends"</title></head><body><h1>Shop</h1>
           <a href="/a">A</a><a href="/b">B</a><a href="/gone">Gone</a>
           <img src="/x.png"></body></html>"#,
    );
    fetcher.page(
        "https://shop.test/a",
        "<html><body><p>No title here.</p></body></html>",
    );
    fetcher.page(
        "https://shop.test/b",
        "<html><head><title>B</title></head><body><h1>B</h1></body></html>",
    );
    if with_404 {
        fetcher.insert("https://shop.test/gone", MockResponse::status(404));
    } else {
        fetcher.page(
            "https://shop.test/gone",
            "<html><head><title>Back</title></head><body><h1>Back</h1></body></html>",
        );
    }
    let store = Store::open_in_memory().unwrap();
    let (crawler, _handle) = Crawler::start_with_fetcher(
        CrawlConfig::spider("https://shop.test/"),
        store,
        Arc::new(fetcher),
        None,
    )
    .await
    .unwrap();
    crawler.run_and_close().await.unwrap().1
}

#[tokio::test]
async fn gathers_the_same_figures_as_the_app() {
    let store = crawl(true).await;
    let report = build(&store, None, ReportOptions::default()).unwrap();

    assert_eq!(report.site, "shop.test");
    assert_eq!(report.start_url.as_deref(), Some("https://shop.test/"));
    assert!(report.stats.status_4xx >= 1, "the missing page");
    let health = report.health().unwrap();
    assert!(health < 1.0 && health > 0.5, "{health}");
    assert!(!report.plan.items.is_empty());
    assert!(report.speed.pages >= 3);
    assert!(report.comparison.is_none());
    assert!(!report.rendered_javascript);
}

#[tokio::test]
async fn the_page_is_self_contained_and_escaped() {
    let store = crawl(true).await;
    let page = html(&build(&store, None, ReportOptions::default()).unwrap());

    assert!(page.starts_with("<!DOCTYPE html>"));
    assert!(page.contains("shop.test"));
    assert!(page.contains("What to fix first"));
    assert!(page.contains("Every issue found"));
    for outside in [
        "<script",
        "src=\"http",
        "href=\"http",
        "@import",
        "url(http",
    ] {
        assert!(
            !page.contains(outside),
            "{outside} would make the page depend on the network"
        );
    }
    // Nothing from the crawled site lands in the page unescaped.
    assert!(!page.contains("Shop & \"friends\""));
    assert!(
        !page.contains("Since the last crawl"),
        "no comparison was asked for"
    );
}

#[tokio::test]
async fn a_previous_crawl_adds_the_progress_section() {
    let before = crawl(true).await;
    let after = crawl(false).await;
    let report = build(&after, Some(&before), ReportOptions::default()).unwrap();

    let comparison = report.comparison.as_ref().unwrap();
    assert!(comparison.pages_after > 0);
    let page = html(&report);
    assert!(page.contains("Since the last crawl"));
}

#[tokio::test]
async fn options_reach_the_page_and_bad_colours_are_ignored() {
    let store = crawl(true).await;
    let options = ReportOptions {
        prepared_by: Some("Acme <Agency>".into()),
        accent: Some("#12ab9F".into()),
        ..ReportOptions::default()
    };
    let page = html(&build(&store, None, options).unwrap());
    assert!(page.contains("Prepared by <strong>Acme &lt;Agency&gt;</strong>"));
    assert!(page.contains("--accent:#12ab9F"));

    let options = ReportOptions {
        accent: Some("red;}body{display:none".into()),
        ..ReportOptions::default()
    };
    let page = html(&build(&store, None, options).unwrap());
    assert!(page.contains("--accent:#4f46e5"));
    assert!(!page.contains("display:none"));
}

#[test]
fn health_and_verdict_follow_the_app() {
    let store = Store::open_in_memory().unwrap();
    let mut report = build(&store, None, ReportOptions::default()).unwrap();
    assert_eq!(report.health(), None);
    assert_eq!(report.verdict(), "Nothing fetched yet");

    report.stats.status_2xx = 97;
    report.stats.status_3xx = 1;
    report.stats.status_4xx = 2;
    assert_eq!(report.health(), Some(0.98));
    assert_eq!(report.verdict(), "Healthy");
    report.stats.failed = 5;
    assert_eq!(report.verdict(), "Some problems");
    report.stats.status_5xx = 50;
    assert_eq!(report.verdict(), "Needs work");
    // An empty crawl still lays out.
    assert!(html(&report).contains("Every issue found"));
}

#[tokio::test]
async fn the_written_report_becomes_the_summary() {
    let store = crawl(true).await;
    let options = ReportOptions {
        narrative: Some(
            "# Shop is in good shape\n\nMostly **healthy**, with *one* broken link and \
             `<img>` tags missing alt text.\n\n## Fix first\n\n1. **Restore `/gone`.** Now\n2. Add alt \
             text\n   to the logo\n\n- snake_case_names stay put\n- <script>alert(1)</script>\n"
                .into(),
        ),
        ..ReportOptions::default()
    };
    let page = html(&build(&store, None, options).unwrap());

    assert!(page.contains("<h2>Shop is in good shape</h2>"));
    assert!(page.contains("Mostly <strong>healthy</strong>, with <em>one</em> broken link"));
    assert!(page.contains("<code>&lt;img&gt;</code>"));
    assert!(page.contains("<h3>Fix first</h3>"));
    assert!(page
        .contains("<ol><li><strong>Restore <code>/gone</code>.</strong> Now</li><li>Add alt text to the logo</li></ol>"));
    assert!(page.contains("<li>snake_case_names stay put</li>"));
    assert!(!page.contains("<script>"));
    // The summary sits between the cover and the overview.
    let summary = page.find("class=\"narrative\"").unwrap();
    assert!(summary < page.find("Where things stand").unwrap());
}

#[tokio::test]
async fn no_written_report_no_summary() {
    let store = crawl(true).await;
    let blank = ReportOptions {
        narrative: Some("  \n".into()),
        ..ReportOptions::default()
    };
    assert!(!html(&build(&store, None, blank).unwrap()).contains("class=\"narrative\""));
}
