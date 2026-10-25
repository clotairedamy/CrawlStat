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

// ─── findings: the rules behind the compliance report ────────────────────

/// A complete, accessible page, so a test can add one fault and check that
/// exactly that fault is reported.
fn page(body: &str) -> String {
    format!(
        r#"<html lang="en"><head><title>Page</title>
           <meta name="viewport" content="width=device-width, initial-scale=1"></head>
           <body><main><h1>Title</h1>{body}</main></body></html>"#
    )
}

fn rules_on(body: &str) -> Vec<&'static str> {
    analyse_html(&page(body))
        .findings
        .iter()
        .map(|finding| finding.rule)
        .collect()
}

#[test]
fn the_accessible_template_page_has_no_findings() {
    assert_eq!(rules_on(""), Vec::<&str>::new());
}

#[test]
fn the_clean_page_has_no_findings_either() {
    let report = analyse_html(
        r##"<html lang="en-GB"><head><title>Home</title></head><body>
             <a href="#main">Skip to content</a>
             <nav><ul><li><a href="/">Home</a></li><li><a href="/about">About us</a></li></ul></nav>
             <main id="main">
               <h1>Title</h1>
               <form>
                 <label for="email">Email</label>
                 <input id="email" type="email" autocomplete="email">
                 <button type="submit" aria-label="Send the form">Send</button>
               </form>
               <img src="/divider.png" alt="">
               <table><tr><th>Plan</th><th>Price</th></tr><tr><td>Basic</td><td>£5</td></tr></table>
               <video controls><source src="/v.mp4"><track kind="captions" src="/v.vtt"></video>
             </main>
           </body></html>"##,
    );
    assert_eq!(report.findings, Vec::new(), "{:#?}", report.findings);
}

#[test]
fn each_original_check_also_produces_a_finding_with_a_sample() {
    let report = analyse_html(&page(r#"<a href="/x"></a>"#));
    let finding = report.finding(rule::LINK_NO_NAME).unwrap();
    assert_eq!(finding.count, 1);
    assert_eq!(finding.samples, vec![r#"<a href="/x"></a>"#]);
}

#[test]
fn samples_are_capped_and_distinct_but_every_instance_is_counted() {
    let links: String = (0..6).map(|i| format!(r#"<a href="/{i}"></a>"#)).collect();
    let report = analyse_html(&page(&format!("{links}<a href=\"/0\"></a>")));
    let finding = report.finding(rule::LINK_NO_NAME).unwrap();
    assert_eq!(finding.count, 7);
    assert_eq!(finding.samples.len(), MAX_SAMPLES);
}

#[test]
fn a_snippet_shows_the_child_of_an_empty_element() {
    let report = analyse_html(&page(r#"<a href="/x"><img src="/i.png" alt=""></a>"#));
    assert_eq!(
        report.finding(rule::LINK_NO_NAME).unwrap().samples,
        // Attributes come out in scraper's stable order, not source order.
        vec![r#"<a href="/x"><img alt="" src="/i.png"></a>"#]
    );
}

#[test]
fn template_keys_ignore_the_numbers_that_differ_between_pages() {
    assert_eq!(
        template_key(r#"<a href="/post/12"></a>"#),
        template_key(r#"<a href="/post/9781"></a>"#)
    );
    assert_ne!(template_key("<a href=\"/x\">"), template_key("<a href=\"/y\">"));
}

#[test]
fn a_div_pretending_to_be_a_button_needs_a_name_and_a_tabindex() {
    assert_eq!(
        rules_on(r#"<div role="button"></div>"#),
        vec![rule::BUTTON_NO_NAME, rule::ROLE_NOT_FOCUSABLE]
    );
    assert_eq!(rules_on(r#"<div role="button" tabindex="0">Save</div>"#), Vec::<&str>::new());
    // Widgets managed by their container need no tabindex of their own.
    assert_eq!(
        rules_on(r#"<ul role="listbox" tabindex="0" aria-label="Fruit"><li role="option">Apple</li></ul>"#),
        Vec::<&str>::new()
    );
}

#[test]
fn a_missing_alt_fails_but_an_empty_alt_is_a_decision() {
    assert_eq!(rules_on(r#"<img src="/a.png">"#), vec![rule::IMG_NO_ALT]);
    for markup in [
        r#"<img src="/a.png" alt="">"#,
        r#"<img src="/a.png" alt="A chart">"#,
        r#"<img src="/a.png" role="presentation">"#,
        r#"<img src="/a.png" aria-hidden="true">"#,
        r#"<svg role="img" aria-label="Logo"></svg>"#,
        r#"<svg role="img"><title>Logo</title></svg>"#,
        r#"<input type="image" src="/go.png" alt="Search">"#,
    ] {
        assert_eq!(rules_on(markup), Vec::<&str>::new(), "flagged: {markup}");
    }
    assert_eq!(rules_on(r#"<svg role="img"></svg>"#), vec![rule::IMG_NO_ALT]);
    assert_eq!(rules_on(r#"<input type="image" src="/go.png">"#), vec![rule::IMG_NO_ALT]);
}

#[test]
fn a_viewport_that_blocks_zoom_is_caught() {
    for content in [
        "width=device-width, user-scalable=no",
        "width=device-width, maximum-scale=1",
        "width=device-width,maximum-scale=1.5",
        "user-scalable=0",
    ] {
        assert!(viewport_blocks_zoom(content), "missed: {content}");
    }
    for content in ["width=device-width, initial-scale=1", "maximum-scale=5", "user-scalable=yes"] {
        assert!(!viewport_blocks_zoom(content), "flagged: {content}");
    }
}

#[test]
fn page_level_checks_need_a_title_a_lang_and_a_way_past_the_header() {
    let report = analyse_html("<body><p>Hello</p></body>");
    for expected in [rule::PAGE_NO_LANG, rule::PAGE_NO_TITLE, rule::NO_BYPASS] {
        assert!(report.finding(expected).is_some(), "missing {expected}");
    }
    // A skip link alone is a way past the header.
    let report = analyse_html(r##"<body><a href="#content">Skip</a><p>Hi</p></body>"##);
    assert!(report.finding(rule::NO_BYPASS).is_none());
}

#[test]
fn an_invalid_language_is_caught_on_the_page_and_on_parts() {
    assert!(is_valid_lang("en"));
    assert!(is_valid_lang("en-GB"));
    assert!(is_valid_lang("zh-Hant-TW"));
    assert!(is_valid_lang("haw"));
    for bad in ["english", "en_US", "xx", "e"] {
        assert!(!is_valid_lang(bad), "{bad} should be invalid");
    }
    let report = analyse_html(r#"<html lang="english"><body><p lang="fr-FR">Bonjour</p><p lang="">?</p></body></html>"#);
    assert_eq!(report.count(rule::LANG_INVALID), 1);
}

#[test]
fn a_timed_refresh_fails_but_an_instant_redirect_does_not() {
    let report = analyse_html(r#"<head><meta http-equiv="refresh" content="30"></head>"#);
    assert_eq!(report.count(rule::META_REFRESH), 1);
    let report = analyse_html(r#"<head><meta http-equiv="Refresh" content="0; url=/new"></head>"#);
    assert_eq!(report.count(rule::META_REFRESH), 0);
}

#[test]
fn media_checks() {
    assert_eq!(
        rules_on(r#"<video src="/v.mp4" autoplay></video>"#),
        vec![rule::AUTOPLAY_AUDIO, rule::VIDEO_NO_CAPTIONS]
    );
    // Muted background video: nothing to hear, nothing to caption.
    assert_eq!(rules_on(r#"<video src="/v.mp4" autoplay muted loop></video>"#), Vec::<&str>::new());
    assert_eq!(rules_on("<marquee>Sale!</marquee>"), vec![rule::BLINK_MARQUEE]);
}

#[test]
fn features_record_what_the_page_contains() {
    let report = analyse_html(&page(
        r#"<iframe title="Intro" src="https://www.youtube.com/embed/x"></iframe>
           <form><label>Password <input type="password"></label></form>"#,
    ));
    assert!(report.has_feature(feature::VIDEO));
    assert!(report.has_feature(feature::FORM));
    assert!(report.has_feature(feature::PASSWORD));
    assert!(!report.has_feature(feature::AUDIO));
}

#[test]
fn focusable_content_inside_aria_hidden_is_caught() {
    assert_eq!(
        rules_on(r#"<div aria-hidden="true"><a href="/x">Pricing</a></div>"#),
        vec![rule::HIDDEN_FOCUSABLE]
    );
    assert_eq!(
        rules_on(r#"<div aria-hidden="true"><a href="/x" tabindex="-1">Pricing</a></div>"#),
        Vec::<&str>::new()
    );
    // Nested hidden regions are reported once, at the outermost.
    assert_eq!(
        analyse_html(&page(r#"<div aria-hidden="true"><div aria-hidden="true"><button>Go</button></div></div>"#))
            .count(rule::HIDDEN_FOCUSABLE),
        1
    );
}

#[test]
fn interactive_elements_inside_each_other_are_caught() {
    assert_eq!(
        rules_on(r#"<a href="/x">Card <button>Buy</button></a>"#),
        vec![rule::NESTED_INTERACTIVE]
    );
}

#[test]
fn an_empty_heading_is_caught() {
    assert_eq!(rules_on("<h2></h2>"), vec![rule::EMPTY_HEADING]);
    assert_eq!(rules_on(r#"<h2><img src="/l.png" alt="Pricing"></h2>"#), Vec::<&str>::new());
}

#[test]
fn vague_link_text_is_flagged_unless_the_name_explains_it() {
    assert_eq!(rules_on(r#"<a href="/x">Read more</a>"#), vec![rule::VAGUE_LINK_TEXT]);
    assert_eq!(
        rules_on(r#"<a href="/x" aria-label="Read more about pricing">Read more</a>"#),
        Vec::<&str>::new()
    );
}

#[test]
fn a_visible_label_missing_from_the_accessible_name_is_flagged() {
    assert_eq!(
        rules_on(r#"<button aria-label="Submit form">Send</button>"#),
        vec![rule::LABEL_NOT_IN_NAME]
    );
    assert_eq!(rules_on(r#"<button aria-label="Close dialog">×</button>"#), Vec::<&str>::new());
    assert_eq!(rules_on(r#"<button aria-label="Send message">Send</button>"#), Vec::<&str>::new());
}

#[test]
fn autocomplete_values_are_validated() {
    for good in ["email", "on", "off", "shipping postal-code", "section-a billing tel", "username webauthn"] {
        assert!(is_valid_autocomplete(good), "{good} should be valid");
    }
    for bad in ["nope", "false", "first-name", "email shipping"] {
        assert!(!is_valid_autocomplete(bad), "{bad} should be invalid");
    }
    assert_eq!(
        rules_on(r#"<label>Email <input type="email" autocomplete="nope"></label>"#),
        vec![rule::INVALID_AUTOCOMPLETE]
    );
    assert_eq!(
        rules_on(r#"<label>Email <input type="email"></label>"#),
        vec![rule::MISSING_AUTOCOMPLETE]
    );
    // A search box is not personal data.
    assert_eq!(rules_on(r#"<label>Search <input name="q"></label>"#), Vec::<&str>::new());
}

#[test]
fn list_structure_is_checked_both_ways() {
    assert_eq!(rules_on("<ul><div>Item</div></ul>"), vec![rule::LIST_STRUCTURE]);
    assert_eq!(rules_on("<div><li>Orphan</li></div>"), vec![rule::LIST_STRUCTURE]);
    assert_eq!(rules_on("<ol><li>One</li><li>Two</li></ol>"), Vec::<&str>::new());
}

#[test]
fn a_data_table_without_headers_is_flagged_for_review() {
    assert_eq!(
        rules_on("<table><tr><td>a</td><td>b</td></tr><tr><td>c</td><td>d</td></tr></table>"),
        vec![rule::TABLE_NO_HEADERS]
    );
    assert_eq!(
        rules_on(r#"<table role="presentation"><tr><td>a</td><td>b</td></tr><tr><td>c</td><td>d</td></tr></table>"#),
        Vec::<&str>::new()
    );
}

#[test]
fn removing_the_focus_ring_is_flagged_unless_replaced() {
    let flagged = |css: &str| {
        analyse_html(&format!("<html><head><style>{css}</style></head></html>"))
            .count(rule::FOCUS_OUTLINE_REMOVED)
    };
    assert_eq!(flagged("a:focus { outline: none }"), 1);
    assert_eq!(flagged("button:focus{outline:0;}"), 1);
    assert_eq!(flagged(":focus:not(:focus-visible) { outline: none }"), 0);
    assert_eq!(flagged("a:focus { outline: none; box-shadow: 0 0 0 3px blue }"), 0);
    assert_eq!(flagged("a:hover { outline: none }"), 0);
}

#[test]
fn a_click_handler_on_a_plain_element_is_flagged_for_review() {
    assert_eq!(
        rules_on(r#"<div onclick="go()">Open</div>"#),
        vec![rule::CLICK_HANDLER_NOT_FOCUSABLE]
    );
    assert_eq!(rules_on(r#"<button onclick="go()">Open</button>"#), Vec::<&str>::new());
}

#[test]
fn a_referenced_duplicate_id_is_worse_than_an_unreferenced_one() {
    let report = analyse_html(&page(
        r#"<label for="email">Email</label><input id="email" autocomplete="email">
           <input id="email" aria-label="Confirm" autocomplete="email">
           <div id="card"></div><div id="card"></div>"#,
    ));
    assert_eq!(report.count(rule::DUPLICATE_ID_REFERENCED), 1);
    assert_eq!(report.count(rule::DUPLICATE_ID), 1);
    assert_eq!(report.duplicate_ids, 2, "the original counter counts both");
}

#[test]
fn every_rule_listed_is_unique() {
    let mut seen = std::collections::HashSet::new();
    for id in rule::ALL {
        assert!(seen.insert(id), "{id} listed twice");
    }
}
