//! The data types crossing the FFI boundary.
//!
//! These mirror the engine's own types rather than re-exporting them, for two
//! reasons: the generator needs plain structs with no generics or lifetimes,
//! and the UI wants a narrower, flatter shape than the engine's internals. The
//! conversions are all in this file, so there is one place to look when a field
//! is missing from a screen.
//!
//! Integers are `i64` throughout. Dart's `int` is a signed 64-bit value, so
//! unsigned engine counters are widened here rather than risking a surprise at
//! the boundary.

use crawlstat_engine::audit;
use crawlstat_engine::config::{CrawlConfig, CrawlMode, ResourcePolicy, Seed};
use crawlstat_engine::model;
use crawlstat_engine::progress;
use crawlstat_engine::store::{queries, CrawlStats};

/// What the new-crawl dialog collects.
///
/// A flat subset of [`CrawlConfig`] — the options the UI actually offers.
/// Everything else keeps its engine default, and a saved preset round-trips
/// through `config_json` so power users and the CLI are not limited by this
/// shape.
#[derive(Debug, Clone)]
pub struct CrawlRequest {
    /// The start URL, the sitemap URL, or ignored in list mode.
    pub start_url: String,
    /// `"spider"`, `"sitemap"` or `"list"`.
    pub mode: String,
    /// The URLs to fetch in list mode.
    pub list_urls: Vec<String>,

    pub max_urls: Option<i64>,
    pub max_depth: Option<i64>,
    pub concurrency: i64,
    pub crawl_delay_ms: i64,
    pub request_timeout_ms: i64,

    pub respect_robots: bool,
    pub user_agent: String,
    pub crawl_subdomains: bool,
    pub check_external_links: bool,
    pub follow_nofollow: bool,

    pub include_patterns: Vec<String>,
    pub exclude_patterns: Vec<String>,

    pub crawl_images: bool,
    pub crawl_css: bool,
    pub crawl_javascript: bool,
    pub crawl_documents: bool,

    pub store_html: bool,
    /// Run each page's JavaScript in headless Chrome before reading it.
    pub render_javascript: bool,
    pub run_audits: bool,

    /// A full [`CrawlConfig`] as JSON. When present every other field is
    /// ignored, which is how a saved preset or a config from the CLI is used
    /// verbatim.
    pub config_json: Option<String>,
}

impl CrawlRequest {
    /// Sensible defaults, matching the engine's.
    ///
    /// `pub(crate)` so the generator does not try to expose it: Dart gets the
    /// same defaults through `api::default_crawl_request`.
    pub(crate) fn new(start_url: String) -> Self {
        let defaults = CrawlConfig::default();
        Self {
            start_url,
            mode: "spider".into(),
            list_urls: Vec::new(),
            max_urls: None,
            max_depth: None,
            concurrency: defaults.concurrency as i64,
            crawl_delay_ms: defaults.crawl_delay_ms as i64,
            request_timeout_ms: defaults.request_timeout_ms as i64,
            respect_robots: defaults.respect_robots,
            user_agent: defaults.user_agent,
            crawl_subdomains: defaults.crawl_subdomains,
            check_external_links: defaults.check_external_links,
            follow_nofollow: defaults.follow_nofollow,
            include_patterns: Vec::new(),
            exclude_patterns: Vec::new(),
            crawl_images: defaults.resources.images,
            crawl_css: defaults.resources.css,
            crawl_javascript: defaults.resources.javascript,
            crawl_documents: defaults.resources.documents,
            store_html: defaults.store_html,
            render_javascript: defaults.render_javascript,
            run_audits: defaults.run_audits,
            config_json: None,
        }
    }

    /// The request that reproduces `config` exactly.
    ///
    /// The flat fields are filled in so a dialog can show them, and the full
    /// configuration rides along in `config_json`, so re-running it keeps the
    /// options the flat shape cannot hold (audit thresholds, stored headers,
    /// retries…). A crawl run again with different settings would not be
    /// comparable with the first, which is the reason to run it again.
    pub(crate) fn from_config(config: &CrawlConfig) -> anyhow::Result<Self> {
        let (mode, start_url, list_urls) = match config.seeds.first() {
            Some(Seed::Sitemap { url }) => ("sitemap", url.clone(), Vec::new()),
            Some(Seed::List { urls }) => (
                "list",
                urls.first().cloned().unwrap_or_default(),
                urls.clone(),
            ),
            Some(Seed::Url { url }) => ("spider", url.clone(), Vec::new()),
            None => ("spider", String::new(), Vec::new()),
        };
        Ok(Self {
            start_url,
            mode: if config.mode == CrawlMode::List {
                "list".into()
            } else {
                mode.into()
            },
            list_urls,
            max_urls: config.max_urls.map(|value| value as i64),
            max_depth: config.max_depth.map(i64::from),
            concurrency: config.concurrency as i64,
            crawl_delay_ms: config.crawl_delay_ms as i64,
            request_timeout_ms: config.request_timeout_ms as i64,
            respect_robots: config.respect_robots,
            user_agent: config.user_agent.clone(),
            crawl_subdomains: config.crawl_subdomains,
            check_external_links: config.check_external_links,
            follow_nofollow: config.follow_nofollow,
            include_patterns: config.include_patterns.clone(),
            exclude_patterns: config.exclude_patterns.clone(),
            crawl_images: config.resources.images,
            crawl_css: config.resources.css,
            crawl_javascript: config.resources.javascript,
            crawl_documents: config.resources.documents,
            store_html: config.store_html,
            render_javascript: config.render_javascript,
            run_audits: config.run_audits,
            config_json: Some(serde_json::to_string(config)?),
        })
    }

    /// Build the engine's configuration.
    ///
    /// Not exposed to Dart: it returns engine types, which the generator would
    /// otherwise try to carry across the boundary as opaque handles.
    pub(crate) fn to_config(&self) -> anyhow::Result<CrawlConfig> {
        if let Some(json) = &self.config_json {
            return Ok(serde_json::from_str(json)?);
        }

        let mut config = CrawlConfig::default();
        config.seeds = match self.mode.as_str() {
            "sitemap" => vec![Seed::Sitemap {
                url: self.start_url.clone(),
            }],
            "list" => {
                config.mode = CrawlMode::List;
                vec![Seed::List {
                    urls: self.list_urls.clone(),
                }]
            }
            _ => vec![Seed::Url {
                url: self.start_url.clone(),
            }],
        };

        config.max_urls = self.max_urls.map(|value| value.max(0) as u64);
        config.max_depth = self.max_depth.map(|value| value.max(0) as u32);
        config.concurrency = self.concurrency.clamp(1, 200) as usize;
        config.concurrency_per_host = config.concurrency;
        config.crawl_delay_ms = self.crawl_delay_ms.max(0) as u64;
        config.request_timeout_ms = self.request_timeout_ms.clamp(1_000, 600_000) as u64;

        config.respect_robots = self.respect_robots;
        if !self.user_agent.trim().is_empty() {
            config.user_agent = self.user_agent.clone();
            // Let the robots.txt token be re-derived from the new agent.
            config.robots_token = String::new();
        }
        config.crawl_subdomains = self.crawl_subdomains;
        config.check_external_links = self.check_external_links;
        config.follow_nofollow = self.follow_nofollow;
        config.include_patterns = self.include_patterns.clone();
        config.exclude_patterns = self.exclude_patterns.clone();
        config.resources = ResourcePolicy {
            images: self.crawl_images,
            css: self.crawl_css,
            javascript: self.crawl_javascript,
            documents: self.crawl_documents,
        };
        config.store_html = self.store_html;
        config.render_javascript = self.render_javascript;
        config.run_audits = self.run_audits;

        // Fail here rather than three frames later, so the dialog can show the
        // problem next to the field that caused it.
        config.compile()?;
        Ok(config)
    }
}

/// What the engine reports while crawling.
///
/// The generator turns this into a Dart sealed class, so the UI can switch on
/// it exhaustively.
#[derive(Debug, Clone)]
pub enum CrawlEvent {
    Started {
        seeded: i64,
        resumed: bool,
    },
    Url(CrawledUrlEvent),
    Progress(ProgressSnapshot),
    Warning {
        message: String,
        url: Option<String>,
    },
    StateChanged {
        state: String,
    },
    AuditStarted,
    Audited {
        errors: i64,
        warnings: i64,
        notices: i64,
        findings: i64,
    },
    Finished {
        summary: CrawlSummary,
    },
    /// The crawl stopped because of an error rather than finishing.
    Failed {
        message: String,
    },
}

#[derive(Debug, Clone)]
pub struct CrawledUrlEvent {
    pub url_id: i64,
    pub url: String,
    pub status_code: Option<i64>,
    pub content_type: Option<String>,
    pub depth: i64,
    pub response_time_ms: Option<i64>,
    pub title: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ProgressSnapshot {
    pub state: String,
    pub crawled: i64,
    pub queued: i64,
    pub in_flight: i64,
    pub errors: i64,
    pub status4xx: i64,
    pub status5xx: i64,
    pub discovered: i64,
    pub urls_per_second: f64,
    pub elapsed_ms: i64,
}

#[derive(Debug, Clone)]
pub struct CrawlSummary {
    pub state: String,
    pub crawled: i64,
    pub discovered: i64,
    pub errors: i64,
    pub status2xx: i64,
    pub status3xx: i64,
    pub status4xx: i64,
    pub status5xx: i64,
    pub elapsed_ms: i64,
    pub urls_per_second: f64,
    pub hit_limit: bool,
}

impl From<progress::ProgressEvent> for CrawlEvent {
    fn from(event: progress::ProgressEvent) -> Self {
        use progress::ProgressEvent as E;
        match event {
            E::Started { seeded, resumed } => CrawlEvent::Started {
                seeded: seeded as i64,
                resumed,
            },
            E::Url(url) => CrawlEvent::Url(CrawledUrlEvent {
                url_id: url.url_id,
                url: url.url,
                status_code: url.status_code.map(i64::from),
                content_type: url.content_type,
                depth: url.depth as i64,
                response_time_ms: url.response_time_ms.map(|ms| ms as i64),
                title: url.title,
                error: url.error,
            }),
            E::Progress(snapshot) => CrawlEvent::Progress(ProgressSnapshot {
                state: snapshot.state.as_str().to_string(),
                crawled: snapshot.crawled as i64,
                queued: snapshot.queued as i64,
                in_flight: snapshot.in_flight as i64,
                errors: snapshot.errors as i64,
                status4xx: snapshot.status_4xx as i64,
                status5xx: snapshot.status_5xx as i64,
                discovered: snapshot.discovered as i64,
                urls_per_second: snapshot.urls_per_second,
                elapsed_ms: snapshot.elapsed_ms as i64,
            }),
            E::Warning { message, url } => CrawlEvent::Warning { message, url },
            E::StateChanged { state } => CrawlEvent::StateChanged {
                state: state.as_str().to_string(),
            },
            E::AuditStarted => CrawlEvent::AuditStarted,
            E::Audited { summary } => CrawlEvent::Audited {
                errors: summary.errors as i64,
                warnings: summary.warnings as i64,
                notices: summary.notices as i64,
                findings: summary.findings as i64,
            },
            E::Finished { summary } => CrawlEvent::Finished {
                summary: summary.into(),
            },
        }
    }
}

impl From<progress::CrawlSummary> for CrawlSummary {
    fn from(summary: progress::CrawlSummary) -> Self {
        Self {
            state: summary.state.as_str().to_string(),
            crawled: summary.crawled as i64,
            discovered: summary.discovered as i64,
            errors: summary.errors as i64,
            status2xx: summary.status_2xx as i64,
            status3xx: summary.status_3xx as i64,
            status4xx: summary.status_4xx as i64,
            status5xx: summary.status_5xx as i64,
            elapsed_ms: summary.elapsed_ms as i64,
            urls_per_second: summary.urls_per_second,
            hit_limit: summary.hit_limit,
        }
    }
}

/// One row of the results table.
#[derive(Debug, Clone)]
pub struct UrlRow {
    pub id: i64,
    pub url: String,
    pub state: String,
    pub kind: String,
    pub is_internal: bool,
    pub indexable: Option<bool>,
    pub indexability_reason: Option<String>,
    pub depth: i64,
    pub status_code: Option<i64>,
    pub final_status_code: Option<i64>,
    pub content_type: Option<String>,
    pub size_bytes: Option<i64>,
    pub response_time_ms: Option<i64>,
    pub title: Option<String>,
    pub title_length: Option<i64>,
    pub meta_description: Option<String>,
    pub meta_description_length: Option<i64>,
    pub meta_robots: Option<String>,
    pub html_lang: Option<String>,
    /// Schema.org types declared on the page, comma-separated.
    pub structured_data_types: Option<String>,
    pub h1: Option<String>,
    pub h1_count: Option<i64>,
    pub word_count: Option<i64>,
    pub canonical: Option<String>,
    pub canonical_is_self: Option<bool>,
    pub inlinks: i64,
    pub internal_links: Option<i64>,
    pub external_links: Option<i64>,
    pub redirect_count: i64,
    pub link_score: Option<f64>,
    pub error_kind: Option<String>,
    /// How many audit issues flag this URL.
    pub issue_count: i64,
    /// `"error"`, `"warning"` or `"notice"`: the worst of them.
    pub worst_severity: Option<String>,
}

impl From<queries::UrlRow> for UrlRow {
    fn from(row: queries::UrlRow) -> Self {
        Self {
            id: row.id,
            url: row.url,
            state: state_name(row.state),
            kind: kind_name(row.kind),
            is_internal: row.is_internal,
            indexable: row.indexable,
            indexability_reason: row.indexability_reason,
            depth: row.depth as i64,
            status_code: row.status_code.map(i64::from),
            final_status_code: row.final_status_code.map(i64::from),
            content_type: row.content_type,
            size_bytes: row.body_bytes.map(|bytes| bytes as i64),
            response_time_ms: row.response_time_ms,
            title: row.title,
            title_length: row.title_length,
            meta_description: row.meta_description,
            meta_description_length: row.meta_description_length,
            meta_robots: row.meta_robots,
            html_lang: row.html_lang,
            structured_data_types: row.structured_data_types,
            h1: row.h1,
            h1_count: row.h1_count,
            word_count: row.word_count,
            canonical: row.canonical,
            canonical_is_self: row.canonical_is_self,
            inlinks: row.inlinks,
            internal_links: row.internal_links,
            external_links: row.external_links,
            redirect_count: row.redirect_count,
            link_score: row.link_score,
            error_kind: row.error_kind,
            issue_count: row.issue_count,
            worst_severity: row.worst_severity.map(|severity| {
                match severity {
                    0 => "error",
                    1 => "warning",
                    _ => "notice",
                }
                .to_string()
            }),
        }
    }
}

/// One page of results, plus the total that matched the filter.
#[derive(Debug, Clone)]
pub struct UrlPage {
    pub rows: Vec<UrlRow>,
    pub total: i64,
    pub offset: i64,
}

/// Which rows a table view wants.
///
/// `tab` selects one of the Results tabs; the rest are the filter bar.
#[derive(Debug, Clone, Default)]
pub struct UrlQuery {
    /// `"internal"`, `"external"`, `"html"`, `"images"`, `"css"`,
    /// `"javascript"`, `"documents"`, `"indexable"`, `"non_indexable"` or
    /// `"all"`.
    pub tab: String,
    pub status_code: Option<i64>,
    pub status_class: Option<i64>,
    pub url_contains: Option<String>,
    pub issue_id: Option<String>,
    pub max_depth: Option<i64>,
    /// One of the Pages tab's lenses, by [`PageLens::id`]; `None` for all.
    pub view: Option<String>,
    /// A column name from `SortBy`; unknown values fall back to row order.
    pub sort_by: String,
    pub descending: bool,
    pub limit: i64,
    pub offset: i64,
}

impl UrlQuery {
    /// Split into the engine's filter and sort. Internal, for the same reason
    /// as [`CrawlRequest::to_config`].
    pub(crate) fn to_filter(&self) -> (queries::UrlFilter, queries::SortBy, bool, u32, u64) {
        let mut filter = queries::UrlFilter {
            status_code: self.status_code.map(|code| code as u16),
            status_class: self.status_class.map(|class| class as u8),
            url_contains: self.url_contains.clone(),
            issue_id: self.issue_id.clone(),
            max_depth: self.max_depth.map(|depth| depth as u32),
            ..Default::default()
        };

        match self.tab.as_str() {
            "internal" => filter.internal = Some(true),
            "external" => filter.internal = Some(false),
            "html" => {
                filter.kind = Some(model::ResourceKind::Html);
                filter.internal = Some(true);
            }
            "images" => filter.kind = Some(model::ResourceKind::Image),
            "css" => filter.kind = Some(model::ResourceKind::Css),
            "javascript" => filter.kind = Some(model::ResourceKind::JavaScript),
            "documents" => filter.kind = Some(model::ResourceKind::Document),
            "indexable" => filter.indexable = Some(true),
            "non_indexable" => filter.indexable = Some(false),
            _ => {}
        }

        let sort = match self.sort_by.as_str() {
            "url" => queries::SortBy::Url,
            "status_code" => queries::SortBy::StatusCode,
            "title" => queries::SortBy::Title,
            "title_length" => queries::SortBy::TitleLength,
            "meta_description_length" => queries::SortBy::MetaDescriptionLength,
            "word_count" => queries::SortBy::WordCount,
            "depth" => queries::SortBy::Depth,
            "response_time" => queries::SortBy::ResponseTime,
            "inlinks" => queries::SortBy::Inlinks,
            "link_score" => queries::SortBy::LinkScore,
            "size" => queries::SortBy::Size,
            "issues" => queries::SortBy::Issues,
            _ => queries::SortBy::Id,
        };

        (
            filter,
            sort,
            self.descending,
            self.limit.clamp(1, 5_000) as u32,
            self.offset.max(0) as u64,
        )
    }
}

/// Fewer words than this and a page is thin. There is no audit for it, so
/// this is the usual rule of thumb rather than a crawl setting.
pub(crate) const THIN_WORDS: i64 = 200;

/// The lenses, in the order the Pages tab shows them.
pub(crate) const LENSES: [(&str, &str); 8] = [
    ("issues", "Has issues"),
    ("broken", "Broken"),
    ("redirects", "Redirects"),
    ("slow", "Slow"),
    ("titles", "Title problems"),
    ("descriptions", "Description problems"),
    ("thin", "Thin content"),
    ("non_indexable", "Not indexable"),
];

/// A lens by id, with the crawl's own thresholds.
pub(crate) fn page_view(id: &str, config: &audit::AuditConfig) -> Option<queries::PageView> {
    Some(match id {
        "issues" => queries::PageView::WithIssues,
        "broken" => queries::PageView::Broken,
        "redirects" => queries::PageView::Redirects,
        "slow" => queries::PageView::Slow {
            ms: config.slow_response_ms as i64,
        },
        "titles" => queries::PageView::TitleProblems {
            min: config.title_min_chars as i64,
            max: config.title_max_chars as i64,
        },
        "descriptions" => queries::PageView::DescriptionProblems {
            min: config.description_min_chars as i64,
            max: config.description_max_chars as i64,
        },
        "thin" => queries::PageView::Thin { words: THIN_WORDS },
        "non_indexable" => queries::PageView::NonIndexable,
        _ => return None,
    })
}

/// One lens on the Pages tab and how many rows it would show.
#[derive(Debug, Clone)]
pub struct PageLens {
    pub id: String,
    pub label: String,
    pub count: i64,
}

/// The lenses for a table view, with the limits they measure against — the
/// grid draws title lengths and response times against the same ones.
#[derive(Debug, Clone)]
pub struct PageLenses {
    pub lenses: Vec<PageLens>,
    /// Rows in the view with no lens applied.
    pub total: i64,
    pub title_min: i64,
    pub title_max: i64,
    pub description_min: i64,
    pub description_max: i64,
    pub slow_ms: i64,
    pub thin_words: i64,
}

/// Everything the URL detail panel shows.
#[derive(Debug, Clone)]
pub struct UrlDetail {
    pub row: UrlRow,
    pub headers: Vec<HeaderPair>,
    pub inlinks: Vec<LinkRow>,
    pub outlinks: Vec<LinkRow>,
    pub redirect_chain: Vec<RedirectHop>,
    pub images: Vec<ImageRow>,
    /// `(issue id, per-URL detail)`.
    pub issues: Vec<UrlIssue>,
}

#[derive(Debug, Clone)]
pub struct HeaderPair {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone)]
pub struct LinkRow {
    pub source_url: String,
    pub target_url: Option<String>,
    pub anchor_text: String,
    pub rel: Option<String>,
    pub position: String,
    pub link_type: String,
    pub nofollow: bool,
    pub status_code: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct RedirectHop {
    pub hop: i64,
    pub from_url: String,
    pub to_url: String,
    pub status: i64,
}

#[derive(Debug, Clone)]
pub struct ImageRow {
    pub src: Option<String>,
    pub raw_src: String,
    pub alt: Option<String>,
    pub alt_missing: bool,
    pub declared_width: Option<i64>,
    pub declared_height: Option<i64>,
    pub size_bytes: Option<i64>,
    pub status_code: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct UrlIssue {
    pub issue_id: String,
    pub title: String,
    pub severity: String,
    pub detail: Option<String>,
}

impl From<queries::LinkRow> for LinkRow {
    fn from(row: queries::LinkRow) -> Self {
        Self {
            source_url: row.source_url,
            target_url: row.target_url,
            anchor_text: row.anchor_text,
            rel: row.rel,
            position: position_name(row.position),
            link_type: link_source_name(row.source),
            nofollow: row.nofollow,
            status_code: row.status_code.map(i64::from),
        }
    }
}

impl From<queries::ImageRow> for ImageRow {
    fn from(row: queries::ImageRow) -> Self {
        Self {
            src: row.src,
            raw_src: row.raw_src,
            alt: row.alt,
            alt_missing: row.alt_missing,
            declared_width: row.declared_width.map(i64::from),
            declared_height: row.declared_height.map(i64::from),
            size_bytes: row.body_bytes.map(|bytes| bytes as i64),
            status_code: row.status_code.map(i64::from),
        }
    }
}

/// One issue type, for the Issues panel.
#[derive(Debug, Clone)]
pub struct Issue {
    pub id: String,
    pub title: String,
    /// `"error"`, `"warning"` or `"notice"`.
    pub severity: String,
    pub description: String,
    pub how_to_fix: String,
    /// Page under `crawlstat.com/docs/audits/`.
    pub docs_slug: String,
    pub url_count: i64,
}

impl From<audit::IssueSummary> for Issue {
    fn from(issue: audit::IssueSummary) -> Self {
        Self {
            id: issue.id,
            title: issue.title,
            severity: issue.severity.as_str().to_string(),
            description: issue.description,
            how_to_fix: issue.how_to_fix,
            docs_slug: issue.docs_slug,
            url_count: issue.url_count as i64,
        }
    }
}

/// The Issues screen's whole payload.
#[derive(Debug, Clone)]
pub struct AuditReport {
    pub issues: Vec<Issue>,
    pub errors: i64,
    pub warnings: i64,
    pub notices: i64,
    pub depth_distribution: Vec<DepthBucket>,
    pub indexable_pages: i64,
    pub non_indexable_pages: i64,
    pub elapsed_ms: i64,
}

#[derive(Debug, Clone)]
pub struct DepthBucket {
    pub depth: i64,
    pub pages: i64,
}

impl From<audit::AuditReport> for AuditReport {
    fn from(report: audit::AuditReport) -> Self {
        Self {
            issues: report.issues.into_iter().map(Issue::from).collect(),
            errors: report.errors as i64,
            warnings: report.warnings as i64,
            notices: report.notices as i64,
            depth_distribution: report
                .depth_distribution
                .into_iter()
                .map(|(depth, pages)| DepthBucket {
                    depth: depth as i64,
                    pages: pages as i64,
                })
                .collect(),
            indexable_pages: report.indexable_pages as i64,
            non_indexable_pages: report.non_indexable_pages as i64,
            elapsed_ms: report.elapsed_ms as i64,
        }
    }
}

/// Live counters for the crawl screen and the project list.
#[derive(Debug, Clone)]
pub struct ProjectStats {
    pub total: i64,
    pub queued: i64,
    pub crawled: i64,
    pub failed: i64,
    pub skipped: i64,
    pub blocked_by_robots: i64,
    pub status2xx: i64,
    pub status3xx: i64,
    pub status4xx: i64,
    pub status5xx: i64,
}

impl From<CrawlStats> for ProjectStats {
    fn from(stats: CrawlStats) -> Self {
        Self {
            total: stats.total as i64,
            queued: stats.queued as i64,
            crawled: stats.done as i64,
            failed: stats.failed as i64,
            skipped: stats.skipped as i64,
            blocked_by_robots: stats.blocked_by_robots as i64,
            status2xx: stats.status_2xx as i64,
            status3xx: stats.status_3xx as i64,
            status4xx: stats.status_4xx as i64,
            status5xx: stats.status_5xx as i64,
        }
    }
}

/// A crawl file the projects screen can list.
#[derive(Debug, Clone)]
pub struct ProjectInfo {
    pub path: String,
    pub start_url: Option<String>,
    pub root_host: Option<String>,
    /// `running`, `paused`, `finished`, `cancelled` or `failed`.
    pub state: Option<String>,
    pub created_at: Option<String>,
    pub finished_at: Option<String>,
    pub stats: ProjectStats,
    pub errors: i64,
    pub warnings: i64,
    pub size_bytes: i64,
}

// ─── enum names ──────────────────────────────────────────────────────────
//
// Sent as strings rather than mirrored enums: the UI only ever shows or
// switches on them, and a string keeps old crawl files readable by newer
// builds without a migration.

fn state_name(state: Option<model::UrlState>) -> String {
    match state {
        Some(model::UrlState::Queued) => "queued",
        Some(model::UrlState::InProgress) => "in_progress",
        Some(model::UrlState::Done) => "done",
        Some(model::UrlState::Failed) => "failed",
        Some(model::UrlState::Skipped) => "skipped",
        Some(model::UrlState::BlockedByRobots) => "blocked_by_robots",
        None => "unknown",
    }
    .to_string()
}

fn kind_name(kind: model::ResourceKind) -> String {
    match kind {
        model::ResourceKind::Html => "html",
        model::ResourceKind::Image => "image",
        model::ResourceKind::Css => "css",
        model::ResourceKind::JavaScript => "javascript",
        model::ResourceKind::Document => "document",
        model::ResourceKind::Other => "other",
    }
    .to_string()
}

fn position_name(position: model::LinkPosition) -> String {
    match position {
        model::LinkPosition::Content => "content",
        model::LinkPosition::Navigation => "navigation",
        model::LinkPosition::Header => "header",
        model::LinkPosition::Footer => "footer",
        model::LinkPosition::Aside => "aside",
        model::LinkPosition::Unknown => "unknown",
    }
    .to_string()
}

fn link_source_name(source: model::LinkSource) -> String {
    match source {
        model::LinkSource::Anchor => "hyperlink",
        model::LinkSource::Image => "image",
        model::LinkSource::Stylesheet => "stylesheet",
        model::LinkSource::Script => "script",
        model::LinkSource::Canonical => "canonical",
        model::LinkSource::Hreflang => "hreflang",
        model::LinkSource::Redirect => "redirect",
        model::LinkSource::Sitemap => "sitemap",
        model::LinkSource::Form => "form",
        model::LinkSource::Iframe => "iframe",
        model::LinkSource::MetaRefresh => "meta refresh",
    }
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(mode: &str) -> CrawlRequest {
        CrawlRequest {
            mode: mode.to_string(),
            ..CrawlRequest::new("https://a.test/".into())
        }
    }

    #[test]
    fn defaults_come_from_the_engine_not_from_a_second_copy() {
        let request = CrawlRequest::new("https://a.test/".into());
        let engine = CrawlConfig::default();

        assert_eq!(request.concurrency, engine.concurrency as i64);
        assert_eq!(request.respect_robots, engine.respect_robots);
        assert_eq!(request.user_agent, engine.user_agent);
        assert_eq!(request.crawl_images, engine.resources.images);
        assert_eq!(request.run_audits, engine.run_audits);
    }

    #[test]
    fn each_mode_produces_the_right_seed() {
        let config = request("spider").to_config().unwrap();
        assert!(matches!(config.seeds.first(), Some(Seed::Url { .. })));
        assert_eq!(config.mode, CrawlMode::Spider);

        let config = request("sitemap").to_config().unwrap();
        assert!(matches!(config.seeds.first(), Some(Seed::Sitemap { .. })));

        let mut listed = request("list");
        listed.list_urls = vec!["https://a.test/one".into(), "https://a.test/two".into()];
        let config = listed.to_config().unwrap();
        assert_eq!(config.mode, CrawlMode::List);
        match config.seeds.first() {
            Some(Seed::List { urls }) => assert_eq!(urls.len(), 2),
            other => panic!("expected a list seed, got {other:?}"),
        }
    }

    #[test]
    fn an_unknown_mode_falls_back_to_spidering() {
        // A newer UI sending a mode this build does not know should crawl, not
        // fail.
        let config = request("teleport").to_config().unwrap();
        assert!(matches!(config.seeds.first(), Some(Seed::Url { .. })));
    }

    #[test]
    fn nonsense_numbers_are_clamped_rather_than_rejected() {
        let mut request = request("spider");
        request.concurrency = 0;
        request.request_timeout_ms = 1;
        request.crawl_delay_ms = -50;
        request.max_urls = Some(-10);

        let config = request.to_config().unwrap();
        assert_eq!(
            config.concurrency, 1,
            "zero concurrency would never fetch anything"
        );
        assert_eq!(config.request_timeout_ms, 1_000);
        assert_eq!(config.crawl_delay_ms, 0);
        assert_eq!(config.max_urls, Some(0));
    }

    #[test]
    fn a_bad_regex_is_reported_at_the_boundary() {
        let mut request = request("spider");
        request.exclude_patterns = vec!["(unclosed".into()];

        // Validated here so the dialog can show the error next to the field,
        // rather than three frames later from a background task.
        let error = request.to_config().unwrap_err().to_string();
        assert!(error.contains("unclosed"), "got: {error}");
    }

    #[test]
    fn an_unusable_url_is_reported_at_the_boundary() {
        let mut request = request("spider");
        request.start_url = "mailto:someone@a.test".into();
        assert!(request.to_config().is_err());
    }

    #[test]
    fn a_blank_user_agent_keeps_the_engine_default() {
        let mut request = request("spider");
        request.user_agent = "   ".into();
        let config = request.to_config().unwrap();
        assert_eq!(config.user_agent, CrawlConfig::default().user_agent);
    }

    #[test]
    fn a_custom_user_agent_re_derives_the_robots_token() {
        let mut request = request("spider");
        request.user_agent = "AcmeBot/3.0 (+https://acme.test/bot)".into();
        let compiled = request.to_config().unwrap().compile().unwrap();
        assert_eq!(compiled.robots_token(), "AcmeBot");
    }

    #[test]
    fn a_stored_config_json_wins_over_every_other_field() {
        let mut saved = CrawlConfig::spider("https://saved.test/");
        saved.max_urls = Some(1_234);
        saved.concurrency = 17;

        let mut request = request("spider");
        request.start_url = "https://ignored.test/".into();
        request.concurrency = 2;
        request.config_json = Some(serde_json::to_string(&saved).unwrap());

        // This is how a preset round-trips: the JSON is the whole truth.
        let config = request.to_config().unwrap();
        assert_eq!(config.max_urls, Some(1_234));
        assert_eq!(config.concurrency, 17);
        match config.seeds.first() {
            Some(Seed::Url { url }) => assert_eq!(url, "https://saved.test/"),
            other => panic!("expected the saved seed, got {other:?}"),
        }
    }

    #[test]
    fn a_stored_config_comes_back_as_a_request_that_reproduces_it() {
        let mut saved = CrawlConfig::spider("https://saved.test/");
        saved.max_urls = Some(500);
        saved.exclude_patterns = vec!["/cart".into()];
        // Not one of the flat fields: only the JSON carries it.
        saved.max_retries = 7;

        let request = CrawlRequest::from_config(&saved).unwrap();
        assert_eq!(request.mode, "spider");
        assert_eq!(request.start_url, "https://saved.test/");
        assert_eq!(request.max_urls, Some(500));
        assert_eq!(request.exclude_patterns, vec!["/cart".to_string()]);

        // Running it again gives the same crawl, down to the options the
        // dialog cannot show.
        let again = request.to_config().unwrap();
        assert_eq!(again.max_retries, 7);
        assert_eq!(
            serde_json::to_value(&again).unwrap(),
            serde_json::to_value(&saved).unwrap()
        );
    }

    #[test]
    fn a_stored_list_crawl_comes_back_in_list_mode() {
        let mut listed = request("list");
        listed.list_urls = vec!["https://a.test/one".into(), "https://a.test/two".into()];
        let request = CrawlRequest::from_config(&listed.to_config().unwrap()).unwrap();
        assert_eq!(request.mode, "list");
        assert_eq!(request.list_urls.len(), 2);
    }

    #[test]
    fn malformed_config_json_is_an_error_not_a_silent_default() {
        let mut request = request("spider");
        request.config_json = Some("{ not json".into());
        assert!(request.to_config().is_err());
    }

    #[test]
    fn resource_toggles_reach_the_engine() {
        let mut request = request("spider");
        request.crawl_images = false;
        request.crawl_css = true;
        request.crawl_javascript = true;
        request.crawl_documents = false;

        let config = request.to_config().unwrap();
        assert!(!config.resources.images);
        assert!(config.resources.css);
        assert!(config.resources.javascript);
        assert!(!config.resources.documents);
    }

    #[test]
    fn each_results_tab_maps_onto_a_filter() {
        let filter_for = |tab: &str| {
            UrlQuery {
                tab: tab.to_string(),
                ..Default::default()
            }
            .to_filter()
            .0
        };

        assert_eq!(filter_for("internal").internal, Some(true));
        assert_eq!(filter_for("external").internal, Some(false));
        assert_eq!(filter_for("images").kind, Some(model::ResourceKind::Image));
        assert_eq!(filter_for("css").kind, Some(model::ResourceKind::Css));
        assert_eq!(
            filter_for("documents").kind,
            Some(model::ResourceKind::Document)
        );
        assert_eq!(filter_for("indexable").indexable, Some(true));
        assert_eq!(filter_for("non_indexable").indexable, Some(false));

        // The HTML tab is internal pages only: an external page's title is not
        // the user's problem to fix.
        let html = filter_for("html");
        assert_eq!(html.kind, Some(model::ResourceKind::Html));
        assert_eq!(html.internal, Some(true));

        // Anything unknown shows everything rather than nothing.
        let all = filter_for("whatever");
        assert_eq!(all.internal, None);
        assert_eq!(all.kind, None);
    }

    #[test]
    fn filters_carry_through_to_the_engine() {
        let query = UrlQuery {
            tab: "internal".into(),
            status_code: Some(404),
            status_class: Some(4),
            url_contains: Some("/blog".into()),
            issue_id: Some("title_missing".into()),
            max_depth: Some(3),
            ..Default::default()
        };
        let (filter, _, _, _, _) = query.to_filter();

        assert_eq!(filter.status_code, Some(404));
        assert_eq!(filter.status_class, Some(4));
        assert_eq!(filter.url_contains.as_deref(), Some("/blog"));
        assert_eq!(filter.issue_id.as_deref(), Some("title_missing"));
        assert_eq!(filter.max_depth, Some(3));
    }

    #[test]
    fn every_sortable_column_name_is_understood() {
        let sort_for = |column: &str| {
            UrlQuery {
                sort_by: column.to_string(),
                ..Default::default()
            }
            .to_filter()
            .1
        };

        assert_eq!(sort_for("url"), queries::SortBy::Url);
        assert_eq!(sort_for("status_code"), queries::SortBy::StatusCode);
        assert_eq!(sort_for("title_length"), queries::SortBy::TitleLength);
        assert_eq!(sort_for("word_count"), queries::SortBy::WordCount);
        assert_eq!(sort_for("inlinks"), queries::SortBy::Inlinks);
        assert_eq!(sort_for("link_score"), queries::SortBy::LinkScore);
        assert_eq!(sort_for("size"), queries::SortBy::Size);
        assert_eq!(sort_for("response_time"), queries::SortBy::ResponseTime);
        // An unknown column keeps insertion order instead of failing.
        assert_eq!(sort_for("made_up"), queries::SortBy::Id);
    }

    #[test]
    fn paging_is_clamped_to_something_a_ui_can_render() {
        let (_, _, _, limit, offset) = UrlQuery {
            limit: 0,
            offset: -5,
            ..Default::default()
        }
        .to_filter();
        assert_eq!(limit, 1);
        assert_eq!(offset, 0);

        // A caller asking for a million rows at once would stall the UI thread
        // marshalling them.
        let (_, _, _, limit, _) = UrlQuery {
            limit: 1_000_000,
            ..Default::default()
        }
        .to_filter();
        assert_eq!(limit, 5_000);
    }

    #[test]
    fn progress_events_convert_without_losing_their_meaning() {
        use crawlstat_engine::progress as p;

        let event = CrawlEvent::from(p::ProgressEvent::Url(p::CrawledUrlEvent {
            url_id: 7,
            url: "https://a.test/p".into(),
            status_code: Some(301),
            content_type: Some("text/html".into()),
            depth: 2,
            response_time_ms: Some(120),
            title: Some("Moved".into()),
            error: None,
        }));
        match event {
            CrawlEvent::Url(url) => {
                assert_eq!(url.url_id, 7);
                assert_eq!(url.status_code, Some(301));
                assert_eq!(url.depth, 2);
            }
            other => panic!("expected a Url event, got {other:?}"),
        }

        let event = CrawlEvent::from(p::ProgressEvent::StateChanged {
            state: p::CrawlState::Paused,
        });
        match event {
            CrawlEvent::StateChanged { state } => assert_eq!(state, "paused"),
            other => panic!("expected a state change, got {other:?}"),
        }
    }

    #[test]
    fn a_crawl_summary_crosses_the_boundary_intact() {
        let summary = CrawlSummary::from(crawlstat_engine::progress::CrawlSummary {
            state: crawlstat_engine::progress::CrawlState::Finished,
            crawled: 1_200,
            discovered: 1_500,
            errors: 3,
            status_2xx: 1_100,
            status_3xx: 50,
            status_4xx: 40,
            status_5xx: 10,
            elapsed_ms: 61_000,
            urls_per_second: 19.7,
            hit_limit: true,
            audit: None,
        });

        assert_eq!(summary.state, "finished");
        assert_eq!(summary.crawled, 1_200);
        assert_eq!(summary.status4xx, 40);
        assert!(summary.hit_limit);
    }

    #[test]
    fn enum_names_sent_to_dart_are_stable() {
        // Dart switches on these strings, so a rename here is a UI break.
        assert_eq!(kind_name(model::ResourceKind::Html), "html");
        assert_eq!(kind_name(model::ResourceKind::JavaScript), "javascript");
        assert_eq!(
            state_name(Some(model::UrlState::BlockedByRobots)),
            "blocked_by_robots"
        );
        assert_eq!(state_name(None), "unknown");
        assert_eq!(position_name(model::LinkPosition::Navigation), "navigation");
        assert_eq!(link_source_name(model::LinkSource::Anchor), "hyperlink");
        assert_eq!(
            link_source_name(model::LinkSource::MetaRefresh),
            "meta refresh"
        );
    }
}
