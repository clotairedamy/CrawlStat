//! Content hashing for duplicate detection.
//!
//! Two complementary hashes are stored per page:
//!
//! * [`content_hash`] — a plain BLAKE3 digest. Equal digests mean byte-identical
//!   content, which is how exact duplicates are found with a `GROUP BY`.
//! * [`simhash`] — a 64-bit locality-sensitive hash over word shingles. Pages
//!   that differ only in a sidebar or a date land within a few bits of each
//!   other, so near-duplicates are found by comparing [`hamming_distance`]
//!   against a threshold (the Phase 2 audit uses 3).

/// Hex BLAKE3 digest of arbitrary bytes.
pub fn content_hash(bytes: &[u8]) -> String {
    blake3::hash(bytes).to_hex().to_string()
}

/// Hex BLAKE3 digest of text, after collapsing whitespace.
///
/// Whitespace collapsing means a page reformatted by a template change still
/// counts as the same text.
pub fn text_hash(text: &str) -> String {
    let normalized = normalize_text(text);
    content_hash(normalized.as_bytes())
}

/// Lowercase the text and collapse every run of whitespace to one space.
pub fn normalize_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut last_was_space = true; // suppresses a leading space
    for ch in text.chars() {
        if ch.is_whitespace() {
            if !last_was_space {
                out.push(' ');
                last_was_space = true;
            }
        } else {
            out.extend(ch.to_lowercase());
            last_was_space = false;
        }
    }
    if out.ends_with(' ') {
        out.pop();
    }
    out
}

/// Number of whitespace-separated words in the text.
pub fn word_count(text: &str) -> u32 {
    text.split_whitespace()
        .filter(|w| w.chars().any(char::is_alphanumeric))
        .count()
        .min(u32::MAX as usize) as u32
}

/// Words per shingle. Three is the usual compromise: long enough that common
/// phrases do not collide, short enough that short pages still produce several.
const SHINGLE_LEN: usize = 3;

/// 64-bit simhash over the text's word shingles.
///
/// Returns `None` for text with no words at all, where the result would be
/// meaningless (and every empty page would look like a duplicate of every
/// other).
pub fn simhash(text: &str) -> Option<u64> {
    let normalized = normalize_text(text);
    let words: Vec<&str> = normalized.split(' ').filter(|w| !w.is_empty()).collect();
    if words.is_empty() {
        return None;
    }

    // Accumulate one signed counter per output bit.
    let mut bits = [0i32; 64];
    let mut shingles = 0u32;

    // Short texts shingle over whatever they have rather than producing nothing.
    let len = SHINGLE_LEN.min(words.len());
    for window in words.windows(len) {
        let hash = hash_shingle(window);
        shingles += 1;
        for (bit, counter) in bits.iter_mut().enumerate() {
            if hash >> bit & 1 == 1 {
                *counter += 1;
            } else {
                *counter -= 1;
            }
        }
    }

    if shingles == 0 {
        return None;
    }

    let mut out = 0u64;
    for (bit, counter) in bits.iter().enumerate() {
        if *counter > 0 {
            out |= 1 << bit;
        }
    }
    Some(out)
}

fn hash_shingle(words: &[&str]) -> u64 {
    let mut hasher = blake3::Hasher::new();
    for (i, word) in words.iter().enumerate() {
        if i > 0 {
            hasher.update(b" ");
        }
        hasher.update(word.as_bytes());
    }
    let digest = hasher.finalize();
    let mut bytes = [0u8; 8];
    bytes.copy_from_slice(&digest.as_bytes()[..8]);
    u64::from_le_bytes(bytes)
}

/// Bits that differ between two simhashes: 0 is identical, 64 is opposite.
pub fn hamming_distance(a: u64, b: u64) -> u32 {
    (a ^ b).count_ones()
}

/// Share of bits two simhashes agree on, in `0.0..=1.0`.
///
/// This is what the UI shows as a similarity percentage.
pub fn similarity(a: u64, b: u64) -> f32 {
    1.0 - hamming_distance(a, b) as f32 / 64.0
}

/// Default near-duplicate threshold, in bits of [`hamming_distance`].
///
/// Six bits out of 64 is roughly 90% similarity, which is the convention other
/// SEO tools use.
///
/// **Length caveat.** Simhash distance grows as documents get shorter, because
/// each shingle carries proportionally more weight. Appending one sentence to a
/// 1,000-word page moves ~2 bits; appending it to a 50-word page can move ~11.
/// Thin pages are therefore better compared by [`text_hash`] equality, and the
/// Phase 2 near-duplicate audit only applies this threshold above a minimum
/// word count.
pub const NEAR_DUPLICATE_THRESHOLD: u32 = 6;

/// Pages shorter than this are not compared by simhash — see
/// [`NEAR_DUPLICATE_THRESHOLD`].
pub const MIN_WORDS_FOR_SIMHASH_COMPARISON: u32 = 200;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_bytes_hash_identically() {
        assert_eq!(content_hash(b"hello"), content_hash(b"hello"));
        assert_ne!(content_hash(b"hello"), content_hash(b"hellp"));
        assert_eq!(content_hash(b"hello").len(), 64);
    }

    #[test]
    fn text_hash_ignores_formatting_and_case() {
        assert_eq!(text_hash("Hello   World\n"), text_hash("hello world"));
        assert_ne!(text_hash("hello world"), text_hash("hello worlds"));
    }

    #[test]
    fn word_count_skips_punctuation_only_tokens() {
        assert_eq!(word_count("one two three"), 3);
        assert_eq!(word_count("  spaced \n out \t words "), 3);
        assert_eq!(word_count("hello — world"), 2);
        assert_eq!(word_count(""), 0);
    }

    #[test]
    fn simhash_is_none_for_empty_text() {
        assert_eq!(simhash(""), None);
        assert_eq!(simhash("   \n  "), None);
    }

    #[test]
    fn simhash_is_stable_and_order_sensitive() {
        let a = simhash("the quick brown fox jumps over the lazy dog").unwrap();
        assert_eq!(
            a,
            simhash("The  QUICK brown fox jumps over the lazy dog").unwrap()
        );
        let shuffled = simhash("dog lazy the over jumps fox brown quick the").unwrap();
        assert!(hamming_distance(a, shuffled) > NEAR_DUPLICATE_THRESHOLD);
    }

    /// A page of roughly the length the near-duplicate audit applies to.
    fn long_page() -> String {
        let para = "Our returns policy allows you to send any item back within thirty days of \
                    delivery for a full refund provided the item is unused and in its original \
                    packaging. Contact support to start a return and we will email a prepaid \
                    label. Refunds are processed within five working days of receipt.";
        [para; 4].join(" ")
    }

    #[test]
    fn near_duplicates_are_within_the_threshold() {
        let base = long_page();
        // Only a trailing sentence differs — a classic template near-duplicate.
        let variant = format!("{base} Last updated in March.");
        let d = hamming_distance(simhash(&base).unwrap(), simhash(&variant).unwrap());
        assert!(
            d <= NEAR_DUPLICATE_THRESHOLD,
            "near-duplicate scored {d} bits apart"
        );
        assert!(similarity(simhash(&base).unwrap(), simhash(&variant).unwrap()) > 0.9);
    }

    #[test]
    fn differing_boilerplate_alone_does_not_separate_pages() {
        let body = long_page();
        let a = format!("Home Shop About {body} Privacy Terms");
        let b = format!("Home Shop About {body} Privacy Terms Copyright 2026");
        assert!(
            hamming_distance(simhash(&a).unwrap(), simhash(&b).unwrap())
                <= NEAR_DUPLICATE_THRESHOLD
        );
    }

    #[test]
    fn short_pages_drift_beyond_the_threshold() {
        // Documents the length caveat on NEAR_DUPLICATE_THRESHOLD: the same
        // four-word edit that moves ~2 bits on a long page moves far more on a
        // thin one, which is why the audit requires
        // MIN_WORDS_FOR_SIMHASH_COMPARISON words.
        let short = "Our returns policy allows you to send any item back within thirty days.";
        let variant = format!("{short} Last updated in March.");
        let d = hamming_distance(simhash(short).unwrap(), simhash(&variant).unwrap());
        assert!(
            d > NEAR_DUPLICATE_THRESHOLD,
            "expected short-page drift, got {d}"
        );
        assert!(word_count(short) < MIN_WORDS_FOR_SIMHASH_COMPARISON);
    }

    #[test]
    fn unrelated_pages_are_far_apart() {
        let a = "Our returns policy allows you to send any item back within thirty days of \
                 delivery for a full refund provided the item is unused.";
        let b = "The annual general meeting will be held in the main auditorium where the board \
                 presents the audited financial statements for the year.";
        let d = hamming_distance(simhash(a).unwrap(), simhash(b).unwrap());
        assert!(d > 10, "unrelated pages only {d} bits apart");
        assert!(similarity(simhash(a).unwrap(), simhash(b).unwrap()) < 0.85);
    }

    #[test]
    fn short_text_still_produces_a_hash() {
        assert!(simhash("hello").is_some());
        assert!(simhash("hello world").is_some());
    }
}
