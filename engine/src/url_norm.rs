//! URL normalisation and the crawl frontier's dedup key.
//!
//! Two different normalisations happen here, and keeping them apart matters:
//!
//! 1. **The URL we fetch and store** gets only *semantically safe* treatment:
//!    lowercased scheme and host, default port removed, `.`/`..` path segments
//!    resolved, fragment dropped. These never change which resource is served.
//!
//! 2. **The dedup key** additionally applies the user's policy: trailing-slash
//!    rules, dropping or sorting query parameters, ignoring `index.html`. These
//!    *can* change meaning (`/a` and `/a/` are allowed to differ), so they only
//!    ever affect whether we consider a URL already seen — never the request.
//!
//! So a crawl that meets `/p?utm_source=x` first fetches exactly that URL, and
//! later skips `/p?utm_source=y` because both share the key `/p`.

use std::fmt;

use serde::{Deserialize, Serialize};
use url::Url;

/// Query parameters that are, by default, ignored when building a dedup key.
/// These are analytics and ad-click decorations: they do not change the page.
pub const DEFAULT_IGNORED_QUERY_PARAMS: &[&str] = &[
    "utm_source",
    "utm_medium",
    "utm_campaign",
    "utm_term",
    "utm_content",
    "utm_id",
    "utm_source_platform",
    "gclid",
    "gclsrc",
    "dclid",
    "gbraid",
    "wbraid",
    "fbclid",
    "msclkid",
    "twclid",
    "ttclid",
    "igshid",
    "mc_cid",
    "mc_eid",
    "_ga",
    "_gl",
    "yclid",
    "ref_src",
    "phpsessid",
    "jsessionid",
    "sessionid",
];

/// What to do with the trailing slash when building a dedup key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum TrailingSlash {
    /// `/a` and `/a/` are different URLs. The safe default, and what a search
    /// engine does.
    #[default]
    Preserve,
    /// Treat `/a/` as `/a`.
    Remove,
    /// Treat `/a` as `/a/` (never applied to URLs that look like files).
    Add,
}

/// What to do with the query string when building a dedup key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum QueryPolicy {
    /// Keep every parameter.
    Keep,
    /// Ignore the query string entirely — `/a?x=1` and `/a?x=2` collapse to `/a`.
    StripAll,
    /// Ignore the listed parameters (the default, seeded with
    /// [`DEFAULT_IGNORED_QUERY_PARAMS`]).
    Ignore { params: Vec<String> },
    /// Ignore everything *except* the listed parameters.
    Only { params: Vec<String> },
}

impl Default for QueryPolicy {
    fn default() -> Self {
        QueryPolicy::Ignore {
            params: DEFAULT_IGNORED_QUERY_PARAMS
                .iter()
                .map(|s| (*s).to_string())
                .collect(),
        }
    }
}

/// How URLs are reduced to dedup keys.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct NormalizationPolicy {
    pub trailing_slash: TrailingSlash,
    pub query: QueryPolicy,
    /// Sort query parameters so `?a=1&b=2` and `?b=2&a=1` share a key.
    pub sort_query: bool,
    /// Treat `/dir/index.html` as `/dir/`.
    pub strip_index_files: bool,
    /// Lowercase the path. Off by default: most servers are case-sensitive.
    pub lowercase_path: bool,
}

impl Default for NormalizationPolicy {
    fn default() -> Self {
        Self {
            trailing_slash: TrailingSlash::default(),
            query: QueryPolicy::default(),
            sort_query: true,
            strip_index_files: false,
            lowercase_path: false,
        }
    }
}

/// Index-file names collapsed to their directory by `strip_index_files`.
const INDEX_FILES: &[&str] = &[
    "index.html",
    "index.htm",
    "index.php",
    "index.asp",
    "index.aspx",
    "index.jsp",
    "default.html",
    "default.htm",
    "default.asp",
    "default.aspx",
];

/// A URL that is ready to be queued: the request form plus its dedup key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NormalizedUrl {
    /// The URL we will actually request and show to the user.
    pub url: Url,
    /// The key the frontier dedups on. Never shown in the UI.
    pub key: String,
}

impl NormalizedUrl {
    pub fn as_str(&self) -> &str {
        self.url.as_str()
    }
}

impl fmt::Display for NormalizedUrl {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.url.as_str())
    }
}

/// Why a URL was rejected before it ever reached the frontier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum NormalizeError {
    #[error("not a valid URL")]
    Invalid,
    /// `mailto:`, `tel:`, `javascript:`, `data:` and friends.
    #[error("scheme is not http(s)")]
    UnsupportedScheme,
    #[error("URL has no host")]
    NoHost,
}

/// Apply the safe normalisations and compute the dedup key.
///
/// `base` is the page the URL was found on; pass `None` for absolute URLs such
/// as a seed or a sitemap entry.
pub fn normalize(
    raw: &str,
    base: Option<&Url>,
    policy: &NormalizationPolicy,
) -> Result<NormalizedUrl, NormalizeError> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Err(NormalizeError::Invalid);
    }

    // Resolve against the containing page when the href is relative. A bare
    // "#section" or "" resolves to the page itself, which the fragment strip
    // below then turns into the page URL — the frontier dedups it away.
    let mut url = match base {
        Some(base) => base.join(raw).map_err(|_| NormalizeError::Invalid)?,
        None => Url::parse(raw)
            .or_else(|e| {
                // Be forgiving about seeds typed without a scheme: "example.com".
                if matches!(e, url::ParseError::RelativeUrlWithoutBase) && !raw.contains(' ') {
                    Url::parse(&format!("https://{raw}"))
                } else {
                    Err(e)
                }
            })
            .map_err(|_| NormalizeError::Invalid)?,
    };

    match url.scheme() {
        "http" | "https" => {}
        _ => return Err(NormalizeError::UnsupportedScheme),
    }
    if url.host_str().is_none_or(str::is_empty) {
        return Err(NormalizeError::NoHost);
    }

    // --- Safe normalisation: this still addresses the same resource. ---------
    url.set_fragment(None);
    // `Url` already lowercases scheme and host and resolves dot segments. What
    // it keeps is a redundant trailing dot on the host ("example.com." ) and an
    // explicitly stated default port, so clear those.
    if let Some(host) = url.host_str() {
        if host.len() > 1 && host.ends_with('.') {
            let trimmed = host.trim_end_matches('.').to_string();
            let _ = url.set_host(Some(&trimmed));
        }
    }
    if url.port() == Some(default_port(url.scheme())) {
        let _ = url.set_port(None);
    }
    if url.path().is_empty() {
        url.set_path("/");
    }
    // An empty query ("/a?") is equivalent to no query at all.
    if url.query() == Some("") {
        url.set_query(None);
    }

    let key = dedup_key(&url, policy);
    Ok(NormalizedUrl { url, key })
}

fn default_port(scheme: &str) -> u16 {
    match scheme {
        "https" => 443,
        _ => 80,
    }
}

/// Build the frontier's dedup key for an already-safe-normalised URL.
pub fn dedup_key(url: &Url, policy: &NormalizationPolicy) -> String {
    let mut path = url.path().to_string();

    if policy.lowercase_path {
        path = path.to_lowercase();
    }

    if policy.strip_index_files {
        if let Some((dir, file)) = path.rsplit_once('/') {
            let lower = file.to_lowercase();
            if INDEX_FILES.contains(&lower.as_str()) {
                path = format!("{dir}/");
            }
        }
    }

    match policy.trailing_slash {
        TrailingSlash::Preserve => {}
        TrailingSlash::Remove => {
            if path.len() > 1 && path.ends_with('/') {
                path.truncate(path.len() - 1);
            }
        }
        TrailingSlash::Add => {
            // Don't append to something that looks like a file: "/a/b.css/" is
            // not the same resource as "/a/b.css".
            let last = path.rsplit('/').next().unwrap_or("");
            if !path.ends_with('/') && !last.contains('.') {
                path.push('/');
            }
        }
    }

    let query = dedup_query(url, policy);

    let mut key = String::with_capacity(url.as_str().len());
    key.push_str(url.scheme());
    key.push_str("://");
    if let Some(host) = url.host_str() {
        key.push_str(host);
    }
    if let Some(port) = url.port() {
        key.push(':');
        key.push_str(&port.to_string());
    }
    key.push_str(&path);
    if let Some(q) = query {
        if !q.is_empty() {
            key.push('?');
            key.push_str(&q);
        }
    }
    key
}

fn dedup_query(url: &Url, policy: &NormalizationPolicy) -> Option<String> {
    if matches!(policy.query, QueryPolicy::StripAll) {
        return None;
    }
    let raw = url.query()?;
    if raw.is_empty() {
        return None;
    }

    let mut pairs: Vec<(String, Option<String>)> = Vec::new();
    for part in raw.split('&') {
        if part.is_empty() {
            continue;
        }
        let (name, value) = match part.split_once('=') {
            Some((n, v)) => (n.to_string(), Some(v.to_string())),
            None => (part.to_string(), None),
        };
        let keep = match &policy.query {
            QueryPolicy::Keep => true,
            QueryPolicy::StripAll => false,
            QueryPolicy::Ignore { params } => !params.iter().any(|p| p.eq_ignore_ascii_case(&name)),
            QueryPolicy::Only { params } => params.iter().any(|p| p.eq_ignore_ascii_case(&name)),
        };
        if keep {
            pairs.push((name, value));
        }
    }

    if policy.sort_query {
        pairs.sort();
    }

    let joined = pairs
        .into_iter()
        .map(|(n, v)| match v {
            Some(v) => format!("{n}={v}"),
            None => n,
        })
        .collect::<Vec<_>>()
        .join("&");

    if joined.is_empty() {
        None
    } else {
        Some(joined)
    }
}

/// The registrable-ish root of a host, used to decide what counts as "this site".
///
/// We deliberately do *not* carry a Public Suffix List: it is a large, ageing
/// table and getting it wrong only ever affects the subdomain toggle. Instead we
/// strip a leading `www.` and compare the rest, which is right for every normal
/// site. For hosts under a multi-label suffix (`foo.co.uk`) this returns the
/// full host, so `a.foo.co.uk` and `b.foo.co.uk` are still recognised as
/// siblings via [`is_same_site`]'s suffix test.
pub fn site_root(host: &str) -> &str {
    host.strip_prefix("www.").unwrap_or(host)
}

/// Does `host` belong to the site rooted at `root_host`?
///
/// With `include_subdomains` off, only the exact host (ignoring `www.`) matches.
pub fn is_same_site(host: &str, root_host: &str, include_subdomains: bool) -> bool {
    let host = host.to_ascii_lowercase();
    let root = site_root(&root_host.to_ascii_lowercase()).to_string();
    let bare = site_root(&host);
    if bare == root {
        return true;
    }
    include_subdomains && bare.ends_with(&format!(".{root}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn norm(raw: &str) -> NormalizedUrl {
        normalize(raw, None, &NormalizationPolicy::default()).unwrap()
    }

    #[test]
    fn safe_normalisation_keeps_the_resource_identical() {
        assert_eq!(
            norm("HTTP://Example.COM/A").as_str(),
            "http://example.com/A"
        );
        assert_eq!(
            norm("https://example.com:443/a").as_str(),
            "https://example.com/a"
        );
        assert_eq!(
            norm("http://example.com:80/a").as_str(),
            "http://example.com/a"
        );
        assert_eq!(
            norm("https://example.com./a").as_str(),
            "https://example.com/a"
        );
        assert_eq!(norm("https://example.com").as_str(), "https://example.com/");
        assert_eq!(
            norm("https://example.com/a#frag").as_str(),
            "https://example.com/a"
        );
        assert_eq!(
            norm("https://example.com/a/b/../c").as_str(),
            "https://example.com/a/c"
        );
        assert_eq!(
            norm("https://example.com/a?").as_str(),
            "https://example.com/a"
        );
        // Path case is preserved: servers are usually case-sensitive.
        assert_ne!(
            norm("https://example.com/A").as_str(),
            norm("https://example.com/a").as_str()
        );
    }

    #[test]
    fn seeds_may_omit_the_scheme() {
        assert_eq!(norm("example.com/a").as_str(), "https://example.com/a");
    }

    #[test]
    fn non_http_schemes_are_rejected() {
        let p = NormalizationPolicy::default();
        for raw in [
            "mailto:a@b.com",
            "tel:+123",
            "javascript:void(0)",
            "data:text/html,x",
        ] {
            assert_eq!(
                normalize(raw, None, &p),
                Err(NormalizeError::UnsupportedScheme),
                "{raw}"
            );
        }
        assert_eq!(normalize("", None, &p), Err(NormalizeError::Invalid));
    }

    #[test]
    fn relative_hrefs_resolve_against_the_page() {
        let base = Url::parse("https://example.com/dir/page.html").unwrap();
        let p = NormalizationPolicy::default();
        let r = |raw: &str| normalize(raw, Some(&base), &p).unwrap().url.to_string();
        assert_eq!(r("other.html"), "https://example.com/dir/other.html");
        assert_eq!(r("/root.html"), "https://example.com/root.html");
        assert_eq!(r("../up.html"), "https://example.com/up.html");
        assert_eq!(r("//cdn.example.com/x.js"), "https://cdn.example.com/x.js");
        // A pure fragment link points back at the page itself.
        assert_eq!(r("#section"), "https://example.com/dir/page.html");
    }

    #[test]
    fn tracking_parameters_are_ignored_for_dedup_but_still_fetched() {
        let a = norm("https://example.com/p?utm_source=x&id=7");
        let b = norm("https://example.com/p?id=7&utm_campaign=y");
        assert_eq!(a.key, b.key, "tracking params must not create duplicates");
        assert_eq!(a.key, "https://example.com/p?id=7");
        // ...but the URL we request keeps them verbatim.
        assert_eq!(a.as_str(), "https://example.com/p?utm_source=x&id=7");
    }

    #[test]
    fn query_sorting_collapses_reordered_parameters() {
        assert_eq!(
            norm("https://example.com/p?b=2&a=1").key,
            norm("https://example.com/p?a=1&b=2").key
        );
    }

    #[test]
    fn query_policies() {
        let url = Url::parse("https://example.com/p?a=1&b=2&utm_source=z").unwrap();
        let with = |query| NormalizationPolicy {
            query,
            ..Default::default()
        };

        assert_eq!(
            dedup_key(&url, &with(QueryPolicy::StripAll)),
            "https://example.com/p"
        );
        assert_eq!(
            dedup_key(&url, &with(QueryPolicy::Keep)),
            "https://example.com/p?a=1&b=2&utm_source=z"
        );
        assert_eq!(
            dedup_key(
                &url,
                &with(QueryPolicy::Only {
                    params: vec!["a".into()]
                })
            ),
            "https://example.com/p?a=1"
        );
        assert_eq!(
            dedup_key(
                &url,
                &with(QueryPolicy::Ignore {
                    params: vec!["A".into(), "b".into()]
                })
            ),
            "https://example.com/p?utm_source=z",
            "ignore list is case-insensitive"
        );
    }

    #[test]
    fn trailing_slash_policies() {
        let slash = Url::parse("https://example.com/a/").unwrap();
        let bare = Url::parse("https://example.com/a").unwrap();
        let file = Url::parse("https://example.com/a/b.css").unwrap();
        let with = |trailing_slash| NormalizationPolicy {
            trailing_slash,
            ..Default::default()
        };

        // Preserve: distinct.
        let p = with(TrailingSlash::Preserve);
        assert_ne!(dedup_key(&slash, &p), dedup_key(&bare, &p));

        // Remove / Add: collapsed either way.
        let p = with(TrailingSlash::Remove);
        assert_eq!(dedup_key(&slash, &p), dedup_key(&bare, &p));
        assert_eq!(dedup_key(&slash, &p), "https://example.com/a");
        let p = with(TrailingSlash::Add);
        assert_eq!(dedup_key(&slash, &p), dedup_key(&bare, &p));
        assert_eq!(dedup_key(&slash, &p), "https://example.com/a/");
        // The root path is left alone, and files never gain a slash.
        assert_eq!(
            dedup_key(&Url::parse("https://example.com/").unwrap(), &p),
            "https://example.com/"
        );
        assert_eq!(dedup_key(&file, &p), "https://example.com/a/b.css");
    }

    #[test]
    fn index_files_collapse_to_their_directory() {
        let p = NormalizationPolicy {
            strip_index_files: true,
            ..Default::default()
        };
        let a = Url::parse("https://example.com/dir/index.html").unwrap();
        let b = Url::parse("https://example.com/dir/").unwrap();
        assert_eq!(dedup_key(&a, &p), dedup_key(&b, &p));
        assert_eq!(
            dedup_key(&Url::parse("https://example.com/index.php").unwrap(), &p),
            "https://example.com/"
        );
    }

    #[test]
    fn site_membership() {
        assert!(is_same_site("example.com", "example.com", false));
        assert!(is_same_site("www.example.com", "example.com", false));
        assert!(is_same_site("example.com", "www.example.com", false));
        assert!(!is_same_site("blog.example.com", "example.com", false));
        assert!(is_same_site("blog.example.com", "example.com", true));
        assert!(is_same_site(
            "deep.blog.example.com",
            "www.example.com",
            true
        ));
        assert!(!is_same_site("example.com.evil.com", "example.com", true));
        assert!(!is_same_site("notexample.com", "example.com", true));
        // Multi-label suffixes still work for sibling subdomains.
        assert!(is_same_site("a.foo.co.uk", "foo.co.uk", true));
    }
}
