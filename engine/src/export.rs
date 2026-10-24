//! Exporting a crawl to CSV.
//!
//! Streaming by design: rows are read from SQLite a page at a time and written
//! straight out, so exporting a 500,000-URL crawl costs no more memory than
//! exporting a small one. Nothing is collected into a `Vec` first.
//!
//! CSV is the free-tier export. XLSX and the shareable HTML audit report are
//! Pro features and live behind the licence check.

use std::fs::File;
use std::io::BufWriter;
use std::path::Path;

use rusqlite::Connection;

use crate::audit::AuditConfig;
use crate::error::Result;
use crate::store::queries::{self, SortBy, UrlFilter};
use crate::store::Store;

/// Rows read per query while streaming an export.
const PAGE: u32 = 1_000;

/// The columns a URL export writes, in order.
///
/// Kept explicit rather than derived from the row struct: this is a file format
/// people build spreadsheets on, so the order and the headings are a contract.
const URL_HEADERS: &[&str] = &[
    "URL",
    "Status Code",
    "Status",
    "Indexable",
    "Indexability Reason",
    "Content Type",
    "Depth",
    "Size (bytes)",
    "Response Time (ms)",
    "Title",
    "Title Length",
    "Meta Description",
    "Meta Description Length",
    "Meta Robots",
    "H1",
    "H1 Count",
    "Word Count",
    "Canonical",
    "Self Canonical",
    "Inlinks",
    "Outlinks (internal)",
    "Outlinks (external)",
    "Redirects",
    "Link Score",
    "Error",
];

/// Write the URL table matching `filter` as CSV. Returns the row count.
pub fn urls_to_csv(
    store: &Store,
    filter: &UrlFilter,
    sort: SortBy,
    descending: bool,
    path: &Path,
) -> Result<u64> {
    let mut writer = csv::Writer::from_writer(BufWriter::new(File::create(path)?));
    writer.write_record(URL_HEADERS)?;

    let mut written = 0u64;
    let mut offset = 0u64;
    loop {
        let page = queries::list_urls(store.connection(), filter, sort, descending, PAGE, offset)?;
        if page.rows.is_empty() {
            break;
        }
        for row in &page.rows {
            writer.write_record([
                row.url.as_str(),
                &opt_number(row.status_code),
                row.state.map(state_label).unwrap_or(""),
                opt_bool(row.indexable),
                row.indexability_reason.as_deref().unwrap_or(""),
                row.content_type.as_deref().unwrap_or(""),
                &row.depth.to_string(),
                &opt_number(row.body_bytes),
                &opt_number(row.response_time_ms),
                row.title.as_deref().unwrap_or(""),
                &opt_number(row.title_length),
                row.meta_description.as_deref().unwrap_or(""),
                &opt_number(row.meta_description_length),
                row.meta_robots.as_deref().unwrap_or(""),
                row.h1.as_deref().unwrap_or(""),
                &opt_number(row.h1_count),
                &opt_number(row.word_count),
                row.canonical.as_deref().unwrap_or(""),
                opt_bool(row.canonical_is_self),
                &row.inlinks.to_string(),
                &opt_number(row.internal_links),
                &opt_number(row.external_links),
                &row.redirect_count.to_string(),
                &row.link_score
                    .map(|score| format!("{score:.1}"))
                    .unwrap_or_default(),
                row.error_kind.as_deref().unwrap_or(""),
            ])?;
            written += 1;
        }
        offset += page.rows.len() as u64;
        if offset >= page.total {
            break;
        }
    }

    writer.flush()?;
    Ok(written)
}

/// Write the issue summary — one row per issue type — as CSV.
pub fn issues_to_csv(store: &Store, config: &AuditConfig, path: &Path) -> Result<u64> {
    let report = store.audit_report(config)?;
    let mut writer = csv::Writer::from_writer(BufWriter::new(File::create(path)?));
    writer.write_record([
        "Issue",
        "Severity",
        "URLs",
        "Description",
        "How To Fix",
        "Documentation",
    ])?;

    for issue in &report.issues {
        writer.write_record([
            issue.title.as_str(),
            issue.severity.as_str(),
            &issue.url_count.to_string(),
            &issue.description,
            &issue.how_to_fix,
            &format!("https://crawlstat.com/docs/audits/{}", issue.docs_slug),
        ])?;
    }
    writer.flush()?;
    Ok(report.issues.len() as u64)
}

/// Write every URL affected by one issue, with its per-URL detail.
pub fn issue_urls_to_csv(
    store: &Store,
    issue_id: &str,
    sort: SortBy,
    descending: bool,
    path: &Path,
) -> Result<u64> {
    // The URL rows, filtered to this issue, plus the issue's own detail column.
    let details = issue_detail_map(store.connection(), issue_id)?;
    let filter = UrlFilter::for_issue(issue_id);

    let mut writer = csv::Writer::from_writer(BufWriter::new(File::create(path)?));
    let mut headers: Vec<&str> = vec!["Detail"];
    headers.extend_from_slice(URL_HEADERS);
    writer.write_record(&headers)?;

    let mut written = 0u64;
    let mut offset = 0u64;
    loop {
        let page = queries::list_urls(store.connection(), &filter, sort, descending, PAGE, offset)?;
        if page.rows.is_empty() {
            break;
        }
        for row in &page.rows {
            let detail = details.get(&row.id).cloned().unwrap_or_default();
            writer.write_record([
                detail.as_str(),
                row.url.as_str(),
                &opt_number(row.status_code),
                row.state.map(state_label).unwrap_or(""),
                opt_bool(row.indexable),
                row.indexability_reason.as_deref().unwrap_or(""),
                row.content_type.as_deref().unwrap_or(""),
                &row.depth.to_string(),
                &opt_number(row.body_bytes),
                &opt_number(row.response_time_ms),
                row.title.as_deref().unwrap_or(""),
                &opt_number(row.title_length),
                row.meta_description.as_deref().unwrap_or(""),
                &opt_number(row.meta_description_length),
                row.meta_robots.as_deref().unwrap_or(""),
                row.h1.as_deref().unwrap_or(""),
                &opt_number(row.h1_count),
                &opt_number(row.word_count),
                row.canonical.as_deref().unwrap_or(""),
                opt_bool(row.canonical_is_self),
                &row.inlinks.to_string(),
                &opt_number(row.internal_links),
                &opt_number(row.external_links),
                &row.redirect_count.to_string(),
                &row.link_score
                    .map(|score| format!("{score:.1}"))
                    .unwrap_or_default(),
                row.error_kind.as_deref().unwrap_or(""),
            ])?;
            written += 1;
        }
        offset += page.rows.len() as u64;
        if offset >= page.total {
            break;
        }
    }

    writer.flush()?;
    Ok(written)
}

/// Write the whole internal link graph as `source,target,anchor,...`.
pub fn links_to_csv(store: &Store, path: &Path) -> Result<u64> {
    let mut writer = csv::Writer::from_writer(BufWriter::new(File::create(path)?));
    writer.write_record([
        "Source",
        "Target",
        "Anchor Text",
        "Type",
        "Position",
        "Rel",
        "Nofollow",
        "Target Status",
    ])?;

    let mut stmt = store.connection().prepare(
        "SELECT src.url, links.target_url, links.anchor_text, links.link_source,
                links.position, links.rel, links.nofollow, target.status_code
         FROM links
         JOIN urls AS src ON src.id = links.source_id
         LEFT JOIN urls AS target ON target.dedup_key = links.target_key
         ORDER BY links.id",
    )?;
    let mut rows = stmt.query([])?;

    let mut written = 0u64;
    while let Some(row) = rows.next()? {
        writer.write_record([
            row.get::<_, String>(0)?.as_str(),
            row.get::<_, Option<String>>(1)?
                .unwrap_or_default()
                .as_str(),
            row.get::<_, Option<String>>(2)?
                .unwrap_or_default()
                .as_str(),
            link_source_label(row.get::<_, i64>(3)?),
            position_label(row.get::<_, i64>(4)?),
            row.get::<_, Option<String>>(5)?
                .unwrap_or_default()
                .as_str(),
            if row.get::<_, i64>(6)? != 0 {
                "yes"
            } else {
                "no"
            },
            &row.get::<_, Option<i64>>(7)?
                .map(|code| code.to_string())
                .unwrap_or_default(),
        ])?;
        written += 1;
    }
    writer.flush()?;
    Ok(written)
}

/// Build `url_id -> detail` for one issue.
fn issue_detail_map(
    conn: &Connection,
    issue_id: &str,
) -> Result<std::collections::HashMap<i64, String>> {
    let mut stmt =
        conn.prepare("SELECT url_id, COALESCE(detail, '') FROM issue_urls WHERE issue_id = ?1")?;
    let map = stmt
        .query_map([issue_id], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
        })?
        .collect::<rusqlite::Result<_>>()?;
    Ok(map)
}

fn opt_number<T: std::fmt::Display>(value: Option<T>) -> String {
    value.map(|v| v.to_string()).unwrap_or_default()
}

fn opt_bool(value: Option<bool>) -> &'static str {
    match value {
        Some(true) => "yes",
        Some(false) => "no",
        None => "",
    }
}

fn state_label(state: crate::model::UrlState) -> &'static str {
    use crate::model::UrlState as S;
    match state {
        S::Queued => "Queued",
        S::InProgress => "In progress",
        S::Done => "Crawled",
        S::Failed => "Failed",
        S::Skipped => "Not crawled",
        S::BlockedByRobots => "Blocked by robots.txt",
    }
}

fn link_source_label(value: i64) -> &'static str {
    match value {
        1 => "Image",
        2 => "Stylesheet",
        3 => "Script",
        4 => "Canonical",
        5 => "Hreflang",
        6 => "Redirect",
        7 => "Sitemap",
        8 => "Form",
        9 => "Iframe",
        10 => "Meta refresh",
        _ => "Hyperlink",
    }
}

fn position_label(value: i64) -> &'static str {
    match value {
        1 => "Navigation",
        2 => "Header",
        3 => "Footer",
        4 => "Aside",
        5 => "Unknown",
        _ => "Content",
    }
}

/// `csv::Error` is not one of the engine's own failures, so it is folded into
/// the generic variant rather than given its own.
impl From<csv::Error> for crate::error::EngineError {
    fn from(error: csv::Error) -> Self {
        crate::error::EngineError::Other(anyhow::anyhow!("CSV write failed: {error}"))
    }
}

#[cfg(test)]
mod tests;
