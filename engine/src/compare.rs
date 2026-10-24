//! Did the fix work?
//!
//! The rest of CrawlStat answers "what is wrong" and "what should I do first".
//! Neither of those is worth much on its own, because the moment somebody acts
//! on the advice they need to know whether acting helped — and until this
//! module existed, every crawl was an island and there was no reason to run
//! the tool a second time.
//!
//! Comparing two crawls of the same site turns a report into a loop: crawl,
//! fix, crawl again, see the impact score fall. That is the difference between
//! a thing you run once out of curiosity and a thing you run every release.
//!
//! ## Matching pages across crawls
//!
//! Pages are matched on `dedup_key`, not on `id` or on the raw URL. Ids are
//! per-file and meaningless across crawls; raw URLs differ by tracking
//! parameters, trailing slashes and case that the normalisation policy already
//! knows how to ignore. The dedup key is what the crawler itself considers
//! "the same page", so using anything else here would report cosmetic URL
//! changes as pages appearing and disappearing.
//!
//! ## What "fixed" means, honestly
//!
//! An issue is *fixed* when a page that had it no longer does, and the page is
//! still in the crawl. A page that simply stopped being crawled has not been
//! fixed — it has gone, which is a different thing and is reported separately.
//! Getting this wrong would let anyone post a perfect score by deleting
//! half their site.

use std::collections::{HashMap, HashSet};

use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::audit::catalog;
use crate::error::Result;
use crate::plan;

/// How one issue changed between two crawls.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IssueDelta {
    pub issue_id: String,
    pub title: String,
    /// Pages with this issue in the earlier crawl.
    pub before: u64,
    /// Pages with it now.
    pub after: u64,
    /// Pages that had it and no longer do, and are still crawled.
    pub fixed: u64,
    /// Pages that did not have it and now do.
    pub introduced: u64,
    /// Impact score before and after, from [`crate::plan`].
    pub impact_before: f64,
    pub impact_after: f64,
}

impl IssueDelta {
    /// Net change in affected pages. Negative is an improvement.
    pub fn net(&self) -> i64 {
        self.after as i64 - self.before as i64
    }

    pub fn improved(&self) -> bool {
        self.net() < 0
    }
}

/// A page whose health changed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PageDelta {
    pub url: String,
    pub issues_before: u64,
    pub issues_after: u64,
    pub status_before: Option<u16>,
    pub status_after: Option<u16>,
}

impl PageDelta {
    /// A page that started returning an error, or stopped returning a good
    /// status. The single most urgent thing a comparison can surface.
    pub fn broke(&self) -> bool {
        let was_ok = self.status_before.is_some_and(|code| code < 400);
        let is_bad = self.status_after.is_none_or(|code| code >= 400);
        was_ok && is_bad
    }
}

/// The whole comparison.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Comparison {
    pub pages_before: u64,
    pub pages_after: u64,
    /// Pages in the new crawl that were not in the old one.
    pub pages_added: Vec<String>,
    /// Pages that were there and are not any more. Not "fixed" — gone.
    pub pages_removed: Vec<String>,
    /// Pages that were fine and now are not.
    pub pages_broken: Vec<PageDelta>,
    /// Every issue that appears in either crawl, biggest improvement first.
    pub issues: Vec<IssueDelta>,
    /// Total pages carrying at least one issue, before and after.
    pub affected_before: u64,
    pub affected_after: u64,
    /// Share of link value touched by an error, before and after.
    pub value_at_risk_before: f64,
    pub value_at_risk_after: f64,
}

impl Comparison {
    pub fn total_fixed(&self) -> u64 {
        self.issues.iter().map(|issue| issue.fixed).sum()
    }

    pub fn total_introduced(&self) -> u64 {
        self.issues.iter().map(|issue| issue.introduced).sum()
    }

    /// Issues that got better, worst-first by how much they improved.
    pub fn improvements(&self) -> Vec<&IssueDelta> {
        let mut list: Vec<&IssueDelta> =
            self.issues.iter().filter(|issue| issue.improved()).collect();
        list.sort_by_key(|issue| issue.net());
        list
    }

    /// Issues that got worse, worst first.
    pub fn regressions(&self) -> Vec<&IssueDelta> {
        let mut list: Vec<&IssueDelta> =
            self.issues.iter().filter(|issue| issue.net() > 0).collect();
        list.sort_by_key(|issue| -issue.net());
        list
    }
}

/// `dedup_key` → the page's URL, status and issue ids.
type Snapshot = HashMap<String, PageState>;

#[derive(Debug, Clone)]
struct PageState {
    url: String,
    status: Option<u16>,
    issues: HashSet<String>,
}

fn snapshot(conn: &Connection) -> Result<Snapshot> {
    let mut pages: Snapshot = HashMap::new();

    {
        let mut stmt = conn.prepare(
            "SELECT dedup_key, url, coalesce(final_status_code, status_code)
             FROM urls
             WHERE is_internal = 1 AND kind = 0 AND status_code IS NOT NULL",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<i64>>(2)?,
            ))
        })?;
        for row in rows {
            let (key, url, status) = row?;
            pages.insert(
                key,
                PageState {
                    url,
                    status: status.map(|code| code as u16),
                    issues: HashSet::new(),
                },
            );
        }
    }

    {
        let mut stmt = conn.prepare(
            "SELECT u.dedup_key, i.issue_id
             FROM issue_urls i JOIN urls u ON u.id = i.url_id",
        )?;
        let rows = stmt
            .query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))?;
        for row in rows {
            let (key, issue) = row?;
            if let Some(page) = pages.get_mut(&key) {
                page.issues.insert(issue);
            }
        }
    }

    Ok(pages)
}

/// Impact score per issue id, from the ranked plan.
fn impacts(conn: &Connection) -> Result<HashMap<String, f64>> {
    Ok(plan::build(conn)?
        .items
        .into_iter()
        .map(|item| (item.issue_id, item.impact))
        .collect())
}

/// How many of the pages listed are worth naming in a report.
///
/// A comparison that prints four thousand new URLs is not a comparison, it is
/// a diff nobody reads. The counts are exact; the lists are a sample.
const SAMPLE: usize = 50;

/// Compare an earlier crawl with a later one.
pub fn compare(before: &Connection, after: &Connection) -> Result<Comparison> {
    let old = snapshot(before)?;
    let new = snapshot(after)?;
    let impact_before = impacts(before)?;
    let impact_after = impacts(after)?;

    let old_keys: HashSet<&String> = old.keys().collect();
    let new_keys: HashSet<&String> = new.keys().collect();

    let mut pages_added: Vec<String> = new_keys
        .difference(&old_keys)
        .filter_map(|key| new.get(*key).map(|page| page.url.clone()))
        .collect();
    let mut pages_removed: Vec<String> = old_keys
        .difference(&new_keys)
        .filter_map(|key| old.get(*key).map(|page| page.url.clone()))
        .collect();
    pages_added.sort();
    pages_removed.sort();

    let mut pages_broken = Vec::new();
    for key in old_keys.intersection(&new_keys) {
        let (was, now) = (&old[*key], &new[*key]);
        let delta = PageDelta {
            url: now.url.clone(),
            issues_before: was.issues.len() as u64,
            issues_after: now.issues.len() as u64,
            status_before: was.status,
            status_after: now.status,
        };
        if delta.broke() {
            pages_broken.push(delta);
        }
    }
    pages_broken.sort_by(|a, b| a.url.cmp(&b.url));

    // Every issue id seen in either crawl.
    let mut issue_ids: HashSet<&str> = HashSet::new();
    for page in old.values().chain(new.values()) {
        issue_ids.extend(page.issues.iter().map(String::as_str));
    }

    let mut issues: Vec<IssueDelta> = issue_ids
        .into_iter()
        .filter_map(|issue_id| {
            let audit = catalog::find(issue_id)?;

            let before_pages: HashSet<&String> = old
                .iter()
                .filter(|(_, page)| page.issues.contains(issue_id))
                .map(|(key, _)| key)
                .collect();
            let after_pages: HashSet<&String> = new
                .iter()
                .filter(|(_, page)| page.issues.contains(issue_id))
                .map(|(key, _)| key)
                .collect();

            // Fixed: had it, does not now, and is *still here*. A page that
            // vanished was not fixed.
            let fixed = before_pages
                .iter()
                .filter(|key| new.contains_key(**key) && !after_pages.contains(**key))
                .count() as u64;
            let introduced = after_pages
                .iter()
                .filter(|key| !before_pages.contains(**key))
                .count() as u64;

            Some(IssueDelta {
                issue_id: issue_id.to_string(),
                title: audit.title.to_string(),
                before: before_pages.len() as u64,
                after: after_pages.len() as u64,
                fixed,
                introduced,
                impact_before: impact_before.get(issue_id).copied().unwrap_or(0.0),
                impact_after: impact_after.get(issue_id).copied().unwrap_or(0.0),
            })
        })
        .collect();

    // Improvements first, then regressions, then the unchanged.
    issues.sort_by(|a, b| {
        a.net()
            .cmp(&b.net())
            .then(b.before.cmp(&a.before))
            .then(a.issue_id.cmp(&b.issue_id))
    });

    let count_affected = |pages: &Snapshot| {
        pages.values().filter(|page| !page.issues.is_empty()).count() as u64
    };

    let at_risk = |conn: &Connection| -> Result<f64> {
        Ok(plan::build(conn)?.value_at_risk)
    };

    Ok(Comparison {
        pages_before: old.len() as u64,
        pages_after: new.len() as u64,
        affected_before: count_affected(&old),
        affected_after: count_affected(&new),
        value_at_risk_before: at_risk(before)?,
        value_at_risk_after: at_risk(after)?,
        pages_added: pages_added.into_iter().take(SAMPLE).collect(),
        pages_removed: pages_removed.into_iter().take(SAMPLE).collect(),
        pages_broken,
        issues,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::Store;

    /// A crawl with the given pages, each carrying the listed issue ids.
    fn crawl(pages: &[(&str, Option<u16>, &[&str])]) -> Store {
        let store = Store::open_in_memory().unwrap();
        let conn = store.connection();
        conn.execute_batch(
            "INSERT INTO issues (issue_id, title, severity, url_count) VALUES
                ('title_missing',  'Missing title',  0, 0),
                ('title_too_long', 'Title too long', 1, 0);",
        )
        .unwrap();

        for (index, (url, status, issues)) in pages.iter().enumerate() {
            let id = index as i64 + 1;
            // The dedup key is the URL without its scheme, which is close
            // enough to the real normaliser for a test about matching.
            let key = url.replace("https://", "");
            conn.execute(
                "INSERT INTO urls (id, url, dedup_key, depth, state, kind, is_internal,
                                   status_code, link_score)
                 VALUES (?1, ?2, ?3, 1, 2, 0, 1, ?4, 0.5)",
                rusqlite::params![id, url, key, status.map(i64::from)],
            )
            .unwrap();
            for issue in *issues {
                conn.execute(
                    "INSERT INTO issue_urls (issue_id, url_id, detail) VALUES (?1, ?2, 'x')",
                    rusqlite::params![issue, id],
                )
                .unwrap();
            }
        }
        store
    }

    #[test]
    fn an_issue_cleared_from_a_page_counts_as_fixed() {
        let before = crawl(&[("https://a.test/one", Some(200), &["title_missing"])]);
        let after = crawl(&[("https://a.test/one", Some(200), &[])]);
        let result = compare(before.connection(), after.connection()).unwrap();

        let delta = result.issues.iter().find(|i| i.issue_id == "title_missing").unwrap();
        assert_eq!(delta.before, 1);
        assert_eq!(delta.after, 0);
        assert_eq!(delta.fixed, 1);
        assert_eq!(result.total_fixed(), 1);
    }

    #[test]
    fn deleting_a_page_is_not_fixing_it() {
        // Otherwise the way to a perfect score is to delete the site, and a
        // tool that rewards that is worse than no tool.
        let before = crawl(&[
            ("https://a.test/one", Some(200), &["title_missing"]),
            ("https://a.test/two", Some(200), &[]),
        ]);
        let after = crawl(&[("https://a.test/two", Some(200), &[])]);
        let result = compare(before.connection(), after.connection()).unwrap();

        let delta = result.issues.iter().find(|i| i.issue_id == "title_missing").unwrap();
        assert_eq!(delta.fixed, 0, "the page went away, it was not fixed");
        assert_eq!(result.pages_removed, vec!["https://a.test/one"]);
    }

    #[test]
    fn a_new_occurrence_is_reported_as_introduced() {
        let before = crawl(&[("https://a.test/one", Some(200), &[])]);
        let after = crawl(&[("https://a.test/one", Some(200), &["title_too_long"])]);
        let result = compare(before.connection(), after.connection()).unwrap();

        let delta = result.issues.iter().find(|i| i.issue_id == "title_too_long").unwrap();
        assert_eq!(delta.introduced, 1);
        assert_eq!(result.regressions().len(), 1);
        assert!(result.improvements().is_empty());
    }

    #[test]
    fn a_page_that_started_failing_is_surfaced() {
        let before = crawl(&[("https://a.test/one", Some(200), &[])]);
        let after = crawl(&[("https://a.test/one", Some(500), &[])]);
        let result = compare(before.connection(), after.connection()).unwrap();

        assert_eq!(result.pages_broken.len(), 1);
        assert_eq!(result.pages_broken[0].status_after, Some(500));
    }

    #[test]
    fn a_page_that_was_already_failing_is_not_a_new_break() {
        let before = crawl(&[("https://a.test/one", Some(404), &[])]);
        let after = crawl(&[("https://a.test/one", Some(404), &[])]);
        let result = compare(before.connection(), after.connection()).unwrap();
        assert!(result.pages_broken.is_empty());
    }

    #[test]
    fn pages_are_matched_on_the_dedup_key_not_the_id() {
        // The same page, different row id in each file. Matching on id would
        // report one removal and one addition instead of one unchanged page.
        let before = crawl(&[
            ("https://a.test/first", Some(200), &[]),
            ("https://a.test/shared", Some(200), &["title_missing"]),
        ]);
        let after = crawl(&[("https://a.test/shared", Some(200), &[])]);
        let result = compare(before.connection(), after.connection()).unwrap();

        assert!(result.pages_added.is_empty());
        assert_eq!(result.pages_removed, vec!["https://a.test/first"]);
        let delta = result.issues.iter().find(|i| i.issue_id == "title_missing").unwrap();
        assert_eq!(delta.fixed, 1);
    }

    #[test]
    fn improvements_sort_before_regressions() {
        let before = crawl(&[
            ("https://a.test/one", Some(200), &["title_missing"]),
            ("https://a.test/two", Some(200), &["title_missing"]),
        ]);
        let after = crawl(&[
            ("https://a.test/one", Some(200), &[]),
            ("https://a.test/two", Some(200), &["title_too_long"]),
        ]);
        let result = compare(before.connection(), after.connection()).unwrap();
        assert_eq!(result.issues[0].issue_id, "title_missing", "the win comes first");
        assert!(result.issues[0].net() < 0);
        assert!(result.issues.last().unwrap().net() > 0);
    }

    #[test]
    fn comparing_a_crawl_with_itself_shows_no_change() {
        let store = crawl(&[("https://a.test/one", Some(200), &["title_missing"])]);
        let result = compare(store.connection(), store.connection()).unwrap();
        assert_eq!(result.total_fixed(), 0);
        assert_eq!(result.total_introduced(), 0);
        assert!(result.pages_added.is_empty());
        assert!(result.pages_removed.is_empty());
        assert!(result.pages_broken.is_empty());
    }

    #[test]
    fn two_empty_crawls_compare_without_panicking() {
        let before = Store::open_in_memory().unwrap();
        let after = Store::open_in_memory().unwrap();
        let result = compare(before.connection(), after.connection()).unwrap();
        assert_eq!(result.pages_before, 0);
        assert!(result.issues.is_empty());
        assert!(result.value_at_risk_before.is_finite());
    }
}
