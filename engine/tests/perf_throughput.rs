//! Throughput reporting.
//!
//! These numbers are reported, not asserted: on a real site throughput is
//! bounded by the network and by how politely we are configured to behave, and
//! here it is bounded by the test server sharing the machine. What the numbers
//! are good for is spotting a regression between two runs on the same machine,
//! and showing that concurrency actually buys parallelism.
//!
//! ```text
//! cargo test --release --test perf_throughput -- --ignored --nocapture
//! ```

mod fixture;
mod perf_support;

use perf_support::measure;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "performance test: run with --release --ignored --nocapture"]
async fn throughput_scales_with_concurrency() {
    let mut rates = Vec::new();
    for concurrency in [1, 5, 20] {
        let report = measure(2_000, concurrency).await;
        report.print(&format!("2,000 URLs at concurrency {concurrency}"));
        assert_eq!(report.crawled, 2_000);
        rates.push((concurrency, report.urls_per_second));
    }

    println!("\n  concurrency → URLs/sec");
    for (concurrency, rate) in &rates {
        println!("  {concurrency:>11} → {rate:.0}");
    }

    let serial = rates[0].1;
    let parallel = rates[2].1;
    assert!(
        parallel > serial,
        "concurrency 20 ({parallel:.0}/s) was no faster than 1 ({serial:.0}/s)"
    );
}
