//! UX Autopilot: inspect a page, test it, prove what was found, rank it,
//! recommend a fix, retest, compare.
//!
//! One audit is one URL, a goal and a depth. The page is read, classified
//! (SaaS, local business, shop …), and a test plan is built from the shared
//! baseline every page gets plus the checks that matter for its type. Each
//! check ends in exactly one of five states — pass, fail, needs review, not
//! tested, blocked — and those are never folded into one pass rate, because a
//! check that could not run is not a check that passed.
//!
//! # What this can and cannot see
//!
//! There is no browser here. The evidence is the page's markup (stored by the
//! crawl or fetched once), the crawl's own data about links, images and
//! timing, and the journey simulator. So:
//!
//! - anything decidable from markup is tested and reported with the DOM clue
//!   that decided it;
//! - anything that is a judgement (is this headline clear?) is a *heuristic*
//!   and can only say "needs review" or raise an observation;
//! - anything that needs pixels, a keyboard, a script engine or a submitted
//!   form is reported as **not tested** or **blocked**, with the reason, and
//!   never as a pass.
//!
//! No screenshot is ever claimed, and no conversion lift is ever promised.

pub mod checks;
pub mod classify;
pub mod compare;
pub mod context;
pub mod page;
pub mod report;
pub mod store;

use serde::{Deserialize, Serialize};

use crate::ux;

pub use checks::{library, CheckDef};
pub use classify::Classification;
pub use compare::{compare, Comparison};
pub use page::PageFacts;

macro_rules! vocabulary {
    ($(#[$meta:meta])* $name:ident { $($variant:ident = $id:literal, $label:literal;)* }) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
        #[serde(rename_all = "snake_case")]
        pub enum $name { $($variant),* }

        impl $name {
            pub const ALL: &'static [$name] = &[$($name::$variant),*];

            pub fn as_str(self) -> &'static str {
                match self { $($name::$variant => $id),* }
            }

            pub fn label(self) -> &'static str {
                match self { $($name::$variant => $label),* }
            }

            pub fn parse(value: &str) -> Option<Self> {
                match value { $($id => Some($name::$variant),)* _ => None }
            }
        }
    };
}

vocabulary! {
    /// What kind of page this is, which decides what it is for.
    PageType {
        Saas = "saas", "SaaS product";
        LocalBusiness = "local_business", "Local business or service";
        Ecommerce = "ecommerce", "Ecommerce product";
        Agency = "agency", "Agency or freelancer";
        LeadGen = "lead_gen", "Lead generation";
        Waitlist = "waitlist", "Waitlist or launch";
        Content = "content", "Content or campaign";
        Marketplace = "marketplace", "Marketplace or directory";
        Other = "other", "Other";
    }
}

impl PageType {
    /// The questions a visitor to this kind of page needs answered.
    pub fn questions(self) -> &'static [&'static str] {
        match self {
            PageType::Saas => &[
                "What does it do, and for whom?",
                "What does it cost?",
                "Can I try it without talking to anyone?",
                "Is it safe to trust with my data?",
            ],
            PageType::LocalBusiness => &[
                "Do they serve my area?",
                "When are they open, and how do I reach them?",
                "Are they any good?",
                "How do I book?",
            ],
            PageType::Ecommerce => &[
                "What exactly is this, and what does it cost?",
                "Is it in stock in my size or variant?",
                "When will it arrive, and can I send it back?",
                "Have other people been happy with it?",
            ],
            PageType::Agency => &[
                "What do they do?",
                "Have they done work like mine?",
                "Who are they?",
                "How do I start a conversation?",
            ],
            PageType::LeadGen => &[
                "What do I get?",
                "What happens after I fill this in?",
                "What will you do with my details?",
            ],
            PageType::Waitlist => &[
                "What is launching, and when?",
                "What do I get for signing up early?",
                "Will I be spammed?",
            ],
            PageType::Content => &[
                "Is this worth my time?",
                "Who wrote it, and when?",
                "Where do I go next?",
            ],
            PageType::Marketplace => &[
                "Can I find what I'm looking for?",
                "Can I narrow the results down?",
                "Can I trust the sellers or listings?",
                "How does it work?",
            ],
            PageType::Other => &["What is this page for?", "What should I do next?"],
        }
    }

    /// The action the page exists to get.
    pub fn conversion(self) -> &'static str {
        match self {
            PageType::Saas => "Start a trial, sign up or book a demo",
            PageType::LocalBusiness => "Call, book or request a quote",
            PageType::Ecommerce => "Add to cart and check out",
            PageType::Agency => "Get in touch about a project",
            PageType::LeadGen => "Submit the form",
            PageType::Waitlist => "Join the list",
            PageType::Content => "Read, then take the next step",
            PageType::Marketplace => "Search, then open a listing",
            PageType::Other => "Take the page's main action",
        }
    }

    pub fn default_goal(self) -> Goal {
        match self {
            PageType::Saas => Goal::Trial,
            PageType::LocalBusiness => Goal::Booking,
            PageType::Ecommerce => Goal::Purchase,
            PageType::Agency | PageType::LeadGen => Goal::Lead,
            PageType::Waitlist => Goal::Signup,
            PageType::Content | PageType::Marketplace => Goal::Engagement,
            PageType::Other => Goal::Contact,
        }
    }
}

vocabulary! {
    /// What a visit should end in.
    Goal {
        Purchase = "purchase", "Purchase";
        Lead = "lead", "Lead";
        Booking = "booking", "Booking";
        Trial = "trial", "Trial";
        Signup = "signup", "Sign-up";
        Contact = "contact", "Contact";
        Engagement = "engagement", "Engagement";
        Other = "other", "Other";
    }
}

impl Goal {
    /// Words an action that serves this goal is likely to use.
    pub fn action_words(self) -> &'static [&'static str] {
        match self {
            Goal::Purchase => &["buy", "add to cart", "add to basket", "add to bag", "checkout", "check out", "order", "purchase", "shop now"],
            Goal::Lead => &["quote", "demo", "consultation", "get in touch", "contact", "enquire", "inquire", "request", "talk to", "call", "download", "get the guide", "apply"],
            Goal::Booking => &["book", "schedule", "appointment", "reserve", "availability", "call"],
            Goal::Trial => &["trial", "try", "get started", "start free", "start for free", "sign up", "demo"],
            Goal::Signup => &["sign up", "signup", "join", "register", "create account", "subscribe", "waitlist", "notify me", "get early access", "get access"],
            Goal::Contact => &["contact", "get in touch", "call", "email", "message", "talk to", "chat"],
            Goal::Engagement => &["read", "explore", "browse", "search", "discover", "watch", "see", "view", "learn"],
            Goal::Other => &[],
        }
    }

    /// The preset journey that ends where this goal is fulfilled, when one
    /// fits; the task text for a custom one otherwise.
    pub fn journey(self) -> Option<(&'static str, &'static str)> {
        match self {
            Goal::Purchase => Some(("custom", "checkout cart basket")),
            Goal::Lead | Goal::Contact => Some(("contact", "")),
            Goal::Booking => Some(("custom", "book appointment booking")),
            Goal::Trial | Goal::Signup => Some(("signup", "")),
            Goal::Engagement | Goal::Other => None,
        }
    }
}

vocabulary! {
    Device {
        Desktop = "desktop", "Desktop";
        Mobile = "mobile", "Mobile";
        Tablet = "tablet", "Tablet";
        All = "all", "All devices";
    }
}

vocabulary! {
    /// How far an audit goes. Ordered: a deep audit includes everything a
    /// standard one does.
    Depth {
        Quick = "quick", "Quick";
        Standard = "standard", "Standard";
        Deep = "deep", "Deep";
    }
}

vocabulary! {
    /// The test library's shelves.
    Category {
        Clarity = "clarity", "Clarity and content";
        Hierarchy = "hierarchy", "Visual hierarchy";
        Navigation = "navigation", "Navigation";
        Forms = "forms", "Forms";
        Responsive = "responsive", "Responsive";
        Accessibility = "accessibility", "Accessibility";
        Trust = "trust", "Trust and risk";
        Performance = "performance", "Performance signals";
        Journey = "journey", "Journey completion";
        Consistency = "consistency", "Consistency";
    }
}

vocabulary! {
    /// How a check gets its answer — which is how much to believe it.
    Method {
        Automated = "automated", "Automated: decided from the markup";
        Crawl = "crawl", "Crawl data: links, images and timing";
        Heuristic = "heuristic", "Heuristic: a judgement worth confirming";
        Browser = "browser", "Needs a real browser";
        Manual = "manual", "Needs a person";
    }
}

vocabulary! {
    /// The five outcomes of a check. Never collapsed into one pass rate.
    Status {
        Pass = "pass", "Pass";
        Fail = "fail", "Fail";
        NeedsReview = "needs_review", "Needs review";
        NotTested = "not_tested", "Not tested";
        Blocked = "blocked", "Blocked";
    }
}

impl Status {
    /// Whether the check actually ran.
    pub fn ran(self) -> bool {
        matches!(self, Status::Pass | Status::Fail | Status::NeedsReview)
    }
}

vocabulary! {
    /// How bad a finding is. Kept apart from how sure we are and how hard it
    /// is to fix — those are separate columns, not ingredients of this one.
    Severity {
        Critical = "critical", "Critical";
        High = "high", "High";
        Medium = "medium", "Medium";
        Low = "low", "Low";
        Observation = "observation", "Observation";
    }
}

vocabulary! {
    EvidenceStatus {
        Verified = "verified", "Verified";
        Partial = "partial", "Partially verified";
        Inferred = "inferred", "Inferred";
        NeedsHuman = "needs_human", "Needs human review";
    }
}

vocabulary! {
    EvidenceKind {
        Dom = "dom", "DOM and accessibility tree";
        Network = "network", "Network and crawl data";
        Journey = "journey", "Simulated journey";
        Note = "note", "Reviewer note";
        Unavailable = "none", "No evidence collected";
    }
}

impl EvidenceKind {
    /// What this kind of evidence cannot show. Every piece of evidence says so.
    pub fn limitation(self) -> &'static str {
        match self {
            EvidenceKind::Dom => "Read from the page's HTML source. CSS and JavaScript were not run, so what is hidden, moved or added after load is not reflected.",
            EvidenceKind::Network => "From the crawl and a single request. It describes what the server sent, not what a visitor's browser did with it.",
            EvidenceKind::Journey => "A simulation over the crawled link graph that follows link text. It is a model of a visitor, not a recording of one.",
            EvidenceKind::Note => "A person's judgement, recorded as given.",
            EvidenceKind::Unavailable => "This check needs something CrawlStat does not have, so nothing was collected.",
        }
    }
}

vocabulary! {
    /// Where a finding sits in its workflow.
    WorkStatus {
        Open = "open", "Open";
        Planned = "planned", "Planned";
        InProgress = "in_progress", "In progress";
        ReadyToRetest = "ready_to_retest", "Ready to retest";
        Resolved = "resolved", "Resolved";
        KnownLimitation = "known_limitation", "Accepted as known limitation";
    }
}

/// Who the page is for.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Audience {
    pub role: String,
    /// `new`, `returning` or `expert`.
    pub familiarity: String,
    pub needs: String,
    /// `search`, `ads`, `social`, `email`, `referral`, `direct` or `unknown`.
    pub traffic_source: String,
}

/// Everything the tester decided before the audit ran.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Setup {
    pub url: String,
    /// `None` to detect it.
    pub page_type: Option<PageType>,
    /// `None` to infer it from the page type.
    pub goal: Option<Goal>,
    pub audience: Audience,
    pub device: Device,
    pub depth: Depth,
    pub constraints: String,
    pub context: String,
    /// Behaviour model for journeys: a [`ux::Persona`] id.
    pub persona: String,
    /// The tester may let CrawlStat fetch the page live.
    pub allow_fetch: bool,
}

impl Setup {
    pub fn new(url: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            page_type: None,
            goal: None,
            audience: Audience::default(),
            device: Device::All,
            depth: Depth::Standard,
            constraints: String::new(),
            context: String::new(),
            persona: ux::Persona::TaskOriented.as_str().to_string(),
            allow_fetch: true,
        }
    }

    pub fn persona(&self) -> ux::Persona {
        ux::Persona::parse(&self.persona).unwrap_or(ux::Persona::TaskOriented)
    }
}

/// Where the markup came from, and what the server said about it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Source {
    pub html: String,
    /// `stored` (from the crawl file) or `fetched` (one live request).
    pub origin: String,
    pub status: Option<u16>,
    pub response_ms: Option<u64>,
    pub bytes: Option<u64>,
    /// Unix seconds.
    pub fetched_at: i64,
}

/// What the page is, said back to the tester before anything is judged.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Understanding {
    pub title: Option<String>,
    pub h1: Option<String>,
    /// The meta description, or the first real paragraph.
    pub offer: Option<String>,
    pub main_cta: Option<String>,
    pub other_ctas: Vec<String>,
    /// One line per form: "3 fields → Send".
    pub forms: Vec<String>,
    pub nav: Vec<String>,
    pub trust: Vec<String>,
    pub word_count: u32,
    /// The visit this page seems built for, in a sentence.
    pub intent: String,
}

/// What a check saw, and how far to trust it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Evidence {
    pub kind: EvidenceKind,
    pub status: EvidenceStatus,
    pub url: String,
    /// Always says when no viewport was rendered, rather than inventing one.
    pub viewport: String,
    pub browser: String,
    /// Unix seconds.
    pub tested_at: i64,
    /// Text exactly as found (redacted), one item per line.
    pub observed: Vec<String>,
    /// Where in the markup to look.
    pub dom_clue: Option<String>,
    /// How to see it for yourself.
    pub steps: Vec<String>,
    pub limitation: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Recommendation {
    pub problem: String,
    pub why: String,
    pub change: String,
    /// Current copy or layout, when there is something concrete to quote.
    pub before: Option<String>,
    /// A proposal for the same slot. A template, labelled as one.
    pub after: Option<String>,
    pub notes: String,
    pub acceptance: Vec<String>,
    pub validation: String,
}

/// A problem worth fixing, with everything needed to fix it and prove it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Finding {
    pub title: String,
    pub explanation: String,
    pub severity: Severity,
    /// 1–5 each.
    pub impact: u8,
    pub reach: u8,
    pub confidence: u8,
    pub effort: u8,
    /// impact × reach × confidence ÷ effort.
    pub priority: f32,
    pub user_goal: String,
    pub consequence: String,
    /// `header`, `nav`, `main`, `form`, `footer` or `page`.
    pub region: String,
    /// The devices it affects: `all` or a device id.
    pub devices: String,
    pub recommendation: Recommendation,
}

impl Finding {
    pub fn score(impact: u8, reach: u8, confidence: u8, effort: u8) -> f32 {
        f32::from(impact) * f32::from(reach) * f32::from(confidence) / f32::from(effort.max(1))
    }
}

/// One check from the plan, after it ran (or did not).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CheckResult {
    pub id: String,
    pub title: String,
    pub question: String,
    pub category: Category,
    pub tier: Depth,
    pub method: Method,
    /// True for the shared baseline, false for a page-type check.
    pub baseline: bool,
    pub status: Status,
    /// One line: what was found.
    pub summary: String,
    pub evidence: Evidence,
    pub finding: Option<Finding>,
}

/// One audit: the plan, what every check found, and the honest list of what
/// it could not do.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Audit {
    /// Zero until saved.
    pub id: i64,
    /// `baseline` or `retest`.
    pub kind: String,
    pub parent_id: Option<i64>,
    pub setup: Setup,
    pub tested_at: i64,
    pub source: String,
    pub http_status: Option<u16>,
    pub classification: Classification,
    pub page_type: PageType,
    pub goal: Goal,
    pub goal_inferred: bool,
    pub understanding: Understanding,
    pub checks: Vec<CheckResult>,
    pub limitations: Vec<String>,
}

/// The numbers across the top. Deliberately several, never one score.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Summary {
    pub selected: u32,
    pub pass: u32,
    pub fail: u32,
    pub needs_review: u32,
    pub not_tested: u32,
    pub blocked: u32,
    pub critical: u32,
    pub high: u32,
    pub medium: u32,
    pub low: u32,
    pub observation: u32,
    /// Checks that ran ÷ checks selected, 0..1.
    pub coverage: f32,
    /// Share of the checks that ran whose evidence is verified, 0..1.
    pub confidence: f32,
}

impl Audit {
    pub fn findings(&self) -> impl Iterator<Item = (&CheckResult, &Finding)> {
        self.checks.iter().filter_map(|check| check.finding.as_ref().map(|f| (check, f)))
    }

    pub fn summary(&self) -> Summary {
        let mut summary = Summary { selected: self.checks.len() as u32, ..Summary::default() };
        let mut verified = 0u32;
        for check in &self.checks {
            match check.status {
                Status::Pass => summary.pass += 1,
                Status::Fail => summary.fail += 1,
                Status::NeedsReview => summary.needs_review += 1,
                Status::NotTested => summary.not_tested += 1,
                Status::Blocked => summary.blocked += 1,
            }
            if check.status.ran() && check.evidence.status == EvidenceStatus::Verified {
                verified += 1;
            }
            if let Some(finding) = &check.finding {
                match finding.severity {
                    Severity::Critical => summary.critical += 1,
                    Severity::High => summary.high += 1,
                    Severity::Medium => summary.medium += 1,
                    Severity::Low => summary.low += 1,
                    Severity::Observation => summary.observation += 1,
                }
            }
        }
        let ran = summary.pass + summary.fail + summary.needs_review;
        if summary.selected > 0 {
            summary.coverage = ran as f32 / summary.selected as f32;
        }
        if ran > 0 {
            summary.confidence = verified as f32 / ran as f32;
        }
        summary
    }
}

/// Run an audit over markup already in hand. Pure: no network, no database.
pub fn run(setup: &Setup, source: &Source, crawl: Option<&context::CrawlFacts>) -> Audit {
    let facts = page::extract(&source.html, &setup.url);
    let classification = classify::classify(&facts, setup.page_type);
    let page_type = classification.used;
    let (goal, goal_inferred) = match setup.goal {
        Some(goal) => (goal, false),
        None => (page_type.default_goal(), true),
    };
    let understanding = page::understand(&facts, page_type, goal);
    let checks = checks::run_plan(setup, source, &facts, page_type, goal, crawl);

    let mut limitations = vec![
        "No browser was used: there are no screenshots, no rendered layout, no colour contrast measurements and no JavaScript execution. Checks that need those are marked not tested.".to_string(),
        "Forms were inspected but never submitted, so error and success messages are unverified.".to_string(),
        "Automated accessibility checks catch a fraction of WCAG failures. This is not a full accessibility audit.".to_string(),
        "Severity, priority and page type are estimates made by rules, and are labelled as such.".to_string(),
    ];
    if source.origin == "fetched" {
        limitations.push(
            "The markup was fetched once, as a crawler; a visitor who is signed in, in another country, or on a phone may be served something different.".to_string(),
        );
    }
    if crawl.is_none() {
        limitations.push(
            "This URL is not in the open crawl, so link, image and journey checks had no data.".to_string(),
        );
    }
    if facts.word_count < 80 {
        limitations.push(
            "The page source holds very little text; it is probably built by JavaScript, so most of what a visitor sees was not available to check.".to_string(),
        );
    }

    Audit {
        id: 0,
        kind: "baseline".to_string(),
        parent_id: None,
        setup: setup.clone(),
        tested_at: source.fetched_at,
        source: source.origin.clone(),
        http_status: source.status,
        classification,
        page_type,
        goal,
        goal_inferred,
        understanding,
        checks,
        limitations,
    }
}

/// Run an audit against the open crawl: use the stored page when the crawl
/// kept its HTML, otherwise fetch it once if the tester allowed it.
pub fn audit(conn: &rusqlite::Connection, setup: &Setup) -> std::result::Result<Audit, String> {
    let mut setup = setup.clone();
    setup.url = context::absolute(&setup.url);
    let stored = context::stored_page(conn, &setup.url).map_err(|e| e.to_string())?;
    let source = match stored.as_ref().and_then(context::StoredPage::source) {
        Some(source) => source,
        None if setup.allow_fetch => context::fetch_live(&setup.url)?,
        None => {
            return Err(if stored.is_some() {
                "The crawl has this page but did not keep its HTML. Allow a live fetch, or re-crawl with HTML storage on.".to_string()
            } else {
                "This URL is not in the open crawl. Allow a live fetch to test it anyway.".to_string()
            })
        }
    };
    let crawl = match &stored {
        Some(page) => {
            let goal = setup.goal.unwrap_or_else(|| {
                let facts = page::extract(&source.html, &setup.url);
                classify::classify(&facts, setup.page_type).used.default_goal()
            });
            Some(
                context::crawl_facts(conn, page, goal, setup.persona(), setup.depth == Depth::Deep)
                    .map_err(|e| e.to_string())?,
            )
        }
        None => None,
    };
    Ok(run(&setup, &source, crawl.as_ref()))
}

/// Unix seconds now.
pub fn now() -> i64 {
    chrono::Utc::now().timestamp()
}

/// Mask what should not end up in a report: email addresses and anything
/// that looks like a token in a query string.
pub fn redact(text: &str) -> String {
    use std::sync::OnceLock;
    static EMAIL: OnceLock<regex::Regex> = OnceLock::new();
    static TOKEN: OnceLock<regex::Regex> = OnceLock::new();
    let email = EMAIL.get_or_init(|| {
        regex::Regex::new(r"([A-Za-z0-9._%+-])[A-Za-z0-9._%+-]*@([A-Za-z0-9.-]+\.[A-Za-z]{2,})").unwrap()
    });
    let token = TOKEN.get_or_init(|| {
        regex::Regex::new(r"(?i)([?&](?:token|key|session|sid|auth|code|password|pwd|email)=)[^&#\s]+").unwrap()
    });
    let masked = email.replace_all(text, "$1•••@$2");
    token.replace_all(&masked, "${1}•••").into_owned()
}

#[cfg(test)]
mod tests;
