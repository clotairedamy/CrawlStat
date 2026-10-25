//! Before and after: what a retest changed.
//!
//! Checks are matched by id. A check that could not run on either side is
//! "not comparable" — it is neither fixed nor still failing, because nobody
//! knows.

use serde::{Deserialize, Serialize};

use super::{Audit, Status};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ComparisonRow {
    pub id: String,
    pub title: String,
    pub before: Option<Status>,
    pub after: Option<Status>,
    /// fixed, regressed, still_failing, unchanged, new, gone, not_comparable
    pub change: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Comparison {
    pub before_id: i64,
    pub after_id: i64,
    pub rows: Vec<ComparisonRow>,
    pub fixed: u32,
    pub regressed: u32,
    pub still_failing: u32,
    /// Of the checks that had a finding before and ran again, the share that
    /// now pass. `None` when nothing could be retested.
    pub retest_pass_rate: Option<f32>,
}

fn bad(status: Status) -> bool {
    matches!(status, Status::Fail | Status::NeedsReview)
}

pub fn compare(before: &Audit, after: &Audit) -> Comparison {
    let mut comparison = Comparison { before_id: before.id, after_id: after.id, ..Comparison::default() };
    let mut retested = 0u32;
    let mut now_passing = 0u32;

    for old in &before.checks {
        let new = after.checks.iter().find(|c| c.id == old.id);
        let after_status = new.map(|c| c.status);
        let change = match after_status {
            None if bad(old.status) => {
                // A rule that no longer fires at all (accessibility rules
                // only report when they find something) has been fixed.
                if old.id.starts_with("a11y:") {
                    "fixed"
                } else {
                    "gone"
                }
            }
            None => "gone",
            Some(new) if !old.status.ran() || !new.ran() => "not_comparable",
            Some(new) if bad(old.status) && new == Status::Pass => "fixed",
            Some(new) if old.status == Status::Pass && bad(new) => "regressed",
            Some(new) if bad(old.status) && bad(new) => "still_failing",
            Some(_) => "unchanged",
        };
        if old.finding.is_some() && (after_status.is_some_and(Status::ran) || change == "fixed") {
            retested += 1;
            if change == "fixed" {
                now_passing += 1;
            }
        }
        match change {
            "fixed" => comparison.fixed += 1,
            "regressed" => comparison.regressed += 1,
            "still_failing" => comparison.still_failing += 1,
            _ => {}
        }
        comparison.rows.push(ComparisonRow {
            id: old.id.clone(),
            title: old.title.clone(),
            before: Some(old.status),
            after: after_status,
            change: change.to_string(),
        });
    }

    for new in after.checks.iter().filter(|c| !before.checks.iter().any(|o| o.id == c.id)) {
        if bad(new.status) {
            comparison.regressed += 1;
        }
        comparison.rows.push(ComparisonRow {
            id: new.id.clone(),
            title: new.title.clone(),
            before: None,
            after: Some(new.status),
            change: if bad(new.status) { "regressed" } else { "new" }.to_string(),
        });
    }

    comparison.retest_pass_rate = (retested > 0).then(|| now_passing as f32 / retested as f32);
    comparison
}
