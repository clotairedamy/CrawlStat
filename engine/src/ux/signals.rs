//! Reading a page's structure as a sequence of things a person can attend to.
//!
//! Every node here comes out of the markup in document order, which is not an
//! arbitrary choice: document order *is* the order a screen reader speaks, the
//! order the Tab key moves, and — for any layout that is not fighting its own
//! source — roughly the order an eye travels. It is the one ordering that is
//! genuinely knowable without rendering the page.

use scraper::{ElementRef, Html, Selector};

/// What kind of thing this is, as far as attention is concerned.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeKind {
    /// `h1`–`h6`, carrying its level.
    Heading(u8),
    /// A link that goes somewhere else.
    Link,
    /// A button, a submit input, or a link styled as an action.
    Action,
    /// A form control the user has to fill in.
    Field,
    /// An image large enough to be worth looking at.
    Image,
    /// A run of body copy.
    Text,
}

/// The part of the page a node sits in.
///
/// Taken from HTML landmarks and ARIA roles. This is the closest thing to
/// "where on the screen" that static markup can honestly give: a node inside
/// `<footer>` is below the fold on essentially every page ever built, and one
/// inside `<nav>` is chrome rather than content, whatever the CSS does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Region {
    Header,
    Nav,
    Main,
    Aside,
    Footer,
    /// Not inside any landmark. Treated as main content, because on a page
    /// with no landmarks at all that is what it usually is.
    Unknown,
}

impl Region {
    /// How much weight the region itself lends a node.
    ///
    /// Main content outranks navigation; the footer is where attention goes
    /// to die. These are coarse on purpose — the region sets a floor and a
    /// ceiling, and the node's own properties decide the rest.
    pub fn weight(self) -> f32 {
        match self {
            Region::Main | Region::Unknown => 1.0,
            Region::Header => 0.7,
            Region::Nav => 0.55,
            Region::Aside => 0.4,
            Region::Footer => 0.2,
        }
    }
}

/// One thing on the page a person could look at or act on.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct UxNode {
    /// Position in document order, from zero.
    pub order: usize,
    pub kind: NodeKind,
    pub region: Region,
    /// The visible text, trimmed and collapsed. Empty for an unlabelled image.
    pub label: String,
    /// Words of visible text.
    pub words: u32,
    /// Where a link or form goes, if anywhere.
    pub target: Option<String>,
    /// How deeply nested the element is. Deeply buried things are, on average,
    /// less prominent — a rough stand-in for visual hierarchy.
    pub nesting: u16,
    /// True when the markup says this is the page's primary action: a submit
    /// button, or an action whose text reads like a commitment.
    pub primary: bool,
}

impl UxNode {
    /// Whether this is something you *do* rather than something you read.
    pub fn is_interactive(&self) -> bool {
        matches!(self.kind, NodeKind::Link | NodeKind::Action | NodeKind::Field)
    }
}

macro_rules! selector {
    ($sel:expr) => {{
        static CELL: std::sync::OnceLock<Selector> = std::sync::OnceLock::new();
        CELL.get_or_init(|| Selector::parse($sel).expect("built-in selector is valid"))
    }};
}

/// Elements that count as a node, in one pass so document order is preserved.
const INTERESTING: &str = "h1, h2, h3, h4, h5, h6, a[href], button, \
     input[type=submit], input[type=button], input:not([type=hidden]), \
     select, textarea, img, p, li";

/// Words that mark an action as the page's main one.
///
/// Deliberately a short list of commitments rather than a thesaurus of verbs:
/// "Learn more" is a link, "Start free trial" is the thing the page exists
/// for, and the difference matters when deciding what competes with what.
const COMMITMENT_WORDS: &[&str] = &[
    "buy", "purchase", "checkout", "subscribe", "sign up", "signup", "register",
    "get started", "start free", "free trial", "try free", "download",
    "book", "request a demo", "request demo", "contact sales", "add to cart",
    "add to basket", "join", "create account", "apply now",
];

/// Text that tells the reader nothing about where it goes.
///
/// A page whose links are mostly these is harder to scan, and the attention
/// model marks them down for it.
pub const VAGUE_LABELS: &[&str] = &[
    "click here", "here", "read more", "more", "learn more", "this", "link",
    "continue", "go", "details", "find out more",
];

fn text_of(element: ElementRef<'_>) -> String {
    let raw: String = element.text().collect::<Vec<_>>().join(" ");
    raw.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn region_of(element: ElementRef<'_>) -> Region {
    // Walk up to the nearest landmark. The first one wins, so a nav inside a
    // footer is footer-weighted, which is right: it is still down there.
    let mut current = element.parent();
    while let Some(node) = current {
        if let Some(parent) = ElementRef::wrap(node) {
            let name = parent.value().name();
            let role = parent.value().attr("role").unwrap_or_default();
            let region = match (name, role) {
                ("header", _) | (_, "banner") => Some(Region::Header),
                ("nav", _) | (_, "navigation") => Some(Region::Nav),
                ("main", _) | (_, "main") => Some(Region::Main),
                ("aside", _) | (_, "complementary") => Some(Region::Aside),
                ("footer", _) | (_, "contentinfo") => Some(Region::Footer),
                _ => None,
            };
            if let Some(region) = region {
                return region;
            }
        }
        current = node.parent();
    }
    Region::Unknown
}

fn nesting_of(element: ElementRef<'_>) -> u16 {
    let mut depth = 0u16;
    let mut current = element.parent();
    while let Some(node) = current {
        if ElementRef::wrap(node).is_some() {
            depth = depth.saturating_add(1);
        }
        current = node.parent();
    }
    depth
}

fn looks_like_commitment(label: &str) -> bool {
    let lower = label.to_ascii_lowercase();
    COMMITMENT_WORDS.iter().any(|word| lower.contains(word))
}

/// Whether a label is one of the uninformative ones.
pub fn is_vague(label: &str) -> bool {
    let lower = label.trim().trim_end_matches(['.', '!', '›', '→', '»']).trim().to_ascii_lowercase();
    VAGUE_LABELS.iter().any(|vague| lower == *vague)
}

/// Pull the attention-relevant nodes out of a document, in document order.
pub fn extract(document: &Html) -> Vec<UxNode> {
    let mut nodes = Vec::new();

    for element in document.select(selector!(INTERESTING)) {
        // Anything explicitly hidden from assistive technology is hidden from
        // this model too — it is not part of the experience being described.
        if element.value().attr("aria-hidden") == Some("true")
            || element.value().attr("hidden").is_some()
        {
            continue;
        }

        let name = element.value().name();
        let label = match name {
            "img" => element
                .value()
                .attr("alt")
                .unwrap_or_default()
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" "),
            "input" | "select" | "textarea" => element
                .value()
                .attr("aria-label")
                .or_else(|| element.value().attr("placeholder"))
                .or_else(|| element.value().attr("value"))
                .unwrap_or_default()
                .trim()
                .to_string(),
            _ => text_of(element),
        };

        let kind = match name {
            "h1" => NodeKind::Heading(1),
            "h2" => NodeKind::Heading(2),
            "h3" => NodeKind::Heading(3),
            "h4" => NodeKind::Heading(4),
            "h5" => NodeKind::Heading(5),
            "h6" => NodeKind::Heading(6),
            "img" => NodeKind::Image,
            "button" => NodeKind::Action,
            "select" | "textarea" => NodeKind::Field,
            "input" => match element.value().attr("type").unwrap_or("text") {
                "submit" | "button" | "image" => NodeKind::Action,
                _ => NodeKind::Field,
            },
            "a" => {
                // A link whose text is a commitment is an action, not
                // navigation. Most "Buy now" buttons on the web are anchors.
                if looks_like_commitment(&label) {
                    NodeKind::Action
                } else {
                    NodeKind::Link
                }
            }
            _ => NodeKind::Text,
        };

        // Body copy only counts once it is long enough to be a block someone
        // reads rather than a stray word between elements.
        let words = label.split_whitespace().count() as u32;
        if kind == NodeKind::Text && words < 4 {
            continue;
        }
        // An image with no alt text is invisible to a screen reader and
        // usually decorative; it contributes nothing directional.
        if kind == NodeKind::Image && label.is_empty() {
            continue;
        }

        let primary = match kind {
            NodeKind::Action => {
                looks_like_commitment(&label)
                    || element.value().attr("type") == Some("submit")
            }
            _ => false,
        };

        let target = match name {
            "a" => element.value().attr("href").map(str::to_string),
            "form" => element.value().attr("action").map(str::to_string),
            _ => None,
        };

        nodes.push(UxNode {
            order: nodes.len(),
            kind,
            region: region_of(element),
            label,
            words,
            target,
            nesting: nesting_of(element),
            primary,
        });
    }

    nodes
}
