//! HTML extraction.
//!
//! One pass over a parsed document produces everything the crawler records
//! about a page: the SEO head fields, headings, visible text (for word count and
//! duplicate detection), links, images, hreflang, social tags and structured
//! data.
//!
//! The parser is **infallible**. Malformed markup is normal on the web, and a
//! page we cannot understand still has a status code, headers and a size worth
//! keeping, so every field simply degrades to empty.

use std::sync::LazyLock;

use scraper::{ElementRef, Html, Selector};
use url::Url;

use super::structured::{microdata_block, parse_json_ld};
use crate::hash;
use crate::model::{
    DiscoveredImage, DiscoveredLink, HreflangEntry, LinkPosition, LinkSource, ParsedPage, SocialTag,
};
use crate::url_norm::{self, NormalizationPolicy};

/// Elements whose text is never page content.
const NON_CONTENT_TAGS: &[&str] = &[
    "script", "style", "noscript", "template", "svg", "iframe", "object", "canvas", "head",
];

/// Inline elements, which do not introduce a word boundary. `<b>hel</b>lo` is
/// one word; `<p>a</p><p>b</p>` is two.
///
/// `<a>` is deliberately *not* here. A browser renders
/// `<a>Widgets</a><a>Gadgets</a>` as "WidgetsGadgets", but counting that as one
/// word would corrupt both the word count and the duplicate-detection hash of
/// every page with a navigation menu.
const INLINE_TAGS: &[&str] = &[
    "abbr", "b", "bdi", "bdo", "cite", "code", "data", "del", "dfn", "em", "i", "ins", "kbd",
    "label", "mark", "q", "rp", "rt", "ruby", "s", "samp", "small", "span", "strong", "sub", "sup",
    "time", "u", "var", "wbr", "font",
];

macro_rules! selector {
    ($name:ident, $sel:expr) => {
        static $name: LazyLock<Selector> = LazyLock::new(|| {
            Selector::parse($sel).expect(concat!("built-in selector is valid: ", $sel))
        });
    };
}

selector!(SEL_HTML, "html");
selector!(SEL_BASE, "base[href]");
selector!(SEL_TITLE, "title");
selector!(SEL_META, "meta");
selector!(SEL_LINK, "link[href]");
selector!(SEL_H1, "h1");
selector!(SEL_H2, "h2");
selector!(SEL_A, "a[href]");
selector!(SEL_IMG, "img");
selector!(SEL_SCRIPT, "script");
selector!(SEL_IFRAME, "iframe[src]");
selector!(SEL_FORM, "form[action]");
selector!(SEL_BODY, "body");
selector!(SEL_ITEMTYPE, "[itemtype]");

/// Extract everything we record about an HTML document.
///
/// `page_url` is the URL the body was served from, used to resolve relative
/// references (overridden by `<base href>` when present).
pub fn parse_html(body: &str, page_url: &Url, policy: &NormalizationPolicy) -> ParsedPage {
    let doc = Html::parse_document(body);
    let mut page = ParsedPage::default();

    // `<base href>` changes how every relative URL on the page resolves.
    let base = doc
        .select(&SEL_BASE)
        .next()
        .and_then(|el| el.value().attr("href"))
        .and_then(|href| page_url.join(href).ok())
        .unwrap_or_else(|| page_url.clone());

    let resolve = |raw: &str| -> (Option<String>, Option<String>) {
        match url_norm::normalize(raw, Some(&base), policy) {
            Ok(n) => (Some(n.url.to_string()), Some(n.key)),
            Err(_) => (None, None),
        }
    };

    page.html_lang = doc
        .select(&SEL_HTML)
        .next()
        .and_then(|el| el.value().attr("lang"))
        .map(collapse_ws)
        .filter(|s| !s.is_empty());

    page.title = doc
        .select(&SEL_TITLE)
        .next()
        .map(|el| collapse_ws(&element_text(el)));

    extract_meta(&doc, &mut page);
    extract_link_elements(&doc, &mut page, &resolve);

    page.h1 = doc
        .select(&SEL_H1)
        .map(|el| collapse_ws(&element_text(el)))
        .collect();
    page.h2 = doc
        .select(&SEL_H2)
        .map(|el| collapse_ws(&element_text(el)))
        .collect();

    extract_anchors(&doc, &mut page, &resolve);
    extract_images(&doc, &mut page, &resolve);
    extract_other_references(&doc, &mut page, &resolve);
    extract_structured_data(&doc, &mut page);
    page.accessibility = super::accessibility::analyse(&doc);

    // Visible text drives the word count and both duplicate-detection hashes.
    let text = match doc.select(&SEL_BODY).next() {
        Some(body_el) => visible_text(body_el),
        // No <body>: fall back to the whole document (fragments, error pages).
        None => doc
            .root_element()
            .select(&SEL_HTML)
            .next()
            .map(visible_text)
            .unwrap_or_default(),
    };
    page.word_count = hash::word_count(&text);
    if page.word_count > 0 {
        page.text_hash = Some(hash::text_hash(&text));
        page.text_simhash = hash::simhash(&text);
    }

    page
}

fn extract_meta(doc: &Html, page: &mut ParsedPage) {
    for el in doc.select(&SEL_META) {
        let attrs = el.value();
        let content = attrs.attr("content").map(collapse_ws);

        // <meta charset="utf-8">
        if let Some(charset) = attrs.attr("charset") {
            page.charset = Some(charset.trim().to_ascii_lowercase());
        }

        if let Some(name) = attrs.attr("name") {
            let name = name.trim().to_ascii_lowercase();
            match name.as_str() {
                "description" => page.meta_description = content.clone(),
                "robots" => page.meta_robots = content.clone(),
                "viewport" => page.has_viewport = true,
                _ if name.starts_with("twitter:") => {
                    if let Some(content) = content.clone() {
                        page.social.push(SocialTag {
                            namespace: "twitter".into(),
                            property: name.trim_start_matches("twitter:").to_string(),
                            content,
                        });
                    }
                }
                _ => {}
            }
        }

        // Open Graph uses `property`, though plenty of sites write `name`.
        if let Some(property) = attrs.attr("property").or_else(|| attrs.attr("name")) {
            let property = property.trim().to_ascii_lowercase();
            if let Some(rest) = property.strip_prefix("og:") {
                if let Some(content) = content.clone() {
                    page.social.push(SocialTag {
                        namespace: "og".into(),
                        property: rest.to_string(),
                        content,
                    });
                }
            }
        }

        if let Some(equiv) = attrs.attr("http-equiv") {
            match equiv.trim().to_ascii_lowercase().as_str() {
                "refresh" => page.meta_refresh = content.clone(),
                "content-type" => {
                    if let Some(c) = &content {
                        if let Some((_, charset)) = c.to_ascii_lowercase().split_once("charset=") {
                            page.charset = Some(charset.trim().to_string());
                        }
                    }
                }
                _ => {}
            }
        }
    }
}

fn extract_link_elements(
    doc: &Html,
    page: &mut ParsedPage,
    resolve: &impl Fn(&str) -> (Option<String>, Option<String>),
) {
    for el in doc.select(&SEL_LINK) {
        let attrs = el.value();
        let Some(href) = attrs.attr("href") else {
            continue;
        };
        let rels: Vec<String> = attrs
            .attr("rel")
            .unwrap_or("")
            .split_whitespace()
            .map(|r| r.to_ascii_lowercase())
            .collect();

        if rels.iter().any(|r| r == "canonical") && page.canonical_raw.is_none() {
            let (url, key) = resolve(href);
            page.canonical_raw = Some(href.trim().to_string());
            page.canonical = url.clone();
            page.canonical_key = key.clone();
            if let (Some(url), Some(key)) = (url, key) {
                page.links
                    .push(reference_link(href, url, key, LinkSource::Canonical));
            }
        }

        if rels.iter().any(|r| r == "alternate") {
            if let Some(lang) = attrs.attr("hreflang") {
                let (url, key) = resolve(href);
                page.hreflang.push(HreflangEntry {
                    lang: lang.trim().to_string(),
                    raw_href: href.trim().to_string(),
                    href: url.clone(),
                    href_key: key.clone(),
                });
                if let (Some(url), Some(key)) = (url, key) {
                    page.links
                        .push(reference_link(href, url, key, LinkSource::Hreflang));
                }
            }
            if attrs
                .attr("type")
                .is_some_and(|t| t.eq_ignore_ascii_case("text/html"))
                && attrs.attr("media").is_some()
            {
                // Mobile alternate; captured as a plain reference below.
            }
        }

        if rels.iter().any(|r| r == "amphtml") {
            page.amp_html = resolve(href).0;
        }

        if rels.iter().any(|r| r == "stylesheet") {
            if let (Some(url), Some(key)) = resolve(href) {
                page.links
                    .push(reference_link(href, url, key, LinkSource::Stylesheet));
            }
        }
    }
}

/// A link that is not a user-visible anchor: canonical, hreflang, asset, ...
fn reference_link(raw: &str, url: String, key: String, source: LinkSource) -> DiscoveredLink {
    DiscoveredLink {
        raw_href: raw.trim().to_string(),
        target: Some(url),
        target_key: Some(key),
        anchor_text: String::new(),
        rel: None,
        position: LinkPosition::Unknown,
        nofollow: false,
        source,
        // Scope is decided by the engine, which knows the crawl's root host.
        is_internal: false,
    }
}

fn extract_anchors(
    doc: &Html,
    page: &mut ParsedPage,
    resolve: &impl Fn(&str) -> (Option<String>, Option<String>),
) {
    for el in doc.select(&SEL_A) {
        let Some(href) = el.value().attr("href") else {
            continue;
        };
        let rel = el
            .value()
            .attr("rel")
            .map(collapse_ws)
            .filter(|r| !r.is_empty());
        let nofollow = rel
            .as_deref()
            .unwrap_or("")
            .split_whitespace()
            .any(|token| token.eq_ignore_ascii_case("nofollow"));

        // An image-only link has no text of its own; the image's alt text is
        // what a screen reader and a search engine see.
        let mut anchor_text = collapse_ws(&element_text(el));
        if anchor_text.is_empty() {
            if let Some(alt) = el
                .select(&SEL_IMG)
                .next()
                .and_then(|img| img.value().attr("alt"))
                .map(collapse_ws)
                .filter(|a| !a.is_empty())
            {
                anchor_text = alt;
            }
        }

        let (target, target_key) = resolve(href);
        page.links.push(DiscoveredLink {
            raw_href: href.trim().to_string(),
            target,
            target_key,
            anchor_text,
            rel,
            position: link_position(el),
            nofollow,
            source: LinkSource::Anchor,
            is_internal: false,
        });
    }
}

fn extract_images(
    doc: &Html,
    page: &mut ParsedPage,
    resolve: &impl Fn(&str) -> (Option<String>, Option<String>),
) {
    for el in doc.select(&SEL_IMG) {
        let attrs = el.value();
        // `src` is usual; lazy-loading libraries hide the real URL in data-src.
        let Some(raw_src) = attrs
            .attr("src")
            .or_else(|| attrs.attr("data-src"))
            .map(str::trim)
            .filter(|s| !s.is_empty())
        else {
            continue;
        };
        let (src, src_key) = resolve(raw_src);
        page.images.push(DiscoveredImage {
            raw_src: raw_src.to_string(),
            src: src.clone(),
            src_key: src_key.clone(),
            // Absent alt and empty alt are different findings, so keep the
            // distinction instead of flattening both to "".
            alt: attrs.attr("alt").map(collapse_ws),
            title: attrs
                .attr("title")
                .map(collapse_ws)
                .filter(|t| !t.is_empty()),
            declared_width: attrs.attr("width").and_then(|w| w.trim().parse().ok()),
            declared_height: attrs.attr("height").and_then(|h| h.trim().parse().ok()),
            loading_lazy: attrs
                .attr("loading")
                .is_some_and(|l| l.eq_ignore_ascii_case("lazy")),
        });
        if let (Some(src), Some(key)) = (src, src_key) {
            page.links
                .push(reference_link(raw_src, src, key, LinkSource::Image));
        }
    }
}

/// Scripts, iframes, forms and `<meta refresh>` targets.
fn extract_other_references(
    doc: &Html,
    page: &mut ParsedPage,
    resolve: &impl Fn(&str) -> (Option<String>, Option<String>),
) {
    for el in doc.select(&SEL_SCRIPT) {
        if let Some(src) = el
            .value()
            .attr("src")
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            if let (Some(url), Some(key)) = resolve(src) {
                page.links
                    .push(reference_link(src, url, key, LinkSource::Script));
            }
        }
    }
    for el in doc.select(&SEL_IFRAME) {
        if let Some(src) = el
            .value()
            .attr("src")
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            if let (Some(url), Some(key)) = resolve(src) {
                page.links
                    .push(reference_link(src, url, key, LinkSource::Iframe));
            }
        }
    }
    for el in doc.select(&SEL_FORM) {
        if let Some(action) = el
            .value()
            .attr("action")
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            if let (Some(url), Some(key)) = resolve(action) {
                page.links
                    .push(reference_link(action, url, key, LinkSource::Form));
            }
        }
    }
    // <meta http-equiv="refresh" content="0; url=/next">
    if let Some(refresh) = page.meta_refresh.clone() {
        if let Some(target) = parse_meta_refresh_target(&refresh) {
            if let (Some(url), Some(key)) = resolve(&target) {
                page.links
                    .push(reference_link(&target, url, key, LinkSource::MetaRefresh));
            }
        }
    }
}

/// Pull the URL out of a `meta refresh` content value.
pub fn parse_meta_refresh_target(content: &str) -> Option<String> {
    let lower = content.to_ascii_lowercase();
    let idx = lower.find("url=")?;
    let raw = content[idx + 4..].trim();
    let raw = raw.trim_matches(|c| c == '\'' || c == '"' || c == ';');
    if raw.is_empty() {
        None
    } else {
        Some(raw.to_string())
    }
}

fn extract_structured_data(doc: &Html, page: &mut ParsedPage) {
    for el in doc.select(&SEL_SCRIPT) {
        let is_ld_json = el
            .value()
            .attr("type")
            .is_some_and(|t| t.trim().eq_ignore_ascii_case("application/ld+json"));
        if !is_ld_json {
            continue;
        }
        let raw = element_text(el);
        if raw.trim().is_empty() {
            continue;
        }
        page.structured_data.push(parse_json_ld(&raw));
    }

    let itemtypes: Vec<String> = doc
        .select(&SEL_ITEMTYPE)
        .filter_map(|el| el.value().attr("itemtype"))
        .map(|t| t.trim().to_string())
        .collect();
    if let Some(block) = microdata_block(&itemtypes) {
        page.structured_data.push(block);
    }
}

/// Where on the page an element sits, from the innermost semantic ancestor.
///
/// Element names are checked first, then ARIA roles, so both `<nav>` and
/// `<div role="navigation">` are recognised.
fn link_position(el: ElementRef) -> LinkPosition {
    for ancestor in el.ancestors() {
        let Some(element) = ancestor.value().as_element() else {
            continue;
        };
        let by_name = match element.name() {
            "nav" => Some(LinkPosition::Navigation),
            "header" => Some(LinkPosition::Header),
            "footer" => Some(LinkPosition::Footer),
            "aside" => Some(LinkPosition::Aside),
            "main" | "article" => Some(LinkPosition::Content),
            _ => None,
        };
        if let Some(position) = by_name {
            return position;
        }
        if let Some(role) = element.attr("role") {
            let by_role = match role.trim().to_ascii_lowercase().as_str() {
                "navigation" => Some(LinkPosition::Navigation),
                "banner" => Some(LinkPosition::Header),
                "contentinfo" => Some(LinkPosition::Footer),
                "complementary" => Some(LinkPosition::Aside),
                "main" => Some(LinkPosition::Content),
                _ => None,
            };
            if let Some(position) = by_role {
                return position;
            }
        }
    }
    LinkPosition::Unknown
}

/// All descendant text of an element, including inside `<script>`.
///
/// Used where the raw contents *are* the data: `<title>`, JSON-LD bodies.
fn element_text(el: ElementRef) -> String {
    el.text().collect::<String>()
}

/// Human-visible text, skipping scripts, styles and other non-content
/// elements, with word boundaries inserted around block-level elements.
fn visible_text(el: ElementRef) -> String {
    let mut out = String::new();
    walk_visible(el, &mut out);
    out
}

fn walk_visible(el: ElementRef, out: &mut String) {
    for child in el.children() {
        if let Some(text) = child.value().as_text() {
            out.push_str(text);
            continue;
        }
        let Some(element) = child.value().as_element() else {
            continue;
        };
        let name = element.name();
        if NON_CONTENT_TAGS.contains(&name) {
            continue;
        }
        // `hidden` content is not visible text.
        if element.attr("hidden").is_some() {
            continue;
        }
        let block = !INLINE_TAGS.contains(&name);
        if block {
            out.push(' ');
        }
        if let Some(child_el) = ElementRef::wrap(child) {
            walk_visible(child_el, out);
        }
        if block {
            out.push(' ');
        }
    }
}

/// Trim and collapse every run of whitespace to one space.
fn collapse_ws(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for word in s.split_whitespace() {
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(word);
    }
    out
}
