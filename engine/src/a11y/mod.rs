//! Web accessibility compliance: WCAG conformance built from the crawl.
//!
//! The parser records rule-level findings per page (`a11y_findings`). This
//! module turns them into what an organisation answering to the ADA, Section
//! 508 or the European Accessibility Act actually needs:
//!
//! - a status for every WCAG success criterion in the chosen standard,
//! - the fixes that remove the most barriers first, grouped by template so one
//!   change to a shared header shows up as one task rather than 4,000,
//! - a manual checklist for everything a crawler cannot see, whose answers are
//!   stored (`a11y_reviews`) and folded back into the report, and
//! - a VPAT-style conformance summary that can be exported.
//!
//! It never claims more than it knows. Automated checks find roughly a third
//! of WCAG failures; a criterion with no automated hits is "checks passed",
//! not "passes", until a person says so.

pub mod rules;
pub mod wcag;

use std::collections::{HashMap, HashSet};

use rusqlite::{params, Connection, OptionalExtension};
use url::Url;

use crate::error::EngineError;
use crate::parse::accessibility::{feature, rule};
use rules::{Certainty, RuleInfo};
use wcag::{Applies, Criterion, Level, WcagVersion};

type Result<T> = std::result::Result<T, EngineError>;

/// Pages under this many words were probably rendered by JavaScript, so the
/// crawler saw a shell rather than the page.
const THIN_WORDS: i64 = 25;

/// The standard to measure against.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Standard {
    /// Section 508 (US federal), and older contracts.
    Wcag20Aa,
    /// ADA Title II and III, EN 301 549 / European Accessibility Act.
    #[default]
    Wcag21Aa,
    /// The current W3C recommendation; a superset of 2.1 except 4.1.1.
    Wcag22Aa,
}

impl Standard {
    pub const ALL: [Standard; 3] = [Standard::Wcag20Aa, Standard::Wcag21Aa, Standard::Wcag22Aa];

    pub fn parse(value: &str) -> Option<Standard> {
        Standard::ALL.into_iter().find(|standard| standard.id() == value)
    }

    pub fn id(self) -> &'static str {
        match self {
            Standard::Wcag20Aa => "wcag20aa",
            Standard::Wcag21Aa => "wcag21aa",
            Standard::Wcag22Aa => "wcag22aa",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Standard::Wcag20Aa => "WCAG 2.0 AA",
            Standard::Wcag21Aa => "WCAG 2.1 AA",
            Standard::Wcag22Aa => "WCAG 2.2 AA",
        }
    }

    pub fn version(self) -> WcagVersion {
        match self {
            Standard::Wcag20Aa => WcagVersion::V20,
            Standard::Wcag21Aa => WcagVersion::V21,
            Standard::Wcag22Aa => WcagVersion::V22,
        }
    }

    /// The laws and policies that point at this standard.
    pub fn laws(self) -> &'static [&'static str] {
        match self {
            Standard::Wcag20Aa => &[
                "Section 508 of the Rehabilitation Act (US federal agencies and their vendors)",
                "Many US state procurement rules",
            ],
            Standard::Wcag21Aa => &[
                "ADA Title II — the 2024 DOJ rule for state and local government sites",
                "ADA Title III — the benchmark courts and DOJ guidance use for businesses open to the public",
                "European Accessibility Act, through EN 301 549",
                "UK Public Sector Bodies Accessibility Regulations",
                "Ontario AODA (2.0 AA, met by 2.1 AA)",
            ],
            Standard::Wcag22Aa => &[
                "Meets everything WCAG 2.1 AA does, plus six newer criteria",
                "Recommended target for new work; increasingly cited in settlements and procurement",
            ],
        }
    }

    pub fn includes(self, criterion: &Criterion) -> bool {
        criterion.in_version(self.version())
    }

    pub fn criteria(self) -> impl Iterator<Item = &'static Criterion> {
        wcag::CRITERIA.iter().filter(move |criterion| self.includes(criterion))
    }
}

/// Where a success criterion stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// An automated check proved a failure, or a person marked it failed.
    Fail,
    /// Automated checks found something a person needs to confirm.
    NeedsReview,
    /// Automated checks for it ran and found nothing. Not a pass on its own.
    ChecksPassed,
    /// No automated check exists; only a person can test it.
    Untested,
    /// A person tested it and it passes.
    Pass,
    /// A person decided it does not apply (no video, no forms…).
    NotApplicable,
}

impl Status {
    pub fn as_str(self) -> &'static str {
        match self {
            Status::Fail => "fail",
            Status::NeedsReview => "review",
            Status::ChecksPassed => "checks_passed",
            Status::Untested => "untested",
            Status::Pass => "pass",
            Status::NotApplicable => "na",
        }
    }

    /// Done means a person can sign it off as it stands.
    pub fn is_settled(self) -> bool {
        matches!(self, Status::Pass | Status::NotApplicable)
    }
}

/// A person's verdict, as stored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Review {
    /// "pass", "fail" or "na".
    pub status: String,
    pub note: Option<String>,
    /// Unix seconds.
    pub reviewed_at: i64,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Coverage {
    /// Internal 2xx HTML pages checked by the current rules.
    pub pages_checked: u32,
    /// Internal HTML pages crawled before rule-level checks existed.
    pub pages_unchecked: u32,
    /// Checked pages with almost no text — probably rendered by JavaScript.
    pub thin_pages: u32,
    pub thin_examples: Vec<String>,
    pub video_pages: u32,
    pub audio_pages: u32,
    pub form_pages: u32,
    pub password_pages: u32,
}

/// Pages that share the same offending markup — usually one template.
#[derive(Debug, Clone, PartialEq)]
pub struct TemplateGroup {
    pub sample: String,
    pub pages: u32,
    pub example_urls: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct RuleResult {
    pub info: &'static RuleInfo,
    pub pages: u32,
    pub instances: u64,
    /// Share of checked pages affected, 0.0..=1.0.
    pub share: f64,
    /// The criteria this rule evidences *in the chosen standard*. Empty means
    /// it is a best practice here (duplicate ids under WCAG 2.2).
    pub criteria: Vec<&'static str>,
    /// The strictest level among those criteria.
    pub level: Option<Level>,
    pub priority: f64,
    pub groups: Vec<TemplateGroup>,
    /// One shared snippet accounts for most affected pages: fix it once in
    /// the template.
    pub template_fix: bool,
}

impl RuleResult {
    pub fn best_practice(&self) -> bool {
        self.criteria.is_empty()
    }
}

#[derive(Debug, Clone)]
pub struct CriterionResult {
    pub criterion: &'static Criterion,
    pub status: Status,
    /// What the crawl alone says, before any review.
    pub automated: Status,
    /// Rules mapped to this criterion that found something.
    pub rules_hit: Vec<&'static str>,
    /// Every rule that tests this criterion.
    pub rules_run: Vec<&'static str>,
    /// Pages with a definite automated failure.
    pub failing_pages: u32,
    pub review: Option<Review>,
    /// A person marked it passed or not applicable, but the crawl still
    /// proves a failure.
    pub conflict: bool,
    /// Set when nothing on the site seems to need this criterion.
    pub not_applicable_hint: Option<String>,
    /// Conformance level term for a VPAT / ACR.
    pub vpat: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    /// Nothing has been checked yet: crawl first.
    NotChecked,
    /// At least one criterion fails.
    DoesNotConform,
    /// No known failures, but criteria remain for a person to test.
    Incomplete,
    /// Every criterion passes or does not apply, on the reviewer's say-so.
    Conforms,
}

impl Verdict {
    pub fn as_str(self) -> &'static str {
        match self {
            Verdict::NotChecked => "not_checked",
            Verdict::DoesNotConform => "does_not_conform",
            Verdict::Incomplete => "incomplete",
            Verdict::Conforms => "conforms",
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct StatusCounts {
    pub fail: u32,
    pub review: u32,
    pub checks_passed: u32,
    pub untested: u32,
    pub pass: u32,
    pub not_applicable: u32,
}

#[derive(Debug, Clone)]
pub struct ComplianceReport {
    pub standard: Standard,
    pub coverage: Coverage,
    pub verdict: Verdict,
    pub headline: String,
    pub detail: String,
    /// Rules with at least one hit, highest priority first.
    pub rules: Vec<RuleResult>,
    /// Every criterion in the standard, in numbering order.
    pub criteria: Vec<CriterionResult>,
    pub counts: StatusCounts,
}

/// Pages that count: internal, successful and checked by rule-level checks.
const CHECKED: &str = "u.a11y_checked IS NOT NULL AND u.is_internal = 1 \
                       AND u.status_code BETWEEN 200 AND 299";

/// Build the compliance report for a crawl.
pub fn report(conn: &Connection, standard: Standard) -> Result<ComplianceReport> {
    let coverage = coverage(conn)?;
    let reviews = reviews(conn)?;

    // Per-rule totals over checked pages.
    let mut totals: HashMap<String, (u32, u64)> = HashMap::new();
    {
        let mut statement = conn.prepare(&format!(
            "SELECT f.rule, count(*), sum(f.count) FROM a11y_findings f
             JOIN urls u ON u.id = f.url_id WHERE {CHECKED} GROUP BY f.rule"
        ))?;
        let rows = statement.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?, row.get::<_, i64>(2)?))
        })?;
        for row in rows {
            let (rule, pages, instances) = row?;
            totals.insert(rule, (pages as u32, instances.max(0) as u64));
        }
    }

    let checked = coverage.pages_checked.max(1) as f64;
    let mut rule_results = Vec::new();
    for info in rules::RULES {
        let Some(&(pages, instances)) = totals.get(info.id) else {
            continue;
        };
        let criteria: Vec<&'static str> = info
            .criteria
            .iter()
            .copied()
            .filter(|id| wcag::criterion(id).is_some_and(|c| standard.includes(c)))
            .collect();
        let level = criteria
            .iter()
            .filter_map(|id| wcag::criterion(id))
            .map(|c| c.level)
            .min_by_key(|level| matches!(level, Level::AA));
        let level_weight = match level {
            Some(Level::A) => 1.5,
            Some(Level::AA) => 1.0,
            None => 0.5,
        };
        let priority = info.impact.weight()
            * level_weight
            * info.certainty.weight()
            * ((1.0 + pages as f64).ln() + 1.0);
        let groups = template_groups(conn, info.id)?;
        let template_fix = groups
            .first()
            .is_some_and(|group| group.pages >= 3 && group.pages * 2 >= pages);
        rule_results.push(RuleResult {
            info,
            pages,
            instances,
            share: pages as f64 / checked,
            criteria,
            level,
            priority,
            groups,
            template_fix,
        });
    }
    rule_results.sort_by(|a, b| b.priority.total_cmp(&a.priority));

    let mut criteria = Vec::new();
    let mut counts = StatusCounts::default();
    for criterion in standard.criteria() {
        let result = criterion_result(conn, criterion, &rule_results, &reviews, &coverage)?;
        match result.status {
            Status::Fail => counts.fail += 1,
            Status::NeedsReview => counts.review += 1,
            Status::ChecksPassed => counts.checks_passed += 1,
            Status::Untested => counts.untested += 1,
            Status::Pass => counts.pass += 1,
            Status::NotApplicable => counts.not_applicable += 1,
        }
        criteria.push(result);
    }

    let verdict = if coverage.pages_checked == 0 {
        Verdict::NotChecked
    } else if counts.fail > 0 {
        Verdict::DoesNotConform
    } else if criteria.iter().all(|c| c.status.is_settled()) {
        Verdict::Conforms
    } else {
        Verdict::Incomplete
    };
    let (headline, detail) = summarise(standard, verdict, &coverage, &counts, &rule_results);

    Ok(ComplianceReport {
        standard,
        coverage,
        verdict,
        headline,
        detail,
        rules: rule_results,
        criteria,
        counts,
    })
}

fn coverage(conn: &Connection) -> Result<Coverage> {
    let pages_checked: i64 =
        conn.query_row(&format!("SELECT count(*) FROM urls u WHERE {CHECKED}"), [], |row| {
            row.get(0)
        })?;
    let pages_unchecked: i64 = conn.query_row(
        "SELECT count(*) FROM urls u WHERE u.a11y_checked IS NULL AND u.is_internal = 1
           AND u.status_code BETWEEN 200 AND 299 AND u.content_type LIKE '%html%'",
        [],
        |row| row.get(0),
    )?;
    let thin_pages: i64 = conn.query_row(
        &format!("SELECT count(*) FROM urls u WHERE {CHECKED} AND coalesce(u.word_count, 0) < ?1"),
        [THIN_WORDS],
        |row| row.get(0),
    )?;
    let thin_examples = conn
        .prepare(&format!(
            "SELECT u.url FROM urls u WHERE {CHECKED} AND coalesce(u.word_count, 0) < ?1
             ORDER BY coalesce(u.link_score, 0) DESC, u.id LIMIT 3"
        ))?
        .query_map([THIN_WORDS], |row| row.get(0))?
        .collect::<rusqlite::Result<Vec<String>>>()?;

    let mut feature_pages: HashMap<String, u32> = HashMap::new();
    {
        let placeholders = vec!["?"; feature::ALL.len()].join(",");
        let mut statement = conn.prepare(&format!(
            "SELECT f.rule, count(*) FROM a11y_findings f JOIN urls u ON u.id = f.url_id
             WHERE {CHECKED} AND f.rule IN ({placeholders}) GROUP BY f.rule"
        ))?;
        let rows = statement.query_map(rusqlite::params_from_iter(feature::ALL), |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })?;
        for row in rows {
            let (name, pages) = row?;
            feature_pages.insert(name, pages as u32);
        }
    }
    let pages_with = |name: &str| feature_pages.get(name).copied().unwrap_or(0);

    Ok(Coverage {
        pages_checked: pages_checked as u32,
        pages_unchecked: pages_unchecked as u32,
        thin_pages: thin_pages as u32,
        thin_examples,
        video_pages: pages_with(feature::VIDEO),
        audio_pages: pages_with(feature::AUDIO),
        form_pages: pages_with(feature::FORM),
        password_pages: pages_with(feature::PASSWORD),
    })
}

fn reviews(conn: &Connection) -> Result<HashMap<String, Review>> {
    let mut statement =
        conn.prepare("SELECT criterion, status, note, reviewed_at FROM a11y_reviews")?;
    let rows = statement.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            Review {
                status: row.get(1)?,
                note: row.get(2)?,
                reviewed_at: row.get(3)?,
            },
        ))
    })?;
    Ok(rows.collect::<rusqlite::Result<HashMap<_, _>>>()?)
}

/// The most common offending snippets for a rule, with the pages they are on.
fn template_groups(conn: &Connection, rule_id: &str) -> Result<Vec<TemplateGroup>> {
    let mut statement = conn.prepare(&format!(
        "SELECT f.sample_key, min(f.sample), count(*) FROM a11y_findings f
         JOIN urls u ON u.id = f.url_id
         WHERE {CHECKED} AND f.rule = ?1 AND f.sample IS NOT NULL
         GROUP BY f.sample_key ORDER BY 3 DESC, 1 LIMIT 5"
    ))?;
    let groups = statement
        .query_map([rule_id], |row| {
            Ok((
                row.get::<_, Option<String>>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    let mut examples = conn.prepare(&format!(
        "SELECT u.url FROM a11y_findings f JOIN urls u ON u.id = f.url_id
         WHERE {CHECKED} AND f.rule = ?1 AND f.sample_key IS ?2
         ORDER BY coalesce(u.link_score, 0) DESC, u.id LIMIT 3"
    ))?;
    let mut out = Vec::with_capacity(groups.len());
    for (key, sample, pages) in groups {
        let example_urls = examples
            .query_map(params![rule_id, key], |row| row.get(0))?
            .collect::<rusqlite::Result<Vec<String>>>()?;
        out.push(TemplateGroup {
            sample,
            pages: pages as u32,
            example_urls,
        });
    }
    Ok(out)
}

fn criterion_result(
    conn: &Connection,
    criterion: &'static Criterion,
    rule_results: &[RuleResult],
    reviews: &HashMap<String, Review>,
    coverage: &Coverage,
) -> Result<CriterionResult> {
    let rules_run: Vec<&'static str> = rules::RULES
        .iter()
        .filter(|info| info.criteria.contains(&criterion.id))
        .map(|info| info.id)
        .collect();
    let hits: Vec<&RuleResult> = rule_results
        .iter()
        .filter(|result| result.criteria.contains(&criterion.id))
        .collect();
    let failing: Vec<&'static str> = hits
        .iter()
        .filter(|result| result.info.certainty == Certainty::Fail)
        .map(|result| result.info.id)
        .collect();

    let automated = if !failing.is_empty() {
        Status::Fail
    } else if !hits.is_empty() {
        Status::NeedsReview
    } else if !rules_run.is_empty() && coverage.pages_checked > 0 {
        Status::ChecksPassed
    } else {
        Status::Untested
    };

    let failing_pages = if failing.is_empty() {
        0
    } else {
        let placeholders = vec!["?"; failing.len()].join(",");
        conn.query_row(
            &format!(
                "SELECT count(DISTINCT f.url_id) FROM a11y_findings f JOIN urls u ON u.id = f.url_id
                 WHERE {CHECKED} AND f.rule IN ({placeholders})"
            ),
            rusqlite::params_from_iter(&failing),
            |row| row.get::<_, i64>(0),
        )? as u32
    };

    let review = reviews.get(criterion.id).cloned();
    let (status, conflict) = match review.as_ref().map(|review| review.status.as_str()) {
        Some("pass") if automated == Status::Fail => (Status::Fail, true),
        Some("na") if automated == Status::Fail => (Status::Fail, true),
        Some("pass") => (Status::Pass, false),
        Some("na") => (Status::NotApplicable, false),
        Some("fail") => (Status::Fail, false),
        _ => (automated, false),
    };

    let not_applicable_hint = if coverage.pages_checked == 0 || status.is_settled() {
        None
    } else {
        let n = coverage.pages_checked;
        match criterion.applies {
            Applies::Media if coverage.video_pages + coverage.audio_pages == 0 => Some(format!(
                "No video or audio found on {n} checked pages. If the site has none, mark this Not applicable."
            )),
            Applies::Forms if coverage.form_pages == 0 => Some(format!(
                "No forms found on {n} checked pages. If the site has none, mark this Not applicable."
            )),
            Applies::Authentication if coverage.password_pages == 0 => Some(format!(
                "No login form found on {n} checked pages. If nobody signs in, mark this Not applicable."
            )),
            _ => None,
        }
    };

    let vpat = match status {
        Status::Pass => "Supports",
        Status::NotApplicable => "Not Applicable",
        Status::Fail if failing_pages > 0 => {
            if failing_pages * 2 >= coverage.pages_checked {
                "Does Not Support"
            } else {
                "Partially Supports"
            }
        }
        Status::Fail => "Does Not Support",
        Status::NeedsReview | Status::ChecksPassed | Status::Untested => "Not Evaluated",
    };

    Ok(CriterionResult {
        criterion,
        status,
        automated,
        rules_hit: hits.iter().map(|result| result.info.id).collect(),
        rules_run,
        failing_pages,
        review,
        conflict,
        not_applicable_hint,
        vpat,
    })
}

fn summarise(
    standard: Standard,
    verdict: Verdict,
    coverage: &Coverage,
    counts: &StatusCounts,
    rules: &[RuleResult],
) -> (String, String) {
    let label = standard.label();
    let total = counts.fail
        + counts.review
        + counts.checks_passed
        + counts.untested
        + counts.pass
        + counts.not_applicable;
    let open = counts.review + counts.checks_passed + counts.untested;
    match verdict {
        Verdict::NotChecked => (
            "Not checked yet".into(),
            if coverage.pages_unchecked > 0 {
                format!(
                    "This crawl predates the accessibility checks. Re-crawl to check its {} pages against {label}.",
                    coverage.pages_unchecked
                )
            } else {
                format!("Crawl a site to check it against {label}.")
            },
        ),
        Verdict::DoesNotConform => {
            let template_fixes = rules
                .iter()
                .filter(|rule| rule.template_fix && !rule.best_practice())
                .count();
            let mut detail = format!(
                "{} of {total} criteria fail. Start with \"Fix first\" below",
                counts.fail
            );
            if template_fixes > 0 {
                detail.push_str(&format!(
                    " — {template_fixes} of the failures come from shared templates, so one change fixes each across the site"
                ));
            }
            detail.push_str(&format!(". {open} more need a person to test."));
            (format!("Does not meet {label}"), detail)
        }
        Verdict::Incomplete => (
            "No automated failures — manual testing needed".into(),
            format!(
                "The crawler found no definite failures, but automated checks cover only part of {label}. \
                 {open} of {total} criteria still need a person to test them with a keyboard, a screen reader \
                 and a contrast checker."
            ),
        ),
        Verdict::Conforms => (
            format!("Meets {label} (self-assessed)"),
            format!(
                "Every one of the {total} criteria has been marked passed or not applicable, and the crawler \
                 found no failures. Keep the review notes as your evidence and re-test after major changes."
            ),
        ),
    }
}

/// One page affected by a rule.
#[derive(Debug, Clone, PartialEq)]
pub struct RulePage {
    pub url_id: i64,
    pub url: String,
    pub count: u32,
    pub samples: Vec<String>,
}

/// Pages a rule fired on, worst and most important first.
pub fn rule_pages(conn: &Connection, rule_id: &str, limit: u32) -> Result<Vec<RulePage>> {
    let mut statement = conn.prepare(&format!(
        "SELECT u.id, u.url, f.count, f.sample, f.more FROM a11y_findings f
         JOIN urls u ON u.id = f.url_id WHERE {CHECKED} AND f.rule = ?1
         ORDER BY f.count DESC, coalesce(u.link_score, 0) DESC, u.id LIMIT ?2"
    ))?;
    let rows = statement.query_map(params![rule_id, limit], |row| {
        let mut samples: Vec<String> = row.get::<_, Option<String>>(3)?.into_iter().collect();
        if let Some(more) = row.get::<_, Option<String>>(4)? {
            samples.extend(more.lines().map(str::to_string));
        }
        Ok(RulePage {
            url_id: row.get(0)?,
            url: row.get(1)?,
            count: row.get::<_, i64>(2)? as u32,
            samples,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// Record a person's verdict on a criterion. An empty status clears it.
pub fn set_review(conn: &Connection, criterion: &str, status: &str, note: &str) -> Result<()> {
    if wcag::criterion(criterion).is_none() {
        return Err(EngineError::Config(format!("unknown WCAG criterion {criterion}")));
    }
    match status {
        "" => {
            conn.execute("DELETE FROM a11y_reviews WHERE criterion = ?1", [criterion])?;
        }
        "pass" | "fail" | "na" => {
            let note = note.trim();
            conn.execute(
                "INSERT INTO a11y_reviews (criterion, status, note, reviewed_at)
                 VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT (criterion) DO UPDATE SET
                    status = excluded.status, note = excluded.note,
                    reviewed_at = excluded.reviewed_at",
                params![
                    criterion,
                    status,
                    (!note.is_empty()).then_some(note),
                    chrono::Utc::now().timestamp()
                ],
            )?;
        }
        other => {
            return Err(EngineError::Config(format!(
                "review status must be pass, fail or na, not {other:?}"
            )))
        }
    }
    Ok(())
}

/// The stored review for one criterion.
pub fn review(conn: &Connection, criterion: &str) -> Result<Option<Review>> {
    Ok(conn
        .query_row(
            "SELECT status, note, reviewed_at FROM a11y_reviews WHERE criterion = ?1",
            [criterion],
            |row| {
                Ok(Review {
                    status: row.get(0)?,
                    note: row.get(1)?,
                    reviewed_at: row.get(2)?,
                })
            },
        )
        .optional()?)
}

/// A rule that fired on a single checked page.
#[derive(Debug, Clone)]
pub struct PageFinding {
    pub info: &'static RuleInfo,
    pub count: u32,
    pub samples: Vec<String>,
}

/// The result of checking one page on demand.
#[derive(Debug, Clone)]
pub struct PageCheck {
    pub url: String,
    pub title: Option<String>,
    pub word_count: u32,
    /// Definite failures first, then by impact.
    pub findings: Vec<PageFinding>,
    pub features: Vec<&'static str>,
    /// Rules that ran and found nothing.
    pub passed: Vec<&'static str>,
    /// So little text that the page is probably built by JavaScript, and the
    /// results describe the loading shell.
    pub likely_script_rendered: bool,
}

/// Check markup that is already in hand.
pub fn check_markup(html: &str, url: &str) -> std::result::Result<PageCheck, String> {
    let parsed_url = Url::parse(url).map_err(|error| format!("{url} is not a valid URL: {error}"))?;
    let page = crate::parse::parse_html(
        html,
        &parsed_url,
        &crate::url_norm::NormalizationPolicy::default(),
    );
    let report = page.accessibility;

    let mut findings: Vec<PageFinding> = report
        .findings
        .iter()
        .filter_map(|finding| {
            rules::info(finding.rule).map(|info| PageFinding {
                info,
                count: finding.count,
                samples: finding.samples.clone(),
            })
        })
        .collect();
    findings.sort_by_key(|finding| {
        (
            finding.info.certainty != Certainty::Fail,
            finding.info.impact,
            std::cmp::Reverse(finding.count),
        )
    });
    let fired: HashSet<&str> = findings.iter().map(|finding| finding.info.id).collect();
    let passed = rule::ALL
        .iter()
        .copied()
        .filter(|id| !fired.contains(id))
        .collect();

    Ok(PageCheck {
        url: url.to_string(),
        title: page.title,
        word_count: page.word_count,
        findings,
        features: report.features.clone(),
        passed,
        likely_script_rendered: (page.word_count as i64) < THIN_WORDS,
    })
}

/// Fetch a live URL and check it — a staging page, a page behind a fix, a
/// competitor. Blocking.
pub fn check_url(url: &str) -> std::result::Result<PageCheck, String> {
    let url = url.trim();
    let url = if url.contains("://") {
        url.to_string()
    } else {
        format!("https://{url}")
    };
    let html = crate::ux::fetch_markup(&url)?;
    check_markup(&html, &url)
}

/// Today's local date, `YYYY-MM-DD`, for dating a report.
pub fn today() -> String {
    chrono::Local::now().format("%Y-%m-%d").to_string()
}

/// The report as a Markdown conformance statement with a VPAT-style table.
pub fn markdown(report: &ComplianceReport, site: &str, date: &str) -> String {
    use std::fmt::Write;

    let label = report.standard.label();
    let mut out = String::new();
    let _ = writeln!(out, "# Accessibility conformance report — {site}\n");
    let _ = writeln!(out, "- **Standard:** {label}");
    let _ = writeln!(out, "- **Date:** {date}");
    let _ = writeln!(
        out,
        "- **Scope:** {} pages checked automatically",
        report.coverage.pages_checked
    );
    let _ = writeln!(out, "- **Status:** {}\n", report.headline);
    let _ = writeln!(out, "{}\n", report.detail);

    let _ = writeln!(out, "## Applicable laws and policies\n");
    for law in report.standard.laws() {
        let _ = writeln!(out, "- {law}");
    }
    out.push('\n');

    if report.coverage.thin_pages > 0 {
        let _ = writeln!(
            out,
            "> **Note:** {} pages had almost no text and are probably rendered by JavaScript. \
             Their results describe the loading shell; test them in a browser.\n",
            report.coverage.thin_pages
        );
    }

    let failures: Vec<&RuleResult> = report.rules.iter().collect();
    if !failures.is_empty() {
        let _ = writeln!(out, "## Issues to fix, in priority order\n");
        for (index, rule) in failures.iter().enumerate() {
            let criteria = if rule.best_practice() {
                "best practice".to_string()
            } else {
                rule.criteria.join(", ")
            };
            let kind = match rule.info.certainty {
                Certainty::Fail => "failure",
                Certainty::Review => "needs review",
            };
            let _ = writeln!(
                out,
                "### {}. {} ({criteria})\n\n**{}**, {kind} — {} pages, {} instances. Affects: {}.\n",
                index + 1,
                rule.info.title,
                rule.info.impact.as_str(),
                rule.pages,
                rule.instances,
                rule.info.affects
            );
            let _ = writeln!(out, "{}\n", rule.info.why);
            let _ = writeln!(out, "**Fix:** {}\n", rule.info.fix);
            if rule.template_fix {
                if let Some(group) = rule.groups.first() {
                    let _ = writeln!(
                        out,
                        "One shared snippet accounts for {} of these pages — fix it in the template:\n",
                        group.pages
                    );
                }
            }
            if let Some(group) = rule.groups.first() {
                let _ = writeln!(out, "```html\n{}\n```\n", group.sample);
                for url in &group.example_urls {
                    let _ = writeln!(out, "- {url}");
                }
                out.push('\n');
            }
        }
    }

    let _ = writeln!(out, "## Success criteria\n");
    let _ = writeln!(out, "| Criterion | Level | Conformance | Remarks |");
    let _ = writeln!(out, "|---|---|---|---|");
    for result in &report.criteria {
        let c = result.criterion;
        let mut remarks = match result.status {
            Status::Fail if result.failing_pages > 0 => format!(
                "Automated failures on {} pages ({}).",
                result.failing_pages,
                result.rules_hit.join(", ")
            ),
            Status::Fail => "Failed manual review.".to_string(),
            Status::NeedsReview => format!("Possible issues to confirm: {}.", result.rules_hit.join(", ")),
            Status::ChecksPassed => "Automated checks found no issues; manual testing pending.".to_string(),
            Status::Untested => "Requires manual testing.".to_string(),
            Status::Pass => "Passed manual review.".to_string(),
            Status::NotApplicable => "Not applicable.".to_string(),
        };
        if result.conflict {
            remarks.push_str(" Marked as passing in review, but automated checks still fail.");
        }
        if let Some(note) = result.review.as_ref().and_then(|review| review.note.as_deref()) {
            remarks.push(' ');
            remarks.push_str(note);
        }
        let _ = writeln!(
            out,
            "| {} {} | {} | {} | {} |",
            c.id,
            c.name,
            c.level.as_str(),
            result.vpat,
            remarks.replace('|', "\\|").replace('\n', " ")
        );
    }

    let _ = writeln!(
        out,
        "\n---\n\n*Generated by CrawlStat. Automated testing finds only part of WCAG failures; \
         \"Not Evaluated\" criteria need testing by a person with a keyboard, a screen reader and a \
         contrast checker. This report is an engineering aid, not legal advice or a certification.*"
    );
    out
}

#[cfg(test)]
mod tests;
