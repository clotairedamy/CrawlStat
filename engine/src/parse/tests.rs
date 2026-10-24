//! Parser tests. The fixture below is deliberately scruffy: unclosed tags,
//! mixed-case attributes, a `<base href>`, lazy images and an invalid JSON-LD
//! block, because that is what real pages look like.

use url::Url;

use super::parse_html;
use crate::model::{LinkPosition, LinkSource};
use crate::url_norm::NormalizationPolicy;

const PAGE: &str = r##"<!DOCTYPE html>
<html lang="en-GB">
<head>
  <meta charset="UTF-8">
  <title>  Widgets &amp; Gadgets
     — Acme </title>
  <meta name="Description" content="  Buy widgets   and gadgets. ">
  <meta name="robots" content="index, follow, max-snippet:-1">
  <meta name="viewport" content="width=device-width">
  <base href="https://acme.test/shop/">
  <link rel="canonical" href="/shop/widgets">
  <link rel="alternate" hreflang="fr-FR" href="https://acme.test/fr/widgets">
  <link rel="alternate" hreflang="x-default" href="https://acme.test/widgets">
  <link rel="stylesheet" href="../css/site.css">
  <link rel="amphtml" href="/amp/widgets">
  <meta property="og:title" content="Widgets">
  <meta property="og:image" content="https://acme.test/og.png">
  <meta name="twitter:card" content="summary_large_image">
  <script type="application/ld+json">
    {"@context":"https://schema.org","@type":"Product",
     "offers":{"@type":"Offer","price":"9.99"}}
  </script>
  <script type="application/ld+json">{ broken json </script>
  <script>var tracking = "this text must not be counted";</script>
  <style>body { color: red }</style>
</head>
<body>
  <header><a href="/">Home</a></header>
  <nav aria-label="main"><a href="widgets">Widgets</a><a href="gadgets/">Gadgets</a></nav>
  <main>
    <h1>Widgets</h1>
    <h2>Small widgets</h2>
    <h2>Large widgets</h2>
    <p>The <b>best</b> widgets on the market.</p>
    <p>Second paragraph here.</p>
    <a href="/spam" rel="NOFOLLOW ugc">Spam</a>
    <a href="https://external.test/page">External</a>
    <a href="mailto:hi@acme.test">Mail us</a>
    <a href="javascript:void(0)">Menu</a>
    <a href="#top"></a>
    <a href="/widget-1"><img src="/img/w1.png" alt="Widget one" width="800" height="600"></a>
    <img src="/img/no-alt.png">
    <img alt="" src="/img/decorative.png" loading="lazy">
    <img data-src="/img/lazy.png" alt="Lazy">
    <div itemscope itemtype="https://schema.org/BreadcrumbList"></div>
    <noscript>Enable JavaScript to count this, which we must not.</noscript>
  </main>
  <aside><a href="/related">Related</a></aside>
  <footer><a href="/privacy">Privacy</a><nav><a href="/sitemap">Sitemap</a></nav></footer>
  <iframe src="https://player.test/embed"></iframe>
  <form action="/search"><input name="q"></form>
  <script src="/js/app.js"></script>
</body>
</html>"##;

fn parsed() -> crate::model::ParsedPage {
    let url = Url::parse("https://acme.test/shop/widgets").unwrap();
    parse_html(PAGE, &url, &NormalizationPolicy::default())
}

#[test]
fn head_fields() {
    let p = parsed();
    assert_eq!(p.title.as_deref(), Some("Widgets & Gadgets — Acme"));
    assert_eq!(
        p.meta_description.as_deref(),
        Some("Buy widgets and gadgets.")
    );
    assert_eq!(
        p.meta_robots.as_deref(),
        Some("index, follow, max-snippet:-1")
    );
    assert_eq!(p.html_lang.as_deref(), Some("en-GB"));
    assert_eq!(p.charset.as_deref(), Some("utf-8"));
    assert!(p.has_viewport);
    assert_eq!(p.amp_html.as_deref(), Some("https://acme.test/amp/widgets"));
}

#[test]
fn canonical_resolves_and_keeps_the_raw_value() {
    let p = parsed();
    assert_eq!(p.canonical_raw.as_deref(), Some("/shop/widgets"));
    assert_eq!(
        p.canonical.as_deref(),
        Some("https://acme.test/shop/widgets")
    );
}

#[test]
fn base_href_governs_relative_resolution() {
    let p = parsed();
    // "widgets" resolves against <base href="/shop/">, not against the page's
    // own directory (which happens to be the same here) nor the site root.
    let target = |raw: &str| {
        p.links
            .iter()
            .find(|l| l.raw_href == raw)
            .and_then(|l| l.target.as_deref())
            .unwrap()
    };
    assert_eq!(target("widgets"), "https://acme.test/shop/widgets");
    assert_eq!(target("gadgets/"), "https://acme.test/shop/gadgets/");
    assert_eq!(
        target("/"),
        "https://acme.test/",
        "a root-relative href ignores the base path"
    );
    // And so does the stylesheet's "../css/site.css".
    assert!(p.links.iter().any(|l| l.source == LinkSource::Stylesheet
        && l.target.as_deref() == Some("https://acme.test/css/site.css")));
}

#[test]
fn headings() {
    let p = parsed();
    assert_eq!(p.h1, vec!["Widgets"]);
    assert_eq!(p.h2, vec!["Small widgets", "Large widgets"]);
}

#[test]
fn hreflang_entries() {
    let p = parsed();
    assert_eq!(p.hreflang.len(), 2);
    assert_eq!(p.hreflang[0].lang, "fr-FR");
    assert_eq!(
        p.hreflang[0].href.as_deref(),
        Some("https://acme.test/fr/widgets")
    );
    assert_eq!(p.hreflang[1].lang, "x-default");
}

#[test]
fn social_tags() {
    let p = parsed();
    let og: Vec<_> = p.social.iter().filter(|t| t.namespace == "og").collect();
    assert_eq!(og.len(), 2);
    assert_eq!(og[0].property, "title");
    assert_eq!(og[0].content, "Widgets");
    assert!(p
        .social
        .iter()
        .any(|t| t.namespace == "twitter" && t.property == "card"));
}

#[test]
fn structured_data_valid_and_invalid_blocks() {
    let p = parsed();
    assert_eq!(
        p.structured_data.len(),
        3,
        "two JSON-LD blocks plus microdata"
    );
    assert_eq!(
        p.structured_data_types(),
        vec!["BreadcrumbList", "Offer", "Product"]
    );
    assert!(
        p.structured_data.iter().any(|b| !b.valid),
        "the broken block is recorded"
    );
}

#[test]
fn unsupported_schemes_are_recorded_without_a_target() {
    let p = parsed();
    let mailto = p
        .links
        .iter()
        .find(|l| l.raw_href.starts_with("mailto:"))
        .unwrap();
    assert!(mailto.target.is_none());
    assert!(p
        .links
        .iter()
        .any(|l| l.raw_href.starts_with("javascript:") && l.target.is_none()));
}

#[test]
fn nofollow_and_rel_are_captured() {
    let p = parsed();
    let spam = p.links.iter().find(|l| l.raw_href == "/spam").unwrap();
    assert!(spam.nofollow, "rel is matched case-insensitively");
    assert_eq!(spam.rel.as_deref(), Some("NOFOLLOW ugc"));

    let external = p
        .links
        .iter()
        .find(|l| l.raw_href == "https://external.test/page")
        .unwrap();
    assert!(!external.nofollow);
}

#[test]
fn link_positions_use_the_innermost_semantic_ancestor() {
    let p = parsed();
    let position = |href: &str| {
        p.links
            .iter()
            .find(|l| l.raw_href == href)
            .map(|l| l.position)
            .unwrap()
    };
    assert_eq!(position("/"), LinkPosition::Header);
    assert_eq!(position("widgets"), LinkPosition::Navigation);
    assert_eq!(position("/spam"), LinkPosition::Content);
    assert_eq!(position("/related"), LinkPosition::Aside);
    assert_eq!(position("/privacy"), LinkPosition::Footer);
    // A <nav> inside the <footer>: the nearest ancestor wins.
    assert_eq!(position("/sitemap"), LinkPosition::Navigation);
}

#[test]
fn fragment_only_links_point_back_at_the_page() {
    let p = parsed();
    let anchor = p.links.iter().find(|l| l.raw_href == "#top").unwrap();
    // Per RFC 3986 a bare fragment resolves against the *base* URL, which
    // `<base href="/shop/">` has overridden here.
    assert_eq!(anchor.target.as_deref(), Some("https://acme.test/shop/"));
    assert_eq!(anchor.anchor_text, "");
}

#[test]
fn an_image_link_borrows_the_alt_text() {
    let p = parsed();
    let link = p.links.iter().find(|l| l.raw_href == "/widget-1").unwrap();
    assert_eq!(link.anchor_text, "Widget one");
}

#[test]
fn images_distinguish_missing_from_empty_alt() {
    let p = parsed();
    assert_eq!(p.images.len(), 4);

    let by_src = |needle: &str| {
        p.images
            .iter()
            .find(|i| i.raw_src.contains(needle))
            .unwrap()
    };

    let w1 = by_src("w1.png");
    assert_eq!(w1.alt.as_deref(), Some("Widget one"));
    assert_eq!(w1.declared_width, Some(800));
    assert_eq!(w1.declared_height, Some(600));

    assert_eq!(by_src("no-alt").alt, None, "a missing alt attribute");
    assert_eq!(
        by_src("decorative").alt.as_deref(),
        Some(""),
        "a deliberately empty alt"
    );
    assert!(by_src("decorative").loading_lazy);

    // Lazy-loaded images hide the URL in data-src.
    assert_eq!(
        by_src("lazy.png").src.as_deref(),
        Some("https://acme.test/img/lazy.png")
    );
}

#[test]
fn assets_and_embeds_are_recorded_with_their_source_kind() {
    let p = parsed();
    let has = |source, needle: &str| {
        p.links
            .iter()
            .any(|l| l.source == source && l.target.as_deref().is_some_and(|t| t.contains(needle)))
    };
    assert!(has(LinkSource::Script, "/js/app.js"));
    assert!(has(LinkSource::Iframe, "player.test/embed"));
    assert!(has(LinkSource::Form, "/search"));
    assert!(has(LinkSource::Image, "/img/w1.png"));
    assert!(has(LinkSource::Canonical, "/shop/widgets"));
    assert!(has(LinkSource::Hreflang, "/fr/widgets"));
}

#[test]
fn word_count_ignores_scripts_styles_and_noscript() {
    let p = parsed();
    let text_words = "Home Widgets Gadgets Widgets Small widgets Large widgets \
                      The best widgets on the market. Second paragraph here. Spam External \
                      Mail us Menu Related Privacy Sitemap";
    assert_eq!(p.word_count, crate::hash::word_count(text_words));
    assert!(p.text_hash.is_some());
    assert!(p.text_simhash.is_some());
}

#[test]
fn inline_tags_do_not_split_words() {
    let url = Url::parse("https://acme.test/").unwrap();
    let p = |body: &str| parse_html(body, &url, &NormalizationPolicy::default());
    assert_eq!(p("<body><p>hel<b>lo</b> world</p></body>").word_count, 2);
    assert_eq!(p("<body><p>one</p><p>two</p></body>").word_count, 2);
    // Without a block boundary these would read as "onetwo".
    assert_eq!(p("<body><div>one</div><div>two</div></body>").word_count, 2);
}

#[test]
fn internal_and_external_anchor_counts() {
    let p = parsed();
    let (internal, external) = p.link_counts(|u| u.starts_with("https://acme.test"));
    assert_eq!(external, 1, "only external.test");
    assert_eq!(
        internal, 9,
        "every acme.test anchor, including the fragment link"
    );
}

#[test]
fn empty_and_broken_documents_parse_to_empty_fields() {
    let url = Url::parse("https://acme.test/").unwrap();
    let policy = NormalizationPolicy::default();

    let empty = parse_html("", &url, &policy);
    assert_eq!(empty.title, None);
    assert_eq!(empty.word_count, 0);
    assert!(empty.links.is_empty());
    assert_eq!(empty.text_hash, None);

    // Unclosed tags, no head, stray attributes: html5ever recovers, we do too.
    let broken = parse_html(
        "<p>hello <a href=/a>link<div><h1>Title</p></h1><img src=x.png",
        &url,
        &policy,
    );
    assert_eq!(broken.h1, vec!["Title"]);
    assert_eq!(broken.links.len(), 2, "the anchor and the image");
    assert!(broken.word_count >= 2);
}

#[test]
fn meta_refresh_target_is_followed() {
    let url = Url::parse("https://acme.test/old").unwrap();
    let p = parse_html(
        r#"<meta http-equiv="refresh" content="0; url=/new"><body>x</body>"#,
        &url,
        &NormalizationPolicy::default(),
    );
    assert_eq!(p.meta_refresh.as_deref(), Some("0; url=/new"));
    let link = p
        .links
        .iter()
        .find(|l| l.source == LinkSource::MetaRefresh)
        .unwrap();
    assert_eq!(link.target.as_deref(), Some("https://acme.test/new"));
}

#[test]
fn meta_refresh_parsing_edge_cases() {
    use super::parse_meta_refresh_target as t;
    assert_eq!(t("0; url=/next").as_deref(), Some("/next"));
    assert_eq!(t("5;URL='/quoted'").as_deref(), Some("/quoted"));
    assert_eq!(
        t("0; url=\"https://a.test/x\"").as_deref(),
        Some("https://a.test/x")
    );
    assert_eq!(t("30"), None, "a plain page refresh is not a redirect");
    assert_eq!(t("0; url="), None);
}

#[test]
fn multiple_h1s_are_all_reported() {
    let url = Url::parse("https://acme.test/").unwrap();
    let p = parse_html(
        "<body><h1>One</h1><h1>Two</h1></body>",
        &url,
        &NormalizationPolicy::default(),
    );
    assert_eq!(p.h1, vec!["One", "Two"]);
}
