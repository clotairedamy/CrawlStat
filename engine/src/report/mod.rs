//! The client report: one crawl, written up for someone who will never open
//! CrawlStat.
//!
//! [`build`] gathers what the app already knows — health, the ranked plan,
//! the accessibility verdict, what changed since the last crawl — and
//! [`html`] lays it out as a single self-contained page: no scripts, no web
//! fonts, no external files, so it opens anywhere, survives being emailed and
//! prints to a clean A4 PDF (see [`crate::chrome`]).
//!
//! Every number in it is one the app shows too. The report adds layout and
//! plain sentences, never a new estimate.

mod html;

#[cfg(test)]
mod tests;

pub use html::html;

use crate::a11y::{self, ComplianceReport, Standard, Verdict};
use crate::audit::{AuditConfig, AuditReport};
use crate::compare::{self, Comparison};
use crate::error::Result;
use crate::plan::{self, ActionPlan};
use crate::store::{CrawlStats, Store};

/// Choices the person exporting makes.
#[derive(Debug, Clone)]
pub struct ReportOptions {
    /// "Prepared by …" on the cover: an agency, a consultant, a team.
    pub prepared_by: Option<String>,
    /// The brand colour, as `#rrggbb`. Anything else keeps the default.
    pub accent: Option<String>,
    pub standard: Standard,
    /// Jobs listed under "What to fix first".
    pub max_jobs: usize,
    /// The report the local model wrote, in Markdown, shown as a summary
    /// after the cover. The figures around it are always the crawl's own.
    pub narrative: Option<String>,
}

impl Default for ReportOptions {
    fn default() -> Self {
        Self {
            prepared_by: None,
            accent: None,
            standard: Standard::default(),
            max_jobs: 8,
            narrative: None,
        }
    }
}

/// Response times over the crawl's successful HTML pages.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Speed {
    pub pages: u64,
    pub median_ms: Option<u64>,
    pub p90_ms: Option<u64>,
    pub slow_pages: u64,
    pub slow_ms: u64,
}

/// Everything the report shows, gathered before any of it is laid out.
pub struct SiteReport {
    pub site: String,
    pub start_url: Option<String>,
    /// RFC 3339, when the crawl finished (or started, if it never did).
    pub crawled_at: Option<String>,
    pub generated_at: chrono::DateTime<chrono::Utc>,
    pub options: ReportOptions,
    pub stats: CrawlStats,
    pub audit: AuditReport,
    pub plan: ActionPlan,
    pub speed: Speed,
    /// `None` when nothing was checked, so the section is left out rather
    /// than shown empty.
    pub accessibility: Option<ComplianceReport>,
    /// Against the previous crawl of the same site, when there is one.
    pub comparison: Option<Comparison>,
    pub rendered_javascript: bool,
}

impl SiteReport {
    /// Share of answered URLs that answered well — the same figure as the
    /// app's health ring, so the report and the screen never disagree.
    pub fn health(&self) -> Option<f64> {
        let stats = &self.stats;
        let fine = stats.status_2xx + stats.status_3xx;
        let answered = fine + stats.status_4xx + stats.status_5xx + stats.failed;
        (answered > 0).then(|| fine as f64 / answered as f64)
    }

    pub fn verdict(&self) -> &'static str {
        match self.health() {
            None => "Nothing fetched yet",
            Some(h) if h >= 0.98 => "Healthy",
            Some(h) if h >= 0.9 => "Some problems",
            Some(_) => "Needs work",
        }
    }
}

/// Gather a report for the crawl in `store`, compared with `previous` when
/// given.
pub fn build(
    store: &Store,
    previous: Option<&Store>,
    options: ReportOptions,
) -> Result<SiteReport> {
    let config = store.stored_config()?;
    let audits: AuditConfig = config
        .as_ref()
        .map(|c| c.audits.clone())
        .unwrap_or_default();
    let start_url = config.as_ref().and_then(|config| {
        config.seeds.iter().find_map(|seed| match seed {
            crate::config::Seed::Url { url } | crate::config::Seed::Sitemap { url } => {
                Some(url.clone())
            }
            crate::config::Seed::List { urls } => urls.first().cloned(),
        })
    });
    let site = store
        .meta("root_host")?
        .or_else(|| start_url.clone())
        .unwrap_or_else(|| "Untitled crawl".into());
    let crawled_at = match store.meta("finished_at")? {
        Some(at) => Some(at),
        None => store.meta("created_at")?,
    };

    let accessibility = a11y::report(store.connection(), options.standard)?;
    let accessibility = (accessibility.verdict != Verdict::NotChecked).then_some(accessibility);
    let comparison = match previous {
        Some(before) => Some(compare::compare(before.connection(), store.connection())?),
        None => None,
    };

    Ok(SiteReport {
        site,
        start_url,
        crawled_at,
        generated_at: chrono::Utc::now(),
        stats: store.stats()?,
        audit: store.audit_report(&audits)?,
        plan: plan::build(store.connection())?,
        speed: speed(store, audits.slow_response_ms)?,
        accessibility,
        comparison,
        rendered_javascript: config.is_some_and(|c| c.render_javascript),
        options,
    })
}

fn speed(store: &Store, slow_ms: u64) -> Result<Speed> {
    let mut statement = store.connection().prepare(
        "SELECT response_time_ms FROM urls
         WHERE is_internal = 1 AND kind = 0 AND response_time_ms IS NOT NULL
           AND status_code BETWEEN 200 AND 299
         ORDER BY response_time_ms",
    )?;
    let times: Vec<u64> = statement
        .query_map([], |row| row.get::<_, i64>(0))?
        .collect::<std::result::Result<Vec<_>, _>>()?
        .into_iter()
        .map(|ms| ms.max(0) as u64)
        .collect();
    let at = |q: f64| -> Option<u64> {
        if times.is_empty() {
            return None;
        }
        let index = ((times.len() - 1) as f64 * q).round() as usize;
        times.get(index).copied()
    };
    Ok(Speed {
        pages: times.len() as u64,
        median_ms: at(0.5),
        p90_ms: at(0.9),
        slow_pages: times.iter().filter(|&&ms| ms >= slow_ms).count() as u64,
        slow_ms,
    })
}
