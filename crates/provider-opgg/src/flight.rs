//! Parser for the Next.js "flight" (React Server Component) payloads embedded
//! in op.gg's server-rendered HTML.
//!
//! op.gg ships no public JSON API: a build/counters page is fully
//! server-rendered, and the data a client component needs to hydrate is
//! inlined as `self.__next_f.push([1,"<hex-id>:<json>"])` script chunks.
//! Fetching the page's HTML and re-parsing those chunks recovers the same
//! data a browser would use, without running any JS.
//!
//! Two shapes show up in practice:
//! * A few sections (runes, counters) pass their data cleanly as a `"data"`
//!   prop — a plain JSON value with sane field names. [`find_data_field`]
//!   digs one out by key.
//! * Everything else (items, skill order, summoner spells) is rendered
//!   straight to React elements (`["$", tag, key, props]` tuples) with no
//!   separate data prop; the interesting values only exist as `metaId`
//!   attributes on icon components. [`collect_meta_nodes`] walks the element
//!   tree and pulls those out, tagged with the nearest ancestor element `key`
//!   (op.gg's own row/section id, e.g. `"core_items_0"`) so callers can group
//!   them back into rows.

use serde::de::DeserializeOwned;
use serde::Deserialize;
use serde_json::Value;

const PUSH_MARKER: &str = "self.__next_f.push([1,";

/// Extract and JSON-decode every flight chunk in `html`. Chunks that aren't
/// `<hex-id>:<json>` (module references, bare strings, `null`, …) are
/// silently skipped — every page has plenty of those alongside the ones that
/// matter.
pub fn extract_flight_chunks(html: &str) -> Vec<Value> {
    let mut chunks = Vec::new();
    let mut cursor = 0;
    while let Some(rel) = html[cursor..].find(PUSH_MARKER) {
        let quote_start = cursor + rel + PUSH_MARKER.len();
        cursor = quote_start;
        let mut de = serde_json::Deserializer::from_str(&html[quote_start..]);
        let Ok(raw) = String::deserialize(&mut de) else {
            continue;
        };
        let payload = match raw.split_once(':') {
            Some((id, rest)) if !id.is_empty() && id.chars().all(|c| c.is_ascii_hexdigit()) => rest,
            _ => raw.as_str(),
        }
        .trim_start();
        if !payload.starts_with(['{', '[', '"']) {
            continue;
        }
        if let Ok(value) = serde_json::from_str(payload) {
            chunks.push(value);
        }
    }
    chunks
}

/// Find every value keyed `key` anywhere in `chunks` and return the first one
/// that deserializes as `T`. A generic key like `"data"` can appear more than
/// once with unrelated shapes (a rune section's `data` is an object, a
/// counters section's is a list), so every occurrence is tried rather than
/// just the first match.
pub fn find_data_field<T: DeserializeOwned>(chunks: &[Value], key: &str) -> Option<T> {
    chunks
        .iter()
        .flat_map(|chunk| {
            let mut hits = Vec::new();
            collect_key(chunk, key, &mut hits);
            hits
        })
        .find_map(|value| serde_json::from_value(value.clone()).ok())
}

fn collect_key<'a>(value: &'a Value, key: &str, out: &mut Vec<&'a Value>) {
    match value {
        Value::Object(map) => {
            if let Some(v) = map.get(key) {
                out.push(v);
            }
            for v in map.values() {
                collect_key(v, key, out);
            }
        }
        Value::Array(items) => {
            for v in items {
                collect_key(v, key, out);
            }
        }
        _ => {}
    }
}

/// A `metaType`-tagged leaf pulled out of a rendered React element tree
/// (`{"metaType":"item","metaId":3161}` for an item icon, `{"metaType":
/// "spell","metaId":4}` for a summoner-spell icon, …).
#[derive(Debug, Clone)]
pub struct MetaNode {
    /// Every ancestor React element `key` on the way down to this node,
    /// outermost first. op.gg keys individual icons (`"3161-0"`) as well as
    /// their enclosing row (`"core_items_0"`), so callers that want the row
    /// id need to search the whole path rather than assume its position.
    pub section_path: Vec<String>,
    pub meta_type: String,
    pub meta_id: Value,
}

/// Walk every chunk's element tree and collect every `metaType`-tagged node,
/// in document order.
pub fn collect_meta_nodes(chunks: &[Value]) -> Vec<MetaNode> {
    let mut out = Vec::new();
    for chunk in chunks {
        let mut path = Vec::new();
        walk_elements(chunk, &mut path, &mut out);
    }
    out
}

/// React server-element serialization is `["$", tag, key, props]`; `props`
/// nests further elements as ordinary object/array values (usually under a
/// `"children"` key, but we don't rely on that — we just recurse into
/// everything). `path` accumulates every keyed ancestor seen so far and is
/// popped back on the way out, so siblings don't see each other's keys.
fn walk_elements(value: &Value, path: &mut Vec<String>, out: &mut Vec<MetaNode>) {
    let Value::Array(items) = value else {
        if let Value::Object(map) = value {
            for v in map.values() {
                walk_elements(v, path, out);
            }
        }
        return;
    };
    if items.len() == 4 && items[0] == Value::String("$".to_string()) {
        let pushed = items[2].as_str().is_some_and(|key| {
            path.push(key.to_string());
            true
        });
        if let Value::Object(props) = &items[3] {
            if let Some(meta_type) = props.get("metaType").and_then(Value::as_str) {
                out.push(MetaNode {
                    section_path: path.clone(),
                    meta_type: meta_type.to_string(),
                    meta_id: props.get("metaId").cloned().unwrap_or(Value::Null),
                });
            }
            for v in props.values() {
                walk_elements(v, path, out);
            }
        }
        if pushed {
            path.pop();
        }
        return;
    }
    for v in items {
        walk_elements(v, path, out);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_id_prefixed_chunk() {
        let html = r#"<script>self.__next_f.push([1,"3a:{\"foo\":1}\n"])</script>"#;
        let chunks = extract_flight_chunks(html);
        assert_eq!(chunks, vec![serde_json::json!({"foo": 1})]);
    }

    #[test]
    fn skips_non_json_chunks() {
        let html = r#"<script>self.__next_f.push([1,"12:I[80622,[],\"IconMark\"]\n"])</script>"#;
        assert!(extract_flight_chunks(html).is_empty());
    }

    #[test]
    fn finds_multiple_chunks_in_order() {
        let html = concat!(
            r#"<script>self.__next_f.push([1,"1:{\"a\":1}"])</script>"#,
            r#"<script>self.__next_f.push([1,"2:{\"b\":2}"])</script>"#,
        );
        let chunks = extract_flight_chunks(html);
        assert_eq!(
            chunks,
            vec![serde_json::json!({"a":1}), serde_json::json!({"b":2})]
        );
    }

    #[test]
    fn find_data_field_digs_through_nesting() {
        let chunks = vec![serde_json::json!({
            "unrelated": ["$", "div", null, {"children": [
                {"data": {"rune_pages": [{"id": 1}]}}
            ]}]
        })];
        let found: Option<Vec<serde_json::Value>> = find_data_field(&chunks, "rune_pages");
        assert_eq!(found, Some(vec![serde_json::json!({"id": 1})]));
    }

    #[test]
    fn find_data_field_tries_every_occurrence_of_an_ambiguous_key() {
        // "data" shows up twice with unrelated shapes; the second (list) one
        // is what a `Vec<i64>` caller wants, and the object one must not
        // short-circuit the search.
        let chunks = vec![serde_json::json!({
            "runeSection": {"data": {"rune_pages": []}},
            "counterSection": {"data": [1, 2, 3]}
        })];
        let found: Option<Vec<i64>> = find_data_field(&chunks, "data");
        assert_eq!(found, Some(vec![1, 2, 3]));
    }

    #[test]
    fn collect_meta_nodes_records_the_full_keyed_ancestor_path() {
        let chunk = serde_json::json!([
            "$", "tr", "core_items_0", {
                "children": [
                    ["$", "div", "3161-0", {
                        "metaType": "item",
                        "metaId": 3161
                    }]
                ]
            }
        ]);
        let nodes = collect_meta_nodes(&[chunk]);
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].section_path, vec!["core_items_0", "3161-0"]);
        assert_eq!(nodes[0].meta_type, "item");
        assert_eq!(nodes[0].meta_id, serde_json::json!(3161));
    }
}
