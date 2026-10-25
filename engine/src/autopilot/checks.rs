//! The test library, and the plan built from it.
//!
//! Every page gets the shared baseline; each page type adds the checks that
//! matter for what that page is for. A check ends in exactly one status, says
//! what it saw, says how much that evidence is worth, and — when it found a
//! problem — carries a finding with a recommendation and acceptance criteria.
//!
//! Three honesty rules are enforced here rather than in the UI:
//! - a check that needs a browser is `NotTested`, never `Pass`;
//! - submitting a form is `Blocked` by guardrail, always;
//! - a judgement call is `NeedsReview` (or a pass on inferred evidence), never
//!   a verified failure.

use serde::Serialize;

use super::context::{absolute, CrawlFacts};
use super::page::{self, PageFacts};
use super::{
    redact, Category, CheckResult, Depth, Evidence, EvidenceKind, EvidenceStatus, Finding, Goal, Method, PageType,
    Recommendation, Setup, Severity, Source, Status,
};
use crate::a11y::{
    self,
    rules::{Certainty, Impact},
};
use crate::ux::signals::Region;

/// One entry in the library.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CheckDef {
    pub id: &'static str,
    pub title: &'static str,
    pub question: &'static str,
    pub category: Category,
    /// The shallowest depth that includes it.
    pub tier: Depth,
    pub method: Method,
    /// Empty for the shared baseline.
    pub page_types: &'static [PageType],
}

impl CheckDef {
    pub fn baseline(&self) -> bool {
        self.page_types.is_empty()
    }

    fn applies(&self, page_type: PageType, depth: Depth) -> bool {
        self.tier <= depth && (self.baseline() || self.page_types.contains(&page_type))
    }
}

const fn def(
    id: &'static str,
    title: &'static str,
    question: &'static str,
    category: Category,
    tier: Depth,
    method: Method,
    page_types: &'static [PageType],
) -> CheckDef {
    CheckDef { id, title, question, category, tier, method, page_types }
}

use Category as C;
use Depth::{Deep, Quick, Standard};
use Method as M;
use PageType as P;

const ALL: &[PageType] = &[];

const LIBRARY: &[CheckDef] = &[
    // ── Baseline: every page
    def("loads", "The page loads", "Does the server return the page?", C::Performance, Quick, M::Crawl, ALL),
    def("response_time", "Server responds quickly", "How long did the server take to answer?", C::Performance, Quick, M::Crawl, ALL),
    def("page_weight", "HTML is a reasonable size", "Is the HTML document light enough for a phone on a weak connection?", C::Performance, Standard, M::Crawl, ALL),
    def("broken_images", "Images load", "Do the images on this page resolve?", C::Performance, Standard, M::Crawl, ALL),
    def("console_errors", "No script errors", "Does the page throw JavaScript errors while loading?", C::Performance, Deep, M::Browser, ALL),
    def("title", "Descriptive page title", "Does the title tell a visitor, a tab and a search result what this is?", C::Clarity, Quick, M::Automated, ALL),
    def("value_prop", "Clear value proposition", "Can a first-time visitor tell what this is and who it is for from the top of the page?", C::Clarity, Quick, M::Heuristic, ALL),
    def("primary_cta", "One obvious next step", "Is there a clear action that serves the goal?", C::Clarity, Quick, M::Automated, ALL),
    def("cta_labels", "Action labels say what happens", "Do buttons and links say where they lead?", C::Clarity, Standard, M::Automated, ALL),
    def("readability", "Readable copy", "Are sentences and paragraphs short enough to scan?", C::Clarity, Standard, M::Heuristic, ALL),
    def("h1", "One main heading", "Does the page have exactly one H1 that names it?", C::Hierarchy, Quick, M::Automated, ALL),
    def("heading_order", "Headings nest in order", "Do heading levels step down without skipping?", C::Hierarchy, Standard, M::Automated, ALL),
    def("competing_ctas", "Actions don't compete", "Are there so many calls to action that none stands out?", C::Hierarchy, Standard, M::Heuristic, ALL),
    def("nav_present", "Visitors can find their way", "Is there navigation to the rest of the site?", C::Navigation, Quick, M::Automated, ALL),
    def("nav_size", "Navigation is scannable", "Is the menu short enough to take in at a glance?", C::Navigation, Standard, M::Heuristic, ALL),
    def("broken_links", "No broken links", "Do the links on this page lead somewhere that works?", C::Navigation, Quick, M::Crawl, ALL),
    def("redirects", "Links point at final URLs", "Do internal links go through redirects?", C::Navigation, Deep, M::Crawl, ALL),
    def("form_labels", "Form fields are labelled", "Does every field have a label that stays visible and is read out?", C::Forms, Quick, M::Automated, ALL),
    def("form_length", "Forms ask only what they need", "Is the form short enough for the reward on offer?", C::Forms, Standard, M::Heuristic, ALL),
    def("form_autocomplete", "Forms help autofill", "Do name, email and phone fields declare autocomplete?", C::Forms, Standard, M::Automated, ALL),
    def("form_submit", "Form errors and success", "What happens when the form is submitted, with good and bad input?", C::Forms, Standard, M::Manual, ALL),
    def("viewport", "Mobile viewport, zoom allowed", "Is the page set up for phones, without blocking pinch-zoom?", C::Responsive, Quick, M::Automated, ALL),
    def("image_dimensions", "Images reserve their space", "Do images declare a size, so the page doesn't jump while loading?", C::Responsive, Standard, M::Automated, ALL),
    def("reflow", "Content reflows at 320px", "Does anything overflow or need sideways scrolling on a narrow phone?", C::Responsive, Standard, M::Browser, ALL),
    def("tap_targets", "Tap targets are big enough", "Are buttons and links at least 24×24px and spaced apart?", C::Responsive, Deep, M::Browser, ALL),
    def("a11y_rules", "Automated accessibility rules", "Does the markup break WCAG rules that can be checked automatically?", C::Accessibility, Quick, M::Automated, ALL),
    def("focus_visible", "Keyboard focus is visible", "Can a keyboard user see where they are?", C::Accessibility, Standard, M::Automated, ALL),
    def("tab_order", "Tab order follows the page", "Is the keyboard order forced with positive tabindex?", C::Accessibility, Standard, M::Automated, ALL),
    def("contrast", "Text contrast", "Does text meet 4.5:1 contrast against its background?", C::Accessibility, Standard, M::Browser, ALL),
    def("keyboard", "Everything works by keyboard", "Can every control be reached and used without a mouse?", C::Accessibility, Deep, M::Browser, ALL),
    def("https", "Served securely", "Is the page on HTTPS?", C::Trust, Quick, M::Automated, ALL),
    def("trust_signals", "Reasons to trust", "Does the page show proof — reviews, clients, guarantees, credentials?", C::Trust, Quick, M::Heuristic, ALL),
    def("contact_route", "A human is reachable", "Can a visitor find a way to contact someone?", C::Trust, Standard, M::Automated, ALL),
    def("privacy_near_form", "Privacy is explained where data is asked for", "Does a form that collects contact details say what happens to them?", C::Trust, Standard, M::Automated, ALL),
    def("goal_journey", "The goal can be reached", "Starting from the home page, can a visitor get to where the goal is completed?", C::Journey, Standard, M::Crawl, ALL),
    def("support_journeys", "Supporting pages are reachable", "Can a visitor find pricing, contact and privacy?", C::Journey, Deep, M::Crawl, ALL),
    def("cta_consistency", "Same action, same words", "Is the main action called the same thing everywhere on the page?", C::Consistency, Deep, M::Heuristic, ALL),
    def("interaction_states", "Hover, focus and disabled states", "Do controls look the same in every state across the page?", C::Consistency, Deep, M::Browser, ALL),
    // ── SaaS
    def("saas_pricing", "Pricing is findable", "Can a visitor find out what it costs?", C::Clarity, Quick, M::Automated, &[P::Saas]),
    def("saas_friction", "Trial terms are stated", "Does the page say whether a trial needs a card or a sales call?", C::Trust, Standard, M::Heuristic, &[P::Saas]),
    def("saas_security", "Data security is addressed", "Does the page say anything about security, privacy or compliance?", C::Trust, Standard, M::Heuristic, &[P::Saas]),
    // ── Local business
    def("local_phone", "Click-to-call phone number", "Can a visitor on a phone call with one tap?", C::Clarity, Quick, M::Automated, &[P::LocalBusiness]),
    def("local_hours", "Opening hours", "Does the page say when they are open?", C::Clarity, Quick, M::Automated, &[P::LocalBusiness]),
    def("local_area", "Location or service area", "Does the page say where they are or which areas they serve?", C::Clarity, Standard, M::Heuristic, &[P::LocalBusiness]),
    def("local_reviews", "Reviews or testimonials", "Is there proof that local customers were happy?", C::Trust, Standard, M::Heuristic, &[P::LocalBusiness]),
    // ── Ecommerce
    def("shop_price", "Price is shown", "Is the price visible on the product page?", C::Clarity, Quick, M::Automated, &[P::Ecommerce]),
    def("shop_cart", "Add to cart", "Is there an add-to-cart or buy button?", C::Journey, Quick, M::Automated, &[P::Ecommerce]),
    def("shop_delivery", "Delivery and returns", "Does the page say when it arrives and how returns work?", C::Trust, Standard, M::Heuristic, &[P::Ecommerce]),
    def("shop_reviews", "Product reviews", "Are there reviews or ratings?", C::Trust, Standard, M::Heuristic, &[P::Ecommerce]),
    def("shop_stock", "Availability", "Does the page say whether it's in stock?", C::Clarity, Deep, M::Heuristic, &[P::Ecommerce]),
    // ── Agency
    def("agency_work", "Work samples", "Can a visitor see work like theirs?", C::Trust, Quick, M::Automated, &[P::Agency]),
    def("agency_people", "Who they are", "Can a visitor see the people behind the work?", C::Trust, Standard, M::Heuristic, &[P::Agency]),
    // ── Lead generation
    def("lead_next", "What happens next", "Does the page say what happens after the form is sent?", C::Clarity, Quick, M::Heuristic, &[P::LeadGen]),
    def("lead_focus", "Single focus", "Do other links pull visitors away from the form?", C::Hierarchy, Standard, M::Heuristic, &[P::LeadGen]),
    // ── Waitlist
    def("waitlist_when", "Launch timing", "Does the page say when it launches?", C::Clarity, Quick, M::Heuristic, &[P::Waitlist]),
    def("waitlist_spam", "No-spam promise", "Does the page say how the email will be used?", C::Trust, Standard, M::Heuristic, &[P::Waitlist]),
    // ── Content
    def("content_byline", "Author and date", "Does the piece say who wrote it and when?", C::Trust, Quick, M::Automated, &[P::Content]),
    def("content_next", "A next step", "Where does a reader go after the last paragraph?", C::Journey, Standard, M::Heuristic, &[P::Content]),
    def("content_structure", "Long text is broken up", "Is a long piece broken into sections a reader can scan?", C::Hierarchy, Standard, M::Automated, &[P::Content]),
    // ── Marketplace
    def("market_search", "Search", "Can a visitor search the listings?", C::Navigation, Quick, M::Automated, &[P::Marketplace]),
    def("market_filters", "Filters and sorting", "Can a visitor narrow the results down?", C::Navigation, Standard, M::Automated, &[P::Marketplace]),
    def("market_how", "How it works", "Does the page explain how buying or booking works?", C::Trust, Standard, M::Heuristic, &[P::Marketplace]),
];

/// The whole library, for the screen that lists it.
pub fn library() -> Vec<CheckDef> {
    LIBRARY.to_vec()
}

/// What a single check concluded, before it is dressed as a [`CheckResult`].
struct Out {
    status: Status,
    summary: String,
    kind: EvidenceKind,
    evidence: EvidenceStatus,
    observed: Vec<String>,
    clue: Option<String>,
    steps: Vec<String>,
    finding: Option<Finding>,
}

impl Out {
    fn new(status: Status, kind: EvidenceKind, evidence: EvidenceStatus, summary: impl Into<String>) -> Self {
        Self { status, summary: summary.into(), kind, evidence, observed: Vec::new(), clue: None, steps: Vec::new(), finding: None }
    }

    /// Decided from the markup, and passed.
    fn pass(summary: impl Into<String>) -> Self {
        Self::new(Status::Pass, EvidenceKind::Dom, EvidenceStatus::Verified, summary)
    }

    /// Decided from the markup, and failed.
    fn fail(summary: impl Into<String>) -> Self {
        Self::new(Status::Fail, EvidenceKind::Dom, EvidenceStatus::Verified, summary)
    }

    /// A judgement: worth a person's look.
    fn review(summary: impl Into<String>) -> Self {
        Self::new(Status::NeedsReview, EvidenceKind::Dom, EvidenceStatus::Inferred, summary)
    }

    /// A judgement that found nothing to worry about. Still only inferred.
    fn fine(summary: impl Into<String>) -> Self {
        Self::new(Status::Pass, EvidenceKind::Dom, EvidenceStatus::Inferred, summary)
    }

    fn not_tested(reason: impl Into<String>) -> Self {
        Self::new(Status::NotTested, EvidenceKind::Unavailable, EvidenceStatus::NeedsHuman, reason)
    }

    fn kind(mut self, kind: EvidenceKind) -> Self {
        self.kind = kind;
        self
    }

    fn partial(mut self) -> Self {
        self.evidence = EvidenceStatus::Partial;
        self
    }

    fn saw<S: Into<String>>(mut self, items: impl IntoIterator<Item = S>) -> Self {
        self.observed.extend(items.into_iter().map(Into::into));
        self
    }

    fn clue(mut self, clue: impl Into<String>) -> Self {
        self.clue = Some(clue.into());
        self
    }

    fn steps(mut self, steps: &[&str]) -> Self {
        self.steps = steps.iter().map(|s| s.to_string()).collect();
        self
    }

    fn with(mut self, finding: Finding) -> Self {
        self.finding = Some(finding);
        self
    }
}

struct Ctx<'a> {
    setup: &'a Setup,
    source: &'a Source,
    facts: &'a PageFacts,
    page_type: PageType,
    goal: Goal,
    crawl: Option<&'a CrawlFacts>,
}

impl Ctx<'_> {
    /// A finding with its factors shown. Critical findings escalate to the
    /// top of the priority order whatever their arithmetic says.
    #[allow(clippy::too_many_arguments)]
    fn finding(
        &self,
        title: impl Into<String>,
        severity: Severity,
        [impact, reach, confidence, effort]: [u8; 4],
        explanation: impl Into<String>,
        consequence: impl Into<String>,
        region: &str,
        recommendation: Recommendation,
    ) -> Finding {
        let score = Finding::score(impact, reach, confidence, effort);
        Finding {
            title: title.into(),
            explanation: explanation.into(),
            severity,
            impact,
            reach,
            confidence,
            effort,
            priority: if severity == Severity::Critical { 125.0 } else { score },
            user_goal: format!("{} ({})", self.page_type.conversion(), self.goal.label().to_lowercase()),
            consequence: consequence.into(),
            region: region.to_string(),
            devices: self.setup.device.as_str().to_string(),
            recommendation,
        }
    }
}

fn rec(problem: &str, why: &str, change: &str, notes: &str, acceptance: &[&str], validation: &str) -> Recommendation {
    Recommendation {
        problem: problem.to_string(),
        why: why.to_string(),
        change: change.to_string(),
        before: None,
        after: None,
        notes: notes.to_string(),
        acceptance: acceptance.iter().map(|a| a.to_string()).collect(),
        validation: validation.to_string(),
    }
}

fn region_name(region: Region) -> &'static str {
    match region {
        Region::Header => "header",
        Region::Nav => "nav",
        Region::Main | Region::Unknown => "main",
        Region::Aside => "aside",
        Region::Footer => "footer",
    }
}

fn quote(text: &str) -> String {
    let text = text.trim();
    let short: String = text.chars().take(140).collect();
    if short.len() < text.len() { format!("“{short}…”") } else { format!("“{short}”") }
}

const NO_BROWSER: &str = "CrawlStat reads markup and does not run a browser, so this was not tested. Check it by hand or with a browser-based tool.";

/// Build the plan for this page and run every check in it.
pub fn run_plan(
    setup: &Setup,
    source: &Source,
    facts: &PageFacts,
    page_type: PageType,
    goal: Goal,
    crawl: Option<&CrawlFacts>,
) -> Vec<CheckResult> {
    let ctx = Ctx { setup, source, facts, page_type, goal, crawl };
    let mut results = Vec::new();
    for def in LIBRARY.iter().filter(|d| d.applies(page_type, setup.depth)) {
        if def.id == "a11y_rules" {
            results.extend(accessibility(&ctx, def));
            continue;
        }
        let out = evaluate(&ctx, def.id);
        results.push(dress(&ctx, def, def.id.to_string(), def.title.to_string(), out));
    }
    results
}

fn dress(ctx: &Ctx, def: &CheckDef, id: String, title: String, out: Out) -> CheckResult {
    let method = if out.status == Status::NotTested && def.method != M::Manual { M::Browser } else { def.method };
    CheckResult {
        id,
        title,
        question: def.question.to_string(),
        category: def.category,
        tier: def.tier,
        method,
        baseline: def.baseline(),
        status: out.status,
        summary: redact(&out.summary),
        evidence: Evidence {
            kind: out.kind,
            status: out.evidence,
            url: redact(&ctx.facts.url),
            viewport: "None rendered: the HTML source was read, not displayed".to_string(),
            browser: format!("No browser ({} HTML)", ctx.source.origin),
            tested_at: ctx.source.fetched_at,
            observed: out.observed.iter().map(|o| redact(o)).collect(),
            dom_clue: out.clue,
            steps: out.steps,
            limitation: out.kind.limitation().to_string(),
        },
        finding: out.finding,
    }
}

/// One result per accessibility rule that fired, or one pass.
fn accessibility(ctx: &Ctx, def: &CheckDef) -> Vec<CheckResult> {
    let url = absolute(&ctx.facts.url);
    let check = match a11y::check_markup(&ctx.source.html, &url) {
        Ok(check) => check,
        Err(error) => {
            return vec![dress(ctx, def, def.id.into(), def.title.into(), Out::not_tested(format!("The accessibility rules could not run: {error}")))];
        }
    };
    if check.findings.is_empty() {
        let out = Out::pass(format!("{} automated rules ran and found nothing. Automated rules catch only part of WCAG; this is not a full audit.", check.passed.len()))
            .saw(check.passed.iter().take(12).map(|r| r.to_string()));
        return vec![dress(ctx, def, def.id.into(), def.title.into(), out)];
    }
    check
        .findings
        .iter()
        .map(|found| {
            let info = found.info;
            let certain = info.certainty == Certainty::Fail;
            let severity = match (info.impact, certain) {
                (Impact::Critical, true) => Severity::Critical,
                (Impact::Critical | Impact::Serious, _) => Severity::High,
                (Impact::Moderate, _) => Severity::Medium,
                (Impact::Minor, _) => Severity::Low,
            };
            let impact = match info.impact {
                Impact::Critical => 5,
                Impact::Serious => 4,
                Impact::Moderate => 3,
                Impact::Minor => 2,
            };
            let criteria = info.criteria.join(", ");
            let finding = ctx.finding(
                info.title,
                severity,
                [impact, 2, if certain { 5 } else { 3 }, 2],
                format!("{} WCAG {criteria}.", info.why),
                format!("Affects {}.", info.affects),
                "page",
                Recommendation {
                    before: Some(info.bad.to_string()),
                    after: Some(info.good.to_string()),
                    ..rec(
                        info.title,
                        info.why,
                        info.fix,
                        "Example markup, not your page's code. Fix every instance, not only the samples.",
                        &["The rule no longer fires on a retest", "A screen reader announces the element sensibly"],
                        "Retest the page here, then confirm with a screen reader (VoiceOver or NVDA).",
                    )
                },
            );
            let summary = format!("{} instance{} — WCAG {criteria}", found.count, if found.count == 1 { "" } else { "s" });
            let out = if certain { Out::fail(summary) } else { Out::review(summary) }
                .saw(found.samples.iter().cloned())
                .steps(&["View the page source", "Search for the samples listed above"])
                .with(finding);
            dress(ctx, def, format!("a11y:{}", info.id), info.title.to_string(), out)
        })
        .collect()
}

fn evaluate(ctx: &Ctx, id: &str) -> Out {
    let f = ctx.facts;
    match id {
        "loads" => match ctx.source.status {
            Some(status) if (200..300).contains(&status) => {
                Out::pass(format!("HTTP {status}")).kind(EvidenceKind::Network).saw([format!("Status {status} from {} HTML", ctx.source.origin)])
            }
            status => {
                let shown = status.map_or("no response".to_string(), |s| format!("HTTP {s}"));
                Out::fail(format!("The page answered with {shown}"))
                    .kind(EvidenceKind::Network)
                    .saw([shown.clone()])
                    .steps(&["Open the URL in a private window", "Look at the status in the browser's network panel"])
                    .with(ctx.finding(
                        "The page does not load",
                        Severity::Critical,
                        [5, 5, 5, 2],
                        format!("The server answered {shown}. Nobody can use a page that does not arrive."),
                        "Every visitor bounces; the goal is impossible.",
                        "page",
                        rec("The page returns an error.", "No other finding matters until the page loads.", "Restore the page or redirect the URL to its replacement.", "Check server logs for the failing route.", &["The URL returns 200", "No redirect chain longer than one hop"], "Retest here and confirm the status is 200."),
                    ))
            }
        },
        "response_time" => match ctx.source.response_ms {
            None => Out::not_tested("The crawl did not record a response time for this page."),
            Some(ms) if ms <= 800 => Out::pass(format!("Server answered in {ms} ms")).kind(EvidenceKind::Network).saw([format!("{ms} ms to first byte and body")]),
            Some(ms) => {
                let slow = ms > 2000;
                let out = if slow { Out::fail(format!("Server took {ms} ms")) } else { Out::review(format!("Server took {ms} ms")) };
                out.kind(EvidenceKind::Network)
                    .partial()
                    .saw([format!("{ms} ms measured once, from {} request", ctx.source.origin)])
                    .steps(&["Load the page a few times with the network panel open", "Compare time to first byte"])
                    .with(ctx.finding(
                        "Slow server response",
                        if slow { Severity::Medium } else { Severity::Low },
                        [if slow { 3 } else { 2 }, 5, 2, 3],
                        format!("One request took {ms} ms before the page could even start to render. A single measurement; network conditions vary."),
                        "Visitors on mobile wait on a blank screen and some leave.",
                        "page",
                        rec("The server is slow to answer.", "Time to first byte delays everything else on the page.", "Cache the page (CDN or full-page cache) and look for slow queries on this route.", "This measures the server, not paint or interactivity. Use a lab tool for those.", &["Server response under 800 ms on three consecutive retests"], "Retest three times and compare."),
                    ))
            }
        },
        "page_weight" => match ctx.source.bytes {
            None => Out::not_tested("The size of the HTML was not recorded."),
            Some(bytes) if bytes <= 500_000 => Out::pass(format!("{} KB of HTML", bytes / 1000)).kind(EvidenceKind::Network),
            Some(bytes) => Out::fail(format!("{} KB of HTML", bytes / 1000)).kind(EvidenceKind::Network).with(ctx.finding(
                "Heavy HTML document",
                Severity::Low,
                [2, 4, 4, 3],
                format!("The HTML alone is {} KB, before images, styles or scripts.", bytes / 1000),
                "Slower first paint on mobile connections.",
                "page",
                rec("The HTML is very large.", "Big documents take longer to download and parse.", "Move inline data and SVG sprites out of the HTML, paginate long lists, and enable compression.", "Size measured uncompressed.", &["HTML under 500 KB"], "Retest and compare the size."),
            )),
        },
        "broken_images" => match ctx.crawl {
            None => Out::not_tested("This page is not in the crawl, so image responses are unknown."),
            Some(crawl) if crawl.broken_images.is_empty() => Out::pass("No image on this page returned an error in the crawl").kind(EvidenceKind::Network),
            Some(crawl) => Out::fail(format!("{} broken image{}", crawl.broken_images.len(), if crawl.broken_images.len() == 1 { "" } else { "s" }))
                .kind(EvidenceKind::Network)
                .saw(crawl.broken_images.iter().take(8).map(|(u, s)| format!("{s} {u}")))
                .with(ctx.finding(
                    "Broken images",
                    Severity::Medium,
                    [3, 4, 5, 1],
                    "Images on this page return errors, so visitors see empty boxes or broken icons.",
                    "The page looks neglected, which costs trust.",
                    "main",
                    rec("Images fail to load.", "Missing images look broken and can hide product or proof content.", "Fix or replace the image URLs listed in the evidence.", "", &["Every image URL returns 200"], "Re-crawl, then retest."),
                )),
        },
        "console_errors" | "reflow" | "tap_targets" | "contrast" | "keyboard" | "interaction_states" => Out::not_tested(NO_BROWSER),
        "form_submit" => Out::new(Status::Blocked, EvidenceKind::Unavailable, EvidenceStatus::NeedsHuman, if f.task_forms().is_empty() {
            "No forms to submit. Forms are never submitted by CrawlStat in any case."
        } else {
            "Blocked by guardrail: CrawlStat never submits forms, so error and success messages were not seen. Test with synthetic data on a staging site."
        }),
        "title" => match f.title.as_deref().map(str::trim).filter(|t| !t.is_empty()) {
            None => Out::fail("No <title>").clue("<head> has no <title>").with(ctx.finding(
                "Missing page title",
                Severity::Medium,
                [3, 5, 5, 1],
                "The page has no title, so tabs, bookmarks and search results show the URL instead.",
                "Visitors with several tabs open can't find it; search results look broken.",
                "page",
                rec("No page title.", "The title is the first thing search and tabs show.", "Add a title that names the page and the site, under 60 characters.", "", &["A <title> exists and is 10–60 characters"], "Retest."),
            )),
            Some(title) if title.chars().count() < 10 || title.chars().count() > 65 => {
                let n = title.chars().count();
                Out::fail(format!("Title is {n} characters")).saw([quote(title)]).clue("<title>").with(ctx.finding(
                    if n < 10 { "Page title is too short" } else { "Page title is too long" },
                    Severity::Low,
                    [2, 4, 5, 1],
                    format!("The title is {n} characters. Short titles don't say what the page is; long ones are cut off in search results."),
                    "Weaker search listings and harder-to-find tabs.",
                    "page",
                    Recommendation { before: Some(title.to_string()), after: Some(format!("[What the page offers] | {}", f.h1s.first().map(String::as_str).unwrap_or("[Site name]"))), ..rec("Title length.", "Search engines show about 60 characters.", "Rewrite the title to 10–60 characters, leading with what the page offers.", "The proposed title is a template; fill in your own wording.", &["Title is 10–60 characters", "It names the page, not only the brand"], "Retest.") },
                ))
            }
            Some(title) => Out::pass("Title is present and a sensible length").saw([quote(title)]).clue("<title>"),
        },
        "h1" => match f.h1s.len() {
            0 => Out::fail("No H1").clue("No <h1> element").with(ctx.finding(
                "No main heading",
                Severity::Medium,
                [3, 5, 5, 1],
                "The page has no H1, so there is no single line that says what it is — for visitors scanning, or for screen reader users jumping by heading.",
                "Visitors take longer to understand where they are.",
                "main",
                rec("Missing H1.", "The H1 is where people and assistive technology look first.", "Make the main headline an <h1>, once.", "", &["Exactly one <h1>", "It states the offer or the page's subject"], "Retest."),
            )),
            1 => Out::pass("Exactly one H1").saw([quote(&f.h1s[0])]).clue("<h1>"),
            n => Out::review(format!("{n} H1 elements")).saw(f.h1s.iter().map(|h| quote(h))).clue("<h1>").with(ctx.finding(
                "Several main headings",
                Severity::Low,
                [2, 4, 4, 1],
                format!("There are {n} H1s. Which is the page about?"),
                "The page's subject is diluted for scanners and screen readers.",
                "main",
                rec("Multiple H1s.", "One H1 gives the page a single subject.", "Keep one H1 and turn the rest into H2s.", "Several H1s are valid HTML; this is a clarity call.", &["One <h1>"], "Retest."),
            )),
        },
        "heading_order" => {
            let mut skips = Vec::new();
            let mut last = 0u8;
            for (level, text) in &f.headings {
                if last > 0 && *level > last + 1 {
                    skips.push(format!("H{last} → H{level}: {}", quote(text)));
                }
                last = *level;
            }
            if f.headings.is_empty() {
                Out::fail("No headings at all").with(ctx.finding("No headings", Severity::Medium, [3, 4, 5, 2], "The page has no headings, so it can't be scanned or navigated by heading.", "Visitors read everything or nothing.", "main", rec("No headings.", "Headings are how people scan.", "Add an H1 and H2s for each section.", "", &["The page has an H1 and section headings"], "Retest.")))
            } else if skips.is_empty() {
                Out::pass(format!("{} headings, levels in order", f.headings.len()))
            } else {
                Out::fail(format!("{} skipped heading level{}", skips.len(), if skips.len() == 1 { "" } else { "s" })).saw(skips.into_iter().take(6)).clue("h1–h6").with(ctx.finding(
                    "Heading levels skip",
                    Severity::Low,
                    [2, 3, 5, 1],
                    "Heading levels jump (for example H2 to H4), which breaks the outline screen reader users navigate by.",
                    "The page's structure is harder to follow.",
                    "main",
                    rec("Skipped heading levels.", "WCAG 1.3.1: structure should be conveyed in markup.", "Use the next level down, and style with CSS rather than picking a level for its size.", "", &["No level is skipped"], "Retest."),
                ))
            }
        }
        "value_prop" => {
            let h1 = f.h1s.first().map(|h| h.trim().to_string()).unwrap_or_default();
            let offer = f.meta_description.clone().or_else(|| f.lead.clone());
            let generic = ["welcome", "home", "homepage", "hello", "untitled"];
            let vague = h1.split_whitespace().count() <= 2 || generic.iter().any(|g| h1.to_ascii_lowercase().starts_with(g));
            let saw: Vec<String> = [Some(format!("H1: {}", if h1.is_empty() { "(none)".into() } else { quote(&h1) })), offer.as_ref().map(|o| format!("Lead: {}", quote(o)))].into_iter().flatten().collect();
            if h1.is_empty() || vague || offer.is_none() {
                Out::review("The top of the page may not say what this is and who it's for").saw(saw).with(ctx.finding(
                    "Value proposition may be unclear",
                    Severity::Medium,
                    [4, 5, 2, 2],
                    "Heuristic: the headline is missing, very short or generic, or there is no supporting line. A person should judge whether a first-time visitor gets it in five seconds.",
                    "Visitors who can't tell what this is leave before reading further.",
                    "header",
                    Recommendation {
                        before: (!h1.is_empty()).then(|| h1.clone()),
                        after: Some(format!("[Outcome you deliver] for [{}] — [how, in one line]", if ctx.setup.audience.role.is_empty() { "who it's for" } else { &ctx.setup.audience.role })),
                        ..rec("Unclear headline.", "Visitors decide in seconds whether a page is for them.", "Lead with the outcome and the audience, then one line on how.", "Template, not copy: write it in your own voice.", &["Five people shown the page for 5 seconds can say what it offers", "The H1 names an outcome, not a greeting"], "Run a 5-second test with a few people from the target audience.")
                    },
                ))
            } else {
                Out::fine("A headline and a supporting line are present. Whether they're clear is a judgement — glance at them.").saw(saw)
            }
        }
        "primary_cta" => match f.primary_cta(ctx.goal) {
            None => Out::fail("No call to action found").with(ctx.finding(
                "No clear next step",
                Severity::High,
                [5, 5, 4, 2],
                format!("No button or prominent link was found for the goal ({}).", ctx.goal.label().to_lowercase()),
                "Visitors who are ready to act don't know how.",
                "main",
                rec("Missing call to action.", "A page that wants an action has to ask for it.", &format!("Add one prominent button near the top: {}.", ctx.page_type.conversion().to_lowercase()), "If the CTA is added by JavaScript it was invisible to this check — confirm in a browser.", &["One primary button above the fold on desktop and mobile", "Its label names the action"], "Retest, and watch three people find it."),
            )),
            Some(cta) if !cta.serves(ctx.goal) => Out::review(format!("Main action “{}” doesn't obviously serve the goal", cta.label)).saw([format!("{} in {}", quote(&cta.label), region_name(cta.region))]).clue(format!("<{}>", cta.tag)).with(ctx.finding(
                "Main action doesn't match the goal",
                Severity::Medium,
                [4, 4, 3, 1],
                format!("The most prominent action reads “{}”, which doesn't say {}.", cta.label, ctx.goal.label().to_lowercase()),
                "Visitors may not realise this is how to do what they came for.",
                region_name(cta.region),
                Recommendation { before: Some(cta.label.clone()), after: Some(ctx.goal.action_words().first().map(|w| { let mut c = w.chars(); c.next().map(|x| x.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default() }).unwrap_or_else(|| "Get started".into())), ..rec("Vague main action.", "Labels that name the outcome get clicked more confidently.", "Relabel the main button with the action and outcome.", "Suggested label is a starting point.", &["The main button names the goal action"], "Retest; optionally A/B test the label.") },
            )),
            Some(cta) if cta.position > 0.5 => Out::review(format!("“{}” first appears late in the page", cta.label)).partial().saw([format!("{} at {:.0}% of the source", quote(&cta.label), cta.position * 100.0)]).with(ctx.finding(
                "Main action appears late",
                Severity::Medium,
                [3, 4, 2, 2],
                "In the HTML, the main action comes after half the page. It may still be visible early if CSS reorders it — confirm in a browser.",
                "Visitors have to scroll to find out how to act.",
                region_name(cta.region),
                rec("Late CTA.", "Ready visitors shouldn't have to hunt.", "Repeat the main action in the first screen.", "Source order is not visual order; check before changing.", &["Main action visible without scrolling on a 390×844 phone and 1280×800 desktop"], "Check at both sizes."),
            )),
            Some(cta) => Out::pass(format!("Main action: “{}”", cta.label)).saw([format!("{} in {}", quote(&cta.label), region_name(cta.region))]).clue(format!("<{}>", cta.tag)),
        },
        "cta_labels" => {
            let vague = ["click here", "here", "read more", "learn more", "more", "submit", "go", "link", "this"];
            let found: Vec<String> = f.links.iter().map(|l| &l.label).chain(f.ctas.iter().map(|c| &c.label)).filter(|l| vague.contains(&l.trim().to_ascii_lowercase().as_str())).map(|l| quote(l)).collect::<std::collections::BTreeSet<_>>().into_iter().collect();
            if found.is_empty() {
                Out::pass("No vague link or button labels")
            } else {
                Out::fail(format!("{} vague label{}", found.len(), if found.len() == 1 { "" } else { "s" })).saw(found.clone()).clue("<a>, <button>").with(ctx.finding(
                    "Links and buttons with vague labels",
                    Severity::Low,
                    [2, 3, 5, 1],
                    "Labels like “Learn more” or “Click here” don't say where they go — out of context (in a screen reader's link list) they are meaningless. WCAG 2.4.4.",
                    "Visitors hesitate, and screen reader users can't tell links apart.",
                    "main",
                    Recommendation { before: found.first().cloned(), after: Some("“See pricing plans” / “Read the case study”".into()), ..rec("Vague labels.", "Specific labels set expectations.", "Name the destination or result in the label.", "If design needs short text, add visually hidden context.", &["No link reads only “click here”, “learn more” or “read more”"], "Retest.") },
                ))
            }
        }
        "readability" => {
            let long_paragraphs = f.paragraphs.iter().filter(|w| **w > 120).count();
            if f.word_count < 40 {
                Out::not_tested("Too little text in the source to judge readability.")
            } else if f.sentence_length > 25.0 || long_paragraphs > 0 {
                Out::review(format!("Average sentence {:.0} words; {long_paragraphs} paragraph(s) over 120 words", f.sentence_length)).with(ctx.finding(
                    "Copy may be hard to scan",
                    Severity::Low,
                    [2, 4, 3, 2],
                    "Heuristic: long sentences or walls of text. Most visitors scan rather than read.",
                    "Key information gets skipped.",
                    "main",
                    rec("Dense copy.", "Scanners read the first words of short paragraphs.", "Aim for sentences under 20 words and paragraphs under 4 lines; use lists for features.", "", &["Average sentence under 20 words", "No paragraph over 120 words"], "Retest; read aloud."),
                ))
            } else {
                Out::fine(format!("Average sentence {:.0} words; no very long paragraphs", f.sentence_length))
            }
        }
        "competing_ctas" => {
            let mut labels: Vec<&str> = f.ctas.iter().filter(|c| c.commitment && !matches!(c.region, Region::Footer | Region::Nav)).map(|c| c.label.as_str()).collect();
            labels.sort_unstable();
            labels.dedup();
            if labels.len() > 4 {
                Out::review(format!("{} different commitment actions", labels.len())).saw(labels.iter().map(|l| quote(l))).with(ctx.finding(
                    "Too many competing actions",
                    Severity::Low,
                    [3, 4, 2, 2],
                    "Heuristic: several different calls to action compete for attention, so none reads as the obvious next step.",
                    "Choice overload: some visitors pick nothing.",
                    "main",
                    rec("Competing CTAs.", "One primary and one secondary action is easier to choose between.", "Keep one primary style for the goal action; demote the rest to secondary or text links.", "", &["One visually primary action per screen"], "Review with the team; retest."),
                ))
            } else {
                Out::fine(format!("{} distinct commitment action(s)", labels.len())).saw(labels.iter().map(|l| quote(l)))
            }
        }
        "nav_present" => {
            let nav = f.nav_links();
            if nav.len() >= 2 {
                Out::pass(format!("{} links in header and navigation", nav.len())).saw(nav.iter().take(10).map(|l| quote(&l.label)))
            } else if matches!(ctx.page_type, P::LeadGen | P::Waitlist) {
                Out::fine("Little or no navigation — usual for a focused landing page")
            } else {
                Out::review("No navigation found in the markup").with(ctx.finding(
                    "No visible way around the site",
                    Severity::Medium,
                    [3, 4, 3, 2],
                    "No navigation links were found in the header or a <nav>. If the menu is built by JavaScript, it was invisible here.",
                    "Visitors who need something else are stuck.",
                    "nav",
                    rec("Missing navigation.", "Visitors expect a menu at the top.", "Add a header with links to the key pages, inside a <nav> element.", "", &["A <nav> with the main sections exists in the HTML"], "Retest."),
                ))
            }
        }
        "nav_size" => {
            let n = f.nav_links().len();
            if n > 12 {
                Out::review(format!("{n} links in the header and navigation")).with(ctx.finding(
                    "Crowded navigation",
                    Severity::Low,
                    [2, 4, 2, 3],
                    "Heuristic: a long menu is slow to scan, especially on a phone.",
                    "Visitors miss the link they need.",
                    "nav",
                    rec("Long menu.", "Fewer top-level choices are faster to scan.", "Group items under 5–7 top-level labels; move the rest to the footer.", "Mega-menus can be fine — check with a tree test.", &["Seven or fewer top-level items"], "Run a tree test (e.g. with the Journeys segment)."),
                ))
            } else {
                Out::fine(format!("{n} navigation links"))
            }
        }
        "broken_links" => match ctx.crawl {
            None => Out::not_tested("This page is not in the crawl, so where its links lead is unknown."),
            Some(crawl) if crawl.broken_links.is_empty() => Out::pass("No link on this page led to an error in the crawl").kind(EvidenceKind::Network),
            Some(crawl) => Out::fail(format!("{} broken link{}", crawl.broken_links.len(), if crawl.broken_links.len() == 1 { "" } else { "s" }))
                .kind(EvidenceKind::Network)
                .saw(crawl.broken_links.iter().take(10).map(|(u, s)| format!("{s} {u}")))
                .steps(&["Open each URL listed", "Find the link on the page that points there"])
                .with(ctx.finding(
                    "Broken links",
                    Severity::High,
                    [4, 3, 5, 1],
                    "Links on this page lead to error pages.",
                    "Visitors hit a dead end and lose trust.",
                    "page",
                    rec("Links to error pages.", "Every dead end is a lost visitor.", "Update or remove each link; redirect removed pages to their replacements.", "", &["No link on the page leads to a 4xx or 5xx"], "Re-crawl, then retest."),
                )),
        },
        "redirects" => match ctx.crawl {
            None => Out::not_tested("This page is not in the crawl."),
            Some(crawl) if crawl.redirected_links <= 3 => Out::pass(format!("{} link(s) go through a redirect", crawl.redirected_links)).kind(EvidenceKind::Network),
            Some(crawl) => Out::review(format!("{} links go through a redirect", crawl.redirected_links)).kind(EvidenceKind::Network).with(ctx.finding(
                "Links go through redirects",
                Severity::Observation,
                [1, 3, 5, 1],
                "Links point at old URLs that redirect. Each hop adds a little delay.",
                "Slightly slower navigation.",
                "page",
                rec("Redirected links.", "Linking to the final URL saves a round trip.", "Update links to point at the final URLs.", "", &["Three or fewer redirected links"], "Re-crawl, then retest."),
            )),
        },
        "form_labels" => {
            let mut unlabelled = Vec::new();
            for form in f.task_forms() {
                for field in &form.fields {
                    if field.label.as_deref().is_none_or(|l| l.trim().is_empty()) {
                        unlabelled.push(field.display());
                    }
                }
            }
            if f.task_forms().is_empty() {
                Out::pass("No forms on this page — nothing to check")
            } else if unlabelled.is_empty() {
                Out::pass("Every form field has a label").partial()
            } else {
                Out::fail(format!("{} unlabelled field{}", unlabelled.len(), if unlabelled.len() == 1 { "" } else { "s" })).partial().saw(unlabelled.clone()).clue("<input>, <select>, <textarea> without <label>, aria-label or aria-labelledby").with(ctx.finding(
                    "Form fields without labels",
                    Severity::High,
                    [4, 3, 4, 1],
                    "Fields rely on placeholder text or nothing at all. Placeholders vanish as people type, and screen readers may announce just “edit text”. WCAG 1.3.1, 3.3.2, 4.1.2.",
                    "People forget what a field was for, and screen reader users can't fill the form in.",
                    "form",
                    Recommendation { before: Some("<input type=\"email\" placeholder=\"Email\">".into()), after: Some("<label for=\"email\">Work email</label>\n<input id=\"email\" type=\"email\" autocomplete=\"email\">".into()), ..rec("Unlabelled fields.", "A visible label is the most reliable way to say what a field wants.", "Give every field a visible <label> tied to it with for/id.", "Example markup.", &["Every field has a visible label", "A screen reader announces each label"], "Retest; tab through the form with VoiceOver or NVDA.") },
                ))
            }
        }
        "form_length" => {
            let longest = f.task_forms().into_iter().max_by_key(|form| form.fields.len());
            match longest {
                None => Out::pass("No forms on this page"),
                Some(form) if form.fields.len() > 6 => Out::review(form.summary()).saw(form.fields.iter().map(|x| x.display())).with(ctx.finding(
                    "Long form",
                    Severity::Medium,
                    [3, 4, 3, 2],
                    format!("Heuristic: the form asks for {} things. Every field is a reason to stop.", form.fields.len()),
                    "Fewer people finish the form.",
                    region_name(form.region),
                    rec("Long form.", "Each extra field adds effort before the reward.", "Remove anything not needed for the first contact; ask the rest later.", "Some long forms are justified (e.g. quotes); weigh it against lead quality.", &["Only fields needed to respond are required", "Six fields or fewer, or a multi-step form"], "Compare completion rates before and after."),
                )),
                Some(form) => Out::fine(form.summary()),
            }
        }
        "form_autocomplete" => {
            let missing: Vec<String> = f.task_forms().iter().flat_map(|form| form.fields.iter()).filter(|x| x.collects_contact() && x.autocomplete.is_none()).map(|x| x.display()).collect();
            if f.task_forms().is_empty() {
                Out::pass("No forms on this page")
            } else if missing.is_empty() {
                Out::pass("Contact fields declare autocomplete, or there are none")
            } else {
                Out::fail(format!("{} contact field(s) without autocomplete", missing.len())).saw(missing).clue("autocomplete attribute").with(ctx.finding(
                    "Fields don't help autofill",
                    Severity::Low,
                    [2, 3, 5, 1],
                    "Name, email and phone fields lack autocomplete, so browsers can't fill them in. WCAG 1.3.5.",
                    "Slower, more error-prone typing, especially on phones.",
                    "form",
                    rec("No autocomplete.", "Autofill turns typing into one tap.", "Add autocomplete=\"name\", \"email\", \"tel\" etc. to those fields.", "", &["Every contact field has a valid autocomplete value"], "Retest."),
                ))
            }
        }
        "viewport" => match f.viewport.as_deref() {
            None => Out::fail("No viewport meta tag").clue("<meta name=\"viewport\">").with({
                let mut finding = ctx.finding("Not set up for phones", Severity::High, [4, 4, 5, 1], "Without a viewport tag, phones render the page at desktop width and shrink it.", "Mobile visitors see tiny text and have to zoom.", "page", rec("Missing viewport.", "Phones need to be told to use their own width.", "Add <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">.", "", &["Viewport tag present"], "Retest; check on a phone."));
                finding.devices = "mobile".into();
                finding
            }),
            Some(v) => {
                let lower = v.to_ascii_lowercase().replace(' ', "");
                if lower.contains("user-scalable=no") || lower.contains("user-scalable=0") || lower.contains("maximum-scale=1,") || lower.ends_with("maximum-scale=1") || lower.contains("maximum-scale=1.0") {
                    Out::fail("Zoom is disabled").saw([v.to_string()]).clue("<meta name=\"viewport\">").with({
                        let mut finding = ctx.finding("Pinch-zoom is blocked", Severity::Medium, [3, 2, 5, 1], "The viewport tag stops people zooming. WCAG 1.4.4.", "Visitors with low vision can't enlarge text.", "page", Recommendation { before: Some(v.to_string()), after: Some("width=device-width, initial-scale=1".into()), ..rec("Zoom disabled.", "Zoom is how many people read small text.", "Remove user-scalable=no and maximum-scale from the viewport tag.", "", &["Pinch-zoom works on iOS and Android"], "Retest; try zooming on a phone.") });
                        finding.devices = "mobile".into();
                        finding
                    })
                } else {
                    Out::pass("Viewport set; zoom allowed").saw([v.to_string()])
                }
            }
        },
        "image_dimensions" => {
            if f.images == 0 {
                Out::pass("No images")
            } else if f.images_without_size == 0 {
                Out::pass(format!("All {} images declare width and height", f.images))
            } else {
                Out::review(format!("{} of {} images have no width/height", f.images_without_size, f.images)).partial().with(ctx.finding(
                    "Images may make the page jump",
                    Severity::Low,
                    [2, 4, 3, 1],
                    "Images without width and height attributes can shift the layout as they load, unless CSS reserves the space (not checked).",
                    "Visitors tap the wrong thing as content moves.",
                    "main",
                    rec("Missing image dimensions.", "Declared sizes let the browser reserve space.", "Add width and height attributes (or CSS aspect-ratio) to images.", "", &["Every image declares its size"], "Retest; measure CLS in a lab tool."),
                ))
            }
        }
        "focus_visible" => {
            if f.outline_removed && !f.focus_styles {
                Out::fail("Focus outline is removed with no replacement").partial().clue("outline: none / outline: 0 in page CSS").with(ctx.finding(
                    "Keyboard focus is invisible",
                    Severity::High,
                    [4, 2, 4, 1],
                    "The page's own CSS removes the focus outline and doesn't style :focus or :focus-visible. WCAG 2.4.7. External stylesheets were not checked.",
                    "Keyboard users can't see where they are.",
                    "page",
                    Recommendation { before: Some("*:focus { outline: none; }".into()), after: Some(":focus-visible { outline: 2px solid currentColor; outline-offset: 2px; }".into()), ..rec("No focus indicator.", "Keyboard users need to see focus.", "Remove the outline reset or replace it with a visible :focus-visible style.", "", &["Tabbing through the page shows a clear indicator on every control"], "Tab through the page in a browser.") },
                ))
            } else {
                Out::not_tested(if f.external_css > 0 {
                    "No focus-hiding CSS in the page itself, but focus visibility depends on external stylesheets and needs a browser to confirm."
                } else {
                    "No focus-hiding CSS found. Whether focus is clearly visible needs a browser to confirm."
                })
            }
        }
        "tab_order" => {
            if f.positive_tabindex == 0 {
                Out::pass("No positive tabindex")
            } else {
                Out::fail(format!("{} element(s) with a positive tabindex", f.positive_tabindex)).clue("[tabindex] > 0").with(ctx.finding(
                    "Forced keyboard order",
                    Severity::Medium,
                    [3, 2, 5, 1],
                    "Positive tabindex values override the natural order, so keyboard focus jumps around. WCAG 2.4.3.",
                    "Keyboard users lose their place.",
                    "page",
                    rec("Positive tabindex.", "Focus should follow reading order.", "Remove positive tabindex values; fix order in the HTML instead.", "", &["No tabindex greater than 0"], "Retest; tab through the page."),
                ))
            }
        }
        "https" => {
            if absolute(&f.url).starts_with("https://") {
                Out::pass("Served over HTTPS")
            } else {
                Out::fail("Served over plain HTTP").with(ctx.finding("Not secure", Severity::High, [4, 5, 5, 2], "The page is on HTTP. Browsers label it “Not secure”, especially next to forms.", "Visitors hesitate to enter anything.", "page", rec("No HTTPS.", "Browsers warn on HTTP pages.", "Serve the site over HTTPS and redirect HTTP to it.", "", &["The URL is https and HTTP redirects to it"], "Retest.")))
            }
        }
        "trust_signals" => {
            let found = page::trust_signals(f);
            if found.is_empty() {
                Out::review("No reviews, client names, guarantees or credentials found in the text").with(ctx.finding(
                    "Little proof on the page",
                    Severity::Medium,
                    [3, 5, 2, 2],
                    "Heuristic: none of the usual trust signals were found in the text. Logos and badges in images can't be read here — check visually.",
                    "Cautious visitors don't convert.",
                    "main",
                    rec("No visible proof.", "People look for evidence that others trusted this.", "Add one or two specific proofs near the main action: a named testimonial, client logos, a rating, or a guarantee.", "Make it real: invented testimonials are a legal risk.", &["At least one verifiable trust signal near the main action"], "Retest; ask five visitors what made them trust it."),
                ))
            } else {
                Out::fine(format!("{} trust signal(s) found", found.len())).saw(found)
            }
        }
        "contact_route" => {
            let link = f.link_to(&["contact", "get in touch", "support", "help"]);
            if !f.tel.is_empty() || !f.mailto.is_empty() || link.is_some() {
                let mut saw: Vec<String> = f.tel.iter().take(2).cloned().collect();
                saw.extend(f.mailto.iter().take(2).cloned());
                saw.extend(link.map(|l| quote(&l.label)));
                Out::pass("A contact route is on the page").saw(saw)
            } else {
                Out::fail("No phone, email or contact link").with(ctx.finding(
                    "No way to reach a person",
                    Severity::Medium,
                    [3, 3, 4, 1],
                    "There is no phone link, email link or link to a contact page.",
                    "Visitors with a question have nowhere to go.",
                    "footer",
                    rec("No contact route.", "A reachable human is a trust signal in itself.", "Add a contact link in the header or footer.", "", &["A contact link, phone or email is on the page"], "Retest."),
                ))
            }
        }
        "privacy_near_form" => {
            let forms: Vec<_> = f.task_forms().into_iter().filter(|form| form.collects_contact()).collect();
            if forms.is_empty() {
                Out::pass("No form collects contact details")
            } else if forms.iter().all(|form| form.privacy_note) || f.link_to(&["privacy"]).is_some() {
                Out::pass("Privacy is mentioned near the form or linked").partial()
            } else {
                Out::fail("A contact form with no privacy note or link").with(ctx.finding(
                    "No word on what happens to personal data",
                    Severity::Medium,
                    [3, 3, 4, 1],
                    "A form asks for contact details but the page says nothing about privacy and doesn't link a policy. In the EU and UK this is also a GDPR transparency issue.",
                    "Visitors worried about spam don't fill it in.",
                    "form",
                    Recommendation { after: Some("We'll only use your email to reply to this request. Privacy policy →".into()), ..rec("No privacy note.", "Reassurance at the moment of asking lifts completion.", "Add one line under the submit button saying how the data is used, linking the privacy policy.", "Not legal advice.", &["A privacy line or link sits next to the form"], "Retest.") },
                ))
            }
        }
        "goal_journey" => journey_check(ctx),
        "support_journeys" => match ctx.crawl {
            None => Out::not_tested("This page is not in the crawl."),
            Some(crawl) => {
                let support: Vec<_> = crawl.journeys.iter().filter(|j| !j.primary).collect();
                if support.is_empty() {
                    return Out::not_tested("No supporting journeys were simulated.");
                }
                let missed: Vec<_> = support.iter().filter(|j| j.outcome != "reached").collect();
                let saw = support.iter().map(|j| format!("{}: {} in {} click(s)", j.goal_label, j.outcome.replace('_', " "), j.clicks));
                if missed.is_empty() {
                    Out::fine("Pricing, contact and privacy were all reached").kind(EvidenceKind::Journey).saw(saw)
                } else {
                    Out::review(format!("{} supporting page(s) not reached", missed.len())).kind(EvidenceKind::Journey).saw(saw).with(ctx.finding(
                        "Supporting pages are hard to find",
                        Severity::Low,
                        [2, 3, 3, 2],
                        format!("The simulated visitor could not reach: {}.", missed.iter().map(|j| j.goal_label.as_str()).collect::<Vec<_>>().join(", ")),
                        "Visitors who need reassurance before acting can't find it.",
                        "nav",
                        rec("Hard-to-find pages.", "Visitors check pricing, contact and privacy before committing.", "Link these from the header or footer with plain labels.", "Simulation, not a recording.", &["Each supporting page is reachable in three clicks or fewer"], "Retest; see Journeys for the path."),
                    ))
                }
            }
        },
        "cta_consistency" => {
            let mut labels: Vec<&str> = f.ctas.iter().filter(|c| c.serves(ctx.goal)).map(|c| c.label.as_str()).collect();
            labels.sort_unstable();
            labels.dedup();
            if labels.len() >= 3 {
                Out::review(format!("The goal action has {} different labels", labels.len())).saw(labels.iter().map(|l| quote(l))).with(ctx.finding(
                    "The same action has different names",
                    Severity::Low,
                    [2, 3, 3, 1],
                    "Heuristic: several labels lead to what looks like the same action. Visitors may wonder if they are different.",
                    "Hesitation.",
                    "page",
                    rec("Inconsistent labels.", "Consistent naming builds confidence.", "Use one label for the main action everywhere.", "", &["One label for the main action"], "Retest."),
                ))
            } else {
                Out::fine("The goal action is named consistently").saw(labels.iter().map(|l| quote(l)))
            }
        }
        // ── Page-type checks
        "saas_pricing" => {
            if let Some(price) = f.price() {
                Out::pass("A price is on the page").saw([price])
            } else if let Some(link) = f.link_to(&["pricing", "plans", "price"]) {
                Out::pass("Links to pricing").saw([quote(&link.label)])
            } else {
                Out::fail("No price and no link to pricing").with(ctx.finding("Pricing is hidden", Severity::High, [4, 5, 4, 2], "There is no price and no link to a pricing page.", "Evaluators can't tell if it fits their budget, and leave to check competitors.", "nav", rec("No pricing.", "Price is one of the first things buyers look for.", "Add a Pricing link to the main navigation; if pricing is custom, say “Pricing” and explain how it works.", "", &["Pricing reachable in one click from this page"], "Retest.")))
            }
        }
        "saas_friction" => match f.says_any(&["no credit card", "free trial", "free forever", "free plan", "cancel anytime", "start free"]) {
            Some(word) => Out::fine("Trial terms are stated").saw([f.excerpt(word).unwrap_or_else(|| word.to_string())]),
            None => Out::review("No mention of trial terms (credit card, free plan, cancel anytime)").with(ctx.finding("Trial terms aren't stated", Severity::Low, [3, 4, 2, 1], "Heuristic: the page doesn't say whether trying it needs a card or a call.", "Visitors fear a commitment and don't start.", "main", Recommendation { after: Some("Free for 14 days · No credit card required".into()), ..rec("Unknown commitment.", "Lower perceived risk lifts sign-ups.", "Add a short line under the main button stating the trial terms.", "Only state what is true.", &["Trial terms are stated next to the main action"], "Retest.") })),
        },
        "saas_security" => match f.says_any(&["security", "secure", "gdpr", "soc 2", "iso 27001", "encrypted", "encryption", "privacy"]) {
            Some(word) => Out::fine("Security or privacy is mentioned").saw([f.excerpt(word).unwrap_or_else(|| word.to_string())]),
            None => Out::review("Nothing about security or compliance").with(ctx.finding("Security isn't addressed", Severity::Low, [2, 3, 2, 2], "Heuristic: business buyers look for security and compliance before trusting a product with data.", "Evaluations stall at procurement.", "main", rec("No security info.", "Buyers need it for sign-off.", "Add a security section or link (encryption, hosting, certifications).", "Only claim certifications you hold.", &["Security information reachable from this page"], "Retest."))),
        },
        "local_phone" => {
            if f.tel.is_empty() {
                Out::fail("No click-to-call link").with({
                    let mut finding = ctx.finding("Visitors can't tap to call", Severity::High, [4, 4, 5, 1], "There is no tel: link. On a phone, a number in plain text has to be copied by hand.", "Callers give up and ring a competitor.", "header", Recommendation { after: Some("<a href=\"tel:+15551234567\">Call (555) 123-4567</a>".into()), ..rec("No tap-to-call.", "Local visitors on phones want to call.", "Put the phone number in the header as a tel: link.", "", &["A tel: link is in the header"], "Retest; tap it on a phone.") });
                    finding.devices = "mobile".into();
                    finding
                })
            } else {
                Out::pass("Click-to-call link present").saw(f.tel.iter().take(2).cloned())
            }
        }
        "local_hours" => match f.opening_hours() {
            Some(hours) => Out::pass("Opening hours shown").saw([hours]),
            None => Out::fail("No opening hours found").partial().with(ctx.finding("Opening hours are missing", Severity::Medium, [3, 4, 3, 1], "No opening hours were found in the text or structured data.", "Visitors don't know whether to call now.", "main", rec("No hours.", "“Are they open?” is a top local question.", "Show hours near the contact details and in LocalBusiness structured data.", "", &["Hours visible on the page"], "Retest."))),
        },
        "local_area" => {
            if f.has_address || f.says_any(&["areas we serve", "service area", "serving", "we cover", "located in"]).is_some() {
                Out::fine("An address or service area is mentioned")
            } else {
                Out::review("No address or service area").with(ctx.finding("Unclear where they operate", Severity::Medium, [3, 4, 2, 1], "Heuristic: no street address or service area was found.", "Visitors can't tell if they're covered.", "main", rec("No location.", "Local services must say where.", "State the address or the towns you serve.", "", &["Address or service area on the page"], "Retest.")))
            }
        }
        "local_reviews" | "shop_reviews" => {
            if f.says_any(&["review", "reviews", "testimonial", "rated", "stars", "★"]).is_some() || f.has_schema(&["Review", "AggregateRating"]).is_some() {
                Out::fine("Reviews or ratings are mentioned")
            } else {
                Out::review("No reviews or ratings found").with(ctx.finding("No reviews", Severity::Low, [3, 4, 2, 2], "Heuristic: no reviews or ratings were found in the text. Review widgets loaded by script would not show here.", "Hesitant visitors look elsewhere.", "main", rec("No social proof.", "Reviews are the strongest trust signal for local and retail.", "Show a few genuine reviews with names, or embed your rating.", "Never write reviews yourself.", &["Reviews or a rating visible"], "Retest.")))
            }
        }
        "shop_price" => match f.price() {
            Some(price) => Out::pass("Price shown").saw([price]),
            None => Out::fail("No price found").with(ctx.finding("No price on the product page", Severity::Critical, [5, 5, 4, 1], "No price was found in the text or Offer structured data.", "Shoppers can't decide, and leave.", "main", rec("Missing price.", "Price is essential to a purchase decision.", "Show the price next to the product name and the add-to-cart button.", "If the price is loaded by script, confirm it shows in a browser.", &["Price visible next to Add to cart"], "Retest."))),
        },
        "shop_cart" => match f.ctas.iter().find(|c| c.serves(Goal::Purchase)) {
            Some(cta) => Out::pass(format!("“{}”", cta.label)).saw([quote(&cta.label)]),
            None => Out::fail("No add-to-cart or buy button").with(ctx.finding("No way to buy", Severity::Critical, [5, 5, 4, 2], "No add-to-cart or buy button was found.", "The page can't sell.", "main", rec("No buy button.", "The page exists to sell this product.", "Add a prominent Add to cart button next to the price.", "If it's rendered by script, confirm in a browser.", &["An Add to cart button is visible without scrolling"], "Retest."))),
        },
        "shop_delivery" => match f.says_any(&["delivery", "shipping", "dispatch", "returns", "return policy", "refund"]) {
            Some(word) => Out::fine("Delivery or returns mentioned").saw([f.excerpt(word).unwrap_or_else(|| word.to_string())]),
            None => Out::review("No delivery or returns information").with(ctx.finding("Delivery and returns not explained", Severity::Medium, [3, 5, 3, 1], "Heuristic: nothing about delivery times, costs or returns was found.", "Surprise costs are a top reason for abandoned carts.", "main", rec("No delivery info.", "Shoppers want to know when and what it costs.", "Add delivery time, cost and the returns window near the buy button.", "", &["Delivery and returns stated near Add to cart"], "Retest."))),
        },
        "shop_stock" => match f.says_any(&["in stock", "out of stock", "available", "sold out", "ships in", "low stock"]) {
            Some(word) => Out::fine("Availability is stated").saw([f.excerpt(word).unwrap_or_else(|| word.to_string())]),
            None => Out::review("Availability isn't stated").with(ctx.finding("Stock status unclear", Severity::Low, [2, 4, 2, 1], "Heuristic: no in-stock or availability wording found.", "Shoppers aren't sure it will ship.", "main", rec("No availability.", "Certainty helps people buy.", "Show stock status by the price.", "", &["Availability shown"], "Retest."))),
        },
        "agency_work" => match f.link_to(&["work", "portfolio", "case stud", "projects", "clients"]) {
            Some(link) => Out::pass("Links to work samples").saw([quote(&link.label)]),
            None if f.says_any(&["case study", "case studies", "portfolio", "our work"]).is_some() => Out::pass("Mentions work samples on the page"),
            None => Out::fail("No work samples or link to them").with(ctx.finding("No proof of past work", Severity::High, [4, 5, 4, 2], "No portfolio, case studies or link to them were found.", "Prospects can't judge if you're right for them.", "nav", rec("No portfolio.", "Buyers hire on evidence of similar work.", "Show 2–3 relevant projects with outcomes, and link to the rest.", "", &["Work samples reachable in one click"], "Retest."))),
        },
        "agency_people" => match f.link_to(&["about", "team", "who we are", "people"]) {
            Some(link) => Out::fine("Links to who they are").saw([quote(&link.label)]),
            None => Out::review("No about or team link").with(ctx.finding("Who's behind it is unclear", Severity::Low, [2, 4, 2, 1], "Heuristic: no about or team page link.", "Prospects can't picture working with you.", "nav", rec("Faceless.", "People hire people.", "Add an About or Team page with real names and photos.", "", &["About/Team link present"], "Retest."))),
        },
        "lead_next" => match f.says_any(&["we will", "we'll", "within 24", "within one", "you'll receive", "you will receive", "next step", "what happens next", "we reply"]) {
            Some(word) => Out::fine("Says what happens next").saw([f.excerpt(word).unwrap_or_else(|| word.to_string())]),
            None => Out::review("Doesn't say what happens after the form").with(ctx.finding("Next step after the form is unclear", Severity::Medium, [3, 4, 2, 1], "Heuristic: nothing says what happens after submitting.", "Uncertainty stops people submitting.", "form", Recommendation { after: Some("We'll reply within one working day with [what they get].".into()), ..rec("Unknown next step.", "Setting expectations lowers anxiety.", "Add one line by the button saying what happens and when.", "Only promise what you deliver.", &["A next-step line near the submit button"], "Retest.") })),
        },
        "lead_focus" => {
            let exits = f.links.iter().filter(|l| !matches!(l.region, Region::Footer)).count();
            if exits > 15 {
                Out::review(format!("{exits} links outside the footer")).with(ctx.finding("Many ways to leave the form", Severity::Low, [2, 4, 2, 2], "Heuristic: a lead page with many links gives visitors many ways to wander off.", "Fewer completed forms.", "nav", rec("Distractions.", "Focused landing pages convert better.", "Trim navigation on this page to the essentials.", "Test it: it isn't always better.", &["15 or fewer links outside the footer"], "A/B test the trimmed page.")))
            } else {
                Out::fine(format!("{exits} links outside the footer"))
            }
        }
        "waitlist_when" => match f.says_any(&["launching", "launch", "coming", "available in", "early 20", "late 20", "spring", "summer", "autumn", "fall", "winter", "q1", "q2", "q3", "q4"]) {
            Some(word) => Out::fine("Launch timing is mentioned").saw([f.excerpt(word).unwrap_or_else(|| word.to_string())]),
            None => Out::review("No launch timing").with(ctx.finding("No idea when it launches", Severity::Low, [2, 4, 2, 1], "Heuristic: nothing says when it's coming.", "Sign-ups feel like a black hole.", "main", rec("No timing.", "A rough date makes signing up concrete.", "Give an honest window (e.g. “early 2027”).", "", &["Launch window stated"], "Retest."))),
        },
        "waitlist_spam" => match f.says_any(&["no spam", "unsubscribe", "never share", "only email", "we respect"]) {
            Some(word) => Out::fine("Says how the email will be used").saw([f.excerpt(word).unwrap_or_else(|| word.to_string())]),
            None => Out::review("No no-spam promise").with(ctx.finding("No reassurance about email", Severity::Low, [2, 4, 2, 1], "Heuristic: the page doesn't say how the email address will be used.", "People hold back their address.", "form", Recommendation { after: Some("One email when we launch. No spam, unsubscribe anytime.".into()), ..rec("No reassurance.", "Fear of spam is the main objection.", "Add a short line under the field.", "", &["Usage line by the email field"], "Retest.") })),
        },
        "content_byline" => {
            let mut missing = Vec::new();
            if !f.has_author {
                missing.push("author");
            }
            if !f.has_published_date {
                missing.push("date");
            }
            if missing.is_empty() {
                Out::pass("Author and date are marked up")
            } else {
                Out::fail(format!("No {}", missing.join(" or "))).with(ctx.finding("Missing byline", Severity::Low, [2, 4, 4, 1], format!("No {} found in the markup.", missing.join(" or ")), "Readers can't judge expertise or freshness.", "main", rec("No byline.", "Who and when are credibility cues.", "Show the author's name and the publication or update date.", "", &["Author and date visible and in markup"], "Retest.")))
            }
        }
        "content_next" => {
            if f.link_to(&["related", "next", "read more", "subscribe", "newsletter"]).is_some() || f.primary_cta(ctx.goal).is_some_and(|c| c.commitment) {
                Out::fine("There is somewhere to go next")
            } else {
                Out::review("No obvious next step for a reader").with(ctx.finding("Dead end after reading", Severity::Low, [2, 4, 2, 1], "Heuristic: no related reading, subscription or call to action found.", "Readers leave after one page.", "main", rec("No next step.", "Engaged readers are ready for one more step.", "End with related articles and one relevant call to action.", "", &["A next step after the article"], "Retest.")))
            }
        }
        "content_structure" => {
            let sections = f.headings.iter().filter(|(l, _)| *l >= 2).count();
            if f.word_count > 1500 && sections < 4 {
                Out::fail(format!("{} words with {sections} subheading(s)", f.word_count)).with(ctx.finding("Long text with few subheadings", Severity::Low, [2, 4, 4, 2], "A long piece has very few subheadings to scan by.", "Readers can't find the part they need.", "main", rec("Wall of text.", "Subheadings let people skim to what matters.", "Add an H2 every 300–400 words, and a table of contents for long pieces.", "", &["A subheading every 400 words or so"], "Retest.")))
            } else {
                Out::pass(format!("{} words, {sections} subheading(s)", f.word_count))
            }
        }
        "market_search" => {
            if f.has_search {
                Out::pass("Search box present")
            } else {
                Out::fail("No search box").with(ctx.finding("No way to search", Severity::High, [4, 4, 4, 3], "No search input was found.", "Visitors who know what they want can't find it.", "header", rec("No search.", "Search is the main path in a large catalogue.", "Add a search box in the header.", "", &["A search field is present"], "Retest.")))
            }
        }
        "market_filters" => {
            if f.filter_controls >= 2 {
                Out::pass(format!("{} filter or sort controls", f.filter_controls))
            } else {
                Out::review("Few or no filter controls").partial().with(ctx.finding("Hard to narrow results", Severity::Medium, [3, 4, 2, 3], "Fewer than two filter or sort controls were found. Filters built by script may not show here.", "Visitors scroll endlessly or give up.", "main", rec("No filters.", "Filtering is how people cope with choice.", "Add filters for the attributes people choose by (price, location, rating) and a sort.", "", &["At least price and one category filter"], "Retest.")))
            }
        }
        "market_how" => match f.says_any(&["how it works", "how does it work", "step 1", "1.", "getting started"]) {
            Some(word) => Out::fine("Explains how it works").saw([f.excerpt(word).unwrap_or_else(|| word.to_string())]),
            None => Out::review("No explanation of how it works").with(ctx.finding("How it works isn't explained", Severity::Low, [2, 4, 2, 2], "Heuristic: no how-it-works section found.", "First-time visitors don't trust the process.", "main", rec("No explanation.", "Marketplaces ask people to trust strangers.", "Add a short how-it-works section: three steps, plus how payment and disputes are handled.", "", &["How-it-works visible"], "Retest."))),
        },
        other => Out::not_tested(format!("No evaluator for check {other}.")),
    }
}

fn journey_check(ctx: &Ctx) -> Out {
    let Some(crawl) = ctx.crawl else {
        return Out::not_tested("This page is not in the crawl, so journeys can't be simulated.");
    };
    let Some(journey) = crawl.journeys.iter().find(|j| j.primary) else {
        return Out::not_tested(format!("No journey is defined for the goal “{}”. Use the Journeys segment to test a task in your own words.", ctx.goal.label()));
    };
    let path = if journey.path.is_empty() { "(no clicks)".to_string() } else { journey.path.join(" → ") };
    let saw = [format!("Path: {path}"), format!("Outcome: {} in {} click(s)", journey.outcome.replace('_', " "), journey.clicks)];
    let base = |out: Out| out.kind(EvidenceKind::Journey).saw(saw.clone()).steps(&["Start at the home page", "Follow the link text in the path above"]);
    if journey.outcome == "reached" && journey.clicks <= 3 {
        return base(Out::fine(format!("{} reached in {} click(s)", journey.goal_label, journey.clicks)));
    }
    if journey.outcome == "reached" {
        return base(Out::review(format!("{} reached, but it took {} clicks", journey.goal_label, journey.clicks))).with(ctx.finding(
            "The goal is buried",
            Severity::Medium,
            [3, 4, 3, 2],
            format!("The simulated visitor needed {} clicks to reach {}.", journey.clicks, journey.goal_label.to_lowercase()),
            "Each extra click loses visitors.",
            "nav",
            rec("Deep goal.", "Important pages should be close to the front door.", "Link the goal page from the header or the home page.", "Simulation over the crawl's link graph.", &["Goal reachable in three clicks or fewer"], "Retest; compare paths in Journeys."),
        ));
    }
    let (title, explanation) = if journey.target_exists {
        ("Visitors can't find their way to the goal", format!("A page for {} exists, but the simulated visitor following link text didn't reach it ({}).", journey.goal_label.to_lowercase(), journey.outcome.replace('_', " ")))
    } else {
        ("No page for the goal was found", format!("No crawled page looks like where {} happens.", journey.goal_label.to_lowercase()))
    };
    base(Out::fail(format!("{}: {}", journey.goal_label, journey.outcome.replace('_', " ")))).partial().with(ctx.finding(
        title,
        Severity::High,
        [5, 4, 3, 3],
        explanation,
        "Visitors who want to act can't.",
        "nav",
        rec("Goal not reachable.", "If visitors can't get there, nothing else matters.", "Add a clearly labelled link to the goal page from the header and home page.", "A model of a visitor, not a recording. Pages not crawled count as unreachable.", &["The simulated visitor reaches the goal", "Three or fewer clicks"], "Retest; then run a first-click test with real people."),
    ))
}
