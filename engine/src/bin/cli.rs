//! `crawlstat-cli` — the crawl engine without a UI.
//!
//! The CLI exists so the engine can be developed, tested and benchmarked on its
//! own, and it is the foundation of the planned "CrawlStat CI" tier (fail a
//! build when new SEO errors appear). It uses exactly the same library the
//! desktop app does, through the same progress stream.
//!
//! ```text
//! crawlstat-cli crawl https://example.com --max-urls 500 --out crawl.db
//! crawlstat-cli resume crawl.db
//! crawlstat-cli report crawl.db
//! crawlstat-cli list crawl.db --status 404
//! ```

use std::io::{IsTerminal, Write};
use std::path::PathBuf;
use std::time::Duration;

use clap::{Args, Parser, Subcommand};
use crawlstat_engine::audit::{AuditConfig, Severity};
use crawlstat_engine::config::{CrawlConfig, CrawlMode, Seed};
use crawlstat_engine::engine::Crawler;
use crawlstat_engine::progress::{CrawlSummary, ProgressEvent};
use crawlstat_engine::store::queries::{self, SortBy, UrlFilter};
use crawlstat_engine::store::Store;
use tokio::sync::mpsc;

#[derive(Parser, Debug)]
#[command(
    name = "crawlstat-cli",
    version,
    about = "CrawlStat's crawl engine: crawl a site into a local SQLite file.",
    long_about = None,
)]
struct Cli {
    #[command(subcommand)]
    command: Command,

    /// Print progress as one JSON object per line, for scripts and CI.
    #[arg(long, global = true)]
    json: bool,

    /// Print nothing but the final summary.
    #[arg(long, short, global = true)]
    quiet: bool,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Crawl a site, starting from a URL.
    Crawl {
        /// The URL to start from. The scheme may be omitted.
        url: String,
        #[command(flatten)]
        options: CrawlOptions,
    },
    /// Crawl every URL in an XML sitemap (or sitemap index).
    Sitemap {
        /// The sitemap URL. `.gz` is handled transparently.
        url: String,
        #[command(flatten)]
        options: CrawlOptions,
    },
    /// Fetch exactly the URLs in a file, one per line, following nothing.
    List {
        /// A file of URLs, or `-` to read standard input.
        file: PathBuf,
        #[command(flatten)]
        options: CrawlOptions,
    },
    /// Continue a crawl that was paused, cancelled or interrupted.
    Resume {
        /// The crawl database to continue.
        db: PathBuf,
    },
    /// Print a summary of a finished crawl.
    Report { db: PathBuf },
    /// Re-run the audits over an existing crawl.
    ///
    /// Useful after changing a threshold, and after upgrading: the checks and
    /// their explanations live in the binary, not in the crawl file.
    Audit {
        db: PathBuf,
        #[command(flatten)]
        thresholds: AuditThresholds,
    },
    /// List the issues found in a crawl, worst first.
    Issues {
        db: PathBuf,
        /// Only this severity: error, warning or notice.
        #[arg(long)]
        severity: Option<String>,
    },
    /// Show or change this machine's licence.
    ///
    /// The same entitlement the desktop app uses, so a CI runner on the Pro
    /// tier is activated once and then works offline.
    License {
        #[command(subcommand)]
        action: LicenseAction,
    },
    /// Show one issue: what it means, how to fix it, and which URLs have it.
    Issue {
        db: PathBuf,
        /// The issue id, as shown by `issues`.
        id: String,
        #[arg(long, default_value_t = 50)]
        limit: u32,
    },
    /// Print rows from a crawl, with optional filters.
    Urls {
        db: PathBuf,
        /// Only this status code.
        #[arg(long)]
        status: Option<u16>,
        /// Only this status class: 2, 3, 4 or 5.
        #[arg(long)]
        status_class: Option<u8>,
        /// Only URLs containing this text.
        #[arg(long)]
        contains: Option<String>,
        /// Only URLs affected by this issue id.
        #[arg(long)]
        issue: Option<String>,
        /// Internal URLs only.
        #[arg(long)]
        internal: bool,
        #[arg(long, default_value_t = 50)]
        limit: u32,
        #[arg(long, default_value_t = 0)]
        offset: u64,
    },
}

#[derive(Subcommand, Debug)]
enum LicenseAction {
    /// What this machine is entitled to.
    Status,
    /// Activate a licence key here.
    Activate {
        key: String,
        /// Point at a different licence API, for testing.
        #[arg(long)]
        api_base: Option<String>,
    },
    /// Re-check the licence now.
    Refresh {
        #[arg(long)]
        api_base: Option<String>,
    },
    /// Remove the licence from this machine.
    Deactivate,
}

/// Flags shared by every crawl mode.
#[derive(Args, Debug, Clone)]
struct CrawlOptions {
    /// Where to write the crawl database.
    #[arg(long, short, default_value = "crawl.db")]
    out: PathBuf,

    /// Stop after this many URLs. Omit for no limit.
    #[arg(long)]
    max_urls: Option<u64>,

    /// Maximum link distance from the start URL.
    #[arg(long)]
    max_depth: Option<u32>,

    /// Simultaneous requests across the crawl.
    #[arg(long, default_value_t = 5)]
    concurrency: usize,

    /// Simultaneous requests to any one host.
    #[arg(long)]
    concurrency_per_host: Option<usize>,

    /// Minimum milliseconds between two requests to the same host.
    #[arg(long, default_value_t = 0)]
    delay: u64,

    /// Crawl even where robots.txt disallows it.
    #[arg(long)]
    ignore_robots: bool,

    /// Override the user agent.
    #[arg(long)]
    user_agent: Option<String>,

    /// Also crawl subdomains of the start URL's host.
    #[arg(long)]
    subdomains: bool,

    /// Do not check external links.
    #[arg(long)]
    no_external: bool,

    /// Only crawl URLs matching this regex. Repeatable.
    #[arg(long = "include")]
    include: Vec<String>,

    /// Never crawl URLs matching this regex. Repeatable.
    #[arg(long = "exclude")]
    exclude: Vec<String>,

    /// Request timeout in milliseconds.
    #[arg(long, default_value_t = 20_000)]
    timeout: u64,

    /// Keep the raw HTML of every page in the database.
    #[arg(long)]
    store_html: bool,

    /// Overwrite the output file if it already exists.
    #[arg(long)]
    force: bool,

    /// Skip the audit pass that normally runs when the crawl finishes.
    #[arg(long)]
    no_audit: bool,

    #[command(flatten)]
    thresholds: AuditThresholds,
}

/// Audit thresholds, shared by `crawl` and `audit`.
#[derive(Args, Debug, Clone, Default)]
struct AuditThresholds {
    /// Titles longer than this are reported.
    #[arg(long)]
    title_max: Option<u32>,
    /// Titles shorter than this are reported.
    #[arg(long)]
    title_min: Option<u32>,
    /// Meta descriptions longer than this are reported.
    #[arg(long)]
    description_max: Option<u32>,
    /// Responses slower than this many milliseconds are reported.
    #[arg(long)]
    slow_ms: Option<u64>,
    /// Images larger than this many kilobytes are reported.
    #[arg(long)]
    large_image_kb: Option<u64>,
    /// Pages more than this many clicks from the start URL are reported.
    #[arg(long)]
    max_depth_notice: Option<u32>,
}

impl AuditThresholds {
    fn apply(&self, mut config: AuditConfig) -> AuditConfig {
        if let Some(value) = self.title_max {
            config.title_max_chars = value;
        }
        if let Some(value) = self.title_min {
            config.title_min_chars = value;
        }
        if let Some(value) = self.description_max {
            config.description_max_chars = value;
        }
        if let Some(value) = self.slow_ms {
            config.slow_response_ms = value;
        }
        if let Some(value) = self.large_image_kb {
            config.large_image_bytes = value * 1024;
        }
        if let Some(value) = self.max_depth_notice {
            config.max_depth_notice = value;
        }
        config
    }
}

impl CrawlOptions {
    fn apply(&self, mut config: CrawlConfig) -> CrawlConfig {
        config.max_urls = self.max_urls;
        config.max_depth = self.max_depth;
        config.concurrency = self.concurrency;
        config.concurrency_per_host = self.concurrency_per_host.unwrap_or(self.concurrency);
        config.crawl_delay_ms = self.delay;
        config.respect_robots = !self.ignore_robots;
        config.crawl_subdomains = self.subdomains;
        config.check_external_links = !self.no_external;
        config.include_patterns = self.include.clone();
        config.exclude_patterns = self.exclude.clone();
        config.request_timeout_ms = self.timeout;
        config.store_html = self.store_html;
        config.run_audits = !self.no_audit;
        config.audits = self.thresholds.apply(config.audits.clone());
        if let Some(user_agent) = &self.user_agent {
            config.user_agent = user_agent.clone();
            // Let the robots token be re-derived from the new agent.
            config.robots_token = String::new();
        }
        config
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // `RUST_LOG=crawlstat_engine=debug` turns on engine tracing.
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "warn".into()),
        )
        .with_writer(std::io::stderr)
        .init();

    let cli = Cli::parse();
    let reporter = Reporter {
        json: cli.json,
        quiet: cli.quiet,
    };

    match cli.command {
        Command::Crawl { url, options } => {
            let config = options.apply(CrawlConfig::spider(url));
            run_crawl(config, options, reporter).await
        }
        Command::Sitemap { url, options } => {
            let mut config = options.apply(CrawlConfig::default());
            config.seeds = vec![Seed::Sitemap { url }];
            run_crawl(config, options, reporter).await
        }
        Command::List { file, options } => {
            let urls = read_url_list(&file)?;
            if urls.is_empty() {
                anyhow::bail!("no URLs found in {}", file.display());
            }
            let mut config = options.apply(CrawlConfig::default());
            config.seeds = vec![Seed::List { urls }];
            config.mode = CrawlMode::List;
            run_crawl(config, options, reporter).await
        }
        Command::Resume { db } => {
            let (tx, rx) = mpsc::channel(4096);
            let printer = tokio::spawn(reporter.consume(rx));
            let (crawler, _handle) = Crawler::resume(&db, Some(tx)).await?;
            let summary = crawler.run().await?;
            let _ = printer.await;
            reporter.print_summary(&summary, &db);
            Ok(())
        }
        Command::License { action } => run_license(action, reporter),
        Command::Report { db } => {
            let store = Store::open(&db)?;
            print_report(&store, &db, reporter.json)
        }
        Command::Audit { db, thresholds } => {
            let mut store = Store::open(&db)?;
            let config =
                thresholds.apply(store.stored_config()?.map(|c| c.audits).unwrap_or_default());
            let report = store.audit(&config)?;
            if reporter.json {
                println!("{}", serde_json::to_string(&report)?);
            } else {
                print_issue_list(&report);
                println!(
                    "\n{} issue type(s), {} affected URL(s), in {} ms",
                    report.issues.len(),
                    report.total_findings(),
                    report.elapsed_ms
                );
            }
            Ok(())
        }
        Command::Issues { db, severity } => {
            let store = Store::open(&db)?;
            let wanted = match severity.as_deref() {
                None => None,
                Some("error") => Some(Severity::Error),
                Some("warning") => Some(Severity::Warning),
                Some("notice") => Some(Severity::Notice),
                Some(other) => {
                    anyhow::bail!("unknown severity `{other}` — use error, warning or notice")
                }
            };
            let report = store.audit_report(&audit_config(&store)?)?;
            if report.issues.is_empty() {
                println!("No issues found. Run `audit` if this crawl predates the audit pass.");
                return Ok(());
            }
            if reporter.json {
                println!("{}", serde_json::to_string(&report)?);
                return Ok(());
            }
            match wanted {
                Some(severity) => {
                    for issue in report.issues_of(severity) {
                        print_issue_line(issue);
                    }
                }
                None => print_issue_list(&report),
            }
            Ok(())
        }
        Command::Issue { db, id, limit } => {
            let store = Store::open(&db)?;
            let report = store.audit_report(&audit_config(&store)?)?;
            let Some(issue) = report.issue(&id) else {
                anyhow::bail!(
                    "no issue `{id}` in this crawl — run `issues {}` to see what was found",
                    db.display()
                );
            };
            if reporter.json {
                println!("{}", serde_json::to_string(issue)?);
                return Ok(());
            }
            println!("{}  [{}]", issue.title, issue.severity.as_str());
            println!("\n{}\n", wrap(&issue.description, 78));
            println!("How to fix\n{}\n", wrap(&issue.how_to_fix, 78));
            println!(
                "Docs: https://crawlstat.com/docs/audits/{}",
                issue.docs_slug
            );
            println!("\nAffected URLs ({} total)", issue.url_count);
            for (_, url, detail) in queries::issue_details(store.connection(), &id, limit)? {
                match detail {
                    Some(detail) => println!("  {url}  —  {detail}"),
                    None => println!("  {url}"),
                }
            }
            Ok(())
        }
        Command::Urls {
            db,
            status,
            status_class,
            contains,
            issue,
            internal,
            limit,
            offset,
        } => {
            let store = Store::open(&db)?;
            let filter = UrlFilter {
                status_code: status,
                status_class,
                url_contains: contains,
                issue_id: issue,
                internal: internal.then_some(true),
                ..Default::default()
            };
            let page = queries::list_urls(
                store.connection(),
                &filter,
                SortBy::Url,
                false,
                limit,
                offset,
            )?;
            println!("{} of {} matching URL(s)", page.rows.len(), page.total);
            for row in page.rows {
                println!(
                    "{:>5}  {:>4}  {:>5}w  {}",
                    row.status_code
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| "-".into()),
                    row.depth,
                    row.word_count.unwrap_or(0),
                    row.url,
                );
            }
            Ok(())
        }
    }
}

fn run_license(action: LicenseAction, reporter: Reporter) -> anyhow::Result<()> {
    use crawlstat_engine::license::{self, client};

    let status = match action {
        LicenseAction::Status => license::load_current(),
        LicenseAction::Activate { key, api_base } => client::activate(
            api_base.as_deref().unwrap_or(client::DEFAULT_API_BASE),
            &key,
        )?,
        LicenseAction::Refresh { api_base } => {
            let key = client::stored_license_key()
                .ok_or_else(|| anyhow::anyhow!("no licence is activated on this machine"))?;
            client::refresh_now(
                api_base.as_deref().unwrap_or(client::DEFAULT_API_BASE),
                &key,
            )?
        }
        LicenseAction::Deactivate => {
            client::deactivate()?;
            license::LicenseStatus::Free
        }
    };

    if reporter.json {
        println!("{}", serde_json::to_string(&status)?);
        return Ok(());
    }

    let entitlement = status.entitlement();
    match &status {
        license::LicenseStatus::Free => {
            println!("Free tier");
            println!("  up to {} URLs per crawl", license::FREE_TIER_MAX_URLS);
            println!("  activate with: crawlstat-cli license activate <KEY>");
        }
        license::LicenseStatus::Active { days_remaining, .. } => {
            println!("Pro — active");
            println!("  key ending   {}", entitlement.key_fingerprint);
            println!("  valid for    {days_remaining} more day(s) without a check");
            println!("  no URL limit");
        }
        license::LicenseStatus::Grace { days_remaining, .. } => {
            println!("Pro — working offline");
            println!("  key ending   {}", entitlement.key_fingerprint);
            println!("  {days_remaining} day(s) left before it needs re-checking");
        }
        license::LicenseStatus::Expired { .. } => {
            println!("Licence lapsed after a fortnight without a check.");
            println!("  run: crawlstat-cli license refresh");
        }
        license::LicenseStatus::Invalid { reason } => {
            println!("The stored licence could not be read: {reason}");
            println!("  re-activate with: crawlstat-cli license activate <KEY>");
        }
    }
    Ok(())
}

async fn run_crawl(
    config: CrawlConfig,
    options: CrawlOptions,
    reporter: Reporter,
) -> anyhow::Result<()> {
    if options.out.exists() {
        if options.force {
            std::fs::remove_file(&options.out)?;
            // WAL sidecars would otherwise be re-attached to the new file.
            for suffix in ["-wal", "-shm"] {
                let mut sidecar = options.out.clone().into_os_string();
                sidecar.push(suffix);
                let _ = std::fs::remove_file(PathBuf::from(sidecar));
            }
        } else {
            anyhow::bail!(
                "{} already exists — pass --force to overwrite, or `resume` to continue it",
                options.out.display()
            );
        }
    }

    let (tx, rx) = mpsc::channel(4096);
    let printer = tokio::spawn(reporter.consume(rx));

    let (crawler, handle) = Crawler::start(config, &options.out, Some(tx)).await?;

    // Ctrl-C cancels politely: in-flight requests finish and are written, and
    // the crawl stays resumable.
    let ctrl_c_handle = handle.clone();
    tokio::spawn(async move {
        if tokio::signal::ctrl_c().await.is_ok() {
            eprintln!("\nstopping — finishing in-flight requests (resume with `resume`)");
            ctrl_c_handle.cancel();
        }
    });

    let summary = crawler.run().await?;
    let _ = printer.await;
    reporter.print_summary(&summary, &options.out);
    Ok(())
}

fn read_url_list(path: &PathBuf) -> anyhow::Result<Vec<String>> {
    let raw = if path.as_os_str() == "-" {
        std::io::read_to_string(std::io::stdin())?
    } else {
        std::fs::read_to_string(path)?
    };
    Ok(raw
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(str::to_string)
        .collect())
}

/// Turns progress events into terminal output.
#[derive(Clone, Copy)]
struct Reporter {
    json: bool,
    quiet: bool,
}

impl Reporter {
    async fn consume(self, mut rx: mpsc::Receiver<ProgressEvent>) {
        let interactive = std::io::stderr().is_terminal() && !self.json && !self.quiet;

        while let Some(event) = rx.recv().await {
            if self.json {
                if let Ok(line) = serde_json::to_string(&event) {
                    println!("{line}");
                }
                continue;
            }
            if self.quiet {
                continue;
            }

            match event {
                ProgressEvent::Started { seeded, resumed } => {
                    let verb = if resumed { "resuming with" } else { "seeded" };
                    eprintln!("{verb} {seeded} URL(s)");
                }
                ProgressEvent::Progress(snapshot) if interactive => {
                    // A single rewritten line, so a long crawl does not scroll.
                    eprint!(
                        "\r\x1b[2K{:>6} crawled  {:>6} queued  {:>4} in flight  {:>6.1}/s  {}",
                        snapshot.crawled,
                        snapshot.queued,
                        snapshot.in_flight,
                        snapshot.urls_per_second,
                        format_duration(Duration::from_millis(snapshot.elapsed_ms)),
                    );
                    let _ = std::io::stderr().flush();
                }
                ProgressEvent::Warning { message, url } => {
                    if interactive {
                        eprint!("\r\x1b[2K");
                    }
                    match url {
                        Some(url) => eprintln!("warning: {message} ({url})"),
                        None => eprintln!("warning: {message}"),
                    }
                }
                _ => {}
            }
        }
        if interactive {
            eprintln!();
        }
    }

    fn print_summary(&self, summary: &CrawlSummary, db: &std::path::Path) {
        if self.json {
            if let Ok(line) = serde_json::to_string(summary) {
                println!("{line}");
            }
            return;
        }
        println!("\n{} — {}", db.display(), summary.state.as_str());
        println!("  crawled      {}", summary.crawled);
        println!("  discovered   {}", summary.discovered);
        println!(
            "  statuses     2xx {}  3xx {}  4xx {}  5xx {}",
            summary.status_2xx, summary.status_3xx, summary.status_4xx, summary.status_5xx
        );
        println!("  fetch errors {}", summary.errors);
        println!(
            "  took         {} ({:.1} URLs/sec)",
            format_duration(Duration::from_millis(summary.elapsed_ms)),
            summary.urls_per_second
        );
        if summary.hit_limit {
            println!("  note         stopped at the configured URL limit");
        }
        if let Some(audit) = summary.audit {
            println!(
                "  issues       {} error(s), {} warning(s), {} notice(s) across {} URL(s)",
                audit.errors, audit.warnings, audit.notices, audit.findings
            );
        }
    }
}

/// The audit thresholds a crawl was run with, falling back to the defaults.
fn audit_config(store: &Store) -> anyhow::Result<AuditConfig> {
    Ok(store
        .stored_config()?
        .map(|config| config.audits)
        .unwrap_or_default())
}

fn print_issue_list(report: &crawlstat_engine::audit::AuditReport) {
    for severity in [Severity::Error, Severity::Warning, Severity::Notice] {
        let issues: Vec<_> = report.issues_of(severity).collect();
        if issues.is_empty() {
            continue;
        }
        println!("\n{}", severity.as_str().to_uppercase());
        for issue in issues {
            print_issue_line(issue);
        }
    }
}

fn print_issue_line(issue: &crawlstat_engine::audit::IssueSummary) {
    println!(
        "  {:>6}  {:<28}  {}",
        issue.url_count, issue.id, issue.title
    );
}

/// Wrap text to a width, for the terminal. Naive but adequate: the audit text
/// has no long unbreakable tokens.
fn wrap(text: &str, width: usize) -> String {
    let mut out = String::with_capacity(text.len() + text.len() / width);
    let mut line = 0;
    for word in text.split_whitespace() {
        if line > 0 && line + 1 + word.len() > width {
            out.push('\n');
            line = 0;
        } else if line > 0 {
            out.push(' ');
            line += 1;
        }
        out.push_str(word);
        line += word.len();
    }
    out
}

fn print_report(store: &Store, db: &std::path::Path, json: bool) -> anyhow::Result<()> {
    let stats = store.stats()?;
    if json {
        let report = store.audit_report(&audit_config(store)?)?;
        println!("{}", serde_json::to_string(&report)?);
        return Ok(());
    }
    println!(
        "{} — {}",
        db.display(),
        store.meta("state")?.unwrap_or_else(|| "unknown".into())
    );
    println!("\nURLs");
    println!("  total        {}", stats.total);
    println!("  crawled      {}", stats.done);
    println!("  failed       {}", stats.failed);
    println!("  queued       {}", stats.queued);
    println!("  skipped      {}", stats.skipped);
    println!("  robots       {}", stats.blocked_by_robots);

    println!("\nStatus codes");
    for (code, count) in queries::status_code_summary(store.connection())? {
        println!("  {code:>3}          {count}");
    }

    println!("\nCrawl depth");
    for (depth, count) in queries::depth_distribution(store.connection())? {
        println!("  {depth:>3}          {count}");
    }

    let structured = queries::structured_data_summary(store.connection())?;
    if !structured.is_empty() {
        println!("\nStructured data");
        for (types, count) in structured.iter().take(10) {
            println!("  {count:>5}        {types}");
        }
    }

    let report = store.audit_report(&audit_config(store)?)?;
    if report.issues.is_empty() {
        println!(
            "\nNo issues recorded. Run `audit {}` to check.",
            db.display()
        );
    } else {
        println!(
            "\nIssues — {} error(s), {} warning(s), {} notice(s)",
            report.errors, report.warnings, report.notices
        );
        print_issue_list(&report);
        println!("\n  `issue <db> <id>` explains any of these and lists the URLs.");
    }

    let log = store.log_entries()?;
    if !log.is_empty() {
        println!("\nNotices");
        for (level, message, url) in log.iter().take(20) {
            match url {
                Some(url) => println!("  {level}: {message} ({url})"),
                None => println!("  {level}: {message}"),
            }
        }
    }
    Ok(())
}

fn format_duration(duration: Duration) -> String {
    let secs = duration.as_secs();
    if secs < 60 {
        format!("{}.{:01}s", secs, duration.subsec_millis() / 100)
    } else if secs < 3600 {
        format!("{}m{:02}s", secs / 60, secs % 60)
    } else {
        format!("{}h{:02}m", secs / 3600, (secs % 3600) / 60)
    }
}
