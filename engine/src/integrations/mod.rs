//! Phase 5: the things the user brings their own key for.
//!
//! Everything here is optional by construction. CrawlStat crawls, audits and
//! plans with none of it configured; these only add what an outside service
//! knows that a crawler cannot — real-world performance numbers, search
//! impressions, or a local model's prose.
//!
//! The rule that governs all of it: **no crawl data leaves the machine except
//! through a call the user set up and triggered.** Keys live in the OS
//! keychain ([`secrets`]) and never cross into Dart; the language model runs on
//! loopback ([`ollama`]).

pub mod ollama;
pub mod secrets;
