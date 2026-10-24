//! Fetching one URL.
//!
//! Everything the crawler needs from the network goes through the [`Fetcher`]
//! trait. There are two implementations: [`HttpFetcher`], used in production,
//! and [`mock::MockFetcher`], which serves canned responses so the engine's
//! control flow can be tested without a network or a server.
//!
//! The trait is also the seam for the headless-Chrome renderer planned after
//! v1: a `ChromeFetcher` returning rendered HTML slots in without the engine
//! changing.
//!
//! ## Why redirects are followed by hand
//!
//! `reqwest` can follow redirects itself, but it does not report the hops it
//! passed through. For an SEO crawler the chain *is* the finding — "this URL
//! takes three hops to reach a 200" and "these two URLs redirect to each other"
//! are both audits. So the client is configured with no redirect policy and
//! this module walks the chain, recording each hop and detecting loops.

use std::sync::OnceLock;
use std::time::{Duration, Instant};

use futures::future::BoxFuture;
use reqwest::header::{HeaderMap, HeaderValue, ACCEPT, ACCEPT_LANGUAGE, USER_AGENT};
use reqwest::redirect::Policy;
use url::Url;

use crate::config::CompiledConfig;
use crate::error::FetchError;
use crate::model::RedirectHop;

/// HTTP method. Only the two a crawler needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Method {
    Get,
    /// Used for external links and non-parsed resources, where only the status
    /// and size matter.
    Head,
}

/// What to fetch.
#[derive(Debug, Clone)]
pub struct FetchRequest {
    pub url: Url,
    pub method: Method,
    /// Stop reading the body after this many bytes.
    pub max_body_bytes: usize,
    pub max_redirects: usize,
}

impl FetchRequest {
    pub fn get(url: Url) -> Self {
        Self {
            url,
            method: Method::Get,
            max_body_bytes: usize::MAX,
            max_redirects: 10,
        }
    }

    pub fn head(url: Url) -> Self {
        Self {
            url,
            method: Method::Head,
            max_body_bytes: 0,
            max_redirects: 10,
        }
    }
}

/// A completed fetch, including every redirect hop traversed.
#[derive(Debug, Clone)]
pub struct FetchedResponse {
    /// The URL that finally answered, after redirects.
    pub final_url: Url,
    /// The status the *requested* URL returned. For a redirect chain this is
    /// the first hop's 3xx, which is what an SEO report shows against the URL.
    pub status: u16,
    /// The status after following every redirect. Equal to `status` when there
    /// were none.
    pub final_status: u16,
    /// All response headers of the final response, lowercased names.
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
    /// Hops traversed to get here. Empty for a direct response.
    pub redirect_chain: Vec<RedirectHop>,
    /// Two URLs in the chain repeated: the site has a redirect loop.
    pub redirect_loop: bool,
    /// Total wall time, including redirects.
    pub response_time: Duration,
    /// Time until the final response's headers arrived.
    pub ttfb: Duration,
    /// The body hit `max_body_bytes` and was cut short.
    pub truncated: bool,
}

impl FetchedResponse {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }

    /// Every value of a repeated header, e.g. multiple `X-Robots-Tag` lines.
    pub fn header_all(&self, name: &str) -> Vec<&str> {
        self.headers
            .iter()
            .filter(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
            .collect()
    }

    pub fn content_type(&self) -> Option<&str> {
        self.header("content-type")
    }

    /// Whether the response that carried the body was a success, following any
    /// redirects. A sitemap behind a 301 still parses.
    pub fn is_success(&self) -> bool {
        (200..300).contains(&self.final_status)
    }

    /// Whether the requested URL was itself a redirect.
    pub fn is_redirect(&self) -> bool {
        !self.redirect_chain.is_empty()
    }
}

/// Fetches URLs. Implementations must be cheap to clone or share behind an
/// `Arc`, and safe to call concurrently.
pub trait Fetcher: Send + Sync + 'static {
    fn fetch<'a>(
        &'a self,
        request: FetchRequest,
    ) -> BoxFuture<'a, Result<FetchedResponse, FetchError>>;
}

/// `rustls` needs a process-wide crypto provider before any TLS connection.
///
/// We pick `ring` rather than the default `aws-lc-rs` because it builds without
/// cmake or NASM, which keeps the Windows and Linux release builds simple.
fn install_crypto_provider() {
    static INSTALLED: OnceLock<()> = OnceLock::new();
    INSTALLED.get_or_init(|| {
        // An error here means another part of the process installed one first,
        // which is fine.
        let _ = rustls::crypto::ring::default_provider().install_default();
    });
}

/// The real fetcher: `reqwest` over rustls with HTTP/2 and compression.
#[derive(Debug)]
pub struct HttpFetcher {
    client: reqwest::Client,
}

impl HttpFetcher {
    /// Build a client from the crawl configuration.
    pub fn new(config: &CompiledConfig) -> Result<Self, FetchError> {
        install_crypto_provider();

        let cfg = &config.config;
        let mut headers = HeaderMap::new();
        headers.insert(
            ACCEPT,
            HeaderValue::from_static(
                "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8",
            ),
        );
        headers.insert(
            ACCEPT_LANGUAGE,
            HeaderValue::from_static("en;q=0.9,*;q=0.5"),
        );
        headers.insert(
            USER_AGENT,
            HeaderValue::from_str(&cfg.user_agent)
                .map_err(|e| FetchError::Request(format!("invalid user agent: {e}")))?,
        );

        let client = reqwest::Client::builder()
            .default_headers(headers)
            // Redirects are followed by this module, not by reqwest — see the
            // module docs.
            .redirect(Policy::none())
            .timeout(cfg.request_timeout())
            .connect_timeout(cfg.connect_timeout())
            // Keep connections warm: a crawl hits one host thousands of times.
            .pool_max_idle_per_host(cfg.concurrency_per_host.max(2))
            .pool_idle_timeout(Duration::from_secs(90))
            .use_rustls_tls()
            .build()
            .map_err(|e| FetchError::Request(e.to_string()))?;

        Ok(Self { client })
    }

    /// Build a client with default settings. Handy for sitemap and robots
    /// fetches outside a crawl.
    pub fn with_user_agent(user_agent: &str) -> Result<Self, FetchError> {
        let mut config = crate::config::CrawlConfig::spider("https://example.com");
        config.user_agent = user_agent.to_string();
        let compiled = config
            .compile()
            .map_err(|e| FetchError::Request(e.to_string()))?;
        Self::new(&compiled)
    }

    async fn fetch_inner(&self, request: FetchRequest) -> Result<FetchedResponse, FetchError> {
        let started = Instant::now();
        let mut url = request.url.clone();
        let mut chain: Vec<RedirectHop> = Vec::new();
        // Chain membership, for loop detection.
        let mut seen: Vec<String> = vec![url.to_string()];
        // The first response's status, kept separately from the last one's.
        let mut initial_status: Option<u16> = None;

        loop {
            let method = match request.method {
                Method::Get => reqwest::Method::GET,
                Method::Head => reqwest::Method::HEAD,
            };

            let headers_at = Instant::now();
            let response = self
                .client
                .request(method, url.clone())
                .send()
                .await
                .map_err(classify)?;
            let ttfb = headers_at.elapsed();

            let status = response.status().as_u16();
            initial_status.get_or_insert(status);
            let headers = collect_headers(response.headers());

            // 3xx with a Location: record the hop and keep walking.
            let location = if (300..400).contains(&status) {
                response
                    .headers()
                    .get(reqwest::header::LOCATION)
                    .and_then(|v| v.to_str().ok())
                    .map(str::to_string)
            } else {
                None
            };

            if let Some(location) = location {
                // A relative Location is legal and common.
                let Ok(next) = url.join(&location) else {
                    // An unparseable Location ends the chain; the 3xx itself is
                    // still the result, and the audit will flag it.
                    return Ok(FetchedResponse {
                        final_url: url,
                        status: initial_status.unwrap_or(status),
                        final_status: status,
                        headers,
                        body: Vec::new(),
                        redirect_chain: chain,
                        redirect_loop: false,
                        response_time: started.elapsed(),
                        ttfb,
                        truncated: false,
                    });
                };

                chain.push(RedirectHop {
                    hop: chain.len() as u32 + 1,
                    from_url: url.to_string(),
                    to_url: next.to_string(),
                    status,
                });

                let next_str = next.to_string();
                if seen.contains(&next_str) {
                    // A loop: stop here and report it rather than spinning.
                    return Ok(FetchedResponse {
                        final_url: url,
                        status: initial_status.unwrap_or(status),
                        final_status: status,
                        headers,
                        body: Vec::new(),
                        redirect_chain: chain,
                        redirect_loop: true,
                        response_time: started.elapsed(),
                        ttfb,
                        truncated: false,
                    });
                }
                if chain.len() > request.max_redirects {
                    return Err(FetchError::TooManyRedirects(chain.len()));
                }
                seen.push(next_str);
                url = next;
                continue;
            }

            // Final response: read the body, up to the cap.
            let (body, truncated) = if request.method == Method::Head {
                (Vec::new(), false)
            } else {
                read_capped_body(response, request.max_body_bytes).await?
            };

            return Ok(FetchedResponse {
                final_url: url,
                status: initial_status.unwrap_or(status),
                final_status: status,
                headers,
                body,
                redirect_chain: chain,
                redirect_loop: false,
                response_time: started.elapsed(),
                ttfb,
                truncated,
            });
        }
    }
}

impl Fetcher for HttpFetcher {
    fn fetch<'a>(
        &'a self,
        request: FetchRequest,
    ) -> BoxFuture<'a, Result<FetchedResponse, FetchError>> {
        Box::pin(self.fetch_inner(request))
    }
}

/// Read a response body, stopping at `limit` bytes.
///
/// Exceeding the limit is not an error: a 40 MB "page" still has a status, a
/// content type and a size worth recording. We just stop reading and mark the
/// body truncated so it is never parsed or hashed as if complete.
async fn read_capped_body(
    mut response: reqwest::Response,
    limit: usize,
) -> Result<(Vec<u8>, bool), FetchError> {
    // Pre-allocate from Content-Length when it is sane, so a 1 MB page is one
    // allocation rather than twenty.
    let hint = response
        .content_length()
        .map(|l| l.min(limit as u64) as usize)
        .unwrap_or(16 * 1024)
        .min(limit);
    let mut body = Vec::with_capacity(hint);

    while let Some(chunk) = response.chunk().await.map_err(classify)? {
        let remaining = limit.saturating_sub(body.len());
        if remaining == 0 {
            return Ok((body, true));
        }
        if chunk.len() > remaining {
            body.extend_from_slice(&chunk[..remaining]);
            return Ok((body, true));
        }
        body.extend_from_slice(&chunk);
    }
    Ok((body, false))
}

fn collect_headers(headers: &HeaderMap) -> Vec<(String, String)> {
    headers
        .iter()
        .filter_map(|(name, value)| {
            value
                .to_str()
                .ok()
                .map(|v| (name.as_str().to_ascii_lowercase(), v.to_string()))
        })
        .collect()
}

/// Turn a `reqwest` error into our stable error taxonomy.
fn classify(err: reqwest::Error) -> FetchError {
    if err.is_timeout() {
        return FetchError::Timeout(Duration::ZERO);
    }
    if err.is_connect() {
        let message = err.to_string();
        let lower = message.to_lowercase();
        if lower.contains("dns") || lower.contains("resolve") {
            return FetchError::Dns(message);
        }
        if lower.contains("certificate")
            || lower.contains("tls")
            || lower.contains("ssl")
            || lower.contains("handshake")
        {
            return FetchError::Tls(message);
        }
        return FetchError::Connect(message);
    }
    if err.is_redirect() {
        return FetchError::TooManyRedirects(0);
    }
    FetchError::Request(err.to_string())
}

/// A fetcher that serves canned responses, for tests.
pub mod mock {
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Mutex;
    use std::time::Duration;

    use futures::future::BoxFuture;
    use url::Url;

    use super::{FetchRequest, FetchedResponse, Fetcher};
    use crate::error::FetchError;
    use crate::model::RedirectHop;

    /// One canned response.
    #[derive(Debug, Clone)]
    pub struct MockResponse {
        pub status: u16,
        pub content_type: String,
        pub body: String,
        /// Serve a redirect to this URL instead of a body.
        pub redirect_to: Option<String>,
        pub headers: Vec<(String, String)>,
        /// Fail the fetch instead of answering.
        pub error: Option<FetchError>,
    }

    impl MockResponse {
        pub fn html(body: impl Into<String>) -> Self {
            Self {
                status: 200,
                content_type: "text/html; charset=utf-8".into(),
                body: body.into(),
                redirect_to: None,
                headers: Vec::new(),
                error: None,
            }
        }

        pub fn status(status: u16) -> Self {
            Self {
                status,
                body: String::new(),
                ..Self::html("")
            }
        }

        pub fn redirect(status: u16, to: impl Into<String>) -> Self {
            Self {
                status,
                redirect_to: Some(to.into()),
                ..Self::html("")
            }
        }

        pub fn failure(error: FetchError) -> Self {
            Self {
                error: Some(error),
                ..Self::html("")
            }
        }

        pub fn with_header(mut self, name: &str, value: &str) -> Self {
            self.headers
                .push((name.to_ascii_lowercase(), value.to_string()));
            self
        }
    }

    /// Serves [`MockResponse`]s by URL. Unknown URLs get a 404.
    ///
    /// Also used by the engine tests to assert on *which* URLs were requested
    /// and how many times.
    #[derive(Default)]
    pub struct MockFetcher {
        responses: Mutex<HashMap<String, MockResponse>>,
        requested: Mutex<Vec<String>>,
        count: AtomicUsize,
    }

    impl MockFetcher {
        pub fn new() -> Self {
            Self::default()
        }

        pub fn insert(&self, url: &str, response: MockResponse) -> &Self {
            self.responses
                .lock()
                .unwrap()
                .insert(url.to_string(), response);
            self
        }

        /// Add an HTML page, the common case.
        pub fn page(&self, url: &str, body: impl Into<String>) -> &Self {
            self.insert(url, MockResponse::html(body))
        }

        pub fn requested(&self) -> Vec<String> {
            self.requested.lock().unwrap().clone()
        }

        pub fn request_count(&self) -> usize {
            self.count.load(Ordering::Relaxed)
        }

        pub fn times_requested(&self, url: &str) -> usize {
            self.requested
                .lock()
                .unwrap()
                .iter()
                .filter(|u| *u == url)
                .count()
        }
    }

    impl Fetcher for MockFetcher {
        fn fetch<'a>(
            &'a self,
            request: FetchRequest,
        ) -> BoxFuture<'a, Result<FetchedResponse, FetchError>> {
            Box::pin(async move {
                let key = request.url.to_string();
                self.count.fetch_add(1, Ordering::Relaxed);
                self.requested.lock().unwrap().push(key.clone());

                let canned = self.responses.lock().unwrap().get(&key).cloned();
                let Some(canned) = canned else {
                    return Ok(FetchedResponse {
                        final_url: request.url,
                        status: 404,
                        final_status: 404,
                        headers: vec![("content-type".into(), "text/html".into())],
                        body: b"<html><body>not found</body></html>".to_vec(),
                        redirect_chain: Vec::new(),
                        redirect_loop: false,
                        response_time: Duration::from_millis(1),
                        ttfb: Duration::from_millis(1),
                        truncated: false,
                    });
                };

                if let Some(error) = canned.error {
                    return Err(error);
                }

                // Walk a redirect chain through the canned responses, exactly
                // as HttpFetcher would.
                let mut url = request.url.clone();
                let mut current = canned;
                let mut chain: Vec<RedirectHop> = Vec::new();
                let mut seen = vec![url.to_string()];
                let initial_status = current.status;

                while let Some(target) = current.redirect_to.clone() {
                    let Ok(next) = url.join(&target) else { break };
                    chain.push(RedirectHop {
                        hop: chain.len() as u32 + 1,
                        from_url: url.to_string(),
                        to_url: next.to_string(),
                        status: current.status,
                    });
                    let next_str = next.to_string();
                    let looped = seen.contains(&next_str);
                    if looped || chain.len() > request.max_redirects {
                        return Ok(FetchedResponse {
                            final_url: url,
                            status: initial_status,
                            final_status: current.status,
                            headers: vec![("location".into(), target)],
                            body: Vec::new(),
                            redirect_chain: chain,
                            redirect_loop: looped,
                            response_time: Duration::from_millis(1),
                            ttfb: Duration::from_millis(1),
                            truncated: false,
                        });
                    }
                    seen.push(next_str.clone());
                    url = next;
                    self.count.fetch_add(1, Ordering::Relaxed);
                    self.requested.lock().unwrap().push(next_str.clone());
                    current = self
                        .responses
                        .lock()
                        .unwrap()
                        .get(&next_str)
                        .cloned()
                        .unwrap_or_else(|| MockResponse::status(404));
                }

                let mut headers = vec![("content-type".to_string(), current.content_type.clone())];
                headers.extend(current.headers.clone());
                let body = current.body.into_bytes();
                headers.push(("content-length".into(), body.len().to_string()));

                Ok(FetchedResponse {
                    final_url: Url::parse(url.as_str()).unwrap(),
                    status: initial_status,
                    final_status: current.status,
                    headers,
                    body,
                    redirect_chain: chain,
                    redirect_loop: false,
                    response_time: Duration::from_millis(1),
                    ttfb: Duration::from_millis(1),
                    truncated: false,
                })
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::mock::{MockFetcher, MockResponse};
    use super::*;

    #[tokio::test]
    async fn mock_serves_pages_and_404s_unknown_urls() {
        let fetcher = MockFetcher::new();
        fetcher.page("https://a.test/", "<h1>Home</h1>");

        let response = fetcher
            .fetch(FetchRequest::get(Url::parse("https://a.test/").unwrap()))
            .await
            .unwrap();
        assert_eq!(response.status, 200);
        assert_eq!(response.body, b"<h1>Home</h1>");
        assert_eq!(response.content_type(), Some("text/html; charset=utf-8"));

        let missing = fetcher
            .fetch(FetchRequest::get(
                Url::parse("https://a.test/nope").unwrap(),
            ))
            .await
            .unwrap();
        assert_eq!(missing.status, 404);
        assert_eq!(fetcher.request_count(), 2);
    }

    #[tokio::test]
    async fn mock_follows_and_records_a_redirect_chain() {
        let fetcher = MockFetcher::new();
        fetcher.insert("https://a.test/1", MockResponse::redirect(301, "/2"));
        fetcher.insert("https://a.test/2", MockResponse::redirect(302, "/3"));
        fetcher.page("https://a.test/3", "done");

        let response = fetcher
            .fetch(FetchRequest::get(Url::parse("https://a.test/1").unwrap()))
            .await
            .unwrap();

        // `status` is what the requested URL answered; `final_status` is what
        // the end of the chain answered.
        assert_eq!(response.status, 301);
        assert_eq!(response.final_status, 200);
        assert!(response.is_success());
        assert!(response.is_redirect());
        assert_eq!(response.final_url.as_str(), "https://a.test/3");
        assert_eq!(response.redirect_chain.len(), 2);
        assert_eq!(response.redirect_chain[0].status, 301);
        assert_eq!(response.redirect_chain[0].to_url, "https://a.test/2");
        assert_eq!(response.redirect_chain[1].hop, 2);
        assert!(!response.redirect_loop);
    }

    #[tokio::test]
    async fn mock_detects_a_redirect_loop() {
        let fetcher = MockFetcher::new();
        fetcher.insert("https://a.test/x", MockResponse::redirect(301, "/y"));
        fetcher.insert("https://a.test/y", MockResponse::redirect(301, "/x"));

        let response = fetcher
            .fetch(FetchRequest::get(Url::parse("https://a.test/x").unwrap()))
            .await
            .unwrap();
        assert!(response.redirect_loop);
        assert_eq!(response.redirect_chain.len(), 2);
    }

    #[tokio::test]
    async fn fetch_errors_propagate() {
        let fetcher = MockFetcher::new();
        fetcher.insert(
            "https://a.test/slow",
            MockResponse::failure(FetchError::Timeout(Duration::from_secs(20))),
        );
        let err = fetcher
            .fetch(FetchRequest::get(
                Url::parse("https://a.test/slow").unwrap(),
            ))
            .await
            .unwrap_err();
        assert_eq!(err.kind(), "timeout");
        assert!(err.is_retryable());
    }

    #[test]
    fn header_lookup_is_case_insensitive_and_handles_repeats() {
        let response = FetchedResponse {
            final_url: Url::parse("https://a.test/").unwrap(),
            status: 200,
            final_status: 200,
            headers: vec![
                ("content-type".into(), "text/html".into()),
                ("x-robots-tag".into(), "noindex".into()),
                ("x-robots-tag".into(), "nofollow".into()),
            ],
            body: Vec::new(),
            redirect_chain: Vec::new(),
            redirect_loop: false,
            response_time: Duration::ZERO,
            ttfb: Duration::ZERO,
            truncated: false,
        };
        assert_eq!(response.header("Content-Type"), Some("text/html"));
        assert_eq!(
            response.header_all("X-Robots-Tag"),
            vec!["noindex", "nofollow"]
        );
        assert!(response.is_success());
    }

    #[test]
    fn http_client_builds_from_config() {
        let config = crate::config::CrawlConfig::spider("https://a.test")
            .compile()
            .unwrap();
        assert!(HttpFetcher::new(&config).is_ok());
    }

    #[test]
    fn an_invalid_user_agent_is_rejected_clearly() {
        let mut config = crate::config::CrawlConfig::spider("https://a.test");
        config.user_agent = "bad\nagent".into();
        let config = config.compile().unwrap();
        let err = HttpFetcher::new(&config).unwrap_err();
        assert!(err.to_string().contains("invalid user agent"));
    }
}
