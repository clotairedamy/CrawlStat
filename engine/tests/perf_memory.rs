//! The memory acceptance test: a 50,000-URL crawl must stay under ~500 MB.
//!
//! This lives in its own test binary on purpose. `ru_maxrss` is a per-process
//! high-water mark, so a second crawl in the same process would inherit the
//! first one's peak and the figure would mean nothing.
//!
//! ```text
//! cargo test --release --test perf_memory -- --ignored --nocapture
//! ```

mod fixture;
mod perf_support;

use perf_support::{measure, mib};

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "performance test: run with --release --ignored --nocapture"]
async fn fifty_thousand_urls_stay_within_the_memory_budget() {
    let report = measure(50_000, 20).await;
    report.print("50,000 URLs");

    assert_eq!(report.crawled, 50_000);
    assert!(
        report.peak_rss_bytes < 500 * 1024 * 1024,
        "peak RSS was {:.0} MiB, over the 500 MB budget",
        mib(report.peak_rss_bytes)
    );
}
