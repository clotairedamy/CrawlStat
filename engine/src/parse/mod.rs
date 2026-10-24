//! Turning response bodies into records.

mod html;
pub mod structured;

pub use html::{parse_html, parse_meta_refresh_target};

#[cfg(test)]
mod tests;
