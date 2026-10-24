//! Memory must be flat in the size of the crawl.
//!
//! This is the design claim that matters: crawl state lives in SQLite, so a
//! bigger crawl costs time and disk, not RAM. Past the point where SQLite's
//! fixed page cache is full, doubling the crawl should barely move peak memory.
//!
//! Each measurement needs its own process, because peak RSS is a per-process
//! high-water mark. So this test runs the `memory_profile` example twice as a
//! subprocess and compares the two results.
//!
//! ```text
//! cargo test --release --test perf_flatness -- --ignored --nocapture
//! ```

use std::path::PathBuf;
use std::process::Command;

/// Locate the compiled `memory_profile` example next to this test binary.
fn example_path() -> Option<PathBuf> {
    // .../target/<profile>/deps/perf_flatness-<hash>
    let exe = std::env::current_exe().ok()?;
    let profile_dir = exe.parent()?.parent()?;
    let candidate = profile_dir.join("examples").join("memory_profile");
    candidate.exists().then_some(candidate)
}

struct Measurement {
    crawled: u64,
    peak_rss_bytes: u64,
}

fn run(path: &PathBuf, urls: u64, concurrency: usize) -> Measurement {
    let output = Command::new(path)
        .arg(urls.to_string())
        .arg(concurrency.to_string())
        .output()
        .expect("the memory_profile example should run");
    assert!(
        output.status.success(),
        "memory_profile failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let result = stdout
        .lines()
        .find(|line| line.starts_with("RESULT "))
        .unwrap_or_else(|| panic!("no RESULT line in:\n{stdout}"));

    let field = |name: &str| -> u64 {
        result
            .split_whitespace()
            .find_map(|pair| pair.strip_prefix(&format!("{name}=")))
            .and_then(|value| value.parse().ok())
            .unwrap_or_else(|| panic!("{name} missing from: {result}"))
    };

    Measurement {
        crawled: field("crawled"),
        peak_rss_bytes: field("peak_rss_bytes"),
    }
}

fn mib(bytes: u64) -> f64 {
    bytes as f64 / (1024.0 * 1024.0)
}

#[test]
#[ignore = "performance test: run with --release --ignored --nocapture"]
fn doubling_the_crawl_barely_moves_peak_memory() {
    let Some(path) = example_path() else {
        panic!(
            "the memory_profile example is not built — run \
             `cargo build --release --example memory_profile` first"
        );
    };

    let small = run(&path, 30_000, 10);
    let large = run(&path, 60_000, 10);

    println!(
        "\n  {:>6} URLs  {:>5.0} MiB",
        small.crawled,
        mib(small.peak_rss_bytes)
    );
    println!(
        "  {:>6} URLs  {:>5.0} MiB",
        large.crawled,
        mib(large.peak_rss_bytes)
    );

    let ratio = large.peak_rss_bytes as f64 / small.peak_rss_bytes as f64;
    let extra_urls = (large.crawled - small.crawled) as f64;
    let marginal = large.peak_rss_bytes.saturating_sub(small.peak_rss_bytes) as f64 / extra_urls;

    println!("\n  2× the URLs cost {ratio:.2}× the peak memory");
    println!("  marginal cost    {marginal:.0} B of RAM per URL");

    assert!(
        ratio < 1.5,
        "peak memory grew {ratio:.2}× when the crawl doubled — state is being accumulated"
    );
    assert!(
        marginal < 1_200.0,
        "{marginal:.0} bytes of RAM per additional URL is too much to call flat"
    );
}
