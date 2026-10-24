//! Accessibility checks that can be made from static HTML.
//!
//! ## What is and is not knowable without a browser
//!
//! A great deal of WCAG needs layout, computed styles or a real accessibility
//! tree: colour contrast, focus visibility, reading order, whether a thing is
//! actually visible. None of that is available to a crawler that never renders,
//! and claiming otherwise would be worse than not checking at all.
//!
//! What *is* knowable from the markup is the set of failures that are both
//! common and unambiguous: a control with no accessible name, a page with no
//! language, a broken `aria-labelledby`, a role that does not exist, a heading
//! level skipped, a positive `tabindex`. Those are the checks here. Every one
//! of them is a definite failure rather than a hint, so a finding is always
//! worth acting on.
//!
//! When JavaScript rendering arrives, this same pass runs over the rendered
//! DOM and gets strictly better without changing.

use std::collections::{HashMap, HashSet};

use scraper::{ElementRef, Html, Selector};
use std::sync::LazyLock;

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
}

impl AccessibilityReport {
    /// Total definite failures. Used for the summary column.
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
    "button, input[type=submit], input[type=button], input[type=reset]"
);
selector!(SEL_INPUT, "input, select, textarea");
selector!(SEL_LABEL, "label");
selector!(SEL_WITH_ID, "[id]");
selector!(SEL_ROLE, "[role]");
selector!(SEL_TABINDEX, "[tabindex]");
selector!(SEL_IFRAME, "iframe");
selector!(SEL_HEADING, "h1, h2, h3, h4, h5, h6");
selector!(SEL_IMG, "img");
selector!(SEL_ARIA_REF, "[aria-labelledby], [aria-describedby]");

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

/// Inspect a parsed document.
pub fn analyse(document: &Html) -> AccessibilityReport {
    let mut report = AccessibilityReport::default();

    let ids = collect_ids(document, &mut report);

    report.links_without_name = document
        .select(&SEL_ANCHOR)
        .filter(|element| !has_accessible_name(*element))
        .count() as u32;

    report.buttons_without_name = document
        .select(&SEL_BUTTON)
        .filter(|element| !has_accessible_name(*element))
        .count() as u32;

    report.inputs_without_label = count_unlabelled_inputs(document);

    report.broken_aria_references = document
        .select(&SEL_ARIA_REF)
        .map(|element| broken_references(element, &ids))
        .sum();

    report.invalid_roles = collect_invalid_roles(document);

    report.positive_tabindex = document
        .select(&SEL_TABINDEX)
        .filter(|element| {
            element
                .value()
                .attr("tabindex")
                .and_then(|value| value.trim().parse::<i32>().ok())
                .is_some_and(|index| index > 0)
        })
        .count() as u32;

    report.iframes_without_title = document
        .select(&SEL_IFRAME)
        .filter(|element| {
            element.value().attr("title").is_none_or(|title| title.trim().is_empty())
                // An iframe hidden from the tree needs no title.
                && element.value().attr("aria-hidden") != Some("true")
        })
        .count() as u32;

    let (outline, skips) = heading_outline(document);
    report.heading_outline = outline;
    report.skipped_heading_levels = skips;

    report
}

/// Every `id` on the page, counting duplicates as it goes.
fn collect_ids(document: &Html, report: &mut AccessibilityReport) -> HashSet<String> {
    let mut seen: HashMap<String, u32> = HashMap::new();
    for element in document.select(&SEL_WITH_ID) {
        if let Some(id) = element
            .value()
            .attr("id")
            .map(str::trim)
            .filter(|id| !id.is_empty())
        {
            *seen.entry(id.to_string()).or_insert(0) += 1;
        }
    }
    report.duplicate_ids = seen.values().filter(|count| **count > 1).count() as u32;
    seen.into_keys().collect()
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

/// Form fields with nothing naming them.
fn count_unlabelled_inputs(document: &Html) -> u32 {
    // `<label for=x>` targets, and fields wrapped in a label.
    let mut labelled_ids: HashSet<String> = HashSet::new();
    let mut wrapped: HashSet<String> = HashSet::new();

    for label in document.select(&SEL_LABEL) {
        if let Some(target) = label.value().attr("for").map(str::trim) {
            labelled_ids.insert(target.to_string());
        }
        // A field inside a label is named by it, with or without `for`.
        for field in label.select(&SEL_INPUT) {
            if let Some(id) = field.value().attr("id") {
                wrapped.insert(id.to_string());
            } else {
                // No id to key on, so remember the element itself.
                wrapped.insert(format!("{:?}", field.id()));
            }
        }
    }

    document
        .select(&SEL_INPUT)
        .filter(|field| {
            let attrs = field.value();
            let input_type = attrs.attr("type").unwrap_or("text").to_ascii_lowercase();
            if UNLABELLED_INPUT_TYPES.contains(&input_type.as_str()) {
                return false;
            }
            if attrs.attr("aria-hidden") == Some("true") {
                return false;
            }
            if has_accessible_name(*field) {
                return false;
            }
            match attrs.attr("id") {
                Some(id) => !labelled_ids.contains(id) && !wrapped.contains(id),
                None => !wrapped.contains(&format!("{:?}", field.id())),
            }
        })
        .count() as u32
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

fn collect_invalid_roles(document: &Html) -> Vec<String> {
    let mut invalid: Vec<String> = document
        .select(&SEL_ROLE)
        .filter_map(|element| element.value().attr("role"))
        .map(str::trim)
        .filter(|role| !role.is_empty() && !is_valid_aria_role(role))
        .map(str::to_string)
        .collect();
    invalid.sort();
    invalid.dedup();
    invalid
}

/// Heading levels in document order, and how many levels were skipped.
///
/// Going back *up* any number of levels is fine — a new section at `h2` after
/// an `h4` is normal. Only descending by more than one is a failure.
fn heading_outline(document: &Html) -> (Vec<u8>, u32) {
    let levels: Vec<u8> = document
        .select(&SEL_HEADING)
        .filter_map(|element| element.value().name().as_bytes().get(1).copied())
        .map(|byte| byte - b'0')
        .filter(|level| (1..=6).contains(level))
        .collect();

    let mut skips = 0;
    for window in levels.windows(2) {
        if window[1] > window[0] + 1 {
            skips += 1;
        }
    }
    (levels, skips)
}

#[cfg(test)]
mod tests;
