//! Streaming Wikidata JSON dump parser.
//!
//! Parses Wikidata JSON dump line by line (each line = one entity).
//! Filters to useful properties and languages.

use lokb_core::Result;
use serde::Deserialize;
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read};

/// A parsed Wikidata entity with filtered properties.
#[derive(Debug, Clone)]
pub struct WikidataEntity {
    pub qid: String,
    pub labels: HashMap<String, String>,
    pub description: Option<String>,
    pub entity_type: Option<String>,
    pub properties: Vec<WikidataProperty>,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
}

/// A filtered property (predicate → value).
#[derive(Debug, Clone)]
pub struct WikidataProperty {
    pub property: String,
    pub property_label: &'static str,
    pub value: WikidataValue,
}

#[derive(Debug, Clone)]
pub enum WikidataValue {
    EntityRef(String), // Q-id
    StringVal(String),
    Quantity(f64),
    Coordinates { lat: f64, lon: f64 },
}

/// Properties we keep from Wikidata.
const INTERESTING_PROPERTIES: &[(&str, &str)] = &[
    ("P31", "instance_of"),
    ("P17", "country"),
    ("P36", "capital"),
    ("P1082", "population"),
    ("P625", "coordinate_location"),
    ("P6", "head_of_government"),
    ("P30", "continent"),
    ("P37", "official_language"),
    ("P35", "head_of_state"),
    ("P47", "shares_border_with"),
    ("P150", "contains"),
    ("P131", "located_in"),
    ("P495", "country_of_origin"),
    ("P571", "inception"),
    ("P18", "image"),
];

/// Languages to keep.
const KEEP_LANGUAGES: &[&str] = &["en", "ru", "de", "fr", "es", "zh", "ja"];

/// Configuration for Wikidata parsing.
#[derive(Default)]
pub struct WikidataConfig {
    /// Max entities to parse (0 = unlimited)
    pub limit: usize,
    /// Only parse entities with these types (P31 values). Empty = all.
    pub type_filter: Vec<String>,
}

/// Parse Wikidata JSON dump from a reader (can be streaming/decompressed).
/// Calls `callback` for each parsed entity.
pub fn parse_wikidata<R: Read>(
    reader: R,
    config: &WikidataConfig,
    mut callback: impl FnMut(WikidataEntity),
) -> Result<u64> {
    let buf_reader = BufReader::with_capacity(16 * 1024 * 1024, reader); // 16MB buffer
    let mut count = 0u64;
    let mut skipped = 0u64;

    for line_result in buf_reader.lines() {
        let line = line_result?;
        let trimmed = line.trim().trim_end_matches(',');

        // Skip array brackets
        if trimmed == "[" || trimmed == "]" || trimmed.is_empty() {
            continue;
        }

        // Parse JSON entity
        let raw: RawEntity = match serde_json::from_str(trimmed) {
            Ok(e) => e,
            Err(_) => {
                skipped += 1;
                continue;
            }
        };

        // Skip non-items (properties, lexemes)
        if raw.r#type != "item" {
            skipped += 1;
            continue;
        }

        // Extract filtered entity
        let entity = match extract_entity(&raw) {
            Some(e) => e,
            None => {
                skipped += 1;
                continue;
            }
        };

        // Type filter
        if !config.type_filter.is_empty() {
            if let Some(ref et) = entity.entity_type {
                if !config.type_filter.iter().any(|t| t == et) {
                    skipped += 1;
                    continue;
                }
            } else {
                skipped += 1;
                continue;
            }
        }

        callback(entity);
        count += 1;

        if config.limit > 0 && count >= config.limit as u64 {
            break;
        }

        if count.is_multiple_of(100_000) {
            eprintln!("Wikidata: {count} entities parsed, {skipped} skipped...");
        }
    }

    Ok(count)
}

fn extract_entity(raw: &RawEntity) -> Option<WikidataEntity> {
    let qid = raw.id.clone()?;

    // Extract labels for kept languages
    let mut labels = HashMap::new();
    if let Some(ref raw_labels) = raw.labels {
        for lang in KEEP_LANGUAGES {
            if let Some(label_obj) = raw_labels.get(*lang)
                && let Some(ref value) = label_obj.value
            {
                labels.insert(lang.to_string(), value.clone());
            }
        }
    }

    // Need at least an English label
    if !labels.contains_key("en") {
        return None;
    }

    // Description (English only)
    let description = raw
        .descriptions
        .as_ref()
        .and_then(|d| d.get("en"))
        .and_then(|d| d.value.clone());

    // Parse claims (properties)
    let mut properties = Vec::new();
    let mut entity_type = None;
    let mut latitude = None;
    let mut longitude = None;

    if let Some(ref claims) = raw.claims {
        for (prop_id, prop_label) in INTERESTING_PROPERTIES {
            if let Some(claim_list) = claims.get(*prop_id) {
                for claim in claim_list {
                    if let Some(ref mainsnak) = claim.mainsnak
                        && let Some(value) = extract_value(mainsnak)
                    {
                        if *prop_id == "P31"
                            && let WikidataValue::EntityRef(ref qid) = value
                        {
                            entity_type = Some(qid.clone());
                        }
                        if *prop_id == "P625"
                            && let WikidataValue::Coordinates { lat, lon } = &value
                        {
                            latitude = Some(*lat);
                            longitude = Some(*lon);
                        }
                        properties.push(WikidataProperty {
                            property: prop_id.to_string(),
                            property_label: prop_label,
                            value,
                        });
                        break;
                    }
                }
            }
        }
    }

    Some(WikidataEntity {
        qid,
        labels,
        description,
        entity_type,
        properties,
        latitude,
        longitude,
    })
}

fn extract_value(mainsnak: &RawMainsnak) -> Option<WikidataValue> {
    let datavalue = mainsnak.datavalue.as_ref()?;
    match datavalue.r#type.as_deref()? {
        "wikibase-entityid" => {
            let id = datavalue.value.as_ref()?.get("id")?.as_str()?.to_string();
            Some(WikidataValue::EntityRef(id))
        }
        "string" => {
            let s = datavalue.value.as_ref()?.as_str()?.to_string();
            Some(WikidataValue::StringVal(s))
        }
        "quantity" => {
            let amount_str = datavalue.value.as_ref()?.get("amount")?.as_str()?;
            let amount: f64 = amount_str.trim_start_matches('+').parse().ok()?;
            Some(WikidataValue::Quantity(amount))
        }
        "globecoordinate" => {
            let lat = datavalue.value.as_ref()?.get("latitude")?.as_f64()?;
            let lon = datavalue.value.as_ref()?.get("longitude")?.as_f64()?;
            Some(WikidataValue::Coordinates { lat, lon })
        }
        _ => None,
    }
}

// ── Raw JSON structures (serde) ──────────────────────────────────────

#[derive(Deserialize)]
struct RawEntity {
    id: Option<String>,
    r#type: String,
    labels: Option<HashMap<String, LangValue>>,
    descriptions: Option<HashMap<String, LangValue>>,
    claims: Option<HashMap<String, Vec<RawClaim>>>,
}

#[derive(Deserialize)]
struct LangValue {
    value: Option<String>,
}

#[derive(Deserialize)]
struct RawClaim {
    mainsnak: Option<RawMainsnak>,
}

#[derive(Deserialize)]
struct RawMainsnak {
    datavalue: Option<RawDatavalue>,
}

#[derive(Deserialize)]
struct RawDatavalue {
    r#type: Option<String>,
    value: Option<serde_json::Value>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_single_entity() {
        let json = r#"[
{"type":"item","id":"Q90","labels":{"en":{"language":"en","value":"Paris"},"ru":{"language":"ru","value":"Париж"}},"descriptions":{"en":{"language":"en","value":"capital of France"}},"claims":{"P31":[{"mainsnak":{"datavalue":{"type":"wikibase-entityid","value":{"id":"Q515"}}}}],"P17":[{"mainsnak":{"datavalue":{"type":"wikibase-entityid","value":{"id":"Q142"}}}}],"P1082":[{"mainsnak":{"datavalue":{"type":"quantity","value":{"amount":"+2102650"}}}}],"P625":[{"mainsnak":{"datavalue":{"type":"globecoordinate","value":{"latitude":48.856,"longitude":2.352}}}}]}}
]"#;

        let mut entities = Vec::new();
        let config = WikidataConfig::default();
        let count = parse_wikidata(json.as_bytes(), &config, |e| entities.push(e)).unwrap();

        assert_eq!(count, 1);
        let paris = &entities[0];
        assert_eq!(paris.qid, "Q90");
        assert_eq!(paris.labels.get("en"), Some(&"Paris".to_string()));
        assert_eq!(paris.labels.get("ru"), Some(&"Париж".to_string()));
        assert_eq!(paris.description, Some("capital of France".to_string()));
        assert_eq!(paris.entity_type, Some("Q515".to_string()));
        assert!((paris.latitude.unwrap() - 48.856).abs() < 0.001);
        assert!((paris.longitude.unwrap() - 2.352).abs() < 0.001);
        assert_eq!(paris.properties.len(), 4); // P31, P17, P1082, P625
    }

    #[test]
    fn test_skip_non_items() {
        let json = r#"[
{"type":"property","id":"P31","labels":{"en":{"language":"en","value":"instance of"}}}
]"#;
        let mut count_cb = 0;
        let config = WikidataConfig::default();
        parse_wikidata(json.as_bytes(), &config, |_| count_cb += 1).unwrap();
        assert_eq!(count_cb, 0);
    }

    #[test]
    fn test_skip_without_english_label() {
        let json = r#"[
{"type":"item","id":"Q999","labels":{"de":{"language":"de","value":"Nur Deutsch"}},"claims":{}}
]"#;
        let mut count_cb = 0;
        let config = WikidataConfig::default();
        parse_wikidata(json.as_bytes(), &config, |_| count_cb += 1).unwrap();
        assert_eq!(count_cb, 0);
    }

    #[test]
    fn test_limit() {
        let json = r#"[
{"type":"item","id":"Q1","labels":{"en":{"language":"en","value":"One"}},"claims":{}},
{"type":"item","id":"Q2","labels":{"en":{"language":"en","value":"Two"}},"claims":{}},
{"type":"item","id":"Q3","labels":{"en":{"language":"en","value":"Three"}},"claims":{}}
]"#;
        let mut entities = Vec::new();
        let config = WikidataConfig {
            limit: 2,
            type_filter: vec![],
        };
        parse_wikidata(json.as_bytes(), &config, |e| entities.push(e)).unwrap();
        assert_eq!(entities.len(), 2);
    }
}
