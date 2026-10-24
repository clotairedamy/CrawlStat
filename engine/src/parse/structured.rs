//! Structured-data extraction: JSON-LD and microdata types.
//!
//! We record *which* schema.org types a page declares, not the full payload.
//! That is what the audits and the UI's "Structured data" tab need, and it
//! keeps the database small. A page declaring `Product` with a nested `Offer`
//! and `AggregateRating` reports all three.

use serde_json::Value;

use crate::model::StructuredDataBlock;

/// Guard against hostile or generated payloads with absurd nesting.
const MAX_DEPTH: usize = 24;
/// Guard against a page declaring thousands of types.
const MAX_TYPES: usize = 200;

/// Parse one `<script type="application/ld+json">` body.
///
/// Invalid JSON is still recorded — "JSON-LD that does not parse" is itself a
/// finding worth surfacing, and silently dropping it would hide the problem.
pub fn parse_json_ld(raw: &str) -> StructuredDataBlock {
    match serde_json::from_str::<Value>(raw) {
        Ok(value) => {
            let mut types = Vec::new();
            collect_types(&value, 0, &mut types);
            dedup(&mut types);
            StructuredDataBlock {
                format: "json-ld".into(),
                types,
                valid: true,
                error: None,
            }
        }
        Err(err) => StructuredDataBlock {
            format: "json-ld".into(),
            types: Vec::new(),
            valid: false,
            error: Some(err.to_string()),
        },
    }
}

/// Walk any JSON shape and collect every `@type` value found.
fn collect_types(value: &Value, depth: usize, out: &mut Vec<String>) {
    if depth > MAX_DEPTH || out.len() >= MAX_TYPES {
        return;
    }
    match value {
        Value::Array(items) => {
            for item in items {
                collect_types(item, depth + 1, out);
            }
        }
        Value::Object(map) => {
            if let Some(ty) = map.get("@type") {
                push_type(ty, out);
            }
            for (key, child) in map {
                // `@context` is a vocabulary URL, never a type.
                if key == "@context" || key == "@type" {
                    continue;
                }
                collect_types(child, depth + 1, out);
            }
        }
        _ => {}
    }
}

fn push_type(value: &Value, out: &mut Vec<String>) {
    match value {
        Value::String(s) => {
            let name = short_type_name(s);
            if !name.is_empty() && out.len() < MAX_TYPES {
                out.push(name);
            }
        }
        Value::Array(items) => {
            for item in items {
                push_type(item, out);
            }
        }
        _ => {}
    }
}

/// `https://schema.org/Product` and `schema:Product` both become `Product`.
pub fn short_type_name(raw: &str) -> String {
    let raw = raw.trim();
    let after_slash = raw.rsplit(['/', '#']).next().unwrap_or(raw);
    let after_colon = after_slash.rsplit(':').next().unwrap_or(after_slash);
    after_colon.trim().to_string()
}

/// Build a microdata block from the `itemtype` values found on a page.
pub fn microdata_block(itemtypes: &[String]) -> Option<StructuredDataBlock> {
    let mut types: Vec<String> = itemtypes
        .iter()
        .map(|t| short_type_name(t))
        .filter(|t| !t.is_empty())
        .collect();
    if types.is_empty() {
        return None;
    }
    dedup(&mut types);
    Some(StructuredDataBlock {
        format: "microdata".into(),
        types,
        valid: true,
        error: None,
    })
}

fn dedup(types: &mut Vec<String>) {
    types.sort();
    types.dedup();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simple_type() {
        let block = parse_json_ld(r#"{"@context":"https://schema.org","@type":"Article"}"#);
        assert!(block.valid);
        assert_eq!(block.types, vec!["Article"]);
    }

    #[test]
    fn nested_and_graph_types_are_all_collected() {
        let block = parse_json_ld(
            r#"{"@context":"https://schema.org","@graph":[
                 {"@type":"Product","offers":{"@type":"Offer","price":"9.99"},
                  "aggregateRating":{"@type":"AggregateRating","ratingValue":4.5}},
                 {"@type":"BreadcrumbList"}]}"#,
        );
        assert_eq!(
            block.types,
            vec!["AggregateRating", "BreadcrumbList", "Offer", "Product"]
        );
    }

    #[test]
    fn array_of_types_and_full_urls() {
        let block = parse_json_ld(
            r#"[{"@type":["Organization","LocalBusiness"]},
                {"@type":"https://schema.org/WebSite"},
                {"@type":"schema:Person"}]"#,
        );
        assert_eq!(
            block.types,
            vec!["LocalBusiness", "Organization", "Person", "WebSite"]
        );
    }

    #[test]
    fn context_is_never_mistaken_for_a_type() {
        let block = parse_json_ld(r#"{"@context":{"@vocab":"https://schema.org/Thing"}}"#);
        assert!(block.types.is_empty());
    }

    #[test]
    fn invalid_json_is_recorded_rather_than_dropped() {
        let block = parse_json_ld("{ not json");
        assert!(!block.valid);
        assert!(block.error.is_some());
        assert!(block.types.is_empty());
    }

    #[test]
    fn microdata_types_are_shortened_and_deduped() {
        let block = microdata_block(&[
            "https://schema.org/Product".into(),
            "http://schema.org/Product".into(),
            "https://schema.org/Offer".into(),
        ])
        .unwrap();
        assert_eq!(block.types, vec!["Offer", "Product"]);
        assert_eq!(block.format, "microdata");
        assert!(microdata_block(&[]).is_none());
    }

    #[test]
    fn deep_payloads_are_bounded() {
        // 100 levels of nesting, with the type only at the very bottom.
        let mut json = String::new();
        for _ in 0..100 {
            json.push_str(r#"{"a":"#);
        }
        json.push_str(r#"{"@type":"Deep"}"#);
        for _ in 0..100 {
            json.push('}');
        }
        let block = parse_json_ld(&json);
        assert!(block.valid);
        assert!(block.types.is_empty(), "should stop before MAX_DEPTH");
    }
}
