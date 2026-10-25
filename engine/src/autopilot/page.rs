//! Reading a page into the facts the checks ask about.
//!
//! One pass over the markup, done once per audit. Every check reads from
//! here rather than from the HTML, so two checks can never disagree about
//! what the page says.

use std::collections::HashMap;
use std::sync::OnceLock;

use regex::Regex;
use scraper::{ElementRef, Html, Selector};

use super::{Goal, PageType, Understanding};
use crate::ux::signals::{self, Region};

macro_rules! selector {
    ($sel:expr) => {{
        static CELL: OnceLock<Selector> = OnceLock::new();
        CELL.get_or_init(|| Selector::parse($sel).expect("built-in selector is valid"))
    }};
}

macro_rules! pattern {
    ($re:expr) => {{
        static CELL: OnceLock<Regex> = OnceLock::new();
        CELL.get_or_init(|| Regex::new($re).expect("built-in pattern is valid"))
    }};
}

/// One form control.
#[derive(Debug, Clone, PartialEq)]
pub struct Field {
    /// The input type, or `select` / `textarea`.
    pub kind: String,
    pub name: String,
    /// The accessible label, from `<label>`, `aria-label`, `aria-labelledby`
    /// or `title`. A placeholder is not a label.
    pub label: Option<String>,
    pub placeholder: Option<String>,
    pub required: bool,
    pub autocomplete: Option<String>,
}

impl Field {
    /// What a person would call this field.
    pub fn display(&self) -> String {
        self.label
            .clone()
            .or_else(|| self.placeholder.clone())
            .filter(|label| !label.is_empty())
            .unwrap_or_else(|| {
                if self.name.is_empty() { self.kind.clone() } else { self.name.clone() }
            })
    }

    pub fn collects_contact(&self) -> bool {
        let name = self.name.to_ascii_lowercase();
        matches!(self.kind.as_str(), "email" | "tel")
            || name.contains("email")
            || name.contains("phone")
            || name.contains("tel")
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Form {
    pub fields: Vec<Field>,
    /// The submit control's label. `None` when the form has no way to submit
    /// it in the markup.
    pub submit: Option<String>,
    pub is_search: bool,
    /// The form or its container says what happens to the data.
    pub privacy_note: bool,
    pub region: Region,
}

impl Form {
    pub fn collects_contact(&self) -> bool {
        self.fields.iter().any(Field::collects_contact)
    }

    pub fn summary(&self) -> String {
        format!(
            "{} field{}{} → {}",
            self.fields.len(),
            if self.fields.len() == 1 { "" } else { "s" },
            if self.is_search { " (search)" } else { "" },
            self.submit.as_deref().unwrap_or("no submit button"),
        )
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Link {
    pub label: String,
    pub href: String,
    pub region: Region,
}

/// Something styled or worded as the thing to do.
#[derive(Debug, Clone, PartialEq)]
pub struct Cta {
    pub label: String,
    pub href: Option<String>,
    pub region: Region,
    /// 0..1 through the page, in document order.
    pub position: f32,
    /// `a`, `button` or `input`.
    pub tag: String,
    /// The label reads like a commitment ("Start free trial").
    pub commitment: bool,
}

impl Cta {
    pub fn serves(&self, goal: Goal) -> bool {
        let label = self.label.to_ascii_lowercase();
        goal.action_words().iter().any(|word| label.contains(word))
    }
}

/// Everything the checks need to know about the page.
#[derive(Debug, Clone, Default)]
pub struct PageFacts {
    pub url: String,
    pub title: Option<String>,
    pub meta_description: Option<String>,
    pub viewport: Option<String>,
    pub lang: Option<String>,
    pub og_type: Option<String>,
    pub h1s: Vec<String>,
    pub headings: Vec<(u8, String)>,
    /// Visible text, collapsed.
    pub text: String,
    /// The same, lowercased and padded word by word for whole-word search.
    norm: String,
    pub word_count: u32,
    /// Word counts of each paragraph.
    pub paragraphs: Vec<u32>,
    /// The first paragraph long enough to be a sentence about the page.
    pub lead: Option<String>,
    /// Average words per sentence across the paragraphs.
    pub sentence_length: f32,
    pub forms: Vec<Form>,
    /// Fields with no `<form>` around them — usually a script-driven form.
    pub loose_fields: u32,
    pub links: Vec<Link>,
    pub ctas: Vec<Cta>,
    pub images: u32,
    pub images_without_alt: u32,
    pub images_without_size: u32,
    pub has_video: bool,
    pub schema_types: Vec<String>,
    pub tel: Vec<String>,
    pub mailto: Vec<String>,
    pub has_address: bool,
    pub has_search: bool,
    pub filter_controls: u32,
    pub analytics: Vec<String>,
    /// Inline CSS removes the focus outline somewhere.
    pub outline_removed: bool,
    /// Inline CSS styles `:focus` or `:focus-visible`.
    pub focus_styles: bool,
    pub external_css: u32,
    pub has_article: bool,
    pub has_published_date: bool,
    pub has_author: bool,
    pub positive_tabindex: u32,
    /// Interactive nodes in document order, for the flow model.
    pub nodes: Vec<signals::UxNode>,
}

impl PageFacts {
    /// Whole-word, case-insensitive: does the visible text say this?
    pub fn says(&self, phrase: &str) -> bool {
        let needle = normalise(phrase);
        !needle.trim().is_empty() && self.norm.contains(&needle)
    }

    /// The first of these phrases the page says.
    pub fn says_any<'a>(&self, phrases: &[&'a str]) -> Option<&'a str> {
        phrases.iter().copied().find(|phrase| self.says(phrase))
    }

    /// Every one of these phrases the page says.
    pub fn says_all_of<'a>(&self, phrases: &[&'a str]) -> Vec<&'a str> {
        phrases.iter().copied().filter(|phrase| self.says(phrase)).collect()
    }

    /// A short excerpt of the visible text around the first match of a
    /// phrase, for evidence.
    pub fn excerpt(&self, phrase: &str) -> Option<String> {
        let pattern = Regex::new(&format!(r"(?i)\b{}", regex::escape(phrase))).ok()?;
        let found = pattern.find(&self.text)?;
        Some(window(&self.text, found.start(), found.end(), 70))
    }

    pub fn excerpt_match(&self, pattern: &Regex) -> Option<String> {
        let found = pattern.find(&self.text)?;
        Some(window(&self.text, found.start(), found.end(), 50))
    }

    pub fn has_schema(&self, types: &[&str]) -> Option<String> {
        self.schema_types
            .iter()
            .find(|found| types.iter().any(|t| found.eq_ignore_ascii_case(t)))
            .cloned()
    }

    /// The first price-looking string on the page.
    pub fn price(&self) -> Option<String> {
        let price = pattern!(
            r"(?i)(?:[$€£¥₹]\s?\d[\d.,]*|\b\d[\d.,]*\s?(?:€|eur|usd|gbp|chf)\b|\b\d[\d.,]*\s?(?:/|per\s)\s?(?:mo|month|year|yr|user|seat)\b)"
        );
        price.find(&self.text).map(|m| m.as_str().trim().to_string())
    }

    pub fn opening_hours(&self) -> Option<String> {
        let hours = pattern!(
            r"(?i)\b(?:mon|tue|wed|thu|fri|sat|sun)[a-z]*\.?\s?(?:[-–—to]+\s?(?:mon|tue|wed|thu|fri|sat|sun)[a-z]*\.?)?[:,]?\s+\d{1,2}(?:[:.]\d{2})?\s?(?:am|pm)?\s?[-–—to]+\s?\d{1,2}"
        );
        self.excerpt_match(hours)
            .or_else(|| self.says_any(&["opening hours", "open hours", "hours of operation", "open 24"]).and_then(|p| self.excerpt(p)))
    }

    /// The page's main action, given what the visit is for.
    ///
    /// The first call to action that serves the goal; failing that the first
    /// that reads like a commitment; failing that the first button-styled
    /// action outside the footer. Navigation and footer links lose to
    /// anything in the content.
    pub fn primary_cta(&self, goal: Goal) -> Option<&Cta> {
        let content = |cta: &&Cta| !matches!(cta.region, Region::Footer | Region::Nav);
        self.ctas
            .iter()
            .filter(content)
            .find(|cta| cta.serves(goal))
            .or_else(|| self.ctas.iter().filter(content).find(|cta| cta.commitment))
            .or_else(|| self.ctas.iter().find(content))
            .or_else(|| self.ctas.iter().find(|cta| cta.serves(goal)))
    }

    pub fn nav_links(&self) -> Vec<&Link> {
        self.links
            .iter()
            .filter(|link| matches!(link.region, Region::Nav | Region::Header))
            .collect()
    }

    /// The forms a visitor fills in, not the site search.
    pub fn task_forms(&self) -> Vec<&Form> {
        self.forms.iter().filter(|form| !form.is_search && !form.fields.is_empty()).collect()
    }

    pub fn link_to(&self, words: &[&str]) -> Option<&Link> {
        self.links.iter().find(|link| {
            let label = link.label.to_ascii_lowercase();
            let href = link.href.to_ascii_lowercase();
            words.iter().any(|word| label.contains(word) || href.contains(&word.replace(' ', "-")))
        })
    }
}

/// Lowercase and space-pad every word, so a phrase can only match whole.
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

fn collapse(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// `pad` characters either side of a byte range, cut on character boundaries.
fn window(text: &str, start: usize, end: usize, pad: usize) -> String {
    let before: Vec<(usize, char)> = text[..start].char_indices().collect();
    let from = before.len().checked_sub(pad).map_or(0, |i| before[i].0);
    let to = text[end..].char_indices().nth(pad).map_or(text.len(), |(i, _)| end + i);
    let mut out = String::new();
    if from > 0 {
        out.push('…');
    }
    out.push_str(text[from..to].trim());
    if to < text.len() {
        out.push('…');
    }
    out
}

fn attr<'a>(element: ElementRef<'a>, name: &str) -> Option<&'a str> {
    element.value().attr(name).map(str::trim).filter(|v| !v.is_empty())
}

fn hidden(element: ElementRef<'_>) -> bool {
    element.value().attr("aria-hidden") == Some("true") || element.value().attr("hidden").is_some()
}

fn inside<'a>(element: ElementRef<'a>, tag: &str) -> Option<ElementRef<'a>> {
    element.ancestors().filter_map(ElementRef::wrap).find(|a| a.value().name() == tag)
}

fn visible_text(document: &Html) -> String {
    let mut out = String::new();
    for node in document.tree.root().descendants() {
        let Some(text) = node.value().as_text() else { continue };
        let invisible = node.ancestors().any(|ancestor| {
            ancestor.value().as_element().is_some_and(|e| {
                matches!(e.name(), "script" | "style" | "noscript" | "template" | "svg" | "head")
                    || e.attr("aria-hidden") == Some("true")
                    || e.attr("hidden").is_some()
            })
        });
        if !invisible {
            out.push_str(text);
            out.push(' ');
        }
    }
    collapse(&out)
}

/// Words that label interface chrome, not an action anyone came to take.
const CHROME: &[&str] = &[
    "menu", "close", "open menu", "toggle navigation", "toggle menu", "search", "×", "x",
    "accept", "accept all", "reject", "reject all", "decline", "ok", "got it", "cookie settings",
    "manage cookies", "previous", "next", "prev", "play", "pause", "skip to content", "back to top",
];

fn label_of_control(element: ElementRef<'_>) -> String {
    let text = signals::text_of(element);
    let label = if text.is_empty() {
        attr(element, "aria-label")
            .or_else(|| attr(element, "value"))
            .or_else(|| attr(element, "title"))
            .unwrap_or_default()
            .to_string()
    } else {
        text
    };
    if label.is_empty() {
        // An icon link: the image inside it is its only name.
        element
            .select(selector!("img[alt]"))
            .find_map(|img| attr(img, "alt").map(str::to_string))
            .unwrap_or_default()
    } else {
        label
    }
}

fn schema_types(document: &Html) -> Vec<String> {
    fn walk(value: &serde_json::Value, out: &mut Vec<String>) {
        match value {
            serde_json::Value::Object(map) => {
                match map.get("@type") {
                    Some(serde_json::Value::String(t)) => out.push(t.clone()),
                    Some(serde_json::Value::Array(ts)) => {
                        out.extend(ts.iter().filter_map(|t| t.as_str().map(str::to_string)))
                    }
                    _ => {}
                }
                for child in map.values() {
                    walk(child, out);
                }
            }
            serde_json::Value::Array(items) => items.iter().for_each(|item| walk(item, out)),
            _ => {}
        }
    }

    let mut out = Vec::new();
    for script in document.select(selector!(r#"script[type="application/ld+json"]"#)) {
        let body: String = script.text().collect();
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(&body) {
            walk(&value, &mut out);
        }
    }
    for item in document.select(selector!("[itemtype]")) {
        if let Some(kind) = item.value().attr("itemtype").and_then(|t| t.rsplit('/').next()) {
            out.push(kind.trim().to_string());
        }
    }
    out.sort();
    out.dedup();
    out
}

const ANALYTICS: &[(&str, &str)] = &[
    ("googletagmanager.com", "Google Tag Manager"),
    ("google-analytics.com", "Google Analytics"),
    ("gtag(", "Google Analytics"),
    ("plausible.io", "Plausible"),
    ("cdn.segment.com", "Segment"),
    ("posthog", "PostHog"),
    ("matomo", "Matomo"),
    ("hotjar", "Hotjar"),
    ("usefathom.com", "Fathom"),
    ("mixpanel", "Mixpanel"),
    ("amplitude", "Amplitude"),
    ("clarity.ms", "Microsoft Clarity"),
    ("umami", "Umami"),
    ("simpleanalytics", "Simple Analytics"),
    ("heap-", "Heap"),
];

fn read_form(form: ElementRef<'_>, labels: &HashMap<String, String>, ids: &HashMap<String, String>) -> Form {
    let fields: Vec<Field> = form
        .select(selector!(
            "input:not([type=hidden]):not([type=submit]):not([type=button]):not([type=image]):not([type=reset]), select, textarea"
        ))
        .filter(|field| !hidden(*field))
        .map(|field| read_field(field, labels, ids))
        .collect();

    let submit = form
        .select(selector!("button:not([type=button]):not([type=reset]), input[type=submit], input[type=image]"))
        .map(label_of_control)
        .next()
        .map(|label| if label.is_empty() { "(unlabelled button)".to_string() } else { label });

    let action = form.value().attr("action").unwrap_or_default().to_ascii_lowercase();
    let is_search = form.value().attr("role") == Some("search")
        || action.contains("search")
        || fields.iter().any(|f| {
            f.kind == "search" || matches!(f.name.as_str(), "q" | "s" | "query" | "search" | "keyword" | "keywords")
        });

    // The reassurance usually sits just under the form, not inside it.
    let container = form.parent().and_then(ElementRef::wrap).unwrap_or(form);
    let around = normalise(&signals::text_of(container));
    let privacy_note = ["privacy", "spam", "unsubscribe", "never share", "won t share", "gdpr", "your data", "personal data", "data protection"]
        .iter()
        .any(|phrase| around.contains(&format!(" {phrase} ")))
        || container.select(selector!("a[href]")).any(|a| a.value().attr("href").unwrap_or_default().contains("privacy"));

    Form { fields, submit, is_search, privacy_note, region: signals::region_of(form) }
}

fn read_field(field: ElementRef<'_>, labels: &HashMap<String, String>, ids: &HashMap<String, String>) -> Field {
    let name = field.value().name();
    let kind = if name == "input" {
        field.value().attr("type").unwrap_or("text").to_ascii_lowercase()
    } else {
        name.to_string()
    };

    let label = attr(field, "id")
        .and_then(|id| labels.get(id).cloned())
        .or_else(|| inside(field, "label").map(signals::text_of))
        .or_else(|| attr(field, "aria-label").map(str::to_string))
        .or_else(|| {
            attr(field, "aria-labelledby").map(|refs| {
                refs.split_whitespace().filter_map(|id| ids.get(id).cloned()).collect::<Vec<_>>().join(" ")
            })
        })
        .or_else(|| attr(field, "title").map(str::to_string))
        .map(|label| collapse(&label))
        .filter(|label| !label.is_empty());

    let required = field.value().attr("required").is_some()
        || field.value().attr("aria-required") == Some("true")
        || label.as_deref().is_some_and(|l| l.contains('*') || l.to_ascii_lowercase().contains("required"));

    Field {
        kind,
        name: field.value().attr("name").unwrap_or_default().to_string(),
        label,
        placeholder: attr(field, "placeholder").map(str::to_string),
        required,
        autocomplete: attr(field, "autocomplete").map(str::to_string),
    }
}

/// Read a page.
pub fn extract(html: &str, url: &str) -> PageFacts {
    let document = Html::parse_document(html);
    let text = visible_text(&document);
    let norm = normalise(&text);

    let meta = |name: &str| {
        document
            .select(selector!("meta[name], meta[property]"))
            .find(|m| {
                m.value().attr("name").or_else(|| m.value().attr("property")).is_some_and(|n| n.eq_ignore_ascii_case(name))
            })
            .and_then(|m| attr(m, "content").map(str::to_string))
    };

    let headings: Vec<(u8, String)> = document
        .select(selector!("h1, h2, h3, h4, h5, h6"))
        .filter(|h| !hidden(*h))
        .map(|h| (h.value().name()[1..].parse().unwrap_or(6), signals::text_of(h)))
        .collect();

    let paragraphs: Vec<String> = document
        .select(selector!("p"))
        .filter(|p| !hidden(*p))
        .map(signals::text_of)
        .filter(|p| !p.is_empty())
        .collect();
    let lead = paragraphs.iter().find(|p| p.split_whitespace().count() >= 8).cloned();
    let (sentences, sentence_words) = paragraphs.iter().fold((0u32, 0u32), |(s, w), p| {
        let count = p.split(['.', '!', '?']).filter(|s| s.split_whitespace().count() > 2).count() as u32;
        (s + count.max(1), w + p.split_whitespace().count() as u32)
    });

    // `<label for>` and `aria-labelledby` both point at ids; resolve once.
    let mut labels = HashMap::new();
    for label in document.select(selector!("label[for]")) {
        if let Some(target) = attr(label, "for") {
            labels.insert(target.to_string(), signals::text_of(label));
        }
    }
    let mut ids = HashMap::new();
    for element in document.select(selector!("[id]")) {
        if let Some(id) = attr(element, "id") {
            ids.entry(id.to_string()).or_insert_with(|| signals::text_of(element));
        }
    }

    let forms: Vec<Form> = document.select(selector!("form")).map(|f| read_form(f, &labels, &ids)).collect();
    let loose_fields = document
        .select(selector!("input:not([type=hidden]):not([type=submit]):not([type=button]), select, textarea"))
        .filter(|field| inside(*field, "form").is_none() && !hidden(*field))
        .count() as u32;

    // Every element a visitor attends to, in document order, so a call to
    // action can say how far down the page it sits.
    let ordered: Vec<ElementRef<'_>> = document
        .select(selector!("h1, h2, h3, h4, h5, h6, a[href], button, input, select, textarea, img, p, li"))
        .collect();
    let total = ordered.len().max(1) as f32;

    let mut links = Vec::new();
    let mut ctas = Vec::new();
    for (index, element) in ordered.iter().enumerate() {
        let element = *element;
        if hidden(element) {
            continue;
        }
        let name = element.value().name();
        let position = index as f32 / total;
        match name {
            "a" => {
                let label = label_of_control(element);
                let href = element.value().attr("href").unwrap_or_default().trim().to_string();
                let region = signals::region_of(element);
                let class = element.value().attr("class").unwrap_or_default().to_ascii_lowercase();
                let styled = class.contains("btn") || class.contains("button") || class.contains("cta")
                    || element.value().attr("role") == Some("button");
                let commitment = signals::looks_like_commitment(&label);
                if (styled || commitment) && !label.is_empty() && !is_chrome(&label) {
                    ctas.push(Cta { label: label.clone(), href: Some(href.clone()), region, position, tag: "a".into(), commitment });
                }
                links.push(Link { label, href, region });
            }
            "button" | "input" => {
                let kind = element.value().attr("type").unwrap_or(if name == "button" { "submit" } else { "text" });
                if name == "input" && !matches!(kind, "submit" | "button" | "image") {
                    continue;
                }
                let form = inside(element, "form");
                let in_search = form.is_some_and(|f| read_form(f, &labels, &ids).is_search);
                let label = label_of_control(element);
                if label.is_empty() || in_search || is_chrome(&label) {
                    continue;
                }
                ctas.push(Cta {
                    commitment: signals::looks_like_commitment(&label) || (form.is_some() && kind == "submit"),
                    label,
                    href: None,
                    region: signals::region_of(element),
                    position,
                    tag: name.to_string(),
                });
            }
            _ => {}
        }
    }

    let images: Vec<ElementRef<'_>> = document.select(selector!("img")).filter(|i| !hidden(*i)).collect();
    let scripts: String = document
        .select(selector!("script"))
        .map(|s| format!("{} {}", s.value().attr("src").unwrap_or_default(), s.text().collect::<String>()))
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase();
    let mut analytics: Vec<String> = ANALYTICS
        .iter()
        .filter(|(marker, _)| scripts.contains(marker))
        .map(|(_, name)| name.to_string())
        .collect();
    analytics.dedup();

    let css: String = document.select(selector!("style")).flat_map(|s| s.text()).collect::<String>().to_ascii_lowercase();
    let inline_styles: String = document
        .select(selector!("[style]"))
        .filter_map(|e| e.value().attr("style"))
        .collect::<Vec<_>>()
        .join(";")
        .to_ascii_lowercase();
    let outline_off = pattern!(r"outline\s*:\s*(?:none|0(?:px)?)\b");

    let tel: Vec<String> = links.iter().filter(|l| l.href.starts_with("tel:")).map(|l| l.href[4..].to_string()).collect();
    let mailto: Vec<String> = links
        .iter()
        .filter(|l| l.href.starts_with("mailto:"))
        .map(|l| super::redact(l.href[7..].split('?').next().unwrap_or_default()))
        .collect();
    let schema_types = schema_types(&document);
    let has_address = document.select(selector!("address")).next().is_some()
        || schema_types.iter().any(|t| t == "PostalAddress")
        || pattern!(r"(?i)\b\d{1,5}\s+[A-Za-z][A-Za-z .'-]{2,30}\s(?:street|st\.|avenue|ave\.?|road|rd\.|boulevard|blvd|lane|ln\.|drive|dr\.|way|place|suite|rue|straße|strasse)\b")
            .is_match(&text);

    let has_search = document.select(selector!("input[type=search], [role=search]")).next().is_some()
        || forms.iter().any(|f| f.is_search);
    let filter_controls = document
        .select(selector!("select, input[type=checkbox], input[type=radio]"))
        .filter(|control| inside(*control, "form").map_or(true, |f| read_form(f, &labels, &ids).is_search || !read_form(f, &labels, &ids).collects_contact()))
        .count() as u32;

    let nodes = signals::extract(&document);

    PageFacts {
        url: url.to_string(),
        title: document.select(selector!("title")).next().map(signals::text_of).filter(|t| !t.is_empty()),
        meta_description: meta("description"),
        viewport: meta("viewport"),
        lang: document.select(selector!("html")).next().and_then(|h| attr(h, "lang").map(str::to_string)),
        og_type: meta("og:type"),
        h1s: headings.iter().filter(|(level, _)| *level == 1).map(|(_, t)| t.clone()).collect(),
        headings,
        word_count: text.split_whitespace().count() as u32,
        paragraphs: paragraphs.iter().map(|p| p.split_whitespace().count() as u32).collect(),
        lead,
        sentence_length: if sentences > 0 { sentence_words as f32 / sentences as f32 } else { 0.0 },
        text,
        norm,
        forms,
        loose_fields,
        links,
        ctas,
        images: images.len() as u32,
        images_without_alt: images.iter().filter(|i| i.value().attr("alt").is_none()).count() as u32,
        images_without_size: images
            .iter()
            .filter(|i| i.value().attr("width").is_none() || i.value().attr("height").is_none())
            .count() as u32,
        has_video: document
            .select(selector!("video, iframe[src*=youtube], iframe[src*=vimeo], iframe[src*=wistia], iframe[src*=loom]"))
            .next()
            .is_some(),
        schema_types,
        tel,
        mailto,
        has_address,
        has_search,
        filter_controls,
        analytics,
        outline_removed: outline_off.is_match(&css) || outline_off.is_match(&inline_styles),
        focus_styles: css.contains(":focus"),
        external_css: document.select(selector!(r#"link[rel~="stylesheet"]"#)).count() as u32,
        has_article: document.select(selector!("article")).next().is_some(),
        has_published_date: document
            .select(selector!(r#"time[datetime], meta[property="article:published_time"], [itemprop=datePublished]"#))
            .next()
            .is_some(),
        has_author: document
            .select(selector!(r#"meta[name=author], [rel=author], [itemprop=author], [class*=author], [class*=byline]"#))
            .next()
            .is_some(),
        positive_tabindex: document
            .select(selector!("[tabindex]"))
            .filter(|e| e.value().attr("tabindex").and_then(|t| t.trim().parse::<i32>().ok()).is_some_and(|t| t > 0))
            .count() as u32,
        nodes,
    }
}

fn is_chrome(label: &str) -> bool {
    let lower = label.trim().to_ascii_lowercase();
    CHROME.contains(&lower.as_str()) || lower.chars().count() <= 1
}

/// Signals a visitor would take as reasons to trust the page.
pub fn trust_signals(facts: &PageFacts) -> Vec<String> {
    let mut found: Vec<String> = facts
        .says_all_of(&[
            "testimonial", "testimonials", "reviews", "rated", "stars", "trusted by", "case study",
            "case studies", "customers", "clients", "award", "certified", "accredited", "guarantee",
            "money back", "secure checkout", "soc 2", "iso 27001", "gdpr", "hipaa", "years of experience",
            "insured", "licensed",
        ])
        .into_iter()
        .map(str::to_string)
        .collect();
    if let Some(schema) = facts.has_schema(&["Review", "AggregateRating"]) {
        found.push(format!("{schema} structured data"));
    }
    found.dedup();
    found
}

/// Say back what the page is, before judging it.
pub fn understand(facts: &PageFacts, page_type: PageType, goal: Goal) -> Understanding {
    let main = facts.primary_cta(goal);
    let mut other_ctas: Vec<String> = Vec::new();
    for cta in &facts.ctas {
        if Some(cta.label.as_str()) != main.map(|m| m.label.as_str()) && !other_ctas.contains(&cta.label) {
            other_ctas.push(cta.label.clone());
        }
    }
    other_ctas.truncate(6);

    let mut nav: Vec<String> = Vec::new();
    for link in facts.nav_links() {
        if !link.label.is_empty() && !nav.contains(&link.label) && link.label.len() <= 40 {
            nav.push(link.label.clone());
        }
    }
    nav.truncate(10);

    Understanding {
        title: facts.title.clone(),
        h1: facts.h1s.first().cloned(),
        offer: facts.meta_description.clone().or_else(|| facts.lead.clone()),
        main_cta: main.map(|cta| cta.label.clone()),
        other_ctas,
        forms: facts.forms.iter().map(Form::summary).collect(),
        nav,
        trust: trust_signals(facts),
        word_count: facts.word_count,
        intent: format!(
            "Reads as a {} page. Visitors are here to: {}. The goal under test is {}.",
            page_type.label().to_lowercase(),
            page_type.conversion().to_lowercase(),
            goal.label().to_lowercase(),
        ),
    }
}
