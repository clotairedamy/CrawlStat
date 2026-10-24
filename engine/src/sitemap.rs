//! XML sitemap parsing: `<urlset>`, `<sitemapindex>`, and gzipped variants.
//!
//! Sitemaps are a seeding source (crawl everything a site declares) and an
//! audit input (a URL in the sitemap that nothing links to is an orphan page).
//!
//! The parser is streaming and bounded: it never holds more than
//! [`MAX_ENTRIES`] entries, so a hostile or accidentally enormous sitemap
//! cannot exhaust memory.

use std::io::Read;

use flate2::read::GzDecoder;
use quick_xml::events::Event;
use quick_xml::Reader;

/// Per the sitemaps.org protocol a single file holds at most 50,000 URLs. We
/// accept a little slack and then stop.
pub const MAX_ENTRIES: usize = 60_000;

/// Largest sitemap body we will decompress, to bound a gzip bomb.
pub const MAX_DECOMPRESSED_BYTES: usize = 128 * 1024 * 1024;

/// One `<url>` entry.
#[derive(Debug, Clone, PartialEq)]
pub struct SitemapEntry {
    pub loc: String,
    pub lastmod: Option<String>,
    pub changefreq: Option<String>,
    pub priority: Option<f32>,
}

/// What a sitemap file turned out to contain.
#[derive(Debug, Clone, PartialEq)]
pub enum Sitemap {
    /// A `<sitemapindex>`: the locations of further sitemaps.
    Index { sitemaps: Vec<String> },
    /// A `<urlset>`: the URLs themselves.
    UrlSet { entries: Vec<SitemapEntry> },
}

impl Sitemap {
    /// Every URL in this document, whatever its kind.
    pub fn locations(&self) -> Vec<&str> {
        match self {
            Sitemap::Index { sitemaps } => sitemaps.iter().map(String::as_str).collect(),
            Sitemap::UrlSet { entries } => entries.iter().map(|e| e.loc.as_str()).collect(),
        }
    }

    pub fn is_empty(&self) -> bool {
        match self {
            Sitemap::Index { sitemaps } => sitemaps.is_empty(),
            Sitemap::UrlSet { entries } => entries.is_empty(),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SitemapError {
    #[error("not valid XML: {0}")]
    Xml(String),
    #[error("gzip decode failed: {0}")]
    Gzip(String),
    #[error("no <urlset> or <sitemapindex> element found")]
    NotASitemap,
}

/// Decompress the body when it is gzipped.
///
/// Detection is by magic bytes rather than `Content-Type` or the `.gz`
/// extension, because servers get both wrong routinely.
pub fn decompress_if_needed(bytes: &[u8]) -> Result<Vec<u8>, SitemapError> {
    if bytes.len() >= 2 && bytes[0] == 0x1f && bytes[1] == 0x8b {
        let mut out = Vec::new();
        GzDecoder::new(bytes)
            .take(MAX_DECOMPRESSED_BYTES as u64)
            .read_to_end(&mut out)
            .map_err(|e| SitemapError::Gzip(e.to_string()))?;
        Ok(out)
    } else {
        Ok(bytes.to_vec())
    }
}

/// Parse a sitemap body, transparently handling gzip.
pub fn parse(bytes: &[u8]) -> Result<Sitemap, SitemapError> {
    let bytes = decompress_if_needed(bytes)?;
    parse_xml(&bytes)
}

/// The element we are currently collecting text for.
#[derive(Clone, Copy, PartialEq)]
enum Field {
    None,
    Loc,
    LastMod,
    ChangeFreq,
    Priority,
}

fn parse_xml(bytes: &[u8]) -> Result<Sitemap, SitemapError> {
    let text = String::from_utf8_lossy(bytes);
    let mut reader = Reader::from_str(&text);
    {
        let config = reader.config_mut();
        config.trim_text_start = true;
        config.trim_text_end = true;
        // Real sitemaps are often truncated or have mismatched tags; take what
        // we can read rather than failing the whole file.
        config.check_end_names = false;
        config.allow_unmatched_ends = true;
    }

    let mut entries: Vec<SitemapEntry> = Vec::new();
    let mut sitemaps: Vec<String> = Vec::new();
    // `None` until we meet <urlset> or <sitemapindex>.
    let mut is_index: Option<bool> = None;
    let mut field = Field::None;
    // The entry being built, flushed on </url> or </sitemap>.
    let mut current: Option<SitemapEntry> = None;

    loop {
        match reader.read_event() {
            Err(e) => return Err(SitemapError::Xml(e.to_string())),
            Ok(Event::Eof) => break,
            Ok(Event::Start(start)) => {
                // Ignore namespace prefixes: `<sm:urlset>` is still a urlset.
                let name = local_name(start.name().as_ref());
                match name.as_str() {
                    "urlset" => is_index = Some(false),
                    "sitemapindex" => is_index = Some(true),
                    "url" | "sitemap" => {
                        current = Some(SitemapEntry {
                            loc: String::new(),
                            lastmod: None,
                            changefreq: None,
                            priority: None,
                        });
                    }
                    "loc" => field = Field::Loc,
                    "lastmod" => field = Field::LastMod,
                    "changefreq" => field = Field::ChangeFreq,
                    "priority" => field = Field::Priority,
                    _ => {}
                }
            }
            Ok(Event::Text(text)) if field != Field::None => {
                let raw = text.decode().unwrap_or_default();
                let value = quick_xml::escape::unescape(&raw)
                    .map(|v| v.trim().to_string())
                    .unwrap_or_else(|_| raw.trim().to_string());
                if let Some(entry) = current.as_mut() {
                    apply_field(entry, field, &value);
                }
            }
            // quick-xml reports entity references as their own event, so
            // `<loc>a?x=1&amp;y=2</loc>` arrives as text, ref, text.
            Ok(Event::GeneralRef(entity)) if field != Field::None => {
                let resolved = match entity.resolve_char_ref() {
                    Ok(Some(ch)) => Some(ch),
                    _ => entity
                        .decode()
                        .ok()
                        .and_then(|name| predefined_entity(name.as_ref())),
                };
                if let (Some(ch), Some(entry)) = (resolved, current.as_mut()) {
                    apply_field(entry, field, &ch.to_string());
                }
            }
            // CDATA is literal: no entity unescaping.
            Ok(Event::CData(cdata)) if field != Field::None => {
                let value = String::from_utf8_lossy(&cdata.into_inner())
                    .trim()
                    .to_string();
                if let Some(entry) = current.as_mut() {
                    apply_field(entry, field, &value);
                }
            }
            Ok(Event::End(end)) => {
                let name = local_name(end.name().as_ref());
                match name.as_str() {
                    "loc" | "lastmod" | "changefreq" | "priority" => field = Field::None,
                    "url" | "sitemap" => {
                        if let Some(entry) = current.take() {
                            if entry.loc.is_empty() {
                                continue;
                            }
                            let index = is_index.unwrap_or(name == "sitemap");
                            if index {
                                if sitemaps.len() < MAX_ENTRIES {
                                    sitemaps.push(entry.loc);
                                }
                            } else if entries.len() < MAX_ENTRIES {
                                entries.push(entry);
                            }
                        }
                    }
                    _ => {}
                }
            }
            Ok(_) => {}
        }
        if entries.len() >= MAX_ENTRIES && sitemaps.len() >= MAX_ENTRIES {
            break;
        }
    }

    match is_index {
        Some(true) => Ok(Sitemap::Index { sitemaps }),
        Some(false) => Ok(Sitemap::UrlSet { entries }),
        None => Err(SitemapError::NotASitemap),
    }
}

/// The five entities XML defines without a DTD.
fn predefined_entity(name: &str) -> Option<char> {
    Some(match name {
        "amp" => '&',
        "lt" => '<',
        "gt" => '>',
        "quot" => '"',
        "apos" => '\'',
        _ => return None,
    })
}

fn apply_field(entry: &mut SitemapEntry, field: Field, value: &str) {
    match field {
        Field::Loc => entry.loc.push_str(value),
        Field::LastMod => entry.lastmod = Some(value.to_string()),
        Field::ChangeFreq => entry.changefreq = Some(value.to_ascii_lowercase()),
        Field::Priority => entry.priority = value.parse().ok(),
        Field::None => {}
    }
}

/// Strip any namespace prefix: `sm:loc` -> `loc`.
fn local_name(raw: &[u8]) -> String {
    let name = String::from_utf8_lossy(raw);
    name.rsplit(':')
        .next()
        .unwrap_or(&name)
        .to_ascii_lowercase()
}

/// Parse a plain-text sitemap: one URL per line, as `robots.txt` sometimes
/// points at. Lines that are not http(s) URLs are ignored.
pub fn parse_text_list(body: &str) -> Vec<String> {
    body.lines()
        .map(str::trim)
        .filter(|l| l.starts_with("http://") || l.starts_with("https://"))
        .take(MAX_ENTRIES)
        .map(str::to_string)
        .collect()
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use super::*;

    const URLSET: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">
  <url>
    <loc>https://acme.test/</loc>
    <lastmod>2026-01-15</lastmod>
    <changefreq>Daily</changefreq>
    <priority>1.0</priority>
  </url>
  <url><loc>https://acme.test/about</loc></url>
  <url><loc>https://acme.test/q?a=1&amp;b=2</loc></url>
  <url><!-- no loc, must be skipped --><lastmod>2026-01-01</lastmod></url>
</urlset>"#;

    #[test]
    fn parses_a_urlset() {
        let Sitemap::UrlSet { entries } = parse(URLSET.as_bytes()).unwrap() else {
            panic!("expected a urlset");
        };
        assert_eq!(entries.len(), 3, "the entry without a <loc> is dropped");
        assert_eq!(entries[0].loc, "https://acme.test/");
        assert_eq!(entries[0].lastmod.as_deref(), Some("2026-01-15"));
        assert_eq!(entries[0].changefreq.as_deref(), Some("daily"));
        assert_eq!(entries[0].priority, Some(1.0));
        assert_eq!(entries[1].loc, "https://acme.test/about");
        assert_eq!(
            entries[2].loc, "https://acme.test/q?a=1&b=2",
            "entities are unescaped"
        );
    }

    #[test]
    fn parses_a_sitemap_index() {
        let xml = r#"<?xml version="1.0"?>
<sitemapindex xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">
  <sitemap><loc>https://acme.test/sitemap-pages.xml</loc><lastmod>2026-02-01</lastmod></sitemap>
  <sitemap><loc>https://acme.test/sitemap-posts.xml.gz</loc></sitemap>
</sitemapindex>"#;
        let Sitemap::Index { sitemaps } = parse(xml.as_bytes()).unwrap() else {
            panic!("expected an index");
        };
        assert_eq!(
            sitemaps,
            vec![
                "https://acme.test/sitemap-pages.xml",
                "https://acme.test/sitemap-posts.xml.gz"
            ]
        );
    }

    #[test]
    fn namespace_prefixes_are_ignored() {
        let xml = r#"<sm:urlset xmlns:sm="http://www.sitemaps.org/schemas/sitemap/0.9">
            <sm:url><sm:loc>https://acme.test/x</sm:loc></sm:url></sm:urlset>"#;
        assert_eq!(
            parse(xml.as_bytes()).unwrap().locations(),
            vec!["https://acme.test/x"]
        );
    }

    #[test]
    fn gzipped_sitemaps_are_detected_by_magic_bytes() {
        let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        encoder.write_all(URLSET.as_bytes()).unwrap();
        let gz = encoder.finish().unwrap();
        assert_eq!(&gz[..2], &[0x1f, 0x8b]);

        let sitemap = parse(&gz).unwrap();
        assert_eq!(sitemap.locations().len(), 3);
    }

    #[test]
    fn non_sitemap_xml_is_rejected() {
        assert!(matches!(
            parse(b"<rss><channel><item/></channel></rss>"),
            Err(SitemapError::NotASitemap)
        ));
        assert!(matches!(
            parse(b"this is not xml at all"),
            Err(SitemapError::NotASitemap)
        ));
    }

    #[test]
    fn truncated_xml_still_yields_what_was_parsed() {
        let xml =
            r#"<urlset><url><loc>https://acme.test/a</loc></url><url><loc>https://acme.test/b"#;
        let sitemap = parse(xml.as_bytes()).unwrap();
        assert_eq!(sitemap.locations(), vec!["https://acme.test/a"]);
    }

    #[test]
    fn plain_text_url_lists() {
        let body = "https://acme.test/a\n  https://acme.test/b  \n# a comment\nnot-a-url\n";
        assert_eq!(
            parse_text_list(body),
            vec!["https://acme.test/a", "https://acme.test/b"]
        );
    }

    #[test]
    fn entry_count_is_bounded() {
        let mut xml = String::from("<urlset>");
        for i in 0..(MAX_ENTRIES + 500) {
            xml.push_str(&format!("<url><loc>https://acme.test/{i}</loc></url>"));
        }
        xml.push_str("</urlset>");
        assert_eq!(
            parse(xml.as_bytes()).unwrap().locations().len(),
            MAX_ENTRIES
        );
    }
}
