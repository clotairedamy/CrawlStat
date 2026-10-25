use rusqlite::{params, Connection};

use super::*;
use crate::store::Store;

const SITE: &str = "https://example.com";

fn page(conn: &Connection, id: i64, path: &str, depth: u32, status: u16, score: f64) {
    let url = format!("{SITE}{path}");
    conn.execute(
        "INSERT INTO urls (id, url, dedup_key, depth, kind, is_internal, status_code,
                           indexable, link_score, discovered_via, discovered_from)
         VALUES (?1, ?2, ?2, ?3, 0, 1, ?4, ?5, ?6, ?7, NULL)",
        params![
            id,
            url,
            depth,
            status,
            i64::from((200..300).contains(&status)),
            score,
            // The home page is the seed; everything else was found by a link.
            if id == 1 { None } else { Some(LinkSource::Anchor as i64) },
        ],
    )
    .unwrap();
}

fn link(conn: &Connection, from: i64, to_path: &str) {
    link_as(conn, from, to_path, LinkSource::Anchor, false);
}

fn link_as(conn: &Connection, from: i64, to_path: &str, source: LinkSource, nofollow: bool) {
    let url = format!("{SITE}{to_path}");
    conn.execute(
        "INSERT INTO links (source_id, target_key, target_url, raw_href, link_source,
                            nofollow, is_internal)
         VALUES (?1, ?2, ?2, ?2, ?3, ?4, 1)",
        params![from, url, source as i64, nofollow],
    )
    .unwrap();
}

/// A small site with one of everything wrong:
///
/// - `/blog` posts each hang off the blog index alone;
/// - `/blog/archive/page/N` buries posts four and five clicks deep;
/// - `/shop/*` has no `/shop` landing page;
/// - `/landing` is only in the sitemap;
/// - `/about` links nowhere;
/// - `/old` redirects but the menu still links it.
fn site() -> Store {
    let store = Store::open_in_memory().unwrap();
    let conn = store.connection();

    page(conn, 1, "/", 0, 200, 100.0);
    page(conn, 2, "/blog", 1, 200, 40.0);
    page(conn, 3, "/about", 1, 200, 30.0);
    page(conn, 4, "/old", 1, 301, 30.0);
    for (n, id) in (10..16).enumerate() {
        page(conn, id, &format!("/blog/post-{n}"), 2, 200, 2.0);
    }
    page(conn, 20, "/blog/archive/page/2", 2, 200, 1.0);
    page(conn, 21, "/blog/archive/page/3", 3, 200, 1.0);
    page(conn, 22, "/blog/deep-a", 4, 200, 0.5);
    page(conn, 23, "/blog/deep-b", 5, 200, 0.5);
    for (n, id) in (30..35).enumerate() {
        page(conn, id, &format!("/shop/item-{n}"), 1, 200, 5.0);
    }
    page(conn, 40, "/landing", 1, 200, 0.0);
    conn.execute(
        "UPDATE urls SET discovered_via = ?1 WHERE id = 40",
        [LinkSource::Sitemap as i64],
    )
    .unwrap();
    conn.execute("UPDATE urls SET discovered_from = 21 WHERE id IN (22, 23)", [])
        .unwrap();

    for target in ["/blog", "/about", "/old"] {
        link(conn, 1, target);
    }
    for n in 0..5 {
        link(conn, 1, &format!("/shop/item-{n}"));
    }
    // The shop items link to each other and home, so none is weak.
    for id in 30..35 {
        link(conn, id, "/");
        for n in 0..5 {
            link(conn, id, &format!("/shop/item-{n}"));
        }
    }
    for n in 0..6 {
        link(conn, 2, &format!("/blog/post-{n}"));
    }
    for id in 10..16 {
        link(conn, id, "/");
        link(conn, id, "/blog");
    }
    link(conn, 2, "/blog/archive/page/2");
    link(conn, 20, "/blog/archive/page/3");
    link(conn, 21, "/blog/deep-a");
    link(conn, 21, "/blog/deep-b");
    for id in [20, 21, 22, 23] {
        link(conn, id, "/");
    }
    store
}

fn finding<'a>(arch: &'a Architecture, id: &str) -> &'a Finding {
    arch.findings
        .iter()
        .find(|f| f.id == id)
        .unwrap_or_else(|| panic!("no {id} finding in {:#?}", arch.findings))
}

fn urls(finding: &Finding) -> Vec<String> {
    finding
        .examples
        .iter()
        .map(|e| e.url.trim_start_matches(SITE).to_string())
        .collect()
}

#[test]
fn an_empty_crawl_has_nothing_to_say() {
    let store = Store::open_in_memory().unwrap();
    let arch = analyse(store.connection()).unwrap();
    assert_eq!(arch.overview.pages, 0);
    assert!(arch.findings.is_empty());
    assert!(arch.tree.children.is_empty());
}

#[test]
fn the_overview_counts_working_pages() {
    let store = site();
    let arch = analyse(store.connection()).unwrap();
    // 20 fetched pages, one of which redirects.
    assert_eq!(arch.overview.pages, 19);
    assert_eq!(arch.overview.max_depth, 5);
    assert!((arch.overview.within_reach - 17.0 / 19.0).abs() < 1e-9);
    assert!(arch.overview.has_link_score);
}

#[test]
fn the_tree_follows_the_folders() {
    let store = site();
    let arch = analyse(store.connection()).unwrap();
    let tree = &arch.tree;
    assert_eq!(tree.label, "example.com");
    assert_eq!(tree.pages, 20);
    assert_eq!(tree.hub_status, Some(200));

    let blog = &tree.children[0];
    assert_eq!(blog.path, "/blog");
    assert_eq!(blog.pages, 11);
    assert_eq!(blog.hub_status, Some(200));
    assert_eq!(blog.deep, 2);
    assert_eq!(blog.max_depth, 5);
    // The archive folder is the blog's only subfolder with two pages or more.
    assert_eq!(blog.children.len(), 1);
    assert_eq!(blog.children[0].path, "/blog/archive");
    assert_eq!(blog.own_pages, 11 - 2);

    let shop = &tree.children[1];
    assert_eq!(shop.path, "/shop");
    assert_eq!(shop.pages, 5);
    assert_eq!(shop.hub_status, None);

    // Single pages at the root are not folders.
    assert!(tree.children.iter().all(|c| c.path != "/about"));
    let shares: f64 = tree.children.iter().map(|c| c.page_share).sum();
    assert!(shares < 1.0);
}

#[test]
fn deep_pages_are_traced_to_pagination() {
    let store = site();
    let arch = analyse(store.connection()).unwrap();
    let deep = finding(&arch, "deep_pages");
    assert_eq!(deep.count, 2);
    assert_eq!(deep.severity, Severity::Warning);
    assert_eq!(urls(deep), ["/blog/deep-b", "/blog/deep-a"]);
    assert!(deep.summary.contains("paging through a list"), "{}", deep.summary);
}

#[test]
fn orphans_say_how_they_were_found() {
    let store = site();
    let arch = analyse(store.connection()).unwrap();
    let orphans = finding(&arch, "orphans");
    assert_eq!(orphans.count, 1);
    assert_eq!(urls(orphans), ["/landing"]);
    assert_eq!(orphans.examples[0].detail, "only in the sitemap");
}

#[test]
fn single_links_name_the_parent() {
    let store = site();
    let arch = analyse(store.connection()).unwrap();
    let weak = finding(&arch, "single_link");
    // Six posts, two archive pages, two deep posts and /about, which only the
    // home page links. /landing has no links at all and is an orphan instead.
    assert_eq!(weak.count, 11, "{weak:#?}");
    assert!(weak.summary.contains("7 of them hang off /blog"), "{}", weak.summary);
    assert!(weak
        .examples
        .iter()
        .any(|e| e.detail == "only from /blog"));
}

#[test]
fn dead_ends_and_missing_hubs() {
    let store = site();
    let arch = analyse(store.connection()).unwrap();
    let dead = finding(&arch, "dead_ends");
    assert!(urls(dead).contains(&"/about".to_string()));
    assert!(urls(dead).contains(&"/landing".to_string()));
    assert!(!urls(dead).contains(&"/old".to_string()), "redirects are not pages");

    let hubs = finding(&arch, "missing_hubs");
    assert_eq!(urls(hubs), ["/shop/"]);
    assert_eq!(hubs.examples[0].detail, "5 pages · never linked");
}

#[test]
fn value_on_redirects_is_wasted() {
    let store = site();
    let arch = analyse(store.connection()).unwrap();
    let wasted = finding(&arch, "wasted_value");
    assert_eq!(urls(wasted), ["/old"]);
    assert!(wasted.examples[0].detail.starts_with("returns 301"));
}

#[test]
fn warnings_come_before_notices() {
    let store = site();
    let arch = analyse(store.connection()).unwrap();
    let severities: Vec<Severity> = arch.findings.iter().map(|f| f.severity).collect();
    let mut sorted = severities.clone();
    sorted.sort();
    assert_eq!(severities, sorted);
}

#[test]
fn a_section_with_little_link_value_is_flagged() {
    let store = site();
    let conn = store.connection();
    // Thirty more blog posts with almost no score: the blog now holds most of
    // the site but receives a sliver of its value.
    for n in 0..30 {
        page(conn, 100 + n, &format!("/news/story-{n}"), 2, 200, 0.01);
        link(conn, 2, &format!("/news/story-{n}"));
    }
    let arch = analyse(conn).unwrap();
    let starved = finding(&arch, "underlinked_sections");
    assert!(starved.summary.starts_with("/news holds"), "{}", starved.summary);
    // Only the blog index links into /news.
    let isolated = finding(&arch, "isolated_sections");
    assert_eq!(isolated.examples[0].url, "/news");
}

#[test]
fn without_link_scores_value_findings_stay_quiet() {
    let store = site();
    let conn = store.connection();
    conn.execute("UPDATE urls SET link_score = NULL", []).unwrap();
    let arch = analyse(conn).unwrap();
    assert!(!arch.overview.has_link_score);
    assert!(arch.findings.iter().all(|f| f.id != "wasted_value"));
    assert!(arch.findings.iter().all(|f| f.id != "underlinked_sections"));
}

#[test]
fn links_count_once_per_pair_and_only_when_followed() {
    let store = site();
    let conn = store.connection();
    // A second link from the same page, a nofollow link and a canonical do
    // not give /landing any real inlink.
    page(conn, 50, "/promo", 1, 200, 1.0);
    link(conn, 1, "/promo");
    link(conn, 1, "/promo");
    link_as(conn, 3, "/landing", LinkSource::Anchor, true);
    link_as(conn, 3, "/landing", LinkSource::Canonical, false);
    let arch = analyse(conn).unwrap();
    assert_eq!(urls(finding(&arch, "orphans")), ["/landing"]);
    // Two links from the home page are still one linking page.
    assert_eq!(finding(&arch, "single_link").count, 12);
}

#[test]
fn the_map_counts_links_between_sections() {
    let store = site();
    let arch = analyse(store.connection()).unwrap();
    let map = &arch.map;
    assert_eq!(map.sections, ["/", "/blog", "/shop"]);
    // /, /about, /old, /landing; /blog belongs to its own section.
    assert_eq!(map.pages, [4, 11, 5]);
    let flow = |from: usize, to: usize| {
        map.flows
            .iter()
            .find(|f| f.from == from && f.to == to)
            .map_or(0, |f| f.links)
    };
    // Home → the five shop items; each item → home.
    assert_eq!(flow(0, 2), 5);
    assert_eq!(flow(2, 0), 5);
    // Home → /blog.
    assert_eq!(flow(0, 1), 1);
    // Shop items link each other: 5 × 4 pairs.
    assert_eq!(flow(2, 2), 20);
}

#[test]
fn urls_split_into_folders() {
    assert_eq!(segments("https://a.com/blog/post?x=1"), ["blog", "post"]);
    assert!(segments("https://a.com/").is_empty());
    assert_eq!(path_of("https://a.com/a?b=1"), "/a?b=1");
    assert!(is_paginated("https://a.com/blog/page/3"));
    assert!(is_paginated("https://a.com/blog?page=3"));
    assert!(is_paginated("https://a.com/list?p=12"));
    assert!(!is_paginated("https://a.com/page/about"));
    assert!(!is_paginated("https://a.com/blog?page=all"));
}
