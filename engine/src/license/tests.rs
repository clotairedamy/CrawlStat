//! Licensing tests.
//!
//! Tokens are signed with the RFC 8032 §7.1 test key, whose private half is
//! published in the standard — the same key [`SIGNING_PUBLIC_KEY_HEX`] carries
//! in a development build. Nothing secret is involved, and the tests exercise
//! exactly the verification path a release build uses.

use ed25519_dalek::{Signer, SigningKey};

use super::*;

/// The RFC 8032 §7.1 TEST 1 private key.
const TEST_SECRET: [u8; 32] = [
    0x9d, 0x61, 0xb1, 0x9d, 0xef, 0xfd, 0x5a, 0x60, 0xba, 0x84, 0x4a, 0xf4, 0x92, 0xec, 0x2c, 0xc4,
    0x44, 0x49, 0xc5, 0x69, 0x7b, 0x32, 0x69, 0x19, 0x70, 0x3b, 0xac, 0x03, 0x1c, 0xae, 0x7f, 0x60,
];

fn signing_key() -> SigningKey {
    SigningKey::from_bytes(&TEST_SECRET)
}

/// Sign an entitlement the way the API's signing step does.
fn sign(entitlement: &Entitlement) -> String {
    let payload = encode_payload(entitlement);
    let signature = signing_key().sign(payload.as_bytes());
    let signature_b64 =
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(signature.to_bytes());
    format!("{payload}.{signature_b64}")
}

/// A Pro entitlement issued `issued_ago_days` ago.
fn pro(now: i64, issued_ago_days: i64) -> Entitlement {
    let issued_at = now - issued_ago_days * 86_400;
    Entitlement {
        version: 1,
        plan: Plan::Pro,
        issued_at,
        expires_at: issued_at + GRACE_PERIOD_DAYS * 86_400,
        key_fingerprint: fingerprint("CS-TEST-0001"),
        instance_id: "instance-abc".into(),
        features: features::PRO.iter().map(|f| f.to_string()).collect(),
    }
}

const NOW: i64 = 1_800_000_000;

// ─── the embedded key ────────────────────────────────────────────────────

#[test]
fn the_embedded_public_key_matches_the_development_private_key() {
    // If these ever disagree, every token in a development build is rejected
    // and the failure looks like a signing bug rather than a key mismatch.
    let derived = signing_key().verifying_key();
    assert_eq!(hex(derived.to_bytes().as_slice()), SIGNING_PUBLIC_KEY_HEX);
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

// ─── verification ────────────────────────────────────────────────────────

#[test]
fn a_properly_signed_token_verifies() {
    let entitlement = pro(NOW, 0);
    let token = sign(&entitlement);
    assert_eq!(
        verify_token(&token, SIGNING_PUBLIC_KEY_HEX).unwrap(),
        entitlement
    );
}

#[test]
fn a_token_signed_with_the_wrong_key_is_rejected() {
    // The whole point: someone with the app cannot mint their own licence.
    let other = SigningKey::from_bytes(&[7u8; 32]);
    let payload = encode_payload(&pro(NOW, 0));
    let signature = other.sign(payload.as_bytes());
    let token = format!(
        "{payload}.{}",
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(signature.to_bytes())
    );

    assert_eq!(
        verify_token(&token, SIGNING_PUBLIC_KEY_HEX),
        Err(TokenError::BadSignature)
    );
}

#[test]
fn editing_the_payload_invalidates_the_token() {
    let token = sign(&pro(NOW, 0));
    let (payload, signature) = token.split_once('.').unwrap();

    // Re-sign nothing; just swap in a payload claiming a longer licence.
    let forged = Entitlement {
        expires_at: NOW + 3_650 * 86_400,
        ..pro(NOW, 0)
    };
    let tampered = format!("{}.{signature}", encode_payload(&forged));
    assert_eq!(
        verify_token(&tampered, SIGNING_PUBLIC_KEY_HEX),
        Err(TokenError::BadSignature)
    );

    // Flipping a single character of the real payload is equally fatal.
    let mut bytes = payload.as_bytes().to_vec();
    bytes[4] = if bytes[4] == b'A' { b'B' } else { b'A' };
    let flipped = format!("{}.{signature}", String::from_utf8(bytes).unwrap());
    assert!(verify_token(&flipped, SIGNING_PUBLIC_KEY_HEX).is_err());
}

#[test]
fn malformed_tokens_are_rejected_without_panicking() {
    for token in [
        "",
        "not-a-token",
        "no-dot-separator",
        ".",
        "a.b",
        "!!!.!!!",
        // A valid payload with a signature of the wrong length.
        &format!("{}.{}", encode_payload(&pro(NOW, 0)), "AAAA"),
    ] {
        assert!(
            verify_token(token, SIGNING_PUBLIC_KEY_HEX).is_err(),
            "accepted {token:?}"
        );
    }
}

#[test]
fn an_unusable_embedded_key_is_reported_clearly() {
    let token = sign(&pro(NOW, 0));
    match verify_token(&token, "not-hex") {
        Err(TokenError::BadVerifyingKey(_)) => {}
        other => panic!("expected a key error, got {other:?}"),
    }
}

#[test]
fn a_future_token_version_is_refused_rather_than_misread() {
    let entitlement = Entitlement {
        version: 2,
        ..pro(NOW, 0)
    };
    assert_eq!(
        verify_token(&sign(&entitlement), SIGNING_PUBLIC_KEY_HEX),
        Err(TokenError::UnsupportedVersion(2))
    );
}

// ─── the clock ───────────────────────────────────────────────────────────

#[test]
fn a_fresh_token_is_active() {
    match classify(pro(NOW, 0), NOW) {
        LicenseStatus::Active {
            days_remaining,
            entitlement,
        } => {
            assert_eq!(days_remaining, GRACE_PERIOD_DAYS);
            assert!(entitlement.is_pro());
        }
        other => panic!("expected Active, got {other:?}"),
    }
}

#[test]
fn a_token_past_its_refresh_window_is_in_grace_but_still_works() {
    // Three days without a successful refresh: everything keeps working, and
    // the UI can mention it.
    let status = classify(pro(NOW, 3), NOW);
    match &status {
        LicenseStatus::Grace { days_remaining, .. } => {
            assert_eq!(*days_remaining, GRACE_PERIOD_DAYS - 3);
        }
        other => panic!("expected Grace, got {other:?}"),
    }
    assert!(status.is_pro(), "a grace-period licence is still a licence");
    assert_eq!(status.entitlement().max_urls_per_crawl(), None);
}

#[test]
fn the_grace_period_is_exactly_a_fortnight() {
    // One day before expiry: still working.
    let status = classify(pro(NOW, GRACE_PERIOD_DAYS - 1), NOW);
    assert!(status.is_pro(), "{status:?}");

    // The moment it lapses, features stop.
    let status = classify(pro(NOW, GRACE_PERIOD_DAYS), NOW);
    assert!(
        matches!(status, LicenseStatus::Expired { .. }),
        "{status:?}"
    );
    assert!(!status.is_pro());
}

#[test]
fn an_expired_licence_falls_back_to_the_free_tier_not_to_nothing() {
    let status = classify(pro(NOW, 30), NOW);
    assert!(matches!(status, LicenseStatus::Expired { .. }));
    // The app keeps working; it just stops being Pro.
    assert_eq!(status.entitlement().plan, Plan::Free);
    assert_eq!(
        status.entitlement().max_urls_per_crawl(),
        Some(FREE_TIER_MAX_URLS)
    );
}

#[test]
fn an_invalid_stored_token_behaves_exactly_like_no_licence() {
    let status = evaluate("rubbish", SIGNING_PUBLIC_KEY_HEX, NOW);
    match &status {
        LicenseStatus::Invalid { reason } => assert!(!reason.is_empty()),
        other => panic!("expected Invalid, got {other:?}"),
    }
    assert_eq!(status.entitlement().plan, Plan::Free);
    assert_eq!(
        status.entitlement().max_urls_per_crawl(),
        Some(FREE_TIER_MAX_URLS)
    );
}

#[test]
fn a_free_entitlement_never_asks_to_be_refreshed() {
    assert!(!Entitlement::free().needs_refresh(NOW));
    assert_eq!(classify(Entitlement::free(), NOW), LicenseStatus::Free);
}

#[test]
fn refresh_is_due_about_once_a_day() {
    assert!(!pro(NOW, 0).needs_refresh(NOW));
    // Just before and just after the interval.
    let almost = Entitlement {
        issued_at: NOW - 23 * 3_600,
        ..pro(NOW, 0)
    };
    assert!(!almost.needs_refresh(NOW));
    let due = Entitlement {
        issued_at: NOW - 25 * 3_600,
        ..pro(NOW, 0)
    };
    assert!(due.needs_refresh(NOW));
}

#[test]
fn days_remaining_rounds_up_so_a_part_day_still_counts() {
    let entitlement = Entitlement {
        expires_at: NOW + 20 * 3_600,
        ..pro(NOW, 0)
    };
    assert_eq!(entitlement.days_remaining(NOW), 1);
    let entitlement = Entitlement {
        expires_at: NOW + 60,
        ..pro(NOW, 0)
    };
    assert_eq!(entitlement.days_remaining(NOW), 1);
    let entitlement = Entitlement {
        expires_at: NOW - 60,
        ..pro(NOW, 0)
    };
    assert!(entitlement.days_remaining(NOW) <= 0);
}

// ─── features and limits ─────────────────────────────────────────────────

#[test]
fn the_free_tier_is_capped_and_pro_is_not() {
    assert_eq!(
        Entitlement::free().max_urls_per_crawl(),
        Some(FREE_TIER_MAX_URLS)
    );
    assert_eq!(pro(NOW, 0).max_urls_per_crawl(), None);
}

#[test]
fn a_pro_token_granting_an_unknown_feature_still_works() {
    // The server may grant something a shipped build has never heard of.
    let entitlement = Entitlement {
        features: vec![features::UNLIMITED_URLS.into(), "time_travel".into()],
        ..pro(NOW, 0)
    };
    assert!(entitlement.has("time_travel"));
    assert!(entitlement.has(features::UNLIMITED_URLS));
    assert!(!entitlement.has(features::XLSX_EXPORT));
    assert_eq!(entitlement.max_urls_per_crawl(), None);
}

#[test]
fn a_pro_plan_without_the_unlimited_feature_is_still_capped() {
    // Features, not the plan name, decide what is allowed — so a downgraded or
    // partially refunded licence cannot grant more than it should.
    let entitlement = Entitlement {
        features: vec![features::XLSX_EXPORT.into()],
        ..pro(NOW, 0)
    };
    assert!(entitlement.is_pro());
    assert_eq!(entitlement.max_urls_per_crawl(), Some(FREE_TIER_MAX_URLS));
}

// ─── enforcement ─────────────────────────────────────────────────────────

use super::testing::with_license;

#[test]
fn an_unlicensed_install_is_capped_whatever_it_asks_for() {
    with_license(LicenseStatus::Free, || {
        // No limit requested: the free cap applies.
        assert_eq!(max_urls_for_crawl(None), Some(FREE_TIER_MAX_URLS));
        // A bigger limit requested: still capped.
        assert_eq!(
            max_urls_for_crawl(Some(1_000_000)),
            Some(FREE_TIER_MAX_URLS)
        );
        // A smaller limit requested: honoured, because it is the user's own
        // choice.
        assert_eq!(max_urls_for_crawl(Some(100)), Some(100));
    });
}

#[test]
fn a_licensed_install_gets_what_it_asks_for() {
    with_license(classify(pro(NOW, 0), NOW), || {
        assert_eq!(max_urls_for_crawl(None), None, "Pro means no limit");
        assert_eq!(max_urls_for_crawl(Some(50_000)), Some(50_000));
    });
}

#[test]
fn an_expired_licence_is_enforced_as_free() {
    with_license(classify(pro(NOW, 30), NOW), || {
        assert_eq!(max_urls_for_crawl(None), Some(FREE_TIER_MAX_URLS));
    });
}

#[test]
fn the_default_state_is_the_safe_one() {
    // A build that never loads a licence must be limited, not unlimited: the
    // direction a mistake here should fail in.
    assert_eq!(
        LicenseStatus::Free.entitlement().max_urls_per_crawl(),
        Some(FREE_TIER_MAX_URLS)
    );
}

// ─── fingerprints ────────────────────────────────────────────────────────

#[test]
fn a_fingerprint_identifies_a_key_without_revealing_it() {
    let key = "CS-LIVE-9f3a-2b71-4c08";
    let print = fingerprint(key);

    assert_eq!(print.len(), 8);
    assert_eq!(
        print,
        fingerprint(key),
        "the same key always prints the same"
    );
    assert_eq!(
        print,
        fingerprint(&format!("  {key}  ")),
        "whitespace is ignored"
    );
    assert_ne!(print, fingerprint("CS-LIVE-9f3a-2b71-4c09"));
    assert!(
        !key.contains(&print),
        "a fingerprint must not be a slice of the key"
    );
}

// ─── the token format ────────────────────────────────────────────────────

#[test]
fn a_token_carries_nothing_personal() {
    // A token found on a stolen laptop should say nothing about who owns it.
    let token = sign(&pro(NOW, 0));
    let payload = token.split('.').next().unwrap();
    let decoded = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload)
        .unwrap();
    let json = String::from_utf8(decoded).unwrap();

    for forbidden in ["@", "email", "name", "customer"] {
        assert!(
            !json.contains(forbidden),
            "token payload contains {forbidden}: {json}"
        );
    }
    // And not the licence key itself, only its fingerprint.
    assert!(!json.contains("CS-TEST-0001"));
}

#[test]
fn a_token_round_trips_through_its_wire_format() {
    let entitlement = pro(NOW, 2);
    let verified = verify_token(&sign(&entitlement), SIGNING_PUBLIC_KEY_HEX).unwrap();

    assert_eq!(verified.plan, Plan::Pro);
    assert_eq!(verified.issued_at, entitlement.issued_at);
    assert_eq!(verified.expires_at, entitlement.expires_at);
    assert_eq!(verified.key_fingerprint, entitlement.key_fingerprint);
    assert_eq!(verified.instance_id, "instance-abc");
    assert_eq!(verified.features.len(), features::PRO.len());
}
