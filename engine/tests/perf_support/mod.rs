//! Shared measurement helper for the performance tests.

#![allow(dead_code)]

use std::sync::Arc;
use std::time::Instant;

use crawlstat_engine::config::CrawlConfig;
use crawlstat_engine::engine::Crawler;
use crawlstat_engine::fetch::HttpFetcher;
use crawlstat_engine::store::Store;

use crate::fixture;

/// Peak resident set size of this process, in bytes.
///
/// `ru_maxrss` is a high-water mark, so it does not matter when we read it and
/// no sampling loop is needed. The unit differs by platform: bytes on macOS,
/// kilobytes on Linux.
pub fn peak_rss_bytes() -> u64 {
    // SAFETY: `getrusage` only writes into the struct we hand it.
    let usage = unsafe {
        let mut usage: libc::rusage = std::mem::zeroed();
        libc::getrusage(libc::RUSAGE_SELF, &mut usage);
        usage
    };
    let max_rss = usage.ru_maxrss as u64;
    if cfg!(target_os = "macos") {
        max_rss
    } else {
        max_rss * 1024
    }
}

pub fn mib(bytes: u64) -> f64 {
    bytes as f64 / (1024.0 * 1024.0)
}

pub struct Report {
    pub crawled: u64,
    pub discovered: u64,
    pub urls_per_second: f64,
    pub elapsed_secs: f64,
    pub peak_rss_bytes: u64,
    pub db_bytes: u64,
    pub concurrency: usize,
}

impl Report {
    pub fn print(&self, label: &str) {
        println!("\n── {label} ───────────────────────────────");
        println!("  crawled          {}", self.crawled);
        println!("  discovered       {}", self.discovered);
        println!("  concurrency      {}", self.concurrency);
        println!("  elapsed          {:.1}s", self.elapsed_secs);
        println!("  throughput       {:.0} URLs/sec", self.urls_per_second);
        println!("  peak RSS         {:.0} MiB", mib(self.peak_rss_bytes));
        println!("  database         {:.1} MiB", mib(self.db_bytes));
        println!(
            "  bytes per URL    {:.0} B on disk",
            self.db_bytes as f64 / self.crawled.max(1) as f64
        );
    }
}

/// Crawl the fixture site's generated pages and report what it cost.
pub async fn measure(max_urls: u64, concurrency: usize) -> Report {
    let site = fixture::spawn().await;

    let mut config = CrawlConfig::spider(site.url("/page/1"));
    config.max_urls = Some(max_urls);
    config.concurrency = concurrency;
    config.concurrency_per_host = concurrency;
    // The fixture is on loopback: politeness would only measure our own sleeps,
    // and robots fetching would add noise.
    config.crawl_delay_ms = 0;
    config.respect_robots = false;
    config.check_external_links = false;
    // One shared image would otherwise be a large share of the URL count.
    config.resources.images = false;

    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("perf.db");

    let compiled = config.compile().unwrap();
    let fetcher = Arc::new(HttpFetcher::new(&compiled).unwrap());
    let store = Store::open(&db_path).unwrap();

    let started = Instant::now();
    let (crawler, _handle) = Crawler::start_with_fetcher(config, store, fetcher, None)
        .await
        .unwrap();
    let (summary, store) = crawler.run_and_close().await.unwrap();
    let elapsed = started.elapsed();

    store.checkpoint().unwrap();
    let db_bytes = std::fs::metadata(&db_path).map(|m| m.len()).unwrap_or(0);
    let stats = store.stats().unwrap();

    Report {
        crawled: summary.crawled,
        discovered: stats.total,
        urls_per_second: summary.crawled as f64 / elapsed.as_secs_f64(),
        elapsed_secs: elapsed.as_secs_f64(),
        peak_rss_bytes: peak_rss_bytes(),
        db_bytes,
        concurrency,
    }
}
