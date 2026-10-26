//! The crawl loop.
//!
//! One task drives everything. It claims URLs from the frontier, spawns a
//! bounded set of visit tasks, collects their results into batches, and commits
//! them through the store task. Nothing else writes to the database, and no
//! worker holds a database handle.
//!
//! ```text
//!   ┌──────────────┐  claim   ┌───────────┐  spawn   ┌───────────┐
//!   │  crawl loop  │─────────▶│  frontier │          │  visit ×N │
//!   │              │◀─────────│  (SQLite) │◀─────────│  (tokio)  │
//!   └──────┬───────┘  batch   └───────────┘  results └───────────┘
//!          │ progress events
//!          ▼
//!     mpsc → CLI / Flutter StreamSink
//! ```
//!
//! ## Memory
//!
//! Peak memory is bounded by `concurrency`, not by the size of the crawl. The
//! loop holds at most a few hundred claimed URLs and one write batch; every
//! visit drops its response body as soon as it has been parsed. A 50,000-URL
//! crawl therefore costs about the same as a 500-URL one.
//!
//! ## Control
//!
//! Pause, resume and cancel travel over a `watch` channel, so they take effect
//! between claims without interrupting a request that is already in flight.

mod host;
mod seed;
mod visit;

use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::{mpsc, watch};
use tokio::task::JoinSet;
use url::Url;

pub use host::HostScheduler;
pub use seed::SeedOutcome;
pub use visit::{visit, VisitOutcome};

use crate::chrome::{self, Chrome, ChromeRenderer, RenderingFetcher};
use crate::config::{CompiledConfig, CrawlConfig, Seed};
use crate::error::{EngineError, Result};
use crate::fetch::{Fetcher, HttpFetcher};
use crate::progress::{
    self, CrawlState, CrawlSummary, CrawledUrlEvent, ProgressEvent, RateTracker,
};
use crate::robots::RobotsCache;
use crate::store::{LogLevel, Store, StoreHandle, WriteBatch};

/// Results committed per transaction.
///
/// Small enough that a crash loses little, large enough that the per-commit
/// cost disappears. Measured on the fixture site, 25 is within noise of 100.
const BATCH_SIZE: usize = 25;

/// How often progress snapshots are emitted.
const PROGRESS_INTERVAL: Duration = Duration::from_millis(400);

/// Everything a visit task needs. Shared behind an `Arc`.
pub struct CrawlContext {
    pub config: Arc<CompiledConfig>,
    pub fetcher: Arc<dyn Fetcher>,
    pub robots: RobotsCache,
    pub hosts: HostScheduler,
    /// The host that defines "internal" for this crawl.
    pub root_host: String,
    pub started: Instant,
}

/// Pause / resume / cancel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Control {
    Run,
    Pause,
    Cancel,
}

/// Controls a running crawl from outside its task.
#[derive(Debug, Clone)]
pub struct CrawlHandle {
    control: watch::Sender<Control>,
}

impl CrawlHandle {
    /// Stop claiming new URLs. In-flight requests finish; the crawl stays
    /// resumable and the task keeps running.
    pub fn pause(&self) {
        let _ = self.control.send(Control::Pause);
    }

    pub fn resume(&self) {
        let _ = self.control.send(Control::Run);
    }

    /// Stop the crawl. In-flight requests are allowed to finish and their
    /// results are written, so nothing is lost; queued URLs stay queued, so the
    /// crawl can be resumed later.
    pub fn cancel(&self) {
        let _ = self.control.send(Control::Cancel);
    }

    pub fn is_paused(&self) -> bool {
        *self.control.borrow() == Control::Pause
    }

    pub fn is_cancelled(&self) -> bool {
        *self.control.borrow() == Control::Cancel
    }
}

/// A crawl, ready to run.
pub struct Crawler {
    ctx: Arc<CrawlContext>,
    store: StoreHandle,
    progress: Option<mpsc::Sender<ProgressEvent>>,
    control: watch::Receiver<Control>,
    /// Keeps the watch channel alive even if every handle is dropped.
    _control_tx: watch::Sender<Control>,
    resumed: bool,
}

impl std::fmt::Debug for Crawler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Crawler")
            .field("root_host", &self.ctx.root_host)
            .field("resumed", &self.resumed)
            .finish_non_exhaustive()
    }
}

/// The production fetcher: plain HTTP, or HTTP followed by headless Chrome
/// when the crawl asks for JavaScript to be rendered.
fn fetcher_for(compiled: &CompiledConfig) -> Result<Arc<dyn Fetcher>> {
    let http = HttpFetcher::new(compiled).map_err(|e| EngineError::HttpClient(e.to_string()))?;
    if !compiled.config.render_javascript {
        return Ok(Arc::new(http));
    }
    let chrome = Chrome::locate().ok_or_else(|| {
        EngineError::Config(
            "rendering JavaScript needs Chrome, Chromium, Edge or Brave installed, and none was \
             found"
                .into(),
        )
    })?;
    let renderer = ChromeRenderer {
        chrome,
        user_agent: compiled.config.user_agent.clone(),
        budget: chrome::DEFAULT_SCRIPT_BUDGET,
        timeout: compiled.config.request_timeout().max(Duration::from_secs(30)),
    };
    let slots = compiled.config.concurrency.min(chrome::DEFAULT_RENDER_SLOTS);
    Ok(Arc::new(RenderingFetcher::new(http, renderer, slots)))
}

impl Crawler {
    /// Start a new crawl, creating or reusing the database at `db_path`.
    pub async fn start(
        config: CrawlConfig,
        db_path: impl AsRef<std::path::Path>,
        progress: Option<mpsc::Sender<ProgressEvent>>,
    ) -> Result<(Self, CrawlHandle)> {
        let compiled = config.compile()?;
        let fetcher = fetcher_for(&compiled)?;
        let store = Store::open(db_path)?;
        Self::assemble(compiled, store, fetcher, progress, false).await
    }

    /// Start a crawl with a caller-supplied fetcher. Used by tests and, later,
    /// by the headless-Chrome renderer.
    pub async fn start_with_fetcher(
        config: CrawlConfig,
        store: Store,
        fetcher: Arc<dyn Fetcher>,
        progress: Option<mpsc::Sender<ProgressEvent>>,
    ) -> Result<(Self, CrawlHandle)> {
        let compiled = config.compile()?;
        Self::assemble(compiled, store, fetcher, progress, false).await
    }

    /// Resume the crawl stored at `db_path`, using its saved configuration.
    ///
    /// Anything left in progress when the previous run stopped is put back on
    /// the queue first, so no URL is lost and none is visited twice.
    pub async fn resume(
        db_path: impl AsRef<std::path::Path>,
        progress: Option<mpsc::Sender<ProgressEvent>>,
    ) -> Result<(Self, CrawlHandle)> {
        let store = Store::open(db_path)?;
        let config = store.stored_config()?.ok_or_else(|| {
            EngineError::Config("this database holds no crawl configuration".into())
        })?;
        let compiled = config.compile()?;
        let fetcher = fetcher_for(&compiled)?;
        Self::assemble(compiled, store, fetcher, progress, true).await
    }

    /// Resume with a caller-supplied fetcher.
    pub async fn resume_with_fetcher(
        store: Store,
        fetcher: Arc<dyn Fetcher>,
        progress: Option<mpsc::Sender<ProgressEvent>>,
    ) -> Result<(Self, CrawlHandle)> {
        let config = store.stored_config()?.ok_or_else(|| {
            EngineError::Config("this database holds no crawl configuration".into())
        })?;
        let compiled = config.compile()?;
        Self::assemble(compiled, store, fetcher, progress, true).await
    }

    async fn assemble(
        mut compiled: CompiledConfig,
        store: Store,
        fetcher: Arc<dyn Fetcher>,
        progress: Option<mpsc::Sender<ProgressEvent>>,
        resumed: bool,
    ) -> Result<(Self, CrawlHandle)> {
        // Licence enforcement happens here, in the engine, not in the UI.
        // Every path into a crawl — the app, the CLI, a future CI runner, a
        // hand-built config — goes through this one line, so there is one
        // place to get right and nothing to route around. The clamped value is
        // what gets stored with the crawl, so a resumed crawl is limited too.
        compiled.config.max_urls = crate::license::max_urls_for_crawl(compiled.config.max_urls);

        let root_host = root_host(&compiled.config)?;

        if resumed {
            let requeued = store.reset_in_progress()?;
            if requeued > 0 {
                store.log(
                    LogLevel::Info,
                    &format!("requeued {requeued} URL(s) that were in flight"),
                    None,
                )?;
            }
        } else {
            store.initialise_crawl(&compiled.config, &root_host)?;
        }

        let config = Arc::new(compiled);
        let ctx = Arc::new(CrawlContext {
            robots: RobotsCache::new(config.robots_token(), config.config.respect_robots),
            hosts: HostScheduler::new(
                config.config.crawl_delay(),
                config.config.concurrency_per_host,
            ),
            root_host,
            started: Instant::now(),
            fetcher,
            config,
        });

        let (control_tx, control_rx) = watch::channel(Control::Run);
        let handle = CrawlHandle {
            control: control_tx.clone(),
        };

        Ok((
            Self {
                ctx,
                store: StoreHandle::spawn(store),
                progress,
                control: control_rx,
                _control_tx: control_tx,
                resumed,
            },
            handle,
        ))
    }

    /// Run to completion. Returns when the frontier is empty, a limit is hit,
    /// or the crawl is cancelled.
    pub async fn run(mut self) -> Result<CrawlSummary> {
        let config = Arc::clone(&self.ctx.config);
        let max_urls = config.config.max_urls;
        let concurrency = config.config.concurrency;

        self.emit(ProgressEvent::StateChanged {
            state: CrawlState::Starting,
        })
        .await;

        if !self.resumed {
            let outcome = seed::seed(&self.ctx, &self.store).await?;
            for (message, url) in &outcome.warnings {
                self.emit(ProgressEvent::Warning {
                    message: message.clone(),
                    url: url.clone(),
                })
                .await;
            }
            if outcome.seeded == 0 {
                self.store.finalize(CrawlState::Failed.as_str()).await?;
                return Err(EngineError::Config(
                    "no usable seed URLs: nothing to crawl".into(),
                ));
            }
        }

        let seeded = self.store.stats().await?.queued;
        self.emit(ProgressEvent::Started {
            seeded,
            resumed: self.resumed,
        })
        .await;
        self.store
            .set_meta("state", CrawlState::Running.as_str())
            .await?;
        self.emit(ProgressEvent::StateChanged {
            state: CrawlState::Running,
        })
        .await;

        let mut tasks: JoinSet<VisitOutcome> = JoinSet::new();
        let mut buffer: std::collections::VecDeque<crate::store::ClaimedUrl> = Default::default();
        let mut pending = WriteBatch::default();
        let mut rate = RateTracker::new();
        let mut ticker = tokio::time::interval(PROGRESS_INTERVAL);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

        // Counts URLs we have *started*, so an in-flight request still counts
        // against the limit and we never overshoot it.
        let mut started = self.store.crawled_count().await?;
        let mut hit_limit = false;
        let mut state = CrawlState::Running;

        loop {
            // --- control --------------------------------------------------
            // Copy the value out: holding the watch guard across an await
            // would keep the channel borrowed.
            let control = *self.control.borrow_and_update();
            match control {
                Control::Cancel => {
                    state = CrawlState::Stopping;
                }
                Control::Pause if tasks.is_empty() => {
                    // Everything has drained: commit and wait to be resumed.
                    self.flush(&mut pending).await?;
                    self.store
                        .set_meta("state", CrawlState::Paused.as_str())
                        .await?;
                    self.emit(ProgressEvent::StateChanged {
                        state: CrawlState::Paused,
                    })
                    .await;
                    if self.control.changed().await.is_err() {
                        break;
                    }
                    if *self.control.borrow() == Control::Run {
                        self.store
                            .set_meta("state", CrawlState::Running.as_str())
                            .await?;
                        self.emit(ProgressEvent::StateChanged {
                            state: CrawlState::Running,
                        })
                        .await;
                    }
                    continue;
                }
                _ => {}
            }

            // --- top up the in-flight set ---------------------------------
            let accepting = state == CrawlState::Running;
            let mut spawned = 0usize;
            while accepting && tasks.len() < concurrency {
                if max_urls.is_some_and(|max| started >= max) {
                    hit_limit = true;
                    break;
                }
                if buffer.is_empty() {
                    // Commit first: URLs discovered on the last batch of pages
                    // have to be in the database before they can be claimed.
                    self.flush(&mut pending).await?;
                    let room = max_urls
                        .map(|max| max.saturating_sub(started) as usize)
                        .unwrap_or(usize::MAX);
                    let want = (concurrency * 4).min(room).max(1);
                    buffer.extend(self.store.claim(want).await?);
                    if buffer.is_empty() {
                        break;
                    }
                }
                let Some(claimed) = buffer.pop_front() else {
                    break;
                };
                let ctx = Arc::clone(&self.ctx);
                tasks.spawn(async move { visit::visit(&ctx, &claimed).await });
                started += 1;
                spawned += 1;
            }

            // Nothing running and nothing left to run: the crawl is over.
            if tasks.is_empty() && spawned == 0 {
                break;
            }

            // --- wait for something to happen -----------------------------
            tokio::select! {
                joined = tasks.join_next(), if !tasks.is_empty() => {
                    match joined {
                        Some(Ok(outcome)) => {
                            rate.record();
                            // A URL that was requeued for a retry, or blocked
                            // by robots.txt, was never actually fetched, so it
                            // must not spend the crawl's URL budget.
                            if !self.handle_outcome(outcome, &mut pending).await {
                                started = started.saturating_sub(1);
                            }
                            if pending.len() >= BATCH_SIZE {
                                self.flush(&mut pending).await?;
                            }
                        }
                        Some(Err(err)) => {
                            // A panic in a visit task is a bug. Log it and keep
                            // crawling; the URL stays in progress and will be
                            // retried if the crawl is resumed.
                            tracing::error!("a visit task failed: {err}");
                        }
                        None => {}
                    }
                }
                _ = ticker.tick() => {
                    let stats = self.store.stats().await?;
                    self.emit(ProgressEvent::Progress(progress::snapshot(
                        state, &stats, tasks.len() as u64, &rate,
                    ))).await;
                }
                changed = self.control.changed() => {
                    if changed.is_err() {
                        break;
                    }
                }
            }
        }

        // --- wind down ----------------------------------------------------
        self.flush(&mut pending).await?;

        let cancelled = *self.control.borrow() == Control::Cancel;
        let final_state = if cancelled {
            CrawlState::Cancelled
        } else {
            CrawlState::Finished
        };

        if hit_limit {
            let skipped = self.store.skip_remaining_queued().await?;
            if skipped > 0 {
                let message = format!(
                    "URL limit of {} reached: {skipped} discovered URL(s) were not crawled",
                    max_urls.unwrap_or_default()
                );
                self.emit(ProgressEvent::Warning { message, url: None })
                    .await;
            }
        }

        self.store.finalize(final_state.as_str()).await?;

        // Audits run over the finished data. A cancelled crawl is audited too:
        // partial findings are still useful, and the UI says the crawl was
        // incomplete.
        let audit = if config.config.run_audits {
            self.emit(ProgressEvent::AuditStarted).await;
            let report = self.store.audit(config.config.audits.clone()).await?;
            let summary = report.summary();
            self.emit(ProgressEvent::Audited { summary }).await;
            Some(summary)
        } else {
            None
        };

        let stats = self.store.stats().await?;
        let summary = CrawlSummary {
            state: final_state,
            crawled: stats.done + stats.failed,
            discovered: stats.total,
            errors: stats.failed,
            status_2xx: stats.status_2xx,
            status_3xx: stats.status_3xx,
            status_4xx: stats.status_4xx,
            status_5xx: stats.status_5xx,
            elapsed_ms: rate.elapsed().as_millis() as u64,
            urls_per_second: if rate.elapsed().as_secs_f64() > 0.0 {
                rate.total() as f64 / rate.elapsed().as_secs_f64()
            } else {
                0.0
            },
            hit_limit,
            audit,
        };

        self.emit(ProgressEvent::StateChanged { state: final_state })
            .await;
        self.emit(ProgressEvent::Finished { summary }).await;
        Ok(summary)
    }

    /// Run to completion and hand back the store, so the caller can query it.
    pub async fn run_and_close(self) -> Result<(CrawlSummary, Store)> {
        let store = self.store.clone();
        let summary = self.run().await?;
        let store = store.close().await?;
        Ok((summary, store))
    }

    /// Record one finished visit. Returns whether it counted as a crawled URL.
    async fn handle_outcome(&mut self, outcome: VisitOutcome, pending: &mut WriteBatch) -> bool {
        let VisitOutcome {
            result,
            depth,
            discovered,
            warnings,
            retryable,
        } = outcome;

        if retryable {
            // Put it back on the queue; `attempts` on the row stops this from
            // looping forever.
            let _ = self.store.requeue(result.url_id).await;
            for (message, url) in warnings {
                self.emit(ProgressEvent::Warning { message, url }).await;
            }
            return false;
        }

        let consumed = result.state != crate::model::UrlState::BlockedByRobots;

        self.emit(ProgressEvent::Url(CrawledUrlEvent {
            url_id: result.url_id,
            url: result.url.clone(),
            status_code: result.status_code,
            content_type: result.content_type.clone(),
            depth,
            response_time_ms: result.response_time.map(|d| d.as_millis() as u64),
            title: result.parsed.as_ref().and_then(|p| p.title.clone()),
            error: result.error_message.clone(),
        }))
        .await;

        for (message, url) in &warnings {
            pending
                .log
                .push((LogLevel::Warning, message.clone(), url.clone()));
            self.emit(ProgressEvent::Warning {
                message: message.clone(),
                url: url.clone(),
            })
            .await;
        }

        pending.results.push(result);
        pending.discovered.extend(discovered);
        consumed
    }

    async fn flush(&self, pending: &mut WriteBatch) -> Result<()> {
        if pending.is_empty() {
            return Ok(());
        }
        let batch = std::mem::take(pending);
        self.store.apply(batch).await?;
        Ok(())
    }

    /// Send a progress event, dropping it if the consumer is not keeping up.
    ///
    /// The database is the source of truth, so a dropped snapshot costs a frame
    /// of UI smoothness and nothing else — far better than stalling the crawl.
    async fn emit(&self, event: ProgressEvent) {
        if let Some(progress) = &self.progress {
            let _ = progress.try_send(event);
        }
    }
}

/// The host that defines "internal" for a crawl: the first seed's host.
fn root_host(config: &CrawlConfig) -> Result<String> {
    let raw = config
        .seeds
        .iter()
        .find_map(|seed| match seed {
            Seed::Url { url } | Seed::Sitemap { url } => Some(url.clone()),
            Seed::List { urls } => urls.first().cloned(),
        })
        .ok_or_else(|| EngineError::Config("no seed URLs given".into()))?;

    let url = crate::url_norm::normalize(&raw, None, &config.normalization)
        .map_err(|e| EngineError::Config(format!("seed `{raw}` is not usable: {e}")))?;

    Url::parse(url.url.as_str())
        .ok()
        .and_then(|u| u.host_str().map(str::to_string))
        .ok_or_else(|| EngineError::Config(format!("seed `{raw}` has no host")))
}

#[cfg(test)]
mod tests;
