//! Who is looking at the page.
//!
//! The same markup is read differently depending on why someone arrived. A
//! person who came to do one specific thing skims past everything that is not
//! that thing; a person browsing reads headings and follows links; a cautious
//! buyer hunts for prices, guarantees and anything that looks like a catch.
//!
//! These are **archetypes, not users**. They are a way of asking "if someone
//! arrived wanting X, what would this markup lead them to?" and the answer is
//! a hypothesis to check, not a measurement. Nothing here observes anybody.

use super::signals::{NodeKind, Region, UxNode};

/// A way of reading a page.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Persona {
    /// Arrived to do one thing. Skips prose, hunts for the control that does
    /// it, and abandons quickly when the path is not obvious.
    TaskOriented,
    /// Reading to find out what this is. Headings and links carry them.
    Explorer,
    /// Wants to be convinced. Weights prices, proof and detail; treats a
    /// forest of competing calls to action as a reason to leave.
    Skeptical,
    /// On a phone, in a hurry. Almost everything past the first screenful of
    /// markup may as well not exist.
    MobileRushed,
}

impl Persona {
    pub const ALL: [Persona; 4] = [
        Persona::TaskOriented,
        Persona::Explorer,
        Persona::Skeptical,
        Persona::MobileRushed,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Persona::TaskOriented => "task_oriented",
            Persona::Explorer => "explorer",
            Persona::Skeptical => "skeptical",
            Persona::MobileRushed => "mobile_rushed",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Persona::TaskOriented => "Task-oriented",
            Persona::Explorer => "Explorer",
            Persona::Skeptical => "Skeptical",
            Persona::MobileRushed => "Mobile, in a hurry",
        }
    }

    pub fn parse(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|p| p.as_str() == name)
    }

    /// How much this persona cares about a kind of element.
    pub fn affinity(self, kind: NodeKind) -> f32 {
        match (self, kind) {
            // Looking for the control that does the job.
            (Persona::TaskOriented, NodeKind::Action) => 1.0,
            (Persona::TaskOriented, NodeKind::Field) => 0.85,
            (Persona::TaskOriented, NodeKind::Heading(1 | 2)) => 0.6,
            (Persona::TaskOriented, NodeKind::Link) => 0.45,
            (Persona::TaskOriented, NodeKind::Text) => 0.15,
            (Persona::TaskOriented, _) => 0.2,

            // Reading the shape of the place.
            (Persona::Explorer, NodeKind::Heading(level)) => 1.0 - f32::from(level - 1) * 0.12,
            (Persona::Explorer, NodeKind::Link) => 0.8,
            (Persona::Explorer, NodeKind::Image) => 0.5,
            (Persona::Explorer, NodeKind::Text) => 0.45,
            (Persona::Explorer, NodeKind::Action) => 0.5,
            (Persona::Explorer, NodeKind::Field) => 0.25,

            // Wants the detail and the catch.
            (Persona::Skeptical, NodeKind::Text) => 0.8,
            (Persona::Skeptical, NodeKind::Heading(level)) => 0.85 - f32::from(level - 1) * 0.08,
            (Persona::Skeptical, NodeKind::Link) => 0.6,
            (Persona::Skeptical, NodeKind::Action) => 0.55,
            (Persona::Skeptical, NodeKind::Field) => 0.3,
            (Persona::Skeptical, NodeKind::Image) => 0.3,

            // Thumb already moving.
            (Persona::MobileRushed, NodeKind::Action) => 1.0,
            (Persona::MobileRushed, NodeKind::Heading(1)) => 0.9,
            (Persona::MobileRushed, NodeKind::Heading(_)) => 0.35,
            (Persona::MobileRushed, NodeKind::Link) => 0.4,
            (Persona::MobileRushed, NodeKind::Field) => 0.5,
            (Persona::MobileRushed, NodeKind::Image) => 0.3,
            (Persona::MobileRushed, NodeKind::Text) => 0.1,
        }
    }

    /// How fast attention falls off as you go down the document.
    ///
    /// Larger means a shorter attention span. The rushed reader's drops away
    /// fastest; the sceptic's barely drops at all, because they are the one
    /// person who actually reads to the bottom.
    pub fn decay(self) -> f32 {
        match self {
            Persona::TaskOriented => 0.9,
            Persona::Explorer => 0.45,
            Persona::Skeptical => 0.25,
            Persona::MobileRushed => 1.6,
        }
    }

    /// Extra weight for the region a node sits in.
    ///
    /// Multiplied with [`Region::weight`], so this expresses only the
    /// *difference* between personas — an explorer treats navigation as
    /// content, which nobody else does.
    pub fn region_bias(self, region: Region) -> f32 {
        match (self, region) {
            (Persona::Explorer, Region::Nav) => 1.6,
            (Persona::Explorer, Region::Footer) => 1.4,
            (Persona::TaskOriented, Region::Nav) => 1.2,
            (Persona::MobileRushed, Region::Footer) => 0.4,
            (Persona::MobileRushed, Region::Aside) => 0.5,
            (Persona::Skeptical, Region::Footer) => 1.5,
            _ => 1.0,
        }
    }

    /// How much of the page this persona is assumed to get through before
    /// deciding, as a share of the nodes.
    ///
    /// Used to work out whether the thing they came for is even reachable
    /// within their patience.
    pub fn patience(self) -> f32 {
        match self {
            Persona::TaskOriented => 0.35,
            Persona::Explorer => 0.8,
            Persona::Skeptical => 1.0,
            Persona::MobileRushed => 0.2,
        }
    }

    /// A one-line description of what this persona is, for the UI.
    pub fn describes(self) -> &'static str {
        match self {
            Persona::TaskOriented => {
                "Came to do one specific thing. Skims past prose looking for the control."
            }
            Persona::Explorer => {
                "Finding out what this is. Reads headings, follows links."
            }
            Persona::Skeptical => {
                "Wants convincing. Reads the detail, looks for the catch."
            }
            Persona::MobileRushed => {
                "On a phone, in a hurry. Barely past the first screenful."
            }
        }
    }
}

/// How well a node matches what the persona is hunting for, 0..1.
///
/// Separate from [`Persona::affinity`], which is about the *kind* of element.
/// This is about the words in it.
pub fn intent_match(persona: Persona, node: &UxNode) -> f32 {
    let label = node.label.to_ascii_lowercase();
    let contains_any = |words: &[&str]| words.iter().any(|w| label.contains(w));

    match persona {
        Persona::TaskOriented if node.primary => 1.0,
        Persona::Skeptical
            if contains_any(&[
                "price", "pricing", "$", "free", "refund", "guarantee", "cancel",
                "privacy", "terms", "security", "review",
            ]) =>
        {
            1.0
        }
        Persona::Explorer
            if contains_any(&["how", "what", "why", "about", "docs", "guide", "example"]) =>
        {
            0.85
        }
        Persona::MobileRushed if node.primary => 1.0,
        _ => 0.0,
    }
}
