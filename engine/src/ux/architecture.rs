//! Information architecture findings: the problems a usability tester spends
//! their week on.
//!
//! The SEO audits answer "is this page correct". These answer "can a person
//! navigate this", which is a different question with different evidence, and
//! none of it shows up in a crawl report sorted by status code.
//!
//! Each finding here was chosen because it is something a tester would
//! otherwise discover by watching someone struggle — and because a crawl can
//! prove it across every page at once, which a moderated session never can.
//!
//! ## The findings
//!
//! * **Ambiguous labels** — one piece of link text going to many different
//!   places. "Learn more" pointing at eleven pages means nobody can predict
//!   where it lands, and a screen reader's link list is unusable.
//! * **Inconsistent labels** — one destination reached by many different
//!   labels. If "Pricing", "Plans" and "What it costs" all go to the same
//!   page, a visitor cannot tell they have already been there.
//! * **Orphans** — pages in the crawl that nothing links to. Reachable only
//!   from a sitemap, a search result or a bookmark.
//! * **Too deep** — pages further from the home page than anyone will travel.
//! * **Choice overload** — pages offering so many links at once that choosing
//!   becomes the task.
//!
//! All of them are *evidence for a hypothesis*, not verdicts. A nav with
//! sixty links may be a deliberate directory. The tool's job is to put it in
//! front of someone who can judge.

use std::collections::HashMap;

use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::error::Result;

/// One link text that leads to several different pages.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AmbiguousLabel {
    pub label: String,
    /// How many distinct pages it leads to.
    pub destinations: u32,
    /// A few of them, for the report.
    pub examples: Vec<String>,
}

/// One page reached through several different link texts.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InconsistentLabel {
    pub url: String,
    pub labels: Vec<String>,
}

/// A page nothing links to.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OrphanPage {
    pub url: String,
    pub title: String,
}

/// A page further from home than anyone will travel.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeepPage {
    pub url: String,
    pub depth: u32,
}

/// A page asking the visitor to choose between too many things.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CrowdedPage {
    pub url: String,
    pub links: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ArchitectureReport {
    pub ambiguous_labels: Vec<AmbiguousLabel>,
    pub inconsistent_labels: Vec<InconsistentLabel>,
    pub orphans: Vec<OrphanPage>,
    pub too_deep: Vec<DeepPage>,
    pub crowded: Vec<CrowdedPage>,
    /// Internal HTML pages considered.
    pub pages: u32,
    /// The deepest page in the crawl.
    pub max_depth: u32,
    /// Median links offered per page — the everyday amount of choice.
    pub median_links: u32,
}

/// Past this many clicks from home, a page is effectively unfindable by
/// browsing. Three is the number most IA guidance lands on; four is the point
/// where even patient visitors stop.
pub const DEEP_THRESHOLD: u32 = 4;

/// Links on one page past which choosing becomes the task. Deliberately
/// generous: real navigation menus are large, and the aim is to surface the
/// outliers rather than to flag every page with a footer.
pub const CROWDED_THRESHOLD: u32 = 120;

/// A label leading to more than this many places cannot be predicted.
pub const AMBIGUOUS_THRESHOLD: u32 = 3;

/// How many of each finding to carry in the report.
const SAMPLE: usize = 40;

/// Link text that is structural rather than chosen, and so not a finding.
///
/// "Skip to content" legitimately appears on every page pointing at that
/// page's own main region, and flagging it as ambiguous would bury the real
/// findings under a result that is correct markup.
fn structural(label: &str) -> bool {
    const SKIP: &[&str] = &[
        "skip to main content",
        "skip to content",
        "skip navigation",
        "skip to navigation",
        "menu",
        "close",
        "home",
        "back",
        "next",
        "previous",
    ];
    let lower = label.trim().to_ascii_lowercase();
    SKIP.contains(&lower.as_str())
}

/// Anchor text that is not really text: CSS leaking out of an inline `<style>`
/// inside an SVG inside a link, which a real crawl is full of.
fn usable(label: &str) -> bool {
    let trimmed = label.trim();
    !trimmed.is_empty()
        && trimmed.chars().count() <= 90
        && !trimmed.contains('{')
        && !trimmed.contains('}')
}

/// Normalise link text for comparison: case and surrounding punctuation vary
/// without changing what the visitor reads.
fn normalise(label: &str) -> String {
    label
        .trim()
        .trim_matches(|c: char| !c.is_alphanumeric())
        .to_ascii_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Thresholds the reader can tune, because what counts as "too deep" or
/// "too many choices" genuinely differs between a brochure site and a
/// documentation set.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArchitectureConfig {
    pub deep_threshold: u32,
    pub crowded_threshold: u32,
    /// A label leading to more than this many places cannot be predicted.
    pub ambiguous_threshold: u32,
}

impl Default for ArchitectureConfig {
    fn default() -> Self {
        Self {
            deep_threshold: DEEP_THRESHOLD,
            crowded_threshold: CROWDED_THRESHOLD,
            ambiguous_threshold: AMBIGUOUS_THRESHOLD,
        }
    }
}

pub fn analyse(conn: &Connection) -> Result<ArchitectureReport> {
    analyse_with(conn, ArchitectureConfig::default())
}

pub fn analyse_with(conn: &Connection, config: ArchitectureConfig) -> Result<ArchitectureReport> {
    // Pages first.
    let mut pages: HashMap<i64, (String, String, u32)> = HashMap::new();
    let mut by_key: HashMap<String, i64> = HashMap::new();
    {
        let mut stmt = conn.prepare(
            "SELECT id, url, dedup_key, coalesce(title, ''), depth
             FROM urls
             WHERE is_internal = 1 AND kind = 0 AND status_code IS NOT NULL
               AND coalesce(final_status_code, status_code) < 400",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, i64>(4)?.max(0) as u32,
            ))
        })?;
        for row in rows {
            let (id, url, key, title, depth) = row?;
            by_key.insert(key, id);
            pages.insert(id, (url, title, depth));
        }
    }

    // Then every internal link with usable text, deduplicated per
    // (source, label, target) so one nav repeated in a footer counts once.
    let mut label_to_targets: HashMap<String, (String, Vec<i64>)> = HashMap::new();
    let mut target_to_labels: HashMap<i64, Vec<String>> = HashMap::new();
    let mut inlinks: HashMap<i64, u32> = HashMap::new();
    let mut outlinks: HashMap<i64, u32> = HashMap::new();
    // (source, label, target) for link identity; target -1 is the sentinel
    // used to count a page's distinct offered labels once each.
    let mut seen: std::collections::HashSet<(i64, String, i64)> = std::collections::HashSet::new();

    {
        let mut stmt = conn.prepare(
            "SELECT source_id, anchor_text, target_key FROM links
             WHERE is_internal = 1 AND target_key IS NOT NULL
               AND anchor_text IS NOT NULL AND trim(anchor_text) <> ''",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?;
        for row in rows {
            let (source, raw, key) = row?;
            if !usable(&raw) {
                continue;
            }
            let label = normalise(&raw);
            if label.is_empty() {
                continue;
            }

            // Choice overload is about what the visitor is *shown*, so every
            // internal link counts — including ones pointing at pages this
            // crawl never got to. Counting only resolved targets made a
            // 157-links-per-page site report a median of two.
            if seen.insert((source, label.clone(), -1)) {
                *outlinks.entry(source).or_insert(0) += 1;
            }

            let Some(&target) = by_key.get(&key) else { continue };
            if target == source {
                continue;
            }
            if !seen.insert((source, label.clone(), target)) {
                continue;
            }
            *inlinks.entry(target).or_insert(0) += 1;

            if !structural(&label) {
                let entry = label_to_targets
                    .entry(label.clone())
                    .or_insert_with(|| (raw.trim().to_string(), Vec::new()));
                if !entry.1.contains(&target) {
                    entry.1.push(target);
                }
                let labels = target_to_labels.entry(target).or_default();
                if !labels.contains(&label) {
                    labels.push(label);
                }
            }
        }
    }

    let mut ambiguous: Vec<AmbiguousLabel> = label_to_targets
        .values()
        .filter(|(_, targets)| targets.len() as u32 > config.ambiguous_threshold)
        .map(|(display, targets)| AmbiguousLabel {
            label: display.clone(),
            destinations: targets.len() as u32,
            examples: targets
                .iter()
                .filter_map(|id| pages.get(id).map(|(url, _, _)| url.clone()))
                .take(5)
                .collect(),
        })
        .collect();
    ambiguous.sort_by(|a, b| b.destinations.cmp(&a.destinations).then(a.label.cmp(&b.label)));

    let mut inconsistent: Vec<InconsistentLabel> = target_to_labels
        .iter()
        .filter(|(_, labels)| labels.len() > 2)
        .filter_map(|(id, labels)| {
            pages.get(id).map(|(url, _, _)| InconsistentLabel {
                url: url.clone(),
                labels: labels.iter().take(6).cloned().collect(),
            })
        })
        .collect();
    inconsistent.sort_by(|a, b| b.labels.len().cmp(&a.labels.len()).then(a.url.cmp(&b.url)));

    let mut orphans: Vec<OrphanPage> = pages
        .iter()
        .filter(|(id, (_, _, depth))| *depth > 0 && !inlinks.contains_key(*id))
        .map(|(_, (url, title, _))| OrphanPage {
            url: url.clone(),
            title: title.clone(),
        })
        .collect();
    orphans.sort_by(|a, b| a.url.cmp(&b.url));

    let mut too_deep: Vec<DeepPage> = pages
        .values()
        .filter(|(_, _, depth)| *depth >= config.deep_threshold)
        .map(|(url, _, depth)| DeepPage {
            url: url.clone(),
            depth: *depth,
        })
        .collect();
    too_deep.sort_by(|a, b| b.depth.cmp(&a.depth).then(a.url.cmp(&b.url)));

    let mut crowded: Vec<CrowdedPage> = outlinks
        .iter()
        .filter(|(_, count)| **count >= config.crowded_threshold)
        .filter_map(|(id, count)| {
            pages.get(id).map(|(url, _, _)| CrowdedPage {
                url: url.clone(),
                links: *count,
            })
        })
        .collect();
    crowded.sort_by(|a, b| b.links.cmp(&a.links).then(a.url.cmp(&b.url)));

    let mut counts: Vec<u32> = outlinks.values().copied().collect();
    counts.sort_unstable();
    let median_links = counts.get(counts.len() / 2).copied().unwrap_or(0);
    let max_depth = pages.values().map(|(_, _, depth)| *depth).max().unwrap_or(0);

    Ok(ArchitectureReport {
        pages: pages.len() as u32,
        max_depth,
        median_links,
        ambiguous_labels: ambiguous.into_iter().take(SAMPLE).collect(),
        inconsistent_labels: inconsistent.into_iter().take(SAMPLE).collect(),
        orphans: orphans.into_iter().take(SAMPLE).collect(),
        too_deep: too_deep.into_iter().take(SAMPLE).collect(),
        crowded: crowded.into_iter().take(SAMPLE).collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::Store;

    fn site(pages: &[(&str, &str, u32)], links: &[(usize, &str, usize)]) -> Store {
        let store = Store::open_in_memory().unwrap();
        let conn = store.connection();
        for (index, (url, title, depth)) in pages.iter().enumerate() {
            conn.execute(
                "INSERT INTO urls (id, url, dedup_key, title, depth, state, kind,
                                   is_internal, status_code)
                 VALUES (?1, ?2, ?3, ?4, ?5, 2, 0, 1, 200)",
                rusqlite::params![index as i64 + 1, url, url.replace("https://", ""), title, depth],
            )
            .unwrap();
        }
        for (from, anchor, to) in links {
            conn.execute(
                "INSERT INTO links (source_id, anchor_text, target_key, target_url,
                                    raw_href, is_internal)
                 VALUES (?1, ?2, ?3, ?4, ?4, 1)",
                rusqlite::params![
                    *from as i64 + 1,
                    anchor,
                    pages[*to].0.replace("https://", ""),
                    pages[*to].0
                ],
            )
            .unwrap();
        }
        store
    }

    #[test]
    fn one_label_leading_everywhere_is_flagged() {
        // "Read more" on eleven cards is the classic screen-reader complaint:
        // the link list is eleven identical entries.
        let mut pages = vec![("https://a.test/", "Home", 0)];
        for index in 0..5 {
            pages.push((
                Box::leak(format!("https://a.test/post-{index}").into_boxed_str()),
                "Post",
                1,
            ));
        }
        let links: Vec<(usize, &str, usize)> =
            (1..=5).map(|index| (0usize, "Read more", index)).collect();
        let store = site(&pages, &links);

        let report = analyse(store.connection()).unwrap();
        assert_eq!(report.ambiguous_labels.len(), 1);
        assert_eq!(report.ambiguous_labels[0].destinations, 5);
        assert_eq!(report.ambiguous_labels[0].label, "Read more");
    }

    #[test]
    fn a_label_with_only_a_couple_of_destinations_is_not_flagged() {
        let store = site(
            &[
                ("https://a.test/", "Home", 0),
                ("https://a.test/a", "A", 1),
                ("https://a.test/b", "B", 1),
            ],
            &[(0, "Details", 1), (0, "Details", 2)],
        );
        assert!(analyse(store.connection()).unwrap().ambiguous_labels.is_empty());
    }

    #[test]
    fn one_page_called_three_different_things_is_flagged() {
        // Three different pages linking to the same one, each calling it
        // something else. (A fourth link from the page to itself would be
        // ignored, which is correct — and is what the first version of this
        // fixture got wrong.)
        let store = site(
            &[
                ("https://a.test/", "Home", 0),
                ("https://a.test/x", "X", 1),
                ("https://a.test/y", "Y", 1),
                ("https://a.test/pricing", "Pricing", 1),
            ],
            &[
                (0, "Pricing", 3),
                (1, "Plans", 3),
                (2, "What it costs", 3),
            ],
        );
        let report = analyse(store.connection()).unwrap();
        let finding = report
            .inconsistent_labels
            .iter()
            .find(|f| f.url.ends_with("/pricing"))
            .expect("the pricing page is called three things");
        assert_eq!(finding.labels.len(), 3);
    }

    #[test]
    fn skip_links_and_other_structural_text_are_not_findings() {
        // "Skip to main content" on every page pointing at that page is
        // correct markup, and flagging it would bury the real results.
        let mut pages = vec![("https://a.test/", "Home", 0)];
        for index in 0..5 {
            pages.push((
                Box::leak(format!("https://a.test/p{index}").into_boxed_str()),
                "P",
                1,
            ));
        }
        let links: Vec<(usize, &str, usize)> = (1..=5)
            .map(|index| (0usize, "Skip to main content", index))
            .collect();
        let store = site(&pages, &links);
        assert!(analyse(store.connection()).unwrap().ambiguous_labels.is_empty());
    }

    #[test]
    fn case_and_punctuation_do_not_make_two_labels_different() {
        let mut pages = vec![("https://a.test/", "Home", 0)];
        for index in 0..4 {
            pages.push((
                Box::leak(format!("https://a.test/n{index}").into_boxed_str()),
                "N",
                1,
            ));
        }
        let store = site(
            &pages,
            &[
                (0, "Read more", 1),
                (0, "read more", 2),
                (0, "Read more!", 3),
                (0, "  Read more  ", 4),
            ],
        );
        let report = analyse(store.connection()).unwrap();
        assert_eq!(report.ambiguous_labels.len(), 1, "they are one label");
        assert_eq!(report.ambiguous_labels[0].destinations, 4);
    }

    #[test]
    fn a_page_nothing_links_to_is_an_orphan() {
        let store = site(
            &[
                ("https://a.test/", "Home", 0),
                ("https://a.test/linked", "Linked", 1),
                ("https://a.test/orphan", "Orphan", 1),
            ],
            &[(0, "Linked", 1)],
        );
        let report = analyse(store.connection()).unwrap();
        assert_eq!(report.orphans.len(), 1);
        assert!(report.orphans[0].url.ends_with("/orphan"));
    }

    #[test]
    fn the_home_page_is_never_an_orphan() {
        // Nothing links to the home page from inside the crawl on plenty of
        // sites, and reporting the front door as unreachable is noise.
        let store = site(&[("https://a.test/", "Home", 0)], &[]);
        assert!(analyse(store.connection()).unwrap().orphans.is_empty());
    }

    #[test]
    fn pages_past_the_depth_threshold_are_listed_deepest_first() {
        let store = site(
            &[
                ("https://a.test/", "Home", 0),
                ("https://a.test/shallow", "Shallow", 2),
                ("https://a.test/deep", "Deep", 4),
                ("https://a.test/deeper", "Deeper", 7),
            ],
            &[(0, "Shallow", 1)],
        );
        let report = analyse(store.connection()).unwrap();
        assert_eq!(report.too_deep.len(), 2);
        assert_eq!(report.too_deep[0].depth, 7);
        assert_eq!(report.max_depth, 7);
    }

    #[test]
    fn a_page_offering_an_overwhelming_number_of_links_is_flagged() {
        let mut pages = vec![("https://a.test/", "Home", 0)];
        for index in 0..(CROWDED_THRESHOLD + 5) {
            pages.push((
                Box::leak(format!("https://a.test/c{index}").into_boxed_str()),
                "C",
                1,
            ));
        }
        let links: Vec<(usize, &str, usize)> = (1..pages.len())
            .map(|index| {
                (
                    0usize,
                    Box::leak(format!("Item {index}").into_boxed_str()) as &str,
                    index,
                )
            })
            .collect();
        let store = site(&pages, &links);
        let report = analyse(store.connection()).unwrap();
        assert_eq!(report.crowded.len(), 1);
        assert!(report.crowded[0].links >= CROWDED_THRESHOLD);
    }

    #[test]
    fn css_leaking_out_of_an_svg_is_ignored() {
        assert!(!usable(".cls-1 { fill: #000; }"));
        assert!(usable("Pricing"));
    }

    #[test]
    fn an_empty_crawl_reports_nothing_rather_than_failing() {
        let store = Store::open_in_memory().unwrap();
        let report = analyse(store.connection()).unwrap();
        assert_eq!(report.pages, 0);
        assert_eq!(report.median_links, 0);
        assert!(report.orphans.is_empty());
    }

    #[test]
    fn the_same_link_repeated_in_a_header_and_footer_counts_once() {
        let store = site(
            &[("https://a.test/", "Home", 0), ("https://a.test/x", "X", 1)],
            &[(0, "X", 1), (0, "X", 1), (0, "X", 1)],
        );
        let report = analyse(store.connection()).unwrap();
        // One page offering one link, not three.
        assert_eq!(report.median_links, 1);
    }
}


#[cfg(test)]
mod config_tests {
    use super::*;
    use crate::store::Store;

    fn deep_site() -> Store {
        let store = Store::open_in_memory().unwrap();
        for (index, depth) in [0u32, 2, 3, 5].iter().enumerate() {
            store
                .connection()
                .execute(
                    "INSERT INTO urls (id, url, dedup_key, title, depth, state, kind,
                                       is_internal, status_code)
                     VALUES (?1, ?2, ?3, 'T', ?4, 2, 0, 1, 200)",
                    rusqlite::params![
                        index as i64 + 1,
                        format!("https://a.test/{index}"),
                        format!("a.test/{index}"),
                        depth
                    ],
                )
                .unwrap();
        }
        store
    }

    #[test]
    fn lowering_the_depth_threshold_finds_more_pages() {
        let store = deep_site();
        let strict = analyse_with(
            store.connection(),
            ArchitectureConfig { deep_threshold: 2, ..Default::default() },
        )
        .unwrap();
        let loose = analyse_with(
            store.connection(),
            ArchitectureConfig { deep_threshold: 5, ..Default::default() },
        )
        .unwrap();
        assert_eq!(strict.too_deep.len(), 3);
        assert_eq!(loose.too_deep.len(), 1);
    }

    #[test]
    fn the_default_matches_the_published_constants() {
        let config = ArchitectureConfig::default();
        assert_eq!(config.deep_threshold, DEEP_THRESHOLD);
        assert_eq!(config.crowded_threshold, CROWDED_THRESHOLD);
    }
}
