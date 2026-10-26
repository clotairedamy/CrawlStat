//! Headless Chrome, for the two jobs that need a real browser: running a
//! page's JavaScript before it is parsed, and printing the client report to
//! PDF.
//!
//! Chrome is driven by its command-line switches (`--dump-dom`,
//! `--print-to-pdf`), one short-lived process per job, rather than over the
//! DevTools protocol. That costs a second or two of start-up per page, but it
//! needs no new dependency, a hung page cannot wedge a shared session, and a
//! crash loses one page instead of the crawl.
//!
//! Nothing is bundled: CrawlStat uses the Chrome, Chromium, Edge or Brave the
//! user already has, and says so plainly when there is none.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use futures::future::BoxFuture;
use tokio::sync::Semaphore;

use crate::error::FetchError;
use crate::fetch::{FetchRequest, FetchedResponse, Fetcher, Method};

/// Overrides the search below, for a browser installed somewhere unusual.
pub const CHROME_ENV: &str = "CRAWLSTAT_CHROME";

/// How long a page's scripts may run, in the page's own (virtual) time.
/// Timers are fast-forwarded, so this is rarely waited out in full.
pub const DEFAULT_SCRIPT_BUDGET: Duration = Duration::from_millis(4_000);

/// Pages rendered at once. Each is a whole browser process.
pub const DEFAULT_RENDER_SLOTS: usize = 4;

/// Where Chromium-family browsers live, most likely first.
fn candidates() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if cfg!(target_os = "macos") {
        for app in [
            "Google Chrome.app/Contents/MacOS/Google Chrome",
            "Chromium.app/Contents/MacOS/Chromium",
            "Microsoft Edge.app/Contents/MacOS/Microsoft Edge",
            "Brave Browser.app/Contents/MacOS/Brave Browser",
        ] {
            paths.push(Path::new("/Applications").join(app));
            if let Some(home) = std::env::var_os("HOME") {
                paths.push(Path::new(&home).join("Applications").join(app));
            }
        }
    } else if cfg!(target_os = "windows") {
        for root in ["PROGRAMFILES", "PROGRAMFILES(X86)", "LOCALAPPDATA"] {
            let Some(root) = std::env::var_os(root) else {
                continue;
            };
            for exe in [
                r"Google\Chrome\Application\chrome.exe",
                r"Chromium\Application\chrome.exe",
                r"Microsoft\Edge\Application\msedge.exe",
                r"BraveSoftware\Brave-Browser\Application\brave.exe",
            ] {
                paths.push(Path::new(&root).join(exe));
            }
        }
    } else if let Some(path) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&path) {
            for name in [
                "google-chrome",
                "google-chrome-stable",
                "chromium",
                "chromium-browser",
                "microsoft-edge",
                "brave-browser",
            ] {
                paths.push(dir.join(name));
            }
        }
    }
    paths
}

/// A Chromium-family browser on this machine.
#[derive(Debug, Clone)]
pub struct Chrome {
    path: PathBuf,
}

impl Chrome {
    /// Find a browser: `CRAWLSTAT_CHROME` first, then the usual install
    /// locations.
    pub fn locate() -> Option<Self> {
        let configured = std::env::var_os(CHROME_ENV).map(PathBuf::from);
        configured
            .into_iter()
            .chain(candidates())
            .find(|path| path.is_file())
            .map(|path| Self { path })
    }

    pub fn at(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Switches every job shares. Each job gets its own profile directory, so
    /// several can run at once without one handing its work to another.
    fn base_args(profile: &Path) -> Vec<String> {
        let mut args: Vec<String> = [
            "--headless=new",
            "--disable-gpu",
            "--no-first-run",
            "--no-default-browser-check",
            "--disable-extensions",
            "--disable-sync",
            "--disable-background-networking",
            "--disable-component-update",
            "--mute-audio",
            "--hide-scrollbars",
        ]
        .map(String::from)
        .to_vec();
        args.push(format!("--user-data-dir={}", profile.display()));
        // Inside the macOS App Sandbox Chrome cannot start its own sandbox,
        // and refuses to run at all unless told not to try. It still runs
        // inside the app's.
        if sandboxed() {
            args.push("--no-sandbox".into());
        }
        args
    }

    /// The page's DOM after its scripts have run, serialised as HTML.
    pub async fn dump_dom(
        &self,
        url: &str,
        user_agent: &str,
        budget: Duration,
        timeout: Duration,
    ) -> Result<String, String> {
        let profile = Profile::new()?;
        let mut command = tokio::process::Command::new(&self.path);
        command
            .args(Self::base_args(&profile.0))
            .arg("--blink-settings=imagesEnabled=false")
            .arg("--window-size=1366,900")
            .arg(format!("--virtual-time-budget={}", budget.as_millis()))
            .arg(format!("--user-agent={user_agent}"))
            .arg("--dump-dom")
            .arg(url)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true);

        let child = command
            .spawn()
            .map_err(|error| format!("could not start {}: {error}", self.path.display()))?;
        let output = tokio::time::timeout(timeout, child.wait_with_output())
            .await
            .map_err(|_| format!("rendering took longer than {}s", timeout.as_secs()))?
            .map_err(|error| error.to_string())?;
        if !output.status.success() {
            return Err(format!("the browser exited with {}", output.status));
        }
        let dom = String::from_utf8_lossy(&output.stdout).into_owned();
        if dom.trim().is_empty() {
            return Err("the browser returned an empty page".into());
        }
        // `--dump-dom` serialises the element tree and drops the doctype, which
        // the page did have: put it back so the audits do not invent a quirks
        // mode problem.
        Ok(format!("<!DOCTYPE html>\n{dom}"))
    }

    /// Print a local HTML file to PDF. Blocking: it is called once, off the
    /// UI thread, when a report is exported.
    ///
    /// Chrome writes into its own profile directory, and this process copies
    /// the result to `pdf`: inside the App Sandbox the child browser does not
    /// share the app's right to write to Downloads or a folder the user chose.
    pub fn print_to_pdf(&self, html: &Path, pdf: &Path, timeout: Duration) -> Result<(), String> {
        let profile = Profile::new()?;
        let source = url::Url::from_file_path(html)
            .map_err(|_| format!("{} is not an absolute path", html.display()))?;
        let printed = profile.0.join("print.pdf");
        let log_path = profile.0.join("chrome.log");
        let log = std::fs::File::create(&log_path).map_err(|error| error.to_string())?;
        let mut child = std::process::Command::new(&self.path)
            .args(Self::base_args(&profile.0.join("profile")))
            // The new switch, then the one older versions understood.
            .arg("--no-pdf-header-footer")
            .arg("--print-to-pdf-no-header")
            .arg("--virtual-time-budget=2000")
            .arg(format!("--print-to-pdf={}", printed.display()))
            .arg(source.as_str())
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(log)
            .spawn()
            .map_err(|error| format!("could not start {}: {error}", self.path.display()))?;

        let started = std::time::Instant::now();
        let status = loop {
            match child.try_wait().map_err(|error| error.to_string())? {
                Some(status) => break status,
                None if started.elapsed() > timeout => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(format!("printing took longer than {}s", timeout.as_secs()));
                }
                None => std::thread::sleep(Duration::from_millis(50)),
            }
        };
        // Chrome exits 0 on some failures; the file is the only real answer.
        match std::fs::metadata(&printed) {
            Ok(meta) if meta.len() > 0 => {}
            _ => {
                let log = std::fs::read_to_string(&log_path).unwrap_or_default();
                let tail: Vec<&str> = log.lines().filter(|line| !line.trim().is_empty()).collect();
                let tail = tail[tail.len().saturating_sub(3)..].join(" / ");
                return Err(if tail.is_empty() {
                    format!("the browser did not write a PDF ({status})")
                } else {
                    format!("the browser did not write a PDF ({status}): {tail}")
                });
            }
        }
        std::fs::copy(&printed, pdf)
            .map(|_| ())
            .map_err(|error| format!("could not save {}: {error}", pdf.display()))
    }
}

/// Whether this process runs inside the macOS App Sandbox. The container id
/// is the documented signal; a home directory inside `~/Library/Containers`
/// is the fallback when the variable is not passed on.
fn sandboxed() -> bool {
    std::env::var_os("APP_SANDBOX_CONTAINER_ID").is_some()
        || std::env::var("HOME").is_ok_and(|home| home.contains("/Library/Containers/"))
}

/// A throwaway profile directory, removed when the job ends.
struct Profile(PathBuf);

impl Profile {
    fn new() -> Result<Self, String> {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "crawlstat-chrome-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&path).map_err(|error| error.to_string())?;
        Ok(Self(path))
    }
}

impl Drop for Profile {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Turns a URL into the HTML a browser ends up with. [`Chrome`] in
/// production; a stub in tests.
pub trait Renderer: Send + Sync + 'static {
    fn render<'a>(&'a self, url: &'a str) -> BoxFuture<'a, Result<String, String>>;
}

/// [`Chrome`] with the crawl's user agent and limits.
#[derive(Debug, Clone)]
pub struct ChromeRenderer {
    pub chrome: Chrome,
    pub user_agent: String,
    pub budget: Duration,
    pub timeout: Duration,
}

impl Renderer for ChromeRenderer {
    fn render<'a>(&'a self, url: &'a str) -> BoxFuture<'a, Result<String, String>> {
        Box::pin(
            self.chrome
                .dump_dom(url, &self.user_agent, self.budget, self.timeout),
        )
    }
}

/// A [`Fetcher`] that hands successful HTML pages to a browser and keeps
/// what the browser built.
///
/// The plain HTTP fetch still happens first and still decides the status,
/// headers, redirects and timings: those are what a search engine's first
/// pass sees, and a browser does not report them faithfully. Only the body is
/// swapped, and only when rendering worked — a page Chrome chokes on is
/// audited as served rather than dropped.
pub struct RenderingFetcher<F, R> {
    inner: F,
    renderer: R,
    slots: Arc<Semaphore>,
}

impl<F: Fetcher, R: Renderer> RenderingFetcher<F, R> {
    pub fn new(inner: F, renderer: R, slots: usize) -> Self {
        Self {
            inner,
            renderer,
            slots: Arc::new(Semaphore::new(slots.max(1))),
        }
    }
}

fn is_html(response: &FetchedResponse) -> bool {
    response.content_type().is_some_and(|value| {
        let value = value.to_ascii_lowercase();
        value.contains("text/html") || value.contains("application/xhtml")
    })
}

impl<F: Fetcher, R: Renderer> Fetcher for RenderingFetcher<F, R> {
    fn fetch<'a>(
        &'a self,
        request: FetchRequest,
    ) -> BoxFuture<'a, Result<FetchedResponse, FetchError>> {
        Box::pin(async move {
            let wants_body = request.method == Method::Get && request.max_body_bytes > 0;
            let mut response = self.inner.fetch(request).await?;
            if !wants_body || !response.is_success() || response.truncated || !is_html(&response) {
                return Ok(response);
            }
            let Ok(_slot) = self.slots.acquire().await else {
                return Ok(response);
            };
            match self.renderer.render(response.final_url.as_str()).await {
                Ok(dom) => response.body = dom.into_bytes(),
                Err(error) => {
                    tracing::warn!(url = %response.final_url, %error, "rendering failed; keeping the served HTML");
                }
            }
            Ok(response)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fetch::mock::{MockFetcher, MockResponse};
    use url::Url;

    struct Stub;

    impl Renderer for Stub {
        fn render<'a>(&'a self, url: &'a str) -> BoxFuture<'a, Result<String, String>> {
            Box::pin(async move {
                if url.contains("broken") {
                    Err("crashed".into())
                } else {
                    Ok(format!(
                        "<html><body><a href=\"/from-js\">rendered {url}</a></body></html>"
                    ))
                }
            })
        }
    }

    fn fetcher() -> RenderingFetcher<MockFetcher, Stub> {
        let mock = MockFetcher::default();
        mock.insert(
            "https://site.test/",
            MockResponse::html("<div id=app></div>"),
        );
        mock.insert(
            "https://site.test/broken",
            MockResponse::html("<p>served</p>"),
        );
        mock.insert("https://site.test/missing", MockResponse::status(404));
        mock.insert(
            "https://site.test/data.json",
            MockResponse {
                content_type: "application/json".into(),
                ..MockResponse::html("{}")
            },
        );
        RenderingFetcher::new(mock, Stub, 2)
    }

    async fn body(
        fetcher: &RenderingFetcher<MockFetcher, Stub>,
        url: &str,
        method: Method,
    ) -> String {
        let mut request = FetchRequest::get(Url::parse(url).unwrap());
        request.method = method;
        if method == Method::Head {
            request.max_body_bytes = 0;
        }
        let response = fetcher.fetch(request).await.unwrap();
        String::from_utf8(response.body).unwrap()
    }

    #[tokio::test]
    async fn successful_html_is_replaced_by_the_rendered_dom() {
        let fetcher = fetcher();
        let html = body(&fetcher, "https://site.test/", Method::Get).await;
        assert!(html.contains("/from-js"), "{html}");
    }

    #[tokio::test]
    async fn everything_else_is_left_as_served() {
        let fetcher = fetcher();
        assert_eq!(
            body(&fetcher, "https://site.test/data.json", Method::Get).await,
            "{}"
        );
        assert_eq!(
            body(&fetcher, "https://site.test/missing", Method::Get).await,
            ""
        );
        assert_eq!(
            body(&fetcher, "https://site.test/", Method::Head).await,
            "<div id=app></div>"
        );
    }

    #[tokio::test]
    async fn a_failed_render_keeps_the_served_html() {
        let fetcher = fetcher();
        assert_eq!(
            body(&fetcher, "https://site.test/broken", Method::Get).await,
            "<p>served</p>"
        );
    }

    /// Runs only where a browser is installed; proves the switches still do
    /// what this module assumes.
    #[tokio::test]
    async fn real_chrome_runs_scripts() {
        let Some(chrome) = Chrome::locate() else {
            return;
        };
        let dir = tempfile::tempdir().unwrap();
        let page = dir.path().join("page.html");
        std::fs::write(
            &page,
            "<html><body><div id=a></div><script>\
             document.getElementById('a').innerHTML='<a href=\"/later\">late</a>'\
             </script></body></html>",
        )
        .unwrap();
        let url = Url::from_file_path(&page).unwrap();
        let dom = chrome
            .dump_dom(
                url.as_str(),
                "CrawlStat test",
                Duration::from_secs(2),
                Duration::from_secs(60),
            )
            .await
            .unwrap();
        assert!(dom.starts_with("<!DOCTYPE html>"));
        assert!(dom.contains("href=\"/later\""), "{dom}");

        let pdf = dir.path().join("page.pdf");
        chrome
            .print_to_pdf(&page, &pdf, Duration::from_secs(60))
            .unwrap();
        assert!(std::fs::read(&pdf).unwrap().starts_with(b"%PDF"));
    }
}
