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
use crawlstat_engine::plan::Effort;
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
    /// Rank a crawl's findings into a list of jobs, best first.
    ///
    /// The same ranking the desktop app's Plan screen shows: severity, the
    /// share of the site's link value affected, and how many pages one change
    /// fixes. Exits non-zero when anything scores above `--fail-above`, which
    /// is how CrawlStat CI will gate a deploy.
    Plan {
        db: PathBuf,
        #[arg(long, default_value_t = 10)]
        limit: usize,
        /// Exit 1 if any job scores higher than this. 0-100.
        #[arg(long)]
        fail_above: Option<f64>,
    },
    /// Compare two crawls of the same site: what was fixed, what broke.
    ///
    /// Exits non-zero when anything regressed, so a release can be gated on
    /// "this change introduced no new problems".
    Compare {
        /// The earlier crawl.
        before: PathBuf,
        /// The later crawl.
        after: PathBuf,
        /// Exit 1 if any issue got worse or any page started failing.
        #[arg(long)]
        fail_on_regression: bool,
    },
    /// Predict how a page leads a reader, and where it loses them.
    ///
    /// Reads the markup only — there is no browser here, so this knows reading
    /// order, landmarks and labels, and nothing about what the CSS did. See
    /// the module docs in `src/ux/mod.rs` for what that does and does not buy.
    Ux {
        /// A URL to fetch, or a path to an HTML file.
        target: String,
        /// Which reader: task_oriented, explorer, skeptical, mobile_rushed.
        #[arg(long, default_value = "task_oriented")]
        persona: String,
        /// How many nodes of the attention ranking to print.
        #[arg(long, default_value_t = 10)]
        limit: usize,
    },
    /// Simulate a visitor hunting for something, and see if they find it.
    Journey {
        db: PathBuf,
        /// Which reader: task_oriented, explorer, skeptical, mobile_rushed.
        #[arg(long, default_value = "task_oriented")]
        persona: String,
        /// One goal id, or every preset when omitted.
        #[arg(long)]
        goal: Option<String>,
    },
    /// Information architecture findings: labels, orphans, depth, crowding.
    Ia { db: PathBuf },
    /// Compare how findable things are on your site versus someone else's.
    Benchmark {
        /// Your crawl.
        ours: PathBuf,
        /// Their crawl.
        theirs: PathBuf,
        #[arg(long, default_value = "task_oriented")]
        persona: String,
        /// One task in plain language instead of the presets.
        #[arg(long)]
        task: Option<String>,
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
        Command::Plan {
            db,
            limit,
            fail_above,
        } => {
            let store = Store::open(&db)?;
            let plan = crawlstat_engine::plan::build(store.connection())?;

            if plan.items.is_empty() {
                println!("No issues to rank. Run `audit` first if you have not.");
                return Ok(());
            }

            println!(
                "{} job(s) · {} of {} pages affected · {:.0}% of link value at risk\n",
                plan.items.len(),
                plan.affected_pages,
                plan.total_pages,
                plan.value_at_risk * 100.0
            );

            for (index, item) in plan.items.iter().take(limit).enumerate() {
                let effort = match item.effort {
                    Effort::Template => "one template change",
                    Effort::Bulk => "a batch of changes",
                    Effort::PerPage => "one change per page",
                };
                println!(
                    "{:>3}.  [{:>3.0}]  {}  ({})",
                    index + 1,
                    item.impact,
                    item.title,
                    item.severity.as_str()
                );
                println!(
                    "            {} page(s), {} distinct fix(es) — {}, {:.1}% of link value",
                    item.pages,
                    item.distinct_fixes,
                    effort,
                    item.reach * 100.0
                );
                if let Some(example) = item.examples.first() {
                    println!("            e.g. {}", example.url);
                }
                println!();
            }

            if let Some(threshold) = fail_above {
                let over: Vec<&str> = plan
                    .items
                    .iter()
                    .filter(|item| item.impact > threshold)
                    .map(|item| item.title.as_str())
                    .collect();
                if !over.is_empty() {
                    eprintln!(
                        "{} job(s) score above {threshold}: {}",
                        over.len(),
                        over.join(", ")
                    );
                    std::process::exit(1);
                }
            }
            Ok(())
        }
        Command::Ux {
            target,
            persona,
            limit,
        } => {
            let persona = crawlstat_engine::ux::Persona::parse(&persona).ok_or_else(|| {
                anyhow::anyhow!(
                    "unknown persona `{persona}` — use task_oriented, explorer, \
                     skeptical or mobile_rushed"
                )
            })?;

            // The async client, not the blocking one: `main` already runs on a
            // tokio runtime and dropping a blocking client inside one panics.
            // The crypto provider is the crawler's own — `ring`, installed
            // process-wide — because rustls refuses to build a client without
            // one and the default would be a different library.
            let body = if target.starts_with("http://") || target.starts_with("https://") {
                crawlstat_engine::fetch::install_crypto_provider();
                let client = reqwest::Client::builder()
                    .user_agent(crawlstat_engine::config::DEFAULT_USER_AGENT)
                    .build()?;
                client.get(&target).send().await?.text().await?
            } else {
                std::fs::read_to_string(&target)?
            };

            let document = scraper::Html::parse_document(&body);
            let nodes = crawlstat_engine::ux::signals::extract(&document);
            let flow = crawlstat_engine::ux::flow_for(&nodes, persona);

            println!("{}  —  {}", persona.label(), persona.describes());
            println!("{} element(s) a reader could attend to\n", nodes.len());

            println!("Where the attention goes:");
            for attention in flow.attention.iter().take(limit) {
                let bar = "#".repeat(((attention.share * 120.0).round() as usize).min(40));
                println!(
                    "  {:>5.1}%  {:<40}  {:<9} {}",
                    attention.share * 100.0,
                    truncate(&attention.label, 40),
                    format!("{:?}", attention.region).to_lowercase(),
                    bar
                );
            }

            println!("\nFlow:");
            println!(
                "  reaches {} of {} elements before deciding",
                flow.path.len(),
                nodes.len()
            );
            match flow.steps_to_action {
                Some(at) if flow.action_out_of_reach => println!(
                    "  !! the primary action is element {at}, past where this reader stops"
                ),
                Some(at) => println!("  primary action at element {at}"),
                None => println!("  no primary action on this page"),
            }
            println!(
                "  coherence {:.0}% ({})",
                flow.coherence * 100.0,
                if flow.coherence > 0.35 {
                    "attention lands somewhere"
                } else {
                    "attention is spread thin — the page does not lead anywhere"
                }
            );
            if flow.dead_end {
                println!("  !! dead end: no onward link in the content");
            }

            for conflict in &flow.cta_conflicts {
                println!(
                    "\n  !! {} actions competing ({:.0}% of attention between them):",
                    conflict.labels.len(),
                    conflict.share * 100.0
                );
                for label in &conflict.labels {
                    println!("       {}", truncate(label, 60));
                }
            }

            if !flow.decision_nodes.is_empty() {
                println!("\n  {} decision point(s):", flow.decision_nodes.len());
                for decision in &flow.decision_nodes {
                    println!(
                        "       element {}: {} options at once",
                        decision.order, decision.options
                    );
                }
            }

            if !flow.vague_links.is_empty() {
                println!("\n  {} link(s) that say nothing:", flow.vague_links.len());
                for label in flow.vague_links.iter().take(8) {
                    println!("       \"{label}\"");
                }
            }

            Ok(())
        }
        Command::Compare {
            before,
            after,
            fail_on_regression,
        } => {
            let old = Store::open(&before)?;
            let new = Store::open(&after)?;
            let result = crawlstat_engine::compare::compare(old.connection(), new.connection())?;

            println!(
                "{} pages -> {} pages   ({} with problems -> {})",
                result.pages_before,
                result.pages_after,
                result.affected_before,
                result.affected_after
            );
            println!(
                "link value at risk: {:.0}% -> {:.0}%\n",
                result.value_at_risk_before * 100.0,
                result.value_at_risk_after * 100.0
            );

            let improvements = result.improvements();
            if improvements.is_empty() {
                println!("Nothing improved.");
            } else {
                println!("Fixed:");
                for issue in &improvements {
                    println!(
                        "  {:>5}  {}   ({} -> {} pages, impact {:.0} -> {:.0})",
                        format!("-{}", issue.fixed),
                        issue.title,
                        issue.before,
                        issue.after,
                        issue.impact_before,
                        issue.impact_after
                    );
                }
            }

            let regressions = result.regressions();
            if !regressions.is_empty() {
                println!("\nGot worse:");
                for issue in &regressions {
                    println!(
                        "  {:>5}  {}   ({} -> {} pages)",
                        format!("+{}", issue.introduced),
                        issue.title,
                        issue.before,
                        issue.after
                    );
                }
            }

            if !result.pages_broken.is_empty() {
                println!("\n{} page(s) started failing:", result.pages_broken.len());
                for page in result.pages_broken.iter().take(20) {
                    println!(
                        "  {} -> {}  {}",
                        page.status_before.map_or("—".to_string(), |c| c.to_string()),
                        page.status_after.map_or("failed".to_string(), |c| c.to_string()),
                        page.url
                    );
                }
            }

            if !result.pages_removed.is_empty() {
                println!("\n{} page(s) no longer crawled", result.pages_removed.len());
            }
            if !result.pages_added.is_empty() {
                println!("{} page(s) new since last time", result.pages_added.len());
            }

            if fail_on_regression && (!regressions.is_empty() || !result.pages_broken.is_empty()) {
                eprintln!(
                    "\n{} issue(s) got worse and {} page(s) started failing",
                    regressions.len(),
                    result.pages_broken.len()
                );
                std::process::exit(1);
            }
            Ok(())
        }
        Command::Journey { db, persona, goal } => {
            use crawlstat_engine::ux::journey;
            let persona = crawlstat_engine::ux::Persona::parse(&persona)
                .ok_or_else(|| anyhow::anyhow!("unknown persona"))?;
            let store = Store::open(&db)?;

            let goals = match goal {
                Some(id) => journey::preset_goals()
                    .into_iter()
                    .filter(|g| g.id == id)
                    .collect::<Vec<_>>(),
                None => journey::preset_goals(),
            };
            if goals.is_empty() {
                anyhow::bail!("no such goal");
            }

            println!("{}\n", persona.label());
            for goal in &goals {
                let result = journey::simulate(store.connection(), goal, persona)?;
                let mark = match result.outcome.as_str() {
                    "reached" => "ok  ",
                    _ if result.target_exists => "MISS",
                    _ => "none",
                };
                println!(
                    "[{mark}] {:<32} {} click(s), {}",
                    goal.label, result.clicks, result.outcome
                );
                for step in &result.steps {
                    println!("          -> \"{}\"  {}", step.anchor, step.to_url);
                    if !step.alternatives.is_empty() {
                        println!("             (also offered: {})", step.alternatives.join(", "));
                    }
                }
                if !result.outcome.eq("reached") {
                    match (&result.target_url, result.shortest_known) {
                        (Some(url), Some(depth)) => println!(
                            "          the page exists at depth {depth}: {url}"
                        ),
                        _ => println!("          no page on this site looks like it"),
                    }
                }
                println!();
            }
            Ok(())
        }
        Command::Ia { db } => {
            let store = Store::open(&db)?;
            let report = crawlstat_engine::ux::architecture::analyse(store.connection())?;
            println!(
                "{} pages, deepest {} clicks from home, {} links offered on a typical page\n",
                report.pages, report.max_depth, report.median_links
            );

            if !report.ambiguous_labels.is_empty() {
                println!("Link text that goes to more than one place:");
                for finding in report.ambiguous_labels.iter().take(8) {
                    println!("  \"{}\" -> {} different pages", finding.label, finding.destinations);
                }
                println!();
            }
            if !report.inconsistent_labels.is_empty() {
                println!("Pages called different things in different places:");
                for finding in report.inconsistent_labels.iter().take(6) {
                    println!("  {}", finding.url);
                    println!("     called: {}", finding.labels.join(", "));
                }
                println!();
            }
            println!("{} orphan(s), {} page(s) {}+ clicks deep, {} crowded page(s)",
                report.orphans.len(), report.too_deep.len(),
                crawlstat_engine::ux::architecture::DEEP_THRESHOLD, report.crowded.len());
            Ok(())
        }
        Command::Benchmark {
            ours,
            theirs,
            persona,
            task,
        } => {
            use crawlstat_engine::ux::journey;
            let persona = crawlstat_engine::ux::Persona::parse(&persona)
                .ok_or_else(|| anyhow::anyhow!("unknown persona"))?;
            let mine = Store::open(&ours)?;
            let other = Store::open(&theirs)?;

            let results = match task {
                Some(task) => vec![journey::benchmark_task(
                    mine.connection(),
                    other.connection(),
                    persona,
                    &task,
                )?],
                None => journey::benchmark(mine.connection(), other.connection(), persona)?,
            };

            println!("{}\n", persona.label());
            println!("{:<32} {:>7} {:>7}   {}", "Task", "ours", "theirs", "verdict");
            println!("{}", "-".repeat(74));
            for result in &results {
                let clicks = |j: &journey::Journey| {
                    if j.outcome == "reached" {
                        j.clicks.to_string()
                    } else if j.target_exists {
                        "lost".to_string()
                    } else {
                        "none".to_string()
                    }
                };
                println!(
                    "{:<32} {:>7} {:>7}   {}",
                    truncate(&result.goal_label, 32),
                    clicks(&result.ours),
                    clicks(&result.theirs),
                    result.verdict()
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


/// Shorten a label to fit a column, with an ellipsis.
fn truncate(text: &str, width: usize) -> String {
    let trimmed = text.trim();
    if trimmed.chars().count() <= width {
        return trimmed.to_string();
    }
    let kept: String = trimmed.chars().take(width.saturating_sub(1)).collect();
    format!("{kept}…")
}
