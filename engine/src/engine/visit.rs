//! Visiting one URL: fetch, classify, parse, and work out what to queue next.
//!
//! This is a free function rather than a method so each visit can run as an
//! independent task holding only an `Arc` to the shared context. Nothing here
//! touches the database: a visit returns its result and its discoveries, and the
//! crawl loop batches them into a transaction.

use std::collections::HashSet;
use std::time::Duration;

use url::Url;

use super::CrawlContext;
use crate::config::CrawlMode;
use crate::fetch::{FetchRequest, FetchedResponse, Method};
use crate::hash;
use crate::model::{CrawledUrl, FrontierUrl, LinkSource, ParsedPage, ResourceKind, UrlState};
use crate::store::ClaimedUrl;
use crate::url_norm;

/// What one visit produced.
#[derive(Debug)]
pub struct VisitOutcome {
    pub result: CrawledUrl,
    /// Link distance of this URL from a seed, carried through for the live
    /// progress table.
    pub depth: u32,
    /// URLs found, already filtered against scope, depth and resource policy.
    pub discovered: Vec<FrontierUrl>,
    /// Notices for the crawl log and the progress stream.
    pub warnings: Vec<(String, Option<String>)>,
    /// Set when the fetch failed in a way worth another attempt.
    pub retryable: bool,
}

/// Fetch and process a single claimed URL.
pub async fn visit(ctx: &CrawlContext, claimed: &ClaimedUrl) -> VisitOutcome {
    let fetched_at = ctx.started.elapsed().as_secs() as i64;

    let Ok(url) = Url::parse(&claimed.url) else {
        // The frontier only ever stores parsed URLs, so this means the database
        // was edited by hand or corrupted.
        return VisitOutcome {
            result: failure_result(
                claimed,
                "invalid_url",
                "URL could not be parsed",
                fetched_at,
            ),
            depth: claimed.depth,
            discovered: Vec::new(),
            warnings: Vec::new(),
            retryable: false,
        };
    };

    // --- robots.txt -------------------------------------------------------
    let robots = ctx.robots.rules_for(&url, ctx.fetcher.as_ref()).await;
    if !robots.allows(url.as_str()) {
        let mut result = failure_result(claimed, "", "", fetched_at);
        result.state = UrlState::BlockedByRobots;
        result.error_kind = Some("blocked_by_robots".into());
        result.error_message = Some("disallowed by robots.txt".into());
        return VisitOutcome {
            result,
            depth: claimed.depth,
            discovered: Vec::new(),
            warnings: Vec::new(),
            retryable: false,
        };
    }

    // --- fetch, with retries ---------------------------------------------
    let config = &ctx.config.config;
    let method = method_for(claimed);
    let mut attempt = 0u8;
    let response = loop {
        // Politeness is applied per attempt, so a retry also waits its turn.
        let _permit = ctx.hosts.acquire(&url, robots.crawl_delay).await;

        let mut request = FetchRequest {
            url: url.clone(),
            method,
            max_body_bytes: config.max_body_bytes,
            max_redirects: config.max_redirects,
        };
        if method == Method::Head {
            request.max_body_bytes = 0;
        }

        match ctx.fetcher.fetch(request).await {
            Ok(response) => {
                // Some servers answer HEAD with 405 or 501. Fall back to GET so
                // the URL is still checked.
                if method == Method::Head && matches!(response.status, 405 | 501) {
                    let retry = FetchRequest {
                        url: url.clone(),
                        method: Method::Get,
                        max_body_bytes: config.max_body_bytes,
                        max_redirects: config.max_redirects,
                    };
                    if let Ok(via_get) = ctx.fetcher.fetch(retry).await {
                        break via_get;
                    }
                }
                break response;
            }
            Err(err) => {
                if err.is_retryable() && attempt < config.max_retries {
                    let backoff = config.retry_backoff_ms.saturating_mul(1 << attempt);
                    tokio::time::sleep(Duration::from_millis(backoff)).await;
                    attempt += 1;
                    continue;
                }
                let retryable = err.is_retryable();
                return VisitOutcome {
                    result: failure_result(claimed, err.kind(), &err.to_string(), fetched_at),
                    depth: claimed.depth,
                    discovered: Vec::new(),
                    warnings: Vec::new(),
                    // Only ask for another attempt if we have not already used
                    // them all up.
                    retryable: retryable && attempt < config.max_retries,
                };
            }
        }
    };

    build_outcome(ctx, claimed, &url, response, fetched_at, attempt + 1)
}

/// `GET` for pages we parse, `HEAD` for everything we only need a status for.
fn method_for(claimed: &ClaimedUrl) -> Method {
    if claimed.is_internal && claimed.kind.is_html() {
        Method::Get
    } else {
        Method::Head
    }
}

fn build_outcome(
    ctx: &CrawlContext,
    claimed: &ClaimedUrl,
    url: &Url,
    response: FetchedResponse,
    fetched_at: i64,
    attempts: u8,
) -> VisitOutcome {
    let config = &ctx.config.config;
    let content_type = response.content_type().map(str::to_string);
    let kind = ResourceKind::from_content_type(content_type.as_deref(), url.path());

    // Multiple X-Robots-Tag headers are legal; keep them all, comma-joined, the
    // way the directives themselves are written.
    let x_robots_tag = {
        let values = response.header_all("x-robots-tag");
        (!values.is_empty()).then(|| values.join(", "))
    };

    let mut result = CrawledUrl {
        url_id: claimed.id,
        url: claimed.url.clone(),
        state: UrlState::Done,
        kind,
        status_code: Some(response.status),
        final_status_code: (response.final_status != response.status)
            .then_some(response.final_status),
        response_time: Some(response.response_time),
        ttfb: Some(response.ttfb),
        content_type: content_type.clone(),
        content_length: response
            .header("content-length")
            .and_then(|v| v.parse::<u64>().ok()),
        // A HEAD request downloads nothing, so reporting zero bytes would be
        // wrong. `None` lets readers fall back to Content-Length.
        body_bytes: (!response.body.is_empty()).then_some(response.body.len() as u64),
        content_encoding: response.header("content-encoding").map(str::to_string),
        x_robots_tag,
        final_url: (response.final_url.as_str() != claimed.url)
            .then(|| response.final_url.to_string()),
        redirect_chain: response.redirect_chain.clone(),
        redirect_loop: response.redirect_loop,
        headers: response
            .headers
            .iter()
            .filter(|(name, _)| ctx.config.stores_header(name))
            .cloned()
            .collect(),
        content_hash: (!response.body.is_empty()).then(|| hash::content_hash(&response.body)),
        truncated: response.truncated,
        html: None,
        parsed: None,
        internal_links: 0,
        external_links: 0,
        error_kind: None,
        error_message: None,
        attempts,
        fetched_at,
    };

    let mut warnings = Vec::new();
    if response.redirect_loop {
        warnings.push((
            format!("redirect loop at {}", claimed.url),
            Some(claimed.url.clone()),
        ));
    }

    let mut discovered: Vec<FrontierUrl> = Vec::new();
    // Dedup within the page, so a nav link repeated forty times is one insert.
    let mut seen_keys: HashSet<String> = HashSet::new();

    // A redirect target is the same resource one hop on, so it keeps the
    // source's depth rather than costing a level.
    for hop in &response.redirect_chain {
        if let Some(found) = frontier_url_for(
            ctx,
            &hop.to_url,
            claimed.depth,
            claimed.id,
            LinkSource::Redirect,
        ) {
            if seen_keys.insert(found.key.clone()) {
                discovered.push(found);
            }
        }
    }

    // Parse HTML bodies of internal pages. A truncated body is still parsed —
    // partial links are better than none — and the row records the truncation.
    let should_parse = claimed.is_internal && kind.is_html() && !response.body.is_empty();

    if should_parse {
        let body = String::from_utf8_lossy(&response.body);
        let mut parsed =
            crate::parse::parse_html(&body, &response.final_url, &config.normalization);

        // The parser has no notion of scope; mark each link now.
        for link in parsed.links.iter_mut() {
            link.is_internal = link
                .target
                .as_deref()
                .and_then(|t| Url::parse(t).ok())
                .is_some_and(|u| ctx.config.is_internal(&u, &ctx.root_host));
        }

        let (internal_links, external_links) =
            parsed.link_counts(|target| is_internal_str(ctx, target));
        result.internal_links = internal_links;
        result.external_links = external_links;

        if config.store_html {
            result.html = Some(body.to_string());
        }

        // List mode records what a page links to, but never follows it.
        if config.mode == CrawlMode::Spider {
            discovered.extend(discoveries_from_page(ctx, claimed, &parsed, &mut seen_keys));
        }
        result.parsed = Some(parsed);
    } else if claimed.is_internal && kind.is_html() && response.body.is_empty() {
        // A 200 with no body, or a HEAD on a page: nothing to parse, but the
        // row is still a valid result.
        result.parsed = None;
    }

    VisitOutcome {
        result,
        depth: claimed.depth,
        discovered,
        warnings,
        retryable: false,
    }
}

/// Turn a parsed page's references into frontier entries.
fn discoveries_from_page(
    ctx: &CrawlContext,
    claimed: &ClaimedUrl,
    parsed: &ParsedPage,
    seen_keys: &mut HashSet<String>,
) -> Vec<FrontierUrl> {
    let config = &ctx.config.config;
    let mut out = Vec::new();

    for link in &parsed.links {
        let Some(target) = link.target.as_deref() else {
            continue;
        };

        // `rel="nofollow"` is a request not to follow; honour it by default.
        if link.nofollow && !config.follow_nofollow {
            continue;
        }

        let depth = match link.source {
            // References to the same document, or a redirect-like hop: no
            // extra level of depth.
            LinkSource::Canonical
            | LinkSource::Hreflang
            | LinkSource::MetaRefresh
            | LinkSource::Redirect => claimed.depth,
            _ => claimed.depth + 1,
        };

        // Canonicals and hreflang alternates are only queued when the user
        // wants the extra coverage (it is what finds orphan pages).
        if matches!(link.source, LinkSource::Canonical | LinkSource::Hreflang)
            && !config.follow_sitemap_and_canonical
        {
            continue;
        }

        if let Some(found) = frontier_url_for(ctx, target, depth, claimed.id, link.source) {
            if seen_keys.insert(found.key.clone()) {
                out.push(found);
            }
        }
    }

    out
}

/// Decide whether a discovered URL belongs in the frontier, and at what depth.
///
/// Returns `None` when the URL is out of scope: a filtered path, too deep, a
/// resource type the user does not want, or an external link when external
/// checking is off.
pub fn frontier_url_for(
    ctx: &CrawlContext,
    raw: &str,
    depth: u32,
    discovered_from: i64,
    via: LinkSource,
) -> Option<FrontierUrl> {
    let config = &ctx.config.config;
    let normalized = url_norm::normalize(raw, None, &config.normalization).ok()?;
    let url = &normalized.url;

    // Exclusions apply to everything, internal or not.
    if !ctx.config.matches_filters(url.as_str()) {
        return None;
    }

    let is_internal = ctx.config.is_internal(url, &ctx.root_host);
    let kind = ResourceKind::from_extension(url.path());

    if is_internal {
        // Depth limits are about how far into *this* site we go.
        if config.max_depth.is_some_and(|max| depth > max) {
            return None;
        }
        let wanted = match kind {
            ResourceKind::Html => true,
            ResourceKind::Image => config.resources.images,
            ResourceKind::Css => config.resources.css,
            ResourceKind::JavaScript => config.resources.javascript,
            ResourceKind::Document => config.resources.documents,
            ResourceKind::Other => false,
        };
        if !wanted {
            return None;
        }
    } else {
        // External URLs are fetched once for their status and never followed,
        // so the depth limit does not apply to them.
        if !config.check_external_links {
            return None;
        }
    }

    Some(FrontierUrl {
        url: url.to_string(),
        key: normalized.key,
        depth,
        discovered_from: Some(discovered_from),
        via: Some(via),
        kind,
        is_internal,
        state: UrlState::Queued,
    })
}

fn is_internal_str(ctx: &CrawlContext, target: &str) -> bool {
    Url::parse(target).is_ok_and(|u| ctx.config.is_internal(&u, &ctx.root_host))
}

/// A result row for a URL that could not be fetched.
fn failure_result(
    claimed: &ClaimedUrl,
    error_kind: &str,
    error_message: &str,
    fetched_at: i64,
) -> CrawledUrl {
    CrawledUrl {
        url_id: claimed.id,
        url: claimed.url.clone(),
        state: UrlState::Failed,
        kind: claimed.kind,
        status_code: None,
        final_status_code: None,
        response_time: None,
        ttfb: None,
        content_type: None,
        content_length: None,
        body_bytes: None,
        content_encoding: None,
        x_robots_tag: None,
        final_url: None,
        redirect_chain: Vec::new(),
        redirect_loop: false,
        headers: Vec::new(),
        content_hash: None,
        truncated: false,
        html: None,
        parsed: None,
        internal_links: 0,
        external_links: 0,
        error_kind: (!error_kind.is_empty()).then(|| error_kind.to_string()),
        error_message: (!error_message.is_empty()).then(|| error_message.to_string()),
        attempts: claimed.attempts,
        fetched_at,
    }
}
