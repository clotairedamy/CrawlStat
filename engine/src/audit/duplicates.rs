//! Near-duplicate detection by simhash.
//!
//! Exact duplicates are a `GROUP BY text_hash` in the catalogue. Near-duplicates
//! need pairwise comparison, which naively is quadratic. The standard fix is
//! *banding*: split each 64-bit simhash into four 16-bit bands and only compare
//! pages that match exactly on at least one band. Two hashes within three bits
//! of each other must agree on at least one of four bands, so nothing within
//! the threshold is missed.
//!
//! Memory is one `(id, simhash)` pair per page — sixteen bytes — and the
//! partner URLs are looked up only for pages that actually matched.

use std::collections::HashMap;

use rusqlite::Connection;

use crate::error::Result;
use crate::hash;

/// Bands the 64-bit hash is split into.
const BANDS: usize = 4;
const BAND_BITS: u32 = 16;

/// Comparisons allowed per page.
///
/// A site with thousands of pages sharing a band — boilerplate-heavy templates
/// do this — would otherwise degenerate to a quadratic scan. The cap means a
/// page in a huge bucket is compared against the first few hundred candidates
/// rather than all of them, which is a reasonable trade: anything in that
/// bucket is near-duplicate with something, so the issue is still reported.
const MAX_COMPARISONS_PER_PAGE: usize = 500;

/// Pages that are similar to another page but not identical to it.
pub fn near_duplicates(
    conn: &Connection,
    threshold_bits: u32,
    min_words: u32,
) -> Result<Vec<(i64, Option<String>)>> {
    // Every page long enough to compare is a *candidate*, including pages that
    // are exact duplicates of something: a third page that nearly matches a
    // duplicated pair is still worth knowing about.
    let mut stmt = conn.prepare(
        "SELECT id, text_simhash FROM indexable_pages
         WHERE text_simhash IS NOT NULL AND COALESCE(word_count, 0) >= ?1",
    )?;
    let pages: Vec<(i64, u64)> = stmt
        .query_map([min_words as i64], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)? as u64))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    // Pages that are byte-for-byte duplicates of another page are *reported*
    // by the exact-duplicate audit instead, so they are excluded from the
    // findings below — but not from the comparison above.
    let mut stmt = conn.prepare(
        "SELECT id FROM indexable_pages
         WHERE text_hash IS NOT NULL
           AND text_hash IN (
               SELECT text_hash FROM indexable_pages
               WHERE text_hash IS NOT NULL
               GROUP BY text_hash HAVING count(*) > 1
           )",
    )?;
    let exact_duplicates: std::collections::HashSet<i64> = stmt
        .query_map([], |row| row.get::<_, i64>(0))?
        .collect::<rusqlite::Result<_>>()?;

    if pages.len() < 2 {
        return Ok(Vec::new());
    }

    // Bucket by each band, so candidates are found without comparing everything.
    let mut buckets: HashMap<(usize, u16), Vec<usize>> = HashMap::new();
    for (index, (_, simhash)) in pages.iter().enumerate() {
        for band in 0..BANDS {
            let value = (simhash >> (band as u32 * BAND_BITS)) as u16;
            buckets.entry((band, value)).or_default().push(index);
        }
    }

    // Best match per page: (distance, partner index).
    let mut best: HashMap<usize, (u32, usize)> = HashMap::new();
    let mut compared: HashMap<usize, usize> = HashMap::new();

    for candidates in buckets.values() {
        if candidates.len() < 2 {
            continue;
        }
        for (position, &a) in candidates.iter().enumerate() {
            for &b in candidates.iter().skip(position + 1) {
                let budget = compared.entry(a).or_insert(0);
                if *budget >= MAX_COMPARISONS_PER_PAGE {
                    break;
                }
                *budget += 1;
                *compared.entry(b).or_insert(0) += 1;

                let distance = hash::hamming_distance(pages[a].1, pages[b].1);
                if distance > threshold_bits {
                    continue;
                }
                for (one, other) in [(a, b), (b, a)] {
                    let entry = best.entry(one).or_insert((u32::MAX, other));
                    if distance < entry.0 {
                        *entry = (distance, other);
                    }
                }
            }
        }
    }

    if best.is_empty() {
        return Ok(Vec::new());
    }

    // Look up the partner URLs, for matched pages only.
    let mut url_of = conn.prepare("SELECT url FROM urls WHERE id = ?1")?;
    let mut findings: Vec<(i64, Option<String>)> = Vec::with_capacity(best.len());
    for (index, (distance, partner)) in best {
        if exact_duplicates.contains(&pages[index].0) {
            continue;
        }
        let partner_url: String = url_of
            .query_row([pages[partner].0], |row| row.get(0))
            .unwrap_or_else(|_| "another page".to_string());
        let similarity = hash::similarity(pages[index].1, pages[partner].1) * 100.0;
        findings.push((
            pages[index].0,
            Some(format!(
                "{similarity:.0}% similar to {partner_url} ({distance} bits)"
            )),
        ));
    }
    // Stable order, so two runs of the same crawl report identically.
    findings.sort_by_key(|(url_id, _)| *url_id);
    Ok(findings)
}
