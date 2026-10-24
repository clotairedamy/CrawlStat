//! Accessibility tests.
//!
//! Each one is a markup pattern that either is or is not a real WCAG failure.
//! The negative cases matter as much as the positive: an accessibility checker
//! that cries wolf gets switched off.

use scraper::Html;

use super::*;

fn analyse_html(html: &str) -> AccessibilityReport {
    analyse(&Html::parse_document(html))
}

// ─── accessible names ────────────────────────────────────────────────────

#[test]
fn a_link_with_no_name_is_reported() {
    // Announced as just "link".
    let report = analyse_html(r#"<a href="/x"></a>"#);
    assert_eq!(report.links_without_name, 1);
}

#[test]
fn the_ways_a_link_can_be_named_are_all_accepted() {
    for markup in [
        r#"<a href="/x">Read more</a>"#,
        r#"<a href="/x" aria-label="Read the report"></a>"#,
        r#"<a href="/x" aria-labelledby="heading"></a><h2 id="heading">Report</h2>"#,
        r#"<a href="/x" title="Read the report"></a>"#,
        r#"<a href="/x"><img src="/i.png" alt="Read the report"></a>"#,
        // Hidden from assistive technology entirely, so not a failure.
        r#"<a href="/x" aria-hidden="true"></a>"#,
    ] {
        assert_eq!(
            analyse_html(markup).links_without_name,
            0,
            "flagged: {markup}"
        );
    }
}

#[test]
fn an_image_link_with_an_empty_alt_is_still_unnamed() {
    // A decorative alt on the *only* content of a link leaves it nameless.
    let report = analyse_html(r#"<a href="/x"><img src="/i.png" alt=""></a>"#);
    assert_eq!(report.links_without_name, 1);
}

#[test]
fn buttons_are_held_to_the_same_standard() {
    assert_eq!(analyse_html("<button></button>").buttons_without_name, 1);
    assert_eq!(
        analyse_html("<button>Send</button>").buttons_without_name,
        0
    );
    // An input button is named by its value.
    assert_eq!(
        analyse_html(r#"<input type="submit" value="Send">"#).buttons_without_name,
        0,
    );
    assert_eq!(
        analyse_html(r#"<input type="submit">"#).buttons_without_name,
        1
    );
}

// ─── form labels ─────────────────────────────────────────────────────────

#[test]
fn an_unlabelled_field_is_reported() {
    assert_eq!(
        analyse_html(r#"<input type="text" id="name">"#).inputs_without_label,
        1
    );
}

#[test]
fn every_legitimate_way_to_label_a_field_is_accepted() {
    for markup in [
        r#"<label for="name">Name</label><input id="name">"#,
        r#"<label>Name <input></label>"#,
        r#"<input aria-label="Name">"#,
        r#"<span id="l">Name</span><input aria-labelledby="l">"#,
        r#"<input title="Name">"#,
        r#"<select aria-label="Country"><option>UK</option></select>"#,
        r#"<label for="bio">Bio</label><textarea id="bio"></textarea>"#,
    ] {
        assert_eq!(
            analyse_html(markup).inputs_without_label,
            0,
            "flagged: {markup}"
        );
    }
}

#[test]
fn fields_that_take_no_label_are_not_reported() {
    // Hidden fields and buttons are not user-facing inputs.
    for markup in [
        r#"<input type="hidden" name="csrf" value="x">"#,
        r#"<input type="submit" value="Go">"#,
        r#"<input type="button" value="Go">"#,
        r#"<input aria-hidden="true">"#,
    ] {
        assert_eq!(
            analyse_html(markup).inputs_without_label,
            0,
            "flagged: {markup}"
        );
    }
}

#[test]
fn a_placeholder_alone_does_not_count_as_a_label() {
    // A placeholder disappears as soon as the user types, so it is not a name.
    let report = analyse_html(r#"<input type="text" placeholder="Your email">"#);
    assert_eq!(report.inputs_without_label, 1);
}

// ─── ids and ARIA references ─────────────────────────────────────────────

#[test]
fn duplicate_ids_are_counted_once_per_repeated_id() {
    let report =
        analyse_html(r#"<div id="a"></div><div id="a"></div><div id="a"></div><div id="b"></div>"#);
    assert_eq!(report.duplicate_ids, 1, "one id is duplicated, not three");
}

#[test]
fn unique_ids_are_fine() {
    assert_eq!(
        analyse_html(r#"<div id="a"></div><div id="b"></div>"#).duplicate_ids,
        0
    );
}

#[test]
fn an_aria_reference_to_a_missing_id_is_reported() {
    let report = analyse_html(r#"<input aria-labelledby="nowhere">"#);
    assert_eq!(report.broken_aria_references, 1);
    // ...and a resolved one is not.
    let report = analyse_html(r#"<span id="l">Name</span><input aria-labelledby="l">"#);
    assert_eq!(report.broken_aria_references, 0);
}

#[test]
fn each_broken_token_in_a_reference_list_counts() {
    // `aria-labelledby` takes several ids; each missing one loses part of the
    // name.
    let report =
        analyse_html(r#"<span id="a">A</span><input aria-labelledby="a missing1 missing2">"#);
    assert_eq!(report.broken_aria_references, 2);
}

#[test]
fn describedby_is_checked_as_well_as_labelledby() {
    let report = analyse_html(r#"<input aria-label="Name" aria-describedby="hint">"#);
    assert_eq!(report.broken_aria_references, 1);
}

// ─── roles ───────────────────────────────────────────────────────────────

#[test]
fn real_aria_roles_are_accepted() {
    for role in [
        "navigation",
        "button",
        "main",
        "tablist",
        "switch",
        "treegrid",
        "none",
    ] {
        assert!(is_valid_aria_role(role), "{role} should be valid");
    }
    // Case is not significant.
    assert!(is_valid_aria_role("Navigation"));
}

#[test]
fn a_misspelt_role_is_caught() {
    // The whole point: these look plausible and silently do nothing.
    for role in ["buton", "navigation-bar", "header", "clickable", ""] {
        assert!(!is_valid_aria_role(role), "{role} should be invalid");
    }

    let report = analyse_html(r#"<div role="buton"></div><nav role="navigation"></nav>"#);
    assert_eq!(report.invalid_roles, vec!["buton"]);
}

#[test]
fn a_fallback_role_list_is_valid_if_any_token_is() {
    // `role` takes a space-separated list; the first understood one wins.
    assert!(is_valid_aria_role("doc-chapter region"));
    assert!(!is_valid_aria_role("nonsense alsononsense"));
}

#[test]
fn the_same_bad_role_twice_is_reported_once() {
    let report = analyse_html(r#"<div role="buton"></div><div role="buton"></div>"#);
    assert_eq!(report.invalid_roles, vec!["buton"]);
}

// ─── keyboard and frames ─────────────────────────────────────────────────

#[test]
fn a_positive_tabindex_is_reported_and_zero_or_minus_one_are_not() {
    assert_eq!(
        analyse_html(r#"<div tabindex="3"></div>"#).positive_tabindex,
        1
    );
    // 0 puts an element in natural order; -1 makes it focusable only by script.
    assert_eq!(
        analyse_html(r#"<div tabindex="0"></div>"#).positive_tabindex,
        0
    );
    assert_eq!(
        analyse_html(r#"<div tabindex="-1"></div>"#).positive_tabindex,
        0
    );
    assert_eq!(
        analyse_html(r#"<div tabindex="nonsense"></div>"#).positive_tabindex,
        0
    );
}

#[test]
fn an_untitled_iframe_is_reported() {
    assert_eq!(
        analyse_html(r#"<iframe src="/x"></iframe>"#).iframes_without_title,
        1
    );
    assert_eq!(
        analyse_html(r#"<iframe src="/x" title="Map"></iframe>"#).iframes_without_title,
        0,
    );
    // Whitespace is not a title.
    assert_eq!(
        analyse_html(r#"<iframe src="/x" title="  "></iframe>"#).iframes_without_title,
        1,
    );
    // Hidden from the tree, so it needs no name.
    assert_eq!(
        analyse_html(r#"<iframe src="/x" aria-hidden="true"></iframe>"#).iframes_without_title,
        0,
    );
}

// ─── heading structure ───────────────────────────────────────────────────

#[test]
fn a_skipped_heading_level_is_reported() {
    let report = analyse_html("<h1>A</h1><h3>B</h3>");
    assert_eq!(report.skipped_heading_levels, 1);
    assert_eq!(report.heading_outline, vec![1, 3]);
}

#[test]
fn descending_one_level_at_a_time_is_correct() {
    let report = analyse_html("<h1>A</h1><h2>B</h2><h3>C</h3><h4>D</h4>");
    assert_eq!(report.skipped_heading_levels, 0);
}

#[test]
fn jumping_back_up_any_number_of_levels_is_fine() {
    // A new section at h2 after an h4 is normal document structure.
    let report = analyse_html("<h1>A</h1><h2>B</h2><h4>C</h4><h2>D</h2>");
    assert_eq!(
        report.skipped_heading_levels, 1,
        "only the h2 → h4 jump counts"
    );
}

#[test]
fn the_outline_is_recorded_in_document_order() {
    let report = analyse_html("<h2>A</h2><h1>B</h1><h3>C</h3>");
    assert_eq!(report.heading_outline, vec![2, 1, 3]);
}

// ─── the whole report ────────────────────────────────────────────────────

#[test]
fn a_clean_page_reports_nothing() {
    let report = analyse_html(
        r#"<html lang="en"><body>
             <nav role="navigation"><a href="/">Home</a></nav>
             <main>
               <h1>Title</h1>
               <h2>Section</h2>
               <form>
                 <label for="q">Search</label><input id="q">
                 <button type="submit">Go</button>
               </form>
               <a href="/x"><img src="/i.png" alt="A chart"></a>
               <iframe src="/embed" title="Embedded map"></iframe>
             </main>
           </body></html>"#,
    );

    assert!(report.is_clean(), "clean page reported: {report:?}");
    assert_eq!(report.issue_count(), 0);
}

#[test]
fn a_bad_page_reports_each_distinct_failure() {
    let report = analyse_html(
        r#"<body>
             <a href="/x"></a>
             <button></button>
             <input type="text">
             <div id="dup"></div><div id="dup"></div>
             <span aria-describedby="gone"></span>
             <div role="clickable" tabindex="4"></div>
             <iframe src="/y"></iframe>
             <h1>A</h1><h4>B</h4>
           </body>"#,
    );

    assert_eq!(report.links_without_name, 1);
    assert_eq!(report.buttons_without_name, 1);
    assert_eq!(report.inputs_without_label, 1);
    assert_eq!(report.duplicate_ids, 1);
    assert_eq!(report.broken_aria_references, 1);
    assert_eq!(report.invalid_roles, vec!["clickable"]);
    assert_eq!(report.positive_tabindex, 1);
    assert_eq!(report.iframes_without_title, 1);
    assert_eq!(report.skipped_heading_levels, 1);
    assert_eq!(report.issue_count(), 9);
}

#[test]
fn an_empty_or_broken_document_does_not_panic() {
    for markup in ["", "<html>", "not html at all", "<a href=/x><button><input"] {
        let report = analyse_html(markup);
        // Whatever it finds, it must not crash — malformed markup is normal.
        let _ = report.issue_count();
    }
}
