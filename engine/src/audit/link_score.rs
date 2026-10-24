//! Internal link score: a simplified PageRank over the site's own link graph.
//!
//! The question it answers is "which pages does this site's own linking treat
//! as important?" — which is usually not the same as which pages the owner
//! thinks are important, and is the most useful thing a crawler can say about
//! internal linking.
//!
//! ## Simplifications
//!
//! * Only followed `<a href>` links between internal, crawled HTML pages count.
//!   Images, canonicals, `nofollow` links and external links are excluded.
//! * Repeated links between the same pair count once, as search engines treat
//!   them. A navigation menu repeated on every page should not count forty
//!   times from one page.
//! * Scores are rescaled so the strongest page on the site is 100. An absolute
//!   PageRank value means nothing to a user; a relative one is directly
//!   actionable.
//!
//! ## Why it runs in SQL
//!
//! The iteration could be done in Rust over an in-memory edge list, which would
//! be faster. But the edge list grows with the crawl — roughly five to ten
//! edges per page — and holding it would break the engine's promise that memory
//! does not grow with crawl size. Each iteration here is a single pass over an
//! indexed table, which SQLite streams.

use rusqlite::params;

use crate::error::Result;
use crate::model::{LinkSource, ResourceKind};
use crate::store::Store;

use super::AuditConfig;

/// Build the graph, iterate, and write `urls.link_score`.
pub fn compute(store: &mut Store, config: &AuditConfig, chunk: i64) -> Result<()> {
    build_graph(store)?;

    let nodes: i64 = store
        .connection()
        .query_row("SELECT count(*) FROM ls_rank", [], |row| row.get(0))?;
    if nodes == 0 {
        drop_graph(store)?;
        return Ok(());
    }

    let damping = config.link_score_damping.clamp(0.0, 0.99);
    for _ in 0..config.link_score_iterations {
        iterate(store, nodes, damping)?;
    }

    write_scores(store, chunk)?;
    drop_graph(store)?;
    Ok(())
}

/// Materialise the node set, the deduplicated edge list and the out-degrees.
fn build_graph(store: &mut Store) -> Result<()> {
    let conn = store.connection_mut();
    let tx = conn.transaction()?;

    tx.execute_batch(
        "DROP TABLE IF EXISTS ls_edges;
         DROP TABLE IF EXISTS ls_out;
         DROP TABLE IF EXISTS ls_rank;
         DROP TABLE IF EXISTS ls_next;
         CREATE TABLE ls_edges (src INTEGER NOT NULL, dst INTEGER NOT NULL,
                                PRIMARY KEY (src, dst)) WITHOUT ROWID;
         CREATE TABLE ls_out  (id INTEGER PRIMARY KEY, outdeg INTEGER NOT NULL);
         CREATE TABLE ls_rank (id INTEGER PRIMARY KEY, r REAL NOT NULL);
         CREATE TABLE ls_next (id INTEGER PRIMARY KEY, r REAL NOT NULL);",
    )?;

    // Nodes: internal HTML pages that actually answered.
    tx.execute(
        "INSERT INTO ls_rank (id, r)
         SELECT id, 0.0 FROM urls
         WHERE is_internal = 1 AND kind = ?1 AND status_code = 200",
        params![ResourceKind::Html as i64],
    )?;
    let nodes: i64 = tx.query_row("SELECT count(*) FROM ls_rank", [], |row| row.get(0))?;
    if nodes == 0 {
        tx.commit()?;
        return Ok(());
    }

    // Start every page with an equal share of the total.
    tx.execute("UPDATE ls_rank SET r = 1.0 / ?1", params![nodes as f64])?;

    // Edges, deduplicated by the primary key.
    tx.execute(
        "INSERT OR IGNORE INTO ls_edges (src, dst)
         SELECT links.source_id, target.id
         FROM links
         JOIN urls AS target ON target.dedup_key = links.target_key
         WHERE links.link_source = ?1
           AND links.nofollow = 0
           AND links.is_internal = 1
           AND target.is_internal = 1
           AND target.kind = ?2
           AND target.status_code = 200
           AND links.source_id != target.id
           AND EXISTS (SELECT 1 FROM ls_rank WHERE ls_rank.id = links.source_id)",
        params![LinkSource::Anchor as i64, ResourceKind::Html as i64],
    )?;

    tx.execute_batch(
        "INSERT INTO ls_out (id, outdeg) SELECT src, count(*) FROM ls_edges GROUP BY src;
         CREATE INDEX IF NOT EXISTS ls_edges_dst ON ls_edges (dst);",
    )?;

    tx.commit()?;
    Ok(())
}

/// One PageRank iteration.
fn iterate(store: &mut Store, nodes: i64, damping: f64) -> Result<()> {
    let conn = store.connection_mut();
    let tx = conn.transaction()?;

    // Mass held by pages with no outgoing links. Left unredistributed it would
    // leak out of the system and every score would shrink each iteration.
    let dangling: f64 = tx.query_row(
        "SELECT COALESCE(SUM(r), 0.0) FROM ls_rank
         WHERE NOT EXISTS (SELECT 1 FROM ls_out WHERE ls_out.id = ls_rank.id)",
        [],
        |row| row.get(0),
    )?;

    // Incoming mass, in one streamed pass over the edges.
    tx.execute_batch("DELETE FROM ls_next;")?;
    tx.execute_batch(
        "INSERT INTO ls_next (id, r)
         SELECT ls_edges.dst, SUM(ls_rank.r / ls_out.outdeg)
         FROM ls_edges
         JOIN ls_rank ON ls_rank.id = ls_edges.src
         JOIN ls_out  ON ls_out.id  = ls_edges.src
         GROUP BY ls_edges.dst;",
    )?;

    // Every page gets the teleport share plus its share of the dangling mass.
    let base = (1.0 - damping) / nodes as f64 + damping * dangling / nodes as f64;
    tx.execute(
        "UPDATE ls_rank SET r = ?1 + ?2 *
             COALESCE((SELECT r FROM ls_next WHERE ls_next.id = ls_rank.id), 0.0)",
        params![base, damping],
    )?;

    tx.commit()?;
    Ok(())
}

/// Rescale to 0–100 and copy into `urls.link_score`.
fn write_scores(store: &mut Store, chunk: i64) -> Result<()> {
    let max: f64 =
        store
            .connection()
            .query_row("SELECT COALESCE(MAX(r), 0.0) FROM ls_rank", [], |row| {
                row.get(0)
            })?;
    if max <= 0.0 {
        return Ok(());
    }

    let max_id: i64 =
        store
            .connection()
            .query_row("SELECT COALESCE(MAX(id), 0) FROM urls", [], |row| {
                row.get(0)
            })?;

    let mut from = 0i64;
    while from < max_id {
        let to = from + chunk;
        let conn = store.connection_mut();
        let tx = conn.transaction()?;
        tx.execute(
            "UPDATE urls SET link_score =
                 (SELECT 100.0 * ls_rank.r / ?3 FROM ls_rank WHERE ls_rank.id = urls.id)
             WHERE id > ?1 AND id <= ?2",
            params![from, to, max],
        )?;
        tx.commit()?;
        from = to;
    }
    Ok(())
}

fn drop_graph(store: &mut Store) -> Result<()> {
    store.connection().execute_batch(
        "DROP TABLE IF EXISTS ls_edges;
         DROP TABLE IF EXISTS ls_out;
         DROP TABLE IF EXISTS ls_rank;
         DROP TABLE IF EXISTS ls_next;",
    )?;
    Ok(())
}
