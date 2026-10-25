//! Web accessibility compliance: the WCAG report, the manual checklist and
//! on-demand page checks.
//!
//! Mirror structs only — the reasoning lives in `crawlstat_engine::a11y`.

use crawlstat_engine::a11y::{self, rules::RuleInfo, wcag, Standard};

use crate::runtime;

/// A standard the report can be measured against.
#[derive(Debug, Clone)]
pub struct A11yStandard {
    /// `wcag20aa`, `wcag21aa` or `wcag22aa`.
    pub id: String,
    pub label: String,
    pub laws: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct A11yCoverage {
    pub pages_checked: i64,
    pub pages_unchecked: i64,
    pub thin_pages: i64,
    pub thin_examples: Vec<String>,
    pub video_pages: i64,
    pub audio_pages: i64,
    pub form_pages: i64,
    pub password_pages: i64,
}

/// What a rule means and how to fix it.
#[derive(Debug, Clone)]
pub struct A11yRuleInfo {
    pub id: String,
    pub title: String,
    pub criteria: Vec<String>,
    /// `critical`, `serious`, `moderate` or `minor`.
    pub impact: String,
    /// `fail` (proven by the markup) or `review` (a person must confirm).
    pub certainty: String,
    pub affects: String,
    pub why: String,
    pub fix: String,
    pub bad: String,
    pub good: String,
}

#[derive(Debug, Clone)]
pub struct A11yGroup {
    pub sample: String,
    pub pages: i64,
    pub example_urls: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct A11yRule {
    pub info: A11yRuleInfo,
    pub pages: i64,
    pub instances: i64,
    /// Share of checked pages affected, 0..1.
    pub share: f64,
    /// Criteria in the chosen standard; empty means best practice only.
    pub criteria: Vec<String>,
    /// `A`, `AA`, or empty for best practice.
    pub level: String,
    pub priority: f64,
    pub groups: Vec<A11yGroup>,
    pub template_fix: bool,
}

#[derive(Debug, Clone)]
pub struct A11yCriterion {
    pub id: String,
    pub name: String,
    pub level: String,
    /// `Perceivable`, `Operable`, `Understandable` or `Robust`.
    pub principle: String,
    /// The WCAG version that introduced it, e.g. `2.1`.
    pub since: String,
    pub summary: String,
    pub how_to_test: String,
    pub url: String,
    /// `fail`, `review`, `checks_passed`, `untested`, `pass` or `na`.
    pub status: String,
    pub automated: String,
    pub rules_hit: Vec<String>,
    pub rules_run: Vec<String>,
    pub failing_pages: i64,
    /// `pass`, `fail` or `na` if a person reviewed it.
    pub review_status: Option<String>,
    pub review_note: Option<String>,
    pub reviewed_at: Option<i64>,
    pub conflict: bool,
    pub not_applicable_hint: Option<String>,
    pub vpat: String,
}

#[derive(Debug, Clone)]
pub struct A11yCounts {
    pub fail: i64,
    pub review: i64,
    pub checks_passed: i64,
    pub untested: i64,
    pub pass: i64,
    pub not_applicable: i64,
}

#[derive(Debug, Clone)]
pub struct A11yReport {
    pub standard: A11yStandard,
    pub coverage: A11yCoverage,
    /// `not_checked`, `does_not_conform`, `incomplete` or `conforms`.
    pub verdict: String,
    pub headline: String,
    pub detail: String,
    pub rules: Vec<A11yRule>,
    pub criteria: Vec<A11yCriterion>,
    pub counts: A11yCounts,
}

#[derive(Debug, Clone)]
pub struct A11yPage {
    pub url_id: i64,
    pub url: String,
    pub count: i64,
    pub samples: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct A11yHit {
    pub info: A11yRuleInfo,
    pub count: i64,
    pub samples: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct A11yPageCheck {
    pub url: String,
    pub title: Option<String>,
    pub word_count: i64,
    pub findings: Vec<A11yHit>,
    pub features: Vec<String>,
    pub passed: Vec<A11yRuleInfo>,
    pub likely_script_rendered: bool,
}

fn standard_of(id: &str) -> Standard {
    Standard::parse(id).unwrap_or_default()
}

fn standard_info(standard: Standard) -> A11yStandard {
    A11yStandard {
        id: standard.id().to_string(),
        label: standard.label().to_string(),
        laws: standard.laws().iter().map(|law| law.to_string()).collect(),
    }
}

fn rule_info(info: &RuleInfo) -> A11yRuleInfo {
    A11yRuleInfo {
        id: info.id.to_string(),
        title: info.title.to_string(),
        criteria: info.criteria.iter().map(|id| id.to_string()).collect(),
        impact: info.impact.as_str().to_string(),
        certainty: info.certainty.as_str().to_string(),
        affects: info.affects.to_string(),
        why: info.why.to_string(),
        fix: info.fix.to_string(),
        bad: info.bad.to_string(),
        good: info.good.to_string(),
    }
}

fn strings(items: &[&str]) -> Vec<String> {
    items.iter().map(|item| item.to_string()).collect()
}

/// The standards on offer, default first.
pub fn accessibility_standards() -> Vec<A11yStandard> {
    let mut all: Vec<Standard> = Standard::ALL.to_vec();
    all.sort_by_key(|standard| *standard != Standard::default());
    all.into_iter().map(standard_info).collect()
}

/// The full compliance report for a crawl.
pub fn accessibility_report(db_path: String, standard: String) -> Result<A11yReport, String> {
    let standard = standard_of(&standard);
    runtime::with_store(&db_path, |store| {
        let report = a11y::report(store.connection(), standard)?;
        Ok(convert(report))
    })
    .map_err(|error| error.to_string())
}

fn convert(report: a11y::ComplianceReport) -> A11yReport {
    let coverage = report.coverage;
    A11yReport {
        standard: standard_info(report.standard),
        coverage: A11yCoverage {
            pages_checked: coverage.pages_checked as i64,
            pages_unchecked: coverage.pages_unchecked as i64,
            thin_pages: coverage.thin_pages as i64,
            thin_examples: coverage.thin_examples,
            video_pages: coverage.video_pages as i64,
            audio_pages: coverage.audio_pages as i64,
            form_pages: coverage.form_pages as i64,
            password_pages: coverage.password_pages as i64,
        },
        verdict: report.verdict.as_str().to_string(),
        headline: report.headline,
        detail: report.detail,
        rules: report
            .rules
            .into_iter()
            .map(|rule| A11yRule {
                info: rule_info(rule.info),
                pages: rule.pages as i64,
                instances: rule.instances as i64,
                share: rule.share,
                criteria: strings(&rule.criteria),
                level: rule.level.map(|level| level.as_str().to_string()).unwrap_or_default(),
                priority: rule.priority,
                groups: rule
                    .groups
                    .into_iter()
                    .map(|group| A11yGroup {
                        sample: group.sample,
                        pages: group.pages as i64,
                        example_urls: group.example_urls,
                    })
                    .collect(),
                template_fix: rule.template_fix,
            })
            .collect(),
        criteria: report
            .criteria
            .into_iter()
            .map(|result| {
                let c: &wcag::Criterion = result.criterion;
                A11yCriterion {
                    id: c.id.to_string(),
                    name: c.name.to_string(),
                    level: c.level.as_str().to_string(),
                    principle: c.principle.as_str().to_string(),
                    since: c.since.label().to_string(),
                    summary: c.summary.to_string(),
                    how_to_test: c.how_to_test.to_string(),
                    url: c.understanding_url(),
                    status: result.status.as_str().to_string(),
                    automated: result.automated.as_str().to_string(),
                    rules_hit: strings(&result.rules_hit),
                    rules_run: strings(&result.rules_run),
                    failing_pages: result.failing_pages as i64,
                    review_status: result.review.as_ref().map(|review| review.status.clone()),
                    review_note: result.review.as_ref().and_then(|review| review.note.clone()),
                    reviewed_at: result.review.as_ref().map(|review| review.reviewed_at),
                    conflict: result.conflict,
                    not_applicable_hint: result.not_applicable_hint,
                    vpat: result.vpat.to_string(),
                }
            })
            .collect(),
        counts: A11yCounts {
            fail: report.counts.fail as i64,
            review: report.counts.review as i64,
            checks_passed: report.counts.checks_passed as i64,
            untested: report.counts.untested as i64,
            pass: report.counts.pass as i64,
            not_applicable: report.counts.not_applicable as i64,
        },
    }
}

/// Record a person's verdict on a criterion: `pass`, `fail`, `na`, or empty
/// to clear it.
pub fn accessibility_set_review(
    db_path: String,
    criterion: String,
    status: String,
    note: String,
) -> Result<(), String> {
    runtime::with_store(&db_path, |store| {
        a11y::set_review(store.connection(), &criterion, &status, &note)
    })
    .map_err(|error| error.to_string())
}

/// The pages a rule fired on, with the offending markup.
pub fn accessibility_rule_pages(db_path: String, rule: String, limit: u32) -> Result<Vec<A11yPage>, String> {
    runtime::with_store(&db_path, |store| {
        Ok(a11y::rule_pages(store.connection(), &rule, limit)?
            .into_iter()
            .map(|page| A11yPage {
                url_id: page.url_id,
                url: page.url,
                count: page.count as i64,
                samples: page.samples,
            })
            .collect())
    })
    .map_err(|error| error.to_string())
}

/// Fetch one live URL and check it now — no crawl needed.
pub fn accessibility_check_url(url: String) -> Result<A11yPageCheck, String> {
    let check = a11y::check_url(&url)?;
    Ok(A11yPageCheck {
        url: check.url,
        title: check.title,
        word_count: check.word_count as i64,
        findings: check
            .findings
            .into_iter()
            .map(|finding| A11yHit {
                info: rule_info(finding.info),
                count: finding.count as i64,
                samples: finding.samples,
            })
            .collect(),
        features: strings(&check.features),
        passed: check
            .passed
            .iter()
            .filter_map(|id| a11y::rules::info(id))
            .map(rule_info)
            .collect(),
        likely_script_rendered: check.likely_script_rendered,
    })
}

/// The conformance report as Markdown, ready to save or share.
pub fn accessibility_report_markdown(
    db_path: String,
    standard: String,
    site: String,
) -> Result<String, String> {
    let standard = standard_of(&standard);
    runtime::with_store(&db_path, |store| {
        let report = a11y::report(store.connection(), standard)?;
        Ok(a11y::markdown(&report, &site, &a11y::today()))
    })
    .map_err(|error| error.to_string())
}
