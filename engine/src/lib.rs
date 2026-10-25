//! The CrawlStat crawl engine.

pub mod a11y;
pub mod architecture;
pub mod autopilot;
pub mod audit;
pub mod compare;
pub mod config;
pub mod engine;
pub mod error;
pub mod export;
pub mod fetch;
pub mod hash;
pub mod integrations;
pub mod license;
pub mod model;
pub mod parse;
pub mod plan;
pub mod progress;
pub mod robots;
pub mod sitemap;
pub mod store;
pub mod url_norm;
pub mod ux;

/// This engine build's version, shown in the app and sent with crash reports.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

pub use config::{CrawlConfig, CrawlMode, Seed};
pub use error::{EngineError, FetchError};
