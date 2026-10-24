//! Reading a crawl: the project list, the results tables, the issue panel and
//! the exports.
//!
//! Every function takes the database path rather than relying on an "open
//! project" having been set first, so Dart never has to sequence calls. The
//! connection is cached behind the path, so paging through a large table does
//! not reopen the file each time.

use std::path::Path;

use crawlstat_engine::audit::AuditConfig;
use crawlstat_engine::export;
use crawlstat_engine::store::queries;
use flutter_rust_bridge::frb;

use super::types::{
    AuditReport, HeaderPair, ImageRow, Issue, LinkRow, ProjectInfo, ProjectStats, RedirectHop,
    UrlDetail, UrlIssue, UrlPage, UrlQuery, UrlRow,
};
use crate::runtime;

/// Inlinks and outlinks shown in the detail panel before it paginates.
const DETAIL_LINK_LIMIT: u32 = 500;

/// Open a crawl file and describe it, for the projects screen.
pub fn open_project(db_path: String) -> Result<ProjectInfo, String> {
    project_info(db_path)
}

/// Release the cached connection — called when the user closes a project.
#[frb(sync)]
pub fn close_project() {
    runtime::close_project();
}

/// Summarise a crawl file without opening it in the UI.
pub fn project_info(db_path: String) -> Result<ProjectInfo, String> {
    let size_bytes = std::fs::metadata(&db_path)
        .map(|meta| meta.len() as i64)
        .unwrap_or(0);

    runtime::with_store(&db_path, |store| {
        let stats = store.stats()?;
        let config = store.stored_config()?;
        let start_url = config.as_ref().and_then(|config| {
            config.seeds.iter().find_map(|seed| match seed {
                crawlstat_engine::config::Seed::Url { url }
                | crawlstat_engine::config::Seed::Sitemap { url } => Some(url.clone()),
                crawlstat_engine::config::Seed::List { urls } => urls.first().cloned(),
            })
        });

        // Issue counts come from the stored audit, so the project list can show
        // "3 errors" without re-auditing every file it lists.
        let audit_config = config.map(|config| config.audits).unwrap_or_default();
        let report = store.audit_report(&audit_config)?;

        Ok(ProjectInfo {
            path: db_path.clone(),
            start_url,
            root_host: store.meta("root_host")?,
            state: store.meta("state")?,
            created_at: store.meta("created_at")?,
            finished_at: store.meta("finished_at")?,
            stats: ProjectStats::from(stats),
            errors: report.errors as i64,
            warnings: report.warnings as i64,
            size_bytes,
        })
    })
    .map_err(|error| error.to_string())
}

/// Live counters, polled by the crawl screen between progress events.
pub fn project_stats(db_path: String) -> Result<ProjectStats, String> {
    runtime::with_store(&db_path, |store| Ok(ProjectStats::from(store.stats()?)))
        .map_err(|error| error.to_string())
}

/// One page of the results table.
pub fn list_urls(db_path: String, query: UrlQuery) -> Result<UrlPage, String> {
    let (filter, sort, descending, limit, offset) = query.to_filter();
    runtime::with_store(&db_path, |store| {
        let page =
            queries::list_urls(store.connection(), &filter, sort, descending, limit, offset)?;
        Ok(UrlPage {
            rows: page.rows.into_iter().map(UrlRow::from).collect(),
            total: page.total as i64,
            offset: page.offset as i64,
        })
    })
    .map_err(|error| error.to_string())
}

/// Everything the URL detail panel shows, in one call.
pub fn url_detail(db_path: String, url_id: i64) -> Result<UrlDetail, String> {
    runtime::with_store(&db_path, |store| {
        let conn = store.connection();
        let row = queries::url_by_id(conn, url_id)?.ok_or_else(|| {
            crawlstat_engine::error::EngineError::Config(format!("no URL with id {url_id}"))
        })?;

        let issue_rows = queries::issues_for_url(conn, url_id)?;
        let catalogued = |id: &str| crawlstat_engine::audit::catalog::find(id);
        let issues = issue_rows
            .into_iter()
            .map(|(issue_id, detail)| {
                let audit = catalogued(&issue_id);
                UrlIssue {
                    title: audit
                        .map(|a| a.title.to_string())
                        .unwrap_or_else(|| issue_id.clone()),
                    severity: audit
                        .map(|a| a.severity.as_str().to_string())
                        .unwrap_or_else(|| "notice".to_string()),
                    issue_id,
                    detail,
                }
            })
            .collect();

        Ok(UrlDetail {
            headers: queries::headers(conn, url_id)?
                .into_iter()
                .map(|(name, value)| HeaderPair { name, value })
                .collect(),
            inlinks: queries::inlinks(conn, url_id, DETAIL_LINK_LIMIT)?
                .into_iter()
                .map(LinkRow::from)
                .collect(),
            outlinks: queries::outlinks(conn, url_id, DETAIL_LINK_LIMIT)?
                .into_iter()
                .map(LinkRow::from)
                .collect(),
            redirect_chain: queries::redirect_chain(conn, url_id)?
                .into_iter()
                .map(|(hop, from_url, to_url, status)| RedirectHop {
                    hop: hop as i64,
                    from_url,
                    to_url,
                    status: status as i64,
                })
                .collect(),
            images: queries::images(conn, url_id)?
                .into_iter()
                .map(ImageRow::from)
                .collect(),
            issues,
            row: UrlRow::from(row),
        })
    })
    .map_err(|error| error.to_string())
}

/// The issues recorded by the last audit pass.
pub fn list_issues(db_path: String) -> Result<AuditReport, String> {
    runtime::with_store(&db_path, |store| {
        let config = audit_config(store)?;
        Ok(AuditReport::from(store.audit_report(&config)?))
    })
    .map_err(|error| error.to_string())
}

/// Re-run the audits, optionally with new thresholds.
///
/// `thresholds_json` is an [`AuditConfig`] as JSON; `None` keeps the crawl's
/// own. The checks live in this binary, so re-running also picks up anything
/// added since the crawl was made.
pub fn run_audit(db_path: String, thresholds_json: Option<String>) -> Result<AuditReport, String> {
    let overrides: Option<AuditConfig> = match thresholds_json {
        Some(json) => Some(
            serde_json::from_str(&json)
                .map_err(|error| format!("audit thresholds are not valid: {error}"))?,
        ),
        None => None,
    };

    runtime::with_store_mut(&db_path, |store| {
        let config = match &overrides {
            Some(config) => config.clone(),
            None => audit_config(store)?,
        };
        Ok(AuditReport::from(store.audit(&config)?))
    })
    .map_err(|error| error.to_string())
}

/// One issue's full text, for the panel that explains it.
#[frb(sync)]
pub fn issue_definition(issue_id: String) -> Option<Issue> {
    let audit = crawlstat_engine::audit::catalog::find(&issue_id)?;
    let config = AuditConfig::default();
    Some(Issue {
        id: audit.id.to_string(),
        title: config.apply(audit.title, true),
        severity: audit.severity.as_str().to_string(),
        description: config.apply(audit.description, true),
        how_to_fix: config.apply(audit.how_to_fix, true),
        docs_slug: audit.docs_slug.to_string(),
        url_count: 0,
    })
}

/// `(status code, count)` for the response-codes tab.
pub fn status_summary(db_path: String) -> Result<Vec<StatusCount>, String> {
    runtime::with_store(&db_path, |store| {
        Ok(queries::status_code_summary(store.connection())?
            .into_iter()
            .map(|(status, count)| StatusCount {
                status: status as i64,
                count: count as i64,
            })
            .collect())
    })
    .map_err(|error| error.to_string())
}

#[derive(Debug, Clone)]
pub struct StatusCount {
    pub status: i64,
    pub count: i64,
}

/// Export the rows a table view is showing. Returns how many were written.
pub fn export_urls_csv(db_path: String, query: UrlQuery, out_path: String) -> Result<i64, String> {
    let (filter, sort, descending, _, _) = query.to_filter();
    runtime::with_store(&db_path, |store| {
        Ok(export::urls_to_csv(store, &filter, sort, descending, Path::new(&out_path))? as i64)
    })
    .map_err(|error| error.to_string())
}

/// Export the issue summary.
pub fn export_issues_csv(db_path: String, out_path: String) -> Result<i64, String> {
    runtime::with_store(&db_path, |store| {
        let config = audit_config(store)?;
        Ok(export::issues_to_csv(store, &config, Path::new(&out_path))? as i64)
    })
    .map_err(|error| error.to_string())
}

/// Export every URL affected by one issue, with its detail column.
pub fn export_issue_urls_csv(
    db_path: String,
    issue_id: String,
    out_path: String,
) -> Result<i64, String> {
    runtime::with_store(&db_path, |store| {
        Ok(export::issue_urls_to_csv(
            store,
            &issue_id,
            queries::SortBy::Url,
            false,
            Path::new(&out_path),
        )? as i64)
    })
    .map_err(|error| error.to_string())
}

/// Export the link graph, for the site-architecture view and for spreadsheets.
pub fn export_links_csv(db_path: String, out_path: String) -> Result<i64, String> {
    runtime::with_store(&db_path, |store| {
        Ok(export::links_to_csv(store, Path::new(&out_path))? as i64)
    })
    .map_err(|error| error.to_string())
}

/// The crawl's own audit thresholds, falling back to the defaults for a file
/// written before they were stored.
fn audit_config(
    store: &crawlstat_engine::store::Store,
) -> Result<AuditConfig, crawlstat_engine::error::EngineError> {
    Ok(store
        .stored_config()?
        .map(|config| config.audits)
        .unwrap_or_default())
}
