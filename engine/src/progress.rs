//! Progress events streamed out of a running crawl.
//!
//! The engine pushes these down an `mpsc` channel. The CLI prints them; in
//! Phase 3 the `flutter_rust_bridge` layer forwards the same enum to a Dart
//! `StreamSink`, so the UI and the CLI see exactly the same crawl.
//!
//! Events are *advisory*: if the consumer is slow, snapshots are dropped rather
//! than allowed to slow the crawl down. Nothing in the engine's correctness
//! depends on an event being delivered — the database is the source of truth.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::store::CrawlStats;

/// What a crawl is currently doing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CrawlState {
    Starting,
    Running,
    Paused,
    /// Cancelled; in-flight requests are being allowed to finish.
    Stopping,
    Finished,
    Cancelled,
    Failed,
}

impl CrawlState {
    pub fn as_str(self) -> &'static str {
        match self {
            CrawlState::Starting => "starting",
            CrawlState::Running => "running",
            CrawlState::Paused => "paused",
            CrawlState::Stopping => "stopping",
            CrawlState::Finished => "finished",
            CrawlState::Cancelled => "cancelled",
            CrawlState::Failed => "failed",
        }
    }

    /// Whether the crawl has stopped for good.
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            CrawlState::Finished | CrawlState::Cancelled | CrawlState::Failed
        )
    }
}

/// A point-in-time view of the crawl, suitable for a progress bar.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ProgressSnapshot {
    pub state: CrawlState,
    /// URLs fetched (successfully or not).
    pub crawled: u64,
    pub queued: u64,
    pub in_flight: u64,
    /// Fetch failures — network, DNS, timeouts. Not HTTP error statuses.
    pub errors: u64,
    pub status_4xx: u64,
    pub status_5xx: u64,
    /// Every URL known: crawled, queued, skipped.
    pub discovered: u64,
    pub urls_per_second: f64,
    pub elapsed_ms: u64,
}

/// One finished URL, for the live results table.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CrawledUrlEvent {
    pub url_id: i64,
    pub url: String,
    pub status_code: Option<u16>,
    pub content_type: Option<String>,
    pub depth: u32,
    pub response_time_ms: Option<u64>,
    pub title: Option<String>,
    pub error: Option<String>,
}

/// Everything the engine reports while running.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum ProgressEvent {
    /// The crawl has been seeded and is about to start fetching.
    Started {
        seeded: u64,
        resumed: bool,
    },
    /// A URL finished.
    Url(CrawledUrlEvent),
    /// Periodic counters.
    Progress(ProgressSnapshot),
    /// Something the user should know: an unreachable robots.txt, a sitemap
    /// that would not parse, a limit that was reached.
    Warning {
        message: String,
        url: Option<String>,
    },
    StateChanged {
        state: CrawlState,
    },
    /// The audit pass has started. It runs after the crawl and takes a moment
    /// on a large one, so the UI needs to be able to say so.
    AuditStarted,
    /// The audit pass has finished.
    Audited {
        summary: crate::audit::AuditSummary,
    },
    /// The crawl is over.
    Finished {
        summary: CrawlSummary,
    },
}

/// The result of a completed crawl.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CrawlSummary {
    pub state: CrawlState,
    pub crawled: u64,
    pub discovered: u64,
    pub errors: u64,
    pub status_2xx: u64,
    pub status_3xx: u64,
    pub status_4xx: u64,
    pub status_5xx: u64,
    pub elapsed_ms: u64,
    pub urls_per_second: f64,
    /// True when a configured limit stopped the crawl early.
    pub hit_limit: bool,
    /// Present when the audit pass ran.
    pub audit: Option<crate::audit::AuditSummary>,
}

/// Tracks throughput over a short sliding window.
///
/// A crawl's average rate since the start is not what a user wants to see — it
/// hides the slowdown when the crawler reaches a slow section of a site. This
/// keeps the last [`WINDOW`] completions and reports the rate across them.
#[derive(Debug)]
pub struct RateTracker {
    started: Instant,
    completions: VecDeque<Instant>,
    total: u64,
}

/// Completions kept for the sliding-window rate. Bounded, so memory is flat.
const WINDOW: usize = 128;

impl RateTracker {
    pub fn new() -> Self {
        Self {
            started: Instant::now(),
            completions: VecDeque::with_capacity(WINDOW),
            total: 0,
        }
    }

    pub fn record(&mut self) {
        self.total += 1;
        if self.completions.len() == WINDOW {
            self.completions.pop_front();
        }
        self.completions.push_back(Instant::now());
    }

    pub fn total(&self) -> u64 {
        self.total
    }

    pub fn elapsed(&self) -> Duration {
        self.started.elapsed()
    }

    /// URLs per second over the recent window, falling back to the overall
    /// average while the window is still filling.
    pub fn rate(&self) -> f64 {
        if self.completions.len() < 2 {
            let secs = self.started.elapsed().as_secs_f64();
            return if secs > 0.0 {
                self.total as f64 / secs
            } else {
                0.0
            };
        }
        let first = *self.completions.front().unwrap();
        let last = *self.completions.back().unwrap();
        let span = last.duration_since(first).as_secs_f64();
        if span <= 0.0 {
            return 0.0;
        }
        (self.completions.len() - 1) as f64 / span
    }
}

impl Default for RateTracker {
    fn default() -> Self {
        Self::new()
    }
}

/// Build a snapshot from the store's counters plus live engine state.
pub fn snapshot(
    state: CrawlState,
    stats: &CrawlStats,
    in_flight: u64,
    rate: &RateTracker,
) -> ProgressSnapshot {
    ProgressSnapshot {
        state,
        crawled: stats.done + stats.failed,
        queued: stats.queued,
        in_flight,
        errors: stats.failed,
        status_4xx: stats.status_4xx,
        status_5xx: stats.status_5xx,
        discovered: stats.total,
        urls_per_second: rate.rate(),
        elapsed_ms: rate.elapsed().as_millis() as u64,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn states_know_when_they_are_final() {
        assert!(CrawlState::Finished.is_terminal());
        assert!(CrawlState::Cancelled.is_terminal());
        assert!(!CrawlState::Paused.is_terminal());
        assert!(!CrawlState::Running.is_terminal());
    }

    #[test]
    fn rate_is_zero_before_anything_completes() {
        let tracker = RateTracker::new();
        assert_eq!(tracker.rate(), 0.0);
        assert_eq!(tracker.total(), 0);
    }

    #[test]
    fn rate_reflects_recent_completions() {
        let mut tracker = RateTracker::new();
        for _ in 0..10 {
            tracker.record();
            std::thread::sleep(Duration::from_millis(2));
        }
        assert_eq!(tracker.total(), 10);
        // ~2ms apart, so a few hundred per second. The bound is loose on
        // purpose: this asserts the arithmetic, not the machine's timing.
        let rate = tracker.rate();
        assert!(rate > 50.0 && rate < 5000.0, "unexpected rate {rate}");
    }

    #[test]
    fn the_window_is_bounded() {
        let mut tracker = RateTracker::new();
        for _ in 0..(WINDOW * 3) {
            tracker.record();
        }
        assert_eq!(tracker.completions.len(), WINDOW);
        assert_eq!(tracker.total(), (WINDOW * 3) as u64);
    }

    #[test]
    fn events_round_trip_as_json() {
        // The Flutter bridge and any future CLI/CI consumer both serialise
        // these, so the tagged shape is part of the contract.
        let event = ProgressEvent::Url(CrawledUrlEvent {
            url_id: 1,
            url: "https://a.test/".into(),
            status_code: Some(200),
            content_type: Some("text/html".into()),
            depth: 0,
            response_time_ms: Some(42),
            title: Some("Home".into()),
            error: None,
        });
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"event\":\"url\""));
        assert_eq!(serde_json::from_str::<ProgressEvent>(&json).unwrap(), event);

        let warning = ProgressEvent::Warning {
            message: "robots.txt unreachable".into(),
            url: None,
        };
        let json = serde_json::to_string(&warning).unwrap();
        assert_eq!(
            serde_json::from_str::<ProgressEvent>(&json).unwrap(),
            warning
        );
    }

    #[test]
    fn snapshot_derives_counters_from_the_store() {
        let stats = CrawlStats {
            total: 100,
            queued: 40,
            done: 50,
            failed: 10,
            status_4xx: 3,
            status_5xx: 1,
            ..Default::default()
        };
        let snap = snapshot(CrawlState::Running, &stats, 5, &RateTracker::new());
        assert_eq!(snap.crawled, 60);
        assert_eq!(snap.discovered, 100);
        assert_eq!(snap.errors, 10);
        assert_eq!(snap.in_flight, 5);
    }
}
