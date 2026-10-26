//! Laying the report out as one self-contained HTML page.
//!
//! Plain string building rather than a template engine: the page is fixed in
//! shape, every value is escaped through [`esc`], and a dependency would buy
//! nothing but a second syntax. The styles are inline in `<head>` and the
//! charts are CSS boxes or inline SVG, so the file has no outside parts.

use std::fmt::Write as _;

use super::{SiteReport, Speed};
use crate::a11y::{ComplianceReport, Verdict};
use crate::audit::Severity;
use crate::compare::Comparison;
use crate::plan::{Effort, PlanItem};

const DEFAULT_ACCENT: &str = "#4f46e5";

/// Escape text for HTML element content and quoted attributes.
fn esc(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
    out
}

/// Escape, then set the fix texts' `code spans` and *emphasis*, as the app
/// does.
fn prose(text: &str) -> String {
    inline(text)
}

/// 12345 → "12,345".
fn count(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out
}

fn plural(n: u64, one: &str, many: &str) -> String {
    format!("{} {}", count(n), if n == 1 { one } else { many })
}

/// A share as a percentage, rounded *down* to a tenth, like the app: a site
/// with one broken page in three thousand is 99.9%, never 100%.
fn percent(share: f64) -> String {
    let tenths = (share.clamp(0.0, 1.0) * 1000.0).floor() / 10.0;
    if tenths.fract() == 0.0 {
        format!("{tenths:.0}%")
    } else {
        format!("{tenths:.1}%")
    }
}

/// A width for a bar, never so thin a non-zero value disappears.
fn width(part: u64, whole: u64) -> f64 {
    if whole == 0 || part == 0 {
        0.0
    } else {
        (part as f64 / whole as f64 * 100.0).max(1.5)
    }
}

fn ms(value: Option<u64>) -> String {
    match value {
        None => "—".into(),
        Some(ms) if ms >= 1000 => format!("{:.1} s", ms as f64 / 1000.0),
        Some(ms) => format!("{ms} ms"),
    }
}

fn long_date(rfc3339: &str) -> String {
    chrono::DateTime::parse_from_rfc3339(rfc3339)
        .map(|at| at.format("%-d %B %Y").to_string())
        .unwrap_or_else(|_| rfc3339.to_string())
}

/// A page's address without the site's own scheme and host, which every
/// example in the report shares.
fn short_url(url: &str, site: &str) -> String {
    let rest = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
        .unwrap_or(url);
    let rest = rest.strip_prefix("www.").unwrap_or(rest);
    let host = site.strip_prefix("www.").unwrap_or(site);
    match rest.strip_prefix(host) {
        Some("") => "/".into(),
        Some(path) if path.starts_with('/') || path.starts_with('?') => path.into(),
        _ => url.into(),
    }
}

fn accent(option: Option<&str>) -> &str {
    match option {
        Some(value)
            if value.len() == 7
                && value.starts_with('#')
                && value[1..].chars().all(|c| c.is_ascii_hexdigit()) =>
        {
            value
        }
        _ => DEFAULT_ACCENT,
    }
}

fn severity_tone(severity: Severity) -> &'static str {
    match severity {
        Severity::Error => "err",
        Severity::Warning => "warn",
        Severity::Notice => "note",
    }
}

fn severity_label(severity: Severity) -> &'static str {
    match severity {
        Severity::Error => "Error",
        Severity::Warning => "Warning",
        Severity::Notice => "Notice",
    }
}

fn effort_label(effort: Effort) -> &'static str {
    match effort {
        Effort::Template => "One template fix",
        Effort::Bulk => "Fix in bulk",
        Effort::PerPage => "Page by page",
    }
}

fn health_tone(health: Option<f64>) -> &'static str {
    match health {
        None => "mute",
        Some(h) if h >= 0.98 => "ok",
        Some(h) if h >= 0.9 => "warn",
        Some(_) => "err",
    }
}

/// The whole report as one HTML document.
pub fn html(report: &SiteReport) -> String {
    let mut out = String::with_capacity(64 * 1024);
    let _ = write!(
        out,
        "<!DOCTYPE html>\n<html lang=\"en\"><head><meta charset=\"utf-8\">\
         <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\
         <meta name=\"generator\" content=\"CrawlStat\">\
         <title>Site audit · {site}</title><style>{css}</style>\
         <style>:root{{--accent:{accent}}}</style></head><body><main class=\"doc\">",
        site = esc(&report.site),
        css = CSS,
        accent = accent(report.options.accent.as_deref()),
    );
    cover(&mut out, report);
    if let Some(text) = report
        .options
        .narrative
        .as_deref()
        .filter(|t| !t.trim().is_empty())
    {
        narrative(&mut out, text);
    }
    standing(&mut out, report);
    if !report.plan.items.is_empty() {
        jobs(&mut out, report);
    }
    if let Some(comparison) = &report.comparison {
        changes(&mut out, report, comparison);
    }
    if let Some(accessibility) = &report.accessibility {
        accessibility_section(&mut out, report, accessibility);
    }
    appendix(&mut out, report);
    out.push_str("</main></body></html>\n");
    out
}

fn sheet_head(out: &mut String, number: usize, eyebrow: &str, title: &str, lede: &str) {
    let _ = write!(
        out,
        "<header class=\"sheet-head\"><p class=\"eyebrow\"><span class=\"num\">{number:02}</span>{eyebrow}</p>\
         <h2>{title}</h2><p class=\"lede\">{lede}</p></header>",
        eyebrow = esc(eyebrow),
        title = esc(title),
    );
}

// ─── summary ─────────────────────────────────────────────────────────────

/// The report the local model wrote, set in the page's own type. The model is
/// asked for a small subset of Markdown — headings, lists, paragraphs, bold,
/// italics and code — and this handles exactly that, as the app's own
/// renderer does. Everything is escaped first, so nothing it writes can
/// become markup.
fn narrative(out: &mut String, markdown: &str) {
    let mut title = None;
    let mut body = String::new();
    let mut paragraph: Vec<&str> = Vec::new();
    // `Some(true)` inside a numbered list, `Some(false)` inside bullets.
    let mut list: Option<bool> = None;

    fn flush(body: &mut String, paragraph: &mut Vec<&str>) {
        if !paragraph.is_empty() {
            let _ = write!(body, "<p>{}</p>", inline(&paragraph.join(" ")));
            paragraph.clear();
        }
    }
    fn close(body: &mut String, list: &mut Option<bool>) {
        if let Some(ordered) = list.take() {
            body.push_str(if ordered { "</ol>" } else { "</ul>" });
        }
    }

    for raw in markdown.lines() {
        let line = raw.trim();
        if line.is_empty() {
            flush(&mut body, &mut paragraph);
            close(&mut body, &mut list);
            continue;
        }
        let hashes = line.chars().take_while(|&c| c == '#').count();
        if (1..=6).contains(&hashes) && line[hashes..].starts_with(' ') {
            flush(&mut body, &mut paragraph);
            close(&mut body, &mut list);
            let text = line[hashes..].replace(['#', '*'], "");
            let text = text.trim();
            if hashes == 1 && title.is_none() && body.is_empty() {
                title = Some(text.to_string());
            } else {
                let _ = write!(body, "<h3>{}</h3>", esc(text));
            }
            continue;
        }
        if let Some((ordered, item)) = list_item(line) {
            flush(&mut body, &mut paragraph);
            if list != Some(ordered) {
                close(&mut body, &mut list);
                body.push_str(if ordered { "<ol>" } else { "<ul>" });
                list = Some(ordered);
            }
            let _ = write!(body, "<li>{}</li>", inline(item));
            continue;
        }
        // A wrapped line under a list item belongs to that item.
        if list.is_some() && paragraph.is_empty() && raw.starts_with(' ') {
            if body.ends_with("</li>") {
                body.truncate(body.len() - "</li>".len());
                let _ = write!(body, " {}</li>", inline(line));
                continue;
            }
        }
        close(&mut body, &mut list);
        paragraph.push(line);
    }
    flush(&mut body, &mut paragraph);
    close(&mut body, &mut list);

    let _ = write!(
        out,
        "<section class=\"sheet\"><header class=\"sheet-head\"><p class=\"eyebrow\">Summary</p>\
         <h2>{title}</h2></header><div class=\"narrative\">{body}</div></section>",
        title = esc(title.as_deref().unwrap_or("In brief")),
    );
}

/// `- item`, `* item`, `• item`, `1. item` or `1) item`, as (numbered, text).
fn list_item(line: &str) -> Option<(bool, &str)> {
    for bullet in ["- ", "* ", "• "] {
        if let Some(rest) = line.strip_prefix(bullet) {
            return Some((false, rest.trim()));
        }
    }
    let digits = line.chars().take_while(char::is_ascii_digit).count();
    let rest = &line[digits..];
    if digits > 0 && (rest.starts_with(". ") || rest.starts_with(") ")) {
        return Some((true, rest[2..].trim()));
    }
    None
}

/// Escape, then `**bold**`, `*italics*` / `_italics_` and `` `code` ``.
fn inline(text: &str) -> String {
    marks(&esc(text))
}

/// [inline] on text that is already escaped, so bold can hold code.
fn marks(escaped: &str) -> String {
    let mut out = String::with_capacity(escaped.len() + 16);
    let mut rest = escaped;
    while !rest.is_empty() {
        if let Some(after) = rest.strip_prefix("**") {
            // An unclosed `**` runs bold to the end, as in the app.
            let end = after.find("**").unwrap_or(after.len());
            let _ = write!(out, "<strong>{}</strong>", marks(&after[..end]));
            rest = after.get(end + 2..).unwrap_or("");
        } else if let Some(after) = rest.strip_prefix('`') {
            match after.find('`') {
                Some(end) => {
                    let _ = write!(out, "<code>{}</code>", &after[..end]);
                    rest = &after[end + 1..];
                }
                None => {
                    out.push('`');
                    rest = after;
                }
            }
        } else if let Some(mark) = rest.chars().next().filter(|c| *c == '*' || *c == '_') {
            let starts_word = !out.ends_with(|c: char| c.is_alphanumeric());
            let after = &rest[1..];
            match after.find(mark) {
                Some(end) if starts_word && end > 0 && !after.starts_with(' ') => {
                    let _ = write!(out, "<em>{}</em>", &after[..end]);
                    rest = &after[end + 1..];
                }
                _ => {
                    out.push(mark);
                    rest = after;
                }
            }
        } else {
            // Never zero: every branch above takes a leading marker.
            let next = rest.find(['*', '_', '`']).unwrap_or(rest.len());
            out.push_str(&rest[..next]);
            rest = &rest[next..];
        }
    }
    out
}

// ─── cover ───────────────────────────────────────────────────────────────

fn ring(health: Option<f64>) -> String {
    let r = 74.0_f64;
    let circumference = 2.0 * std::f64::consts::PI * r;
    let filled = circumference * health.unwrap_or(0.0).clamp(0.0, 1.0);
    let label = health.map(percent).unwrap_or_else(|| "—".into());
    format!(
        "<svg class=\"ring tone-{tone}\" viewBox=\"0 0 180 180\" role=\"img\" aria-label=\"Health {label}\">\
         <circle class=\"track\" cx=\"90\" cy=\"90\" r=\"{r}\"/>\
         <circle class=\"arc\" cx=\"90\" cy=\"90\" r=\"{r}\" stroke-dasharray=\"{filled:.2} {circumference:.2}\" transform=\"rotate(-90 90 90)\"/>\
         <text x=\"90\" y=\"92\" class=\"ring-value\">{label}</text>\
         <text x=\"90\" y=\"118\" class=\"ring-caption\">healthy</text></svg>",
        tone = health_tone(health),
    )
}

fn cover(out: &mut String, report: &SiteReport) {
    let stats = &report.stats;
    let plan = &report.plan;
    let health = report.health();
    let crawled = report.crawled_at.as_deref().map(long_date);
    let broken = stats.status_4xx + stats.status_5xx + stats.failed;

    let mut summary = format!(
        "We crawled {} on {}.",
        plural(stats.done, "URL", "URLs"),
        esc(&report.site)
    );
    if let Some(h) = health {
        let _ = write!(summary, " {} of them answered as they should", percent(h));
        if broken > 0 {
            let _ = write!(
                summary,
                "; {} broken or never answered.",
                if broken == 1 {
                    "one was".to_string()
                } else {
                    format!("{} were", count(broken))
                }
            );
        } else {
            summary.push('.');
        }
    }
    if plan.total_pages > 0 {
        let _ = write!(
            summary,
            " {} of {} pages carry at least one issue",
            count(plan.affected_pages),
            count(plan.total_pages)
        );
        let _ = match plan.items.len() {
            0 => write!(summary, "."),
            n => write!(
                summary,
                ", and the plan inside ranks {} by how much of the site each one touches.",
                plural(n as u64, "job", "jobs")
            ),
        };
    }

    let _ = write!(
        out,
        "<section class=\"sheet cover\"><div class=\"brandbar\"><span class=\"brand\"><span class=\"mark\"></span>CrawlStat</span>\
         <span class=\"brand-meta\">Site audit{when}</span></div>\
         <div class=\"cover-body\"><p class=\"eyebrow\">Technical SEO &amp; accessibility audit</p>\
         <h1>{site}</h1>{start}\
         <div class=\"hero\">{ring}<div class=\"hero-text\"><p class=\"verdict tone-{tone}\">{verdict}</p>\
         <p class=\"summary\">{summary}</p>{js}</div></div>",
        when = crawled.as_deref().map(|d| format!(" · {}", esc(d))).unwrap_or_default(),
        site = esc(&report.site),
        start = report
            .start_url
            .as_deref()
            .map(|url| format!("<p class=\"start\">{}</p>", esc(url)))
            .unwrap_or_default(),
        ring = ring(health),
        tone = health_tone(health),
        verdict = report.verdict(),
        js = if report.rendered_javascript {
            "<p class=\"chip\">JavaScript rendered in Chrome</p>"
        } else {
            ""
        },
    );

    out.push_str("<div class=\"kpis\">");
    kpi(
        out,
        "URLs crawled",
        &count(stats.done),
        &format!("{} answered 200–299", count(stats.status_2xx)),
        "accent",
        None,
    );
    kpi(
        out,
        "Broken or failed",
        &count(broken),
        "4xx, 5xx and requests that never answered",
        if broken > 0 { "err" } else { "ok" },
        None,
    );
    let affected_share = if plan.total_pages == 0 {
        0.0
    } else {
        plan.affected_pages as f64 / plan.total_pages as f64
    };
    kpi(
        out,
        "Pages with issues",
        &percent(affected_share),
        &format!(
            "{} of {} pages",
            count(plan.affected_pages),
            count(plan.total_pages)
        ),
        "warn",
        Some(affected_share),
    );
    kpi(
        out,
        "Link value at risk",
        &percent(plan.value_at_risk),
        "of internal link value sits on pages with errors",
        if plan.value_at_risk > 0.0 {
            "err"
        } else {
            "ok"
        },
        Some(plan.value_at_risk),
    );
    out.push_str("</div></div>");

    let _ = write!(
        out,
        "<footer class=\"cover-foot\"><span>{prepared}</span><span>Generated {generated} with CrawlStat</span></footer></section>",
        prepared = report
            .options
            .prepared_by
            .as_deref()
            .filter(|p| !p.trim().is_empty())
            .map(|p| format!("Prepared by <strong>{}</strong>", esc(p.trim())))
            .unwrap_or_default(),
        generated = report.generated_at.format("%-d %B %Y"),
    );
}

fn kpi(out: &mut String, label: &str, value: &str, sub: &str, tone: &str, share: Option<f64>) {
    let bar = share
        .map(|s| {
            format!(
                "<span class=\"meter\"><span style=\"width:{:.1}%\"></span></span>",
                (s.clamp(0.0, 1.0) * 100.0).max(if s > 0.0 { 1.5 } else { 0.0 })
            )
        })
        .unwrap_or_default();
    let _ = write!(
        out,
        "<div class=\"kpi tone-{tone}\"><p class=\"kpi-label\"><span class=\"dot\"></span>{label}</p>\
         <p class=\"kpi-value\">{value}</p>{bar}<p class=\"kpi-sub\">{sub}</p></div>",
        label = esc(label),
        value = esc(value),
        sub = esc(sub),
    );
}

// ─── where things stand ──────────────────────────────────────────────────

fn standing(out: &mut String, report: &SiteReport) {
    let stats = &report.stats;
    out.push_str("<section class=\"sheet\">");
    sheet_head(
        out,
        1,
        "Overview",
        "Where things stand",
        "How the site answered, how fast, how deep its pages sit, and how many kinds of problem the audit found.",
    );

    // Responses: one stacked bar, the legend underneath carries the numbers.
    let parts = [
        ("Success", "2xx", stats.status_2xx, "ok"),
        ("Redirect", "3xx", stats.status_3xx, "note"),
        ("Client error", "4xx", stats.status_4xx, "warn"),
        ("Server error", "5xx", stats.status_5xx, "err"),
        ("No answer", "failed", stats.failed, "mute"),
    ];
    let total: u64 = parts.iter().map(|p| p.2).sum();
    out.push_str("<div class=\"card\"><h3>Responses</h3><div class=\"stack\">");
    for (label, _, n, tone) in parts.iter().filter(|p| p.2 > 0) {
        let _ = write!(
            out,
            "<span class=\"seg tone-{tone}\" style=\"width:{:.2}%\" title=\"{label}\"></span>",
            width(*n, total)
        );
    }
    out.push_str("</div><div class=\"legend\">");
    for (label, code, n, tone) in parts {
        let _ = write!(
            out,
            "<div class=\"legend-item{dim}\"><span class=\"swatch tone-{tone}\"></span>\
             <span class=\"legend-label\">{label} <span class=\"muted\">{code}</span></span>\
             <span class=\"legend-value\">{n}</span><span class=\"legend-share\">{share}</span></div>",
            dim = if n == 0 { " dim" } else { "" },
            n = count(n),
            share = if total == 0 { "—".into() } else { percent(n as f64 / total as f64) },
        );
    }
    out.push_str("</div></div>");

    out.push_str("<div class=\"grid-3\">");
    // Issue types by severity.
    let audit = &report.audit;
    out.push_str("<div class=\"card\"><h3>Issue types found</h3><div class=\"triple\">");
    for (label, n, tone) in [
        ("Errors", audit.errors, "err"),
        ("Warnings", audit.warnings, "warn"),
        ("Notices", audit.notices, "note"),
    ] {
        let _ = write!(
            out,
            "<div class=\"mini tone-{tone}\"><span class=\"mini-value\">{}</span><span class=\"mini-label\"><span class=\"dot\"></span>{label}</span></div>",
            count(n)
        );
    }
    out.push_str("</div><p class=\"note\">Kinds of problem, not pages. The appendix lists each one with the pages it affects.</p></div>");

    speed_card(out, &report.speed);

    // Indexability.
    let indexable = audit.indexable_pages;
    let blocked = audit.non_indexable_pages;
    let _ = write!(
        out,
        "<div class=\"card\"><h3>Indexable pages</h3><p class=\"big\">{share}</p>\
         <div class=\"stack thin\"><span class=\"seg tone-ok\" style=\"width:{a:.2}%\"></span><span class=\"seg tone-mute\" style=\"width:{b:.2}%\"></span></div>\
         <p class=\"note\">{indexable} can appear in search results; {blocked} kept out by noindex, a canonical elsewhere or a robots rule.</p></div>",
        indexable = plural(indexable, "page", "pages"),
        blocked = match blocked {
            1 => "1 is".to_string(),
            n => format!("{} are", count(n)),
        },
        share = if indexable + blocked == 0 {
            "—".into()
        } else {
            percent(indexable as f64 / (indexable + blocked) as f64)
        },
        a = width(indexable, indexable + blocked),
        b = width(blocked, indexable + blocked),
    );
    out.push_str("</div>");

    depth_card(out, &audit.depth_distribution);
    out.push_str("</section>");
}

fn speed_card(out: &mut String, speed: &Speed) {
    let tone = match speed.median_ms {
        None => "mute",
        Some(ms) if ms >= speed.slow_ms => "err",
        Some(ms) if ms * 2 >= speed.slow_ms => "warn",
        Some(_) => "ok",
    };
    let _ = write!(
        out,
        "<div class=\"card\"><h3>Response time</h3><p class=\"big tone-text-{tone}\">{median}<span class=\"unit\"> median</span></p>\
         <dl class=\"facts\"><div><dt>Slowest 10% from</dt><dd>{p90}</dd></div>\
         <div><dt>Slower than {slow}</dt><dd>{slow_pages}</dd></div></dl>\
         <p class=\"note\">Server response for {pages}, measured during the crawl.</p></div>",
        median = ms(speed.median_ms),
        p90 = ms(speed.p90_ms),
        slow = ms(Some(speed.slow_ms)),
        slow_pages = plural(speed.slow_pages, "page", "pages"),
        pages = plural(speed.pages, "page", "pages"),
    );
}

fn depth_card(out: &mut String, distribution: &[(u32, u64)]) {
    if distribution.is_empty() {
        return;
    }
    // Everything past eight clicks is one column: the shape, not the tail,
    // is the finding.
    let mut columns: Vec<(String, u64)> = Vec::new();
    for &(depth, pages) in distribution {
        if depth >= 8 {
            match columns.last_mut() {
                Some((label, n)) if label == "8+" => *n += pages,
                _ => columns.push(("8+".into(), pages)),
            }
        } else {
            columns.push((depth.to_string(), pages));
        }
    }
    let max = columns.iter().map(|c| c.1).max().unwrap_or(1).max(1);
    let total: u64 = columns.iter().map(|c| c.1).sum();
    let deep: u64 = distribution.iter().filter(|d| d.0 > 3).map(|d| d.1).sum();
    out.push_str("<div class=\"card\"><h3>Clicks from the home page</h3><div class=\"columns\">");
    for (label, pages) in &columns {
        let _ = write!(
            out,
            "<div class=\"col\"><span class=\"col-value\">{n}</span><span class=\"col-bar\" style=\"height:{h:.1}%\"></span><span class=\"col-label\">{label}</span></div>",
            n = count(*pages),
            h = (*pages as f64 / max as f64 * 100.0).max(2.0),
        );
    }
    let _ = write!(
        out,
        "</div><p class=\"note\">{} more than three clicks from the home page. Pages that deep are found and revisited less often.</p></div>",
        if total == 0 {
            "No pages sit".to_string()
        } else {
            format!("{} of pages sit", percent(deep as f64 / total as f64))
        }
    );
}

// ─── what to fix first ───────────────────────────────────────────────────

fn jobs(out: &mut String, report: &SiteReport) {
    let plan = &report.plan;
    let shown = plan.items.len().min(report.options.max_jobs.max(1));
    out.push_str("<section class=\"sheet\">");
    sheet_head(
        out,
        2,
        "Action plan",
        "What to fix first",
        &format!(
            "The {} with the most effect, ranked by severity, by how much of the site's internal link value the affected pages hold, and by how many pages one change repairs.",
            if shown == 1 { "job".to_string() } else { format!("{shown} jobs") }
        ),
    );
    out.push_str("<ol class=\"jobs\">");
    for (index, item) in plan.items.iter().take(shown).enumerate() {
        job(out, index + 1, item, &report.site);
    }
    out.push_str("</ol>");
    if plan.items.len() > shown {
        let _ = write!(
            out,
            "<p class=\"note center\">{} more in the appendix.</p>",
            plural((plan.items.len() - shown) as u64, "issue", "issues")
        );
    }
    out.push_str("</section>");
}

fn job(out: &mut String, rank: usize, item: &PlanItem, site: &str) {
    let tone = severity_tone(item.severity);
    let one_change = if item.distinct_fixes > 0 && item.pages > item.distinct_fixes {
        format!(
            " · {} {} {}",
            plural(item.distinct_fixes, "change", "changes"),
            if item.distinct_fixes == 1 {
                "fixes"
            } else {
                "fix"
            },
            plural(item.pages, "page", "pages")
        )
    } else {
        String::new()
    };
    let _ = write!(
        out,
        "<li class=\"job tone-{tone}\"><div class=\"rank\">{rank:02}</div><div class=\"job-body\">\
         <div class=\"job-top\"><h3>{title}</h3><span class=\"pill tone-{tone}\">{severity}</span>\
         {effort}</div>\
         <p class=\"job-meta\">{pages}{one_change} · reaches {reach} of link value</p>\
         <div class=\"impact\"><span class=\"impact-label\">Impact</span><span class=\"meter\"><span style=\"width:{impact:.1}%\"></span></span><span class=\"impact-value\">{impact:.0}</span></div>\
         <p class=\"fix\">{fix}</p>",
        title = esc(&item.title),
        severity = severity_label(item.severity),
        effort = format!(
            "<span class=\"pill {}\">{}</span>",
            if item.effort == Effort::Template { "tone-accent" } else { "plain" },
            effort_label(item.effort)
        ),
        pages = plural(item.pages, "page", "pages"),
        reach = percent(item.reach),
        impact = item.impact.clamp(0.0, 100.0),
        fix = prose(&item.how_to_fix),
    );
    if !item.examples.is_empty() {
        out.push_str("<ul class=\"examples\">");
        for example in item.examples.iter().take(3) {
            let _ = write!(
                out,
                "<li><span class=\"url\">{}</span>{}</li>",
                esc(&short_url(&example.url, site)),
                example
                    .detail
                    .as_deref()
                    .filter(|d| !d.is_empty())
                    .map(|d| format!("<span class=\"detail\">{}</span>", esc(d)))
                    .unwrap_or_default()
            );
        }
        out.push_str("</ul>");
    }
    out.push_str("</div></li>");
}

// ─── since the last crawl ────────────────────────────────────────────────

fn changes(out: &mut String, report: &SiteReport, comparison: &Comparison) {
    out.push_str("<section class=\"sheet\">");
    sheet_head(
        out,
        3,
        "Progress",
        "Since the last crawl",
        &format!(
            "Compared with the previous crawl of {}: {} then, {} now.",
            esc(&report.site),
            plural(comparison.pages_before, "page", "pages"),
            plural(comparison.pages_after, "page", "pages")
        ),
    );
    let fixed = comparison.total_fixed();
    let introduced = comparison.total_introduced();
    let _ = write!(
        out,
        "<div class=\"grid-3\">\
         <div class=\"card stat tone-ok\"><p class=\"kpi-label\"><span class=\"dot\"></span>Fixed</p><p class=\"kpi-value\">{fixed}</p><p class=\"kpi-sub\">issue instances gone from pages still on the site</p></div>\
         <div class=\"card stat tone-err\"><p class=\"kpi-label\"><span class=\"dot\"></span>New</p><p class=\"kpi-value\">{introduced}</p><p class=\"kpi-sub\">issue instances that were not there before</p></div>\
         <div class=\"card stat tone-accent\"><p class=\"kpi-label\"><span class=\"dot\"></span>Pages with issues</p><p class=\"kpi-value\">{before}<span class=\"arrow\">→</span>{after}</p><p class=\"kpi-sub\">{added} added, {removed} removed</p></div></div>",
        fixed = count(fixed),
        introduced = count(introduced),
        before = count(comparison.affected_before),
        after = count(comparison.affected_after),
        added = plural(comparison.pages_added.len() as u64, "page", "pages"),
        removed = plural(comparison.pages_removed.len() as u64, "page", "pages"),
    );

    let improvements = comparison.improvements();
    let regressions = comparison.regressions();
    let scale = comparison
        .issues
        .iter()
        .map(|issue| issue.before.max(issue.after))
        .max()
        .unwrap_or(1)
        .max(1);
    out.push_str("<div class=\"grid-2\">");
    for (title, list, tone, empty) in [
        (
            "Better",
            &improvements,
            "ok",
            "Nothing improved between the two crawls.",
        ),
        (
            "Worse",
            &regressions,
            "err",
            "Nothing got worse. Well done.",
        ),
    ] {
        let _ = write!(out, "<div class=\"card\"><h3>{title}</h3>");
        if list.is_empty() {
            let _ = write!(out, "<p class=\"note\">{empty}</p>");
        } else {
            out.push_str("<ul class=\"moves\">");
            for issue in list.iter().take(6) {
                let net = issue.net();
                let _ = write!(
                    out,
                    "<li><span class=\"move-title\">{title}</span>\
                     <span class=\"move-bars\"><span class=\"was\" style=\"width:{was:.1}%\"></span><span class=\"now tone-{tone}\" style=\"width:{now:.1}%\"></span></span>\
                     <span class=\"move-figures\">{before} → {after}</span><span class=\"delta tone-{tone}\">{sign}{net}</span></li>",
                    title = esc(&issue.title),
                    was = width(issue.before, scale),
                    now = width(issue.after, scale),
                    before = count(issue.before),
                    after = count(issue.after),
                    sign = if net > 0 { "+" } else { "−" },
                    net = count(net.unsigned_abs()),
                );
            }
            out.push_str("</ul>");
        }
        out.push_str("</div>");
    }
    out.push_str("</div>");

    let broken: Vec<_> = comparison
        .pages_broken
        .iter()
        .filter(|p| p.broke())
        .collect();
    if !broken.is_empty() {
        out.push_str(
            "<div class=\"card alert\"><h3>Pages that stopped working</h3><ul class=\"broken\">",
        );
        for page in broken.iter().take(10) {
            let status = |s: Option<u16>| {
                s.map(|c| c.to_string())
                    .unwrap_or_else(|| "no answer".into())
            };
            let _ = write!(
                out,
                "<li><span class=\"url\">{}</span><span class=\"codes\">{} → <strong>{}</strong></span></li>",
                esc(&short_url(&page.url, &report.site)),
                status(page.status_before),
                status(page.status_after),
            );
        }
        if broken.len() > 10 {
            let _ = write!(
                out,
                "<li class=\"muted\">and {} more</li>",
                count((broken.len() - 10) as u64)
            );
        }
        out.push_str("</ul></div>");
    }
    out.push_str("</section>");
}

// ─── accessibility ───────────────────────────────────────────────────────

fn accessibility_section(out: &mut String, report: &SiteReport, a11y: &ComplianceReport) {
    let number = if report.comparison.is_some() { 4 } else { 3 };
    out.push_str("<section class=\"sheet\">");
    sheet_head(
        out,
        number,
        "Accessibility",
        &format!("Against {}", a11y.standard.label()),
        &format!(
            "Automated checks over {}. They find many barriers, but cannot prove conformance on their own: criteria marked for review need a person.",
            plural(a11y.coverage.pages_checked as u64, "page", "pages")
        ),
    );
    let (tone, verdict) = match a11y.verdict {
        Verdict::DoesNotConform => ("err", "Not compliant"),
        Verdict::Incomplete => ("warn", "Needs review"),
        Verdict::Conforms => ("ok", "Conforms"),
        Verdict::NotChecked => ("mute", "Not checked"),
    };
    let counts = a11y.counts;
    let passed = counts.pass + counts.checks_passed;
    let parts = [
        ("Failing", counts.fail, "err"),
        ("To review", counts.review + counts.untested, "warn"),
        ("Passing", passed, "ok"),
        ("Not applicable", counts.not_applicable, "mute"),
    ];
    let total: u64 = parts.iter().map(|p| p.1 as u64).sum();
    let _ = write!(
        out,
        "<div class=\"card verdict-card tone-{tone}\"><div class=\"verdict-top\"><span class=\"pill tone-{tone} large\">{verdict}</span>\
         <div><p class=\"verdict-headline\">{headline}</p><p class=\"note\">{detail}</p></div></div><div class=\"stack\">",
        headline = esc(&a11y.headline),
        detail = esc(&a11y.detail),
    );
    for (label, n, tone) in parts.iter().filter(|p| p.1 > 0) {
        let _ = write!(
            out,
            "<span class=\"seg tone-{tone}\" style=\"width:{:.2}%\" title=\"{label}\"></span>",
            width(*n as u64, total)
        );
    }
    out.push_str("</div><div class=\"legend four\">");
    for (label, n, tone) in parts {
        let _ = write!(
            out,
            "<div class=\"legend-item{dim}\"><span class=\"swatch tone-{tone}\"></span><span class=\"legend-label\">{label}</span>\
             <span class=\"legend-value\">{n}</span></div>",
            dim = if n == 0 { " dim" } else { "" },
            n = plural(n as u64, "criterion", "criteria"),
        );
    }
    out.push_str("</div></div>");

    if !a11y.rules.is_empty() {
        out.push_str("<div class=\"card\"><h3>Biggest barriers</h3><ul class=\"barriers\">");
        for rule in a11y.rules.iter().take(6) {
            let _ = write!(
                out,
                "<li><div class=\"barrier-top\"><span class=\"impact-dot impact-{impact}\"></span><span class=\"barrier-title\">{title}</span>{template}\
                 <span class=\"barrier-pages\">{pages}</span></div>\
                 <div class=\"barrier-bar\"><span class=\"meter\"><span style=\"width:{share:.1}%\"></span></span><span class=\"barrier-share\">{share_label}</span></div>\
                 <p class=\"note\">Affects {affects}. {fix}</p></li>",
                impact = rule.info.impact.as_str(),
                title = esc(rule.info.title),
                template = if rule.template_fix {
                    "<span class=\"pill tone-accent\">One template fix</span>"
                } else {
                    ""
                },
                pages = plural(rule.pages as u64, "page", "pages"),
                share = (rule.share.clamp(0.0, 1.0) * 100.0).max(1.5),
                share_label = percent(rule.share),
                affects = esc(rule.info.affects),
                fix = prose(rule.info.fix),
            );
        }
        out.push_str("</ul></div>");
    }
    out.push_str("</section>");
}

// ─── appendix ────────────────────────────────────────────────────────────

fn appendix(out: &mut String, report: &SiteReport) {
    let issues = &report.audit.issues;
    out.push_str("<section class=\"sheet\">");
    let number =
        3 + usize::from(report.comparison.is_some()) + usize::from(report.accessibility.is_some());
    sheet_head(
        out,
        number,
        "Appendix",
        "Every issue found",
        &format!(
            "{} across the crawl, worst first. Page counts can overlap: one page can carry several issues.",
            plural(issues.len() as u64, "kind of issue", "kinds of issue")
        ),
    );
    if issues.is_empty() {
        out.push_str(
            "<div class=\"card\"><p class=\"big tone-text-ok\">No issues found.</p></div>",
        );
    } else {
        let max = issues.iter().map(|i| i.url_count).max().unwrap_or(1).max(1);
        out.push_str("<table class=\"issues\"><thead><tr><th class=\"sev\"></th><th>Issue</th><th class=\"num\">Pages</th><th class=\"barcol\"></th></tr></thead><tbody>");
        for issue in issues {
            let tone = severity_tone(issue.severity);
            let _ = write!(
                out,
                "<tr><td class=\"sev\"><span class=\"pill tone-{tone} small\">{severity}</span></td><td>{title}</td>\
                 <td class=\"num\">{pages}</td><td class=\"barcol\"><span class=\"meter tone-{tone}\"><span style=\"width:{w:.1}%\"></span></span></td></tr>",
                severity = severity_label(issue.severity),
                title = esc(&issue.title),
                pages = count(issue.url_count),
                w = width(issue.url_count, max),
            );
        }
        out.push_str("</tbody></table>");
    }

    let _ = write!(
        out,
        "<div class=\"method\"><h3>How this report was made</h3><ul>\
         <li>Every figure comes from one crawl of {site}{when}, {js}.</li>\
         <li>Health is the share of crawled URLs that answered with a success or a redirect.</li>\
         <li>Impact scores rank work against the rest of this crawl. They are not a traffic forecast: CrawlStat has not seen the site's analytics.</li>\
         <li>Automated accessibility checks cover what markup can show. Keyboard use, reading order and meaning still need a person.</li></ul></div></section>",
        site = esc(&report.site),
        when = report
            .crawled_at
            .as_deref()
            .map(|d| format!(" on {}", esc(&long_date(d))))
            .unwrap_or_default(),
        js = if report.rendered_javascript {
            "with each page's JavaScript run in headless Chrome"
        } else {
            "reading the HTML as served, without running JavaScript"
        },
    );
}

const CSS: &str = r#"
*{box-sizing:border-box;margin:0;padding:0}
:root{--ink:#11131a;--ink-2:#3b414d;--muted:#6b7280;--faint:#9aa1ad;--line:#e6e8ee;--soft:#f5f6f9;--canvas:#eceef3;
--ok:#16915a;--warn:#c47a0a;--err:#d6403f;--note:#2f78d0;--mute:#9aa1ad}
html{-webkit-print-color-adjust:exact;print-color-adjust:exact}
body{background:var(--canvas);color:var(--ink);font:14px/1.55 "Inter",-apple-system,BlinkMacSystemFont,"Segoe UI",Roboto,"Helvetica Neue",Arial,sans-serif;
font-feature-settings:"cv11","ss01";-webkit-font-smoothing:antialiased}
.doc{max-width:210mm;margin:0 auto;padding:28px 0 64px}
.sheet{background:#fff;border-radius:22px;padding:15mm 15mm 16mm;margin-bottom:28px;box-shadow:0 1px 2px rgba(17,19,26,.05),0 18px 40px -18px rgba(17,19,26,.18)}
h1,h2,h3{letter-spacing:-.02em;line-height:1.15}
h2{font-size:30px;font-weight:700;margin:6px 0 8px}
h3{font-size:15px;font-weight:650;margin-bottom:12px}
code{font:12px/1.4 ui-monospace,"SF Mono",Menlo,Consolas,monospace;background:var(--soft);border:1px solid var(--line);border-radius:5px;padding:0 4px}
.muted{color:var(--faint)}
.note{color:var(--muted);font-size:12.5px;margin-top:10px}
.center{text-align:center}
.eyebrow{display:flex;align-items:center;gap:10px;font-size:11px;font-weight:650;letter-spacing:.14em;text-transform:uppercase;color:var(--accent)}
.eyebrow .num{display:inline-grid;place-items:center;min-width:28px;height:22px;border-radius:7px;background:color-mix(in srgb,var(--accent) 12%,#fff);font-variant-numeric:tabular-nums;letter-spacing:0}
.sheet-head{margin-bottom:22px;padding-bottom:18px;border-bottom:1px solid var(--line)}
.lede{color:var(--muted);max-width:56ch;font-size:14.5px}
.narrative{max-width:68ch;font-size:14.5px;line-height:1.65}
.narrative p{margin:0 0 12px}
.narrative h3{font-size:17px;margin:22px 0 8px}
.narrative h3:first-child{margin-top:0}
.narrative ol,.narrative ul{margin:0 0 14px;padding-left:22px}
.narrative li{margin:0 0 6px;padding-left:4px}
.narrative li::marker{color:var(--accent);font-weight:700}
.narrative strong{font-weight:700}
.tone-ok{--tone:var(--ok)}.tone-warn{--tone:var(--warn)}.tone-err{--tone:var(--err)}.tone-note{--tone:var(--note)}.tone-mute{--tone:var(--mute)}.tone-accent{--tone:var(--accent)}
.tone-text-ok{color:var(--ok)}.tone-text-warn{color:var(--warn)}.tone-text-err{color:var(--err)}.tone-text-mute{color:var(--faint)}
.dot{display:inline-block;width:8px;height:8px;border-radius:50%;background:var(--tone,var(--accent));margin-right:8px;vertical-align:1px}
.pill{display:inline-flex;align-items:center;height:22px;padding:0 9px;border-radius:999px;font-size:11.5px;font-weight:600;white-space:nowrap;
color:color-mix(in srgb,var(--tone) 82%,#000);background:color-mix(in srgb,var(--tone) 13%,#fff)}
.pill.plain{color:var(--ink-2);background:var(--soft);border:1px solid var(--line)}
.pill.small{height:19px;font-size:10.5px;padding:0 7px}
.pill.large{height:30px;font-size:13px;padding:0 14px}
.meter{display:block;position:relative;height:6px;border-radius:99px;background:var(--soft);overflow:hidden;flex:1}
.meter>span{position:absolute;inset:0 auto 0 0;border-radius:99px;background:var(--tone,var(--accent))}
/* cover */
.cover{display:flex;flex-direction:column;min-height:273mm;padding-top:12mm;position:relative;overflow:hidden}
.cover::before{content:"";position:absolute;inset:-30% -30% auto auto;width:150mm;height:150mm;border-radius:50%;
background:radial-gradient(circle,color-mix(in srgb,var(--accent) 16%,transparent) 0,transparent 68%);pointer-events:none}
.brandbar{display:flex;justify-content:space-between;align-items:center;font-size:12.5px;color:var(--muted);position:relative}
.brand{display:flex;align-items:center;gap:9px;font-weight:700;color:var(--ink);font-size:14px;letter-spacing:-.01em}
.mark{width:22px;height:22px;border-radius:7px;background:linear-gradient(135deg,var(--accent),color-mix(in srgb,var(--accent) 55%,#fff))}
.cover-body{flex:1;display:flex;flex-direction:column;justify-content:center;position:relative;padding:14mm 0 8mm}
.cover h1{font-size:50px;font-weight:750;letter-spacing:-.035em;margin:12px 0 6px;word-break:break-word}
.start{color:var(--muted);font-size:14px}
.hero{display:flex;align-items:center;gap:30px;margin:13mm 0 12mm}
.ring{width:180px;height:180px;flex:none}
.ring .track{fill:none;stroke:var(--soft);stroke-width:14}
.ring .arc{fill:none;stroke:var(--tone);stroke-width:14;stroke-linecap:round}
.ring-value{font-size:34px;font-weight:750;text-anchor:middle;fill:var(--ink);letter-spacing:-.03em;font-variant-numeric:tabular-nums}
.ring-caption{font-size:12px;text-anchor:middle;fill:var(--muted);letter-spacing:.08em;text-transform:uppercase}
.verdict{font-size:26px;font-weight:700;color:var(--tone);letter-spacing:-.02em;margin-bottom:8px}
.summary{color:var(--ink-2);font-size:15.5px;max-width:50ch}
.chip{display:inline-block;margin-top:14px;font-size:12px;color:var(--ink-2);background:var(--soft);border:1px solid var(--line);border-radius:99px;padding:3px 11px}
.kpis{display:grid;grid-template-columns:repeat(4,1fr);gap:12px}
.kpi,.stat{border:1px solid var(--line);border-radius:16px;padding:15px 16px;background:#fff;display:flex;flex-direction:column}
.kpi-label{font-size:11.5px;font-weight:600;color:var(--muted);text-transform:uppercase;letter-spacing:.06em}
.kpi-value{font-size:30px;font-weight:720;letter-spacing:-.03em;margin:6px 0 4px;font-variant-numeric:tabular-nums}
.kpi .meter{flex:none;margin:2px 0 8px}
.kpi-sub{font-size:12px;color:var(--muted);line-height:1.4;margin-top:auto}
.arrow{color:var(--faint);font-weight:400;margin:0 8px}
.cover-foot{display:flex;justify-content:space-between;gap:20px;font-size:12px;color:var(--muted);border-top:1px solid var(--line);padding-top:12px;position:relative}
.cover-foot strong{color:var(--ink)}
/* cards and charts */
.card{border:1px solid var(--line);border-radius:16px;padding:18px 18px 16px;margin-bottom:14px;break-inside:avoid}
.grid-3{display:grid;grid-template-columns:repeat(3,1fr);gap:14px;margin-bottom:14px}
.grid-2{display:grid;grid-template-columns:repeat(2,1fr);gap:14px}
.grid-3>.card,.grid-2>.card{margin-bottom:0}
.stack{display:flex;height:14px;border-radius:99px;overflow:hidden;background:var(--soft);gap:2px}
.stack.thin{height:8px;margin-top:10px}
.seg{background:var(--tone);height:100%}
.legend{display:grid;grid-template-columns:repeat(5,1fr);gap:10px;margin-top:14px}
.legend.four{grid-template-columns:repeat(4,1fr)}
.legend-item{display:grid;grid-template-columns:10px 1fr;column-gap:8px;align-items:center}
.legend-item.dim{opacity:.45}
.swatch{width:10px;height:10px;border-radius:3px;background:var(--tone)}
.legend-label{font-size:12px;color:var(--muted)}
.legend-value{grid-column:2;font-size:18px;font-weight:700;letter-spacing:-.02em;font-variant-numeric:tabular-nums}
.legend-share{grid-column:2;font-size:11.5px;color:var(--faint);font-variant-numeric:tabular-nums}
.triple{display:grid;grid-template-columns:repeat(3,auto);justify-content:space-between;gap:8px}
.mini{display:flex;flex-direction:column}
.mini-value{font-size:28px;font-weight:720;letter-spacing:-.03em;font-variant-numeric:tabular-nums}
.mini-label{font-size:12px;color:var(--muted);white-space:nowrap}
.mini-label .dot{margin-right:5px}
.big{font-size:30px;font-weight:720;letter-spacing:-.03em;font-variant-numeric:tabular-nums}
.unit{font-size:13px;font-weight:500;color:var(--muted);letter-spacing:0}
.facts{margin-top:8px}
.facts div{display:flex;justify-content:space-between;font-size:12.5px;padding:5px 0;border-top:1px dashed var(--line)}
.facts dt{color:var(--muted)}.facts dd{font-weight:600;font-variant-numeric:tabular-nums}
.columns{display:flex;align-items:flex-end;gap:10px;height:150px;padding-top:6px}
.col{flex:1;display:flex;flex-direction:column;align-items:center;justify-content:flex-end;height:100%;min-width:0}
.col-bar{width:100%;max-width:56px;border-radius:9px 9px 3px 3px;background:linear-gradient(180deg,var(--accent),color-mix(in srgb,var(--accent) 55%,#fff))}
.col-value{font-size:11.5px;font-weight:600;color:var(--ink-2);margin-bottom:5px;font-variant-numeric:tabular-nums}
.col-label{font-size:11.5px;color:var(--muted);margin-top:7px}
/* jobs */
.jobs{list-style:none;display:flex;flex-direction:column;gap:12px}
.job{display:grid;grid-template-columns:52px 1fr;gap:6px;border:1px solid var(--line);border-radius:16px;padding:16px 18px 16px 12px;break-inside:avoid;position:relative;overflow:hidden}
.job::before{content:"";position:absolute;left:0;top:0;bottom:0;width:4px;background:var(--tone)}
.rank{font-size:22px;font-weight:750;color:color-mix(in srgb,var(--tone) 70%,#000);letter-spacing:-.03em;text-align:center;font-variant-numeric:tabular-nums;padding-top:1px}
.job-top{display:flex;align-items:center;gap:8px;flex-wrap:wrap}
.job-top h3{margin:0 6px 0 0;font-size:16px}
.job-meta{font-size:12.5px;color:var(--muted);margin:5px 0 10px;font-variant-numeric:tabular-nums}
.impact{display:flex;align-items:center;gap:10px;max-width:340px;margin-bottom:10px}
.impact .meter{--tone:var(--accent)}
.impact-label{font-size:11px;font-weight:650;letter-spacing:.08em;text-transform:uppercase;color:var(--faint)}
.impact-value{font-size:12.5px;font-weight:700;width:24px;text-align:right;font-variant-numeric:tabular-nums}
.fix{color:var(--ink-2);font-size:13.5px}
.examples{list-style:none;margin-top:10px;border-top:1px dashed var(--line);padding-top:8px}
.examples li{display:flex;gap:12px;font-size:12px;padding:2px 0;min-width:0}
.url{font:12px/1.5 ui-monospace,"SF Mono",Menlo,Consolas,monospace;color:var(--ink-2);overflow:hidden;text-overflow:ellipsis;white-space:nowrap;min-width:0;flex:1}
.detail{color:var(--faint);white-space:nowrap;overflow:hidden;text-overflow:ellipsis;max-width:45%}
/* changes */
.moves{list-style:none}
.moves li{display:grid;grid-template-columns:1fr auto 46px;align-items:center;gap:5px 12px;font-size:12.5px;padding:7px 0;border-top:1px solid var(--line)}
.moves li:first-child{border-top:0}
.move-title{grid-column:1/-1;overflow:hidden;text-overflow:ellipsis;white-space:nowrap}
.move-bars{display:flex;flex-direction:column;gap:3px}
.move-bars span{height:4px;border-radius:9px}
.was{background:var(--line)}.now{background:var(--tone)}
.move-figures{color:var(--muted);text-align:right;font-variant-numeric:tabular-nums}
.delta{font-weight:700;text-align:right;color:var(--tone);font-variant-numeric:tabular-nums}
.alert{border-color:color-mix(in srgb,var(--err) 35%,#fff);background:color-mix(in srgb,var(--err) 4%,#fff)}
.broken{list-style:none}
.broken li{display:flex;gap:12px;padding:5px 0;font-size:12.5px;border-top:1px solid color-mix(in srgb,var(--err) 14%,#fff)}
.broken li:first-child{border-top:0}
.codes{color:var(--muted);white-space:nowrap;font-variant-numeric:tabular-nums}.codes strong{color:var(--err)}
/* accessibility */
.verdict-card{border-color:color-mix(in srgb,var(--tone) 30%,#fff);background:linear-gradient(180deg,color-mix(in srgb,var(--tone) 6%,#fff),#fff 60%)}
.verdict-top{display:flex;gap:16px;align-items:flex-start;margin-bottom:16px}
.verdict-headline{font-weight:650;font-size:15px}
.verdict-top .note{margin-top:4px}
.barriers{list-style:none}
.barriers li{padding:11px 0;border-top:1px solid var(--line);break-inside:avoid}
.barriers li:first-child{border-top:0;padding-top:0}
.barrier-top{display:flex;align-items:center;gap:9px}
.barrier-title{font-weight:600;flex:1}
.barrier-pages{font-size:12.5px;color:var(--muted);font-variant-numeric:tabular-nums;white-space:nowrap}
.barrier-bar{display:flex;align-items:center;gap:10px;margin-top:7px}
.barrier-bar .meter{--tone:var(--accent)}
.barrier-share{font-size:12px;font-weight:650;width:44px;text-align:right;font-variant-numeric:tabular-nums}
.barriers .note{margin-top:5px}
.impact-dot{width:9px;height:9px;border-radius:50%;flex:none}
.impact-critical{background:var(--err)}.impact-serious{background:#e8743b}.impact-moderate{background:var(--warn)}.impact-minor{background:var(--note)}
/* appendix */
.issues{width:100%;border-collapse:collapse;font-size:13px}
.issues th{text-align:left;font-size:10.5px;font-weight:650;letter-spacing:.1em;text-transform:uppercase;color:var(--faint);padding:0 8px 8px;border-bottom:1px solid var(--line)}
.issues td{padding:8px;border-bottom:1px solid var(--line);vertical-align:middle}
.issues tr{break-inside:avoid}
.issues .sev{width:84px;padding-left:0}
.issues .num{width:70px;text-align:right;font-weight:600;font-variant-numeric:tabular-nums}
.issues .barcol{width:120px;padding-right:0}
.method{margin-top:24px;background:var(--soft);border-radius:16px;padding:18px 20px;break-inside:avoid}
.method h3{font-size:13px;margin-bottom:8px}
.method ul{padding-left:18px;color:var(--muted);font-size:12.5px}
.method li{margin:3px 0}
@page{size:A4;margin:12mm 0 14mm}
@page{@bottom-center{content:counter(page);font:10px -apple-system,"Segoe UI",sans-serif;color:#9aa1ad}}
@page:first{@bottom-center{content:none}}
@media print{
 body{background:#fff}
 .doc{max-width:none;padding:0}
 .sheet{box-shadow:none;border-radius:0;margin:0;padding:0 14mm;break-after:page}
 .sheet:last-child{break-after:auto}
 .cover{min-height:271mm;padding-top:0}
}
@media (max-width:760px){
 .doc{padding:12px}
 .sheet{padding:22px 18px;border-radius:16px}
 .kpis,.grid-3,.grid-2{grid-template-columns:1fr 1fr}
 .legend{grid-template-columns:repeat(3,1fr)}
 .hero{flex-direction:column;align-items:flex-start}
 .cover h1{font-size:36px}
}
"#;
