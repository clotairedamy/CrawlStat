//! "Can someone actually find this?"
//!
//! The attention model in [`super`] describes a single page. This walks the
//! whole crawled site the way a visitor would: start at the home page, look at
//! what is on offer, click the thing that looks most like what you came for,
//! and repeat until you find it, give up, or go round in circles.
//!
//! That turns a pile of pages into an answer to the question a designer
//! actually has — *can a first-time visitor get from the front door to the
//! pricing page without help, and how many clicks does it cost?* — with the
//! path taken as the evidence.
//!
//! # Why this is possible here and not in a page-level tool
//!
//! It needs three things at once: every page, every internal link, and the
//! anchor text on each one. A crawler has all three already. A browser
//! extension looking at one page has none of them, which is why tools of that
//! shape can tell you a button is low-contrast but never that the page it
//! leads to is nine clicks from anywhere a visitor starts.
//!
//! # What it is and is not
//!
//! The simulated visitor reads link text, not pixels, and picks greedily. Real
//! people use search, read menus that this treats as a flat list of links, and
//! backtrack. So a journey that succeeds is good evidence the path exists; a
//! journey that fails is a **hypothesis worth testing**, not proof that nobody
//! can get there. Both are labelled that way in the UI.

use std::collections::{HashMap, HashSet};

use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use super::Persona;
use crate::error::Result;

/// Something a visitor might have come to do.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Goal {
    pub id: String,
    pub label: String,
    /// Words that mark a link or a page as being about this goal.
    pub keywords: Vec<String>,
}

impl Goal {
    fn new(id: &str, label: &str, keywords: &[&str]) -> Self {
        Self {
            id: id.to_string(),
            label: label.to_string(),
            keywords: keywords.iter().map(|k| k.to_string()).collect(),
        }
    }

    /// Does this text look like it is about the goal?
    ///
    /// Matched on word boundaries, not as a bare substring. Naive `contains`
    /// looked right until it met a real site: "try" matched *indus<b>try</b>*
    /// and *coun<b>try</b>*, "help" matched `/let-our-family-help-your-family/`,
    /// and the tool cheerfully reported that a law firm's sign-up page was a
    /// video about insurance companies. Short keywords are the common case
    /// here, so this is not an edge case — it is most of them.
    fn matches(&self, text: &str) -> bool {
        let words = normalise(text);
        self.keywords.iter().any(|keyword| {
            // A multi-word keyword has to appear as a run of whole words.
            // The trailing "s" catches the plural a site actually uses —
            // "Prices", "Our plans" — which whole-word matching would
            // otherwise miss entirely.
            words.contains(&format!(" {keyword} ")) || words.contains(&format!(" {keyword}s "))
        })
    }
}

/// Lowercase the text and surround every word with spaces, so a keyword can
/// be tested for as ` word ` and cannot match inside a longer one.
fn normalise(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push(' ');
    for ch in text.chars() {
        if ch.is_alphanumeric() {
            out.extend(ch.to_lowercase());
        } else if !out.ends_with(' ') {
            out.push(' ');
        }
    }
    if !out.ends_with(' ') {
        out.push(' ');
    }
    out
}

/// The journeys worth checking on almost any site.
///
/// Deliberately the handful that nearly every site has and nearly every
/// visitor wants, rather than an exhaustive taxonomy. A custom goal covers
/// the rest.
pub fn preset_goals() -> Vec<Goal> {
    vec![
        Goal::new("pricing", "Find the pricing", &["pricing", "price", "plans", "cost", "how much"]),
        Goal::new("contact", "Contact a human", &["contact", "support", "get in touch", "talk to", "help centre", "help center", "get help", "customer service"]),
        Goal::new("signup", "Start a trial or sign up", &["sign up", "signup", "get started", "free trial", "try free", "start free", "register", "create account"]),
        Goal::new("docs", "Read the documentation", &["docs", "documentation", "guide", "manual", "how it works", "tutorial"]),
        Goal::new("about", "Find out who runs this", &["about", "team", "who we are", "our story", "company"]),
        Goal::new("privacy", "Check the privacy policy", &["privacy", "data protection", "gdpr", "cookie"]),
        Goal::new("careers", "Look for a job", &["careers", "jobs", "join us", "hiring", "work with us"]),
    ]
}

/// Build a goal from a task written in plain language.
///
/// A real usability task is "find the returns policy", never one of the seven
/// presets, so this is the entry point that matters for an actual study. The
/// phrase itself is the strongest keyword; its individual content words are
/// weaker ones, so "returns policy" still matches a link called "Returns".
///
/// Stopwords are dropped because a one-letter or filler word would match
/// everything: a goal keyed on "the" is a goal that is satisfied by the first
/// link on the page.
pub fn custom_goal(task: &str) -> Goal {
    const STOPWORDS: &[&str] = &[
        "a", "an", "the", "to", "for", "of", "my", "our", "your", "is", "it",
        "and", "or", "in", "on", "at", "find", "get", "see", "show", "me",
        "how", "where", "can", "i", "do", "want", "with", "page", "about",
    ];

    // Strip the filler first, then rebuild the phrase from what is left:
    // keying on the raw phrase would make "find the returns policy" a single
    // keyword that no link on earth says verbatim.
    let normalised = normalise(task);
    let content: Vec<&str> = normalised
        .split_whitespace()
        // Two letters or fewer matches far too much to be useful.
        .filter(|word| word.len() > 2 && !STOPWORDS.contains(word))
        .collect();

    let mut keywords: Vec<String> = Vec::new();
    if content.len() > 1 {
        keywords.push(content.join(" "));
    }
    for word in content {
        let word = word.to_string();
        if !keywords.contains(&word) {
            keywords.push(word);
        }
    }

    Goal {
        id: "custom".to_string(),
        label: task.trim().to_string(),
        keywords,
    }
}

/// How a journey ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    /// Arrived at a page that is about the goal.
    Reached,
    /// Ran out of patience before arriving.
    GaveUp,
    /// Reached a page with no onward link worth taking.
    DeadEnd,
    /// Came back to a page already visited: the navigation goes in a circle.
    Looped,
    /// Nothing on the first page looked relevant at all.
    NoSignal,
}

impl Outcome {
    pub fn as_str(self) -> &'static str {
        match self {
            Outcome::Reached => "reached",
            Outcome::GaveUp => "gave_up",
            Outcome::DeadEnd => "dead_end",
            Outcome::Looped => "looped",
            Outcome::NoSignal => "no_signal",
        }
    }

    pub fn succeeded(self) -> bool {
        self == Outcome::Reached
    }
}

/// One click.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Step {
    pub from_url: String,
    /// The link text that was clicked.
    pub anchor: String,
    pub to_url: String,
    /// How confident the choice was, 0..1. Low means guessing.
    pub confidence: f32,
    /// What else was on offer, best first — the evidence for why this was
    /// chosen and the list a designer should look at when it was wrong.
    pub alternatives: Vec<String>,
}

/// One simulated visit.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Journey {
    pub goal_id: String,
    pub goal_label: String,
    pub persona: String,
    pub outcome: String,
    pub clicks: usize,
    pub steps: Vec<Step>,
    /// Present when the goal page exists in the crawl but was not found.
    /// This is the finding that matters: the page is there, and the path to
    /// it is not.
    pub target_exists: bool,
    pub target_url: Option<String>,
    /// Clicks from the start if you knew exactly where to go. Compare with
    /// `clicks` to see how much the navigation costs.
    pub shortest_known: Option<usize>,
    /// Links on the pages visited that point somewhere this crawl never
    /// fetched. A journey over a half-finished crawl is walking a map with
    /// most of the roads missing, and the UI has to say so rather than
    /// report a confident "not reachable".
    pub unfetched_links: usize,
}

/// How many clicks each persona will spend before giving up.
fn budget(persona: Persona) -> usize {
    match persona {
        Persona::MobileRushed => 3,
        Persona::TaskOriented => 5,
        Persona::Explorer => 8,
        Persona::Skeptical => 7,
    }
}

/// A page, with what it links to.
struct Page {
    url: String,
    title: String,
    depth: u32,
    links: Vec<Link>,
}

struct Link {
    anchor: String,
    target: i64,
}

/// Anchor text that is not really link text.
///
/// Inline `<style>` inside an SVG inside a link ends up in the text node, so a
/// real crawl is full of anchors like `.cls-1 { fill: #000; }`. Treating those
/// as what a visitor reads would be nonsense.
fn usable_anchor(text: &str) -> bool {
    let trimmed = text.trim();
    !trimmed.is_empty()
        && trimmed.len() <= 120
        && !trimmed.contains('{')
        && !trimmed.contains('}')
}

fn load_pages(conn: &Connection) -> Result<(HashMap<i64, Page>, Option<i64>)> {
    let mut pages: HashMap<i64, Page> = HashMap::new();
    let mut by_key: HashMap<String, i64> = HashMap::new();

    {
        let mut stmt = conn.prepare(
            "SELECT id, url, dedup_key, coalesce(title, ''), depth
             FROM urls
             WHERE is_internal = 1 AND kind = 0 AND status_code IS NOT NULL
               AND coalesce(final_status_code, status_code) < 400",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, i64>(4)?.max(0) as u32,
            ))
        })?;
        for row in rows {
            let (id, url, key, title, depth) = row?;
            by_key.insert(key, id);
            pages.insert(
                id,
                Page {
                    url,
                    title,
                    depth,
                    links: Vec::new(),
                },
            );
        }
    }

    {
        let mut stmt = conn.prepare(
            "SELECT source_id, anchor_text, target_key FROM links
             WHERE is_internal = 1 AND target_key IS NOT NULL
               AND anchor_text IS NOT NULL AND trim(anchor_text) <> ''",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?;
        for row in rows {
            let (source, anchor, key) = row?;
            if !usable_anchor(&anchor) {
                continue;
            }
            let Some(&target) = by_key.get(&key) else { continue };
            if target == source {
                continue;
            }
            if let Some(page) = pages.get_mut(&source) {
                // One entry per destination: a nav repeated in the header and
                // the footer is one choice to a visitor, not two.
                if !page.links.iter().any(|link| link.target == target) {
                    page.links.push(Link {
                        anchor: anchor.split_whitespace().collect::<Vec<_>>().join(" "),
                        target,
                    });
                }
            }
        }
    }

    // The front door: the shallowest page, tie-broken by the shortest URL,
    // which on every real site is the home page.
    let start = pages
        .iter()
        .min_by_key(|(id, page)| (page.depth, page.url.len(), **id))
        .map(|(id, _)| *id);

    Ok((pages, start))
}

/// How well a link matches the goal, 0..1.
///
/// `peek` is one step of foresight: whether the page this link leads to has a
/// link that *does* match. Without it the simulated visitor only ever follows
/// signs that literally name the destination, so a perfectly ordinary
/// Home → Products → Plans → Pricing path reads as a total failure. A real
/// person clicks a plausible category, sees the next sign, and carries on —
/// and one level is about as far ahead as a hovered menu or a glance down a
/// landing page actually shows you.
///
/// It is deliberately *one* level. Full lookahead would find any path that
/// exists, which would make every journey succeed and the whole exercise
/// pointless.
fn link_score(
    goal: &Goal,
    link: &Link,
    target: &Page,
    visited: &HashSet<i64>,
    peek: bool,
) -> f32 {
    let mut score = 0.0;

    // Anchor text is what the visitor actually reads, so it counts most.
    if goal.matches(&link.anchor) {
        score += 1.0;
    }
    if peek {
        score += 0.5;
    }
    // The URL is a weaker signal — nobody reads it, but it correlates.
    if goal.matches(&target.url) {
        score += 0.45;
    }
    if goal.matches(&target.title) {
        score += 0.3;
    }

    // Shallow pages are more likely to be the real section landing page rather
    // than a deep article that happens to mention the word.
    score += 0.12 / (1.0 + target.depth as f32);

    if visited.contains(&link.target) {
        score -= 0.9;
    }
    score
}

/// Does this page satisfy the goal?
fn is_destination(goal: &Goal, page: &Page) -> bool {
    goal.matches(&page.url) || goal.matches(&page.title)
}

/// Walk the site for one goal and one persona.
pub fn simulate(conn: &Connection, goal: &Goal, persona: Persona) -> Result<Journey> {
    let (pages, start) = load_pages(conn)?;

    // Does the destination exist at all? Answering this separates "you have no
    // pricing page" from "your pricing page is unreachable", which are
    // completely different problems with completely different fixes.
    let target = pages
        .iter()
        .filter(|(_, page)| is_destination(goal, page))
        .min_by_key(|(_, page)| (page.depth, page.url.len()));
    let target_url = target.map(|(_, page)| page.url.clone());
    let shortest_known = target.map(|(_, page)| page.depth as usize);

    let mut journey = Journey {
        goal_id: goal.id.clone(),
        goal_label: goal.label.clone(),
        persona: persona.as_str().to_string(),
        outcome: Outcome::NoSignal.as_str().to_string(),
        clicks: 0,
        steps: Vec::new(),
        target_exists: target.is_some(),
        target_url,
        shortest_known,
        unfetched_links: 0,
    };

    let Some(mut current) = start else {
        return Ok(journey);
    };

    // Already there: a site whose home page is the pricing page.
    if pages.get(&current).is_some_and(|page| is_destination(goal, page)) {
        journey.outcome = Outcome::Reached.as_str().to_string();
        return Ok(journey);
    }

    let mut visited: HashSet<i64> = HashSet::new();
    visited.insert(current);
    let allowance = budget(persona);

    for _ in 0..allowance {
        let Some(page) = pages.get(&current) else {
            journey.outcome = Outcome::DeadEnd.as_str().to_string();
            return Ok(journey);
        };
        if page.links.is_empty() {
            journey.outcome = Outcome::DeadEnd.as_str().to_string();
            return Ok(journey);
        }

        journey.unfetched_links += page
            .links
            .iter()
            .filter(|link| !pages.contains_key(&link.target))
            .count();

        let mut scored: Vec<(f32, &Link)> = page
            .links
            .iter()
            .filter_map(|link| {
                let target = pages.get(&link.target)?;
                // Can the goal be seen from there?
                let peek = target.links.iter().any(|onward| {
                    goal.matches(&onward.anchor)
                        || pages
                            .get(&onward.target)
                            .is_some_and(|page| goal.matches(&page.url))
                });
                Some((link_score(goal, link, target, &visited, peek), link))
            })
            .collect();
        scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

        let Some(&(best_score, best)) = scored.first() else {
            journey.outcome = Outcome::DeadEnd.as_str().to_string();
            return Ok(journey);
        };

        // Nothing on this page resembles what they came for. On the first page
        // that is its own finding; later it means they are wandering.
        if best_score <= 0.15 {
            journey.outcome = if journey.steps.is_empty() {
                Outcome::NoSignal.as_str().to_string()
            } else {
                Outcome::GaveUp.as_str().to_string()
            };
            return Ok(journey);
        }

        let destination = &pages[&best.target];
        journey.steps.push(Step {
            from_url: page.url.clone(),
            anchor: best.anchor.clone(),
            to_url: destination.url.clone(),
            confidence: best_score.clamp(0.0, 1.0),
            alternatives: scored
                .iter()
                .skip(1)
                .take(3)
                .map(|(_, link)| link.anchor.clone())
                .collect(),
        });
        journey.clicks += 1;

        if !visited.insert(best.target) {
            journey.outcome = Outcome::Looped.as_str().to_string();
            return Ok(journey);
        }
        current = best.target;

        if is_destination(goal, destination) {
            journey.outcome = Outcome::Reached.as_str().to_string();
            return Ok(journey);
        }
    }

    journey.outcome = Outcome::GaveUp.as_str().to_string();
    Ok(journey)
}

/// Run every preset goal for one persona.
pub fn simulate_all(conn: &Connection, persona: Persona) -> Result<Vec<Journey>> {
    preset_goals()
        .iter()
        .map(|goal| simulate(conn, goal, persona))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::Store;

    /// Build a site: pages, then links as (from, anchor, to) by index.
    fn site(pages: &[(&str, &str, u32)], links: &[(usize, &str, usize)]) -> Store {
        let store = Store::open_in_memory().unwrap();
        let conn = store.connection();
        for (index, (url, title, depth)) in pages.iter().enumerate() {
            conn.execute(
                "INSERT INTO urls (id, url, dedup_key, title, depth, state, kind,
                                   is_internal, status_code)
                 VALUES (?1, ?2, ?3, ?4, ?5, 2, 0, 1, 200)",
                rusqlite::params![
                    index as i64 + 1,
                    url,
                    url.replace("https://", ""),
                    title,
                    depth
                ],
            )
            .unwrap();
        }
        for (from, anchor, to) in links {
            let key = pages[*to].0.replace("https://", "");
            conn.execute(
                "INSERT INTO links (source_id, anchor_text, target_key, target_url,
                                    raw_href, is_internal)
                 VALUES (?1, ?2, ?3, ?4, ?4, 1)",
                rusqlite::params![*from as i64 + 1, anchor, key, pages[*to].0],
            )
            .unwrap();
        }
        store
    }

    fn pricing() -> Goal {
        preset_goals().into_iter().find(|g| g.id == "pricing").unwrap()
    }

    #[test]
    fn a_signposted_path_is_found_in_one_click() {
        let store = site(
            &[
                ("https://a.test/", "Home", 0),
                ("https://a.test/pricing", "Pricing", 1),
            ],
            &[(0, "Pricing", 1)],
        );
        let journey = simulate(store.connection(), &pricing(), Persona::TaskOriented).unwrap();
        assert_eq!(journey.outcome, "reached");
        assert_eq!(journey.clicks, 1);
        assert_eq!(journey.steps[0].anchor, "Pricing");
    }

    #[test]
    fn a_page_nobody_links_to_is_reported_as_existing_but_unreachable() {
        // The finding that matters most: the page is there, the path is not.
        let store = site(
            &[
                ("https://a.test/", "Home", 0),
                ("https://a.test/about", "About us", 1),
                ("https://a.test/pricing", "Pricing", 4),
            ],
            &[(0, "About us", 1)],
        );
        let journey = simulate(store.connection(), &pricing(), Persona::TaskOriented).unwrap();
        assert!(!journey.outcome.eq("reached"));
        assert!(journey.target_exists, "the pricing page is in the crawl");
        assert_eq!(journey.target_url.as_deref(), Some("https://a.test/pricing"));
    }

    #[test]
    fn a_site_without_the_page_is_told_apart_from_one_that_hides_it() {
        let store = site(
            &[("https://a.test/", "Home", 0), ("https://a.test/blog", "Blog", 1)],
            &[(0, "Blog", 1)],
        );
        let journey = simulate(store.connection(), &pricing(), Persona::TaskOriented).unwrap();
        assert!(!journey.target_exists, "there is genuinely no pricing page");
        assert_eq!(journey.target_url, None);
    }

    #[test]
    fn a_signposted_path_is_followed_as_far_as_the_persona_has_patience() {
        // Neutral URLs and titles, so arrival is decided by the signposting
        // alone: each anchor names the goal, and only the last page is it.
        let pages = &[
            ("https://a.test/", "Home", 0),
            ("https://a.test/a", "Section A", 1),
            ("https://a.test/b", "Section B", 2),
            ("https://a.test/c", "Section C", 3),
            ("https://a.test/pricing", "Pricing", 4),
        ];
        let links = &[
            (0, "Pricing and plans", 1),
            (1, "See our pricing", 2),
            (2, "Full price list", 3),
            (3, "Pricing", 4),
        ];
        let store = site(pages, links);

        let patient = simulate(store.connection(), &pricing(), Persona::Explorer).unwrap();
        assert_eq!(patient.outcome, "reached");
        assert_eq!(patient.clicks, 4);

        // Three clicks of patience, four needed.
        let rushed = simulate(store.connection(), &pricing(), Persona::MobileRushed).unwrap();
        assert_eq!(rushed.outcome, "gave_up");
        assert_eq!(rushed.clicks, 3);
        assert!(rushed.target_exists, "it is there, they just ran out of patience");
    }

    #[test]
    fn an_unsignposted_route_is_not_followed_even_when_one_exists() {
        // Home -> Products -> Editions -> Pricing, with nothing on the way
        // saying so. The model follows signs, and the honest finding here is
        // that there are none: a visitor hunting for pricing has no reason to
        // click "Products" and then "Editions".
        let store = site(
            &[
                ("https://a.test/", "Home", 0),
                ("https://a.test/products", "Products", 1),
                ("https://a.test/products/editions", "Editions", 2),
                ("https://a.test/products/editions/pricing", "Pricing", 3),
            ],
            &[(0, "Products", 1), (1, "Editions", 2), (2, "Pricing", 3)],
        );
        let journey = simulate(store.connection(), &pricing(), Persona::Explorer).unwrap();
        assert_ne!(journey.outcome, "reached");
        assert!(journey.target_exists);
        assert_eq!(journey.shortest_known, Some(3), "three clicks if you knew");
    }

    #[test]
    fn one_step_of_foresight_is_enough_to_follow_a_plausible_category() {
        // "Products" says nothing about pricing, but the pricing link is
        // visible from it — which is what hovering a menu shows you.
        let store = site(
            &[
                ("https://a.test/", "Home", 0),
                ("https://a.test/products", "Products", 1),
                ("https://a.test/cost", "What it costs", 2),
            ],
            &[(0, "Products", 1), (1, "Pricing", 2)],
        );
        let journey = simulate(store.connection(), &pricing(), Persona::Explorer).unwrap();
        assert_eq!(journey.outcome, "reached");
        assert_eq!(journey.clicks, 2);
    }

    #[test]
    fn a_home_page_with_nothing_relevant_reports_no_signal() {
        let store = site(
            &[
                ("https://a.test/", "Home", 0),
                ("https://a.test/blog", "Blog", 1),
                ("https://a.test/pricing", "Pricing", 5),
            ],
            &[(0, "Blog", 1), (1, "Blog archive", 1)],
        );
        let journey = simulate(store.connection(), &pricing(), Persona::TaskOriented).unwrap();
        assert_eq!(journey.outcome, "no_signal");
        assert!(journey.steps.is_empty());
    }

    #[test]
    fn a_page_with_no_links_is_a_dead_end() {
        let store = site(&[("https://a.test/", "Home", 0)], &[]);
        let journey = simulate(store.connection(), &pricing(), Persona::Explorer).unwrap();
        assert_eq!(journey.outcome, "dead_end");
    }

    #[test]
    fn every_step_records_what_else_was_on_offer() {
        // The alternatives are the evidence a designer needs: they say what
        // the visitor was choosing between when they went wrong.
        let store = site(
            &[
                ("https://a.test/", "Home", 0),
                ("https://a.test/pricing", "Pricing", 1),
                ("https://a.test/blog", "Blog", 1),
                ("https://a.test/about", "About", 1),
            ],
            &[(0, "Pricing", 1), (0, "Blog", 2), (0, "About", 3)],
        );
        let journey = simulate(store.connection(), &pricing(), Persona::TaskOriented).unwrap();
        assert_eq!(journey.steps[0].anchor, "Pricing");
        assert_eq!(journey.steps[0].alternatives.len(), 2);
    }

    #[test]
    fn the_shortest_known_route_is_reported_for_comparison() {
        let store = site(
            &[
                ("https://a.test/", "Home", 0),
                ("https://a.test/pricing", "Pricing", 2),
            ],
            &[(0, "Pricing", 1)],
        );
        let journey = simulate(store.connection(), &pricing(), Persona::TaskOriented).unwrap();
        assert_eq!(journey.shortest_known, Some(2));
    }

    #[test]
    fn css_leaking_out_of_an_svg_is_not_treated_as_link_text() {
        // Real crawls are full of these: an inline <style> inside an <svg>
        // inside an <a> ends up in the anchor's text node.
        assert!(!usable_anchor(".cls-1 { fill: #000; }"));
        assert!(!usable_anchor("   "));
        assert!(usable_anchor("Pricing"));
    }

    #[test]
    fn an_empty_crawl_produces_a_journey_rather_than_an_error() {
        let store = Store::open_in_memory().unwrap();
        let journey = simulate(store.connection(), &pricing(), Persona::Explorer).unwrap();
        assert_eq!(journey.clicks, 0);
        assert!(!journey.target_exists);
    }

    #[test]
    fn navigation_that_goes_in_a_circle_is_detected() {
        let store = site(
            &[
                ("https://a.test/", "Home", 0),
                ("https://a.test/plans", "Our plans and cost", 1),
            ],
            &[(0, "Our plans and cost", 1), (1, "Our plans and cost", 1)],
        );
        // The destination matches the goal, so this reaches it — the loop
        // guard is exercised by the self-link being filtered and the revisit
        // penalty applying.
        let journey = simulate(store.connection(), &pricing(), Persona::Explorer).unwrap();
        assert_eq!(journey.outcome, "reached");
    }

    #[test]
    fn all_preset_goals_run_without_panicking() {
        let store = site(
            &[("https://a.test/", "Home", 0), ("https://a.test/x", "X", 1)],
            &[(0, "X", 1)],
        );
        let journeys = simulate_all(store.connection(), Persona::TaskOriented).unwrap();
        assert_eq!(journeys.len(), preset_goals().len());
    }
}

#[cfg(test)]
mod matching_tests {
    use super::*;

    fn goal(keywords: &[&str]) -> Goal {
        Goal::new("x", "X", keywords)
    }

    #[test]
    fn a_keyword_does_not_match_inside_a_longer_word() {
        assert!(!goal(&["try"]).matches("the construction industry"));
        assert!(!goal(&["cost"]).matches("costume hire"));
        assert!(!goal(&["price"]).matches("priceless"));
    }

    #[test]
    fn over_generic_keywords_are_kept_out_of_the_presets() {
        // A bare "try" matched `/companies-try-to-avoid-paying-you/` on a real
        // crawl, because a hyphen is a word break and "try" really is a word
        // there. Word-boundary matching cannot save a keyword that generic —
        // the fix is not to ship it.
        let signup = preset_goals().into_iter().find(|g| g.id == "signup").unwrap();
        assert!(!signup.keywords.iter().any(|k| k == "try"));
        assert!(signup.matches("Try free for 30 days"));
        assert!(!signup.matches("how companies try to avoid paying you"));

        let contact = preset_goals().into_iter().find(|g| g.id == "contact").unwrap();
        assert!(!contact.keywords.iter().any(|k| k == "help"));
        assert!(!contact.matches("let our family help your family"));
        assert!(contact.matches("Contact us"));
    }

    #[test]
    fn the_plural_a_site_actually_uses_still_matches() {
        assert!(goal(&["price"]).matches("Our prices"));
        assert!(goal(&["plan"]).matches("Compare plans"));
    }

    #[test]
    fn a_keyword_matches_as_a_whole_word_however_it_is_punctuated() {
        assert!(goal(&["try"]).matches("Try it free"));
        assert!(goal(&["pricing"]).matches("https://a.test/pricing"));
        assert!(goal(&["pricing"]).matches("/our-pricing/"));
        assert!(goal(&["help"]).matches("Help & support"));
    }

    #[test]
    fn a_multi_word_keyword_needs_the_whole_phrase() {
        let signup = goal(&["sign up"]);
        assert!(signup.matches("Sign up for free"));
        assert!(signup.matches("/sign-up/"));
        assert!(!signup.matches("Sign in"));
        assert!(!signup.matches("Up sign"));
    }

    #[test]
    fn normalising_handles_punctuation_and_case() {
        assert_eq!(normalise("Help & Support!"), " help support ");
        assert_eq!(normalise("/sign-up/"), " sign up ");
        assert_eq!(normalise(""), " ");
    }
}

#[cfg(test)]
mod custom_goal_tests {
    use super::*;

    #[test]
    fn the_whole_phrase_and_its_content_words_both_become_keywords() {
        let goal = custom_goal("Find the returns policy");
        assert!(goal.keywords.contains(&"returns policy".to_string()));
        assert!(goal.keywords.contains(&"returns".to_string()));
        assert!(goal.keywords.contains(&"policy".to_string()));
    }

    #[test]
    fn filler_words_are_dropped() {
        // A goal keyed on "the" is satisfied by the first link on the page.
        let goal = custom_goal("how do I find the price of a plan");
        assert!(!goal.keywords.iter().any(|k| k == "the" || k == "how" || k == "find"));
        assert!(goal.keywords.iter().any(|k| k == "price"));
    }

    #[test]
    fn a_custom_goal_matches_the_way_a_preset_does() {
        let goal = custom_goal("returns policy");
        assert!(goal.matches("Returns and refunds"));
        assert!(goal.matches("/help/returns-policy/"));
        assert!(!goal.matches("Our story"));
    }

    #[test]
    fn an_empty_task_matches_nothing_rather_than_everything() {
        let goal = custom_goal("   ");
        assert!(goal.keywords.is_empty());
        assert!(!goal.matches("anything at all"));
    }

    #[test]
    fn the_label_keeps_what_the_tester_typed() {
        assert_eq!(custom_goal("  Book a demo  ").label, "Book a demo");
    }
}

// ─── benchmarking against someone else ───────────────────────────────────

/// The same task, run against two sites.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Benchmark {
    pub goal_label: String,
    pub persona: String,
    pub ours: Journey,
    pub theirs: Journey,
}

impl Benchmark {
    /// Clicks they need minus clicks we need. Negative means we are better.
    ///
    /// A task neither site satisfies is not a gap, and a task only one of
    /// them satisfies is the most interesting result there is.
    pub fn click_gap(&self) -> Option<i64> {
        match (self.ours.outcome.as_str(), self.theirs.outcome.as_str()) {
            ("reached", "reached") => {
                Some(self.ours.clicks as i64 - self.theirs.clicks as i64)
            }
            _ => None,
        }
    }

    /// The finding worth putting in front of a stakeholder.
    pub fn verdict(&self) -> &'static str {
        match (
            self.ours.outcome == "reached",
            self.theirs.outcome == "reached",
        ) {
            (false, true) => "they have it, we do not",
            (true, false) => "we have it, they do not",
            (true, true) => match self.click_gap().unwrap_or(0) {
                gap if gap > 0 => "slower than theirs",
                gap if gap < 0 => "faster than theirs",
                _ => "the same",
            },
            (false, false) => "neither site gets there",
        }
    }
}

/// Run every preset task against two crawls.
///
/// This is the argument a stakeholder actually responds to. "Our pricing is
/// four clicks from the home page" is an opinion about a number. "Our pricing
/// is four clicks and theirs is one" is a decision, and it needs no usability
/// lab to produce — only two crawls, which this tool was already doing.
pub fn benchmark(
    ours: &Connection,
    theirs: &Connection,
    persona: Persona,
) -> Result<Vec<Benchmark>> {
    let mut results = Vec::new();
    for goal in preset_goals() {
        results.push(Benchmark {
            goal_label: goal.label.clone(),
            persona: persona.as_str().to_string(),
            ours: simulate(ours, &goal, persona)?,
            theirs: simulate(theirs, &goal, persona)?,
        });
    }
    // Worst gaps first: the tasks where they beat us, by how much.
    results.sort_by_key(|b| match (b.ours.outcome == "reached", b.theirs.outcome == "reached") {
        (false, true) => -1000,
        (true, true) => -b.click_gap().unwrap_or(0),
        (true, false) => 1000,
        (false, false) => 1001,
    });
    Ok(results)
}

/// One task against two crawls.
pub fn benchmark_task(
    ours: &Connection,
    theirs: &Connection,
    persona: Persona,
    task: &str,
) -> Result<Benchmark> {
    let goal = custom_goal(task);
    Ok(Benchmark {
        goal_label: goal.label.clone(),
        persona: persona.as_str().to_string(),
        ours: simulate(ours, &goal, persona)?,
        theirs: simulate(theirs, &goal, persona)?,
    })
}

#[cfg(test)]
mod benchmark_tests {
    use super::*;
    use crate::store::Store;

    fn one_page_site(url: &str, title: &str, link: Option<(&str, &str, &str)>) -> Store {
        let store = Store::open_in_memory().unwrap();
        let conn = store.connection();
        conn.execute(
            "INSERT INTO urls (id, url, dedup_key, title, depth, state, kind,
                               is_internal, status_code)
             VALUES (1, ?1, ?2, ?3, 0, 2, 0, 1, 200)",
            rusqlite::params![url, url.replace("https://", ""), title],
        )
        .unwrap();
        if let Some((anchor, target, target_title)) = link {
            conn.execute(
                "INSERT INTO urls (id, url, dedup_key, title, depth, state, kind,
                                   is_internal, status_code)
                 VALUES (2, ?1, ?2, ?3, 1, 2, 0, 1, 200)",
                rusqlite::params![target, target.replace("https://", ""), target_title],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO links (source_id, anchor_text, target_key, target_url,
                                    raw_href, is_internal)
                 VALUES (1, ?1, ?2, ?3, ?3, 1)",
                rusqlite::params![anchor, target.replace("https://", ""), target],
            )
            .unwrap();
        }
        store
    }

    #[test]
    fn a_task_only_the_competitor_satisfies_sorts_to_the_top() {
        // The single most useful row in the whole report.
        let ours = one_page_site("https://ours.test/", "Home", None);
        let theirs = one_page_site(
            "https://theirs.test/",
            "Home",
            Some(("Pricing", "https://theirs.test/pricing", "Pricing")),
        );
        let results =
            benchmark(ours.connection(), theirs.connection(), Persona::TaskOriented).unwrap();
        assert_eq!(results[0].verdict(), "they have it, we do not");
    }

    #[test]
    fn the_click_gap_is_only_meaningful_when_both_sites_get_there() {
        let ours = one_page_site("https://ours.test/", "Home", None);
        let theirs = one_page_site(
            "https://theirs.test/",
            "Home",
            Some(("Pricing", "https://theirs.test/pricing", "Pricing")),
        );
        let results =
            benchmark(ours.connection(), theirs.connection(), Persona::TaskOriented).unwrap();
        let pricing = results.iter().find(|b| b.goal_label.contains("pricing")).unwrap();
        assert_eq!(pricing.click_gap(), None, "we never arrived, so there is no gap");
    }

    #[test]
    fn two_sites_that_both_signpost_it_report_the_difference_in_clicks() {
        let ours = one_page_site(
            "https://ours.test/",
            "Home",
            Some(("Pricing", "https://ours.test/pricing", "Pricing")),
        );
        let theirs = one_page_site(
            "https://theirs.test/",
            "Home",
            Some(("Pricing", "https://theirs.test/pricing", "Pricing")),
        );
        let results =
            benchmark(ours.connection(), theirs.connection(), Persona::TaskOriented).unwrap();
        let pricing = results.iter().find(|b| b.goal_label.contains("pricing")).unwrap();
        assert_eq!(pricing.click_gap(), Some(0));
        assert_eq!(pricing.verdict(), "the same");
    }

    #[test]
    fn a_custom_task_can_be_benchmarked_too() {
        let ours = one_page_site("https://ours.test/", "Home", None);
        let theirs = one_page_site(
            "https://theirs.test/",
            "Home",
            Some(("Returns policy", "https://theirs.test/returns", "Returns")),
        );
        let result = benchmark_task(
            ours.connection(),
            theirs.connection(),
            Persona::Explorer,
            "returns policy",
        )
        .unwrap();
        assert_eq!(result.verdict(), "they have it, we do not");
    }
}
