//! Audits: turning a finished crawl into a prioritised list of problems.
//!
//! The pass runs after the crawl, over the data already in SQLite. Most checks
//! are a single query from [`catalog::AUDITS`]; a few need real computation and
//! live in [`duplicates`], [`hreflang`] and [`link_score`].
//!
//! Results go into two tables: `issues` (one row per issue type, with a count)
//! and `issue_urls` (the affected URLs, with a short per-URL detail). The UI
//! reads the first for its Issues panel and the second to filter the URL table
//! when an issue is clicked.
//!
//! ## Order of work
//!
//! 1. **Indexability.** Which pages a search engine could actually index is
//!    needed by half the audits, so it is computed once into a column rather
//!    than re-derived in every query.
//! 2. **Link score.** A simplified PageRank over the internal link graph.
//! 3. **The audits themselves**, each independent of the others.
//!
//! Every step is chunked or streaming, so auditing a large crawl costs no more
//! memory than crawling it did.

pub mod catalog;
mod duplicates;
mod hreflang;
mod indexability;
mod link_score;

use std::time::Instant;

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

pub use catalog::{Audit, AUDITS};

use crate::error::Result;
use crate::hash;
use crate::model::ResourceKind;
use crate::store::{schema, Store};

/// How serious an issue is. Ordered worst-first, so `ORDER BY severity` is the
/// order the UI wants.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[repr(i64)]
pub enum Severity {
    /// Breaks something: the page is unreachable, or search engines cannot use
    /// it at all.
    Error = 0,
    /// Costs traffic or wastes crawl budget, but nothing is broken.
    Warning = 1,
    /// Worth knowing. Often deliberate, and listed so it can be confirmed.
    Notice = 2,
}

impl Severity {
    pub fn as_str(self) -> &'static str {
        match self {
            Severity::Error => "error",
            Severity::Warning => "warning",
            Severity::Notice => "notice",
        }
    }

    pub fn from_i64(value: i64) -> Option<Self> {
        Some(match value {
            0 => Severity::Error,
            1 => Severity::Warning,
            2 => Severity::Notice,
            _ => return None,
        })
    }
}

/// How an audit finds its affected URLs.
#[derive(Debug, Clone, Copy)]
pub enum Check {
    /// A query returning `url_id` and `detail` columns. Placeholders in braces
    /// are substituted from [`AuditConfig`] first.
    Sql(&'static str),
    /// Something SQL cannot express.
    Rust(RustCheck),
}

/// The checks that need real computation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RustCheck {
    /// Simhash clustering: pages that are similar but not identical.
    NearDuplicates,
    /// Language-tag validation against the ISO code lists.
    HreflangInvalidCodes,
}

/// Thresholds for the audits that have one.
///
/// Stored with the crawl so a re-run reproduces the same findings, and exposed
/// in the app's settings. The defaults are the conventional SEO guides, not
/// hard rules — which is why most of the issues they drive are warnings or
/// notices rather than errors.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct AuditConfig {
    pub title_min_chars: u32,
    pub title_max_chars: u32,
    pub description_min_chars: u32,
    pub description_max_chars: u32,
    /// More hops than this counts as a redirect chain.
    pub max_redirect_hops: u32,
    pub slow_response_ms: u64,
    pub large_image_bytes: u64,
    /// Clicks from the start URL before a page counts as buried.
    pub max_depth_notice: u32,
    /// Simhash bits that may differ before two pages stop being near-duplicates.
    pub near_duplicate_bits: u32,
    /// Pages shorter than this are not compared by simhash: the measure gets
    /// unreliable on thin content. See [`crate::hash`].
    pub near_duplicate_min_words: u32,
    pub link_score_iterations: u32,
    pub link_score_damping: f64,
}

impl Default for AuditConfig {
    fn default() -> Self {
        Self {
            // 30–60 characters is the usual advice. The real limit is pixel
            // width, which we do not measure, so these are deliberately soft.
            title_min_chars: 30,
            title_max_chars: 60,
            description_min_chars: 70,
            description_max_chars: 155,
            max_redirect_hops: 2,
            slow_response_ms: 1_000,
            large_image_bytes: 100 * 1024,
            max_depth_notice: 5,
            near_duplicate_bits: hash::NEAR_DUPLICATE_THRESHOLD,
            near_duplicate_min_words: hash::MIN_WORDS_FOR_SIMHASH_COMPARISON,
            // PageRank converges well before this on a site-sized graph.
            link_score_iterations: 20,
            link_score_damping: 0.85,
        }
    }
}

impl AuditConfig {
    /// Replace `{placeholder}` markers with this configuration's numbers.
    ///
    /// Every value substituted is an integer or a float from this struct, never
    /// user text, so the result cannot contain injected SQL. `site_is_https`
    /// comes from the crawl rather than the configuration.
    pub fn apply(&self, template: &str, site_is_https: bool) -> String {
        let similarity = 100.0 - (self.near_duplicate_bits as f64 / 64.0 * 100.0);
        let pairs: [(&str, String); 11] = [
            ("{title_min_chars}", self.title_min_chars.to_string()),
            ("{title_max_chars}", self.title_max_chars.to_string()),
            (
                "{description_min_chars}",
                self.description_min_chars.to_string(),
            ),
            (
                "{description_max_chars}",
                self.description_max_chars.to_string(),
            ),
            ("{max_redirect_hops}", self.max_redirect_hops.to_string()),
            ("{slow_response_ms}", self.slow_response_ms.to_string()),
            ("{large_image_bytes}", self.large_image_bytes.to_string()),
            (
                "{large_image_kb}",
                (self.large_image_bytes / 1024).to_string(),
            ),
            ("{max_depth_notice}", self.max_depth_notice.to_string()),
            ("{near_duplicate_similarity}", format!("{similarity:.0}")),
            ("{site_is_https}", i64::from(site_is_https).to_string()),
        ];

        let mut out = template.to_string();
        for (marker, value) in pairs {
            if out.contains(marker) {
                out = out.replace(marker, &value);
            }
        }
        out
    }
}

/// One issue type that was found, with its text already substituted.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IssueSummary {
    pub id: String,
    pub title: String,
    pub severity: Severity,
    pub description: String,
    pub how_to_fix: String,
    pub docs_slug: String,
    pub url_count: u64,
}

/// The result of an audit pass.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AuditReport {
    /// Issues that were actually found, worst first.
    pub issues: Vec<IssueSummary>,
    pub errors: u64,
    pub warnings: u64,
    pub notices: u64,
    /// `(depth, page count)` for crawled internal pages.
    pub depth_distribution: Vec<(u32, u64)>,
    pub indexable_pages: u64,
    pub non_indexable_pages: u64,
    pub elapsed_ms: u64,
}

impl AuditReport {
    /// Total affected URLs across every issue. A URL may appear in several.
    pub fn total_findings(&self) -> u64 {
        self.issues.iter().map(|issue| issue.url_count).sum()
    }

    pub fn summary(&self) -> AuditSummary {
        AuditSummary {
            errors: self.errors,
            warnings: self.warnings,
            notices: self.notices,
            findings: self.total_findings(),
            elapsed_ms: self.elapsed_ms,
        }
    }

    pub fn issues_of(&self, severity: Severity) -> impl Iterator<Item = &IssueSummary> {
        self.issues
            .iter()
            .filter(move |issue| issue.severity == severity)
    }

    pub fn issue(&self, id: &str) -> Option<&IssueSummary> {
        self.issues.iter().find(|issue| issue.id == id)
    }
}

/// Rows updated per transaction when writing derived columns, matching the
/// store's own chunking so memory stays flat.
const CHUNK: i64 = 5_000;

/// Run every audit over a finished crawl.
pub fn run(store: &mut Store, config: &AuditConfig) -> Result<AuditReport> {
    let started = Instant::now();
    let site_is_https = site_is_https(store)?;

    schema::reset_issues(store.connection())?;
    create_indexable_view(store.connection())?;
    indexability::compute(store, CHUNK)?;
    link_score::compute(store, config, CHUNK)?;

    for audit in AUDITS {
        let findings = match audit.check {
            Check::Sql(sql) => {
                run_sql_audit(store.connection(), &config.apply(sql, site_is_https))?
            }
            Check::Rust(RustCheck::NearDuplicates) => duplicates::near_duplicates(
                store.connection(),
                config.near_duplicate_bits,
                config.near_duplicate_min_words,
            )?,
            Check::Rust(RustCheck::HreflangInvalidCodes) => {
                hreflang::invalid_codes(store.connection())?
            }
        };
        record(store, audit, &findings, config, site_is_https)?;
    }

    build_report(store, config, site_is_https, Some(started))
}

/// Read the issues an earlier audit pass recorded, without re-running it.
///
/// The descriptions and fix text come from the catalogue in this binary, so
/// improving the wording improves what an old crawl file shows.
pub fn read_report(store: &Store, config: &AuditConfig) -> Result<AuditReport> {
    let site_is_https = site_is_https(store)?;
    build_report(store, config, site_is_https, None)
}

/// A small, copyable summary for the progress stream and the crawl result.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuditSummary {
    pub errors: u64,
    pub warnings: u64,
    pub notices: u64,
    /// Affected URLs across every issue. One URL may appear in several.
    pub findings: u64,
    pub elapsed_ms: u64,
}

/// Was the crawl of an HTTPS site? Decides whether http:// URLs are a problem.
fn site_is_https(store: &Store) -> Result<bool> {
    // The seed is the most reliable signal; fall back to the majority of
    // crawled internal URLs if there is no stored config.
    if let Some(config) = store.stored_config()? {
        if let Some(url) = config.seeds.iter().find_map(|seed| match seed {
            crate::config::Seed::Url { url } | crate::config::Seed::Sitemap { url } => {
                Some(url.clone())
            }
            crate::config::Seed::List { urls } => urls.first().cloned(),
        }) {
            return Ok(url.starts_with("https://") || !url.contains("://"));
        }
    }
    let https: i64 = store.connection().query_row(
        "SELECT count(*) FROM urls WHERE is_internal = 1 AND url LIKE 'https://%'",
        [],
        |row| row.get(0),
    )?;
    let http: i64 = store.connection().query_row(
        "SELECT count(*) FROM urls WHERE is_internal = 1 AND url LIKE 'http://%'",
        [],
        |row| row.get(0),
    )?;
    Ok(https >= http)
}

/// The `indexable_pages` view the catalogue's queries read from.
fn create_indexable_view(conn: &Connection) -> Result<()> {
    conn.execute_batch(&format!(
        "DROP VIEW IF EXISTS {view};
         CREATE VIEW {view} AS
             SELECT * FROM urls
             WHERE is_internal = 1
               AND kind = {html}
               AND status_code = 200
               AND indexable = 1;",
        view = catalog::INDEXABLE_VIEW,
        html = ResourceKind::Html as i64,
    ))?;
    Ok(())
}

/// Run one SQL audit and collect `(url_id, detail)` rows.
fn run_sql_audit(conn: &Connection, sql: &str) -> Result<Vec<(i64, Option<String>)>> {
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt
        .query_map([], |row| Ok((row.get("url_id")?, row.get("detail")?)))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

/// Write one audit's findings.
fn record(
    store: &mut Store,
    audit: &Audit,
    findings: &[(i64, Option<String>)],
    config: &AuditConfig,
    site_is_https: bool,
) -> Result<()> {
    if findings.is_empty() {
        return Ok(());
    }

    let conn = store.connection_mut();
    let tx = conn.transaction()?;
    tx.execute(
        "INSERT INTO issues (issue_id, title, severity, url_count) VALUES (?1, ?2, ?3, 0)
         ON CONFLICT (issue_id) DO UPDATE SET url_count = 0",
        params![
            audit.id,
            config.apply(audit.title, site_is_https),
            audit.severity as i64
        ],
    )?;

    {
        // A URL can be reported once per issue; the primary key enforces it, so
        // queries do not have to bother with DISTINCT.
        let mut stmt = tx.prepare_cached(
            "INSERT OR IGNORE INTO issue_urls (issue_id, url_id, detail) VALUES (?1, ?2, ?3)",
        )?;
        for (url_id, detail) in findings {
            stmt.execute(params![audit.id, url_id, detail])?;
        }
    }

    // Counted from the table rather than from `findings`: a query may return
    // several rows for one URL (a page linking to two broken URLs returns two),
    // and the count the UI shows must match the number of rows it will display.
    tx.execute(
        "UPDATE issues SET url_count =
             (SELECT count(*) FROM issue_urls WHERE issue_urls.issue_id = issues.issue_id)
         WHERE issue_id = ?1",
        params![audit.id],
    )?;
    tx.execute(
        "DELETE FROM issues WHERE issue_id = ?1 AND url_count = 0",
        params![audit.id],
    )?;
    tx.commit()?;
    Ok(())
}

fn build_report(
    store: &Store,
    config: &AuditConfig,
    site_is_https: bool,
    started: Option<Instant>,
) -> Result<AuditReport> {
    let conn = store.connection();

    let mut stmt = conn.prepare(
        "SELECT issue_id, url_count FROM issues WHERE url_count > 0
         ORDER BY severity, url_count DESC",
    )?;
    let found = stmt
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)? as u64))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    let mut issues = Vec::with_capacity(found.len());
    for (id, url_count) in found {
        // The text lives in the binary, so a build with better wording improves
        // existing crawl files too.
        let Some(audit) = catalog::find(&id) else {
            continue;
        };
        issues.push(IssueSummary {
            id,
            title: config.apply(audit.title, site_is_https),
            severity: audit.severity,
            description: config.apply(audit.description, site_is_https),
            how_to_fix: config.apply(audit.how_to_fix, site_is_https),
            docs_slug: audit.docs_slug.to_string(),
            url_count,
        });
    }

    let count_of = |severity: Severity| {
        issues
            .iter()
            .filter(|issue| issue.severity == severity)
            .count() as u64
    };

    let indexable: i64 =
        conn.query_row("SELECT count(*) FROM urls WHERE indexable = 1", [], |row| {
            row.get(0)
        })?;
    let non_indexable: i64 =
        conn.query_row("SELECT count(*) FROM urls WHERE indexable = 0", [], |row| {
            row.get(0)
        })?;

    Ok(AuditReport {
        errors: count_of(Severity::Error),
        warnings: count_of(Severity::Warning),
        notices: count_of(Severity::Notice),
        issues,
        depth_distribution: crate::store::queries::depth_distribution(conn)?,
        indexable_pages: indexable as u64,
        non_indexable_pages: non_indexable as u64,
        elapsed_ms: started
            .map(|at| at.elapsed().as_millis() as u64)
            .unwrap_or(0),
    })
}

#[cfg(test)]
mod tests;
