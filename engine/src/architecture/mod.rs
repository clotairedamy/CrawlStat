//! Information architecture: how a site is organised, and what to change.
//!
//! The link graph on its own is a picture of a few hundred pages. What a
//! person improving a site actually needs is the answers that picture hides:
//!
//! - **the section tree** — the site's folders, from its URLs, with how many
//!   pages each holds, how deep they sit and how much internal link value
//!   reaches them;
//! - **the section map** — which top-level sections link to which, so an area
//!   nothing else points at stands out;
//! - **findings** — pages buried too deep, pages hanging off a single link,
//!   orphans, dead ends, folders with no landing page, sections that get far
//!   less link value than their size deserves — each with a count, examples
//!   and the change that fixes it.
//!
//! Everything here is read from a finished crawl and never written back.
//! Only followed `<a href>` links between crawled internal HTML pages count,
//! and each pair of pages counts once, the way search engines treat a menu
//! repeated on every page.

use std::collections::{BTreeMap, HashMap, HashSet};

use rusqlite::Connection;
use url::Url;

use crate::error::Result;
use crate::model::{LinkSource, ResourceKind};

#[cfg(test)]
mod tests;

/// Pages more than this many clicks from the start page count as buried.
pub const DEEP: u32 = 3;

/// A folder needs at least this many pages before a missing landing page is
/// worth mentioning.
const HUB_MIN_PAGES: u64 = 5;

/// How many example pages each finding carries.
const EXAMPLES: usize = 8;

/// How many child folders the tree keeps per level, and how deep it goes.
const TREE_CHILDREN: usize = 12;
const TREE_LEVELS: usize = 4;

/// How many top-level sections the map shows before folding the rest into
/// "Other".
const MAP_SECTIONS: usize = 8;

/// How bad a finding is. Mirrors the audit severities so the UI can colour
/// them the same way.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Warning,
    Notice,
}

impl Severity {
    pub fn as_str(self) -> &'static str {
        match self {
            Severity::Warning => "warning",
            Severity::Notice => "notice",
        }
    }
}

/// One page cited as an example of a finding.
#[derive(Debug, Clone, PartialEq)]
pub struct Example {
    pub url: String,
    pub detail: String,
}

/// Something about the site's structure worth changing.
#[derive(Debug, Clone, PartialEq)]
pub struct Finding {
    pub id: &'static str,
    pub severity: Severity,
    pub title: String,
    /// What is wrong, in numbers from this crawl.
    pub summary: String,
    /// What to do about it.
    pub fix: &'static str,
    /// How many pages (or, for section findings, sections) it covers.
    pub count: u64,
    pub examples: Vec<Example>,
}

/// One folder of the site.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Section {
    /// `/blog/2024`, or `/` for the root.
    pub path: String,
    /// The last segment, or the host for the root.
    pub label: String,
    /// Every fetched HTML URL in this folder and below.
    pub pages: u64,
    /// Pages here that are not in a child shown below — the folder's own
    /// leaf pages and any small subfolders folded into it.
    pub own_pages: u64,
    /// The status of the folder's own URL (`/blog/`), if it was crawled.
    pub hub_status: Option<u16>,
    pub avg_depth: f64,
    pub max_depth: u32,
    /// Pages deeper than [`DEEP`].
    pub deep: u64,
    /// Indexable pages linked from at most one other page.
    pub weak: u64,
    pub errors: u64,
    pub redirects: u64,
    /// Share of all fetched pages, 0..=1.
    pub page_share: f64,
    /// Share of the site's internal link score, 0..=1.
    pub value_share: f64,
    pub children: Vec<Section>,
}

/// Links from one top-level section to another (or to itself).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Flow {
    pub from: usize,
    pub to: usize,
    /// Distinct linking page pairs.
    pub links: u64,
}

/// Which top-level sections link to which.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SectionMap {
    /// `/blog`, `/`, … and possibly `Other`, biggest first.
    pub sections: Vec<String>,
    pub pages: Vec<u64>,
    pub flows: Vec<Flow>,
}

/// The headline numbers.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Overview {
    /// Fetched internal HTML pages that answered 2xx.
    pub pages: u64,
    pub avg_depth: f64,
    pub max_depth: u32,
    /// Share of 2xx pages within [`DEEP`] clicks.
    pub within_reach: f64,
    pub top_sections: u64,
    /// Whether link scores were computed for this crawl. Without them the
    /// value columns are empty rather than zero.
    pub has_link_score: bool,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Architecture {
    pub overview: Overview,
    pub findings: Vec<Finding>,
    pub tree: Section,
    pub map: SectionMap,
}

/// A fetched internal HTML URL, with what the analysis needs about it.
struct Page {
    id: i64,
    url: String,
    /// Path segments, without query or fragment.
    segments: Vec<String>,
    depth: u32,
    status: u16,
    indexable: bool,
    reason: Option<String>,
    score: f64,
    via: Option<i64>,
    from: Option<i64>,
    /// Distinct pages linking here.
    linked_from: u32,
    /// One of them — the only one, when `linked_from` is 1.
    parent: Option<i64>,
    /// Distinct pages this links to.
    links_to: u32,
}

impl Page {
    fn ok(&self) -> bool {
        (200..300).contains(&self.status)
    }

    /// A page a search engine should find: 2xx and indexable.
    fn wanted(&self) -> bool {
        self.ok() && self.indexable
    }
}

/// Build the whole picture from a crawl.
pub fn analyse(conn: &Connection) -> Result<Architecture> {
    let mut pages = load_pages(conn)?;
    let index: HashMap<i64, usize> = pages
        .iter()
        .enumerate()
        .map(|(at, page)| (page.id, at))
        .collect();

    let sections = top_sections(&pages);
    let section_of: Vec<usize> = pages
        .iter()
        .map(|page| section_index(&sections, page))
        .collect();

    let mut flows: HashMap<(usize, usize), u64> = HashMap::new();
    let mut outside_sources: Vec<HashSet<i64>> = vec![HashSet::new(); sections.len()];
    read_links(conn, |source, target| {
        let (Some(&s), Some(&t)) = (index.get(&source), index.get(&target)) else {
            return;
        };
        pages[s].links_to += 1;
        pages[t].linked_from += 1;
        pages[t].parent = Some(source);
        let (from, to) = (section_of[s], section_of[t]);
        *flows.entry((from, to)).or_default() += 1;
        if from != to {
            outside_sources[to].insert(source);
        }
    })?;

    let total_value: f64 = pages.iter().map(|page| page.score).sum();
    let tree = build_tree(&pages, total_value);

    let ok: Vec<&Page> = pages.iter().filter(|page| page.ok()).collect();
    let overview = Overview {
        pages: ok.len() as u64,
        avg_depth: if ok.is_empty() {
            0.0
        } else {
            ok.iter().map(|page| f64::from(page.depth)).sum::<f64>() / ok.len() as f64
        },
        max_depth: ok.iter().map(|page| page.depth).max().unwrap_or(0),
        within_reach: share(
            ok.iter().filter(|page| page.depth <= DEEP).count() as u64,
            ok.len() as u64,
        ),
        top_sections: tree.children.len() as u64,
        has_link_score: total_value > 0.0,
    };

    let mut map = SectionMap {
        pages: vec![0; sections.len()],
        sections: sections.iter().map(|s| s.name.clone()).collect(),
        flows: flows
            .into_iter()
            .map(|((from, to), links)| Flow { from, to, links })
            .collect(),
    };
    for &at in &section_of {
        map.pages[at] += 1;
    }
    map.flows.sort_by_key(|flow| (flow.from, flow.to));

    let by_id = |id: i64| index.get(&id).map(|&at| &pages[at]);
    let mut findings = Vec::new();
    findings.extend(deep_pages(&ok, &by_id));
    findings.extend(orphans(&ok));
    findings.extend(single_link(&ok, &by_id));
    findings.extend(dead_ends(&ok));
    findings.extend(missing_hubs(&tree));
    findings.extend(underlinked(&tree, overview.has_link_score));
    findings.extend(isolated(&sections, &map, &outside_sources));
    findings.extend(wasted_value(&pages, total_value));
    findings.sort_by(|a, b| a.severity.cmp(&b.severity).then(b.count.cmp(&a.count)));

    Ok(Architecture {
        overview,
        findings,
        tree,
        map,
    })
}

// ─── reading the crawl ──────────────────────────────────────────────────

fn load_pages(conn: &Connection) -> Result<Vec<Page>> {
    let mut stmt = conn.prepare(
        "SELECT id, url, depth, coalesce(final_status_code, status_code),
                coalesce(indexable, 0), indexability_reason,
                coalesce(link_score, 0), discovered_via, discovered_from
         FROM urls
         WHERE is_internal = 1 AND kind = ?1 AND status_code IS NOT NULL
         ORDER BY id",
    )?;
    let rows = stmt.query_map([ResourceKind::Html as i64], |row| {
        let url: String = row.get(1)?;
        Ok(Page {
            id: row.get(0)?,
            segments: segments(&url),
            url,
            depth: row.get::<_, i64>(2)?.max(0) as u32,
            status: row.get::<_, i64>(3)?.clamp(0, 999) as u16,
            indexable: row.get::<_, i64>(4)? == 1,
            reason: row.get(5)?,
            score: row.get::<_, f64>(6)?.max(0.0),
            via: row.get(7)?,
            from: row.get(8)?,
            linked_from: 0,
            parent: None,
            links_to: 0,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// Stream every distinct followed link between two different pages.
///
/// Grouped in SQLite: a site whose menu is on every page has hundreds of
/// thousands of link rows and a small fraction of that in distinct pairs.
fn read_links(conn: &Connection, mut each: impl FnMut(i64, i64)) -> Result<()> {
    let mut stmt = conn.prepare(
        "SELECT l.source_id, t.id
         FROM links l
         JOIN urls t ON t.dedup_key = l.target_key
         WHERE l.is_internal = 1 AND l.nofollow = 0 AND l.link_source = ?1
           AND t.id != l.source_id
         GROUP BY l.source_id, t.id",
    )?;
    let mut rows = stmt.query([LinkSource::Anchor as i64])?;
    while let Some(row) = rows.next()? {
        each(row.get(0)?, row.get(1)?);
    }
    Ok(())
}

/// The path segments of a URL, without query or fragment.
pub fn segments(url: &str) -> Vec<String> {
    match Url::parse(url) {
        Ok(parsed) => parsed
            .path_segments()
            .map(|parts| {
                parts
                    .filter(|segment| !segment.is_empty())
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default(),
        Err(_) => Vec::new(),
    }
}

/// The URL without scheme and host, for showing a page compactly.
pub fn path_of(url: &str) -> String {
    match Url::parse(url) {
        Ok(parsed) => match parsed.query() {
            Some(query) => format!("{}?{query}", parsed.path()),
            None => parsed.path().to_string(),
        },
        Err(_) => url.to_string(),
    }
}

/// Whether a URL is a page of a paginated list: `/page/3`, `?page=3`, `?p=3`.
pub fn is_paginated(url: &str) -> bool {
    let Ok(parsed) = Url::parse(url) else {
        return false;
    };
    let numeric = |value: &str| !value.is_empty() && value.bytes().all(|b| b.is_ascii_digit());
    let by_path = parsed
        .path_segments()
        .map(|parts| {
            let parts: Vec<&str> = parts.collect();
            parts
                .windows(2)
                .any(|pair| matches!(pair[0], "page" | "p") && numeric(pair[1]))
        })
        .unwrap_or(false);
    by_path
        || parsed.query_pairs().any(|(key, value)| {
            matches!(key.as_ref(), "page" | "p" | "pg" | "paged" | "start" | "offset")
                && numeric(&value)
        })
}

fn share(part: u64, whole: u64) -> f64 {
    if whole == 0 {
        0.0
    } else {
        part as f64 / whole as f64
    }
}

fn percent(part: u64, whole: u64) -> String {
    let value = share(part, whole) * 100.0;
    if value > 0.0 && value < 1.0 {
        "<1%".to_string()
    } else {
        format!("{value:.0}%")
    }
}

fn plural(count: u64, one: &str, many: &str) -> String {
    format!("{count} {}", if count == 1 { one } else { many })
}

// ─── the section tree ───────────────────────────────────────────────────

#[derive(Default)]
struct Folder {
    pages: u64,
    depth_sum: u64,
    max_depth: u32,
    deep: u64,
    weak: u64,
    errors: u64,
    redirects: u64,
    value: f64,
    hub_status: Option<u16>,
    children: BTreeMap<String, Folder>,
}

impl Folder {
    fn add(&mut self, page: &Page) {
        self.pages += 1;
        self.depth_sum += u64::from(page.depth);
        self.max_depth = self.max_depth.max(page.depth);
        self.deep += u64::from(page.ok() && page.depth > DEEP);
        self.weak += u64::from(page.wanted() && page.depth > 0 && page.linked_from <= 1);
        self.errors += u64::from(page.status >= 400);
        self.redirects += u64::from((300..400).contains(&page.status));
        self.value += page.score;
    }
}

fn build_tree(pages: &[Page], total_value: f64) -> Section {
    let mut root = Folder::default();
    for page in pages {
        root.add(page);
        let mut folder = &mut root;
        for segment in &page.segments {
            folder = folder.children.entry(segment.clone()).or_default();
            folder.add(page);
        }
        // The folder's own URL: `/blog/` is the landing page of `/blog`. Of
        // several (`/blog` and `/blog?ref=x`), the healthiest one stands.
        let status = Some(page.status);
        if folder.hub_status.is_none_or(|known| (200..300).contains(&page.status) && known >= 300) {
            folder.hub_status = status;
        }
    }

    let host = pages
        .iter()
        .find_map(|page| Url::parse(&page.url).ok()?.host_str().map(str::to_string))
        .unwrap_or_default();
    let total_pages = root.pages;
    to_section(&root, "/".to_string(), host, 0, total_pages, total_value)
}

fn to_section(
    folder: &Folder,
    path: String,
    label: String,
    level: usize,
    total_pages: u64,
    total_value: f64,
) -> Section {
    // A child of one page is a page, not a folder: it stays in its parent's
    // own count rather than becoming a row of its own.
    let mut children: Vec<(&String, &Folder)> = folder
        .children
        .iter()
        .filter(|(_, child)| child.pages >= 2)
        .collect();
    children.sort_by(|a, b| b.1.pages.cmp(&a.1.pages).then(a.0.cmp(b.0)));
    if level + 1 >= TREE_LEVELS {
        children.clear();
    }
    children.truncate(TREE_CHILDREN);

    let children: Vec<Section> = children
        .into_iter()
        .map(|(name, child)| {
            let child_path = if path == "/" {
                format!("/{name}")
            } else {
                format!("{path}/{name}")
            };
            to_section(child, child_path, name.clone(), level + 1, total_pages, total_value)
        })
        .collect();
    let shown: u64 = children.iter().map(|child| child.pages).sum();

    Section {
        path,
        label,
        pages: folder.pages,
        own_pages: folder.pages - shown,
        hub_status: folder.hub_status,
        avg_depth: if folder.pages == 0 {
            0.0
        } else {
            folder.depth_sum as f64 / folder.pages as f64
        },
        max_depth: folder.max_depth,
        deep: folder.deep,
        weak: folder.weak,
        errors: folder.errors,
        redirects: folder.redirects,
        page_share: share(folder.pages, total_pages),
        value_share: if total_value > 0.0 {
            folder.value / total_value
        } else {
            0.0
        },
        children,
    }
}

// ─── the section map ────────────────────────────────────────────────────

struct TopSection {
    name: String,
    /// The first path segment it stands for; `None` for the root and Other.
    segment: Option<String>,
}

/// The sections the map shows: the biggest top-level folders, the root's own
/// pages, and everything else as Other.
fn top_sections(pages: &[Page]) -> Vec<TopSection> {
    let mut counts: HashMap<&str, u64> = HashMap::new();
    for page in pages {
        if page.segments.len() >= 2 || (page.segments.len() == 1 && !page.segments[0].contains('.')) {
            *counts.entry(page.segments[0].as_str()).or_default() += 1;
        }
    }
    let mut ranked: Vec<(&str, u64)> = counts.into_iter().filter(|(_, n)| *n >= 2).collect();
    ranked.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));

    let mut sections = vec![TopSection {
        name: "/".to_string(),
        segment: None,
    }];
    sections.extend(ranked.iter().take(MAP_SECTIONS).map(|(segment, _)| TopSection {
        name: format!("/{segment}"),
        segment: Some(segment.to_string()),
    }));
    if ranked.len() > MAP_SECTIONS {
        sections.push(TopSection {
            name: "Other".to_string(),
            segment: None,
        });
    }
    sections
}

fn section_index(sections: &[TopSection], page: &Page) -> usize {
    let first = page.segments.first();
    if let Some(at) = sections
        .iter()
        .position(|s| s.segment.is_some() && s.segment.as_ref() == first)
    {
        return at;
    }
    // Pages at the root — the home page, /about, /contact.html — belong to
    // `/`; pages in small folders go to Other when there is one.
    let small_folder = page.segments.len() >= 2
        || (page.segments.len() == 1 && !page.segments[0].contains('.'));
    if small_folder && sections.last().is_some_and(|s| s.name == "Other") {
        sections.len() - 1
    } else {
        0
    }
}

// ─── findings ───────────────────────────────────────────────────────────

fn deep_pages<'a>(ok: &[&'a Page], by_id: &impl Fn(i64) -> Option<&'a Page>) -> Option<Finding> {
    let mut deep: Vec<&Page> = ok.iter().copied().filter(|page| page.depth > DEEP).collect();
    if deep.is_empty() {
        return None;
    }
    deep.sort_by(|a, b| b.depth.cmp(&a.depth).then(a.id.cmp(&b.id)));
    let count = deep.len() as u64;

    let paged = deep
        .iter()
        .filter(|page| {
            page.from
                .and_then(by_id)
                .is_some_and(|parent| is_paginated(&parent.url))
                || is_paginated(&page.url)
        })
        .count() as u64;

    let mut by_section: HashMap<String, u64> = HashMap::new();
    for page in &deep {
        let name = page
            .segments
            .first()
            .filter(|_| page.segments.len() >= 2)
            .map_or("/".to_string(), |segment| format!("/{segment}"));
        *by_section.entry(name).or_default() += 1;
    }
    let (top, top_count) = by_section
        .into_iter()
        .max_by(|a, b| a.1.cmp(&b.1).then(b.0.cmp(&a.0)))
        .unwrap_or_default();

    let mut summary = format!(
        "{count} {} ({} of the site) more than {DEEP} clicks from the start \
         page — most visitors and crawlers never get that far.",
        if count == 1 { "page is" } else { "pages are" },
        percent(count, ok.len() as u64),
    );
    if top_count * 2 >= count && count >= 4 {
        summary.push_str(&format!(" {top_count} of them are in {top}."));
    }
    if paged * 2 >= count && paged > 0 {
        summary.push_str(&format!(
            " {paged} are reached only by paging through a list (page 2, 3, 4…), \
             which is the usual cause."
        ));
    }

    Some(Finding {
        id: "deep_pages",
        severity: if share(count, ok.len() as u64) >= 0.1 {
            Severity::Warning
        } else {
            Severity::Notice
        },
        title: "Pages buried too deep".to_string(),
        summary,
        fix: "Bring important pages within three clicks of the home page. Add \
            category or topic hub pages linked from the navigation, link \
            related pages to each other, and replace long paginated archives \
            with filtered hubs or 'view all' pages. Breadcrumbs help too: \
            every page then links back up its own branch.",
        count,
        examples: deep
            .iter()
            .take(EXAMPLES)
            .map(|page| Example {
                url: page.url.clone(),
                detail: format!("{} clicks", page.depth),
            })
            .collect(),
    })
}

fn orphans(ok: &[&Page]) -> Option<Finding> {
    let orphans: Vec<&Page> = ok
        .iter()
        .copied()
        .filter(|page| page.indexable && page.linked_from == 0 && page.via.is_some())
        .collect();
    if orphans.is_empty() {
        return None;
    }
    let count = orphans.len() as u64;
    Some(Finding {
        id: "orphans",
        severity: Severity::Warning,
        title: "Orphan pages".to_string(),
        summary: format!(
            "{} no internal links pointing at {} at all — the crawler found {} \
             only through the sitemap or a canonical tag. Visitors can't reach \
             {} and search engines give {} almost no weight.",
            plural(count, "page has", "pages have"),
            if count == 1 { "it" } else { "them" },
            if count == 1 { "it" } else { "them" },
            if count == 1 { "it" } else { "them" },
            if count == 1 { "it" } else { "them" },
        ),
        fix: "Link each one from the section it belongs to — its category hub, \
            a related-content block, or the navigation if it matters. If a \
            page should not be found, take it out of the sitemap as well.",
        count,
        examples: orphans
            .iter()
            .take(EXAMPLES)
            .map(|page| Example {
                url: page.url.clone(),
                detail: match page.via {
                    Some(via) if via == LinkSource::Sitemap as i64 => "only in the sitemap",
                    Some(via) if via == LinkSource::Canonical as i64 => "only via a canonical",
                    Some(via) if via == LinkSource::Redirect as i64 => "only via a redirect",
                    _ => "no internal links",
                }
                .to_string(),
            })
            .collect(),
    })
}

fn single_link<'a>(ok: &[&'a Page], by_id: &impl Fn(i64) -> Option<&'a Page>) -> Option<Finding> {
    let weak: Vec<&Page> = ok
        .iter()
        .copied()
        .filter(|page| page.indexable && page.depth > 0 && page.linked_from == 1)
        .collect();
    if weak.is_empty() {
        return None;
    }
    let count = weak.len() as u64;

    let mut parents: HashMap<i64, u64> = HashMap::new();
    for page in &weak {
        if let Some(parent) = page.parent {
            *parents.entry(parent).or_default() += 1;
        }
    }
    let busiest = parents
        .into_iter()
        .max_by(|a, b| a.1.cmp(&b.1).then(b.0.cmp(&a.0)))
        .and_then(|(id, n)| Some((by_id(id)?, n)));

    let mut summary = format!(
        "{} linked from exactly one other page. If that link is removed or \
         the page is redesigned, {} silently drop{} out of the site.",
        plural(count, "page is", "pages are"),
        if count == 1 { "it" } else { "they" },
        if count == 1 { "s" } else { "" },
    );
    if let Some((parent, n)) = busiest.filter(|(_, n)| *n >= 3) {
        summary.push_str(&format!(
            " {n} of them hang off {}.",
            path_of(&parent.url)
        ));
    }

    let mut examples: Vec<&Page> = weak.clone();
    examples.sort_by(|a, b| b.score.total_cmp(&a.score).then(a.id.cmp(&b.id)));
    Some(Finding {
        id: "single_link",
        severity: if share(count, ok.len() as u64) >= 0.2 {
            Severity::Warning
        } else {
            Severity::Notice
        },
        title: "Pages hanging off a single link".to_string(),
        summary,
        fix: "Give each page a second and third way in: link it from its \
            category hub, from related articles, and from breadcrumbs. Pages \
            that matter commercially deserve a link from the navigation or the \
            home page.",
        count,
        examples: examples
            .iter()
            .take(EXAMPLES)
            .map(|page| Example {
                url: page.url.clone(),
                detail: page
                    .parent
                    .and_then(by_id)
                    .map_or("one inlink".to_string(), |parent| {
                        format!("only from {}", path_of(&parent.url))
                    }),
            })
            .collect(),
    })
}

fn dead_ends(ok: &[&Page]) -> Option<Finding> {
    let dead: Vec<&Page> = ok
        .iter()
        .copied()
        .filter(|page| page.links_to == 0)
        .collect();
    if dead.is_empty() {
        return None;
    }
    let count = dead.len() as u64;
    Some(Finding {
        id: "dead_ends",
        severity: Severity::Notice,
        title: "Dead-end pages".to_string(),
        summary: format!(
            "{} no links to any other page on the site. Visitors who land \
             there from search have nowhere to go next, and link value that \
             reaches {} stops there.",
            plural(count, "page has", "pages have"),
            if count == 1 { "it" } else { "them" },
        ),
        fix: "Add the site's navigation if the template is missing it, and a \
            'related' or 'next step' block that links onward. If links are \
            built by JavaScript after load, render them in the HTML so \
            crawlers see them too.",
        count,
        examples: dead
            .iter()
            .take(EXAMPLES)
            .map(|page| Example {
                url: page.url.clone(),
                detail: format!("{} clicks deep", page.depth),
            })
            .collect(),
    })
}

fn missing_hubs(tree: &Section) -> Option<Finding> {
    fn walk<'a>(section: &'a Section, out: &mut Vec<&'a Section>) {
        for child in &section.children {
            if child.pages >= HUB_MIN_PAGES
                && !child.hub_status.is_some_and(|status| (200..300).contains(&status))
            {
                out.push(child);
            }
            walk(child, out);
        }
    }
    let mut missing = Vec::new();
    walk(tree, &mut missing);
    if missing.is_empty() {
        return None;
    }
    missing.sort_by(|a, b| b.pages.cmp(&a.pages).then(a.path.cmp(&b.path)));
    let count = missing.len() as u64;
    let host = &tree.label;
    Some(Finding {
        id: "missing_hubs",
        severity: Severity::Notice,
        title: "Sections without a landing page".to_string(),
        summary: format!(
            "{} hold{} pages but no working page at {} own address. A \
             visitor who trims the URL, or a breadcrumb that links up a \
             level, hits a dead end — and there is no hub to pass link value \
             down into the section.",
            plural(count, "folder", "folders"),
            if count == 1 { "s" } else { "" },
            if count == 1 { "its" } else { "their" },
        ),
        fix: "Give each folder an index page that introduces the section and \
            links to its most important pages, and link that page from the \
            navigation or breadcrumbs. If the folder is only an artefact of \
            the URL scheme, redirect its address to the nearest real hub.",
        count,
        examples: missing
            .iter()
            .take(EXAMPLES)
            .map(|section| Example {
                url: format!("https://{host}{}/", section.path),
                detail: match section.hub_status {
                    Some(status) => format!("{} pages · returns {status}", section.pages),
                    None => format!("{} pages · never linked", section.pages),
                },
            })
            .collect(),
    })
}

fn underlinked(tree: &Section, has_link_score: bool) -> Option<Finding> {
    if !has_link_score {
        return None;
    }
    let mut starved: Vec<&Section> = tree
        .children
        .iter()
        .filter(|section| {
            section.pages >= 5 && section.page_share >= 0.05 && section.value_share < section.page_share * 0.4
        })
        .collect();
    if starved.is_empty() {
        return None;
    }
    starved.sort_by(|a, b| {
        (b.page_share - b.value_share).total_cmp(&(a.page_share - a.value_share))
    });
    let worst = starved[0];
    let count = starved.len() as u64;
    let host = &tree.label;
    Some(Finding {
        id: "underlinked_sections",
        severity: Severity::Warning,
        title: "Sections starved of internal links".to_string(),
        summary: format!(
            "{} holds {:.0}% of the site's pages but receives only {:.0}% of \
             its internal link value{}. Search engines read that as the site \
             saying the section doesn't matter.",
            worst.path,
            worst.page_share * 100.0,
            worst.value_share * 100.0,
            if count > 1 {
                format!(", and {} more like it", plural(count - 1, "section is", "sections are"))
            } else {
                String::new()
            },
        ),
        fix: "Link the section's hub from the main navigation or the home \
            page, link its best pages from related content elsewhere on the \
            site, and check the section's own template links between its \
            pages rather than only back to the home page.",
        count,
        examples: starved
            .iter()
            .take(EXAMPLES)
            .map(|section| Example {
                url: format!("https://{host}{}/", section.path),
                detail: format!(
                    "{:.0}% of pages · {:.1}% of link value",
                    section.page_share * 100.0,
                    section.value_share * 100.0
                ),
            })
            .collect(),
    })
}

fn isolated(
    sections: &[TopSection],
    map: &SectionMap,
    outside_sources: &[HashSet<i64>],
) -> Option<Finding> {
    let mut lonely: Vec<(usize, usize)> = sections
        .iter()
        .enumerate()
        .filter(|(at, section)| section.segment.is_some() && map.pages[*at] >= 5)
        .map(|(at, _)| (at, outside_sources[at].len()))
        .filter(|(_, sources)| *sources <= 2)
        .collect();
    if lonely.is_empty() {
        return None;
    }
    lonely.sort_by(|a, b| a.1.cmp(&b.1).then(map.pages[b.0].cmp(&map.pages[a.0])));
    let count = lonely.len() as u64;
    let (first, sources) = lonely[0];
    Some(Finding {
        id: "isolated_sections",
        severity: Severity::Notice,
        title: "Sections cut off from the rest of the site".to_string(),
        summary: format!(
            "{} ({} pages) is linked from {} outside itself. The rest of the \
             site barely acknowledges it exists, so its pages depend on that \
             one route.",
            map.sections[first],
            map.pages[first],
            match sources {
                0 => "no page".to_string(),
                n => plural(n as u64, "page", "pages"),
            },
        ),
        fix: "Cross-link: point related pages in other sections at it, add it \
            to the navigation or footer if it serves visitors, and link back \
            out from it so the two areas reinforce each other.",
        count,
        examples: lonely
            .iter()
            .map(|&(at, sources)| Example {
                url: map.sections[at].clone(),
                detail: format!(
                    "{} pages · linked from {} outside",
                    map.pages[at],
                    plural(sources as u64, "page", "pages")
                ),
            })
            .collect(),
    })
}

fn wasted_value(pages: &[Page], total_value: f64) -> Option<Finding> {
    if total_value <= 0.0 {
        return None;
    }
    let mut wasted: Vec<&Page> = pages
        .iter()
        .filter(|page| page.score > 0.0 && !page.wanted())
        .collect();
    let value: f64 = wasted.iter().map(|page| page.score).sum();
    let ratio = value / total_value;
    if ratio < 0.05 {
        return None;
    }
    wasted.sort_by(|a, b| b.score.total_cmp(&a.score).then(a.id.cmp(&b.id)));
    let count = wasted.len() as u64;
    Some(Finding {
        id: "wasted_value",
        severity: if ratio >= 0.15 {
            Severity::Warning
        } else {
            Severity::Notice
        },
        title: "Link value spent on pages that can't rank".to_string(),
        summary: format!(
            "{:.0}% of the site's internal link value flows to {} that \
             redirect, fail, or are kept out of search (noindex, canonicalised \
             elsewhere). It is value the pages you want found don't get.",
            ratio * 100.0,
            plural(count, "page", "pages"),
        ),
        fix: "Point links straight at the final URL instead of a redirect, \
            remove or fix links to broken pages, and stop linking prominently \
            (navigation, footer) to pages that are deliberately noindexed — \
            or reconsider whether they should be noindexed.",
        count,
        examples: wasted
            .iter()
            .take(EXAMPLES)
            .map(|page| Example {
                url: page.url.clone(),
                detail: if !page.ok() {
                    format!("returns {} · score {:.0}", page.status, page.score)
                } else {
                    format!(
                        "{} · score {:.0}",
                        page.reason.as_deref().unwrap_or("not indexable"),
                        page.score
                    )
                },
            })
            .collect(),
    })
}
