//! A local test site with every awkward thing a crawler meets.
//!
//! Served by `axum` on an ephemeral port, so the integration tests exercise the
//! real HTTP stack — TLS aside — including redirect chains, loops, error
//! statuses, robots.txt, sitemaps, canonicals and oversized bodies.
//!
//! Every route is deterministic: the same crawl always produces the same
//! database, which is what lets the tests assert exact counts.

// Shared by several test binaries, each of which uses a different subset.
#![allow(dead_code)]

use std::net::SocketAddr;

use axum::extract::Path;
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;

/// A running fixture site. Shuts down when dropped.
pub struct FixtureSite {
    pub base: String,
    shutdown: Option<tokio::sync::oneshot::Sender<()>>,
    handle: Option<tokio::task::JoinHandle<()>>,
}

impl FixtureSite {
    /// Absolute URL for a path on the fixture site.
    pub fn url(&self, path: &str) -> String {
        format!("{}{}", self.base, path)
    }
}

impl Drop for FixtureSite {
    fn drop(&mut self) {
        if let Some(shutdown) = self.shutdown.take() {
            let _ = shutdown.send(());
        }
        if let Some(handle) = self.handle.take() {
            handle.abort();
        }
    }
}

/// How many pages `/page/{n}` serves. They are generated on request, so this
/// costs nothing until a crawl actually asks for them; the performance tests
/// crawl tens of thousands.
pub const GENERATED_PAGES: usize = 60_000;

/// Start the fixture site on a free port.
pub async fn spawn() -> FixtureSite {
    let app = router();
    let listener = tokio::net::TcpListener::bind(SocketAddr::from(([127, 0, 0, 1], 0)))
        .await
        .expect("a free port");
    let addr = listener.local_addr().expect("the bound address");
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();

    let handle = tokio::spawn(async move {
        let _ = axum::serve(listener, app)
            .with_graceful_shutdown(async {
                let _ = shutdown_rx.await;
            })
            .await;
    });

    FixtureSite {
        base: format!("http://{addr}"),
        shutdown: Some(shutdown_tx),
        handle: Some(handle),
    }
}

/// A TCP port nothing is listening on, for testing connection failures.
pub async fn closed_port() -> u16 {
    let listener = tokio::net::TcpListener::bind(SocketAddr::from(([127, 0, 0, 1], 0)))
        .await
        .expect("a free port");
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    port
}

fn router() -> Router {
    Router::new()
        .route("/", get(home))
        .route("/about", get(about))
        .route("/contact", get(contact))
        // Redirect chain: /redirect/1 -> /redirect/2 -> /redirect/3 -> /about
        .route("/redirect/{step}", get(redirect_step))
        // Two URLs redirecting to each other.
        .route("/loop/a", get(|| async { redirect(302, "/loop/b") }))
        .route("/loop/b", get(|| async { redirect(302, "/loop/a") }))
        .route(
            "/missing",
            get(|| async { (StatusCode::NOT_FOUND, html("<h1>Not found</h1>")) }),
        )
        .route(
            "/server-error",
            get(|| async { (StatusCode::INTERNAL_SERVER_ERROR, html("<h1>Oops</h1>")) }),
        )
        .route("/slow", get(slow))
        .route("/canonicalised", get(canonicalised))
        .route("/noindex", get(noindex))
        .route("/x-robots", get(x_robots))
        .route(
            "/secret/hidden",
            get(|| async { html("<h1>Disallowed by robots</h1>") }),
        )
        .route("/robots.txt", get(robots_txt))
        .route("/sitemap.xml", get(sitemap_index))
        .route("/sitemap-pages.xml", get(sitemap_pages))
        .route(
            "/orphan",
            get(|| async { html("<h1>Orphan</h1><p>Nothing links here.</p>") }),
        )
        .route("/dup-a", get(duplicate))
        .route("/dup-b", get(duplicate))
        .route("/near-dup", get(near_duplicate))
        .route(
            "/no-title",
            get(|| async { html("<body><p>A page with no title.</p></body>") }),
        )
        .route(
            "/multi-h1",
            get(|| async { html("<h1>One</h1><h1>Two</h1>") }),
        )
        .route("/big", get(big_body))
        .route("/image.png", get(image))
        .route("/styles.css", get(stylesheet))
        .route("/report.pdf", get(pdf))
        .route("/hreflang/en", get(hreflang_en))
        .route("/hreflang/fr", get(hreflang_fr))
        .route("/hreflang/de", get(hreflang_de))
        .route("/page/{n}", get(generated_page))
        .route("/head-only", get(head_only))
}

// --- routes ---------------------------------------------------------------

async fn home() -> Response {
    html(
        r#"<!DOCTYPE html><html lang="en"><head>
             <title>Fixture home</title>
             <meta name="description" content="The CrawlStat fixture site home page.">
             <link rel="canonical" href="/">
           </head><body>
             <header><a href="/">Home</a></header>
             <nav>
               <a href="/about">About</a>
               <a href="/contact">Contact</a>
               <a href="/page/1">Pages</a>
             </nav>
             <main>
               <h1>Fixture home</h1>
               <h2>Things that go wrong</h2>
               <p>This page links to every awkward case the crawler must handle.</p>
               <ul>
                 <li><a href="/redirect/1">Redirect chain</a></li>
                 <li><a href="/loop/a">Redirect loop</a></li>
                 <li><a href="/missing">Missing page</a></li>
                 <li><a href="/server-error">Server error</a></li>
                 <li><a href="/slow">Slow page</a></li>
                 <li><a href="/canonicalised">Canonicalised</a></li>
                 <li><a href="/noindex">Noindex</a></li>
                 <li><a href="/x-robots">X-Robots-Tag</a></li>
                 <li><a href="/secret/hidden">Disallowed</a></li>
                 <li><a href="/dup-a">Duplicate A</a></li>
                 <li><a href="/dup-b">Duplicate B</a></li>
                 <li><a href="/near-dup">Near duplicate</a></li>
                 <li><a href="/no-title">No title</a></li>
                 <li><a href="/multi-h1">Two H1s</a></li>
                 <li><a href="/report.pdf">A PDF</a></li>
                 <li><a href="/hreflang/en">Hreflang</a></li>
                 <li><a href="/nofollow" rel="nofollow">Nofollow</a></li>
                 <li><a href="https://example.com/external">External</a></li>
                 <li><a href="mailto:hello@example.com">Email</a></li>
               </ul>
               <img src="/image.png" alt="A test image" width="1200" height="900">
               <img src="/image.png">
               <script type="application/ld+json">
                 {"@context":"https://schema.org","@type":"WebSite","name":"Fixture"}
               </script>
             </main>
             <footer><a href="/about">About us</a></footer>
             <link rel="stylesheet" href="/styles.css">
           </body></html>"#,
    )
}

async fn about() -> Response {
    html(
        r#"<html><head><title>About the fixture site</title>
             <meta name="description" content="About page.">
             <link rel="canonical" href="/about"></head>
           <body><h1>About</h1>
             <p>Some words so the page has a word count worth measuring here.</p>
             <a href="/">Home</a><a href="/contact">Contact</a>
           </body></html>"#,
    )
}

async fn contact() -> Response {
    html(
        r#"<html><head><title>Contact</title></head>
           <body><h1>Contact</h1><a href="/">Home</a>
             <p>Write to us at the address below, we answer within two days.</p>
           </body></html>"#,
    )
}

async fn redirect_step(Path(step): Path<u32>) -> Response {
    match step {
        1 => redirect(301, "/redirect/2"),
        2 => redirect(302, "/redirect/3"),
        3 => redirect(301, "/about"),
        _ => (StatusCode::NOT_FOUND, html("<h1>No such step</h1>")).into_response(),
    }
}

async fn slow() -> Response {
    tokio::time::sleep(std::time::Duration::from_millis(400)).await;
    html("<html><head><title>Slow</title></head><body><h1>Slow page</h1></body></html>")
}

async fn canonicalised() -> Response {
    html(
        r#"<html><head><title>Canonicalised</title>
             <link rel="canonical" href="/"></head>
           <body><h1>Canonicalised to the home page</h1></body></html>"#,
    )
}

async fn noindex() -> Response {
    html(
        r#"<html><head><title>Noindex</title>
             <meta name="robots" content="noindex, follow"></head>
           <body><h1>Not for the index</h1><a href="/">Home</a></body></html>"#,
    )
}

async fn x_robots() -> Response {
    let mut headers = HeaderMap::new();
    headers.insert("x-robots-tag", "noindex, nofollow".parse().unwrap());
    headers.insert(
        header::CONTENT_TYPE,
        "text/html; charset=utf-8".parse().unwrap(),
    );
    (
        headers,
        "<html><head><title>X-Robots</title></head><body><h1>Header</h1></body></html>",
    )
        .into_response()
}

/// Advertises the sitemap with an absolute URL, as the protocol requires. The
/// sitemap's own `<loc>` entries stay relative, which is both common in the
/// wild and a test of the crawler's resolution.
async fn robots_txt(headers: HeaderMap) -> Response {
    let host = headers
        .get(header::HOST)
        .and_then(|h| h.to_str().ok())
        .unwrap_or("127.0.0.1");
    let body = format!(
        "User-agent: *\n\
         Disallow: /secret/\n\
         \n\
         Sitemap: http://{host}/sitemap.xml\n"
    );
    ([(header::CONTENT_TYPE, "text/plain; charset=utf-8")], body).into_response()
}

async fn sitemap_index() -> Response {
    xml(r#"<?xml version="1.0" encoding="UTF-8"?>
<sitemapindex xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">
  <sitemap><loc>/sitemap-pages.xml</loc><lastmod>2026-02-01</lastmod></sitemap>
</sitemapindex>"#)
}

async fn sitemap_pages() -> Response {
    xml(r#"<?xml version="1.0" encoding="UTF-8"?>
<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">
  <url><loc>/</loc><lastmod>2026-02-01</lastmod><priority>1.0</priority></url>
  <url><loc>/about</loc><changefreq>monthly</changefreq></url>
  <url><loc>/orphan</loc></url>
</urlset>"#)
}

/// The shared body behind `/dup-a`, `/dup-b` and `/near-dup`.
///
/// Long enough to clear the near-duplicate audit's word-count floor: simhash
/// distance is unreliable on thin pages, so the audit ignores them, and a
/// forty-word fixture would silently test nothing.
fn duplicated_body() -> String {
    let paragraph = "<p>Returns are accepted within thirty days of delivery for a full         refund, provided the item is unused and still in its original packaging. Contact         support to start a return and we will email a prepaid label the same working day.         Refunds are processed within five working days of the parcel reaching our         warehouse, and appear on the original payment method.</p>";
    paragraph.repeat(5)
}

/// Two URLs with byte-identical bodies.
async fn duplicate() -> Response {
    html(&format!(
        "<html><head><title>Duplicate content</title></head><body>
           <h1>Returns policy</h1>{}</body></html>",
        duplicated_body()
    ))
}

/// The same text plus one sentence: not an exact duplicate, but well inside the
/// near-duplicate threshold.
async fn near_duplicate() -> Response {
    html(&format!(
        "<html><head><title>Duplicate content</title></head><body>
           <h1>Returns policy</h1>{}<p>Last updated in March.</p></body></html>",
        duplicated_body()
    ))
}

/// A body larger than the crawler's default parse cap.
async fn big_body() -> Response {
    let filler = "word ".repeat(400_000); // ~2 MB
    html(&format!(
        "<html><head><title>Big</title></head><body><p>{filler}</p></body></html>"
    ))
}

async fn image() -> Response {
    // A minimal valid PNG header plus padding, so Content-Length is realistic.
    let mut body = vec![0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
    body.extend(std::iter::repeat_n(0u8, 120_000));
    ([(header::CONTENT_TYPE, "image/png")], body).into_response()
}

async fn stylesheet() -> Response {
    ([(header::CONTENT_TYPE, "text/css")], "body { color: #222 }").into_response()
}

async fn pdf() -> Response {
    (
        [(header::CONTENT_TYPE, "application/pdf")],
        b"%PDF-1.4 fixture".to_vec(),
    )
        .into_response()
}

/// Self-references correctly, but declares `de`, which does not point back —
/// and carries an invalid region code.
async fn hreflang_en() -> Response {
    html(
        r#"<html><head><title>English</title>
             <link rel="alternate" hreflang="en" href="/hreflang/en">
             <link rel="alternate" hreflang="fr" href="/hreflang/fr">
             <link rel="alternate" hreflang="de" href="/hreflang/de">
             <link rel="alternate" hreflang="en-UK" href="/hreflang/en">
           </head><body><h1>English</h1></body></html>"#,
    )
}

/// Declares its sibling but not itself: a missing self-reference.
async fn hreflang_fr() -> Response {
    html(
        r#"<html><head><title>Français</title>
             <link rel="alternate" hreflang="en" href="/hreflang/en">
           </head><body><h1>Français</h1></body></html>"#,
    )
}

/// Named as an alternate by `/hreflang/en`, but declares nothing itself, so the
/// English page's annotation is never confirmed.
async fn hreflang_de() -> Response {
    html(
        r#"<html><head><title>Deutsch</title></head>
           <body><h1>Deutsch</h1><p>Eine Seite ohne hreflang-Angaben.</p></body></html>"#,
    )
}

/// Generated pages, for volume and performance work.
///
/// Page `n` links to `2n` and `2n+1`, so the site fans out as a binary tree.
/// A chain would serialise the crawl and measure latency instead of
/// throughput; a tree gives the frontier real parallel work from the second
/// page onwards, which is how a real site behaves.
async fn generated_page(Path(n): Path<usize>) -> Response {
    if n == 0 || n > GENERATED_PAGES {
        return (StatusCode::NOT_FOUND, html("<h1>No such page</h1>")).into_response();
    }
    let mut children = String::new();
    for child in [n * 2, n * 2 + 1] {
        if child <= GENERATED_PAGES {
            children.push_str(&format!(
                r#"<li><a href="/page/{child}">Page {child}</a></li>"#
            ));
        }
    }
    html(&format!(
        r#"<html lang="en"><head><title>Page {n}</title>
             <meta name="description" content="Generated fixture page number {n}.">
             <link rel="canonical" href="/page/{n}"></head>
           <body>
             <nav><a href="/">Home</a><a href="/about">About</a></nav>
             <main><h1>Page {n}</h1>
               <h2>Children</h2>
               <p>This is generated page {n} of the fixture site. It exists so the
                  crawler has a predictable amount of work to do, with a realistic
                  amount of markup and text on every page, including a heading,
                  a meta description, a canonical tag and an image.</p>
               <ul>{children}</ul>
             </main>
             <img src="/image.png" alt="Shared image">
             <footer><a href="/about">About</a></footer>
           </body></html>"#
    ))
}

/// 405s on GET but answers HEAD, exercising the method fallback in reverse.
async fn head_only() -> Response {
    (StatusCode::METHOD_NOT_ALLOWED, html("<h1>Use HEAD</h1>")).into_response()
}

// --- helpers --------------------------------------------------------------

fn html(body: &str) -> Response {
    (
        [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
        body.to_string(),
    )
        .into_response()
}

fn xml(body: &str) -> Response {
    (
        [(header::CONTENT_TYPE, "application/xml")],
        body.to_string(),
    )
        .into_response()
}

fn redirect(status: u16, location: &str) -> Response {
    Response::builder()
        .status(StatusCode::from_u16(status).unwrap())
        .header(header::LOCATION, location)
        .body(axum::body::Body::empty())
        .unwrap()
}
