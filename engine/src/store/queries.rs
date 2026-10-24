//! Paginated, sortable, filterable reads.
//!
//! The UI never loads a crawl into memory: every table view is one of these
//! queries with a `LIMIT`/`OFFSET`, and every sort is an indexed `ORDER BY`.
//! The CLI uses the same functions, which keeps them honest.
//!
//! Filter values are always bound as parameters, and sort columns come from a
//! fixed enum, so no caller can inject SQL.

use rusqlite::{params, Connection, OptionalExtension, Row, ToSql};

use super::{link_source_from_i64, position_from_i64};
use crate::error::Result;
use crate::model::{LinkPosition, LinkSource, ResourceKind, UrlState};

/// One row of the main URL table.
#[derive(Debug, Clone, PartialEq)]
pub struct UrlRow {
    pub id: i64,
    pub url: String,
    pub state: Option<UrlState>,
    pub kind: ResourceKind,
    pub is_internal: bool,
    pub depth: u32,
    pub status_code: Option<u16>,
    /// Status at the end of the redirect chain, when the URL redirected.
    pub final_status_code: Option<u16>,
    pub content_type: Option<String>,
    pub body_bytes: Option<u64>,
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
    pub error_kind: Option<String>,
    /// `None` where indexability does not apply: external URLs and non-HTML
    /// resources.
    pub indexable: Option<bool>,
    pub indexability_reason: Option<String>,
    /// Simplified PageRank, rescaled so the strongest page on the site is 100.
    pub link_score: Option<f64>,
}

/// The columns `SELECT`ed for a [`UrlRow`], in order.
const URL_COLUMNS: &str = "
    id, url, state, kind, is_internal, depth, status_code, final_status_code, content_type,
    body_bytes, response_time_ms, title, title_length, meta_description,
    meta_description_length, meta_robots, h1, h1_count, word_count, canonical,
    canonical_is_self, inlinks, internal_links, external_links, redirect_count, error_kind,
    indexable, indexability_reason, link_score, html_lang, structured_data_types";

fn url_row(row: &Row<'_>) -> rusqlite::Result<UrlRow> {
    Ok(UrlRow {
        id: row.get(0)?,
        url: row.get(1)?,
        state: UrlState::from_i64(row.get(2)?),
        kind: super::kind_from_i64(row.get(3)?),
        is_internal: row.get::<_, i64>(4)? != 0,
        depth: row.get::<_, i64>(5)?.max(0) as u32,
        status_code: row.get::<_, Option<i64>>(6)?.map(|s| s as u16),
        final_status_code: row.get::<_, Option<i64>>(7)?.map(|s| s as u16),
        content_type: row.get(8)?,
        body_bytes: row.get::<_, Option<i64>>(9)?.map(|b| b.max(0) as u64),
        response_time_ms: row.get(10)?,
        title: row.get(11)?,
        title_length: row.get(12)?,
        meta_description: row.get(13)?,
        meta_description_length: row.get(14)?,
        meta_robots: row.get(15)?,
        h1: row.get(16)?,
        h1_count: row.get(17)?,
        word_count: row.get(18)?,
        canonical: row.get(19)?,
        canonical_is_self: row.get::<_, Option<i64>>(20)?.map(|v| v != 0),
        inlinks: row.get(21)?,
        internal_links: row.get(22)?,
        external_links: row.get(23)?,
        redirect_count: row.get(24)?,
        error_kind: row.get(25)?,
        indexable: row.get::<_, Option<i64>>(26)?.map(|v| v != 0),
        indexability_reason: row.get(27)?,
        link_score: row.get(28)?,
        html_lang: row.get(29)?,
        structured_data_types: row.get(30)?,
    })
}

/// Which URLs to show.
#[derive(Debug, Clone, Default)]
pub struct UrlFilter {
    pub state: Option<UrlState>,
    pub kind: Option<ResourceKind>,
    /// 2, 3, 4 or 5 for a status class; see [`UrlFilter::status_class`].
    pub status_class: Option<u8>,
    pub status_code: Option<u16>,
    /// Substring match on the URL.
    pub url_contains: Option<String>,
    /// `Some(true)` for internal only, `Some(false)` for external only.
    pub internal: Option<bool>,
    pub max_depth: Option<u32>,
    /// Only URLs that were actually fetched.
    pub crawled_only: bool,
    /// Only URLs affected by this issue. This is how clicking an issue in the
    /// UI filters the URL table.
    pub issue_id: Option<String>,
    /// `Some(true)` for indexable pages only, `Some(false)` for the rest.
    pub indexable: Option<bool>,
}

impl UrlFilter {
    pub fn internal() -> Self {
        Self {
            internal: Some(true),
            ..Default::default()
        }
    }

    pub fn status_class(class: u8) -> Self {
        Self {
            status_class: Some(class),
            ..Default::default()
        }
    }

    /// The URLs affected by one issue.
    pub fn for_issue(issue_id: impl Into<String>) -> Self {
        Self {
            issue_id: Some(issue_id.into()),
            ..Default::default()
        }
    }

    pub fn html_pages() -> Self {
        Self {
            kind: Some(ResourceKind::Html),
            internal: Some(true),
            crawled_only: true,
            ..Default::default()
        }
    }

    /// Build the `WHERE` clause and its bound parameters.
    fn where_clause(&self) -> (String, Vec<Box<dyn ToSql>>) {
        let mut clauses: Vec<String> = Vec::new();
        let mut params: Vec<Box<dyn ToSql>> = Vec::new();

        if let Some(state) = self.state {
            clauses.push("state = ?".into());
            params.push(Box::new(state as i64));
        }
        if let Some(kind) = self.kind {
            clauses.push("kind = ?".into());
            params.push(Box::new(kind as i64));
        }
        if let Some(class) = self.status_class {
            clauses.push("status_code >= ? AND status_code < ?".into());
            params.push(Box::new(class as i64 * 100));
            params.push(Box::new((class as i64 + 1) * 100));
        }
        if let Some(code) = self.status_code {
            clauses.push("status_code = ?".into());
            params.push(Box::new(code as i64));
        }
        if let Some(needle) = &self.url_contains {
            clauses.push("url LIKE ?".into());
            params.push(Box::new(format!("%{needle}%")));
        }
        if let Some(internal) = self.internal {
            clauses.push("is_internal = ?".into());
            params.push(Box::new(internal as i64));
        }
        if let Some(depth) = self.max_depth {
            clauses.push("depth <= ?".into());
            params.push(Box::new(depth as i64));
        }
        if self.crawled_only {
            clauses.push("status_code IS NOT NULL".into());
        }
        if let Some(issue) = &self.issue_id {
            clauses.push("id IN (SELECT url_id FROM issue_urls WHERE issue_id = ?)".into());
            params.push(Box::new(issue.clone()));
        }
        if let Some(indexable) = self.indexable {
            clauses.push("indexable = ?".into());
            params.push(Box::new(indexable as i64));
        }

        if clauses.is_empty() {
            (String::new(), params)
        } else {
            (format!(" WHERE {}", clauses.join(" AND ")), params)
        }
    }
}

/// Sortable columns. A fixed set, so the column name is never caller-supplied.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SortBy {
    #[default]
    Id,
    Url,
    StatusCode,
    Title,
    TitleLength,
    MetaDescriptionLength,
    WordCount,
    Depth,
    ResponseTime,
    Inlinks,
    LinkScore,
    Size,
}

impl SortBy {
    fn column(self) -> &'static str {
        match self {
            SortBy::Id => "id",
            SortBy::Url => "url",
            SortBy::StatusCode => "status_code",
            SortBy::Title => "title",
            SortBy::TitleLength => "title_length",
            SortBy::MetaDescriptionLength => "meta_description_length",
            SortBy::WordCount => "word_count",
            SortBy::Depth => "depth",
            SortBy::ResponseTime => "response_time_ms",
            SortBy::Inlinks => "inlinks",
            SortBy::LinkScore => "link_score",
            SortBy::Size => "body_bytes",
        }
    }
}

/// One page of results, plus the total that matched.
#[derive(Debug, Clone, PartialEq)]
pub struct PageOf<T> {
    pub rows: Vec<T>,
    pub total: u64,
    pub offset: u64,
}

/// A page of the URL table.
pub fn list_urls(
    conn: &Connection,
    filter: &UrlFilter,
    sort: SortBy,
    descending: bool,
    limit: u32,
    offset: u64,
) -> Result<PageOf<UrlRow>> {
    let (where_clause, params) = filter.where_clause();
    let bound: Vec<&dyn ToSql> = params.iter().map(|p| p.as_ref()).collect();

    // SQLite integers are signed, so counts come back as i64.
    let total = conn.query_row(
        &format!("SELECT count(*) FROM urls{where_clause}"),
        bound.as_slice(),
        |r| r.get::<_, i64>(0),
    )? as u64;

    // `id` is appended as a tiebreaker so paging never repeats or skips a row
    // when the sort column has duplicates.
    let sql = format!(
        "SELECT {URL_COLUMNS} FROM urls{where_clause}
         ORDER BY {} {}, id ASC LIMIT ?{} OFFSET ?{}",
        sort.column(),
        if descending { "DESC" } else { "ASC" },
        params.len() + 1,
        params.len() + 2,
    );

    let mut stmt = conn.prepare(&sql)?;
    let mut all: Vec<Box<dyn ToSql>> = params;
    all.push(Box::new(limit as i64));
    all.push(Box::new(offset as i64));
    let bound: Vec<&dyn ToSql> = all.iter().map(|p| p.as_ref()).collect();

    let rows = stmt
        .query_map(bound.as_slice(), url_row)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(PageOf {
        rows,
        total,
        offset,
    })
}

/// A single URL by id.
pub fn url_by_id(conn: &Connection, id: i64) -> Result<Option<UrlRow>> {
    Ok(conn
        .query_row(
            &format!("SELECT {URL_COLUMNS} FROM urls WHERE id = ?1"),
            [id],
            url_row,
        )
        .optional()?)
}

/// A single URL by its address.
pub fn url_by_address(conn: &Connection, url: &str) -> Result<Option<UrlRow>> {
    Ok(conn
        .query_row(
            &format!("SELECT {URL_COLUMNS} FROM urls WHERE url = ?1"),
            [url],
            url_row,
        )
        .optional()?)
}

/// One link, as shown in the inlinks/outlinks panels.
#[derive(Debug, Clone, PartialEq)]
pub struct LinkRow {
    pub source_url: String,
    pub target_url: Option<String>,
    pub anchor_text: String,
    pub rel: Option<String>,
    pub position: LinkPosition,
    /// How the link was expressed: a hyperlink, an `<img src>`, a canonical...
    /// The UI shows this as the "Type" column.
    pub source: LinkSource,
    pub nofollow: bool,
    pub status_code: Option<u16>,
}

/// Everything referencing this URL — hyperlinks, images, stylesheets, scripts,
/// canonicals.
///
/// All reference types are returned, with [`LinkRow::source`] distinguishing
/// them, because "which stylesheet 404s and who asks for it" matters as much as
/// which page links where. The followed-anchor count used for link scoring is
/// the `inlinks` column on the URL row instead.
pub fn inlinks(conn: &Connection, url_id: i64, limit: u32) -> Result<Vec<LinkRow>> {
    let mut stmt = conn.prepare(
        "SELECT src.url, links.target_url, links.anchor_text, links.rel, links.position,
                links.link_source, links.nofollow, src.status_code
         FROM links
         JOIN urls AS src ON src.id = links.source_id
         WHERE links.target_key = (SELECT dedup_key FROM urls WHERE id = ?1)
         ORDER BY src.url LIMIT ?2",
    )?;
    let rows = stmt
        .query_map([url_id, limit as i64], link_row)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

/// Links *from* this URL, with the status of each target when known.
pub fn outlinks(conn: &Connection, url_id: i64, limit: u32) -> Result<Vec<LinkRow>> {
    let mut stmt = conn.prepare(
        "SELECT src.url, links.target_url, links.anchor_text, links.rel, links.position,
                links.link_source, links.nofollow, target.status_code
         FROM links
         JOIN urls AS src ON src.id = links.source_id
         LEFT JOIN urls AS target ON target.dedup_key = links.target_key
         WHERE links.source_id = ?1
         ORDER BY links.id LIMIT ?2",
    )?;
    let rows = stmt
        .query_map([url_id, limit as i64], link_row)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

fn link_row(row: &Row<'_>) -> rusqlite::Result<LinkRow> {
    Ok(LinkRow {
        source_url: row.get(0)?,
        target_url: row.get(1)?,
        anchor_text: row.get(2)?,
        rel: row.get(3)?,
        position: position_from_i64(row.get(4)?),
        source: link_source_from_i64(row.get(5)?),
        nofollow: row.get::<_, i64>(6)? != 0,
        status_code: row.get::<_, Option<i64>>(7)?.map(|s| s as u16),
    })
}

/// Response headers stored for a URL.
pub fn headers(conn: &Connection, url_id: i64) -> Result<Vec<(String, String)>> {
    let mut stmt =
        conn.prepare("SELECT name, value FROM headers WHERE url_id = ?1 ORDER BY name")?;
    let rows = stmt
        .query_map([url_id], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

/// The redirect chain for a URL, in order.
pub fn redirect_chain(conn: &Connection, url_id: i64) -> Result<Vec<(u32, String, String, u16)>> {
    let mut stmt = conn.prepare(
        "SELECT hop, from_url, to_url, status FROM redirects WHERE url_id = ?1 ORDER BY hop",
    )?;
    let rows = stmt
        .query_map([url_id], |r| {
            Ok((
                r.get::<_, i64>(0)? as u32,
                r.get(1)?,
                r.get(2)?,
                r.get::<_, i64>(3)? as u16,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

/// Images found on a URL.
#[derive(Debug, Clone, PartialEq)]
pub struct ImageRow {
    pub src: Option<String>,
    pub raw_src: String,
    pub alt: Option<String>,
    pub alt_missing: bool,
    pub declared_width: Option<u32>,
    pub declared_height: Option<u32>,
    /// Byte size of the image, when it was itself crawled.
    pub body_bytes: Option<u64>,
    pub status_code: Option<u16>,
}

pub fn images(conn: &Connection, url_id: i64) -> Result<Vec<ImageRow>> {
    let mut stmt = conn.prepare(
        "SELECT images.src, images.raw_src, images.alt, images.alt_missing,
                images.declared_width, images.declared_height, target.body_bytes,
                target.status_code
         FROM images
         LEFT JOIN urls AS target ON target.dedup_key = images.src_key
         WHERE images.url_id = ?1 ORDER BY images.id",
    )?;
    let rows = stmt
        .query_map([url_id], |r| {
            Ok(ImageRow {
                src: r.get(0)?,
                raw_src: r.get(1)?,
                alt: r.get(2)?,
                alt_missing: r.get::<_, i64>(3)? != 0,
                declared_width: r.get::<_, Option<i64>>(4)?.map(|v| v.max(0) as u32),
                declared_height: r.get::<_, Option<i64>>(5)?.map(|v| v.max(0) as u32),
                body_bytes: r.get::<_, Option<i64>>(6)?.map(|v| v.max(0) as u64),
                status_code: r.get::<_, Option<i64>>(7)?.map(|v| v as u16),
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

/// `(status code, count)`, highest count first.
pub fn status_code_summary(conn: &Connection) -> Result<Vec<(u16, u64)>> {
    let mut stmt = conn.prepare(
        "SELECT status_code, count(*) FROM urls
         WHERE status_code IS NOT NULL GROUP BY status_code ORDER BY count(*) DESC",
    )?;
    let rows = stmt
        .query_map([], |r| {
            Ok((r.get::<_, i64>(0)? as u16, r.get::<_, i64>(1)? as u64))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

/// `(depth, count)` for crawled internal HTML pages — the crawl-depth
/// distribution the UI charts and the Phase 2 audit reports on.
pub fn depth_distribution(conn: &Connection) -> Result<Vec<(u32, u64)>> {
    let mut stmt = conn.prepare(
        "SELECT depth, count(*) FROM urls
         WHERE is_internal = 1 AND status_code IS NOT NULL
         GROUP BY depth ORDER BY depth",
    )?;
    let rows = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?.max(0) as u32,
                r.get::<_, i64>(1)? as u64,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

/// URLs declared in a sitemap, with whether they were reached by the crawl.
pub fn sitemap_urls(conn: &Connection, limit: u32) -> Result<Vec<(String, Option<i64>)>> {
    let mut stmt = conn.prepare(
        "SELECT sitemap_urls.url, urls.id
         FROM sitemap_urls
         LEFT JOIN urls ON urls.dedup_key = sitemap_urls.dedup_key
         ORDER BY sitemap_urls.url LIMIT ?1",
    )?;
    let rows = stmt
        .query_map([limit as i64], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

/// One issue type, as recorded by the audit pass.
#[derive(Debug, Clone, PartialEq)]
pub struct IssueRow {
    pub issue_id: String,
    pub title: String,
    pub severity: i64,
    pub url_count: u64,
}

/// Every issue found, worst first. The descriptions and fix text live in
/// `audit::catalog`; use [`crate::store::Store::audit_report`] for those.
pub fn issues(conn: &Connection) -> Result<Vec<IssueRow>> {
    let mut stmt = conn.prepare(
        "SELECT issue_id, title, severity, url_count FROM issues
         WHERE url_count > 0 ORDER BY severity, url_count DESC",
    )?;
    let rows = stmt
        .query_map([], |r| {
            Ok(IssueRow {
                issue_id: r.get(0)?,
                title: r.get(1)?,
                severity: r.get(2)?,
                url_count: r.get::<_, i64>(3)? as u64,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

/// The issues affecting one URL, for the URL detail panel.
pub fn issues_for_url(conn: &Connection, url_id: i64) -> Result<Vec<(String, Option<String>)>> {
    let mut stmt = conn.prepare(
        "SELECT issue_urls.issue_id, issue_urls.detail
         FROM issue_urls
         JOIN issues ON issues.issue_id = issue_urls.issue_id
         WHERE issue_urls.url_id = ?1
         ORDER BY issues.severity",
    )?;
    let rows = stmt
        .query_map([url_id], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

/// The per-URL detail recorded for one issue, keyed by URL id.
pub fn issue_details(
    conn: &Connection,
    issue_id: &str,
    limit: u32,
) -> Result<Vec<(i64, String, Option<String>)>> {
    let mut stmt = conn.prepare(
        "SELECT issue_urls.url_id, urls.url, issue_urls.detail
         FROM issue_urls
         JOIN urls ON urls.id = issue_urls.url_id
         WHERE issue_urls.issue_id = ?1
         ORDER BY urls.url LIMIT ?2",
    )?;
    let rows = stmt
        .query_map(params![issue_id, limit as i64], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

/// Structured-data types seen across the crawl, with page counts.
pub fn structured_data_summary(conn: &Connection) -> Result<Vec<(String, u64)>> {
    let mut stmt = conn.prepare(
        "SELECT types, count(DISTINCT url_id) FROM structured_data
         WHERE types != '' GROUP BY types ORDER BY count(DISTINCT url_id) DESC",
    )?;
    let rows = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get::<_, i64>(1)? as u64)))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}
