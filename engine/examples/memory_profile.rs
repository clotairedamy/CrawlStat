//! Measures one crawl's throughput and peak memory in a fresh process.
//!
//! Peak RSS is a per-process high-water mark, so several crawls in one process
//! cannot be compared: the second always inherits the first's peak. This binary
//! does exactly one crawl, which makes the numbers comparable across runs.
//!
//! ```text
//! cargo run --release --example memory_profile -- 10000 10
//! cargo run --release --example memory_profile -- 50000 20
//! ```
//!
//! It also samples resident memory while the crawl runs, so the shape of the
//! curve is visible: a plateau means fixed caches, a ramp means the engine is
//! accumulating state.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crawlstat_engine::config::CrawlConfig;
use crawlstat_engine::engine::Crawler;
use crawlstat_engine::fetch::HttpFetcher;
use crawlstat_engine::store::Store;

#[path = "../tests/fixture/mod.rs"]
mod fixture;

fn peak_rss_bytes() -> u64 {
    // SAFETY: `getrusage` only writes into the struct we pass it.
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

/// Current resident memory, via `ps`. Peak alone cannot show the curve.
fn current_rss_bytes() -> u64 {
    std::process::Command::new("ps")
        .args(["-o", "rss=", "-p", &std::process::id().to_string()])
        .output()
        .ok()
        .and_then(|out| String::from_utf8(out.stdout).ok())
        .and_then(|text| text.trim().parse::<u64>().ok())
        .map(|kib| kib * 1024)
        .unwrap_or(0)
}

fn mib(bytes: u64) -> f64 {
    bytes as f64 / (1024.0 * 1024.0)
}

#[tokio::main(flavor = "multi_thread", worker_threads = 4)]
async fn main() {
    let mut args = std::env::args().skip(1);
    let max_urls: u64 = args.next().and_then(|a| a.parse().ok()).unwrap_or(10_000);
    let concurrency: usize = args.next().and_then(|a| a.parse().ok()).unwrap_or(10);

    let site = fixture::spawn().await;

    let mut config = CrawlConfig::spider(site.url("/page/1"));
    config.max_urls = Some(max_urls);
    config.concurrency = concurrency;
    config.concurrency_per_host = concurrency;
    config.crawl_delay_ms = 0;
    config.respect_robots = false;
    config.check_external_links = false;
    config.resources.images = false;

    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("profile.db");

    let compiled = config.compile().unwrap();
    let fetcher = Arc::new(HttpFetcher::new(&compiled).unwrap());
    let store = Store::open(&db_path).unwrap();

    // Sample resident memory while the crawl runs.
    let sampling = Arc::new(AtomicBool::new(true));
    let samples = Arc::new(std::sync::Mutex::new(Vec::<(f64, u64)>::new()));
    {
        let sampling = Arc::clone(&sampling);
        let samples = Arc::clone(&samples);
        let started = Instant::now();
        std::thread::spawn(move || {
            while sampling.load(Ordering::Relaxed) {
                samples
                    .lock()
                    .unwrap()
                    .push((started.elapsed().as_secs_f64(), current_rss_bytes()));
                std::thread::sleep(Duration::from_millis(250));
            }
        });
    }

    let baseline = current_rss_bytes();
    let started = Instant::now();
    let (crawler, _handle) = Crawler::start_with_fetcher(config, store, fetcher, None)
        .await
        .unwrap();
    let (summary, store) = crawler.run_and_close().await.unwrap();
    let crawl_elapsed = started.elapsed();
    let after_crawl = current_rss_bytes();

    sampling.store(false, Ordering::Relaxed);
    store.checkpoint().unwrap();
    let db_bytes = std::fs::metadata(&db_path).map(|m| m.len()).unwrap_or(0);

    println!("crawled          {}", summary.crawled);
    println!("concurrency      {concurrency}");
    println!("elapsed          {:.1}s", crawl_elapsed.as_secs_f64());
    println!(
        "throughput       {:.0} URLs/sec",
        summary.crawled as f64 / crawl_elapsed.as_secs_f64()
    );
    println!("baseline RSS     {:.0} MiB", mib(baseline));
    println!("RSS after crawl  {:.0} MiB", mib(after_crawl));
    println!("peak RSS         {:.0} MiB", mib(peak_rss_bytes()));
    println!("database         {:.1} MiB", mib(db_bytes));

    // One machine-readable line, so the flatness test can parse it.
    println!(
        "RESULT crawled={} peak_rss_bytes={} elapsed_ms={} db_bytes={}",
        summary.crawled,
        peak_rss_bytes(),
        crawl_elapsed.as_millis(),
        db_bytes
    );

    let samples = samples.lock().unwrap();
    if samples.len() > 2 {
        println!("\nresident memory over time");
        // Print at most 20 evenly spaced samples.
        let step = (samples.len() / 20).max(1);
        for (at, rss) in samples.iter().step_by(step) {
            let bar = "█".repeat((mib(*rss) / 10.0).round() as usize);
            println!("  {at:>5.1}s  {:>5.0} MiB  {bar}", mib(*rss));
        }
    }
}
