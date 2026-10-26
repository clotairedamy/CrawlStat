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
use crawlstat_engine::integrations::{ollama, secrets};
use crawlstat_engine::plan;
use crawlstat_engine::ux;
use crawlstat_engine::store::{queries, Store};
use flutter_rust_bridge::frb;

// `StreamSink` lives in the generated module, not in the crate root.
use crate::frb_generated::StreamSink;

use super::types::{
    page_view, AuditReport, CrawlRequest, HeaderPair, ImageRow, Issue, LinkRow, PageLens,
    PageLenses, ProjectInfo, ProjectStats, RedirectHop, UrlDetail, UrlIssue, UrlPage, UrlQuery,
    UrlRow, LENSES, THIN_WORDS,
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

/// The settings a crawl was started with, as a request that runs it again.
///
/// Fails for a file with no stored configuration — one written by a build
/// old enough not to record it — since there is nothing to repeat.
pub fn stored_crawl_request(db_path: String) -> Result<CrawlRequest, String> {
    let config = runtime::with_store(&db_path, |store| store.stored_config())
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "this crawl did not record its settings".to_string())?;
    CrawlRequest::from_config(&config).map_err(|error| error.to_string())
}

/// Live counters, polled by the crawl screen between progress events.
pub fn project_stats(db_path: String) -> Result<ProjectStats, String> {
    runtime::with_store(&db_path, |store| Ok(ProjectStats::from(store.stats()?)))
        .map_err(|error| error.to_string())
}

/// One page of the results table.
pub fn list_urls(db_path: String, query: UrlQuery) -> Result<UrlPage, String> {
    let (mut filter, sort, descending, limit, offset) = query.to_filter();
    runtime::with_store(&db_path, |store| {
        filter.view = lens(store, &query)?;
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

/// A page one issue affects, with that page's own reason.
#[derive(Debug, Clone)]
pub struct AffectedPage {
    pub url_id: i64,
    pub url: String,
    pub status: Option<i64>,
    /// Why this page in particular: the over-long title, the broken target.
    pub detail: Option<String>,
}

/// An issue's affected pages, the most valuable first, for the list under
/// the issue's explanation.
pub fn issue_urls(
    db_path: String,
    issue_id: String,
    limit: u32,
) -> Result<Vec<AffectedPage>, String> {
    runtime::with_store(&db_path, |store| {
        let pages = queries::affected_urls(store.connection(), &issue_id, limit)?;
        Ok(pages
            .into_iter()
            .map(|page| AffectedPage {
                url_id: page.url_id,
                url: page.url,
                status: page.status.map(i64::from),
                detail: page.detail.filter(|detail| !detail.is_empty()),
            })
            .collect())
    })
    .map_err(|error| error.to_string())
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
    let (mut filter, sort, descending, _, _) = query.to_filter();
    runtime::with_store(&db_path, |store| {
        filter.view = lens(store, &query)?;
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

/// The lens a query asks for, with the crawl's thresholds.
fn lens(
    store: &Store,
    query: &UrlQuery,
) -> Result<Option<queries::PageView>, crawlstat_engine::error::EngineError> {
    Ok(match query.view.as_deref() {
        Some(id) => page_view(id, &audit_config(store)?),
        None => None,
    })
}

/// How many rows each Pages lens would show for a table view, ignoring any
/// lens the query already has, so the counts stay put while you switch.
pub fn page_lenses(db_path: String, query: UrlQuery) -> Result<PageLenses, String> {
    let (filter, _, _, _, _) = query.to_filter();
    runtime::with_store(&db_path, |store| {
        let config = audit_config(store)?;
        let views: Vec<_> = LENSES
            .iter()
            .filter_map(|(id, _)| page_view(id, &config))
            .collect();
        let counts = queries::count_views(store.connection(), &filter, &views)?;
        let total = queries::list_urls(
            store.connection(),
            &filter,
            queries::SortBy::Id,
            false,
            1,
            0,
        )?
        .total;
        Ok(PageLenses {
            lenses: LENSES
                .iter()
                .zip(counts)
                .map(|((id, label), count)| PageLens {
                    id: id.to_string(),
                    label: label.to_string(),
                    count: count as i64,
                })
                .collect(),
            total: total as i64,
            title_min: config.title_min_chars as i64,
            title_max: config.title_max_chars as i64,
            description_min: config.description_min_chars as i64,
            description_max: config.description_max_chars as i64,
            slow_ms: config.slow_response_ms as i64,
            thin_words: THIN_WORDS,
        })
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

// ─── site structure, for the Visualise screen ────────────────────────────

/// A page in the link diagram.
#[derive(Debug, Clone)]
pub struct SiteNode {
    pub id: i64,
    pub url: String,
    pub label: String,
    pub depth: i64,
    pub status: Option<i64>,
    pub inlinks: i64,
    pub outlinks: i64,
    /// Internal link score, 0–100; 0 when audits have not run.
    pub link_score: f64,
}

/// A followed internal link between two pages in the diagram.
#[derive(Debug, Clone)]
pub struct SiteEdge {
    pub source: i64,
    pub target: i64,
}

/// One top-level section of the site.
#[derive(Debug, Clone)]
pub struct SiteSection {
    pub path: String,
    pub pages: i64,
    pub issues: i64,
}

/// Everything the Visualise screen draws, in one call.
///
/// One call rather than four, because every one of these crosses the
/// Dart/Rust boundary and opens the same database: a screen that assembles
/// itself from four futures spends more time waiting than drawing.
#[derive(Debug, Clone)]
pub struct SiteStructure {
    pub nodes: Vec<SiteNode>,
    pub edges: Vec<SiteEdge>,
    pub sections: Vec<SiteSection>,
    /// `(depth, pages)`, shallowest first.
    pub depths: Vec<DepthCount>,
    pub statuses: Vec<StatusCount>,
    /// How many internal HTML pages the crawl holds, so the screen can say
    /// when the diagram is showing only part of the site.
    pub total_pages: i64,
}

#[derive(Debug, Clone)]
pub struct DepthCount {
    pub depth: i64,
    pub count: i64,
}

/// Read the site structure. `node_limit` caps the diagram; 400 draws fast and
/// still shows the shape of a large site.
pub fn site_structure(db_path: String, node_limit: u32) -> Result<SiteStructure, String> {
    runtime::with_store(&db_path, |store| {
        let conn = store.connection();
        let (nodes, edges) = queries::site_graph(conn, node_limit)?;
        let sections = queries::directory_breakdown(conn, 12)?;
        let depths = queries::depth_distribution(conn)?;
        let statuses = queries::status_code_summary(conn)?;
        let total_pages: i64 = conn.query_row(
            "SELECT count(*) FROM urls
             WHERE is_internal = 1 AND kind = 0 AND status_code IS NOT NULL",
            [],
            |row| row.get(0),
        )?;

        Ok(SiteStructure {
            nodes: nodes
                .into_iter()
                .map(|node| SiteNode {
                    id: node.id,
                    url: node.url,
                    label: node.label,
                    depth: node.depth as i64,
                    status: node.status.map(i64::from),
                    inlinks: node.inlinks as i64,
                    outlinks: node.outlinks as i64,
                    link_score: node.link_score,
                })
                .collect(),
            edges: edges
                .into_iter()
                .map(|edge| SiteEdge {
                    source: edge.source,
                    target: edge.target,
                })
                .collect(),
            sections: sections
                .into_iter()
                .map(|(path, pages, issues)| SiteSection {
                    path,
                    pages: pages as i64,
                    issues: issues as i64,
                })
                .collect(),
            depths: depths
                .into_iter()
                .map(|(depth, count)| DepthCount {
                    depth: depth as i64,
                    count: count as i64,
                })
                .collect(),
            statuses: statuses
                .into_iter()
                .map(|(status, count)| StatusCount {
                    status: status as i64,
                    count: count as i64,
                })
                .collect(),
            total_pages,
        })
    })
    .map_err(|error| error.to_string())
}

// ─── the action plan ─────────────────────────────────────────────────────

/// One of the worst-affected pages for a job.
#[derive(Debug, Clone)]
pub struct PlanExample {
    pub url: String,
    pub detail: Option<String>,
    pub depth: i64,
}

/// One job in the plan.
#[derive(Debug, Clone)]
pub struct PlanItem {
    pub issue_id: String,
    pub title: String,
    /// `error`, `warning` or `notice`.
    pub severity: String,
    pub how_to_fix: String,
    pub docs_slug: String,
    pub pages: i64,
    /// Distinct findings behind those pages — the number of actual changes.
    pub distinct_fixes: i64,
    /// How many pages one change improves.
    pub leverage: f64,
    /// Share of the site's internal link value on the affected pages, 0..1.
    pub reach: f64,
    /// 0..100, relative to everything else in this crawl.
    pub impact: f64,
    /// `template`, `bulk` or `per_page`.
    pub effort: String,
    pub examples: Vec<PlanExample>,
}

#[derive(Debug, Clone)]
pub struct ActionPlan {
    pub items: Vec<PlanItem>,
    pub affected_pages: i64,
    pub total_pages: i64,
    /// Share of link value touched by at least one error, 0..1.
    pub value_at_risk: f64,
}

/// Rank a crawl's findings into a list of jobs, best first.
pub fn action_plan(db_path: String) -> Result<ActionPlan, String> {
    runtime::with_store(&db_path, |store| {
        let plan = plan::build(store.connection())?;
        Ok(ActionPlan {
            affected_pages: plan.affected_pages as i64,
            total_pages: plan.total_pages as i64,
            value_at_risk: plan.value_at_risk,
            items: plan
                .items
                .into_iter()
                .map(|item| PlanItem {
                    issue_id: item.issue_id,
                    title: item.title,
                    severity: item.severity.as_str().to_string(),
                    how_to_fix: item.how_to_fix,
                    docs_slug: item.docs_slug,
                    pages: item.pages as i64,
                    distinct_fixes: item.distinct_fixes as i64,
                    leverage: item.leverage,
                    reach: item.reach,
                    impact: item.impact,
                    effort: item.effort.as_str().to_string(),
                    examples: item
                        .examples
                        .into_iter()
                        .map(|example| PlanExample {
                            url: example.url,
                            detail: example.detail,
                            depth: example.depth as i64,
                        })
                        .collect(),
                })
                .collect(),
        })
    })
    .map_err(|error| error.to_string())
}

// ─── Phase 5: bring your own key ─────────────────────────────────────────

/// A model the local Ollama has installed.
#[derive(Debug, Clone)]
pub struct OllamaModel {
    pub name: String,
    pub size_bytes: i64,
}

/// Where Ollama listens by default. Loopback, so nothing leaves the machine.
pub fn ollama_default_endpoint() -> String {
    ollama::DEFAULT_ENDPOINT.to_string()
}

pub fn ollama_default_model() -> String {
    ollama::DEFAULT_MODEL.to_string()
}

/// Ask the local Ollama what it has. Also doubles as the "test connection"
/// button: if this returns, Ollama is running.
pub fn ollama_models(endpoint: String) -> Result<Vec<OllamaModel>, String> {
    ollama::installed_models(&endpoint)
        .map(|models| {
            models
                .into_iter()
                .filter(|model| model.generates)
                .map(|model| OllamaModel {
                    name: model.name,
                    size_bytes: model.size_bytes as i64,
                })
                .collect()
        })
        .map_err(|error| error.to_string())
}

/// How a report from the local model is coming along.
#[derive(Debug, Clone)]
pub enum ReportEvent {
    /// The model that is writing. Differs from the one asked for when that
    /// one is not installed, so the app can remember the substitute.
    Model {
        name: String,
    },
    /// Everything written so far, not just the latest words.
    Progress {
        text: String,
    },
    Done {
        text: String,
    },
    Failed {
        message: String,
    },
}

/// Have the local model write up the plan, streaming it as it is written.
///
/// Slow — a local model on a laptop takes tens of seconds, plus the time to
/// load it the first time — so it runs on its own thread and the UI shows
/// the report filling in rather than a spinner.
pub fn ollama_write_report(
    db_path: String,
    endpoint: String,
    model: String,
    site: String,
    sink: StreamSink<ReportEvent>,
) -> Result<(), String> {
    std::thread::spawn(move || {
        let outcome = (|| -> Result<String, String> {
            let plan = runtime::with_store(&db_path, |store| Ok(plan::build(store.connection())?))
                .map_err(|error| error.to_string())?;
            let installed =
                ollama::installed_models(&endpoint).map_err(|error| error.to_string())?;
            let model = ollama::pick_model(&installed, &model).ok_or_else(|| {
                format!(
                    "Ollama has no models installed — run `ollama pull {}`",
                    ollama::DEFAULT_MODEL
                )
            })?;
            let _ = sink.add(ReportEvent::Model {
                name: model.clone(),
            });
            ollama::summarise_streaming(&endpoint, &model, &plan, &site, |text| {
                // Fails only once Dart has dropped the stream; stop writing.
                sink.add(ReportEvent::Progress {
                    text: text.to_string(),
                })
                .is_ok()
            })
            .map_err(|error| error.to_string())
        })();
        let _ = sink.add(match outcome {
            Ok(text) => ReportEvent::Done { text },
            Err(message) => ReportEvent::Failed { message },
        });
    });
    Ok(())
}

/// Exactly what would be sent to the model, so the user can read it first.
///
/// A privacy claim nobody can check is worth very little. This is the button
/// next to "Write the summary" that shows the whole brief.
pub fn ollama_brief(db_path: String, site: String) -> Result<String, String> {
    let plan = runtime::with_store(&db_path, |store| Ok(plan::build(store.connection())?))
        .map_err(|error| error.to_string())?;
    Ok(ollama::brief(&plan, &site))
}

/// Store a BYOK credential in the OS keychain. An empty value clears it.
///
/// The value goes straight from the text field into the keychain and is never
/// read back into Dart — [`secret_is_set`] is all the UI can ask.
pub fn store_secret(name: String, value: String) -> Result<(), String> {
    let secret = secrets::Secret::parse(&name).ok_or_else(|| format!("unknown key '{name}'"))?;
    secrets::store(secret, &value).map_err(|error| error.to_string())
}

/// Whether a credential is stored. Never the value.
pub fn secret_is_set(name: String) -> bool {
    secrets::Secret::parse(&name).is_some_and(secrets::is_set)
}

pub fn clear_secret(name: String) -> Result<(), String> {
    let secret = secrets::Secret::parse(&name).ok_or_else(|| format!("unknown key '{name}'"))?;
    secrets::clear(secret).map_err(|error| error.to_string())
}

// ─── directional UX intelligence ─────────────────────────────────────────

/// A way of reading a page, for the persona picker.
#[derive(Debug, Clone)]
pub struct UxPersona {
    pub id: String,
    pub label: String,
    pub describes: String,
}

/// One element's share of a reader's attention.
#[derive(Debug, Clone)]
pub struct UxAttention {
    pub order: i64,
    pub label: String,
    /// `heading`, `link`, `action`, `field`, `image` or `text`.
    pub kind: String,
    /// `header`, `nav`, `main`, `aside`, `footer` or `unknown`.
    pub region: String,
    /// 0..1, and every share in a report sums to one.
    pub share: f64,
    pub confidence: f64,
}

#[derive(Debug, Clone)]
pub struct UxConflict {
    pub labels: Vec<String>,
    pub share: f64,
}

#[derive(Debug, Clone)]
pub struct UxDecision {
    pub order: i64,
    pub options: i64,
    pub share: f64,
}

/// What the model thinks happens when this persona meets this page.
#[derive(Debug, Clone)]
pub struct UxFlow {
    pub url: String,
    pub persona: String,
    /// How the markup was obtained: `stored` or `fetched`.
    pub source: String,
    pub node_count: i64,
    pub attention: Vec<UxAttention>,
    /// How many elements this reader gets through before deciding.
    pub reach: i64,
    pub steps_to_action: Option<i64>,
    pub action_out_of_reach: bool,
    pub conflicts: Vec<UxConflict>,
    pub decisions: Vec<UxDecision>,
    pub dead_end: bool,
    pub coherence: f64,
    pub vague_links: Vec<String>,
}

/// The personas the model knows, for the UI's picker.
pub fn ux_personas() -> Vec<UxPersona> {
    ux::Persona::ALL
        .into_iter()
        .map(|persona| UxPersona {
            id: persona.as_str().to_string(),
            label: persona.label().to_string(),
            describes: persona.describes().to_string(),
        })
        .collect()
}

fn kind_name(kind: ux::NodeKind) -> String {
    match kind {
        ux::NodeKind::Heading(level) => format!("h{level}"),
        ux::NodeKind::Link => "link".into(),
        ux::NodeKind::Action => "action".into(),
        ux::NodeKind::Field => "field".into(),
        ux::NodeKind::Image => "image".into(),
        ux::NodeKind::Text => "text".into(),
    }
}

fn region_name(region: ux::Region) -> String {
    match region {
        ux::Region::Header => "header",
        ux::Region::Nav => "nav",
        ux::Region::Main => "main",
        ux::Region::Aside => "aside",
        ux::Region::Footer => "footer",
        ux::Region::Unknown => "unknown",
    }
    .to_string()
}

/// Analyse one page of the open crawl.
///
/// The markup comes from the crawl file when "Keep raw HTML" was on for that
/// crawl, and is fetched once otherwise — a single request for the page the
/// user is looking at, which is why this is a button rather than something
/// that happens on opening the screen.
pub fn ux_analyse(db_path: String, url_id: i64, persona: String) -> Result<UxFlow, String> {
    let persona = ux::Persona::parse(&persona)
        .ok_or_else(|| format!("unknown persona '{persona}'"))?;

    let (address, stored) = runtime::with_store(&db_path, |store| {
        let row = store.connection().query_row(
            "SELECT url, html FROM urls WHERE id = ?1",
            [url_id],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?)),
        )?;
        Ok(row)
    })
    .map_err(|error| error.to_string())?;

    let (body, source) = match stored.filter(|html| !html.trim().is_empty()) {
        Some(html) => (html, "stored"),
        None => (ux::fetch_markup(&address)?, "fetched"),
    };

    let (nodes, flow) = ux::analyse_markup(&body, persona);

    Ok(UxFlow {
        url: address,
        persona: flow.persona,
        source: source.to_string(),
        node_count: nodes.len() as i64,
        reach: flow.path.len() as i64,
        steps_to_action: flow.steps_to_action.map(|at| at as i64),
        action_out_of_reach: flow.action_out_of_reach,
        dead_end: flow.dead_end,
        coherence: flow.coherence as f64,
        vague_links: flow.vague_links,
        attention: flow
            .attention
            .into_iter()
            .map(|a| UxAttention {
                order: a.order as i64,
                label: a.label,
                kind: kind_name(a.kind),
                region: region_name(a.region),
                share: a.share as f64,
                confidence: a.confidence as f64,
            })
            .collect(),
        conflicts: flow
            .cta_conflicts
            .into_iter()
            .map(|c| UxConflict {
                labels: c.labels,
                share: c.share as f64,
            })
            .collect(),
        decisions: flow
            .decision_nodes
            .into_iter()
            .map(|d| UxDecision {
                order: d.order as i64,
                options: d.options as i64,
                share: d.share as f64,
            })
            .collect(),
    })
}

/// The pages worth looking at first: the best-connected HTML pages in the
/// crawl, which are the ones most visitors actually land on.
pub fn ux_candidate_pages(db_path: String, limit: u32) -> Result<Vec<SiteNode>, String> {
    runtime::with_store(&db_path, |store| {
        let (nodes, _) = queries::site_graph(store.connection(), limit)?;
        Ok(nodes
            .into_iter()
            .map(|node| SiteNode {
                id: node.id,
                url: node.url,
                label: node.label,
                depth: node.depth as i64,
                status: node.status.map(i64::from),
                inlinks: node.inlinks as i64,
                outlinks: node.outlinks as i64,
                link_score: node.link_score,
            })
            .collect())
    })
    .map_err(|error| error.to_string())
}

// ─── crawl comparison ────────────────────────────────────────────────────

/// How one issue changed between two crawls.
#[derive(Debug, Clone)]
pub struct IssueDelta {
    pub issue_id: String,
    pub title: String,
    pub before: i64,
    pub after: i64,
    /// Pages that had it, no longer do, and are still in the crawl.
    pub fixed: i64,
    pub introduced: i64,
    pub impact_before: f64,
    pub impact_after: f64,
}

/// A page that was fine and now is not.
#[derive(Debug, Clone)]
pub struct BrokenPage {
    pub url: String,
    pub status_before: Option<i64>,
    pub status_after: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct CrawlComparison {
    pub pages_before: i64,
    pub pages_after: i64,
    pub affected_before: i64,
    pub affected_after: i64,
    pub value_at_risk_before: f64,
    pub value_at_risk_after: f64,
    pub total_fixed: i64,
    pub total_introduced: i64,
    /// Improvements first, then regressions.
    pub issues: Vec<IssueDelta>,
    pub pages_broken: Vec<BrokenPage>,
    pub pages_added: i64,
    pub pages_removed: i64,
}

/// Compare an earlier crawl with a later one.
///
/// Pro only: comparison is the feature that turns a report into a workflow,
/// and the limit is enforced here rather than by the UI hiding a button.
pub fn compare_crawls(before_path: String, after_path: String) -> Result<CrawlComparison, String> {
    use crawlstat_engine::license;

    if !license::current()
        .entitlement()
        .has(license::features::CRAWL_COMPARISON)
    {
        return Err(
            "Comparing crawls is a Pro feature. See Settings for your licence.".to_string(),
        );
    }

    let before = Store::open(std::path::Path::new(&before_path)).map_err(|e| e.to_string())?;
    let after = Store::open(std::path::Path::new(&after_path)).map_err(|e| e.to_string())?;
    let result = crawlstat_engine::compare::compare(before.connection(), after.connection())
        .map_err(|error| error.to_string())?;

    Ok(CrawlComparison {
        pages_before: result.pages_before as i64,
        pages_after: result.pages_after as i64,
        affected_before: result.affected_before as i64,
        affected_after: result.affected_after as i64,
        value_at_risk_before: result.value_at_risk_before,
        value_at_risk_after: result.value_at_risk_after,
        total_fixed: result.total_fixed() as i64,
        total_introduced: result.total_introduced() as i64,
        pages_added: result.pages_added.len() as i64,
        pages_removed: result.pages_removed.len() as i64,
        pages_broken: result
            .pages_broken
            .iter()
            .map(|page| BrokenPage {
                url: page.url.clone(),
                status_before: page.status_before.map(i64::from),
                status_after: page.status_after.map(i64::from),
            })
            .collect(),
        issues: result
            .issues
            .into_iter()
            .map(|issue| IssueDelta {
                issue_id: issue.issue_id,
                title: issue.title,
                before: issue.before as i64,
                after: issue.after as i64,
                fixed: issue.fixed as i64,
                introduced: issue.introduced as i64,
                impact_before: issue.impact_before,
                impact_after: issue.impact_after,
            })
            .collect(),
    })
}

// ─── journeys: can a visitor find it? ────────────────────────────────────

#[derive(Debug, Clone)]
pub struct JourneyGoal {
    pub id: String,
    pub label: String,
}

#[derive(Debug, Clone)]
pub struct JourneyStep {
    pub from_url: String,
    pub anchor: String,
    pub to_url: String,
    pub confidence: f64,
    /// What else was on offer at this step, best first.
    pub alternatives: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct JourneyResult {
    pub goal_id: String,
    pub goal_label: String,
    pub persona: String,
    /// `reached`, `gave_up`, `dead_end`, `looped` or `no_signal`.
    pub outcome: String,
    pub clicks: i64,
    pub steps: Vec<JourneyStep>,
    /// Whether a page matching the goal exists in the crawl at all.
    pub target_exists: bool,
    pub target_url: Option<String>,
    pub shortest_known: Option<i64>,
    /// Links encountered that point at pages this crawl never fetched.
    pub unfetched_links: i64,
}

pub fn journey_goals() -> Vec<JourneyGoal> {
    ux::journey::preset_goals()
        .into_iter()
        .map(|goal| JourneyGoal {
            id: goal.id,
            label: goal.label,
        })
        .collect()
}

fn to_result(journey: ux::journey::Journey) -> JourneyResult {
    JourneyResult {
        goal_id: journey.goal_id,
        goal_label: journey.goal_label,
        persona: journey.persona,
        outcome: journey.outcome,
        clicks: journey.clicks as i64,
        target_exists: journey.target_exists,
        target_url: journey.target_url,
        shortest_known: journey.shortest_known.map(|d| d as i64),
        unfetched_links: journey.unfetched_links as i64,
        steps: journey
            .steps
            .into_iter()
            .map(|step| JourneyStep {
                from_url: step.from_url,
                anchor: step.anchor,
                to_url: step.to_url,
                confidence: step.confidence as f64,
                alternatives: step.alternatives,
            })
            .collect(),
    }
}

/// Walk the crawled site for every preset goal as one persona.
pub fn run_journeys(db_path: String, persona: String) -> Result<Vec<JourneyResult>, String> {
    let persona =
        ux::Persona::parse(&persona).ok_or_else(|| format!("unknown persona '{persona}'"))?;
    runtime::with_store(&db_path, |store| {
        Ok(ux::journey::simulate_all(store.connection(), persona)?
            .into_iter()
            .map(to_result)
            .collect())
    })
    .map_err(|error| error.to_string())
}

/// Walk it for one goal only.
pub fn run_journey(
    db_path: String,
    persona: String,
    goal_id: String,
) -> Result<JourneyResult, String> {
    let persona =
        ux::Persona::parse(&persona).ok_or_else(|| format!("unknown persona '{persona}'"))?;
    let goal = ux::journey::preset_goals()
        .into_iter()
        .find(|goal| goal.id == goal_id)
        .ok_or_else(|| format!("unknown goal '{goal_id}'"))?;
    runtime::with_store(&db_path, |store| {
        Ok(to_result(ux::journey::simulate(
            store.connection(),
            &goal,
            persona,
        )?))
    })
    .map_err(|error| error.to_string())
}

// ─── a tester's working set ──────────────────────────────────────────────

/// Walk the site for a task written in plain language.
///
/// The entry point that matters for a real study: a usability task is "find
/// the returns policy", never one of the presets.
pub fn run_custom_journey(
    db_path: String,
    persona: String,
    task: String,
) -> Result<JourneyResult, String> {
    let persona =
        ux::Persona::parse(&persona).ok_or_else(|| format!("unknown persona '{persona}'"))?;
    let goal = ux::journey::custom_goal(&task);
    if goal.keywords.is_empty() {
        return Err("Describe the task in a few words — \"find the returns policy\".".into());
    }
    runtime::with_store(&db_path, |store| {
        Ok(to_result(ux::journey::simulate(
            store.connection(),
            &goal,
            persona,
        )?))
    })
    .map_err(|error| error.to_string())
}

/// One information-architecture finding, flattened for the UI.
#[derive(Debug, Clone)]
pub struct IaFinding {
    /// `ambiguous_label`, `inconsistent_label`, `orphan`, `too_deep`
    /// or `crowded`.
    pub kind: String,
    /// The headline, already written as a sentence.
    pub summary: String,
    /// Supporting URLs or labels the tester can go and look at.
    pub evidence: Vec<String>,
    /// The number the finding turns on, for sorting.
    pub magnitude: i64,
}

#[derive(Debug, Clone)]
pub struct IaReport {
    pub pages: i64,
    pub max_depth: i64,
    pub median_links: i64,
    pub findings: Vec<IaFinding>,
}

/// Information-architecture findings for the open crawl.
pub fn architecture_report(
    db_path: String,
    deep_threshold: u32,
    crowded_threshold: u32,
) -> Result<IaReport, String> {
    use crawlstat_engine::ux::architecture as ia;

    runtime::with_store(&db_path, |store| {
        let report = ia::analyse_with(
            store.connection(),
            ia::ArchitectureConfig {
                deep_threshold,
                crowded_threshold,
                ..Default::default()
            },
        )?;

        let mut findings = Vec::new();

        for item in &report.ambiguous_labels {
            findings.push(IaFinding {
                kind: "ambiguous_label".into(),
                summary: format!(
                    "\"{}\" leads to {} different pages",
                    item.label, item.destinations
                ),
                evidence: item.examples.clone(),
                magnitude: item.destinations as i64,
            });
        }
        for item in &report.inconsistent_labels {
            findings.push(IaFinding {
                kind: "inconsistent_label".into(),
                summary: format!(
                    "One page is called {} different things",
                    item.labels.len()
                ),
                evidence: std::iter::once(item.url.clone())
                    .chain(item.labels.iter().map(|l| format!("called \"{l}\"")))
                    .collect(),
                magnitude: item.labels.len() as i64,
            });
        }
        if !report.orphans.is_empty() {
            findings.push(IaFinding {
                kind: "orphan".into(),
                summary: format!(
                    "{} page(s) have no internal links pointing at them",
                    report.orphans.len()
                ),
                evidence: report.orphans.iter().map(|o| o.url.clone()).take(20).collect(),
                magnitude: report.orphans.len() as i64,
            });
        }
        if !report.too_deep.is_empty() {
            findings.push(IaFinding {
                kind: "too_deep".into(),
                summary: format!(
                    "{} page(s) sit {}+ clicks from the home page",
                    report.too_deep.len(),
                    deep_threshold
                ),
                evidence: report
                    .too_deep
                    .iter()
                    .map(|d| format!("{} clicks — {}", d.depth, d.url))
                    .take(20)
                    .collect(),
                magnitude: report.too_deep.len() as i64,
            });
        }
        if !report.crowded.is_empty() {
            findings.push(IaFinding {
                kind: "crowded".into(),
                summary: format!(
                    "{} page(s) offer {}+ links at once",
                    report.crowded.len(),
                    crowded_threshold
                ),
                evidence: report
                    .crowded
                    .iter()
                    .map(|c| format!("{} links — {}", c.links, c.url))
                    .take(20)
                    .collect(),
                magnitude: report.crowded.len() as i64,
            });
        }

        findings.sort_by(|a, b| b.magnitude.cmp(&a.magnitude));

        Ok(IaReport {
            pages: report.pages as i64,
            max_depth: report.max_depth as i64,
            median_links: report.median_links as i64,
            findings,
        })
    })
    .map_err(|error| error.to_string())
}

/// Write the findings out as Markdown, which is what a tester hands over.
pub fn export_ux_report(
    db_path: String,
    out_path: String,
    deep_threshold: u32,
    crowded_threshold: u32,
) -> Result<String, String> {
    use std::fmt::Write as _;

    let report = architecture_report(db_path, deep_threshold, crowded_threshold)?;
    let mut out = String::from("# Information architecture findings\n\n");
    let _ = writeln!(
        out,
        "{} pages · deepest {} clicks from home · {} links on a typical page\n",
        report.pages, report.max_depth, report.median_links
    );

    if report.findings.is_empty() {
        out.push_str("No findings.\n");
    }
    for finding in &report.findings {
        let _ = writeln!(out, "## {}\n", finding.summary);
        for line in &finding.evidence {
            let _ = writeln!(out, "- {line}");
        }
        out.push('\n');
    }
    out.push_str(
        "\n---\n\nFindings are evidence for a hypothesis, not verdicts. \
         A page offering a hundred links may be a deliberate directory.\n",
    );

    std::fs::write(&out_path, out).map_err(|error| error.to_string())?;
    Ok(out_path)
}

/// The same task run against your crawl and someone else's.
#[derive(Debug, Clone)]
pub struct TaskBenchmark {
    pub goal_label: String,
    /// `reached`, `gave_up`, `dead_end`, `looped` or `no_signal`.
    pub our_outcome: String,
    pub their_outcome: String,
    pub our_clicks: i64,
    pub their_clicks: i64,
    pub our_exists: bool,
    pub their_exists: bool,
    /// One line a stakeholder can read: "they have it, we do not".
    pub verdict: String,
    /// Their clicks minus ours, only when both sites arrive.
    pub click_gap: Option<i64>,
}

fn to_benchmark(b: ux::journey::Benchmark) -> TaskBenchmark {
    TaskBenchmark {
        verdict: b.verdict().to_string(),
        click_gap: b.click_gap(),
        goal_label: b.goal_label,
        our_outcome: b.ours.outcome,
        their_outcome: b.theirs.outcome,
        our_clicks: b.ours.clicks as i64,
        their_clicks: b.theirs.clicks as i64,
        our_exists: b.ours.target_exists,
        their_exists: b.theirs.target_exists,
    }
}

/// Run every preset task against two crawls — yours and a competitor's.
pub fn benchmark_against(
    our_path: String,
    their_path: String,
    persona: String,
) -> Result<Vec<TaskBenchmark>, String> {
    let persona =
        ux::Persona::parse(&persona).ok_or_else(|| format!("unknown persona '{persona}'"))?;
    let ours = Store::open(std::path::Path::new(&our_path)).map_err(|e| e.to_string())?;
    let theirs = Store::open(std::path::Path::new(&their_path)).map_err(|e| e.to_string())?;
    Ok(
        ux::journey::benchmark(ours.connection(), theirs.connection(), persona)
            .map_err(|error| error.to_string())?
            .into_iter()
            .map(to_benchmark)
            .collect(),
    )
}

/// Run one task of your own against both.
pub fn benchmark_task_against(
    our_path: String,
    their_path: String,
    persona: String,
    task: String,
) -> Result<TaskBenchmark, String> {
    let persona =
        ux::Persona::parse(&persona).ok_or_else(|| format!("unknown persona '{persona}'"))?;
    if ux::journey::custom_goal(&task).keywords.is_empty() {
        return Err("Describe the task in a few words.".into());
    }
    let ours = Store::open(std::path::Path::new(&our_path)).map_err(|e| e.to_string())?;
    let theirs = Store::open(std::path::Path::new(&their_path)).map_err(|e| e.to_string())?;
    Ok(to_benchmark(
        ux::journey::benchmark_task(ours.connection(), theirs.connection(), persona, &task)
            .map_err(|error| error.to_string())?,
    ))
}

/// The browser CrawlStat would use to render pages and print reports, or
/// `None` when there is none.
#[frb(sync)]
pub fn chrome_path() -> Option<String> {
    crawlstat_engine::chrome::Chrome::locate().map(|chrome| chrome.path().display().to_string())
}

/// Write the client report for a crawl to `out_path`, as a self-contained
/// HTML page or, with `pdf`, printed to PDF by Chrome.
///
/// `previous_path` adds a "since the last crawl" section. That is the crawl
/// comparison, so it is left out without Pro rather than failing the export.
#[allow(clippy::too_many_arguments)]
pub fn export_report(
    db_path: String,
    previous_path: Option<String>,
    out_path: String,
    prepared_by: Option<String>,
    accent: Option<String>,
    standard: String,
    narrative: Option<String>,
    pdf: bool,
) -> Result<String, String> {
    use crawlstat_engine::a11y::Standard;
    use crawlstat_engine::chrome::Chrome;
    use crawlstat_engine::license;
    use crawlstat_engine::report::{self, ReportOptions};

    let compare = license::current()
        .entitlement()
        .has(license::features::CRAWL_COMPARISON);
    let previous = match previous_path.filter(|_| compare) {
        Some(path) => Some(Store::open(Path::new(&path)).map_err(|e| e.to_string())?),
        None => None,
    };
    let options = ReportOptions {
        prepared_by: prepared_by.filter(|p| !p.trim().is_empty()),
        accent,
        standard: Standard::parse(&standard).unwrap_or_default(),
        narrative,
        ..ReportOptions::default()
    };
    let html = runtime::with_store(&db_path, |store| {
        let report = report::build(store, previous.as_ref(), options)?;
        Ok(report::html(&report))
    })
    .map_err(|error| error.to_string())?;

    let out = Path::new(&out_path);
    if !pdf {
        std::fs::write(out, html).map_err(|error| error.to_string())?;
        return Ok(out_path);
    }
    let chrome = Chrome::locate().ok_or(
        "Saving a PDF needs Chrome, Chromium, Edge or Brave installed. Save it as a web page \
         instead, and print it from any browser.",
    )?;
    // Chrome prints a file, so the page goes to disk first, in the app's own
    // temporary folder: the sandboxed browser can read there, and not always
    // in Downloads.
    let page = std::env::temp_dir().join(format!(
        "crawlstat-report-{}-{}.html",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |since| since.as_millis())
    ));
    std::fs::write(&page, html).map_err(|error| error.to_string())?;
    let printed = chrome.print_to_pdf(&page, out, std::time::Duration::from_secs(90));
    let _ = std::fs::remove_file(&page);
    printed.map(|()| out_path)
}
