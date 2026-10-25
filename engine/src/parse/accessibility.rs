//! Accessibility checks that can be made from static HTML.
//!
//! ## What is and is not knowable without a browser
//!
//! A great deal of WCAG needs layout, computed styles or a real accessibility
//! tree: colour contrast, focus visibility, reading order, whether a thing is
//! actually visible. None of that is available to a crawler that never renders,
//! and claiming otherwise would be worse than not checking at all.
//!
//! What *is* knowable from the markup is a set of failures that are common and
//! unambiguous: a control with no accessible name, a page with no language, a
//! broken `aria-labelledby`, a role that does not exist, a zoom lock in the
//! viewport. Those are reported as definite failures.
//!
//! A second, smaller set is *suspicious* rather than certain — a table with no
//! headers may be a layout table, a video with no `<track>` may have its
//! captions burned in. Those are recorded too, and `a11y::rules` marks them as
//! needing a person to confirm, so the report never presents a guess as a fact.
//!
//! When JavaScript rendering arrives, this same pass runs over the rendered
//! DOM and gets strictly better without changing.
//!
//! ## Two shapes of output
//!
//! The original ten counters stay as they were: the crawl table and the Issues
//! tab are built on them. Every rule — those ten and the rest — also lands in
//! [`AccessibilityReport::findings`], with a few snippets of the offending
//! markup, which is what the compliance report is built from.

use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;

use regex::Regex;
use scraper::{ElementRef, Html, Selector};

/// Bumped whenever a rule is added or changes meaning. Stored per page, so a
/// report can tell a page checked by today's rules from one crawled before
/// they existed.
pub const RULES_VERSION: i64 = 1;

/// Snippets kept per rule per page. Enough to recognise the pattern; the
/// count says how many there were.
pub const MAX_SAMPLES: usize = 3;

/// Rule ids. Their meaning, criteria and fixes live in `a11y::rules`.
pub mod rule {
    pub const LINK_NO_NAME: &str = "link_no_name";
    pub const BUTTON_NO_NAME: &str = "button_no_name";
    pub const FIELD_NO_LABEL: &str = "field_no_label";
    pub const BROKEN_ARIA_REF: &str = "broken_aria_ref";
    pub const PAGE_NO_LANG: &str = "page_no_lang";
    pub const LANG_INVALID: &str = "lang_invalid";
    pub const DUPLICATE_ID: &str = "duplicate_id";
    pub const DUPLICATE_ID_REFERENCED: &str = "duplicate_id_referenced";
    pub const INVALID_ROLE: &str = "invalid_role";
    pub const POSITIVE_TABINDEX: &str = "positive_tabindex";
    pub const IFRAME_NO_TITLE: &str = "iframe_no_title";
    pub const HEADING_SKIP: &str = "heading_skip";
    pub const IMG_NO_ALT: &str = "img_no_alt";
    pub const ZOOM_DISABLED: &str = "zoom_disabled";
    pub const META_REFRESH: &str = "meta_refresh";
    pub const BLINK_MARQUEE: &str = "blink_marquee";
    pub const AUTOPLAY_AUDIO: &str = "autoplay_audio";
    pub const VIDEO_NO_CAPTIONS: &str = "video_no_captions";
    pub const PAGE_NO_TITLE: &str = "page_no_title";
    pub const NO_BYPASS: &str = "no_bypass";
    pub const HIDDEN_FOCUSABLE: &str = "hidden_focusable";
    pub const NESTED_INTERACTIVE: &str = "nested_interactive";
    pub const EMPTY_HEADING: &str = "empty_heading";
    pub const VAGUE_LINK_TEXT: &str = "vague_link_text";
    pub const LABEL_NOT_IN_NAME: &str = "label_not_in_name";
    pub const INVALID_AUTOCOMPLETE: &str = "invalid_autocomplete";
    pub const MISSING_AUTOCOMPLETE: &str = "missing_autocomplete";
    pub const LIST_STRUCTURE: &str = "list_structure";
    pub const TABLE_NO_HEADERS: &str = "table_no_headers";
    pub const FOCUS_OUTLINE_REMOVED: &str = "focus_outline_removed";
    pub const ROLE_NOT_FOCUSABLE: &str = "role_not_focusable";
    pub const CLICK_HANDLER_NOT_FOCUSABLE: &str = "click_handler_not_focusable";

    /// Every rule, in the order a report lists them.
    pub const ALL: &[&str] = &[
        LINK_NO_NAME,
        BUTTON_NO_NAME,
        FIELD_NO_LABEL,
        IMG_NO_ALT,
        ZOOM_DISABLED,
        HIDDEN_FOCUSABLE,
        BROKEN_ARIA_REF,
        DUPLICATE_ID_REFERENCED,
        PAGE_NO_LANG,
        LANG_INVALID,
        PAGE_NO_TITLE,
        IFRAME_NO_TITLE,
        NESTED_INTERACTIVE,
        ROLE_NOT_FOCUSABLE,
        INVALID_ROLE,
        INVALID_AUTOCOMPLETE,
        LIST_STRUCTURE,
        META_REFRESH,
        BLINK_MARQUEE,
        NO_BYPASS,
        EMPTY_HEADING,
        POSITIVE_TABINDEX,
        HEADING_SKIP,
        DUPLICATE_ID,
        AUTOPLAY_AUDIO,
        VIDEO_NO_CAPTIONS,
        FOCUS_OUTLINE_REMOVED,
        CLICK_HANDLER_NOT_FOCUSABLE,
        TABLE_NO_HEADERS,
        LABEL_NOT_IN_NAME,
        VAGUE_LINK_TEXT,
        MISSING_AUTOCOMPLETE,
    ];
}

/// Things a page *has*, rather than does wrong. They decide which success
/// criteria apply at all: a site with no video has nothing to caption.
pub mod feature {
    pub const VIDEO: &str = "has_video";
    pub const AUDIO: &str = "has_audio";
    pub const FORM: &str = "has_form";
    pub const PASSWORD: &str = "has_password";

    pub const ALL: &[&str] = &[VIDEO, AUDIO, FORM, PASSWORD];
}

/// One rule's failures on one page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub rule: &'static str,
    /// Every instance on the page.
    pub count: u32,
    /// Up to [`MAX_SAMPLES`] distinct snippets of the offending markup.
    pub samples: Vec<String>,
}

/// What one page's markup says about its accessibility.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AccessibilityReport {
    /// Links with no text, no image alt text and no ARIA label. A screen
    /// reader announces these as "link" and nothing else.
    pub links_without_name: u32,
    /// Buttons in the same state.
    pub buttons_without_name: u32,
    /// Form fields with no `<label>`, `aria-label` or `aria-labelledby`.
    pub inputs_without_label: u32,
    /// `id` values used more than once. Duplicate ids silently break every
    /// `aria-labelledby`, `aria-describedby` and `<label for>` that uses them.
    pub duplicate_ids: u32,
    /// `aria-labelledby` or `aria-describedby` pointing at an id that is not
    /// on the page, which leaves the element unnamed.
    pub broken_aria_references: u32,
    /// `role="..."` values that are not ARIA roles. An unknown role is ignored,
    /// so the element falls back to its native semantics — or to none.
    pub invalid_roles: Vec<String>,
    /// `tabindex` greater than zero, which forces an element out of document
    /// order and scrambles keyboard navigation for the whole page.
    pub positive_tabindex: u32,
    /// `<iframe>` with no `title`, announced only as "frame".
    pub iframes_without_title: u32,
    /// Heading levels skipped going down, such as an `<h2>` followed by an
    /// `<h4>`. Screen-reader users navigate by heading level.
    pub skipped_heading_levels: u32,
    /// Every heading level found, in document order. Kept so the UI can show
    /// the outline.
    pub heading_outline: Vec<u8>,
    /// Every rule that failed on this page, in the order first met.
    pub findings: Vec<Finding>,
    /// Which of [`feature::ALL`] the page has.
    pub features: Vec<&'static str>,
}

impl AccessibilityReport {
    /// Total definite failures among the original checks. Used for the
    /// summary column, so its meaning does not drift as rules are added.
    pub fn issue_count(&self) -> u32 {
        self.links_without_name
            + self.buttons_without_name
            + self.inputs_without_label
            + self.duplicate_ids
            + self.broken_aria_references
            + self.invalid_roles.len() as u32
            + self.positive_tabindex
            + self.iframes_without_title
            + self.skipped_heading_levels
    }

    pub fn is_clean(&self) -> bool {
        self.issue_count() == 0
    }

    /// One rule's finding, if it failed here.
    pub fn finding(&self, rule: &str) -> Option<&Finding> {
        self.findings.iter().find(|finding| finding.rule == rule)
    }

    /// How many times a rule failed here.
    pub fn count(&self, rule: &str) -> u32 {
        self.finding(rule).map_or(0, |finding| finding.count)
    }

    pub fn has_feature(&self, feature: &str) -> bool {
        self.features.contains(&feature)
    }
}

macro_rules! selector {
    ($name:ident, $sel:expr) => {
        static $name: LazyLock<Selector> = LazyLock::new(|| {
            Selector::parse($sel).expect(concat!("built-in selector is valid: ", $sel))
        });
    };
}

selector!(SEL_ANCHOR, "a[href]");
selector!(
    SEL_BUTTON,
    "button, input[type=submit], input[type=button], input[type=reset], [role=button]"
);
selector!(SEL_INPUT, "input, select, textarea");
selector!(SEL_LABEL, "label");
selector!(SEL_WITH_ID, "[id]");
selector!(SEL_ROLE, "[role]");
selector!(SEL_TABINDEX, "[tabindex]");
selector!(SEL_IFRAME, "iframe");
selector!(SEL_HEADING, "h1, h2, h3, h4, h5, h6");
selector!(SEL_IMG, "img");
selector!(
    SEL_IMAGE_LIKE,
    "img, input[type=image], area[href], svg[role=img]"
);
selector!(SEL_SVG_TITLE, "title");
selector!(SEL_ARIA_REF, "[aria-labelledby], [aria-describedby]");
selector!(SEL_HTML, "html");
selector!(SEL_HEAD_TITLE, "head title");
selector!(SEL_META, "meta");
selector!(SEL_LANG, "[lang]");
selector!(SEL_OLD_MOTION, "blink, marquee");
selector!(SEL_MEDIA, "video, audio");
selector!(SEL_TRACK, "track");
selector!(SEL_MAIN, "main, [role=main]");
selector!(SEL_ARIA_HIDDEN, "[aria-hidden=true]");
selector!(
    SEL_MAYBE_FOCUSABLE,
    "a[href], button, input, select, textarea, summary, iframe, [tabindex], [contenteditable]"
);
selector!(SEL_INTERACTIVE_HOST, "a[href], button");
selector!(SEL_LIST, "ul, ol");
selector!(SEL_LIST_ITEM, "li");
selector!(SEL_TABLE, "table");
selector!(SEL_ROW, "tr");
selector!(SEL_CELL, "td, th");
selector!(
    SEL_TABLE_HEADER,
    "th, [role=columnheader], [role=rowheader], [scope]"
);
selector!(SEL_STYLE, "style");
selector!(SEL_STYLED, "[style]");
selector!(SEL_ONCLICK, "[onclick]");
selector!(SEL_FORM, "form");
selector!(SEL_PASSWORD, "input[type=password]");

/// Input types that take no label because they are not user-facing fields.
const UNLABELLED_INPUT_TYPES: &[&str] = &["hidden", "submit", "button", "reset", "image"];

/// The ARIA 1.2 role vocabulary.
///
/// Spelled out rather than pattern-matched: the whole value of this check is
/// catching `role="buton"` and `role="navigation-bar"`, which only a real list
/// can do.
const ARIA_ROLES: &str = "alert alertdialog application article associationlist associationlistitemkey \
    associationlistitemvalue banner blockquote button caption cell checkbox code columnheader combobox \
    command comment complementary composite contentinfo definition deletion dialog directory document \
    emphasis feed figure form generic grid gridcell group heading image img input insertion landmark \
    link list listbox listitem log main mark marquee math menu menubar menuitem menuitemcheckbox \
    menuitemradio meter navigation none note option paragraph presentation progressbar radio radiogroup \
    range region roletype row rowgroup rowheader scrollbar search searchbox section sectionhead select \
    separator slider spinbutton status strong structure subscript suggestion superscript switch tab \
    table tablist tabpanel term textbox time timer toolbar tooltip tree treegrid treeitem widget window";

/// Widget roles a keyboard user must be able to reach by Tab on their own.
///
/// Roles that are legitimately managed by a parent's `aria-activedescendant`
/// (option, treeitem, gridcell, menuitem, radio) are left out: they need no
/// tabindex of their own, and flagging them would be crying wolf.
const STANDALONE_WIDGET_ROLES: &[&str] = &[
    "button",
    "link",
    "checkbox",
    "switch",
    "slider",
    "spinbutton",
    "textbox",
    "searchbox",
    "combobox",
    "tab",
];

/// Elements that are keyboard-reachable without help.
const NATIVELY_FOCUSABLE: &[&str] = &[
    "a", "button", "input", "select", "textarea", "summary", "iframe", "area",
];

/// Elements where an `onclick` is expected and keyboard support comes free.
const NATIVELY_CLICKABLE: &[&str] = &[
    "a", "button", "input", "select", "textarea", "summary", "label", "option", "details",
    "area", "body", "html", "form",
];

/// Attributes with no meaningful value, printed bare in snippets.
const BOOLEAN_ATTRIBUTES: &[&str] = &[
    "allowfullscreen",
    "async",
    "autofocus",
    "autoplay",
    "checked",
    "controls",
    "defer",
    "disabled",
    "hidden",
    "inert",
    "loop",
    "multiple",
    "muted",
    "novalidate",
    "open",
    "playsinline",
    "readonly",
    "required",
    "selected",
];

/// Elements with no closing tag.
const VOID_ELEMENTS: &[&str] = &[
    "area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "source",
    "track", "wbr",
];

/// The HTML autofill field names (WHATWG, "autofill detail tokens"), which is
/// the list WCAG 1.3.5 refers to.
const AUTOFILL_FIELDS: &[&str] = &[
    "name",
    "honorific-prefix",
    "given-name",
    "additional-name",
    "family-name",
    "honorific-suffix",
    "nickname",
    "username",
    "new-password",
    "current-password",
    "one-time-code",
    "organization-title",
    "organization",
    "street-address",
    "address-line1",
    "address-line2",
    "address-line3",
    "address-level4",
    "address-level3",
    "address-level2",
    "address-level1",
    "country",
    "country-name",
    "postal-code",
    "cc-name",
    "cc-given-name",
    "cc-additional-name",
    "cc-family-name",
    "cc-number",
    "cc-exp",
    "cc-exp-month",
    "cc-exp-year",
    "cc-csc",
    "cc-type",
    "transaction-currency",
    "transaction-amount",
    "language",
    "bday",
    "bday-day",
    "bday-month",
    "bday-year",
    "sex",
    "url",
    "photo",
    "email",
    "tel",
    "tel-country-code",
    "tel-national",
    "tel-area-code",
    "tel-local",
    "tel-extension",
    "impp",
];

/// Tokens that may precede the field name.
const AUTOFILL_PREFIXES: &[&str] = &[
    "shipping", "billing", "home", "work", "mobile", "fax", "pager",
];

/// Field names that almost always collect personal data, and so should carry
/// an `autocomplete` value for WCAG 1.3.5.
const PERSONAL_FIELD_HINTS: &[&str] = &[
    "email",
    "e-mail",
    "phone",
    "mobile",
    "firstname",
    "first_name",
    "first-name",
    "fname",
    "lastname",
    "last_name",
    "last-name",
    "lname",
    "surname",
    "fullname",
    "full_name",
    "full-name",
    "postcode",
    "postal",
    "zipcode",
    "street",
    "address",
    "username",
];

/// Video hosts whose players are embedded in an iframe.
const VIDEO_HOSTS: &[&str] = &[
    "youtube.com",
    "youtube-nocookie.com",
    "youtu.be",
    "vimeo.com",
    "wistia",
    "loom.com",
    "dailymotion.com",
    "vidyard",
    "brightcove",
];

/// Is this a role ARIA defines?
pub fn is_valid_aria_role(role: &str) -> bool {
    let role = role.trim();
    if role.is_empty() {
        return false;
    }
    // `role` takes a space-separated fallback list; the first understood one
    // wins, so the value is valid if any token is.
    role.split_whitespace().any(|token| {
        ARIA_ROLES
            .split(' ')
            .any(|known| known.eq_ignore_ascii_case(token))
    })
}

/// Is this a usable `autocomplete` value?
pub fn is_valid_autocomplete(value: &str) -> bool {
    let tokens: Vec<String> = value
        .split_whitespace()
        .map(str::to_ascii_lowercase)
        .collect();
    let mut tokens: &[String] = &tokens;
    match tokens {
        [] => return true,
        [only] if only == "on" || only == "off" => return true,
        _ => {}
    }
    if tokens.last().is_some_and(|last| last == "webauthn") {
        tokens = &tokens[..tokens.len() - 1];
    }
    let Some((field, prefixes)) = tokens.split_last() else {
        return false;
    };
    AUTOFILL_FIELDS.contains(&field.as_str())
        && prefixes.iter().all(|prefix| {
            prefix.starts_with("section-") || AUTOFILL_PREFIXES.contains(&prefix.as_str())
        })
}

/// Is this a plausible BCP 47 language tag? Only the primary language is
/// checked against the ISO list; anything after it is left alone, because
/// scripts and variants (`zh-Hant`, `sr-Latn-RS`) are valid and rare enough
/// that a wrong one does no harm a screen reader would notice.
pub fn is_valid_lang(value: &str) -> bool {
    let value = value.trim();
    if value.contains('_') || value.contains(' ') {
        return false;
    }
    let primary = value.split('-').next().unwrap_or_default();
    match primary.len() {
        2 => crate::audit::hreflang::is_known_language(primary),
        // ISO 639-2/3 codes: too many to list, and rarely mistyped.
        3 => primary.chars().all(|c| c.is_ascii_alphabetic()),
        _ => false,
    }
}

/// A short, readable rendering of an element for a report: its opening tag,
/// a clipped glimpse of its content, and its closing tag.
pub fn snippet(element: ElementRef<'_>) -> String {
    let name = element.value().name();
    let mut out = opening_tag(element);
    if VOID_ELEMENTS.contains(&name) {
        return out;
    }
    let text = collapse_ws(&element.text().collect::<String>());
    if !text.is_empty() {
        out.push_str(&clip(&text, 60));
    } else {
        let mut children = element.children().filter_map(ElementRef::wrap);
        if let Some(child) = children.next() {
            out.push_str(&opening_tag(child));
            if children.next().is_some() || !VOID_ELEMENTS.contains(&child.value().name()) {
                out.push('…');
            }
        }
    }
    out.push_str("</");
    out.push_str(name);
    out.push('>');
    clip(&out, 240)
}

/// The key that groups the same failure across pages built from one template:
/// the snippet with every run of digits masked, so `/post/12` and `/post/97`
/// count as one fix.
pub fn template_key(snippet: &str) -> String {
    static DIGITS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\d+").expect("valid regex"));
    DIGITS.replace_all(snippet, "#").into_owned()
}

fn opening_tag(element: ElementRef<'_>) -> String {
    let value = element.value();
    let mut out = format!("<{}", value.name());
    for (shown, (attribute, content)) in value.attrs().enumerate() {
        if shown == 6 {
            out.push_str(" …");
            break;
        }
        out.push(' ');
        out.push_str(attribute);
        if !(content.is_empty() && BOOLEAN_ATTRIBUTES.contains(&attribute)) {
            out.push_str("=\"");
            out.push_str(&clip(&collapse_ws(content), 48));
            out.push('"');
        }
    }
    out.push('>');
    out
}

fn clip(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let mut clipped: String = text.chars().take(max - 1).collect();
    clipped.push('…');
    clipped
}

fn collapse_ws(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Accumulates findings in document order.
#[derive(Default)]
struct Findings(Vec<Finding>);

impl Findings {
    fn add(&mut self, rule: &'static str, sample: impl FnOnce() -> String) {
        let index = match self.0.iter().position(|finding| finding.rule == rule) {
            Some(index) => index,
            None => {
                self.0.push(Finding {
                    rule,
                    count: 0,
                    samples: Vec::new(),
                });
                self.0.len() - 1
            }
        };
        let finding = &mut self.0[index];
        finding.count += 1;
        if finding.samples.len() < MAX_SAMPLES {
            let sample = sample();
            if !finding.samples.contains(&sample) {
                finding.samples.push(sample);
            }
        }
    }

    fn element(&mut self, rule: &'static str, element: ElementRef<'_>) {
        self.add(rule, || snippet(element));
    }

    fn note(&mut self, rule: &'static str, text: &str) {
        self.add(rule, || text.to_string());
    }
}

/// Inspect a parsed document.
pub fn analyse(document: &Html) -> AccessibilityReport {
    let mut report = AccessibilityReport::default();
    let mut found = Findings::default();

    let ids = collect_ids(document, &mut report, &mut found);

    check_links(document, &mut report, &mut found);
    check_buttons(document, &mut report, &mut found);
    check_fields(document, &mut report, &mut found);
    check_images(document, &mut found);

    for element in document.select(&SEL_ARIA_REF) {
        let broken = broken_references(element, &ids);
        report.broken_aria_references += broken;
        for _ in 0..broken {
            found.element(rule::BROKEN_ARIA_REF, element);
        }
    }

    check_roles(document, &mut report, &mut found);

    for element in document.select(&SEL_TABINDEX) {
        if tabindex_of(element).is_some_and(|index| index > 0) {
            report.positive_tabindex += 1;
            found.element(rule::POSITIVE_TABINDEX, element);
        }
    }

    for frame in document.select(&SEL_IFRAME) {
        let attrs = frame.value();
        let untitled = attrs.attr("title").is_none_or(|title| title.trim().is_empty())
            && !has_aria_name(frame)
            // An iframe hidden from the tree needs no title.
            && attrs.attr("aria-hidden") != Some("true");
        if untitled {
            report.iframes_without_title += 1;
            found.element(rule::IFRAME_NO_TITLE, frame);
        }
    }

    check_headings(document, &mut report, &mut found);
    check_page(document, &report, &mut found);
    check_media(document, &mut found);
    check_hidden_and_nested(document, &mut found);
    check_structure(document, &mut found);
    check_focus_styles(document, &mut found);
    collect_features(document, &mut report);

    report.findings = found.0;
    report
}

/// Every `id` on the page, recording duplicates as it goes.
///
/// A duplicate that something points at — a `<label for>`, an
/// `aria-labelledby` — is a real failure: the reference resolves to the first
/// element, so the second is left unnamed. One nothing points at is harmless
/// today and only a best practice, which is how WCAG 2.2 came to drop 4.1.1.
fn collect_ids(
    document: &Html,
    report: &mut AccessibilityReport,
    found: &mut Findings,
) -> HashSet<String> {
    let referenced = referenced_ids(document);
    let mut seen: HashMap<String, u32> = HashMap::new();
    for element in document.select(&SEL_WITH_ID) {
        let Some(id) = element
            .value()
            .attr("id")
            .map(str::trim)
            .filter(|id| !id.is_empty())
        else {
            continue;
        };
        let count = seen.entry(id.to_string()).or_insert(0);
        *count += 1;
        if *count == 2 {
            report.duplicate_ids += 1;
            let rule = if referenced.contains(id) {
                rule::DUPLICATE_ID_REFERENCED
            } else {
                rule::DUPLICATE_ID
            };
            found.add(rule, || opening_tag(element));
        }
    }
    seen.into_keys().collect()
}

/// Ids something on the page refers to.
fn referenced_ids(document: &Html) -> HashSet<String> {
    const REFERRING: &[&str] = &[
        "for",
        "aria-labelledby",
        "aria-describedby",
        "aria-controls",
        "aria-owns",
        "aria-activedescendant",
        "aria-errormessage",
        "aria-details",
        "headers",
    ];
    let mut ids = HashSet::new();
    for element in document.root_element().descendants().filter_map(ElementRef::wrap) {
        for attribute in REFERRING {
            if let Some(value) = element.value().attr(attribute) {
                ids.extend(value.split_whitespace().map(str::to_string));
            }
        }
    }
    ids
}

fn check_links(document: &Html, report: &mut AccessibilityReport, found: &mut Findings) {
    for link in document.select(&SEL_ANCHOR) {
        if !has_accessible_name(link) {
            report.links_without_name += 1;
            found.element(rule::LINK_NO_NAME, link);
            continue;
        }
        let text = collapse_ws(&link.text().collect::<String>());
        let explained = has_aria_name(link)
            || ["title", "aria-describedby"]
                .iter()
                .any(|attribute| link.value().attr(attribute).is_some_and(|v| !v.trim().is_empty()));
        if !explained && crate::ux::signals::is_vague(&text) {
            found.element(rule::VAGUE_LINK_TEXT, link);
        }
        check_label_in_name(link, &text, found);
    }
}

fn check_buttons(document: &Html, report: &mut AccessibilityReport, found: &mut Findings) {
    for button in document.select(&SEL_BUTTON) {
        // A link styled as a button has already been checked as a link.
        if button.value().name() == "a" && button.value().attr("href").is_some() {
            continue;
        }
        let native = matches!(button.value().name(), "button" | "input");
        if !has_accessible_name(button) {
            // The original counter covers native buttons only.
            if native {
                report.buttons_without_name += 1;
            }
            found.element(rule::BUTTON_NO_NAME, button);
            continue;
        }
        let text = collapse_ws(&button.text().collect::<String>());
        check_label_in_name(button, &text, found);
    }
}

/// WCAG 2.5.3: a visible label must be part of the accessible name, or a
/// voice-control user who says what they see gets nothing.
fn check_label_in_name(element: ElementRef<'_>, visible: &str, found: &mut Findings) {
    let Some(label) = element
        .value()
        .attr("aria-label")
        .filter(|label| !label.trim().is_empty())
    else {
        return;
    };
    let visible = comparable(visible);
    // Icon glyphs and single characters are not a "label" a user would speak.
    if visible.chars().filter(|c| c.is_alphanumeric()).count() < 2 {
        return;
    }
    if !comparable(label).contains(&visible) {
        found.element(rule::LABEL_NOT_IN_NAME, element);
    }
}

fn comparable(text: &str) -> String {
    let cleaned: String = text
        .chars()
        .map(|c| {
            if c.is_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                ' '
            }
        })
        .collect();
    collapse_ws(&cleaned)
}

fn check_fields(document: &Html, report: &mut AccessibilityReport, found: &mut Findings) {
    // `<label for=x>` targets, and fields wrapped in a label.
    let mut labelled_ids: HashSet<String> = HashSet::new();
    let mut wrapped = HashSet::new();

    for label in document.select(&SEL_LABEL) {
        if let Some(target) = label.value().attr("for").map(str::trim) {
            labelled_ids.insert(target.to_string());
        }
        // A field inside a label is named by it, with or without `for`.
        wrapped.extend(label.select(&SEL_INPUT).map(|field| field.id()));
    }

    for field in document.select(&SEL_INPUT) {
        let attrs = field.value();
        let input_type = attrs.attr("type").unwrap_or("text").to_ascii_lowercase();
        if UNLABELLED_INPUT_TYPES.contains(&input_type.as_str()) {
            continue;
        }
        if attrs.attr("aria-hidden") == Some("true") {
            continue;
        }

        let labelled = has_accessible_name(field)
            || wrapped.contains(&field.id())
            || attrs.attr("id").is_some_and(|id| labelled_ids.contains(id));
        if !labelled {
            report.inputs_without_label += 1;
            found.element(rule::FIELD_NO_LABEL, field);
        }

        match attrs.attr("autocomplete") {
            Some(value) if !is_valid_autocomplete(value) => {
                found.element(rule::INVALID_AUTOCOMPLETE, field);
            }
            Some(_) => {}
            None => {
                if collects_personal_data(field, &input_type) {
                    found.element(rule::MISSING_AUTOCOMPLETE, field);
                }
            }
        }
    }
}

/// Does this field look like it asks for the user's own details?
fn collects_personal_data(field: ElementRef<'_>, input_type: &str) -> bool {
    if field.value().name() != "input" {
        return false;
    }
    if matches!(input_type, "email" | "tel") {
        return true;
    }
    if !matches!(input_type, "text" | "") {
        return false;
    }
    ["name", "id"].iter().any(|attribute| {
        field.value().attr(attribute).is_some_and(|value| {
            let value = value.to_ascii_lowercase();
            value == "tel"
                || value == "name"
                || PERSONAL_FIELD_HINTS.iter().any(|hint| value.contains(hint))
        })
    })
}

fn check_images(document: &Html, found: &mut Findings) {
    for image in document.select(&SEL_IMAGE_LIKE) {
        let attrs = image.value();
        if attrs.attr("aria-hidden") == Some("true") || has_aria_name(image) {
            continue;
        }
        let non_empty = |name: &str| attrs.attr(name).is_some_and(|v| !v.trim().is_empty());
        let unnamed = match attrs.name() {
            // `alt=""` is a deliberate "decorative": only a missing alt fails.
            "img" => {
                attrs.attr("alt").is_none()
                    && !non_empty("title")
                    && !matches!(attrs.attr("role"), Some("presentation" | "none"))
            }
            "input" => !non_empty("alt") && !non_empty("value") && !non_empty("title"),
            "area" => !non_empty("alt"),
            // An svg with role=img is named by aria-label or its <title>.
            _ => image
                .select(&SEL_SVG_TITLE)
                .all(|title| title.text().collect::<String>().trim().is_empty()),
        };
        if unnamed {
            found.element(rule::IMG_NO_ALT, image);
        }
    }
}

fn check_roles(document: &Html, report: &mut AccessibilityReport, found: &mut Findings) {
    let mut invalid: Vec<String> = Vec::new();
    for element in document.select(&SEL_ROLE) {
        let Some(role) = element.value().attr("role").map(str::trim) else {
            continue;
        };
        if role.is_empty() {
            continue;
        }
        if !is_valid_aria_role(role) {
            invalid.push(role.to_string());
            found.element(rule::INVALID_ROLE, element);
            continue;
        }

        let primary = role
            .split_whitespace()
            .find(|token| is_valid_aria_role(token))
            .unwrap_or_default()
            .to_ascii_lowercase();
        let widget = STANDALONE_WIDGET_ROLES.contains(&primary.as_str());
        let attrs = element.value();
        let reachable = NATIVELY_FOCUSABLE.contains(&attrs.name())
            || attrs.attr("tabindex").is_some()
            || attrs.attr("contenteditable").is_some_and(|v| v != "false")
            || attrs.attr("aria-disabled") == Some("true")
            || attrs.attr("disabled").is_some();
        if widget && !reachable && !inside_aria_hidden(element) {
            found.element(rule::ROLE_NOT_FOCUSABLE, element);
        }
    }
    invalid.sort();
    invalid.dedup();
    report.invalid_roles = invalid;

    for element in document.select(&SEL_ONCLICK) {
        let attrs = element.value();
        if NATIVELY_CLICKABLE.contains(&attrs.name())
            || attrs.attr("role").is_some()
            || attrs.attr("tabindex").is_some()
        {
            continue;
        }
        found.element(rule::CLICK_HANDLER_NOT_FOCUSABLE, element);
    }
}

fn check_headings(document: &Html, report: &mut AccessibilityReport, found: &mut Findings) {
    let mut previous: Option<u8> = None;
    for heading in document.select(&SEL_HEADING) {
        let Some(level) = heading
            .value()
            .name()
            .as_bytes()
            .get(1)
            .map(|byte| byte.wrapping_sub(b'0'))
            .filter(|level| (1..=6).contains(level))
        else {
            continue;
        };
        report.heading_outline.push(level);
        // Going back *up* any number of levels is fine — a new section at
        // `h2` after an `h4` is normal. Only descending by more than one is a
        // failure.
        if previous.is_some_and(|before| level > before + 1) {
            report.skipped_heading_levels += 1;
            found.element(rule::HEADING_SKIP, heading);
        }
        previous = Some(level);

        if heading.value().attr("aria-hidden") != Some("true") && !has_accessible_name(heading) {
            found.element(rule::EMPTY_HEADING, heading);
        }
    }
}

/// Checks that are about the page as a whole.
fn check_page(document: &Html, report: &AccessibilityReport, found: &mut Findings) {
    let html = document.select(&SEL_HTML).next();
    match html.and_then(|element| element.value().attr("lang")) {
        Some(lang) if !lang.trim().is_empty() => {
            if !is_valid_lang(lang) {
                found.add(rule::LANG_INVALID, || {
                    html.map(opening_tag).unwrap_or_default()
                });
            }
        }
        _ => found.add(rule::PAGE_NO_LANG, || {
            html.map(opening_tag).unwrap_or_else(|| "<html>".into())
        }),
    }
    // Language of parts: an empty `lang=""` means "unknown" and is allowed.
    for element in document.select(&SEL_LANG) {
        if element.value().name() == "html" {
            continue;
        }
        let lang = element.value().attr("lang").unwrap_or_default();
        if !lang.trim().is_empty() && !is_valid_lang(lang) {
            found.add(rule::LANG_INVALID, || opening_tag(element));
        }
    }

    let titled = document
        .select(&SEL_HEAD_TITLE)
        .next()
        .is_some_and(|title| !title.text().collect::<String>().trim().is_empty());
    if !titled {
        found.note(rule::PAGE_NO_TITLE, "<title></title>");
    }

    for meta in document.select(&SEL_META) {
        let attrs = meta.value();
        let content = attrs.attr("content").unwrap_or_default();
        if attrs
            .attr("name")
            .is_some_and(|name| name.eq_ignore_ascii_case("viewport"))
            && viewport_blocks_zoom(content)
        {
            found.element(rule::ZOOM_DISABLED, meta);
        }
        if attrs
            .attr("http-equiv")
            .is_some_and(|value| value.eq_ignore_ascii_case("refresh"))
        {
            // A delay of 0 is an instant redirect, which is fine; anything
            // under twenty hours moves the page out from under the reader.
            let delay = content
                .split([';', ','])
                .next()
                .and_then(|value| value.trim().parse::<f64>().ok());
            if delay.is_some_and(|delay| delay > 0.0 && delay < 72_000.0) {
                found.element(rule::META_REFRESH, meta);
            }
        }
    }

    let has_main = document.select(&SEL_MAIN).next().is_some();
    let has_skip_link = document.select(&SEL_ANCHOR).take(5).any(|link| {
        link.value()
            .attr("href")
            .is_some_and(|href| href.starts_with('#') && href.len() > 1)
    });
    if !has_main && !has_skip_link && report.heading_outline.is_empty() {
        found.note(
            rule::NO_BYPASS,
            "No <main> landmark, skip link or heading on the page",
        );
    }
}

/// Does a viewport `content` value stop the reader zooming to 200%?
fn viewport_blocks_zoom(content: &str) -> bool {
    content.split([',', ';']).any(|pair| {
        let mut parts = pair.splitn(2, '=');
        let key = parts.next().unwrap_or_default().trim().to_ascii_lowercase();
        let value = parts.next().unwrap_or_default().trim().to_ascii_lowercase();
        match key.as_str() {
            "user-scalable" => matches!(value.as_str(), "no" | "0"),
            "maximum-scale" => value.parse::<f32>().is_ok_and(|scale| scale < 2.0),
            _ => false,
        }
    })
}

fn check_media(document: &Html, found: &mut Findings) {
    for element in document.select(&SEL_OLD_MOTION) {
        found.element(rule::BLINK_MARQUEE, element);
    }

    for media in document.select(&SEL_MEDIA) {
        let attrs = media.value();
        let muted = attrs.attr("muted").is_some();
        if attrs.attr("autoplay").is_some() && !muted {
            found.element(rule::AUTOPLAY_AUDIO, media);
        }
        // A muted video is almost always decorative background with no
        // soundtrack to caption.
        if attrs.name() == "video" && !muted {
            let captioned = media.select(&SEL_TRACK).any(|track| {
                track
                    .value()
                    .attr("kind")
                    .is_some_and(|kind| matches!(kind, "captions" | "subtitles"))
            });
            if !captioned {
                found.element(rule::VIDEO_NO_CAPTIONS, media);
            }
        }
    }
}

/// Focusable things hidden from assistive technology, and interactive things
/// inside each other.
fn check_hidden_and_nested(document: &Html, found: &mut Findings) {
    for hidden in document.select(&SEL_ARIA_HIDDEN) {
        // Report the outermost hidden region only.
        if hidden
            .ancestors()
            .filter_map(ElementRef::wrap)
            .any(|ancestor| ancestor.value().attr("aria-hidden") == Some("true"))
        {
            continue;
        }
        let traps_focus = is_focusable(hidden)
            || hidden.select(&SEL_MAYBE_FOCUSABLE).any(is_focusable);
        if traps_focus {
            found.element(rule::HIDDEN_FOCUSABLE, hidden);
        }
    }

    for host in document.select(&SEL_INTERACTIVE_HOST) {
        let nested = host
            .select(&SEL_MAYBE_FOCUSABLE)
            .any(|inner| inner.id() != host.id() && is_focusable(inner));
        if nested {
            found.element(rule::NESTED_INTERACTIVE, host);
        }
    }
}

/// Can a keyboard user land on this element?
fn is_focusable(element: ElementRef<'_>) -> bool {
    let attrs = element.value();
    if let Some(index) = tabindex_of(element) {
        return index >= 0;
    }
    if attrs.attr("disabled").is_some() {
        return false;
    }
    match attrs.name() {
        "a" | "area" => attrs.attr("href").is_some(),
        "input" => attrs.attr("type") != Some("hidden"),
        "button" | "select" | "textarea" | "summary" | "iframe" => true,
        _ => attrs
            .attr("contenteditable")
            .is_some_and(|value| value != "false"),
    }
}

fn tabindex_of(element: ElementRef<'_>) -> Option<i32> {
    element
        .value()
        .attr("tabindex")
        .and_then(|value| value.trim().parse::<i32>().ok())
}

fn inside_aria_hidden(element: ElementRef<'_>) -> bool {
    std::iter::once(element)
        .chain(element.ancestors().filter_map(ElementRef::wrap))
        .any(|node| node.value().attr("aria-hidden") == Some("true"))
}

/// Lists and tables whose structure is not what it looks like.
fn check_structure(document: &Html, found: &mut Findings) {
    for list in document.select(&SEL_LIST) {
        if list.value().attr("role").is_some() {
            continue;
        }
        let malformed = list
            .children()
            .filter_map(ElementRef::wrap)
            .any(|child| !matches!(child.value().name(), "li" | "script" | "template"));
        if malformed {
            found.element(rule::LIST_STRUCTURE, list);
        }
    }
    for item in document.select(&SEL_LIST_ITEM) {
        let in_list = item.parent().and_then(ElementRef::wrap).is_some_and(|parent| {
            matches!(parent.value().name(), "ul" | "ol" | "menu")
                || parent.value().attr("role") == Some("list")
        });
        if !in_list && item.value().attr("role").is_none() {
            found.element(rule::LIST_STRUCTURE, item);
        }
    }

    for table in document.select(&SEL_TABLE) {
        if matches!(table.value().attr("role"), Some("presentation" | "none")) {
            continue;
        }
        let rows: Vec<ElementRef<'_>> = table.select(&SEL_ROW).collect();
        let widest = rows
            .iter()
            .map(|row| row.select(&SEL_CELL).count())
            .max()
            .unwrap_or(0);
        if rows.len() >= 2 && widest >= 2 && table.select(&SEL_TABLE_HEADER).next().is_none() {
            found.add(rule::TABLE_NO_HEADERS, || opening_tag(table));
        }
    }
}

/// Stylesheets on the page that take the focus ring away.
///
/// Only inline `<style>` blocks and `style` attributes are visible to a
/// crawler; linked stylesheets are not fetched, so this can miss the problem
/// but never invents it.
fn check_focus_styles(document: &Html, found: &mut Findings) {
    static FOCUS_RULE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?s)([^{}]*:focus[^{}]*)\{([^{}]*)\}").expect("valid regex")
    });
    static NO_OUTLINE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)outline(-style)?\s*:\s*(none|0)\b|outline-width\s*:\s*0").expect("valid regex")
    });
    static REPLACEMENT: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?i)box-shadow|border|background|text-decoration").expect("valid regex"));

    for style in document.select(&SEL_STYLE) {
        let css = style.text().collect::<String>();
        for rule_match in FOCUS_RULE.captures_iter(&css) {
            let selector = collapse_ws(&rule_match[1]);
            let body = &rule_match[2];
            // `:focus:not(:focus-visible) { outline: none }` is the correct
            // pattern, and a rule that draws something else instead is fine.
            if selector.contains("focus-visible")
                || !NO_OUTLINE.is_match(body)
                || REPLACEMENT.is_match(body)
            {
                continue;
            }
            found.add(rule::FOCUS_OUTLINE_REMOVED, || {
                clip(&format!("{selector} {{ {} }}", collapse_ws(body)), 160)
            });
        }
    }

    for element in document.select(&SEL_STYLED) {
        let style = element.value().attr("style").unwrap_or_default();
        if NO_OUTLINE.is_match(style) && is_focusable(element) {
            found.element(rule::FOCUS_OUTLINE_REMOVED, element);
        }
    }
}

fn collect_features(document: &Html, report: &mut AccessibilityReport) {
    let embeds_video = document.select(&SEL_IFRAME).any(|frame| {
        frame.value().attr("src").is_some_and(|src| {
            let src = src.to_ascii_lowercase();
            VIDEO_HOSTS.iter().any(|host| src.contains(host))
        })
    });
    let has_media = |name: &str| document.select(&SEL_MEDIA).any(|m| m.value().name() == name);
    let has_video = embeds_video || has_media("video");
    let has_audio = has_media("audio");
    let has_form = document.select(&SEL_FORM).next().is_some()
        || document.select(&SEL_INPUT).any(|field| {
            let input_type = field.value().attr("type").unwrap_or("text").to_ascii_lowercase();
            !UNLABELLED_INPUT_TYPES.contains(&input_type.as_str())
        });
    let has_password = document.select(&SEL_PASSWORD).next().is_some();

    for (present, name) in [
        (has_video, feature::VIDEO),
        (has_audio, feature::AUDIO),
        (has_form, feature::FORM),
        (has_password, feature::PASSWORD),
    ] {
        if present {
            report.features.push(name);
        }
    }
}

/// Is the element named by an ARIA attribute?
fn has_aria_name(element: ElementRef<'_>) -> bool {
    ["aria-label", "aria-labelledby"].iter().any(|attribute| {
        element
            .value()
            .attr(attribute)
            .is_some_and(|value| !value.trim().is_empty())
    })
}

/// Would a screen reader have anything to announce for this element?
///
/// Follows the order the accessible-name computation uses, simplified: an
/// explicit ARIA label wins, then the element's own text, then the alt text of
/// an image inside it, then a title attribute.
fn has_accessible_name(element: ElementRef<'_>) -> bool {
    let attrs = element.value();

    // Hidden from the accessibility tree entirely: not a failure.
    if attrs.attr("aria-hidden") == Some("true") {
        return true;
    }

    for attribute in ["aria-label", "aria-labelledby", "title"] {
        if attrs
            .attr(attribute)
            .is_some_and(|value| !value.trim().is_empty())
        {
            return true;
        }
    }

    // `<input type=submit value="Send">` is named by its value.
    if attrs
        .attr("value")
        .is_some_and(|value| !value.trim().is_empty())
    {
        return true;
    }

    if !element.text().collect::<String>().trim().is_empty() {
        return true;
    }

    // An image-only link is named by the image's alt text.
    element.select(&SEL_IMG).any(|image| {
        image
            .value()
            .attr("alt")
            .is_some_and(|alt| !alt.trim().is_empty())
    })
}

/// `aria-labelledby`/`aria-describedby` tokens with no matching id.
fn broken_references(element: ElementRef<'_>, ids: &HashSet<String>) -> u32 {
    let mut broken = 0;
    for attribute in ["aria-labelledby", "aria-describedby"] {
        let Some(value) = element.value().attr(attribute) else {
            continue;
        };
        for token in value.split_whitespace() {
            if !ids.contains(token) {
                broken += 1;
            }
        }
    }
    broken
}

#[cfg(test)]
mod tests;
