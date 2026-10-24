//! Seeding the frontier.
//!
//! Three kinds of seed, per the crawl config:
//!
//! * a start URL to spider outwards from,
//! * an XML sitemap (or sitemap index, optionally gzipped), expanded here,
//! * a pasted list of URLs.
//!
//! Sitemaps are resolved during seeding rather than inside the crawl loop. They
//! are not pages — they are a *source of* URLs — and fetching them up front
//! keeps the loop uniform. Every sitemap URL is also recorded in
//! `sitemap_urls`, which is what lets the Phase 2 audit spot pages that are in
//! the sitemap but have nothing linking to them.

use std::collections::HashSet;

use url::Url;

use super::CrawlContext;
use crate::config::Seed;
use crate::error::Result;
use crate::fetch::FetchRequest;
use crate::model::{FrontierUrl, LinkSource, ResourceKind, UrlState};
use crate::sitemap::{self, Sitemap};
use crate::store::{LogLevel, StoreHandle, WriteBatch};
use crate::url_norm;

/// Nested sitemap indexes to follow. Two levels covers every real site; the
/// bound stops a malicious index from looping forever.
const MAX_SITEMAP_DEPTH: usize = 3;
/// Total sitemap files fetched per crawl.
const MAX_SITEMAP_FILES: usize = 100;

/// What seeding produced.
#[derive(Debug, Default)]
pub struct SeedOutcome {
    /// URLs added to the frontier.
    pub seeded: u64,
    /// Sitemap files successfully read.
    pub sitemaps_read: usize,
    pub warnings: Vec<(String, Option<String>)>,
}

/// Resolve every seed and fill the frontier.
pub async fn seed(ctx: &CrawlContext, store: &StoreHandle) -> Result<SeedOutcome> {
    let mut outcome = SeedOutcome::default();
    let mut batch = WriteBatch::default();
    let mut seen: HashSet<String> = HashSet::new();

    for seed in &ctx.config.config.seeds {
        match seed {
            Seed::Url { url } => {
                push_seed(ctx, url, &mut batch, &mut seen, &mut outcome);
            }
            Seed::List { urls } => {
                for url in urls {
                    push_seed(ctx, url, &mut batch, &mut seen, &mut outcome);
                }
            }
            Seed::Sitemap { url } => {
                expand_sitemap(ctx, url, &mut batch, &mut seen, &mut outcome).await;
            }
        }
    }

    // robots.txt often advertises the sitemap. Reading it gives us the full URL
    // inventory for free, which is what the orphan-page audit compares against.
    if ctx.config.config.respect_robots && ctx.config.config.follow_sitemap_and_canonical {
        for sitemap_url in robots_sitemaps(ctx).await {
            if outcome.sitemaps_read >= MAX_SITEMAP_FILES {
                break;
            }
            expand_sitemap(ctx, &sitemap_url, &mut batch, &mut seen, &mut outcome).await;
        }
    }

    for (message, url) in &outcome.warnings {
        batch
            .log
            .push((LogLevel::Warning, message.clone(), url.clone()));
    }

    let added = store.apply(batch).await?;
    outcome.seeded = added as u64;
    Ok(outcome)
}

/// Add one seed URL at depth 0.
fn push_seed(
    ctx: &CrawlContext,
    raw: &str,
    batch: &mut WriteBatch,
    seen: &mut HashSet<String>,
    outcome: &mut SeedOutcome,
) {
    match url_norm::normalize(raw, None, &ctx.config.config.normalization) {
        Ok(normalized) => {
            if !seen.insert(normalized.key.clone()) {
                return;
            }
            batch.discovered.push(FrontierUrl {
                url: normalized.url.to_string(),
                key: normalized.key,
                depth: 0,
                discovered_from: None,
                // A seed, not a discovery.
                via: None,
                kind: ResourceKind::from_extension(normalized.url.path()),
                // Seeds define the site, so they are internal by definition —
                // including in list mode, where several hosts may be mixed.
                is_internal: true,
                state: UrlState::Queued,
            });
        }
        Err(err) => {
            outcome.warnings.push((
                format!("ignored seed `{raw}`: {err}"),
                Some(raw.to_string()),
            ));
        }
    }
}

/// Fetch a sitemap, follow any nested indexes, and queue everything in it.
async fn expand_sitemap(
    ctx: &CrawlContext,
    sitemap_url: &str,
    batch: &mut WriteBatch,
    seen: &mut HashSet<String>,
    outcome: &mut SeedOutcome,
) {
    // Breadth-first over (url, depth), so a sitemap index is read before the
    // sitemaps it points at.
    let mut queue: Vec<(String, usize)> = vec![(sitemap_url.to_string(), 0)];
    let mut fetched: HashSet<String> = HashSet::new();

    while let Some((current, depth)) = queue.pop() {
        if depth > MAX_SITEMAP_DEPTH || outcome.sitemaps_read >= MAX_SITEMAP_FILES {
            continue;
        }
        if !fetched.insert(current.clone()) {
            continue;
        }

        let Ok(url) = Url::parse(&current) else {
            outcome
                .warnings
                .push((format!("sitemap URL is not valid: {current}"), None));
            continue;
        };

        let _permit = ctx.hosts.acquire(&url, None).await;
        let mut request = FetchRequest::get(url.clone());
        // Sitemaps may be large, but not unbounded.
        request.max_body_bytes = 64 * 1024 * 1024;

        let response = match ctx.fetcher.fetch(request).await {
            Ok(response) if response.is_success() => response,
            Ok(response) => {
                outcome.warnings.push((
                    format!("sitemap {current} returned HTTP {}", response.status),
                    Some(current.clone()),
                ));
                continue;
            }
            Err(err) => {
                outcome.warnings.push((
                    format!("sitemap {current} could not be fetched: {err}"),
                    Some(current),
                ));
                continue;
            }
        };

        match sitemap::parse(&response.body) {
            Ok(Sitemap::Index { sitemaps }) => {
                outcome.sitemaps_read += 1;
                for nested in sitemaps {
                    // Nested `<loc>` values are often relative, so resolve
                    // them against the index they came from.
                    match url.join(&nested) {
                        Ok(resolved) => queue.push((resolved.to_string(), depth + 1)),
                        Err(_) => outcome.warnings.push((
                            format!("sitemap index {current} lists an unusable URL: {nested}"),
                            Some(current.clone()),
                        )),
                    }
                }
            }
            Ok(Sitemap::UrlSet { entries }) => {
                outcome.sitemaps_read += 1;
                for entry in entries {
                    let Ok(normalized) = url_norm::normalize(
                        &entry.loc,
                        Some(&url),
                        &ctx.config.config.normalization,
                    ) else {
                        continue;
                    };
                    // Record every declared URL, in scope or not: the audit
                    // wants to know about sitemap entries pointing off-site.
                    batch
                        .sitemap_entries
                        .push((current.clone(), entry, normalized.key.clone()));

                    if !ctx.config.matches_filters(normalized.url.as_str()) {
                        continue;
                    }
                    if !seen.insert(normalized.key.clone()) {
                        continue;
                    }
                    batch.discovered.push(FrontierUrl {
                        url: normalized.url.to_string(),
                        key: normalized.key,
                        depth: 0,
                        discovered_from: None,
                        via: Some(LinkSource::Sitemap),
                        kind: ResourceKind::from_extension(normalized.url.path()),
                        is_internal: ctx.config.is_internal(&normalized.url, &ctx.root_host),
                        state: UrlState::Queued,
                    });
                }
            }
            Err(err) => {
                outcome.warnings.push((
                    format!("sitemap {current} could not be parsed: {err}"),
                    Some(current),
                ));
            }
        }
    }
}

/// `Sitemap:` lines from the root host's robots.txt.
async fn robots_sitemaps(ctx: &CrawlContext) -> Vec<String> {
    // Use the first seed URL to work out which origin to ask.
    let Some(origin_url) = ctx.config.config.seeds.iter().find_map(|seed| match seed {
        Seed::Url { url } | Seed::Sitemap { url } => Url::parse(url).ok(),
        Seed::List { urls } => urls.first().and_then(|u| Url::parse(u).ok()),
    }) else {
        return Vec::new();
    };

    ctx.robots
        .rules_for(&origin_url, ctx.fetcher.as_ref())
        .await;

    // The sitemaps.org protocol says these must be absolute, and plenty of
    // sites write "/sitemap.xml" anyway. Resolve against the origin rather
    // than throwing the line away.
    ctx.robots
        .discovered_sitemaps()
        .await
        .iter()
        .filter_map(|raw| origin_url.join(raw).ok().map(|u| u.to_string()))
        .collect()
}
