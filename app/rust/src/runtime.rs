//! Process-wide state the UI needs: an async runtime, the running crawl, and
//! the open project's database handle.
//!
//! The app shows one crawl and one project at a time, so this is a few globals
//! rather than a handle-passing API. That keeps the Dart side simple — no
//! lifetimes to manage across the FFI boundary — at the cost of making the
//! single-crawl assumption explicit, which it is in the UI anyway.

use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use crawlstat_engine::engine::CrawlHandle;
use crawlstat_engine::error::EngineError;
use crawlstat_engine::store::Store;
use tokio::runtime::Runtime;

/// The runtime every crawl runs on.
///
/// Created once, on first use. Dart calls arrive on `flutter_rust_bridge`'s own
/// worker threads, which are not async, so the crawl needs a runtime of its own
/// to live in.
pub fn runtime() -> &'static Runtime {
    static RUNTIME: OnceLock<Runtime> = OnceLock::new();
    RUNTIME.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .thread_name("crawlstat-engine")
            .enable_all()
            .build()
            .expect("the tokio runtime should start")
    })
}

/// The crawl currently running, if any.
fn crawl_slot() -> &'static Mutex<Option<CrawlHandle>> {
    static CRAWL: OnceLock<Mutex<Option<CrawlHandle>>> = OnceLock::new();
    CRAWL.get_or_init(|| Mutex::new(None))
}

pub fn set_crawl(handle: Option<CrawlHandle>) {
    *crawl_slot()
        .lock()
        .expect("the crawl lock is never poisoned") = handle;
}

/// Run something with the current crawl's handle, if there is one.
pub fn with_crawl<T>(action: impl FnOnce(&CrawlHandle) -> T) -> Option<T> {
    crawl_slot()
        .lock()
        .expect("the crawl lock is never poisoned")
        .as_ref()
        .map(action)
}

/// The project database the UI is reading.
struct OpenProject {
    path: PathBuf,
    store: Store,
}

fn project_slot() -> &'static Mutex<Option<OpenProject>> {
    static PROJECT: OnceLock<Mutex<Option<OpenProject>>> = OnceLock::new();
    PROJECT.get_or_init(|| Mutex::new(None))
}

/// Open a crawl database for reading, replacing whatever was open.
///
/// Reads go through a long-lived connection rather than opening the file per
/// query, so SQLite's page cache stays warm while the user pages through a
/// table of a hundred thousand rows.
pub fn open_project(path: &Path) -> Result<(), EngineError> {
    let store = Store::open(path)?;
    *project_slot()
        .lock()
        .expect("the project lock is never poisoned") = Some(OpenProject {
        path: path.to_path_buf(),
        store,
    });
    Ok(())
}

pub fn close_project() {
    *project_slot()
        .lock()
        .expect("the project lock is never poisoned") = None;
}

pub fn open_project_path() -> Option<PathBuf> {
    project_slot()
        .lock()
        .expect("the project lock is never poisoned")
        .as_ref()
        .map(|project| project.path.clone())
}

/// Read from the open project.
///
/// Opens `path` first if a different project — or none — is currently open, so
/// Dart never has to sequence an explicit open before a query.
pub fn with_store<T>(
    path: &str,
    action: impl FnOnce(&Store) -> Result<T, EngineError>,
) -> Result<T, EngineError> {
    let wanted = Path::new(path);
    let needs_open = {
        let slot = project_slot()
            .lock()
            .expect("the project lock is never poisoned");
        match slot.as_ref() {
            Some(project) => project.path != wanted,
            None => true,
        }
    };
    if needs_open {
        open_project(wanted)?;
    }

    let slot = project_slot()
        .lock()
        .expect("the project lock is never poisoned");
    let project = slot.as_ref().expect("just opened");
    action(&project.store)
}

/// Read and write the open project — for the audit pass, which writes.
pub fn with_store_mut<T>(
    path: &str,
    action: impl FnOnce(&mut Store) -> Result<T, EngineError>,
) -> Result<T, EngineError> {
    let wanted = Path::new(path);
    let needs_open = {
        let slot = project_slot()
            .lock()
            .expect("the project lock is never poisoned");
        match slot.as_ref() {
            Some(project) => project.path != wanted,
            None => true,
        }
    };
    if needs_open {
        open_project(wanted)?;
    }

    let mut slot = project_slot()
        .lock()
        .expect("the project lock is never poisoned");
    let project = slot.as_mut().expect("just opened");
    action(&mut project.store)
}
