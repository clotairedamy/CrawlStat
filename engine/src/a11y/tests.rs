use super::*;
use crate::store::Store;

fn store() -> Store {
    Store::open_in_memory().unwrap()
}

/// A checked, successful internal page.
fn page(conn: &Connection, id: i64, url: &str) {
    conn.execute(
        "INSERT INTO urls (id, url, dedup_key, is_internal, status_code, content_type, word_count, a11y_checked)
         VALUES (?1, ?2, ?2, 1, 200, 'text/html', 400, 1)",
        params![id, url],
    )
    .unwrap();
}

fn finding(conn: &Connection, url_id: i64, rule: &str, sample: &str) {
    conn.execute(
        "INSERT INTO a11y_findings (url_id, rule, count, sample, sample_key) VALUES (?1, ?2, 1, ?3, ?4)",
        params![url_id, rule, sample, crate::parse::accessibility::template_key(sample)],
    )
    .unwrap();
}

fn status_of(report: &ComplianceReport, id: &str) -> Status {
    report
        .criteria
        .iter()
        .find(|result| result.criterion.id == id)
        .unwrap_or_else(|| panic!("{id} not in report"))
        .status
}

#[test]
fn standards_have_the_right_number_of_criteria() {
    assert_eq!(Standard::Wcag20Aa.criteria().count(), 38);
    assert_eq!(Standard::Wcag21Aa.criteria().count(), 50);
    assert_eq!(Standard::Wcag22Aa.criteria().count(), 55);
    assert!(Standard::Wcag21Aa.criteria().any(|c| c.id == "4.1.1"));
    assert!(!Standard::Wcag22Aa.criteria().any(|c| c.id == "4.1.1"));
    for standard in Standard::ALL {
        assert_eq!(Standard::parse(standard.id()), Some(standard));
    }
}

#[test]
fn criteria_ids_are_unique_and_link_to_w3c() {
    let mut seen = HashSet::new();
    for criterion in wcag::CRITERIA {
        assert!(seen.insert(criterion.id), "{} listed twice", criterion.id);
        assert!(criterion.understanding_url().starts_with("https://www.w3.org/WAI/"));
    }
}

#[test]
fn an_empty_crawl_is_not_checked() {
    let store = store();
    let report = report(store.connection(), Standard::Wcag21Aa).unwrap();
    assert_eq!(report.verdict, Verdict::NotChecked);
    assert_eq!(report.criteria.len(), 50);
    assert!(report.criteria.iter().all(|c| c.status == Status::Untested));
}

#[test]
fn a_clean_crawl_is_incomplete_not_conformant() {
    let store = store();
    let conn = store.connection();
    page(conn, 1, "https://example.com/");
    let report = report(conn, Standard::Wcag21Aa).unwrap();
    assert_eq!(report.verdict, Verdict::Incomplete);
    assert_eq!(status_of(&report, "1.1.1"), Status::ChecksPassed);
    assert_eq!(status_of(&report, "1.4.3"), Status::Untested);
    assert!(report.rules.is_empty());
}

#[test]
fn template_failures_are_grouped_and_ranked() {
    let store = store();
    let conn = store.connection();
    for id in 1..=4 {
        page(conn, id, &format!("https://example.com/p/{id}"));
        // The same header icon link on every page, differing only by a number.
        finding(conn, id, rule::LINK_NO_NAME, &format!("<a href=\"/cart?v={id}\"></a>"));
    }
    finding(conn, 1, rule::VAGUE_LINK_TEXT, "<a href=\"/x\">Read more</a>");

    let report = report(conn, Standard::Wcag21Aa).unwrap();
    assert_eq!(report.verdict, Verdict::DoesNotConform);
    let top = &report.rules[0];
    assert_eq!(top.info.id, rule::LINK_NO_NAME);
    assert_eq!(top.pages, 4);
    assert!(top.template_fix);
    assert_eq!(top.groups.len(), 1);
    assert_eq!(top.groups[0].pages, 4);
    assert_eq!(top.groups[0].example_urls.len(), 3);

    assert_eq!(status_of(&report, "4.1.2"), Status::Fail);
    let link_purpose = report.criteria.iter().find(|c| c.criterion.id == "2.4.4").unwrap();
    assert_eq!(link_purpose.status, Status::Fail);
    assert_eq!(link_purpose.failing_pages, 4);
    assert_eq!(link_purpose.vpat, "Does Not Support");
}

#[test]
fn review_only_rules_need_review_rather_than_fail() {
    let store = store();
    let conn = store.connection();
    page(conn, 1, "https://example.com/");
    finding(conn, 1, rule::TABLE_NO_HEADERS, "<table>");
    let report = report(conn, Standard::Wcag21Aa).unwrap();
    assert_eq!(status_of(&report, "1.3.1"), Status::NeedsReview);
    assert_eq!(report.verdict, Verdict::Incomplete);
}

#[test]
fn manual_reviews_fold_in_but_cannot_hide_a_failure() {
    let store = store();
    let conn = store.connection();
    page(conn, 1, "https://example.com/");
    page(conn, 2, "https://example.com/b");
    page(conn, 3, "https://example.com/c");
    finding(conn, 1, rule::IMG_NO_ALT, "<img src=\"/a.png\">");

    set_review(conn, "1.4.3", "pass", "Checked with CCA").unwrap();
    set_review(conn, "1.1.1", "pass", "").unwrap();
    set_review(conn, "1.2.2", "na", "").unwrap();

    let report = report(conn, Standard::Wcag21Aa).unwrap();
    assert_eq!(status_of(&report, "1.4.3"), Status::Pass);
    assert_eq!(status_of(&report, "1.2.2"), Status::NotApplicable);
    let alt = report.criteria.iter().find(|c| c.criterion.id == "1.1.1").unwrap();
    assert_eq!(alt.status, Status::Fail);
    assert!(alt.conflict);
    assert_eq!(alt.vpat, "Partially Supports");

    set_review(conn, "1.4.3", "", "").unwrap();
    assert!(super::review(conn, "1.4.3").unwrap().is_none());
    assert!(set_review(conn, "9.9.9", "pass", "").is_err());
    assert!(set_review(conn, "1.4.3", "maybe", "").is_err());
}

#[test]
fn every_criterion_settled_conforms() {
    let store = store();
    let conn = store.connection();
    page(conn, 1, "https://example.com/");
    for criterion in Standard::Wcag22Aa.criteria() {
        set_review(conn, criterion.id, "pass", "").unwrap();
    }
    let report = report(conn, Standard::Wcag22Aa).unwrap();
    assert_eq!(report.verdict, Verdict::Conforms);
    assert_eq!(report.counts.pass, 55);
}

#[test]
fn media_criteria_hint_not_applicable_when_no_media_found() {
    let store = store();
    let conn = store.connection();
    page(conn, 1, "https://example.com/");
    let report = report(conn, Standard::Wcag21Aa).unwrap();
    let captions = report.criteria.iter().find(|c| c.criterion.id == "1.2.2").unwrap();
    assert!(captions.not_applicable_hint.is_some());

    conn.execute(
        "INSERT INTO a11y_findings (url_id, rule, count) VALUES (1, 'has_video', 1)",
        [],
    )
    .unwrap();
    let report = super::report(conn, Standard::Wcag21Aa).unwrap();
    assert_eq!(report.coverage.video_pages, 1);
    let captions = report.criteria.iter().find(|c| c.criterion.id == "1.2.2").unwrap();
    assert!(captions.not_applicable_hint.is_none());
}

#[test]
fn duplicate_ids_are_a_best_practice_under_wcag_22() {
    let store = store();
    let conn = store.connection();
    page(conn, 1, "https://example.com/");
    finding(conn, 1, rule::DUPLICATE_ID, "<div id=\"x\">");
    let older = report(conn, Standard::Wcag21Aa).unwrap();
    assert_eq!(status_of(&older, "4.1.1"), Status::Fail);
    let newer = report(conn, Standard::Wcag22Aa).unwrap();
    assert!(newer.rules[0].best_practice());
    assert_eq!(newer.verdict, Verdict::Incomplete);
}

#[test]
fn unchecked_and_thin_pages_are_reported() {
    let store = store();
    let conn = store.connection();
    page(conn, 1, "https://example.com/");
    conn.execute("UPDATE urls SET word_count = 3 WHERE id = 1", []).unwrap();
    conn.execute(
        "INSERT INTO urls (id, url, dedup_key, status_code, content_type) VALUES (2, 'https://example.com/old', 'o', 200, 'text/html')",
        [],
    )
    .unwrap();
    let report = report(conn, Standard::Wcag21Aa).unwrap();
    assert_eq!(report.coverage.pages_checked, 1);
    assert_eq!(report.coverage.pages_unchecked, 1);
    assert_eq!(report.coverage.thin_pages, 1);
    assert_eq!(report.coverage.thin_examples, vec!["https://example.com/".to_string()]);
}

#[test]
fn rule_pages_lists_samples() {
    let store = store();
    let conn = store.connection();
    page(conn, 1, "https://example.com/");
    conn.execute(
        "INSERT INTO a11y_findings (url_id, rule, count, sample, sample_key, more)
         VALUES (1, 'img_no_alt', 3, '<img src=\"/a\">', 'k', '<img src=\"/b\">\n<img src=\"/c\">')",
        [],
    )
    .unwrap();
    let pages = rule_pages(conn, rule::IMG_NO_ALT, 10).unwrap();
    assert_eq!(pages.len(), 1);
    assert_eq!(pages[0].count, 3);
    assert_eq!(pages[0].samples.len(), 3);
}

#[test]
fn checking_markup_reports_failures_first() {
    let check = check_markup(
        r#"<html><head><title>Hi</title></head><body><main>
           <img src="/a.png"><a href="/x">Read more</a>
           <p>Plenty of words so this page does not look like a script-rendered shell at all,
           because it has real content written in it for people to read.</p></main></body></html>"#,
        "https://example.com/",
    )
    .unwrap();
    let ids: Vec<&str> = check.findings.iter().map(|f| f.info.id).collect();
    assert!(ids.contains(&rule::PAGE_NO_LANG));
    assert!(ids.contains(&rule::IMG_NO_ALT));
    assert!(ids.contains(&rule::VAGUE_LINK_TEXT));
    assert_eq!(check.findings.last().unwrap().info.certainty, Certainty::Review);
    assert!(check.passed.contains(&rule::PAGE_NO_TITLE));
    assert!(!check.likely_script_rendered);
    assert!(check_markup("<html></html>", "not a url").is_err());
}

#[test]
fn markdown_has_a_vpat_table_and_disclaimer() {
    let store = store();
    let conn = store.connection();
    page(conn, 1, "https://example.com/");
    finding(conn, 1, rule::PAGE_NO_LANG, "<html>");
    let report = report(conn, Standard::Wcag21Aa).unwrap();
    let text = markdown(&report, "example.com", "2026-10-24");
    assert!(text.contains("| Criterion | Level | Conformance | Remarks |"));
    assert!(text.contains("| 3.1.1 Language of Page | A | Does Not Support |"));
    assert!(text.contains("not legal advice"));
    assert!(text.contains("ADA Title II"));
}
