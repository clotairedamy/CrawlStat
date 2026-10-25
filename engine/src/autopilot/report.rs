//! Exports: a report a client can read, and a CSV a developer can work from.

use std::fmt::Write as _;

use super::store::FindingState;
use super::{Audit, Comparison, Severity, Status};

fn date(at: i64) -> String {
    chrono::DateTime::from_timestamp(at, 0).map(|d| d.format("%Y-%m-%d %H:%M UTC").to_string()).unwrap_or_default()
}

fn state<'a>(states: &'a [FindingState], id: &str) -> Option<&'a FindingState> {
    states.iter().find(|s| s.check_id == id)
}

fn severity_of(audit_severity: Severity, state: Option<&FindingState>) -> String {
    state
        .and_then(|s| s.severity_override.as_deref())
        .and_then(Severity::parse)
        .map(|s| format!("{} (reviewer override; rules said {})", s.label(), audit_severity.label()))
        .unwrap_or_else(|| audit_severity.label().to_string())
}

/// The client report, as Markdown.
pub fn client_markdown(audit: &Audit, states: &[FindingState], comparison: Option<&Comparison>) -> String {
    let summary = audit.summary();
    let mut out = String::new();
    let _ = writeln!(out, "# UX audit: {}\n", audit.setup.url);
    let _ = writeln!(out, "- **Tested:** {}", date(audit.tested_at));
    let _ = writeln!(
        out,
        "- **Page type:** {}{}",
        audit.page_type.label(),
        if audit.classification.overridden { " (set by the tester)" } else { " (detected — an estimate)" }
    );
    let _ = writeln!(out, "- **Goal:** {}{}", audit.goal.label(), if audit.goal_inferred { " (inferred)" } else { "" });
    let _ = writeln!(out, "- **Device:** {} · **Depth:** {}", audit.setup.device.label(), audit.setup.depth.label());
    let _ = writeln!(out, "- **Evidence:** {} HTML, no browser\n", audit.source);

    let _ = writeln!(out, "## Executive summary\n");
    let _ = writeln!(
        out,
        "{} checks were selected. {} passed, {} failed, {} need a person's review, {} could not be tested and {} were blocked by guardrails. \
         These numbers are kept separate on purpose: a check that could not run has not passed.\n",
        summary.selected, summary.pass, summary.fail, summary.needs_review, summary.not_tested, summary.blocked
    );
    let _ = writeln!(
        out,
        "Findings: **{} critical**, **{} high**, {} medium, {} low, {} observations. Coverage {:.0}%; {:.0}% of the checks that ran rest on verified evidence.\n",
        summary.critical,
        summary.high,
        summary.medium,
        summary.low,
        summary.observation,
        summary.coverage * 100.0,
        summary.confidence * 100.0
    );
    if !audit.understanding.intent.is_empty() {
        let _ = writeln!(out, "**What the page is for:** {}\n", audit.understanding.intent);
    }

    let mut findings: Vec<_> =
        audit.findings().filter(|(c, _)| !state(states, &c.id).is_some_and(|s| s.flagged)).collect();
    findings.sort_by(|a, b| b.1.priority.partial_cmp(&a.1.priority).unwrap_or(std::cmp::Ordering::Equal));

    let _ = writeln!(out, "## Findings, in priority order\n");
    if findings.is_empty() {
        let _ = writeln!(out, "No findings.\n");
    }
    for (i, (check, finding)) in findings.iter().enumerate() {
        let st = state(states, &check.id);
        let _ = writeln!(out, "### {}. {}\n", i + 1, finding.title);
        let _ = writeln!(
            out,
            "**Severity:** {} · **Status:** {} · **Evidence:** {} ({}) · **Priority:** {:.0}{}\n",
            severity_of(finding.severity, st),
            check.status.label(),
            check.evidence.status.label(),
            check.method.label(),
            finding.priority,
            if finding.severity == Severity::Critical { " (critical — escalated)" } else { "" }
        );
        let _ = writeln!(out, "{}\n", finding.explanation);
        let _ = writeln!(out, "**Why it matters:** {}\n", finding.consequence);
        if !check.evidence.observed.is_empty() {
            let _ = writeln!(out, "**What was observed:**\n");
            for o in check.evidence.observed.iter().take(6) {
                let _ = writeln!(out, "- {o}");
            }
            let _ = writeln!(out);
        }
        let r = &finding.recommendation;
        let _ = writeln!(out, "**Recommendation:** {}\n", r.change);
        if let (Some(before), Some(after)) = (&r.before, &r.after) {
            let _ = writeln!(out, "Before:\n\n```\n{before}\n```\n\nAfter (example):\n\n```\n{after}\n```\n");
        } else if let Some(after) = &r.after {
            let _ = writeln!(out, "Example:\n\n```\n{after}\n```\n");
        }
        if !r.acceptance.is_empty() {
            let _ = writeln!(out, "**Done when:**\n");
            for a in &r.acceptance {
                let _ = writeln!(out, "- [ ] {a}");
            }
            let _ = writeln!(out);
        }
        if let Some(st) = st {
            let mut work = vec![format!("Work status: {}", st.work_status.label())];
            if !st.owner.is_empty() {
                work.push(format!("owner {}", st.owner));
            }
            if !st.due.is_empty() {
                work.push(format!("due {}", st.due));
            }
            if let Some(r) = &st.retest {
                work.push(format!("last retest {}", r.replace('_', " ")));
            }
            let _ = writeln!(out, "_{}_\n", work.join(" · "));
        }
    }

    let journeys: Vec<_> = audit.checks.iter().filter(|c| c.id == "goal_journey" || c.id == "support_journeys").collect();
    if !journeys.is_empty() {
        let _ = writeln!(out, "## Journeys\n");
        let _ = writeln!(out, "Simulated over the crawl's links — a model of a visitor, not a recording.\n");
        for check in journeys {
            let _ = writeln!(out, "- **{}:** {} — {}", check.title, check.status.label(), check.summary);
            for o in &check.evidence.observed {
                let _ = writeln!(out, "  - {o}");
            }
        }
        let _ = writeln!(out);
    }

    let _ = writeln!(out, "## Responsive and accessibility\n");
    for check in audit.checks.iter().filter(|c| {
        matches!(c.category, super::Category::Responsive | super::Category::Accessibility)
    }) {
        let _ = writeln!(out, "- {} — **{}**: {}", check.title, check.status.label(), check.summary);
    }
    let _ = writeln!(out, "\nAutomated accessibility checks are not a full audit and do not establish legal compliance.\n");

    if let Some(c) = comparison {
        let _ = writeln!(out, "## Before and after\n");
        let _ = writeln!(
            out,
            "Fixed {}, still failing {}, regressed {}{}.\n",
            c.fixed,
            c.still_failing,
            c.regressed,
            c.retest_pass_rate.map(|r| format!("; retest pass rate {:.0}%", r * 100.0)).unwrap_or_default()
        );
        let _ = writeln!(out, "| Check | Before | After | Change |\n|---|---|---|---|");
        for row in c.rows.iter().filter(|r| r.change != "unchanged") {
            let _ = writeln!(
                out,
                "| {} | {} | {} | {} |",
                row.title.replace('|', "/"),
                row.before.map_or("—", Status::label),
                row.after.map_or("—", Status::label),
                row.change.replace('_', " ")
            );
        }
        let _ = writeln!(out);
    }

    let _ = writeln!(out, "## Appendix: what was not tested and why\n");
    for check in audit.checks.iter().filter(|c| matches!(c.status, Status::NotTested | Status::Blocked)) {
        let _ = writeln!(out, "- **{}** ({}): {}", check.title, check.status.label(), check.summary);
    }
    let _ = writeln!(out, "\n### Limitations\n");
    for l in &audit.limitations {
        let _ = writeln!(out, "- {l}");
    }
    let flagged: Vec<_> = states.iter().filter(|s| s.flagged).collect();
    if !flagged.is_empty() {
        let _ = writeln!(out, "\n{} finding(s) were flagged as incorrect by a reviewer and left out of this report.", flagged.len());
    }
    let _ = writeln!(out, "\n_Generated by CrawlStat UX Testing. Severity, priority and page type are rule-based estimates._");
    out
}

fn csv_field(value: &str) -> String {
    if value.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}

/// One row per finding, for a tracker.
pub fn developer_csv(audit: &Audit, states: &[FindingState]) -> String {
    let mut out = String::from(
        "id,title,severity,priority,impact,reach,confidence,effort,status,evidence,method,region,devices,url,problem,change,acceptance,validation,dom_clue,work_status,owner,due,retest,flagged\n",
    );
    let mut findings: Vec<_> = audit.findings().collect();
    findings.sort_by(|a, b| b.1.priority.partial_cmp(&a.1.priority).unwrap_or(std::cmp::Ordering::Equal));
    for (check, f) in findings {
        let st = state(states, &check.id);
        let severity = st
            .and_then(|s| s.severity_override.as_deref())
            .and_then(Severity::parse)
            .unwrap_or(f.severity);
        let fields = [
            check.id.clone(),
            f.title.clone(),
            severity.label().to_string(),
            format!("{:.1}", f.priority),
            f.impact.to_string(),
            f.reach.to_string(),
            f.confidence.to_string(),
            f.effort.to_string(),
            check.status.label().to_string(),
            check.evidence.status.label().to_string(),
            check.method.label().to_string(),
            f.region.clone(),
            f.devices.clone(),
            check.evidence.url.clone(),
            f.recommendation.problem.clone(),
            f.recommendation.change.clone(),
            f.recommendation.acceptance.join("; "),
            f.recommendation.validation.clone(),
            check.evidence.dom_clue.clone().unwrap_or_default(),
            st.map_or("Open", |s| s.work_status.label()).to_string(),
            st.map(|s| s.owner.clone()).unwrap_or_default(),
            st.map(|s| s.due.clone()).unwrap_or_default(),
            st.and_then(|s| s.retest.clone()).unwrap_or_default(),
            st.is_some_and(|s| s.flagged).to_string(),
        ];
        out.push_str(&fields.iter().map(|v| csv_field(v)).collect::<Vec<_>>().join(","));
        out.push('\n');
    }
    out
}
