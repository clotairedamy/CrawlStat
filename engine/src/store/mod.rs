//! The crawl store: SQLite, one file per crawl.
//!
//! [`Store`] is the synchronous, single-threaded owner of the connection.
//! [`StoreHandle`] wraps it in a task so async workers can talk to it from
//! anywhere: all database work happens on one blocking thread, which keeps
//! SQLite happy and makes write batching natural.
//!
//! A batch is applied in **one transaction**, holding both the finished page
//! rows and the URLs discovered on them. That atomicity is what makes a crawl
//! resumable: a crash can never leave a page marked done while the links it
//! found are missing.

pub mod queries;
pub mod schema;
mod writer;

use std::path::{Path, PathBuf};
use std::time::Duration;

use chrono::Utc;
use rusqlite::{named_params, params, Connection, OptionalExtension};

use crate::config::CrawlConfig;
use crate::error::{EngineError, Result};
use crate::model::{
    CrawledUrl, FrontierUrl, LinkPosition, LinkSource, ParsedPage, ResourceKind, UrlState,
};
use crate::sitemap::SitemapEntry;

pub use writer::{StoreHandle, WriteBatch};

/// A URL taken off the frontier and marked in progress.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaimedUrl {
    pub id: i64,
    pub url: String,
    pub depth: u32,
    pub kind: ResourceKind,
    pub is_internal: bool,
    /// How many times this URL has been attempted, including this one.
    pub attempts: u8,
}

/// Live counters, cheap enough to poll for the progress stream.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CrawlStats {
    pub total: u64,
    pub queued: u64,
    pub in_progress: u64,
    pub done: u64,
    pub failed: u64,
    pub skipped: u64,
    pub blocked_by_robots: u64,
    pub status_2xx: u64,
    pub status_3xx: u64,
    pub status_4xx: u64,
    pub status_5xx: u64,
}

impl CrawlStats {
    /// URLs that will not be visited again.
    pub fn finished(&self) -> u64 {
        self.done + self.failed + self.skipped + self.blocked_by_robots
    }
}

/// Severity of a [`Store::log`] entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogLevel {
    Info,
    Warning,
    Error,
}

impl LogLevel {
    fn as_str(self) -> &'static str {
        match self {
            LogLevel::Info => "info",
            LogLevel::Warning => "warning",
            LogLevel::Error => "error",
        }
    }
}

/// The crawl database.
pub struct Store {
    conn: Connection,
    path: PathBuf,
}

impl std::fmt::Debug for Store {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Store")
            .field("path", &self.path)
            .finish_non_exhaustive()
    }
}

impl Store {
    /// Open (or create) a crawl database.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        let conn = Connection::open(&path)?;
        schema::apply_pragmas(&conn)?;
        schema::migrate(&conn, &path.to_string_lossy())?;
        Ok(Self { conn, path })
    }

    /// An in-memory database, for tests.
    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch("PRAGMA foreign_keys = ON; PRAGMA temp_store = MEMORY;")?;
        schema::migrate(&conn, ":memory:")?;
        Ok(Self {
            conn,
            path: PathBuf::from(":memory:"),
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn connection(&self) -> &Connection {
        &self.conn
    }

    /// Mutable access, needed to open a transaction.
    pub fn connection_mut(&mut self) -> &mut Connection {
        &mut self.conn
    }

    // --- meta ---------------------------------------------------------------

    pub fn set_meta(&self, key: &str, value: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO meta (key, value) VALUES (?1, ?2)
             ON CONFLICT (key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
        Ok(())
    }

    pub fn meta(&self, key: &str) -> Result<Option<String>> {
        Ok(self
            .conn
            .query_row("SELECT value FROM meta WHERE key = ?1", [key], |r| r.get(0))
            .optional()?)
    }

    /// Record the configuration and starting state of a new crawl.
    pub fn initialise_crawl(&self, config: &CrawlConfig, root_host: &str) -> Result<()> {
        let json = serde_json::to_string(config)
            .map_err(|e| EngineError::Config(format!("config is not serialisable: {e}")))?;
        self.set_meta("config", &json)?;
        self.set_meta("root_host", root_host)?;
        self.set_meta("engine_version", env!("CARGO_PKG_VERSION"))?;
        self.set_meta("created_at", &Utc::now().to_rfc3339())?;
        self.set_meta("state", "running")?;
        Ok(())
    }

    /// The configuration a crawl was started with, for resuming.
    pub fn stored_config(&self) -> Result<Option<CrawlConfig>> {
        let Some(json) = self.meta("config")? else {
            return Ok(None);
        };
        serde_json::from_str(&json)
            .map(Some)
            .map_err(|e| EngineError::Config(format!("stored config is unreadable: {e}")))
    }

    pub fn log(&self, level: LogLevel, message: &str, url: Option<&str>) -> Result<()> {
        self.conn.execute(
            "INSERT INTO crawl_log (at, level, message, url) VALUES (?1, ?2, ?3, ?4)",
            params![Utc::now().timestamp(), level.as_str(), message, url],
        )?;
        Ok(())
    }

    /// Every log entry, oldest first.
    pub fn log_entries(&self) -> Result<Vec<(String, String, Option<String>)>> {
        let mut stmt = self
            .conn
            .prepare("SELECT level, message, url FROM crawl_log ORDER BY id")?;
        let rows = stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    // --- frontier -----------------------------------------------------------

    /// Add URLs to the frontier, ignoring any already present.
    ///
    /// Returns how many were new. Dedup is the `urls_dedup_key` unique index
    /// doing the work, so no lookup round-trip is needed per URL.
    pub fn enqueue(&mut self, urls: &[FrontierUrl]) -> Result<usize> {
        let tx = self.conn.transaction()?;
        let added = enqueue_in(&tx, urls)?;
        tx.commit()?;
        Ok(added)
    }

    /// Take up to `limit` queued URLs, marking them in progress.
    ///
    /// Lowest depth first, so the crawl is breadth-first; `id` breaks ties,
    /// which keeps the order deterministic for a given discovery sequence.
    pub fn claim(&self, limit: usize) -> Result<Vec<ClaimedUrl>> {
        let mut stmt = self.conn.prepare_cached(
            "UPDATE urls SET state = ?1, attempts = attempts + 1
             WHERE id IN (
                 SELECT id FROM urls WHERE state = ?2 ORDER BY depth, id LIMIT ?3
             )
             RETURNING id, url, depth, kind, is_internal, attempts",
        )?;
        let rows = stmt
            .query_map(
                params![
                    UrlState::InProgress as i64,
                    UrlState::Queued as i64,
                    limit as i64
                ],
                |row| {
                    Ok(ClaimedUrl {
                        id: row.get(0)?,
                        url: row.get(1)?,
                        depth: row.get::<_, i64>(2)? as u32,
                        kind: kind_from_i64(row.get(3)?),
                        is_internal: row.get::<_, i64>(4)? != 0,
                        attempts: row.get::<_, i64>(5)?.clamp(0, u8::MAX as i64) as u8,
                    })
                },
            )?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    /// Put a claimed URL back on the queue, for a retry after a failure.
    pub fn requeue(&self, id: i64) -> Result<()> {
        self.conn.execute(
            "UPDATE urls SET state = ?1 WHERE id = ?2",
            params![UrlState::Queued as i64, id],
        )?;
        Ok(())
    }

    /// Reset anything left in progress back to queued.
    ///
    /// Run when opening an existing crawl: rows stuck in progress are the ones
    /// that were in flight when the process died.
    pub fn reset_in_progress(&self) -> Result<usize> {
        let n = self.conn.execute(
            "UPDATE urls SET state = ?1 WHERE state = ?2",
            params![UrlState::Queued as i64, UrlState::InProgress as i64],
        )?;
        Ok(n)
    }

    /// Mark every remaining queued URL as skipped, with a reason logged once.
    ///
    /// Used when a limit is reached: the rows stay visible in the UI as "not
    /// crawled" rather than vanishing.
    pub fn skip_remaining_queued(&self) -> Result<usize> {
        let n = self.conn.execute(
            "UPDATE urls SET state = ?1 WHERE state IN (?2, ?3)",
            params![
                UrlState::Skipped as i64,
                UrlState::Queued as i64,
                UrlState::InProgress as i64
            ],
        )?;
        Ok(n)
    }

    pub fn stats(&self) -> Result<CrawlStats> {
        let mut stats = CrawlStats::default();
        let mut stmt = self
            .conn
            .prepare_cached("SELECT state, count(*) FROM urls GROUP BY state")?;
        for row in stmt.query_map([], |r| {
            Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)? as u64))
        })? {
            let (state, count) = row?;
            stats.total += count;
            match UrlState::from_i64(state) {
                Some(UrlState::Queued) => stats.queued = count,
                Some(UrlState::InProgress) => stats.in_progress = count,
                Some(UrlState::Done) => stats.done = count,
                Some(UrlState::Failed) => stats.failed = count,
                Some(UrlState::Skipped) => stats.skipped = count,
                Some(UrlState::BlockedByRobots) => stats.blocked_by_robots = count,
                None => {}
            }
        }

        let mut stmt = self.conn.prepare_cached(
            "SELECT status_code / 100, count(*) FROM urls
             WHERE status_code IS NOT NULL GROUP BY status_code / 100",
        )?;
        for row in stmt.query_map([], |r| {
            Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)? as u64))
        })? {
            let (class, count) = row?;
            match class {
                2 => stats.status_2xx = count,
                3 => stats.status_3xx = count,
                4 => stats.status_4xx = count,
                5 => stats.status_5xx = count,
                _ => {}
            }
        }
        Ok(stats)
    }

    /// How many URLs have been visited (done or failed), for limit checks.
    pub fn crawled_count(&self) -> Result<u64> {
        let count: i64 = self.conn.query_row(
            "SELECT count(*) FROM urls WHERE state IN (?1, ?2)",
            params![UrlState::Done as i64, UrlState::Failed as i64],
            |r| r.get(0),
        )?;
        Ok(count as u64)
    }

    // --- writing results ----------------------------------------------------

    /// Apply a batch of results and newly discovered URLs in one transaction.
    pub fn apply(&mut self, batch: &WriteBatch) -> Result<usize> {
        let tx = self.conn.transaction()?;
        for result in &batch.results {
            write_result_in(&tx, result)?;
        }
        let added = enqueue_in(&tx, &batch.discovered)?;
        for (sitemap_url, entry, key) in &batch.sitemap_entries {
            tx.execute(
                "INSERT OR REPLACE INTO sitemap_urls
                 (url, dedup_key, sitemap_url, lastmod, changefreq, priority)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    entry.loc,
                    key,
                    sitemap_url,
                    entry.lastmod,
                    entry.changefreq,
                    entry.priority
                ],
            )?;
        }
        for (level, message, url) in &batch.log {
            tx.execute(
                "INSERT INTO crawl_log (at, level, message, url) VALUES (?1, ?2, ?3, ?4)",
                params![Utc::now().timestamp(), level.as_str(), message, url],
            )?;
        }
        tx.commit()?;
        Ok(added)
    }

    /// Finish the crawl: build the analysis indexes, count inlinks, record the
    /// end state.
    ///
    /// Inlink counts are computed here rather than maintained during the crawl
    /// because a link's target usually has not been crawled yet when the link
    /// is written — one aggregate at the end is both simpler and faster.
    pub fn finalize(&mut self, state: &str) -> Result<()> {
        // Indexes first: the aggregate below then reads `links` in target-key
        // order instead of sorting it.
        schema::create_analysis_indexes(&self.conn)?;
        self.compute_inlinks()?;

        self.set_meta("state", state)?;
        self.set_meta("finished_at", &Utc::now().to_rfc3339())?;
        // Refresh the planner's statistics so the UI's queries pick the right
        // indexes.
        self.conn.execute_batch("ANALYZE;")?;
        Ok(())
    }

    /// Fill in `urls.inlinks` and `urls.internal_inlinks`.
    ///
    /// Done in two steps, both bounded in memory:
    ///
    /// 1. Aggregate the link graph once into a helper table. Counting with a
    ///    correlated subquery per row would instead be one index lookup per
    ///    URL, repeated for both columns.
    /// 2. Apply it in chunks of [`FINALIZE_CHUNK`] rows, each its own
    ///    transaction. A single `UPDATE` over every row would hold every dirty
    ///    page and index entry in the page cache until it committed, which on
    ///    a large crawl is hundreds of megabytes.
    fn compute_inlinks(&mut self) -> Result<()> {
        self.conn.execute_batch(
            "DROP TABLE IF EXISTS inlink_counts;
             CREATE TABLE inlink_counts (
                 target_key        TEXT PRIMARY KEY,
                 total             INTEGER NOT NULL,
                 followed_internal INTEGER NOT NULL
             ) WITHOUT ROWID;",
        )?;

        // Followed `<a href>` links only: an image or canonical reference is
        // not an internal link for ranking purposes.
        self.conn.execute(
            "INSERT INTO inlink_counts (target_key, total, followed_internal)
             SELECT target_key,
                    count(*),
                    sum(CASE WHEN is_internal = 1 AND nofollow = 0 THEN 1 ELSE 0 END)
             FROM links
             WHERE link_source = ?1 AND target_key IS NOT NULL
             GROUP BY target_key",
            params![LinkSource::Anchor as i64],
        )?;

        let max_id: i64 =
            self.conn
                .query_row("SELECT COALESCE(MAX(id), 0) FROM urls", [], |r| r.get(0))?;

        let mut from = 0i64;
        while from < max_id {
            let to = from + FINALIZE_CHUNK;
            let tx = self.conn.transaction()?;
            tx.execute(
                "UPDATE urls SET
                     inlinks = COALESCE(
                         (SELECT total FROM inlink_counts WHERE target_key = urls.dedup_key), 0),
                     internal_inlinks = COALESCE(
                         (SELECT followed_internal FROM inlink_counts
                          WHERE target_key = urls.dedup_key), 0)
                 WHERE id > ?1 AND id <= ?2",
                params![from, to],
            )?;
            tx.commit()?;
            from = to;
        }

        self.conn
            .execute_batch("DROP TABLE IF EXISTS inlink_counts;")?;
        Ok(())
    }

    /// Run the audit pass over a finished crawl.
    pub fn audit(
        &mut self,
        config: &crate::audit::AuditConfig,
    ) -> Result<crate::audit::AuditReport> {
        crate::audit::run(self, config)
    }

    /// Read the issues an earlier audit pass recorded.
    pub fn audit_report(
        &self,
        config: &crate::audit::AuditConfig,
    ) -> Result<crate::audit::AuditReport> {
        crate::audit::read_report(self, config)
    }

    /// Flush the write-ahead log into the main database file.
    pub fn checkpoint(&self) -> Result<()> {
        self.conn
            .execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")?;
        Ok(())
    }
}

/// Insert frontier rows inside an existing transaction.
fn enqueue_in(tx: &Connection, urls: &[FrontierUrl]) -> Result<usize> {
    if urls.is_empty() {
        return Ok(0);
    }
    let mut stmt = tx.prepare_cached(
        "INSERT INTO urls
             (url, dedup_key, depth, state, kind, is_internal, discovered_from, discovered_via)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
         ON CONFLICT (dedup_key) DO NOTHING",
    )?;
    let mut added = 0;
    for url in urls {
        added += stmt.execute(params![
            url.url,
            url.key,
            url.depth as i64,
            url.state as i64,
            url.kind as i64,
            url.is_internal as i64,
            url.discovered_from,
            url.via.map(|via| via as i64),
        ])?;
    }
    Ok(added)
}

/// Write one result row plus its children inside an existing transaction.
fn write_result_in(tx: &Connection, result: &CrawledUrl) -> Result<()> {
    let parsed = result.parsed.as_ref();
    let empty = ParsedPage::default();
    let page = parsed.unwrap_or(&empty);

    let canonical_is_self = page
        .canonical_key
        .as_ref()
        .map(|canonical| {
            // Compared on the dedup key so trailing slashes and tracking
            // parameters do not produce false "canonicalised" findings.
            tx.query_row(
                "SELECT dedup_key FROM urls WHERE id = ?1",
                params![result.url_id],
                |r| r.get::<_, String>(0),
            )
            .map(|key| &key == canonical)
            .unwrap_or(false)
        })
        .map(|same| same as i64);

    let structured_types = page.structured_data_types().join(", ");

    tx.prepare_cached(
        "UPDATE urls SET
             state = :state,
             kind = :kind,
             status_code = :status_code,
             final_status_code = :final_status_code,
             final_url = :final_url,
             redirect_count = :redirect_count,
             redirect_loop = :redirect_loop,
             response_time_ms = :response_time_ms,
             ttfb_ms = :ttfb_ms,
             content_type = :content_type,
             content_length = :content_length,
             body_bytes = :body_bytes,
             content_encoding = :content_encoding,
             x_robots_tag = :x_robots_tag,
             content_hash = :content_hash,
             body_truncated = :body_truncated,
             title = :title,
             title_length = :title_length,
             meta_description = :meta_description,
             meta_description_length = :meta_description_length,
             meta_robots = :meta_robots,
             meta_refresh = :meta_refresh,
             canonical = :canonical,
             canonical_is_self = :canonical_is_self,
             amp_html = :amp_html,
             html_lang = :html_lang,
             charset = :charset,
             has_viewport = :has_viewport,
             h1 = :h1,
             h1_count = :h1_count,
             h2_count = :h2_count,
             word_count = :word_count,
             text_hash = :text_hash,
             text_simhash = :text_simhash,
             internal_links = :internal_links,
             external_links = :external_links,
             image_count = :image_count,
             structured_data_types = :structured_data_types,
             error_kind = :error_kind,
             error_message = :error_message,
             fetched_at = :fetched_at,
             html = :html
         WHERE id = :id",
    )?
    .execute(named_params! {
        ":id": result.url_id,
        ":state": result.state as i64,
        ":kind": result.kind as i64,
        ":status_code": result.status_code,
        ":final_status_code": result.final_status_code,
        ":final_url": result.final_url,
        ":redirect_count": result.redirect_chain.len() as i64,
        // Detected by the fetcher while walking the chain: it is the only thing
        // that knows the URL the chain started from, which is usually the URL
        // the loop comes back to.
        ":redirect_loop": result.redirect_loop as i64,
        ":response_time_ms": result.response_time.map(|d| d.as_millis() as i64),
        ":ttfb_ms": result.ttfb.map(|d| d.as_millis() as i64),
        ":content_type": result.content_type,
        ":content_length": result.content_length.map(|l| l as i64),
        ":body_bytes": result.body_bytes.map(|l| l as i64),
        ":content_encoding": result.content_encoding,
        ":x_robots_tag": result.x_robots_tag,
        ":content_hash": result.content_hash,
        ":body_truncated": result.truncated as i64,
        ":title": page.title,
        ":title_length": page.title.as_ref().map(|t| t.chars().count() as i64),
        ":meta_description": page.meta_description,
        ":meta_description_length":
            page.meta_description.as_ref().map(|d| d.chars().count() as i64),
        ":meta_robots": page.meta_robots,
        ":meta_refresh": page.meta_refresh,
        ":canonical": page.canonical,
        ":canonical_is_self": canonical_is_self,
        ":amp_html": page.amp_html,
        ":html_lang": page.html_lang,
        ":charset": page.charset,
        ":has_viewport": parsed.map(|_| page.has_viewport as i64),
        ":h1": page.h1.first(),
        ":h1_count": parsed.map(|_| page.h1.len() as i64),
        ":h2_count": parsed.map(|_| page.h2.len() as i64),
        ":word_count": parsed.map(|_| page.word_count as i64),
        ":text_hash": page.text_hash,
        // SQLite integers are signed; a u64 simhash is stored as the same
        // 64 bits reinterpreted, and read back the same way.
        ":text_simhash": page.text_simhash.map(|h| h as i64),
        ":internal_links": parsed.map(|_| result.internal_links as i64),
        ":external_links": parsed.map(|_| result.external_links as i64),
        ":image_count": parsed.map(|_| page.images.len() as i64),
        ":structured_data_types":
            (!structured_types.is_empty()).then_some(structured_types.as_str()),
        ":error_kind": result.error_kind,
        ":error_message": result.error_message,
        ":fetched_at": result.fetched_at,
        ":html": result.html,
    })?;

    // Children are replaced wholesale: a URL re-fetched after a crash or a
    // retry must not accumulate two copies of its links.
    for sql in CHILD_DELETES {
        tx.execute(sql, params![result.url_id])?;
    }

    {
        let mut stmt =
            tx.prepare_cached("INSERT INTO headers (url_id, name, value) VALUES (?1, ?2, ?3)")?;
        for (name, value) in &result.headers {
            stmt.execute(params![result.url_id, name, value])?;
        }
    }

    {
        let mut stmt = tx.prepare_cached(
            "INSERT INTO redirects (url_id, hop, from_url, to_url, status)
             VALUES (?1, ?2, ?3, ?4, ?5)",
        )?;
        for hop in &result.redirect_chain {
            stmt.execute(params![
                result.url_id,
                hop.hop as i64,
                hop.from_url,
                hop.to_url,
                hop.status as i64
            ])?;
        }
    }

    if let Some(page) = parsed {
        {
            let mut stmt = tx.prepare_cached(
                "INSERT INTO links
                 (source_id, target_key, target_url, raw_href, anchor_text, rel, position,
                  link_source, nofollow, is_internal)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            )?;
            for link in &page.links {
                stmt.execute(params![
                    result.url_id,
                    link.target_key,
                    link.target,
                    link.raw_href,
                    link.anchor_text,
                    link.rel,
                    link.position as i64,
                    link.source as i64,
                    link.nofollow as i64,
                    link.is_internal as i64,
                ])?;
            }
        }

        {
            let mut stmt = tx.prepare_cached(
                "INSERT INTO images
                 (url_id, raw_src, src, src_key, alt, alt_missing, title,
                  declared_width, declared_height, loading_lazy)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            )?;
            for image in &page.images {
                stmt.execute(params![
                    result.url_id,
                    image.raw_src,
                    image.src,
                    image.src_key,
                    image.alt,
                    image.alt.is_none() as i64,
                    image.title,
                    image.declared_width,
                    image.declared_height,
                    image.loading_lazy as i64,
                ])?;
            }
        }

        {
            let mut stmt = tx.prepare_cached(
                "INSERT INTO headings (url_id, level, position, text) VALUES (?1, ?2, ?3, ?4)",
            )?;
            for (level, texts) in [(1i64, &page.h1), (2, &page.h2)] {
                for (index, text) in texts.iter().enumerate() {
                    stmt.execute(params![result.url_id, level, index as i64, text])?;
                }
            }
        }

        {
            let mut stmt = tx.prepare_cached(
                "INSERT INTO hreflang (url_id, lang, raw_href, href, href_key)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
            )?;
            for entry in &page.hreflang {
                stmt.execute(params![
                    result.url_id,
                    entry.lang,
                    entry.raw_href,
                    entry.href,
                    entry.href_key
                ])?;
            }
        }

        {
            let mut stmt = tx.prepare_cached(
                "INSERT INTO social_tags (url_id, namespace, property, content)
                 VALUES (?1, ?2, ?3, ?4)",
            )?;
            for tag in &page.social {
                stmt.execute(params![
                    result.url_id,
                    tag.namespace,
                    tag.property,
                    tag.content
                ])?;
            }
        }

        {
            let mut stmt = tx.prepare_cached(
                "INSERT INTO structured_data (url_id, format, types, valid, error)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
            )?;
            for block in &page.structured_data {
                stmt.execute(params![
                    result.url_id,
                    block.format,
                    block.types.join(", "),
                    block.valid as i64,
                    block.error
                ])?;
            }
        }
    }

    Ok(())
}

/// Rows updated per transaction when finalising a crawl. Keeps the page cache
/// from having to hold the whole table's dirty pages at once.
const FINALIZE_CHUNK: i64 = 5_000;

/// Child rows are keyed by the parent URL, and are rewritten whenever a URL is
/// written, so a retried fetch replaces rather than duplicates them.
const CHILD_DELETES: &[&str] = &[
    "DELETE FROM links WHERE source_id = ?1",
    "DELETE FROM headers WHERE url_id = ?1",
    "DELETE FROM images WHERE url_id = ?1",
    "DELETE FROM headings WHERE url_id = ?1",
    "DELETE FROM hreflang WHERE url_id = ?1",
    "DELETE FROM social_tags WHERE url_id = ?1",
    "DELETE FROM structured_data WHERE url_id = ?1",
    "DELETE FROM redirects WHERE url_id = ?1",
];

fn kind_from_i64(value: i64) -> ResourceKind {
    match value {
        1 => ResourceKind::Image,
        2 => ResourceKind::Css,
        3 => ResourceKind::JavaScript,
        4 => ResourceKind::Document,
        5 => ResourceKind::Other,
        _ => ResourceKind::Html,
    }
}

pub(crate) fn link_source_from_i64(value: i64) -> LinkSource {
    match value {
        1 => LinkSource::Image,
        2 => LinkSource::Stylesheet,
        3 => LinkSource::Script,
        4 => LinkSource::Canonical,
        5 => LinkSource::Hreflang,
        6 => LinkSource::Redirect,
        7 => LinkSource::Sitemap,
        8 => LinkSource::Form,
        9 => LinkSource::Iframe,
        10 => LinkSource::MetaRefresh,
        _ => LinkSource::Anchor,
    }
}

pub(crate) fn position_from_i64(value: i64) -> LinkPosition {
    match value {
        0 => LinkPosition::Content,
        1 => LinkPosition::Navigation,
        2 => LinkPosition::Header,
        3 => LinkPosition::Footer,
        4 => LinkPosition::Aside,
        _ => LinkPosition::Unknown,
    }
}

/// Helper used by the engine to build sitemap batch entries.
pub fn sitemap_batch_entry(
    sitemap_url: &str,
    entry: &SitemapEntry,
    key: &str,
) -> (String, SitemapEntry, String) {
    (sitemap_url.to_string(), entry.clone(), key.to_string())
}

/// Convert a stored millisecond count back into a `Duration`.
pub fn millis(value: Option<i64>) -> Option<Duration> {
    value.map(|ms| Duration::from_millis(ms.max(0) as u64))
}

#[cfg(test)]
mod tests;
