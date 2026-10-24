//! Starting, controlling and watching a crawl.
//!
//! The app runs one crawl at a time, so these are process-level operations
//! rather than methods on a handle. That keeps the Dart side free of object
//! lifetimes across the FFI boundary: start, then pause/resume/cancel, then the
//! stream closes.

use std::sync::Arc;

use crawlstat_engine::engine::Crawler;
use crawlstat_engine::progress::ProgressEvent;
use flutter_rust_bridge::frb;
use tokio::sync::mpsc;

// `StreamSink` lives in the generated module, not in the crate root.
use crate::frb_generated::StreamSink;

use super::types::{CrawlEvent, CrawlRequest};
use crate::runtime;

/// Progress events buffered before the engine starts dropping snapshots.
///
/// Generous, because a slow frame should not cost the user a `Url` event — the
/// live table is built from them. The engine drops rather than blocks, so a
/// stalled UI can never stall the crawl.
const EVENT_BUFFER: usize = 4_096;

/// Start a crawl, writing to `db_path`, and stream its progress.
///
/// Returns as soon as the crawl has been accepted; everything after that
/// arrives on the stream. The stream closes when the crawl finishes, is
/// cancelled, or fails.
#[frb(sync)]
pub fn start_crawl(
    request: CrawlRequest,
    db_path: String,
    sink: StreamSink<CrawlEvent>,
) -> Result<(), String> {
    if is_crawl_running() {
        return Err("a crawl is already running".to_string());
    }

    // Built here, not on the worker, so a bad regex or an unusable URL is
    // reported to the dialog that collected it.
    let config = request.to_config().map_err(|error| error.to_string())?;

    // Reading and writing the same file from two connections is fine in WAL
    // mode, but the reader would hold a stale snapshot of a database that is
    // about to be rewritten, so let it reopen on the next query.
    if runtime::open_project_path().as_deref() == Some(std::path::Path::new(&db_path)) {
        runtime::close_project();
    }

    spawn(sink, move |tx| async move {
        Crawler::start(config, &db_path, Some(tx)).await
    });
    Ok(())
}

/// Continue the crawl stored in `db_path`, using its saved configuration.
#[frb(sync)]
pub fn resume_crawl(db_path: String, sink: StreamSink<CrawlEvent>) -> Result<(), String> {
    if is_crawl_running() {
        return Err("a crawl is already running".to_string());
    }
    if runtime::open_project_path().as_deref() == Some(std::path::Path::new(&db_path)) {
        runtime::close_project();
    }

    spawn(sink, move |tx| async move {
        Crawler::resume(&db_path, Some(tx)).await
    });
    Ok(())
}

/// Run a crawl on the shared runtime and forward its events to Dart.
fn spawn<F, Fut>(sink: StreamSink<CrawlEvent>, build: F)
where
    F: FnOnce(mpsc::Sender<ProgressEvent>) -> Fut + Send + 'static,
    Fut: std::future::Future<
            Output = Result<
                (Crawler, crawlstat_engine::engine::CrawlHandle),
                crawlstat_engine::error::EngineError,
            >,
        > + Send,
{
    let sink = Arc::new(sink);

    runtime::runtime().spawn(async move {
        let (tx, mut rx) = mpsc::channel::<ProgressEvent>(EVENT_BUFFER);

        let (crawler, handle) = match build(tx).await {
            Ok(pair) => pair,
            Err(error) => {
                let _ = sink.add(CrawlEvent::Failed {
                    message: error.to_string(),
                });
                return;
            }
        };
        runtime::set_crawl(Some(handle));

        // Forward events on their own task: the crawl must never wait on Dart.
        let forwarder = {
            let sink = Arc::clone(&sink);
            tokio::spawn(async move {
                while let Some(event) = rx.recv().await {
                    // An error here means Dart dropped the stream — the user
                    // closed the screen. The crawl carries on; its results are
                    // in the database either way.
                    if sink.add(CrawlEvent::from(event)).is_err() {
                        break;
                    }
                }
            })
        };

        let outcome = crawler.run().await;
        runtime::set_crawl(None);

        if let Err(error) = outcome {
            let _ = sink.add(CrawlEvent::Failed {
                message: error.to_string(),
            });
        }
        // The engine has stopped sending, so the forwarder's channel closes on
        // its own; wait for it so no event is lost after `Finished`.
        let _ = forwarder.await;
    });
}

/// Stop claiming new URLs. In-flight requests finish and are written.
#[frb(sync)]
pub fn pause_crawl() -> bool {
    runtime::with_crawl(|handle| handle.pause()).is_some()
}

/// Continue a paused crawl.
#[frb(sync)]
pub fn resume_paused_crawl() -> bool {
    runtime::with_crawl(|handle| handle.resume()).is_some()
}

/// Stop the crawl. Queued URLs stay queued, so it can be resumed later.
#[frb(sync)]
pub fn cancel_crawl() -> bool {
    runtime::with_crawl(|handle| handle.cancel()).is_some()
}

#[frb(sync)]
pub fn is_crawl_running() -> bool {
    runtime::with_crawl(|_| ()).is_some()
}

#[frb(sync)]
pub fn is_crawl_paused() -> bool {
    runtime::with_crawl(|handle| handle.is_paused()).unwrap_or(false)
}
