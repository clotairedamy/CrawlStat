//! Turning a list of findings into a list of jobs.
//!
//! Every crawler on the market ends at the same place: a table of ten thousand
//! rows and forty-odd issue types, sorted by severity, with no answer to the
//! only question anybody actually has — *what should I fix first?* The work of
//! answering that is what an SEO consultant bills for, and it is done by hand,
//! in a spreadsheet, every time.
//!
//! This module does it from the crawl data instead.
//!
//! ## What the ranking is made of
//!
//! Three numbers, none of them invented:
//!
//! * **Severity.** An error is worth more than a warning, which is worth more
//!   than a notice. This is the only hand-set weight, and it is deliberately
//!   coarse — it decides the order of magnitude, not the order.
//!
//! * **Reach.** The share of the site's internal link value that sits on the
//!   affected pages, taken from the `link_score` the audit pass already
//!   computes. A missing title on the home page and a missing title on an
//!   orphan at depth seven are the same finding and emphatically not the same
//!   problem; nothing else in this category of tool draws that distinction.
//!
//! * **Leverage.** How many pages one change fixes, measured as pages divided
//!   by *distinct* findings. Thirty-four pages sharing one duplicated title is
//!   one job. Thirty-four pages with thirty-four different missing titles is
//!   thirty-four jobs. Sorting by severity alone buries the first behind the
//!   second, which is exactly backwards.
//!
//! ## What it deliberately is not
//!
//! It is not a traffic estimate. CrawlStat has not seen your analytics, and a
//! number dressed up as "+1,200 visits/month" would be a guess wearing a
//! lab coat. Impact here is a *relative* score across this crawl: it says what
//! to do first, not what it will earn.

use std::collections::HashMap;

use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::audit::{catalog, Severity};
use crate::error::Result;

/// How much work a job is, inferred from the shape of its findings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Effort {
    /// Many pages, very few distinct findings: almost certainly one template,
    /// one include, or one setting. The best work in the list.
    Template,
    /// Several pages per distinct finding — a section, a pattern, a bad batch.
    Bulk,
    /// One fix per page. Honest work, usually last.
    PerPage,
}

impl Effort {
    pub fn as_str(self) -> &'static str {
        match self {
            Effort::Template => "template",
            Effort::Bulk => "bulk",
            Effort::PerPage => "per_page",
        }
    }
}

/// One of the worst-affected pages, shown so the reader can go and look.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlanExample {
    pub url: String,
    pub detail: Option<String>,
    pub depth: u32,
}

/// One job: a single issue, scored and described.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlanItem {
    pub issue_id: String,
    pub title: String,
    pub severity: Severity,
    pub how_to_fix: String,
    pub docs_slug: String,

    /// Pages carrying this issue.
    pub pages: u64,
    /// Distinct findings behind those pages — the number of actual changes.
    pub distinct_fixes: u64,
    /// `pages / distinct_fixes`: how many pages one change improves.
    pub leverage: f64,
    /// Share of the site's internal link value on the affected pages, 0..1.
    pub reach: f64,
    /// 0..100, relative to everything else in this crawl.
    pub impact: f64,
    pub effort: Effort,
    pub examples: Vec<PlanExample>,
}

/// The ranked plan for a crawl.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActionPlan {
    /// Jobs, best first.
    pub items: Vec<PlanItem>,
    /// Pages with at least one issue.
    pub affected_pages: u64,
    pub total_pages: u64,
    /// Share of link value touched by at least one error-severity issue.
    pub value_at_risk: f64,
}

/// Severity's contribution. Coarse on purpose: it sets the band, and reach and
/// leverage order things inside it.
fn severity_weight(severity: Severity) -> f64 {
    match severity {
        Severity::Error => 1.0,
        Severity::Warning => 0.55,
        Severity::Notice => 0.2,
    }
}

/// Compress a share into something comparable.
///
/// Raw link-value shares are long-tailed: one issue covers 40% of a site and
/// the next covers 0.8%. Linearly, the second scores ~zero and sorts below
/// every notice. The square root keeps the big one first while leaving the
/// small one a score a human would recognise as "small but real".
fn compress(share: f64) -> f64 {
    share.clamp(0.0, 1.0).sqrt()
}

/// Leverage's contribution, capped.
///
/// One change fixing 30 pages is far better than one fixing 3. One fixing 3,000
/// is not a hundred times better than one fixing 30 — past a point it is simply
/// "a template fix", and letting it run away would push one enormous but
/// trivial issue above everything that is actually broken.
fn leverage_weight(leverage: f64) -> f64 {
    1.0 + (leverage.max(1.0)).log10().min(1.5)
}

/// Score a single job, 0..100 before normalisation.
pub fn score(severity: Severity, reach: f64, leverage: f64) -> f64 {
    severity_weight(severity) * compress(reach) * leverage_weight(leverage) * 100.0
}

/// The most distinct findings a job can have and still be one template change.
///
/// Two conditions, not one. The ratio alone is not enough: a real crawl threw
/// up 492 pages whose titles were too long across 23 distinct lengths, which
/// is a ratio of 21 and looks like a template fix, but 23 different numbers
/// are 23 different pages that each need a new title. A handful of causes
/// behind hundreds of pages is unambiguous; two dozen is not.
const TEMPLATE_MAX_FIXES: u64 = 10;

/// Classify how much work a job is.
///
/// The signal is the ratio of distinct findings to affected pages, which comes
/// straight out of the data: `issue_urls.detail` holds the duplicate's partner,
/// the broken target, the measured value. When many pages report the *same*
/// detail, they usually share a cause.
///
/// ## Where this is weaker than it looks
///
/// It reads `detail` as a *cause*, and for most audits it is one — a duplicate's
/// partner URL, a broken link's target, the offending template. For the handful
/// where `detail` is a *measurement* — a character count, a response time — two
/// pages sharing a value share nothing, so the count understates the work.
/// [`TEMPLATE_MAX_FIXES`] keeps that case out of the top category rather than
/// pretending the signal is cleaner than it is.
pub fn classify_effort(pages: u64, distinct_fixes: u64) -> Effort {
    if pages < 2 {
        return Effort::PerPage;
    }
    let distinct = distinct_fixes.max(1);
    let leverage = pages as f64 / distinct as f64;
    if leverage >= 3.0 && distinct <= TEMPLATE_MAX_FIXES {
        Effort::Template
    } else if leverage >= 1.5 {
        Effort::Bulk
    } else {
        Effort::PerPage
    }
}

/// How many example pages each job carries.
const EXAMPLES_PER_ITEM: usize = 5;

/// Build the plan for a crawl.
pub fn build(conn: &Connection) -> Result<ActionPlan> {
    // Link value is the weight behind "reach". A crawl audited before link
    // scores existed, or one where every score is null, falls back to counting
    // pages — a worse measure, but never a crash and never a silent zero.
    let (total_value, has_scores): (f64, bool) = {
        let total: Option<f64> = conn.query_row(
            "SELECT sum(link_score) FROM urls WHERE is_internal = 1 AND link_score IS NOT NULL",
            [],
            |row| row.get(0),
        )?;
        match total {
            Some(value) if value > 0.0 => (value, true),
            _ => {
                let pages: i64 = conn.query_row(
                    "SELECT count(*) FROM urls WHERE is_internal = 1 AND status_code IS NOT NULL",
                    [],
                    |row| row.get(0),
                )?;
                (pages.max(1) as f64, false)
            }
        }
    };

    let total_pages: u64 = conn.query_row(
        "SELECT count(*) FROM urls
         WHERE is_internal = 1 AND kind = 0 AND status_code IS NOT NULL",
        [],
        |row| row.get::<_, i64>(0).map(|n| n.max(0) as u64),
    )?;

    let affected_pages: u64 = conn.query_row(
        "SELECT count(DISTINCT url_id) FROM issue_urls",
        [],
        |row| row.get::<_, i64>(0).map(|n| n.max(0) as u64),
    )?;

    // One pass over the join, aggregated in SQL: pages, distinct details and
    // summed link value per issue.
    let value_expression = if has_scores {
        "coalesce(u.link_score, 0)"
    } else {
        "1.0"
    };
    let mut stmt = conn.prepare(&format!(
        "SELECT i.issue_id,
                count(DISTINCT i.url_id)  AS pages,
                count(DISTINCT coalesce(i.detail, cast(i.url_id AS TEXT))) AS distinct_fixes,
                sum({value_expression}) AS value
         FROM issue_urls i
         JOIN urls u ON u.id = i.url_id
         GROUP BY i.issue_id"
    ))?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, i64>(1)?.max(0) as u64,
            row.get::<_, i64>(2)?.max(0) as u64,
            row.get::<_, Option<f64>>(3)?.unwrap_or(0.0),
        ))
    })?;

    let mut items = Vec::new();
    let mut value_at_risk = 0.0;

    for row in rows {
        let (issue_id, pages, distinct_fixes, value) = row?;
        let Some(audit) = catalog::find(&issue_id) else {
            // An issue written by a newer build than this one. Skipping it is
            // right: the plan would have no idea what it means or how to fix
            // it, and a job with no fix text is worse than no job.
            continue;
        };

        let reach = (value / total_value).clamp(0.0, 1.0);
        let leverage = pages as f64 / distinct_fixes.max(1) as f64;

        if audit.severity == Severity::Error {
            value_at_risk += reach;
        }

        items.push(PlanItem {
            issue_id: issue_id.clone(),
            title: audit.title.to_string(),
            severity: audit.severity,
            how_to_fix: audit.how_to_fix.to_string(),
            docs_slug: audit.docs_slug.to_string(),
            pages,
            distinct_fixes,
            leverage,
            reach,
            impact: score(audit.severity, reach, leverage),
            effort: classify_effort(pages, distinct_fixes),
            examples: Vec::new(),
        });
    }

    // Normalise so the best job in *this* crawl reads as 100. The number is
    // only ever meaningful against the others beside it, and saying "73" when
    // the top of the list is 9 would be a lie about an absolute scale that
    // does not exist.
    if let Some(best) = items
        .iter()
        .map(|item| item.impact)
        .fold(None::<f64>, |acc, value| {
            Some(acc.map_or(value, |current: f64| current.max(value)))
        })
        .filter(|best| *best > 0.0)
    {
        for item in &mut items {
            item.impact = (item.impact / best * 100.0).clamp(0.0, 100.0);
        }
    }

    items.sort_by(|a, b| {
        b.impact
            .partial_cmp(&a.impact)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(b.pages.cmp(&a.pages))
            .then(a.issue_id.cmp(&b.issue_id))
    });

    attach_examples(conn, &mut items, has_scores)?;

    Ok(ActionPlan {
        items,
        affected_pages,
        total_pages,
        // An aborted crawl can hold issue rows while holding no pages, and the
        // page-count fallback for `total_value` then makes a single orphaned
        // row look like the entire site is at risk. With nothing crawled there
        // is nothing to be at risk.
        value_at_risk: if total_pages == 0 {
            0.0
        } else {
            value_at_risk.clamp(0.0, 1.0)
        },
    })
}

/// Fill in each job's example pages, worst first.
///
/// One query for all of them rather than one per job: a crawl with forty issue
/// types would otherwise make forty round trips for twenty rows each.
fn attach_examples(conn: &Connection, items: &mut [PlanItem], has_scores: bool) -> Result<()> {
    if items.is_empty() {
        return Ok(());
    }
    let order = if has_scores {
        "u.link_score DESC"
    } else {
        "u.depth ASC"
    };
    let mut stmt = conn.prepare(&format!(
        "SELECT issue_id, url, detail, depth FROM (
            SELECT i.issue_id, u.url, i.detail, u.depth,
                   row_number() OVER (
                       PARTITION BY i.issue_id ORDER BY {order}, u.id
                   ) AS rank
            FROM issue_urls i
            JOIN urls u ON u.id = i.url_id
         ) WHERE rank <= ?1"
    ))?;
    let rows = stmt.query_map([EXAMPLES_PER_ITEM as i64], |row| {
        Ok((
            row.get::<_, String>(0)?,
            PlanExample {
                url: row.get(1)?,
                detail: row.get(2)?,
                depth: row.get::<_, i64>(3)?.max(0) as u32,
            },
        ))
    })?;

    let mut by_issue: HashMap<String, Vec<PlanExample>> = HashMap::new();
    for row in rows {
        let (issue_id, example) = row?;
        by_issue.entry(issue_id).or_default().push(example);
    }
    for item in items.iter_mut() {
        if let Some(examples) = by_issue.remove(&item.issue_id) {
            item.examples = examples;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::Store;

    /// A crawl with three pages and two issues on them.
    ///
    /// Issue rows are written directly rather than by running an audit pass:
    /// the audits have their own tests, and what this needs is a known shape —
    /// one issue that is a single template fix across every page, and one that
    /// is a separate fix per page.
    fn store_with_issues() -> Store {
        let store = Store::open_in_memory().unwrap();
        let conn = store.connection();
        conn.execute_batch(
            "INSERT INTO urls (id, url, dedup_key, depth, state, kind, is_internal,
                               status_code, link_score)
             VALUES (1, 'https://a.test/',     'a.test/',     0, 2, 0, 1, 200, 0.60),
                    (2, 'https://a.test/one',  'a.test/one',  1, 2, 0, 1, 200, 0.30),
                    (3, 'https://a.test/two',  'a.test/two',  3, 2, 0, 1, 200, 0.10);

             INSERT INTO issues (issue_id, title, severity, url_count) VALUES
                ('title_missing',  'Missing title',  0, 3),
                ('title_too_long', 'Title too long', 1, 2);

             -- Every page reports the same detail: one template, one fix.
             INSERT INTO issue_urls (issue_id, url_id, detail) VALUES
                ('title_missing', 1, 'layout.html'),
                ('title_missing', 2, 'layout.html'),
                ('title_missing', 3, 'layout.html'),
             -- Different details: a separate fix each.
                ('title_too_long', 1, '84 chars'),
                ('title_too_long', 2, '91 chars');",
        )
        .unwrap();
        store
    }

    #[test]
    fn the_plan_counts_distinct_fixes_not_affected_pages() {
        // The whole argument for this screen: 968 pages sharing three causes
        // is three jobs, and a list sorted by page count says otherwise.
        let store = store_with_issues();
        let plan = build(store.connection()).unwrap();

        let missing = plan
            .items
            .iter()
            .find(|item| item.issue_id == "title_missing")
            .expect("the issue is in the plan");
        assert_eq!(missing.pages, 3);
        assert_eq!(missing.distinct_fixes, 1);
        assert_eq!(missing.effort, Effort::Template);
    }

    #[test]
    fn an_issue_with_a_different_cause_per_page_is_not_a_template_fix() {
        let store = store_with_issues();
        let plan = build(store.connection()).unwrap();
        let long = plan
            .items
            .iter()
            .find(|item| item.issue_id == "title_too_long")
            .unwrap();
        assert_eq!(long.distinct_fixes, 2);
        assert_eq!(long.effort, Effort::PerPage);
    }

    #[test]
    fn reach_is_weighted_by_link_value_not_by_page_count() {
        let store = store_with_issues();
        let plan = build(store.connection()).unwrap();
        let missing = plan
            .items
            .iter()
            .find(|item| item.issue_id == "title_missing")
            .unwrap();
        // All three pages, so all of the site's link value.
        assert!((missing.reach - 1.0).abs() < 1e-6, "reach was {}", missing.reach);

        let long = plan
            .items
            .iter()
            .find(|item| item.issue_id == "title_too_long")
            .unwrap();
        // 0.60 + 0.30 of 1.00.
        assert!((long.reach - 0.9).abs() < 1e-6, "reach was {}", long.reach);
    }

    #[test]
    fn the_best_job_scores_one_hundred_and_the_list_is_sorted() {
        let plan = build(store_with_issues().connection()).unwrap();
        assert!(!plan.items.is_empty());
        assert!((plan.items[0].impact - 100.0).abs() < 1e-6);
        for pair in plan.items.windows(2) {
            assert!(pair[0].impact >= pair[1].impact, "the plan is not sorted");
        }
    }

    #[test]
    fn examples_lead_with_the_most_valuable_page() {
        let plan = build(store_with_issues().connection()).unwrap();
        let missing = plan
            .items
            .iter()
            .find(|item| item.issue_id == "title_missing")
            .unwrap();
        assert_eq!(missing.examples.len(), 3);
        assert_eq!(missing.examples[0].url, "https://a.test/");
    }

    #[test]
    fn a_crawl_with_no_pages_has_nothing_at_risk() {
        // An aborted crawl left one issue row and no pages, and the fallback
        // weighting reported "100% of link value at risk" for an empty file.
        let store = Store::open_in_memory().unwrap();
        store
            .connection()
            .execute_batch(
                "INSERT INTO issues (issue_id, title, severity, url_count)
                 VALUES ('title_missing', 'Missing title', 0, 1);
                 INSERT INTO urls (id, url, dedup_key, state, kind, is_internal)
                 VALUES (1, 'https://a.test/', 'a.test/', 0, 0, 1);
                 INSERT INTO issue_urls (issue_id, url_id) VALUES ('title_missing', 1);",
            )
            .unwrap();
        let plan = build(store.connection()).unwrap();
        assert_eq!(plan.total_pages, 0, "nothing was actually fetched");
        assert_eq!(plan.value_at_risk, 0.0);
    }

    #[test]
    fn a_crawl_with_no_issues_produces_an_empty_plan_rather_than_an_error() {
        let store = Store::open_in_memory().unwrap();
        let plan = build(store.connection()).unwrap();
        assert!(plan.items.is_empty());
        assert_eq!(plan.affected_pages, 0);
        assert_eq!(plan.value_at_risk, 0.0);
    }

    #[test]
    fn an_error_outranks_a_warning_with_the_same_reach() {
        let error = score(Severity::Error, 0.3, 1.0);
        let warning = score(Severity::Warning, 0.3, 1.0);
        assert!(error > warning);
    }

    #[test]
    fn reach_beats_severity_when_it_is_lopsided_enough() {
        // A warning across a third of the site's link value is worth more than
        // an error on one forgotten page. This is the whole point of the
        // ranking, and the thing a severity-sorted list gets wrong.
        let big_warning = score(Severity::Warning, 0.35, 1.0);
        let tiny_error = score(Severity::Error, 0.002, 1.0);
        assert!(big_warning > tiny_error);
    }

    #[test]
    fn a_small_reach_still_scores_something_a_human_would_recognise() {
        // Linearly this would round to nothing and sort below every notice.
        let small = score(Severity::Error, 0.01, 1.0);
        assert!(small > 5.0, "scored {small}, which reads as zero");
    }

    #[test]
    fn leverage_lifts_a_template_fix_above_the_same_issue_spread_thin() {
        let template = score(Severity::Warning, 0.2, 30.0);
        let scattered = score(Severity::Warning, 0.2, 1.0);
        assert!(template > scattered);
    }

    #[test]
    fn leverage_is_capped_so_one_huge_trivial_issue_cannot_take_the_top() {
        // 30 pages per fix and 30,000 both mean "it is a template".
        let large = score(Severity::Notice, 0.1, 30.0);
        let absurd = score(Severity::Notice, 0.1, 30_000.0);
        assert!(absurd / large < 2.0, "{absurd} vs {large}");
    }

    #[test]
    fn effort_reads_the_ratio_of_fixes_to_pages() {
        // 34 pages, one duplicated title: one job.
        assert_eq!(classify_effort(34, 1), Effort::Template);
        // 34 pages, 12 distinct problems: a section to work through.
        assert_eq!(classify_effort(34, 12), Effort::Bulk);
        // 34 pages, 34 different missing titles: 34 jobs.
        assert_eq!(classify_effort(34, 34), Effort::PerPage);
    }

    #[test]
    fn a_good_ratio_over_many_distinct_findings_is_not_a_template_fix() {
        // Taken from a real crawl: 492 over-long titles across 23 distinct
        // lengths. The ratio is 21, which looks like one template change, but
        // 23 different numbers mean 23 different pages to rewrite.
        assert_eq!(classify_effort(492, 23), Effort::Bulk);
        // Whereas 968 pages with three distinct duplicated ids really is three
        // edits — also from the same crawl.
        assert_eq!(classify_effort(968, 3), Effort::Template);
    }

    #[test]
    fn a_single_page_is_never_a_template_fix() {
        assert_eq!(classify_effort(1, 1), Effort::PerPage);
    }

    #[test]
    fn effort_survives_a_zero_fix_count() {
        // Defensive: distinct_fixes comes from a COUNT and should never be
        // zero while pages is not, but dividing by it must not panic.
        assert_eq!(classify_effort(10, 0), Effort::Template);
    }

    #[test]
    fn scores_never_leave_the_scale() {
        for reach in [0.0, 0.5, 1.0, 2.0] {
            for leverage in [0.0, 1.0, 1e9] {
                let value = score(Severity::Error, reach, leverage);
                assert!(value.is_finite(), "reach {reach} leverage {leverage}");
                assert!(value >= 0.0);
            }
        }
    }
}
