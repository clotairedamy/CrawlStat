use super::*;

const SAAS: &str = r#"<!doctype html><html lang="en"><head>
<title>Acme — Automate your invoicing workflow</title>
<meta name="viewport" content="width=device-width, initial-scale=1">
<meta name="description" content="Acme automates invoicing for small finance teams.">
</head><body>
<header><nav><a href="/">Home</a><a href="/pricing">Pricing</a><a href="/login">Log in</a><a href="/contact">Contact</a></nav></header>
<main>
<h1>Send invoices in seconds, not hours</h1>
<p>Acme is invoicing software for small finance teams. Free trial, no credit card required.</p>
<a class="button" href="/signup">Start free trial</a>
<h2>Integrations</h2><p>Connect your dashboard to the tools you already use.</p>
<form><input type="email" placeholder="Email"><button>Get started</button></form>
<img src="/hero.png">
</main>
<footer><a href="/privacy">Privacy</a> Write to jane.doe@example.com</footer>
</body></html>"#;

fn source(html: &str) -> Source {
    Source { html: html.to_string(), origin: "stored".into(), status: Some(200), response_ms: Some(240), bytes: Some(html.len() as u64), fetched_at: 1 }
}

fn audit_of(html: &str, depth: Depth) -> Audit {
    let mut setup = Setup::new("https://acme.test/");
    setup.depth = depth;
    run(&setup, &source(html), None)
}

#[test]
fn classifies_and_plans_a_saas_page() {
    let audit = audit_of(SAAS, Depth::Standard);
    assert_eq!(audit.page_type, PageType::Saas);
    assert!(audit.checks.iter().any(|c| c.id == "saas_pricing" && c.status == Status::Pass));
    assert!(!audit.checks.iter().any(|c| c.id.starts_with("shop_")));
}

#[test]
fn never_claims_what_it_cannot_see() {
    let audit = audit_of(SAAS, Depth::Deep);
    for id in ["contrast", "reflow", "keyboard", "tap_targets"] {
        let check = audit.checks.iter().find(|c| c.id == id).expect(id);
        assert_eq!(check.status, Status::NotTested, "{id}");
    }
    assert_eq!(audit.checks.iter().find(|c| c.id == "form_submit").unwrap().status, Status::Blocked);
    // No crawl: link checks can't have passed.
    assert_eq!(audit.checks.iter().find(|c| c.id == "broken_links").unwrap().status, Status::NotTested);
}

#[test]
fn finds_unlabelled_fields_with_a_recommendation() {
    let audit = audit_of(SAAS, Depth::Standard);
    let labels = audit.checks.iter().find(|c| c.id == "form_labels").unwrap();
    assert_eq!(labels.status, Status::Fail);
    let finding = labels.finding.as_ref().unwrap();
    assert!(!finding.recommendation.acceptance.is_empty());
    assert!(finding.priority > 0.0);
}

#[test]
fn depth_widens_the_plan() {
    let quick = audit_of(SAAS, Depth::Quick).checks.len();
    let deep = audit_of(SAAS, Depth::Deep).checks.len();
    assert!(deep > quick);
}

#[test]
fn summary_keeps_statuses_apart() {
    let s = audit_of(SAAS, Depth::Deep).summary();
    assert_eq!(s.selected, s.pass + s.fail + s.needs_review + s.not_tested + s.blocked);
    assert!(s.not_tested > 0 && s.blocked > 0);
    assert!(s.coverage < 1.0);
}

#[test]
fn redacts_emails_and_tokens() {
    assert_eq!(redact("mail jane.doe@example.com"), "mail j•••@example.com");
    assert_eq!(redact("/reset?token=abc123&x=1"), "/reset?token=•••&x=1");
}

#[test]
fn compare_sees_a_fix() {
    let before = audit_of(SAAS, Depth::Standard);
    let fixed = SAAS.replace(r#"<input type="email" placeholder="Email">"#, r#"<label for="e">Email</label><input id="e" type="email" autocomplete="email">"#);
    let after = audit_of(&fixed, Depth::Standard);
    let c = compare(&before, &after);
    let row = c.rows.iter().find(|r| r.id == "form_labels").unwrap();
    assert_eq!(row.change, "fixed");
    assert!(c.retest_pass_rate.unwrap() > 0.0);
}

#[test]
fn store_round_trip_and_workflow() {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    let mut audit = audit_of(SAAS, Depth::Standard);
    let id = store::save(&conn, &mut audit).unwrap();
    let loaded = store::load(&conn, id).unwrap().unwrap();
    assert_eq!(loaded.checks.len(), audit.checks.len());
    assert_eq!(store::list(&conn).unwrap().len(), 1);

    let mut state = store::FindingState::new("form_labels");
    state.work_status = WorkStatus::InProgress;
    state.owner = "Sam".into();
    store::set_state(&conn, id, &state).unwrap();
    assert_eq!(store::states(&conn, id).unwrap()[0].owner, "Sam");
    assert_eq!(store::activity(&conn, id).unwrap().len(), 2);

    let csv = report::developer_csv(&loaded, &store::states(&conn, id).unwrap());
    assert!(csv.lines().count() > 1);
    let md = report::client_markdown(&loaded, &[], None);
    assert!(md.contains("not a full audit"));
    assert!(!md.contains("jane.doe@"));

    store::delete(&conn, id).unwrap();
    assert!(store::load(&conn, id).unwrap().is_none());
}
