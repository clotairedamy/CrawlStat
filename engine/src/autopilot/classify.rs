//! Deciding what kind of page this is.
//!
//! A weighted vote over signals in the markup. Each page type collects points
//! for what it would have — a cart button, a phone number, a byline — and the
//! winner has to clear a floor, or the page is called "Other" rather than
//! forced into a box. The reasons are kept, because a classification nobody
//! can check is a classification nobody should trust, and the tester can
//! always overrule it.

use serde::{Deserialize, Serialize};

use super::page::PageFacts;
use super::PageType;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Classification {
    pub detected: PageType,
    /// 0..1: how far ahead of the runner-up the winner was.
    pub confidence: f32,
    pub reasons: Vec<String>,
    pub runner_up: Option<PageType>,
    /// The type the audit used — the tester's choice when they made one.
    pub used: PageType,
    pub overridden: bool,
}

struct Tally {
    page_type: PageType,
    score: f32,
    reasons: Vec<String>,
}

impl Tally {
    fn add(&mut self, points: f32, reason: impl Into<String>) {
        self.score += points;
        self.reasons.push(reason.into());
    }

    fn words(&mut self, facts: &PageFacts, points: f32, phrases: &[&str]) {
        let found = facts.says_all_of(phrases);
        if !found.is_empty() {
            // Diminishing: a page that says "pricing" twelve ways is not
            // twelve times more of a SaaS page.
            let score = points * (found.len() as f32).sqrt();
            self.add(score, format!("Says “{}”", found.iter().take(3).copied().collect::<Vec<_>>().join("”, “")));
        }
    }
}

fn tally(tallies: &mut [Tally], page_type: PageType) -> &mut Tally {
    tallies.iter_mut().find(|t| t.page_type == page_type).expect("every type has a tally")
}

pub fn classify(facts: &PageFacts, chosen: Option<PageType>) -> Classification {
    let mut tallies: Vec<Tally> = PageType::ALL
        .iter()
        .filter(|t| **t != PageType::Other)
        .map(|t| Tally { page_type: *t, score: 0.0, reasons: Vec::new() })
        .collect();

    let forms = facts.task_forms();
    let contact_form = forms.iter().any(|f| f.collects_contact());
    let only_email = forms.len() == 1 && forms[0].fields.len() <= 2 && contact_form;

    // ── Ecommerce product
    {
        let t = tally(&mut tallies, PageType::Ecommerce);
        if let Some(schema) = facts.has_schema(&["Product", "Offer"]) {
            t.add(3.0, format!("{schema} structured data"));
        }
        if let Some(cta) = facts.ctas.iter().find(|c| {
            let l = c.label.to_ascii_lowercase();
            l.contains("add to cart") || l.contains("add to basket") || l.contains("add to bag") || l.contains("buy now")
        }) {
            t.add(3.0, format!("“{}” button", cta.label));
        }
        t.words(facts, 0.8, &["in stock", "out of stock", "free shipping", "free delivery", "returns", "sku", "size guide", "quantity"]);
        if facts.price().is_some() {
            t.add(0.8, "Shows a price");
        }
    }

    // ── SaaS
    {
        let t = tally(&mut tallies, PageType::Saas);
        if let Some(schema) = facts.has_schema(&["SoftwareApplication", "WebApplication"]) {
            t.add(2.5, format!("{schema} structured data"));
        }
        t.words(facts, 1.0, &["free trial", "start free", "request a demo", "book a demo", "integrations", "api", "per user", "per seat", "dashboard", "platform", "workflow", "automate", "software", "no credit card"]);
        if facts.link_to(&["pricing", "plans"]).is_some() {
            t.add(1.0, "Links to pricing");
        }
        if facts.link_to(&["log in", "login", "sign in"]).is_some() {
            t.add(0.8, "Has a sign-in link");
        }
    }

    // ── Local business or service
    {
        let t = tally(&mut tallies, PageType::LocalBusiness);
        if let Some(schema) = facts.has_schema(&["LocalBusiness", "Restaurant", "Dentist", "Plumber", "Electrician", "HomeAndConstructionBusiness", "MedicalBusiness", "LegalService", "AutoRepair", "Store", "ProfessionalService"]) {
            t.add(3.0, format!("{schema} structured data"));
        }
        if !facts.tel.is_empty() {
            t.add(1.5, "Click-to-call phone link");
        }
        if facts.has_address {
            t.add(1.2, "Shows a street address");
        }
        if facts.opening_hours().is_some() {
            t.add(1.5, "Shows opening hours");
        }
        t.words(facts, 0.9, &["near you", "near me", "call us", "free quote", "free estimate", "book an appointment", "service area", "areas we serve", "locally owned", "family owned", "visit us", "directions"]);
    }

    // ── Agency or freelancer
    {
        let t = tally(&mut tallies, PageType::Agency);
        t.words(facts, 1.0, &["portfolio", "case study", "case studies", "our work", "selected work", "clients", "agency", "studio", "freelance", "freelancer", "hire me", "let s work together", "start a project", "branding", "web design"]);
        if facts.link_to(&["work", "portfolio", "case studies", "projects"]).is_some() {
            t.add(0.8, "Links to work or case studies");
        }
    }

    // ── Lead generation
    {
        let t = tally(&mut tallies, PageType::LeadGen);
        if contact_form && !only_email {
            t.add(2.0, "A form that asks for contact details");
        }
        t.words(facts, 1.0, &["get a quote", "free consultation", "download the guide", "free guide", "ebook", "white paper", "whitepaper", "webinar", "get your free", "request a callback", "speak to an expert"]);
        if facts.nav_links().len() <= 3 && contact_form {
            t.add(1.0, "Little navigation: the form is the only way on");
        }
    }

    // ── Waitlist or launch
    {
        let t = tally(&mut tallies, PageType::Waitlist);
        t.words(facts, 1.4, &["waitlist", "wait list", "coming soon", "launching", "notify me", "early access", "join the list", "be the first", "pre order", "beta"]);
        if only_email {
            t.add(1.5, "A single email sign-up form");
        }
    }

    // ── Content or campaign
    {
        let t = tally(&mut tallies, PageType::Content);
        if let Some(schema) = facts.has_schema(&["Article", "BlogPosting", "NewsArticle", "TechArticle", "HowTo", "Recipe"]) {
            t.add(3.0, format!("{schema} structured data"));
        }
        if facts.og_type.as_deref() == Some("article") {
            t.add(1.5, "og:type is article");
        }
        if facts.has_article {
            t.add(1.0, "Uses an <article> element");
        }
        if facts.has_published_date {
            t.add(1.0, "Has a publication date");
        }
        if facts.has_author {
            t.add(0.8, "Has a byline");
        }
        if facts.word_count > 900 {
            t.add(1.2, format!("{} words of text", facts.word_count));
        }
        t.words(facts, 0.8, &["min read", "minute read", "posted on", "published", "share this", "related posts", "related articles"]);
    }

    // ── Marketplace or directory
    {
        let t = tally(&mut tallies, PageType::Marketplace);
        if facts.has_search {
            t.add(1.2, "Has a search box");
        }
        if facts.filter_controls >= 3 {
            t.add(1.0, format!("{} filter or sort controls", facts.filter_controls));
        }
        t.words(facts, 1.0, &["listings", "browse", "sellers", "vendors", "providers", "directory", "results", "sort by", "filter", "categories", "find a", "near you"]);
        if let Some(schema) = facts.has_schema(&["ItemList", "SearchResultsPage"]) {
            t.add(1.5, format!("{schema} structured data"));
        }
    }

    tallies.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    let best = &tallies[0];
    let second = &tallies[1];

    // Below this, nothing on the page says much about what it is.
    const FLOOR: f32 = 2.0;
    let (detected, confidence, reasons, runner_up) = if best.score < FLOOR {
        (
            PageType::Other,
            0.3,
            vec!["Nothing on the page points clearly at one kind of page".to_string()],
            (best.score > 0.0).then_some(best.page_type),
        )
    } else {
        let margin = (best.score - second.score) / best.score;
        let strength = (best.score / 8.0).min(1.0);
        (
            best.page_type,
            (0.35 + 0.35 * margin + 0.3 * strength).clamp(0.0, 1.0),
            best.reasons.clone(),
            (second.score >= FLOOR * 0.75).then_some(second.page_type),
        )
    };

    Classification {
        detected,
        confidence,
        reasons,
        runner_up,
        used: chosen.unwrap_or(detected),
        overridden: chosen.is_some_and(|c| c != detected),
    }
}
