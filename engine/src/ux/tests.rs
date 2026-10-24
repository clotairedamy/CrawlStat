use super::*;
use scraper::Html;

fn page(body: &str) -> Html {
    Html::parse_document(&format!("<html><body>{body}</body></html>"))
}

/// A plausible landing page: nav, a hero with one commitment, some copy, a
/// footer. Used where a test needs something realistic rather than minimal.
const LANDING: &str = r#"
<header><nav>
  <a href="/features">Features</a>
  <a href="/pricing">Pricing</a>
</nav></header>
<main>
  <h1>Crawl it all</h1>
  <p>A desktop SEO crawler that runs on your own machine and never uploads a thing.</p>
  <a href="/download">Start free trial</a>
  <h2>How it works</h2>
  <p>Point it at a site and it fetches every page, then audits what it found.</p>
  <a href="/docs">Read the documentation</a>
</main>
<footer>
  <a href="/privacy">Privacy</a>
  <a href="/terms">Terms</a>
</footer>"#;

// ─── extraction ──────────────────────────────────────────────────────────

#[test]
fn nodes_come_out_in_document_order() {
    let report = analyse(&page(LANDING));
    let orders: Vec<usize> = report.nodes.iter().map(|n| n.order).collect();
    assert_eq!(orders, (0..report.nodes.len()).collect::<Vec<_>>());
}

#[test]
fn landmarks_place_each_node() {
    let report = analyse(&page(LANDING));
    let region_of = |text: &str| {
        report.nodes.iter().find(|n| n.label.contains(text)).map(|n| n.region)
    };
    assert_eq!(region_of("Features"), Some(Region::Nav));
    assert_eq!(region_of("Crawl it all"), Some(Region::Main));
    assert_eq!(region_of("Privacy"), Some(Region::Footer));
}

#[test]
fn a_link_that_asks_for_a_commitment_is_an_action_not_navigation() {
    // Most "buy" buttons on the web are anchors, and treating them as
    // ordinary links is how a model misses the point of the page.
    let report = analyse(&page(LANDING));
    let trial = report.nodes.iter().find(|n| n.label.contains("Start free trial")).unwrap();
    assert_eq!(trial.kind, NodeKind::Action);
    assert!(trial.primary);

    let docs = report.nodes.iter().find(|n| n.label.contains("documentation")).unwrap();
    assert_eq!(docs.kind, NodeKind::Link);
    assert!(!docs.primary);
}

#[test]
fn content_hidden_from_assistive_technology_is_hidden_from_the_model() {
    let report = analyse(&page(
        r#"<main><h1>Shown</h1><h2 aria-hidden="true">Hidden</h2>
           <h3 hidden>Also hidden</h3></main>"#,
    ));
    let labels: Vec<&str> = report.nodes.iter().map(|n| n.label.as_str()).collect();
    assert!(labels.contains(&"Shown"));
    assert!(!labels.iter().any(|l| l.contains("Hidden")));
    assert!(!labels.iter().any(|l| l.contains("Also hidden")));
}

#[test]
fn an_image_with_no_alt_text_contributes_nothing() {
    let report = analyse(&page(
        r#"<main><img src="a.png" alt="A chart of crawl speed"><img src="b.png"></main>"#,
    ));
    let images: Vec<_> = report.nodes.iter().filter(|n| n.kind == NodeKind::Image).collect();
    assert_eq!(images.len(), 1, "the undescribed image should be skipped");
}

// ─── attention ───────────────────────────────────────────────────────────

#[test]
fn attention_is_a_share_of_one() {
    let report = analyse(&page(LANDING));
    for flow in &report.flows {
        let total: f32 = flow.attention.iter().map(|a| a.share).sum();
        assert!((total - 1.0).abs() < 1e-4, "{} summed to {total}", flow.persona);
    }
}

#[test]
fn an_empty_page_does_not_divide_by_zero() {
    let report = analyse(&page("<main></main>"));
    assert!(report.nodes.is_empty());
    for flow in &report.flows {
        assert!(flow.attention.is_empty());
        assert!(flow.coherence.is_finite());
    }
}

#[test]
fn the_task_oriented_reader_lands_on_the_action_and_the_explorer_does_not() {
    // The same markup, read by two people who came for different reasons,
    // must not produce the same ranking — that is the whole premise.
    let report = analyse(&page(LANDING));

    let top = |p: Persona| report.flow(p).unwrap().attention[0].label.clone();
    assert!(top(Persona::TaskOriented).contains("Start free trial"));
    assert_ne!(top(Persona::Explorer), top(Persona::TaskOriented));
}

#[test]
fn the_sceptic_weighs_the_small_print_the_rushed_reader_ignores() {
    let html = page(
        r#"<main><h1>Plans</h1><a href="/buy">Buy now</a></main>
           <footer><p>Cancel any time. Full refund within 30 days, no questions.</p></footer>"#,
    );
    let report = analyse(&html);
    let share_of = |p: Persona| {
        let flow = report.flow(p).unwrap();
        flow.attention.iter().find(|a| a.label.contains("refund")).map(|a| a.share).unwrap_or(0.0)
    };
    assert!(
        share_of(Persona::Skeptical) > share_of(Persona::MobileRushed) * 3.0,
        "sceptic {} vs rushed {}",
        share_of(Persona::Skeptical),
        share_of(Persona::MobileRushed)
    );
}

#[test]
fn attention_falls_off_down_the_page_and_faster_for_impatient_readers() {
    let mut body = String::from("<main>");
    for index in 0..40 {
        body.push_str(&format!("<h2>Section {index}</h2>"));
    }
    body.push_str("</main>");
    let report = analyse(&page(&body));

    let last_share = |p: Persona| {
        let flow = report.flow(p).unwrap();
        flow.attention.iter().find(|a| a.order == 39).unwrap().share
    };
    let first_share = |p: Persona| {
        let flow = report.flow(p).unwrap();
        flow.attention.iter().find(|a| a.order == 0).unwrap().share
    };

    for persona in Persona::ALL {
        assert!(
            last_share(persona) < first_share(persona),
            "{} did not decay",
            persona.as_str()
        );
    }
    // The rushed reader should fall off hardest.
    let rushed_ratio = last_share(Persona::MobileRushed) / first_share(Persona::MobileRushed);
    let sceptic_ratio = last_share(Persona::Skeptical) / first_share(Persona::Skeptical);
    assert!(rushed_ratio < sceptic_ratio);
}

#[test]
fn a_control_nobody_can_name_scores_below_one_that_is_labelled() {
    let report = analyse(&page(
        r#"<main><button>Download for macOS</button><button></button></main>"#,
    ));
    let flow = report.flow(Persona::TaskOriented).unwrap();
    let named = flow.attention.iter().find(|a| !a.label.is_empty()).unwrap();
    let unnamed = flow.attention.iter().find(|a| a.label.is_empty()).unwrap();
    assert!(named.share > unnamed.share);
    assert!(unnamed.confidence < named.confidence, "an unnamed control is also less certain");
}

// ─── coherence ───────────────────────────────────────────────────────────

#[test]
fn coherence_is_high_when_one_thing_dominates_and_low_when_nothing_does() {
    assert!(coherence(&[0.9, 0.05, 0.05]) > 0.5);
    let even = vec![0.25; 4];
    assert!(coherence(&even) < 0.01, "evenly spread attention is incoherent");
}

#[test]
fn coherence_handles_degenerate_input() {
    assert_eq!(coherence(&[]), 1.0);
    assert_eq!(coherence(&[1.0]), 1.0);
    assert!(coherence(&[0.0, 0.0]).is_finite());
}

#[test]
fn a_page_with_one_clear_action_is_more_coherent_than_a_wall_of_equals() {
    let focused = analyse(&page(
        r#"<main><h1>One thing</h1><a href="/go">Start free trial</a></main>"#,
    ));
    let mut body = String::from("<main>");
    for index in 0..18 {
        body.push_str(&format!(r#"<a href="/x{index}">Option {index}</a>"#));
    }
    body.push_str("</main>");
    let scattered = analyse(&page(&body));

    let f = focused.flow(Persona::TaskOriented).unwrap().coherence;
    let s = scattered.flow(Persona::TaskOriented).unwrap().coherence;
    assert!(f > s, "focused {f} should beat scattered {s}");
}

// ─── diagnostics ─────────────────────────────────────────────────────────

#[test]
fn competing_actions_beside_each_other_are_reported_as_a_conflict() {
    let report = analyse(&page(
        r#"<main><h1>Pick one</h1>
           <button>Start free trial</button>
           <button>Buy now</button>
           <button>Request a demo</button></main>"#,
    ));
    let flow = report.flow(Persona::TaskOriented).unwrap();
    assert_eq!(flow.cta_conflicts.len(), 1);
    assert_eq!(flow.cta_conflicts[0].labels.len(), 3);
}

#[test]
fn one_action_on_its_own_is_not_a_conflict() {
    let report = analyse(&page(
        r#"<main><h1>Clear</h1><button>Start free trial</button></main>"#,
    ));
    assert!(report.flow(Persona::TaskOriented).unwrap().cta_conflicts.is_empty());
}

#[test]
fn actions_far_apart_are_not_a_conflict() {
    let mut body = String::from(r#"<main><button>Start free trial</button>"#);
    for index in 0..12 {
        body.push_str(&format!("<p>Paragraph number {index} with enough words to count.</p>"));
    }
    body.push_str("<button>Buy now</button></main>");
    let report = analyse(&page(&body));
    assert!(
        report.flow(Persona::TaskOriented).unwrap().cta_conflicts.is_empty(),
        "actions a dozen elements apart are not competing"
    );
}

#[test]
fn a_page_with_no_onward_content_link_is_a_dead_end() {
    let stuck = analyse(&page(
        r#"<main><h1>Thanks</h1><p>Your order is confirmed and on its way.</p></main>
           <nav><a href="/">Home</a></nav>"#,
    ));
    // The nav link does not rescue it: every page has navigation, and that is
    // not where the content was taking the reader.
    assert!(stuck.flow(Persona::Explorer).unwrap().dead_end);

    let onward = analyse(&page(LANDING));
    assert!(!onward.flow(Persona::Explorer).unwrap().dead_end);
}

#[test]
fn a_buried_action_is_flagged_out_of_reach_for_the_personas_who_stop_early() {
    let mut body = String::from("<main><h1>A very long page</h1>");
    for index in 0..50 {
        body.push_str(&format!("<p>Paragraph {index} with several words in it here.</p>"));
    }
    body.push_str(r#"<button type="submit">Sign up</button></main>"#);
    let report = analyse(&page(&body));

    assert!(report.flow(Persona::MobileRushed).unwrap().action_out_of_reach);
    assert!(!report.flow(Persona::Skeptical).unwrap().action_out_of_reach,
        "the sceptic reads to the bottom, so nothing is out of their reach");
}

#[test]
fn steps_to_action_is_none_when_there_is_nothing_to_do() {
    let report = analyse(&page(
        r#"<main><h1>An article</h1><p>Words and more words, at some length.</p></main>"#,
    ));
    let flow = report.flow(Persona::TaskOriented).unwrap();
    assert_eq!(flow.steps_to_action, None);
    assert!(!flow.action_out_of_reach, "no action cannot be out of reach");
}

#[test]
fn a_row_of_options_is_a_decision_point() {
    let report = analyse(&page(
        r#"<main><a href="/a">Plan A</a><a href="/b">Plan B</a>
           <a href="/c">Plan C</a><a href="/d">Plan D</a></main>"#,
    ));
    assert!(!report.flow(Persona::Explorer).unwrap().decision_nodes.is_empty());
}

#[test]
fn links_that_say_nothing_are_collected() {
    let report = analyse(&page(
        r#"<main><a href="/a">Click here</a><a href="/b">read more</a>
           <a href="/c">The 2026 crawl report</a></main>"#,
    ));
    let flow = report.flow(Persona::Explorer).unwrap();
    assert_eq!(flow.vague_links.len(), 2);
    assert!(!flow.vague_links.iter().any(|l| l.contains("2026")));
}

#[test]
fn the_path_is_document_order_cut_to_the_personas_patience() {
    let mut body = String::from("<main>");
    for index in 0..20 {
        body.push_str(&format!("<h2>Heading {index}</h2>"));
    }
    body.push_str("</main>");
    let report = analyse(&page(&body));

    for persona in Persona::ALL {
        let flow = report.flow(persona).unwrap();
        assert!(flow.path.windows(2).all(|w| w[0] < w[1]), "the path must run forwards");
        assert!(!flow.path.is_empty(), "somebody always sees something");
        assert!(flow.path.len() <= report.nodes.len());
    }
    let rushed = report.flow(Persona::MobileRushed).unwrap().path.len();
    let sceptic = report.flow(Persona::Skeptical).unwrap().path.len();
    assert!(rushed < sceptic);
}

#[test]
fn every_persona_is_reported_once() {
    let report = analyse(&page(LANDING));
    assert_eq!(report.flows.len(), Persona::ALL.len());
    for persona in Persona::ALL {
        assert!(report.flow(persona).is_some(), "{} missing", persona.as_str());
        assert!(Persona::parse(persona.as_str()) == Some(persona));
    }
}

#[test]
fn scores_stay_finite_on_hostile_markup() {
    // Deep nesting, empty labels, no landmarks: nothing here should produce a
    // NaN, and one NaN would poison every share in the distribution.
    let nested = "<div>".repeat(300) + "<a href=\"/x\"></a><button></button>" + &"</div>".repeat(300);
    let report = analyse(&page(&nested));
    for flow in &report.flows {
        for attention in &flow.attention {
            assert!(attention.share.is_finite(), "share was not finite");
            assert!(attention.confidence.is_finite());
            assert!((0.0..=1.0).contains(&attention.confidence));
        }
        assert!(flow.coherence.is_finite());
    }
}
