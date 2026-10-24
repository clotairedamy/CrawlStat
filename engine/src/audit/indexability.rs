//! Whether a search engine could index each URL, and why not if it could not.
//!
//! Half the audits only make sense for indexable pages — there is no point
//! nagging about the title of a page that says `noindex`. Rather than repeat
//! the same four conditions in every query, it is computed once into
//! `urls.indexable` with a human-readable `indexability_reason`, which the UI
//! also shows as its own column.
//!
//! The column is `NULL` where the question does not apply: external URLs, and
//! internal non-HTML resources like images and PDFs.

use rusqlite::params;

use crate::error::Result;
use crate::model::{ResourceKind, UrlState};
use crate::store::Store;

/// Reasons, worst first. The first matching one is recorded.
///
/// Order matters: a URL that 404s *and* says `noindex` is reported as the 404,
/// because that is the problem to fix.
pub fn compute(store: &mut Store, chunk: i64) -> Result<()> {
    let max_id: i64 =
        store
            .connection()
            .query_row("SELECT COALESCE(MAX(id), 0) FROM urls", [], |row| {
                row.get(0)
            })?;

    let sql = format!(
        "UPDATE urls SET
             indexable = CASE
                 WHEN is_internal = 0 OR kind != {html} THEN NULL
                 WHEN state = {blocked} THEN 0
                 WHEN status_code IS NULL THEN 0
                 WHEN status_code != 200 THEN 0
                 WHEN LOWER(COALESCE(meta_robots, '')) LIKE '%noindex%' THEN 0
                 WHEN LOWER(COALESCE(x_robots_tag, '')) LIKE '%noindex%' THEN 0
                 WHEN canonical IS NOT NULL AND canonical_is_self = 0 THEN 0
                 ELSE 1
             END,
             indexability_reason = CASE
                 WHEN is_internal = 0 OR kind != {html} THEN NULL
                 WHEN state = {blocked} THEN 'Blocked by robots.txt'
                 WHEN status_code IS NULL THEN 'Not fetched'
                 WHEN status_code >= 300 AND status_code < 400 THEN 'Redirected'
                 WHEN status_code != 200 THEN 'HTTP ' || status_code
                 WHEN LOWER(COALESCE(meta_robots, '')) LIKE '%noindex%' THEN 'noindex (meta robots)'
                 WHEN LOWER(COALESCE(x_robots_tag, '')) LIKE '%noindex%' THEN 'noindex (X-Robots-Tag)'
                 WHEN canonical IS NOT NULL AND canonical_is_self = 0 THEN 'Canonicalised'
                 ELSE NULL
             END
         WHERE id > ?1 AND id <= ?2",
        html = ResourceKind::Html as i64,
        blocked = UrlState::BlockedByRobots as i64,
    );

    // Chunked, for the same reason the crawl's finalisation is: one statement
    // over every row would hold the whole table's dirty pages until it
    // committed.
    let mut from = 0i64;
    while from < max_id {
        let to = from + chunk;
        let conn = store.connection_mut();
        let tx = conn.transaction()?;
        tx.execute(&sql, params![from, to])?;
        tx.commit()?;
        from = to;
    }
    Ok(())
}
