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
