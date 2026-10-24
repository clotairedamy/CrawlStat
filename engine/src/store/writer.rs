//! The store task: one blocking thread owning the SQLite connection.
//!
//! Async workers never touch the database directly. They send a [`WriteBatch`]
//! (or a request) down a channel, and this task applies it on the blocking
//! thread pool. That gives us three things for free:
//!
//! * SQLite sees a single writer, so there is no lock contention and no
//!   `SQLITE_BUSY` retry logic.
//! * Writes are naturally batched into transactions, which is the difference
//!   between a few hundred and several thousand URLs per second.
//! * The async side never blocks on disk I/O.

use std::time::Duration;

use tokio::sync::{mpsc, oneshot};

use super::{ClaimedUrl, CrawlStats, LogLevel, Store};
use crate::error::{EngineError, Result};
use crate::model::{CrawledUrl, FrontierUrl};
use crate::sitemap::SitemapEntry;

/// Results and discoveries to be committed together.
#[derive(Debug, Default)]
pub struct WriteBatch {
    /// Pages that have been visited.
    pub results: Vec<CrawledUrl>,
    /// URLs found on them, to be added to the frontier.
    pub discovered: Vec<FrontierUrl>,
    /// `(sitemap URL, entry, dedup key)` triples.
    pub sitemap_entries: Vec<(String, SitemapEntry, String)>,
    /// Crawl-level notices.
    pub log: Vec<(LogLevel, String, Option<String>)>,
}

impl WriteBatch {
    pub fn is_empty(&self) -> bool {
        self.results.is_empty()
            && self.discovered.is_empty()
            && self.sitemap_entries.is_empty()
            && self.log.is_empty()
    }

    pub fn len(&self) -> usize {
        self.results.len()
    }

    pub fn merge(&mut self, other: WriteBatch) {
        self.results.extend(other.results);
        self.discovered.extend(other.discovered);
        self.sitemap_entries.extend(other.sitemap_entries);
        self.log.extend(other.log);
    }
}

/// Messages the store task understands.
enum Command {
    Apply {
        batch: WriteBatch,
        reply: Option<oneshot::Sender<Result<usize>>>,
    },
    Enqueue {
        urls: Vec<FrontierUrl>,
        reply: oneshot::Sender<Result<usize>>,
    },
    Claim {
        limit: usize,
        reply: oneshot::Sender<Result<Vec<ClaimedUrl>>>,
    },
    Requeue {
        id: i64,
    },
    Stats {
        reply: oneshot::Sender<Result<CrawlStats>>,
    },
    CrawledCount {
        reply: oneshot::Sender<Result<u64>>,
    },
    SkipRemaining {
        reply: oneshot::Sender<Result<usize>>,
    },
    SetMeta {
        key: String,
        value: String,
        reply: oneshot::Sender<Result<()>>,
    },
    Finalize {
        state: String,
        reply: oneshot::Sender<Result<()>>,
    },
    Audit {
        config: crate::audit::AuditConfig,
        reply: oneshot::Sender<Result<crate::audit::AuditReport>>,
    },
    /// Hand the connection back, ending the task.
    Close {
        reply: oneshot::Sender<Store>,
    },
}

/// A cloneable handle to the store task.
#[derive(Clone)]
pub struct StoreHandle {
    tx: mpsc::Sender<Command>,
}

/// How many commands may queue up before workers are made to wait.
///
/// Back-pressure is a feature: if the disk cannot keep up, we want the fetchers
/// to slow down rather than the queue to grow without bound.
const CHANNEL_CAPACITY: usize = 256;

impl StoreHandle {
    /// Move `store` onto a blocking task and return a handle to it.
    pub fn spawn(mut store: Store) -> Self {
        let (tx, mut rx) = mpsc::channel::<Command>(CHANNEL_CAPACITY);

        tokio::task::spawn_blocking(move || {
            while let Some(command) = rx.blocking_recv() {
                match command {
                    Command::Apply { batch, reply } => {
                        let result = store.apply(&batch);
                        if let Some(reply) = reply {
                            let _ = reply.send(result);
                        } else if let Err(err) = result {
                            // Nobody is waiting, but a write failure must not
                            // pass silently.
                            tracing::error!("store write failed: {err}");
                        }
                    }
                    Command::Enqueue { urls, reply } => {
                        let _ = reply.send(store.enqueue(&urls));
                    }
                    Command::Claim { limit, reply } => {
                        let _ = reply.send(store.claim(limit));
                    }
                    Command::Requeue { id } => {
                        if let Err(err) = store.requeue(id) {
                            tracing::error!("requeue of url {id} failed: {err}");
                        }
                    }
                    Command::Stats { reply } => {
                        let _ = reply.send(store.stats());
                    }
                    Command::CrawledCount { reply } => {
                        let _ = reply.send(store.crawled_count());
                    }
                    Command::SkipRemaining { reply } => {
                        let _ = reply.send(store.skip_remaining_queued());
                    }
                    Command::SetMeta { key, value, reply } => {
                        let _ = reply.send(store.set_meta(&key, &value));
                    }
                    Command::Audit { config, reply } => {
                        let _ = reply.send(store.audit(&config));
                    }
                    Command::Finalize { state, reply } => {
                        let _ = reply.send(store.finalize(&state));
                    }
                    Command::Close { reply } => {
                        let _ = reply.send(store);
                        return;
                    }
                }
            }
        });

        Self { tx }
    }

    async fn send(&self, command: Command) -> Result<()> {
        self.tx.send(command).await.map_err(|_| {
            EngineError::Other(anyhow::anyhow!("the store task has stopped unexpectedly"))
        })
    }

    async fn request<T>(
        &self,
        make: impl FnOnce(oneshot::Sender<Result<T>>) -> Command,
    ) -> Result<T> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.send(make(reply_tx)).await?;
        reply_rx
            .await
            .map_err(|_| EngineError::Other(anyhow::anyhow!("the store task dropped a reply")))?
    }

    /// Commit a batch, waiting for it to land. Returns the number of newly
    /// queued URLs.
    pub async fn apply(&self, batch: WriteBatch) -> Result<usize> {
        self.request(|reply| Command::Apply {
            batch,
            reply: Some(reply),
        })
        .await
    }

    /// Commit a batch without waiting. Errors are logged.
    pub async fn apply_detached(&self, batch: WriteBatch) -> Result<()> {
        self.send(Command::Apply { batch, reply: None }).await
    }

    pub async fn enqueue(&self, urls: Vec<FrontierUrl>) -> Result<usize> {
        self.request(|reply| Command::Enqueue { urls, reply }).await
    }

    pub async fn claim(&self, limit: usize) -> Result<Vec<ClaimedUrl>> {
        self.request(|reply| Command::Claim { limit, reply }).await
    }

    pub async fn requeue(&self, id: i64) -> Result<()> {
        self.send(Command::Requeue { id }).await
    }

    pub async fn stats(&self) -> Result<CrawlStats> {
        self.request(|reply| Command::Stats { reply }).await
    }

    pub async fn crawled_count(&self) -> Result<u64> {
        self.request(|reply| Command::CrawledCount { reply }).await
    }

    pub async fn skip_remaining_queued(&self) -> Result<usize> {
        self.request(|reply| Command::SkipRemaining { reply }).await
    }

    pub async fn set_meta(&self, key: &str, value: &str) -> Result<()> {
        let (key, value) = (key.to_string(), value.to_string());
        self.request(|reply| Command::SetMeta { key, value, reply })
            .await
    }

    pub async fn finalize(&self, state: &str) -> Result<()> {
        let state = state.to_string();
        self.request(|reply| Command::Finalize { state, reply })
            .await
    }

    /// Run the audit pass. It is blocking work, so it happens on the store
    /// task alongside every other database operation.
    pub async fn audit(
        &self,
        config: crate::audit::AuditConfig,
    ) -> Result<crate::audit::AuditReport> {
        self.request(|reply| Command::Audit { config, reply }).await
    }

    /// Stop the task and take the store back, so the caller can run queries on
    /// it directly.
    pub async fn close(self) -> Result<Store> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.send(Command::Close { reply: reply_tx }).await?;
        // The task may be mid-batch; give it a moment to finish and hand over.
        tokio::time::timeout(Duration::from_secs(30), reply_rx)
            .await
            .map_err(|_| EngineError::Other(anyhow::anyhow!("the store task did not shut down")))?
            .map_err(|_| EngineError::Other(anyhow::anyhow!("the store task dropped the store")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ResourceKind, UrlState};

    fn frontier_url(url: &str, depth: u32) -> FrontierUrl {
        FrontierUrl {
            url: url.to_string(),
            key: url.to_string(),
            depth,
            discovered_from: None,
            via: None,
            kind: ResourceKind::Html,
            is_internal: true,
            state: UrlState::Queued,
        }
    }

    #[tokio::test]
    async fn handle_round_trips_through_the_task() {
        let handle = StoreHandle::spawn(Store::open_in_memory().unwrap());

        let added = handle
            .enqueue(vec![
                frontier_url("https://a.test/", 0),
                frontier_url("https://a.test/x", 1),
            ])
            .await
            .unwrap();
        assert_eq!(added, 2);

        // Duplicates are rejected by the unique index, not by a lookup.
        assert_eq!(
            handle
                .enqueue(vec![frontier_url("https://a.test/", 0)])
                .await
                .unwrap(),
            0
        );

        let claimed = handle.claim(10).await.unwrap();
        assert_eq!(claimed.len(), 2);
        assert_eq!(claimed[0].url, "https://a.test/", "lowest depth first");
        assert_eq!(claimed[0].attempts, 1);

        let stats = handle.stats().await.unwrap();
        assert_eq!((stats.total, stats.in_progress, stats.queued), (2, 2, 0));

        handle.set_meta("state", "paused").await.unwrap();
        let store = handle.close().await.unwrap();
        assert_eq!(store.meta("state").unwrap().as_deref(), Some("paused"));
    }

    #[tokio::test]
    async fn requeue_puts_a_url_back() {
        let handle = StoreHandle::spawn(Store::open_in_memory().unwrap());
        handle
            .enqueue(vec![frontier_url("https://a.test/", 0)])
            .await
            .unwrap();
        let claimed = handle.claim(1).await.unwrap();
        handle.requeue(claimed[0].id).await.unwrap();

        // Requeue is fire-and-forget, so wait for a round trip before asserting.
        let stats = handle.stats().await.unwrap();
        assert_eq!(stats.queued, 1);
        assert_eq!(handle.claim(1).await.unwrap()[0].attempts, 2);
    }

    #[test]
    fn batches_merge() {
        let mut a = WriteBatch {
            discovered: vec![frontier_url("https://a.test/1", 1)],
            log: vec![(LogLevel::Info, "one".into(), None)],
            ..Default::default()
        };
        let b = WriteBatch {
            discovered: vec![frontier_url("https://a.test/2", 1)],
            ..Default::default()
        };
        a.merge(b);
        assert_eq!(a.discovered.len(), 2);
        assert_eq!(a.log.len(), 1);
        assert!(!a.is_empty());
        assert!(WriteBatch::default().is_empty());
    }
}
