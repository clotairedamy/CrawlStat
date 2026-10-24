//! The crawl database: one SQLite file per crawl.
//!
//! Everything lives here — the frontier, the results, the link graph — which is
//! what makes a crawl resumable after a crash and lets the UI page through
//! 100,000 rows without holding them in memory.
//!
//! ## Layout
//!
//! `urls` is one wide row per URL: its frontier state *and*, once fetched, its
//! response and page fields. The UI's main table is a single indexed query over
//! it with no joins. Everything repeating (links, headers, images, headings,
//! hreflang, redirect hops) is a child table keyed by `url_id`.
//!
//! ## Joining the link graph
//!
//! `links.target_key` holds the *dedup key* of the target rather than a row id.
//! That means a page's links can be written in one batch without first looking
//! up ids for targets that may not exist yet, at the cost of joining on
//! `urls.dedup_key` (a unique index) when reading. Inserts are the hot path in
//! a crawl; reads happen at human speed.

use rusqlite::Connection;

use crate::error::{EngineError, Result};

/// Bumped whenever the schema changes.
///
/// Older databases are migrated forward by [`MIGRATIONS`]; a database from a
/// *newer* build is refused, because this one cannot know what changed.
pub const SCHEMA_VERSION: i64 = 2;

/// Connection pragmas. WAL lets the UI read while the crawl writes.
const PRAGMAS: &str = "
    PRAGMA journal_mode = WAL;
    PRAGMA synchronous = NORMAL;
    PRAGMA foreign_keys = ON;
    -- Sorters and temp tables go to disk, NOT to memory. Building the analysis
    -- indexes over a few hundred thousand rows sorts a lot of data, and with
    -- `temp_store = MEMORY` that sort is held in RAM, which is exactly the
    -- memory growth this engine is supposed not to have.
    PRAGMA temp_store = FILE;
    -- 64 MiB page cache: the frontier's hot index stays resident. This is a
    -- fixed cost, not one that grows with the size of the crawl.
    PRAGMA cache_size = -65536;
    PRAGMA busy_timeout = 5000;
";

/// Tables and the indexes needed *during* a crawl.
///
/// Indexes used only for analysis are created by [`ANALYSIS_INDEXES`] when the
/// crawl finishes, so they do not slow down several hundred thousand inserts.
const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS meta (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
) WITHOUT ROWID;

-- One row per URL: frontier state and, once visited, the result.
CREATE TABLE IF NOT EXISTS urls (
    id                      INTEGER PRIMARY KEY,
    url                     TEXT    NOT NULL,
    dedup_key               TEXT    NOT NULL,
    depth                   INTEGER NOT NULL DEFAULT 0,
    state                   INTEGER NOT NULL DEFAULT 0,
    kind                    INTEGER NOT NULL DEFAULT 0,
    is_internal             INTEGER NOT NULL DEFAULT 1,
    discovered_from         INTEGER REFERENCES urls(id) ON DELETE SET NULL,
    discovered_via          INTEGER,

    -- Response
    status_code             INTEGER,
    final_status_code       INTEGER,
    final_url               TEXT,
    redirect_count          INTEGER NOT NULL DEFAULT 0,
    redirect_loop           INTEGER NOT NULL DEFAULT 0,
    response_time_ms        INTEGER,
    ttfb_ms                 INTEGER,
    content_type            TEXT,
    content_length          INTEGER,
    body_bytes              INTEGER,
    content_encoding        TEXT,
    x_robots_tag            TEXT,
    content_hash            TEXT,
    body_truncated          INTEGER NOT NULL DEFAULT 0,

    -- Page
    title                   TEXT,
    title_length            INTEGER,
    meta_description        TEXT,
    meta_description_length INTEGER,
    meta_robots             TEXT,
    meta_refresh            TEXT,
    canonical               TEXT,
    canonical_is_self       INTEGER,
    amp_html                TEXT,
    html_lang               TEXT,
    charset                 TEXT,
    has_viewport            INTEGER,
    h1                      TEXT,
    h1_count                INTEGER,
    h2_count                INTEGER,
    word_count              INTEGER,
    text_hash               TEXT,
    text_simhash            INTEGER,
    internal_links          INTEGER,
    external_links          INTEGER,
    image_count             INTEGER,
    structured_data_types   TEXT,

    -- Filled in by finalize(), after the link graph is complete.
    inlinks                 INTEGER NOT NULL DEFAULT 0,
    internal_inlinks        INTEGER NOT NULL DEFAULT 0,
    -- Whether a search engine could index this URL, and why not if it could
    -- not. Computed once rather than re-derived by every audit that needs it.
    indexable               INTEGER,
    indexability_reason     TEXT,
    -- Simplified PageRank over the internal link graph, 0.0..=1.0 scaled so the
    -- strongest page on the site is 100.
    link_score              REAL,

    -- Bookkeeping
    error_kind              TEXT,
    error_message           TEXT,
    attempts                INTEGER NOT NULL DEFAULT 0,
    fetched_at              INTEGER,
    html                    TEXT
);

-- Dedup: the frontier relies on this to reject URLs it has already seen.
CREATE UNIQUE INDEX IF NOT EXISTS urls_dedup_key ON urls (dedup_key);
-- Frontier claim: WHERE state = 0 ORDER BY depth, id.
CREATE INDEX IF NOT EXISTS urls_state_depth ON urls (state, depth, id);

CREATE TABLE IF NOT EXISTS links (
    id          INTEGER PRIMARY KEY,
    source_id   INTEGER NOT NULL REFERENCES urls(id) ON DELETE CASCADE,
    target_key  TEXT,
    target_url  TEXT,
    raw_href    TEXT    NOT NULL,
    anchor_text TEXT,
    rel         TEXT,
    position    INTEGER NOT NULL DEFAULT 5,
    link_source INTEGER NOT NULL DEFAULT 0,
    nofollow    INTEGER NOT NULL DEFAULT 0,
    is_internal INTEGER NOT NULL DEFAULT 1
);
CREATE INDEX IF NOT EXISTS links_source ON links (source_id);

CREATE TABLE IF NOT EXISTS headers (
    url_id INTEGER NOT NULL REFERENCES urls(id) ON DELETE CASCADE,
    name   TEXT    NOT NULL,
    value  TEXT    NOT NULL
);
CREATE INDEX IF NOT EXISTS headers_url ON headers (url_id);

CREATE TABLE IF NOT EXISTS images (
    id               INTEGER PRIMARY KEY,
    url_id           INTEGER NOT NULL REFERENCES urls(id) ON DELETE CASCADE,
    raw_src          TEXT    NOT NULL,
    src              TEXT,
    src_key          TEXT,
    alt              TEXT,
    alt_missing      INTEGER NOT NULL DEFAULT 0,
    title            TEXT,
    declared_width   INTEGER,
    declared_height  INTEGER,
    loading_lazy     INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX IF NOT EXISTS images_url ON images (url_id);

CREATE TABLE IF NOT EXISTS headings (
    url_id   INTEGER NOT NULL REFERENCES urls(id) ON DELETE CASCADE,
    level    INTEGER NOT NULL,
    position INTEGER NOT NULL,
    text     TEXT    NOT NULL
);
CREATE INDEX IF NOT EXISTS headings_url ON headings (url_id);

CREATE TABLE IF NOT EXISTS hreflang (
    url_id   INTEGER NOT NULL REFERENCES urls(id) ON DELETE CASCADE,
    lang     TEXT    NOT NULL,
    raw_href TEXT    NOT NULL,
    href     TEXT,
    href_key TEXT
);
CREATE INDEX IF NOT EXISTS hreflang_url ON hreflang (url_id);

CREATE TABLE IF NOT EXISTS social_tags (
    url_id    INTEGER NOT NULL REFERENCES urls(id) ON DELETE CASCADE,
    namespace TEXT    NOT NULL,
    property  TEXT    NOT NULL,
    content   TEXT    NOT NULL
);
CREATE INDEX IF NOT EXISTS social_tags_url ON social_tags (url_id);

CREATE TABLE IF NOT EXISTS structured_data (
    url_id INTEGER NOT NULL REFERENCES urls(id) ON DELETE CASCADE,
    format TEXT    NOT NULL,
    types  TEXT    NOT NULL,
    valid  INTEGER NOT NULL DEFAULT 1,
    error  TEXT
);
CREATE INDEX IF NOT EXISTS structured_data_url ON structured_data (url_id);

CREATE TABLE IF NOT EXISTS redirects (
    url_id   INTEGER NOT NULL REFERENCES urls(id) ON DELETE CASCADE,
    hop      INTEGER NOT NULL,
    from_url TEXT    NOT NULL,
    to_url   TEXT    NOT NULL,
    status   INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS redirects_url ON redirects (url_id);

-- URLs declared in sitemaps. Kept even when not crawled, so the orphan-page
-- audit can compare "in the sitemap" against "linked internally".
CREATE TABLE IF NOT EXISTS sitemap_urls (
    url         TEXT NOT NULL,
    dedup_key   TEXT NOT NULL,
    sitemap_url TEXT NOT NULL,
    lastmod     TEXT,
    changefreq  TEXT,
    priority    REAL,
    PRIMARY KEY (dedup_key, sitemap_url)
) WITHOUT ROWID;

-- Crawl-level notices: an unreachable robots.txt, a sitemap that failed to
-- parse, a limit that was hit. Surfaced in the UI alongside the results.
CREATE TABLE IF NOT EXISTS crawl_log (
    id      INTEGER PRIMARY KEY,
    at      INTEGER NOT NULL,
    level   TEXT    NOT NULL,
    message TEXT    NOT NULL,
    url     TEXT
);
"#;

/// Tables holding audit results. Separate from [`SCHEMA`] so the audit pass can
/// recreate them from scratch on every run.
const ISSUE_SCHEMA: &str = r#"
-- One row per issue type that was found, with its count. The definition text
-- lives in the binary (`audit::catalog`), not here, so improving the wording of
-- a fix does not require a schema change — but the id, title and severity are
-- denormalised so an old database still reads sensibly in a newer build.
CREATE TABLE IF NOT EXISTS issues (
    issue_id    TEXT PRIMARY KEY,
    title       TEXT    NOT NULL,
    severity    INTEGER NOT NULL,
    url_count   INTEGER NOT NULL DEFAULT 0
) WITHOUT ROWID;

-- The affected URLs, one row per issue per URL.
CREATE TABLE IF NOT EXISTS issue_urls (
    issue_id TEXT    NOT NULL,
    url_id   INTEGER NOT NULL REFERENCES urls(id) ON DELETE CASCADE,
    -- Issue-specific context: the duplicate's partner URL, the measured value,
    -- the invalid hreflang code. Shown in the UI's issue table.
    detail   TEXT,
    PRIMARY KEY (issue_id, url_id)
) WITHOUT ROWID;

CREATE INDEX IF NOT EXISTS issue_urls_url ON issue_urls (url_id);
"#;

/// Forward migrations, applied in order to bring an older database up to
/// [`SCHEMA_VERSION`]. Each entry is `(target version, SQL)`.
///
/// Migrations must be idempotent-safe to *attempt*: they run inside a
/// transaction and the version is only bumped once they succeed.
const MIGRATIONS: &[(i64, &str)] = &[(
    2,
    "ALTER TABLE urls ADD COLUMN indexable INTEGER;
     ALTER TABLE urls ADD COLUMN indexability_reason TEXT;
     ALTER TABLE urls ADD COLUMN link_score REAL;",
)];

/// Indexes for reading and auditing, created once the crawl is done.
pub const ANALYSIS_INDEXES: &str = "
    CREATE INDEX IF NOT EXISTS urls_status      ON urls (status_code);
    CREATE INDEX IF NOT EXISTS urls_kind        ON urls (kind, state);
    CREATE INDEX IF NOT EXISTS urls_title       ON urls (title);
    CREATE INDEX IF NOT EXISTS urls_description ON urls (meta_description);
    CREATE INDEX IF NOT EXISTS urls_text_hash   ON urls (text_hash);
    CREATE INDEX IF NOT EXISTS urls_content_hash ON urls (content_hash);
    CREATE INDEX IF NOT EXISTS urls_depth       ON urls (depth);
    CREATE INDEX IF NOT EXISTS links_target     ON links (target_key);
    CREATE INDEX IF NOT EXISTS images_src       ON images (src_key);
    CREATE INDEX IF NOT EXISTS hreflang_href    ON hreflang (href_key);
    CREATE INDEX IF NOT EXISTS urls_indexable   ON urls (indexable);
    CREATE INDEX IF NOT EXISTS urls_link_score  ON urls (link_score);
";

/// Apply the pragmas every connection needs.
pub fn apply_pragmas(conn: &Connection) -> Result<()> {
    conn.execute_batch(PRAGMAS)?;
    Ok(())
}

/// Create the schema if absent, migrate it forward if it is older, or refuse it
/// if it comes from a newer build.
pub fn migrate(conn: &Connection, path: &str) -> Result<()> {
    match stored_version(conn) {
        // A fresh file: create everything at the current version.
        None => {
            conn.execute_batch(SCHEMA)?;
            conn.execute_batch(ISSUE_SCHEMA)?;
            set_version(conn, SCHEMA_VERSION)?;
            Ok(())
        }
        Some(version) if version == SCHEMA_VERSION => {
            // The issue tables are created lazily, so a database written before
            // the audit pass ever ran still gets them.
            conn.execute_batch(ISSUE_SCHEMA)?;
            Ok(())
        }
        Some(version) if version < SCHEMA_VERSION => {
            for (target, sql) in MIGRATIONS {
                if *target > version {
                    conn.execute_batch(sql)?;
                }
            }
            conn.execute_batch(ISSUE_SCHEMA)?;
            set_version(conn, SCHEMA_VERSION)?;
            Ok(())
        }
        // Newer than this build: we cannot know what changed, so refuse rather
        // than risk writing nonsense into someone's crawl.
        Some(found) => Err(EngineError::SchemaMismatch {
            path: path.to_string(),
            found,
            expected: SCHEMA_VERSION,
        }),
    }
}

/// The schema version recorded in the file, or `None` for a fresh database.
fn stored_version(conn: &Connection) -> Option<i64> {
    let has_meta: i64 = conn
        .query_row(
            "SELECT count(*) FROM sqlite_master WHERE type = 'table' AND name = 'meta'",
            [],
            |row| row.get(0),
        )
        .unwrap_or(0);
    if has_meta == 0 {
        return None;
    }
    conn.query_row(
        "SELECT value FROM meta WHERE key = 'schema_version'",
        [],
        |row| row.get::<_, String>(0),
    )
    .ok()
    .and_then(|v| v.parse().ok())
}

fn set_version(conn: &Connection, version: i64) -> Result<()> {
    conn.execute(
        "INSERT OR REPLACE INTO meta (key, value) VALUES ('schema_version', ?1)",
        [version.to_string()],
    )?;
    Ok(())
}

/// Drop and recreate the audit tables, so a re-run replaces its predecessor.
pub fn reset_issues(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "DROP TABLE IF EXISTS issue_urls;
         DROP TABLE IF EXISTS issues;",
    )?;
    conn.execute_batch(ISSUE_SCHEMA)?;
    Ok(())
}

/// Create the analysis indexes. Called when a crawl finishes.
pub fn create_analysis_indexes(conn: &Connection) -> Result<()> {
    conn.execute_batch(ANALYSIS_INDEXES)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn in_memory() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        // WAL is not available for :memory:, so skip the pragma batch here.
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        migrate(&conn, ":memory:").unwrap();
        conn
    }

    #[test]
    fn schema_creates_every_table() {
        let conn = in_memory();
        let mut stmt = conn
            .prepare("SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name")
            .unwrap();
        let tables: Vec<String> = stmt
            .query_map([], |r| r.get(0))
            .unwrap()
            .map(Result::unwrap)
            .collect();
        for expected in [
            "crawl_log",
            "headers",
            "headings",
            "hreflang",
            "images",
            "links",
            "meta",
            "redirects",
            "sitemap_urls",
            "social_tags",
            "structured_data",
            "urls",
        ] {
            assert!(
                tables.contains(&expected.to_string()),
                "missing table {expected}"
            );
        }
    }

    #[test]
    fn migrate_is_idempotent() {
        let conn = in_memory();
        migrate(&conn, ":memory:").unwrap();
        let version: String = conn
            .query_row(
                "SELECT value FROM meta WHERE key = 'schema_version'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(version, SCHEMA_VERSION.to_string());
    }

    #[test]
    fn a_future_schema_version_is_refused_rather_than_corrupted() {
        let conn = in_memory();
        conn.execute(
            "UPDATE meta SET value = '99' WHERE key = 'schema_version'",
            [],
        )
        .unwrap();
        match migrate(&conn, "/tmp/x.db") {
            Err(EngineError::SchemaMismatch {
                found, expected, ..
            }) => {
                assert_eq!((found, expected), (99, SCHEMA_VERSION));
            }
            other => panic!("expected a schema mismatch, got {other:?}"),
        }
    }

    #[test]
    fn the_dedup_key_is_unique() {
        let conn = in_memory();
        let insert = "INSERT INTO urls (url, dedup_key) VALUES (?1, ?2)";
        conn.execute(insert, ["https://a.test/x", "https://a.test/x"])
            .unwrap();
        let err = conn.execute(insert, ["https://a.test/x?utm=1", "https://a.test/x"]);
        assert!(err.is_err(), "a duplicate dedup_key must be rejected");
    }

    #[test]
    fn child_rows_are_deleted_with_their_url() {
        let conn = in_memory();
        conn.execute(
            "INSERT INTO urls (id, url, dedup_key) VALUES (1, 'u', 'k')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO headers (url_id, name, value) VALUES (1, 'a', 'b')",
            [],
        )
        .unwrap();
        conn.execute("DELETE FROM urls WHERE id = 1", []).unwrap();
        let count: i64 = conn
            .query_row("SELECT count(*) FROM headers", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 0, "ON DELETE CASCADE should have removed the header");
    }

    #[test]
    fn analysis_indexes_apply_cleanly() {
        let conn = in_memory();
        create_analysis_indexes(&conn).unwrap();
        create_analysis_indexes(&conn).unwrap(); // idempotent
        let count: i64 = conn
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE type = 'index' AND name LIKE 'urls_%'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert!(count >= 6);
    }
}
