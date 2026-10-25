//! Directional UX intelligence: where a page leads, and whether it leads
//! anywhere.
//!
//! Given a page's markup and a persona, this predicts the order someone is
//! likely to attend to things in, how much attention each thing gets, and what
//! goes wrong on the way — actions competing with each other, a primary action
//! buried past the point anyone would still be reading, a page that leads
//! nowhere at all.
//!
//! # What this is not
//!
//! **It is not eye tracking, and it has not seen the page.** CrawlStat has no
//! browser: there is no layout, no pixel positions, no contrast, no screenshot
//! and no vision model. Everything here is inferred from the markup, so the
//! model knows about reading order, landmarks, element types and the words in
//! them, and knows nothing about what the CSS did with any of it. A page whose
//! source order fights its visual order will be described wrongly, and that is
//! a real limitation, not a rounding error.
//!
//! What saves it from being a guess is that the base ordering is not invented.
//! Document order is exactly what a screen reader speaks and what the Tab key
//! follows, so the predicted path is *literally correct* for keyboard and
//! assistive-technology users, and a reasonable prior for everyone else.
//!
//! Treat the output as a hypothesis worth testing, never as a measurement. It
//! does not replace watching a real person use the thing.
//!
//! # How a score is built
//!
//! For each node, in document order:
//!
//! ```text
//!   weight = affinity(persona, kind)      what this persona cares about
//!          × prominence(node)             heading level, label quality, length
//!          × region(node) × bias(persona) where in the page it sits
//!          × decay(position, persona)     how far down it is
//!          + intent_match(persona, node)  does the text say what they want
//! ```
//!
//! then the weights are turned into a distribution that sums to one, so
//! "attention" means a share of a finite thing rather than an unbounded score.
//! Confidence is reported per node and is lower for nodes whose evidence is
//! thin — an unlabelled control, a vague link.

pub mod architecture;
pub mod journey;
pub mod persona;
pub mod signals;

use scraper::Html;

pub use persona::Persona;
pub use signals::{NodeKind, Region, UxNode};

/// One node's share of attention.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Attention {
    pub order: usize,
    pub label: String,
    pub kind: NodeKind,
    pub region: Region,
    /// Share of total attention, 0..1. All shares in a report sum to 1.
    pub share: f32,
    /// How much to trust this node's score, 0..1.
    pub confidence: f32,
}

/// Two or more actions close together, competing for the same decision.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CtaConflict {
    /// The competing actions, strongest first.
    pub labels: Vec<String>,
    /// Combined share of attention they are splitting.
    pub share: f32,
}

/// A point where the path forks and nothing says which way to go.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DecisionNode {
    pub order: usize,
    /// How many comparable options are presented at once.
    pub options: usize,
    pub share: f32,
}

/// What the model thinks happens when this persona meets this page.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct FlowReport {
    pub persona: String,
    /// Nodes in predicted order of attention, strongest first.
    pub attention: Vec<Attention>,
    /// The predicted scan path: the nodes they actually reach, in the order
    /// they reach them. Document order, truncated by patience.
    pub path: Vec<usize>,
    /// How many nodes in before the primary action appears. `None` when the
    /// page has no primary action at all.
    pub steps_to_action: Option<usize>,
    /// True when the primary action sits past where this persona stops.
    pub action_out_of_reach: bool,
    pub cta_conflicts: Vec<CtaConflict>,
    pub decision_nodes: Vec<DecisionNode>,
    /// No onward link anywhere in the content: the page is a cul-de-sac.
    pub dead_end: bool,
    /// 0..1. How concentrated attention is on a few things rather than spread
    /// evenly over everything. A page that leads somewhere scores high.
    pub coherence: f32,
    /// Links whose text says nothing about where they go.
    pub vague_links: Vec<String>,
}

/// Everything the analysis found, for every persona.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct UxReport {
    pub nodes: Vec<UxNode>,
    pub flows: Vec<FlowReport>,
}

impl UxReport {
    pub fn flow(&self, persona: Persona) -> Option<&FlowReport> {
        self.flows.iter().find(|f| f.persona == persona.as_str())
    }
}

/// How prominent a node is from its own properties, independent of who is
/// looking.
fn prominence(node: &UxNode) -> f32 {
    let base = match node.kind {
        // An h1 is the page's claim about itself; by h4 nobody is counting.
        NodeKind::Heading(level) => 1.0 - f32::from(level.saturating_sub(1)) * 0.14,
        NodeKind::Action => 0.9,
        NodeKind::Field => 0.6,
        NodeKind::Link => 0.5,
        NodeKind::Image => 0.45,
        NodeKind::Text => {
            // Longer paragraphs draw more eye than one-liners, but with
            // diminishing returns: a wall of text is not ten times a sentence.
            0.3 + (node.words as f32).min(120.0).sqrt() / 40.0
        }
    };

    // A control nobody can name is a control nobody uses.
    let named = if node.label.trim().is_empty() {
        0.35
    } else if signals::is_vague(&node.label) {
        0.6
    } else {
        1.0
    };

    // Buried markup tends to be buried visually. Gentle, because it is the
    // weakest of these signals.
    let nesting = 1.0 / (1.0 + f32::from(node.nesting).max(0.0) * 0.015);

    let primary = if node.primary { 1.45 } else { 1.0 };

    base * named * nesting * primary
}

/// Confidence in a single node's score.
fn confidence(node: &UxNode) -> f32 {
    let mut score: f32 = 0.8;
    // Markup that names itself is markup we can reason about.
    if node.label.trim().is_empty() {
        score -= 0.35;
    } else if signals::is_vague(&node.label) {
        score -= 0.15;
    }
    // Landmarks are the only positional evidence available, so a node outside
    // them is placed by guesswork.
    if node.region == Region::Unknown {
        score -= 0.15;
    }
    // Headings and form controls carry their own semantics; a bare paragraph
    // is the vaguest thing here.
    if matches!(node.kind, NodeKind::Heading(_) | NodeKind::Field) {
        score += 0.15;
    }
    score.clamp(0.1, 1.0)
}

/// Score every node for one persona, as an unnormalised weight.
fn weigh(nodes: &[UxNode], persona: Persona) -> Vec<f32> {
    let total = nodes.len().max(1) as f32;
    nodes
        .iter()
        .map(|node| {
            let position = node.order as f32 / total;
            // Exponential fall-off down the page, steeper for impatient
            // personas. A linear fall-off would make the bottom of a long page
            // score the same as the bottom of a short one, which is wrong.
            let decay = (-persona.decay() * position).exp();
            let region = node.region.weight() * persona.region_bias(node.region);

            let weight = persona.affinity(node.kind)
                * prominence(node)
                * region
                * decay
                + persona::intent_match(persona, node) * 0.35 * decay;

            weight.max(0.0)
        })
        .collect()
}

/// Turn weights into shares that sum to one.
fn distribute(weights: &[f32]) -> Vec<f32> {
    let total: f32 = weights.iter().sum();
    if total <= f32::EPSILON {
        // Nothing scored: spread evenly rather than returning NaN.
        let share = if weights.is_empty() { 0.0 } else { 1.0 / weights.len() as f32 };
        return vec![share; weights.len()];
    }
    weights.iter().map(|w| w / total).collect()
}

/// How concentrated a distribution is, 0..1.
///
/// Normalised entropy, inverted: one dominant node scores near 1, perfectly
/// even attention scores 0. This is the number that says whether a page leads
/// anywhere — a page where everything is equally important leads nowhere.
pub fn coherence(shares: &[f32]) -> f32 {
    let present: Vec<f32> = shares.iter().copied().filter(|s| *s > 0.0).collect();
    if present.len() < 2 {
        return 1.0;
    }
    let entropy: f32 = -present.iter().map(|s| s * s.ln()).sum::<f32>();
    let max_entropy = (present.len() as f32).ln();
    (1.0 - entropy / max_entropy).clamp(0.0, 1.0)
}

/// Actions that sit close together in the path and split attention.
///
/// "Close together" is measured in nodes, not pixels — the model has no
/// pixels. Two commitments within a few elements of each other are asking the
/// reader to choose, whatever the layout did.
fn find_conflicts(nodes: &[UxNode], shares: &[f32]) -> Vec<CtaConflict> {
    const WINDOW: usize = 6;
    let actions: Vec<usize> = nodes
        .iter()
        .enumerate()
        .filter(|(_, n)| n.kind == NodeKind::Action && !n.label.trim().is_empty())
        .map(|(i, _)| i)
        .collect();

    let mut conflicts = Vec::new();
    let mut group: Vec<usize> = Vec::new();

    for &index in &actions {
        match group.last() {
            Some(&last) if index - last <= WINDOW => group.push(index),
            Some(_) => {
                if group.len() > 1 {
                    conflicts.push(group.clone());
                }
                group = vec![index];
            }
            None => group.push(index),
        }
    }
    if group.len() > 1 {
        conflicts.push(group);
    }

    conflicts
        .into_iter()
        .map(|group| {
            let mut ordered: Vec<usize> = group.clone();
            ordered.sort_by(|a, b| {
                shares[*b].partial_cmp(&shares[*a]).unwrap_or(std::cmp::Ordering::Equal)
            });
            CtaConflict {
                share: group.iter().map(|i| shares[*i]).sum(),
                labels: ordered.into_iter().map(|i| nodes[i].label.clone()).collect(),
            }
        })
        // Two actions sharing a trivial slice of attention are not a conflict,
        // they are a footer.
        .filter(|conflict| conflict.share > 0.04)
        .collect()
}

/// Points where several comparable options appear at once.
fn find_decisions(nodes: &[UxNode], shares: &[f32]) -> Vec<DecisionNode> {
    const WINDOW: usize = 5;
    const MIN_OPTIONS: usize = 3;

    let mut decisions = Vec::new();
    let mut index = 0;
    while index < nodes.len() {
        let end = (index + WINDOW).min(nodes.len());
        let options = nodes[index..end].iter().filter(|n| n.is_interactive()).count();
        if options >= MIN_OPTIONS {
            decisions.push(DecisionNode {
                order: index,
                options,
                share: shares[index..end].iter().sum(),
            });
            index = end;
        } else {
            index += 1;
        }
    }
    decisions
}

/// Fetch one page's markup so it can be analysed live.
///
/// A single blocking request for the page the user is looking at, not a crawl:
/// this exists because the UX screen has to work whether or not the crawl
/// stored raw HTML, and re-fetching one page is cheaper than storing every
/// page's body on the chance somebody opens this screen.
pub fn fetch_markup(url: &str) -> Result<String, String> {
    crate::fetch::install_crypto_provider();
    let client = reqwest::blocking::Client::builder()
        .user_agent(crate::config::DEFAULT_USER_AGENT)
        .timeout(std::time::Duration::from_secs(20))
        .build()
        .map_err(|error| error.to_string())?;
    client
        .get(url)
        .send()
        .and_then(|response| response.error_for_status())
        .and_then(|response| response.text())
        .map_err(|error| format!("could not fetch {url}: {error}"))
}

/// Analyse raw markup for one persona, returning the nodes it found and the
/// flow over them.
pub fn analyse_markup(body: &str, persona: Persona) -> (Vec<UxNode>, FlowReport) {
    let document = Html::parse_document(body);
    let nodes = signals::extract(&document);
    let flow = flow_for(&nodes, persona);
    (nodes, flow)
}

/// Run the full analysis over a parsed document.
pub fn analyse(document: &Html) -> UxReport {
    let nodes = signals::extract(document);
    let flows = Persona::ALL.into_iter().map(|p| flow_for(&nodes, p)).collect();
    UxReport { nodes, flows }
}

/// The flow for one persona over already-extracted nodes.
pub fn flow_for(nodes: &[UxNode], persona: Persona) -> FlowReport {
    let weights = weigh(nodes, persona);
    let shares = distribute(&weights);

    let mut attention: Vec<Attention> = nodes
        .iter()
        .zip(&shares)
        .map(|(node, share)| Attention {
            order: node.order,
            label: node.label.clone(),
            kind: node.kind,
            region: node.region,
            share: *share,
            confidence: confidence(node),
        })
        .collect();
    attention.sort_by(|a, b| {
        b.share.partial_cmp(&a.share).unwrap_or(std::cmp::Ordering::Equal).then(a.order.cmp(&b.order))
    });

    // The path is document order — what a screen reader speaks and the Tab
    // key follows — cut off where this persona's patience runs out.
    let reach = ((nodes.len() as f32) * persona.patience()).ceil() as usize;
    let reach = reach.clamp(1, nodes.len().max(1));
    let path: Vec<usize> = nodes.iter().take(reach).map(|n| n.order).collect();

    let steps_to_action = nodes.iter().position(|n| n.primary);
    let action_out_of_reach = steps_to_action.is_some_and(|at| at >= reach);

    // A page with no onward link in its content is a cul-de-sac. Navigation
    // and footer links do not count: every page has those, and they are not
    // where the content was taking you.
    let dead_end = !nodes.iter().any(|node| {
        matches!(node.kind, NodeKind::Link | NodeKind::Action)
            && matches!(node.region, Region::Main | Region::Unknown)
    });

    let vague_links = nodes
        .iter()
        .filter(|n| n.kind == NodeKind::Link && signals::is_vague(&n.label))
        .map(|n| n.label.clone())
        .collect();

    FlowReport {
        persona: persona.as_str().to_string(),
        path,
        steps_to_action,
        action_out_of_reach,
        cta_conflicts: find_conflicts(nodes, &shares),
        decision_nodes: find_decisions(nodes, &shares),
        dead_end,
        coherence: coherence(&shares),
        vague_links,
        attention,
    }
}

#[cfg(test)]
mod tests;
