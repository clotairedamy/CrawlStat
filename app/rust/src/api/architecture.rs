//! Information architecture: the section tree, the section link map and the
//! structural findings for the Visualise screen.
//!
//! Mirror structs only — the reasoning lives in `crawlstat_engine::architecture`.

use crawlstat_engine::architecture::{self, Section};

use crate::runtime;

/// A page cited as an example of a finding.
#[derive(Debug, Clone)]
pub struct ArchExample {
    pub url: String,
    pub detail: String,
}

/// Something about the site's structure worth changing.
#[derive(Debug, Clone)]
pub struct ArchFinding {
    pub id: String,
    /// `warning` or `notice`.
    pub severity: String,
    pub title: String,
    pub summary: String,
    pub fix: String,
    pub count: i64,
    pub examples: Vec<ArchExample>,
}

/// One folder of the site. The tree arrives flattened, depth first, with
/// `level` saying how far to indent: 0 is the whole site.
#[derive(Debug, Clone)]
pub struct ArchSection {
    pub path: String,
    pub label: String,
    pub level: i64,
    pub has_children: bool,
    pub pages: i64,
    /// Pages not in a child row: the folder's own pages and small subfolders.
    pub own_pages: i64,
    /// Status of the folder's own URL, if it was crawled.
    pub hub_status: Option<i64>,
    pub avg_depth: f64,
    pub max_depth: i64,
    pub deep: i64,
    pub weak: i64,
    pub errors: i64,
    pub redirects: i64,
    pub page_share: f64,
    pub value_share: f64,
}

/// Distinct linking page pairs from one top-level section to another.
#[derive(Debug, Clone)]
pub struct ArchFlow {
    pub from: i64,
    pub to: i64,
    pub links: i64,
}

#[derive(Debug, Clone)]
pub struct ArchMap {
    pub sections: Vec<String>,
    /// Working pages per section. i32 so it crosses as a plain Int32List.
    pub pages: Vec<i32>,
    pub flows: Vec<ArchFlow>,
}

#[derive(Debug, Clone)]
pub struct ArchOverview {
    pub pages: i64,
    pub avg_depth: f64,
    pub max_depth: i64,
    /// Share of working pages within three clicks of the start page.
    pub within_reach: f64,
    pub top_sections: i64,
    pub has_link_score: bool,
}

#[derive(Debug, Clone)]
pub struct SiteArchitecture {
    pub overview: ArchOverview,
    pub findings: Vec<ArchFinding>,
    pub sections: Vec<ArchSection>,
    pub map: ArchMap,
}

/// Read a crawl's information architecture.
pub fn site_architecture(db_path: String) -> Result<SiteArchitecture, String> {
    runtime::with_store(&db_path, |store| {
        let arch = architecture::analyse(store.connection())?;
        let mut sections = Vec::new();
        flatten(&arch.tree, 0, &mut sections);
        Ok(SiteArchitecture {
            overview: ArchOverview {
                pages: arch.overview.pages as i64,
                avg_depth: arch.overview.avg_depth,
                max_depth: i64::from(arch.overview.max_depth),
                within_reach: arch.overview.within_reach,
                top_sections: arch.overview.top_sections as i64,
                has_link_score: arch.overview.has_link_score,
            },
            findings: arch
                .findings
                .into_iter()
                .map(|finding| ArchFinding {
                    id: finding.id.to_string(),
                    severity: finding.severity.as_str().to_string(),
                    title: finding.title,
                    summary: finding.summary,
                    fix: finding.fix.to_string(),
                    count: finding.count as i64,
                    examples: finding
                        .examples
                        .into_iter()
                        .map(|example| ArchExample {
                            url: example.url,
                            detail: example.detail,
                        })
                        .collect(),
                })
                .collect(),
            sections,
            map: ArchMap {
                sections: arch.map.sections,
                pages: arch
                    .map
                    .pages
                    .into_iter()
                    .map(|n| i32::try_from(n).unwrap_or(i32::MAX))
                    .collect(),
                flows: arch
                    .map
                    .flows
                    .into_iter()
                    .map(|flow| ArchFlow {
                        from: flow.from as i64,
                        to: flow.to as i64,
                        links: flow.links as i64,
                    })
                    .collect(),
            },
        })
    })
    .map_err(|error| error.to_string())
}

fn flatten(section: &Section, level: i64, out: &mut Vec<ArchSection>) {
    out.push(ArchSection {
        path: section.path.clone(),
        label: section.label.clone(),
        level,
        has_children: !section.children.is_empty(),
        pages: section.pages as i64,
        own_pages: section.own_pages as i64,
        hub_status: section.hub_status.map(i64::from),
        avg_depth: section.avg_depth,
        max_depth: i64::from(section.max_depth),
        deep: section.deep as i64,
        weak: section.weak as i64,
        errors: section.errors as i64,
        redirects: section.redirects as i64,
        page_share: section.page_share,
        value_share: section.value_share,
    });
    for child in &section.children {
        flatten(child, level + 1, out);
    }
}
