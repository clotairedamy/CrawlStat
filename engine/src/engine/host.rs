//! Per-host politeness: request spacing and concurrency.
//!
//! A crawl of one site is thousands of requests at one server. Two limits keep
//! that civil, both applied per host rather than globally:
//!
//! * **Spacing** — at most one request every `crawl_delay`. Tickets are issued
//!   under the lock, so N waiting workers get N distinct slots instead of all
//!   racing through the same one.
//! * **Concurrency** — a per-host semaphore, so a crawl spread over several
//!   subdomains can use the full global concurrency without hammering any one
//!   of them.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::{Mutex, OwnedSemaphorePermit, Semaphore};
use tokio::time::Instant;
use url::Url;

/// Held for the duration of a request. Dropping it frees the host slot.
pub struct HostPermit {
    _permit: OwnedSemaphorePermit,
}

#[derive(Debug)]
struct HostState {
    /// The earliest moment the next request to this host may start.
    next_slot: Instant,
    limiter: Arc<Semaphore>,
}

/// Rate limits and concurrency caps, keyed by host.
#[derive(Debug)]
pub struct HostScheduler {
    hosts: Mutex<HashMap<String, HostState>>,
    default_delay: Duration,
    per_host_concurrency: usize,
}

impl HostScheduler {
    pub fn new(default_delay: Duration, per_host_concurrency: usize) -> Self {
        Self {
            hosts: Mutex::new(HashMap::new()),
            default_delay,
            per_host_concurrency: per_host_concurrency.max(1),
        }
    }

    /// Wait until it is this host's turn, then return a permit.
    ///
    /// `robots_delay` is the host's `Crawl-delay`, when it published one; the
    /// longer of it and the configured delay wins.
    pub async fn acquire(&self, url: &Url, robots_delay: Option<Duration>) -> HostPermit {
        let host = url.host_str().unwrap_or_default().to_ascii_lowercase();
        let delay = match robots_delay {
            Some(robots) => robots.max(self.default_delay),
            None => self.default_delay,
        };

        // Take a numbered slot under the lock, so concurrent callers are spaced
        // out rather than all waking at the same instant.
        let (limiter, slot) = {
            let mut hosts = self.hosts.lock().await;
            let now = Instant::now();
            let state = hosts.entry(host).or_insert_with(|| HostState {
                next_slot: now,
                limiter: Arc::new(Semaphore::new(self.per_host_concurrency)),
            });
            let slot = state.next_slot.max(now);
            state.next_slot = slot + delay;
            (Arc::clone(&state.limiter), slot)
        };

        // Concurrency cap first, then the spacing: a queued worker should not
        // hold a host slot while it waits for a free connection.
        let permit = limiter
            .acquire_owned()
            .await
            .expect("host semaphores are never closed while the crawl runs");

        let now = Instant::now();
        if slot > now {
            tokio::time::sleep_until(slot).await;
        }

        HostPermit { _permit: permit }
    }

    /// Hosts seen so far. Used by tests and diagnostics.
    pub async fn host_count(&self) -> usize {
        self.hosts.lock().await.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn url(raw: &str) -> Url {
        Url::parse(raw).unwrap()
    }

    #[tokio::test(start_paused = true)]
    async fn requests_to_one_host_are_spaced_by_the_delay() {
        let scheduler = HostScheduler::new(Duration::from_millis(500), 5);
        let start = Instant::now();

        for _ in 0..3 {
            let _permit = scheduler.acquire(&url("https://a.test/x"), None).await;
        }

        // Three requests, two gaps: the third starts a full second in.
        assert_eq!(start.elapsed(), Duration::from_millis(1000));
    }

    #[tokio::test(start_paused = true)]
    async fn different_hosts_do_not_wait_for_each_other() {
        let scheduler = HostScheduler::new(Duration::from_millis(500), 5);
        let start = Instant::now();

        let _a = scheduler.acquire(&url("https://a.test/x"), None).await;
        let _b = scheduler.acquire(&url("https://b.test/x"), None).await;
        let _c = scheduler.acquire(&url("https://c.test/x"), None).await;

        assert_eq!(start.elapsed(), Duration::ZERO);
        assert_eq!(scheduler.host_count().await, 3);
    }

    #[tokio::test(start_paused = true)]
    async fn the_longer_of_robots_delay_and_config_wins() {
        let scheduler = HostScheduler::new(Duration::from_millis(100), 5);

        let start = Instant::now();
        let _first = scheduler
            .acquire(&url("https://a.test/1"), Some(Duration::from_secs(2)))
            .await;
        let _second = scheduler.acquire(&url("https://a.test/2"), None).await;
        assert_eq!(
            start.elapsed(),
            Duration::from_secs(2),
            "robots.txt asked for 2s"
        );

        // The other way round: a configured delay longer than robots.txt's.
        let scheduler = HostScheduler::new(Duration::from_secs(3), 5);
        let start = Instant::now();
        let _first = scheduler
            .acquire(&url("https://b.test/1"), Some(Duration::from_millis(10)))
            .await;
        let _second = scheduler.acquire(&url("https://b.test/2"), None).await;
        assert_eq!(start.elapsed(), Duration::from_secs(3));
    }

    #[tokio::test(start_paused = true)]
    async fn concurrent_waiters_each_get_their_own_slot() {
        let scheduler = Arc::new(HostScheduler::new(Duration::from_millis(200), 10));
        let start = Instant::now();

        // Five workers racing for the same host must not collapse onto one slot.
        let mut tasks = tokio::task::JoinSet::new();
        for _ in 0..5 {
            let scheduler = Arc::clone(&scheduler);
            tasks.spawn(async move {
                let _permit = scheduler.acquire(&url("https://a.test/x"), None).await;
                Instant::now()
            });
        }
        let mut finished: Vec<Instant> = Vec::new();
        while let Some(joined) = tasks.join_next().await {
            finished.push(joined.unwrap());
        }
        finished.sort();

        for (index, at) in finished.iter().enumerate() {
            assert_eq!(
                at.duration_since(start),
                Duration::from_millis(200 * index as u64),
                "worker {index} started at the wrong slot"
            );
        }
    }

    #[tokio::test]
    async fn per_host_concurrency_is_capped() {
        let scheduler = Arc::new(HostScheduler::new(Duration::ZERO, 2));
        let a = scheduler.acquire(&url("https://a.test/1"), None).await;
        let b = scheduler.acquire(&url("https://a.test/2"), None).await;

        // A third request to the same host must wait for a permit to free up.
        let scheduler_clone = Arc::clone(&scheduler);
        let third = tokio::spawn(async move {
            scheduler_clone
                .acquire(&url("https://a.test/3"), None)
                .await
        });
        tokio::task::yield_now().await;
        assert!(!third.is_finished(), "the third request should be blocked");

        drop(a);
        let _third = third.await.unwrap();
        drop(b);
    }

    #[tokio::test(start_paused = true)]
    async fn a_zero_delay_never_waits() {
        let scheduler = HostScheduler::new(Duration::ZERO, 5);
        let start = Instant::now();
        for _ in 0..50 {
            let _permit = scheduler.acquire(&url("https://a.test/x"), None).await;
        }
        assert_eq!(start.elapsed(), Duration::ZERO);
    }
}
