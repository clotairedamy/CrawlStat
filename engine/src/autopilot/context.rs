//! Where an audit gets its evidence: the crawl file, and at most one polite
//! live request.

use std::time::{Duration, Instant};

use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use super::{now, Goal, Source};
use crate::error::Result;
use crate::ux::{self, journey};

/// One simulated journey, reduced to what a check needs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JourneyFact {
    pub goal_label: String,
    /// The journey to the goal under test, rather than a supporting one.
    pub primary: bool,
    pub outcome: String,
    pub clicks: usize,
    /// The link text clicked at each step.
    pub path: Vec<String>,
    pub target_exists: bool,
    pub target_url: Option<String>,
    pub unfetched: usize,
}

/// What the crawl knows about the audited page.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct CrawlFacts {
    pub url_id: i64,
    /// Outgoing links that ended in an error, with the status.
    pub broken_links: Vec<(String, u16)>,
    pub redirected_links: u32,
    pub broken_images: Vec<(String, u16)>,
    pub journeys: Vec<JourneyFact>,
}

/// A page as the crawl stored it.
#[derive(Debug, Clone)]
pub struct StoredPage {
    pub id: i64,
    pub url: String,
    pub html: Option<String>,
    pub status: Option<u16>,
    pub response_ms: Option<u64>,
    pub bytes: Option<u64>,
}

impl StoredPage {
    /// The stored markup as an audit source, when the crawl kept it.
    pub fn source(&self) -> Option<Source> {
        let html = self.html.clone().filter(|html| !html.trim().is_empty())?;
        Some(Source {
            html,
            origin: "stored".to_string(),
            status: self.status,
            response_ms: self.response_ms,
            bytes: self.bytes,
            fetched_at: now(),
        })
    }
}

/// Find a URL in the crawl, forgiving a trailing slash.
pub fn stored_page(conn: &Connection, url: &str) -> Result<Option<StoredPage>> {
    let trimmed = url.trim().trim_end_matches('/');
    let candidates = [url.trim().to_string(), trimmed.to_string(), format!("{trimmed}/")];
    for candidate in candidates {
        let found = conn
            .query_row(
                "SELECT id, url, html, status_code, response_time_ms, body_bytes
                 FROM urls WHERE url = ?1 AND status_code IS NOT NULL",
                [&candidate],
                |row| {
                    Ok(StoredPage {
                        id: row.get(0)?,
                        url: row.get(1)?,
                        html: row.get(2)?,
                        status: row.get::<_, Option<i64>>(3)?.map(|s| s as u16),
                        response_ms: row.get::<_, Option<i64>>(4)?.map(|s| s.max(0) as u64),
                        bytes: row.get::<_, Option<i64>>(5)?.map(|s| s.max(0) as u64),
                    })
                },
            )
            .optional()?;
        if found.is_some() {
            return Ok(found);
        }
    }
    Ok(None)
}

/// Everything the crawl can add about one page.
///
/// `deep` adds the supporting journeys (pricing, contact, privacy) to the one
/// for the goal under test.
pub fn crawl_facts(
    conn: &Connection,
    page: &StoredPage,
    goal: Goal,
    persona: ux::Persona,
    deep: bool,
) -> Result<CrawlFacts> {
    let mut broken_links = Vec::new();
    let mut redirected_links = 0u32;
    {
        let mut stmt = conn.prepare(
            "SELECT t.url, coalesce(t.final_status_code, t.status_code), t.redirect_count
             FROM links l JOIN urls t ON t.dedup_key = l.target_key
             WHERE l.source_id = ?1 AND t.status_code IS NOT NULL
             GROUP BY t.id",
        )?;
        let rows = stmt.query_map([page.id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?, row.get::<_, i64>(2)?))
        })?;
        for row in rows {
            let (url, status, redirects) = row?;
            if status >= 400 {
                broken_links.push((url, status as u16));
            } else if redirects > 0 {
                redirected_links += 1;
            }
        }
    }

    let mut broken_images = Vec::new();
    {
        let mut stmt = conn.prepare(
            "SELECT t.url, coalesce(t.final_status_code, t.status_code)
             FROM images i JOIN urls t ON t.dedup_key = i.src_key
             WHERE i.url_id = ?1 AND t.status_code >= 400
             GROUP BY t.id",
        )?;
        let rows = stmt.query_map([page.id], |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)))?;
        for row in rows {
            let (url, status) = row?;
            broken_images.push((url, status as u16));
        }
    }

    let mut goals: Vec<(journey::Goal, bool)> = Vec::new();
    if let Some((id, task)) = goal.journey() {
        let preset = journey::preset_goals().into_iter().find(|g| g.id == id);
        let chosen = preset.unwrap_or_else(|| journey::custom_goal(task));
        if !chosen.keywords.is_empty() {
            goals.push((chosen, true));
        }
    }
    if deep {
        for preset in journey::preset_goals() {
            if ["pricing", "contact", "privacy"].contains(&preset.id.as_str())
                && !goals.iter().any(|(g, _)| g.id == preset.id)
            {
                goals.push((preset, false));
            }
        }
    }

    let mut journeys = Vec::new();
    for (goal, primary) in goals {
        let walk = journey::simulate(conn, &goal, persona)?;
        journeys.push(JourneyFact {
            goal_label: walk.goal_label,
            primary,
            outcome: walk.outcome,
            clicks: walk.clicks,
            path: walk.steps.iter().map(|s| s.anchor.clone()).collect(),
            target_exists: walk.target_exists,
            target_url: walk.target_url,
            unfetched: walk.unfetched_links,
        });
    }

    Ok(CrawlFacts { url_id: page.id, broken_links, redirected_links, broken_images, journeys })
}

/// Normalise what a tester typed into an absolute URL.
pub fn absolute(url: &str) -> String {
    let url = url.trim();
    if url.contains("://") { url.to_string() } else { format!("https://{url}") }
}

/// Fetch one page, live, once — after asking robots.txt.
///
/// This is the audit's only network traffic: a robots.txt request and one
/// GET, timed. A page that robots.txt asks crawlers to leave alone is not
/// fetched, whatever the tester wants; that is the guardrail, not a setting.
/// Non-2xx responses are returned rather than refused, because "the page
/// does not load" is itself the most important finding an audit can make.
pub fn fetch_live(url: &str) -> std::result::Result<Source, String> {
    let address = url::Url::parse(&absolute(url)).map_err(|error| format!("{url} is not a valid URL: {error}"))?;
    if !matches!(address.scheme(), "http" | "https") {
        return Err("Only http and https pages can be audited.".to_string());
    }

    crate::fetch::install_crypto_provider();
    let client = reqwest::blocking::Client::builder()
        .user_agent(crate::config::DEFAULT_USER_AGENT)
        .timeout(Duration::from_secs(20))
        .build()
        .map_err(|error| error.to_string())?;

    if let Ok(robots_url) = address.join("/robots.txt") {
        if let Ok(response) = client.get(robots_url).send() {
            if response.status().is_success() {
                if let Ok(body) = response.bytes() {
                    let rules = crate::robots::RobotsRules::parse(&body, crate::config::DEFAULT_ROBOTS_TOKEN);
                    if !rules.allows(address.as_str()) {
                        return Err(format!(
                            "robots.txt on {} asks crawlers not to fetch this page, so CrawlStat won't. Audit it from a crawl that stored its HTML, or test a page you are allowed to fetch.",
                            address.host_str().unwrap_or_default()
                        ));
                    }
                }
            }
        }
    }

    let started = Instant::now();
    let response = client
        .get(address.as_str())
        .send()
        .map_err(|error| format!("Could not fetch {address}: {error}"))?;
    let status = response.status().as_u16();
    let html = response.text().map_err(|error| format!("Could not read {address}: {error}"))?;
    let elapsed = started.elapsed().as_millis() as u64;

    Ok(Source {
        bytes: Some(html.len() as u64),
        html,
        origin: "fetched".to_string(),
        status: Some(status),
        response_ms: Some(elapsed),
        fetched_at: now(),
    })
}
