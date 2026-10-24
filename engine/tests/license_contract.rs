//! The licence contract between the Node API and the Rust engine.
//!
//! A token is signed by `site/server/utils/token.ts` and verified by
//! `engine/src/license.rs`. Those are two languages, two base64
//! implementations and two JSON serialisers, and if any of them disagrees then
//! every activation fails in production — after shipping, at the worst
//! possible moment.
//!
//! So the fixture in `tests/fixtures/` is a real token, signed by the real Node
//! code, committed to the repository. This test verifies it with the real Rust
//! verifier. It needs no Node at test time and no network, and it fails the
//! moment either side changes its encoding.
//!
//! Regenerate it with `cd site && npx tsx test/make-fixture.ts`.

use crawlstat_engine::license::{self, Plan};

#[derive(serde::Deserialize)]
struct Fixture {
    #[serde(rename = "publicKeyHex")]
    public_key_hex: String,
    token: String,
    expected: Expected,
}

#[derive(serde::Deserialize)]
struct Expected {
    version: u8,
    plan: String,
    issued_at: i64,
    expires_at: i64,
    key_fingerprint: String,
    instance_id: String,
    features: Vec<String>,
}

fn fixture() -> Fixture {
    let raw = include_str!("fixtures/node_signed_token.json");
    serde_json::from_str(raw).expect("the fixture is valid JSON")
}

#[test]
fn a_token_signed_by_the_node_api_verifies_in_the_engine() {
    let fixture = fixture();

    let entitlement = license::verify_token(&fixture.token, &fixture.public_key_hex)
        .expect("a token signed by the API must verify in the app");

    // Every field survived both serialisers.
    assert_eq!(entitlement.version, fixture.expected.version);
    assert_eq!(entitlement.plan, Plan::Pro);
    assert_eq!(entitlement.plan.as_str(), fixture.expected.plan);
    assert_eq!(entitlement.issued_at, fixture.expected.issued_at);
    assert_eq!(entitlement.expires_at, fixture.expected.expires_at);
    assert_eq!(
        entitlement.key_fingerprint,
        fixture.expected.key_fingerprint
    );
    assert_eq!(entitlement.instance_id, fixture.expected.instance_id);
    assert_eq!(entitlement.features, fixture.expected.features);
}

#[test]
fn the_grace_period_matches_on_both_sides() {
    let fixture = fixture();
    let span_days = (fixture.expected.expires_at - fixture.expected.issued_at) / 86_400;

    // The API decides the window and the engine enforces it; a disagreement
    // would either cut a licence short or extend it past what was sold.
    assert_eq!(span_days, license::GRACE_PERIOD_DAYS);
}

#[test]
fn the_api_grants_exactly_the_features_this_build_knows() {
    let fixture = fixture();

    for granted in &fixture.expected.features {
        assert!(
            license::features::PRO.contains(&granted.as_str()),
            "the API grants `{granted}`, which this build does not know about",
        );
    }
    for known in license::features::PRO {
        assert!(
            fixture
                .expected
                .features
                .iter()
                .any(|granted| granted == known),
            "this build expects `{known}`, which the API does not grant",
        );
    }
}

#[test]
fn the_verified_token_is_enforced_as_a_full_licence() {
    let fixture = fixture();
    let entitlement = license::verify_token(&fixture.token, &fixture.public_key_hex).unwrap();

    assert!(entitlement.is_pro());
    assert_eq!(
        entitlement.max_urls_per_crawl(),
        None,
        "a Pro licence has no URL cap"
    );
    assert!(entitlement.has(license::features::UNLIMITED_URLS));
    assert!(entitlement.has(license::features::XLSX_EXPORT));

    // And it classifies as active when read at the moment it was issued.
    let status = license::classify(entitlement, fixture.expected.issued_at);
    assert!(status.is_pro(), "{status:?}");
}

#[test]
fn the_fixtures_token_does_not_verify_with_the_wrong_key() {
    let fixture = fixture();

    // The embedded development key is a different key, so the fixture must be
    // rejected by it — proof that verification actually checks the signature
    // rather than merely decoding the payload.
    assert!(
        license::verify_token(&fixture.token, license::SIGNING_PUBLIC_KEY_HEX).is_err(),
        "a token must not verify under a key that did not sign it",
    );
}
