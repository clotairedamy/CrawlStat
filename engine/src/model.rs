//! The records that flow from the fetcher, through the parser, into SQLite.
//!
//! These are deliberately plain structs with owned data. One is produced per
//! crawled URL, handed to the store writer over a channel, and dropped — so
//! peak memory is bounded by `concurrency`, not by the size of the crawl.

use std::time::Duration;

use serde::{Deserialize, Serialize};

/// Lifecycle of a URL in the frontier. Stored as an integer so the UI can
/// filter on it cheaply, and so a crashed crawl can be resumed by resetting
/// every `InProgress` row back to `Queued`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(i64)]
pub enum UrlState {
    Queued = 0,
    InProgress = 1,
    Done = 2,
    /// Fetching failed (network, DNS, timeout) — distinct from an HTTP error
    /// status, which is a successful fetch with a 4xx/5xx code.
    Failed = 3,
    /// Out of scope: filtered out, too deep, or past the URL limit.
    Skipped = 4,
    /// Disallowed by robots.txt.
    BlockedByRobots = 5,
}

impl UrlState {
    pub fn from_i64(v: i64) -> Option<Self> {
        Some(match v {
            0 => UrlState::Queued,
            1 => UrlState::InProgress,
            2 => UrlState::Done,
            3 => UrlState::Failed,
            4 => UrlState::Skipped,
            5 => UrlState::BlockedByRobots,
            _ => return None,
        })
    }
}

/// What kind of thing a URL is, which decides whether we parse its body.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[repr(i64)]
pub enum ResourceKind {
    /// Parsed for links and SEO fields.
    Html = 0,
    Image = 1,
    Css = 2,
    JavaScript = 3,
    /// PDF, DOCX, ...
    Document = 4,
    Other = 5,
}

impl ResourceKind {
    /// Classify from a `Content-Type` header, falling back to the URL's extension.
    pub fn from_content_type(content_type: Option<&str>, url_path: &str) -> Self {
        if let Some(ct) = content_type {
            let ct = ct
                .split(';')
                .next()
                .unwrap_or("")
                .trim()
                .to_ascii_lowercase();
            return match ct.as_str() {
                "text/html" | "application/xhtml+xml" => ResourceKind::Html,
                "text/css" => ResourceKind::Css,
                "text/javascript"
                | "application/javascript"
                | "application/x-javascript"
                | "module" => ResourceKind::JavaScript,
                _ if ct.starts_with("image/") => ResourceKind::Image,
                "application/pdf" => ResourceKind::Document,
                _ if ct.starts_with("application/vnd.openxmlformats")
                    || ct.starts_with("application/msword")
                    || ct == "application/rtf" =>
                {
                    ResourceKind::Document
                }
                // An unspecified or generic type on a page-looking URL is most
                // often HTML from a misconfigured server.
                "application/octet-stream" | "" => Self::from_extension(url_path),
                _ => ResourceKind::Other,
            };
        }
        Self::from_extension(url_path)
    }

    /// Guess from a URL path. Used before a response exists (to decide whether
    /// a discovered URL is in scope) and when the server sends no type.
    pub fn from_extension(url_path: &str) -> Self {
        let file = url_path.rsplit('/').next().unwrap_or("");
        let ext = match file.rsplit_once('.') {
            Some((_, ext)) => ext.to_ascii_lowercase(),
            None => return ResourceKind::Html,
        };
        match ext.as_str() {
            "html" | "htm" | "xhtml" | "php" | "asp" | "aspx" | "jsp" | "cfm" | "shtml" => {
                ResourceKind::Html
            }
            "jpg" | "jpeg" | "png" | "gif" | "webp" | "avif" | "svg" | "bmp" | "ico" | "tiff" => {
                ResourceKind::Image
            }
            "css" => ResourceKind::Css,
            "js" | "mjs" => ResourceKind::JavaScript,
            "pdf" | "doc" | "docx" | "xls" | "xlsx" | "ppt" | "pptx" | "rtf" | "odt" | "csv" => {
                ResourceKind::Document
            }
            "zip" | "gz" | "tar" | "rar" | "7z" | "dmg" | "exe" | "mp4" | "webm" | "mp3"
            | "wav" | "woff" | "woff2" | "ttf" | "otf" | "eot" | "xml" | "json" | "txt" => {
                ResourceKind::Other
            }
            // Unknown extension: most likely a slug like "/2024/report.v2".
            _ => ResourceKind::Html,
        }
    }

    pub fn is_html(self) -> bool {
        matches!(self, ResourceKind::Html)
    }
}

/// Where on the page a link was found. A rough but useful signal: links in the
/// main content carry more weight than boilerplate in a footer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[repr(i64)]
pub enum LinkPosition {
    Content = 0,
    Navigation = 1,
    Header = 2,
    Footer = 3,
    Aside = 4,
    Unknown = 5,
}

/// One `<a href>` (or other link-ish element) found on a page.
#[derive(Debug, Clone, PartialEq)]
pub struct DiscoveredLink {
    /// The href exactly as written in the markup.
    pub raw_href: String,
    /// Absolute, safe-normalised target. `None` when the href could not be
    /// resolved (`mailto:`, `javascript:`, malformed).
    pub target: Option<String>,
    /// Dedup key of the target, for joining back to the `urls` table.
    pub target_key: Option<String>,
    pub anchor_text: String,
    /// The `rel` attribute verbatim.
    pub rel: Option<String>,
    pub position: LinkPosition,
    pub nofollow: bool,
    /// Discovered via something other than `<a href>` (canonical, sitemap,
    /// redirect, `<img src>`, ...).
    pub source: LinkSource,
    /// Whether the target belongs to the site being crawled. The parser has no
    /// notion of scope, so it leaves this `false` and the engine sets it.
    pub is_internal: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[repr(i64)]
pub enum LinkSource {
    Anchor = 0,
    Image = 1,
    Stylesheet = 2,
    Script = 3,
    Canonical = 4,
    Hreflang = 5,
    Redirect = 6,
    Sitemap = 7,
    Form = 8,
    Iframe = 9,
    MetaRefresh = 10,
}

/// One `<img>` on a page.
#[derive(Debug, Clone, PartialEq)]
pub struct DiscoveredImage {
    pub raw_src: String,
    pub src: Option<String>,
    pub src_key: Option<String>,
    /// `None` when the attribute is absent (an accessibility problem);
    /// `Some("")` for a deliberately empty decorative alt.
    pub alt: Option<String>,
    pub title: Option<String>,
    /// Declared `width`/`height` attributes, when numeric.
    pub declared_width: Option<u32>,
    pub declared_height: Option<u32>,
    pub loading_lazy: bool,
}

/// A `<link rel="alternate" hreflang="...">` entry.
#[derive(Debug, Clone, PartialEq)]
pub struct HreflangEntry {
    pub lang: String,
    pub raw_href: String,
    pub href: Option<String>,
    pub href_key: Option<String>,
}

/// An Open Graph or Twitter card tag.
#[derive(Debug, Clone, PartialEq)]
pub struct SocialTag {
    /// `"og"` or `"twitter"`.
    pub namespace: String,
    /// `"title"`, `"image"`, ...
    pub property: String,
    pub content: String,
}

/// A structured-data block found on the page.
#[derive(Debug, Clone, PartialEq)]
pub struct StructuredDataBlock {
    /// `"json-ld"` or `"microdata"`.
    pub format: String,
    /// Schema types in the block, e.g. `["Product", "Offer"]`.
    pub types: Vec<String>,
    /// Whether the block parsed as valid JSON.
    pub valid: bool,
    pub error: Option<String>,
}

/// One hop in a redirect chain.
#[derive(Debug, Clone, PartialEq)]
pub struct RedirectHop {
    pub hop: u32,
    pub from_url: String,
    pub to_url: String,
    pub status: u16,
}

/// Everything the HTML parser extracts from one document.
///
/// Produced by [`crate::parse::parse_html`], which never fails: a document that
/// cannot be understood simply yields empty fields.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ParsedPage {
    pub title: Option<String>,
    pub meta_description: Option<String>,
    pub meta_robots: Option<String>,
    pub meta_refresh: Option<String>,
    pub canonical_raw: Option<String>,
    pub canonical: Option<String>,
    pub canonical_key: Option<String>,
    pub html_lang: Option<String>,
    pub charset: Option<String>,
    pub has_viewport: bool,
    pub amp_html: Option<String>,

    pub h1: Vec<String>,
    pub h2: Vec<String>,

    pub word_count: u32,
    /// Hash of the extracted visible text, for exact-duplicate detection.
    pub text_hash: Option<String>,
    /// 64-bit simhash of the text's shingles, for near-duplicate detection.
    pub text_simhash: Option<u64>,

    pub links: Vec<DiscoveredLink>,
    pub images: Vec<DiscoveredImage>,
    pub hreflang: Vec<HreflangEntry>,
    pub social: Vec<SocialTag>,
    pub structured_data: Vec<StructuredDataBlock>,
    /// What the markup says about this page's accessibility.
    pub accessibility: crate::parse::accessibility::AccessibilityReport,
}

impl ParsedPage {
    /// Number of `<a href>` links pointing at `internal` targets.
    pub fn link_counts(&self, is_internal: impl Fn(&str) -> bool) -> (u32, u32) {
        let mut internal = 0;
        let mut external = 0;
        for link in self.links.iter().filter(|l| l.source == LinkSource::Anchor) {
            match &link.target {
                Some(t) if is_internal(t) => internal += 1,
                Some(_) => external += 1,
                None => {}
            }
        }
        (internal, external)
    }

    /// Distinct schema.org types across every structured-data block.
    pub fn structured_data_types(&self) -> Vec<String> {
        let mut types: Vec<String> = self
            .structured_data
            .iter()
            .flat_map(|b| b.types.clone())
            .collect();
        types.sort();
        types.dedup();
        types
    }
}

/// The complete result of visiting one URL: the row to write plus its children.
#[derive(Debug, Clone)]
pub struct CrawledUrl {
    /// `urls.id` of the row this result belongs to.
    pub url_id: i64,
    pub url: String,
    pub state: UrlState,
    pub kind: ResourceKind,

    /// The status the URL itself returned — a 3xx for a redirect.
    pub status_code: Option<u16>,
    /// The status at the end of the redirect chain, when there was one.
    pub final_status_code: Option<u16>,
    pub response_time: Option<Duration>,
    /// Time to the first byte of the response.
    pub ttfb: Option<Duration>,
    pub content_type: Option<String>,
    pub content_length: Option<u64>,
    /// Bytes actually received (may differ from `Content-Length`).
    pub body_bytes: Option<u64>,
    pub content_encoding: Option<String>,
    pub x_robots_tag: Option<String>,
    /// Final URL after redirects, when it differs from `url`.
    pub final_url: Option<String>,
    pub redirect_chain: Vec<RedirectHop>,
    /// The chain came back to a URL it had already visited.
    pub redirect_loop: bool,
    pub headers: Vec<(String, String)>,
    /// Hash of the raw response body.
    pub content_hash: Option<String>,
    /// The body hit the size cap and was cut short, so page fields derived from
    /// it are incomplete.
    pub truncated: bool,
    pub html: Option<String>,

    pub parsed: Option<ParsedPage>,
    pub internal_links: u32,
    pub external_links: u32,

    pub error_kind: Option<String>,
    pub error_message: Option<String>,
    pub attempts: u8,
    /// Crawl-relative seconds at which this URL was fetched.
    pub fetched_at: i64,
}

/// A URL newly discovered on a page, ready to be inserted into the frontier.
#[derive(Debug, Clone, PartialEq)]
pub struct FrontierUrl {
    pub url: String,
    pub key: String,
    pub depth: u32,
    pub discovered_from: Option<i64>,
    /// How this URL was found. `None` marks a seed — a URL the user asked for
    /// directly — which is what lets the orphan-page audit tell "nothing links
    /// here" apart from "this is where the crawl started".
    pub via: Option<LinkSource>,
    pub kind: ResourceKind,
    pub is_internal: bool,
    pub state: UrlState,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_type_wins_over_extension() {
        assert_eq!(
            ResourceKind::from_content_type(Some("text/html; charset=utf-8"), "/a.pdf"),
            ResourceKind::Html
        );
        assert_eq!(
            ResourceKind::from_content_type(Some("image/webp"), "/a"),
            ResourceKind::Image
        );
        assert_eq!(
            ResourceKind::from_content_type(Some("application/pdf"), "/report"),
            ResourceKind::Document
        );
    }

    #[test]
    fn generic_content_type_falls_back_to_the_extension() {
        assert_eq!(
            ResourceKind::from_content_type(Some("application/octet-stream"), "/logo.png"),
            ResourceKind::Image
        );
        assert_eq!(
            ResourceKind::from_content_type(None, "/style.css"),
            ResourceKind::Css
        );
    }

    #[test]
    fn extensionless_and_slug_paths_are_treated_as_pages() {
        assert_eq!(
            ResourceKind::from_extension("/blog/my-post"),
            ResourceKind::Html
        );
        assert_eq!(ResourceKind::from_extension("/"), ResourceKind::Html);
        assert_eq!(
            ResourceKind::from_extension("/2024/report.v2"),
            ResourceKind::Html
        );
    }

    #[test]
    fn url_state_round_trips() {
        for state in [
            UrlState::Queued,
            UrlState::InProgress,
            UrlState::Done,
            UrlState::Failed,
            UrlState::Skipped,
            UrlState::BlockedByRobots,
        ] {
            assert_eq!(UrlState::from_i64(state as i64), Some(state));
        }
        assert_eq!(UrlState::from_i64(99), None);
    }

    #[test]
    fn link_counts_only_consider_anchors() {
        let link = |target: &str, source| DiscoveredLink {
            raw_href: target.into(),
            target: Some(target.into()),
            target_key: Some(target.into()),
            anchor_text: String::new(),
            rel: None,
            position: LinkPosition::Content,
            nofollow: false,
            source,
            is_internal: true,
        };
        let page = ParsedPage {
            links: vec![
                link("https://example.com/a", LinkSource::Anchor),
                link("https://other.com/b", LinkSource::Anchor),
                link("https://example.com/c.png", LinkSource::Image),
            ],
            ..Default::default()
        };
        let (internal, external) = page.link_counts(|u| u.starts_with("https://example.com"));
        assert_eq!((internal, external), (1, 1));
    }
}
