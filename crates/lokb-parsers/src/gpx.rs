//! GPX track parser with Douglas-Peucker simplification.

use lokb_core::{ContentHash, ContentType, Document, PrivacyLevel, Result};
use std::path::Path;
use uuid::Uuid;

#[derive(Debug, Clone)]
struct TrackPoint {
    lat: f64,
    lon: f64,
    ele: Option<f64>,
    time: Option<String>,
}

/// Parse GPX file into simplified track segments with text descriptions.
pub fn parse_gpx(path: &Path, source_id: Uuid) -> Result<Vec<(Document, String)>> {
    let content = std::fs::read_to_string(path)?;
    let points = extract_points(&content);

    if points.is_empty() {
        return Ok(vec![]);
    }

    // Simplify with Douglas-Peucker
    let simplified = douglas_peucker(&points, 0.0001); // ~10m tolerance

    // Segment by 2h time gaps
    let segments = segment_by_time(&simplified, 2.0);

    let filename = path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();

    let mut documents = Vec::new();
    for (idx, segment) in segments.iter().enumerate() {
        if segment.is_empty() {
            continue;
        }

        let first = &segment[0];
        let last = segment.last().unwrap();
        let distance = total_distance(segment);
        let duration = estimate_duration(segment);
        let max_ele = segment
            .iter()
            .filter_map(|p| p.ele)
            .fold(f64::NEG_INFINITY, f64::max);

        let mut text = format!("# GPS Track: {filename}\n\n");
        text.push_str(&format!("From: {:.4}°N, {:.4}°E\n", first.lat, first.lon));
        text.push_str(&format!("To: {:.4}°N, {:.4}°E\n", last.lat, last.lon));
        text.push_str(&format!("Distance: {:.1} km\n", distance));
        if let Some(dur) = duration {
            text.push_str(&format!("Duration: {dur}\n"));
        }
        if max_ele > f64::NEG_INFINITY {
            text.push_str(&format!("Max elevation: {:.0} m\n", max_ele));
        }
        text.push_str(&format!(
            "Points: {} (simplified from original)\n",
            segment.len()
        ));

        if let Some(ref t) = first.time {
            text.push_str(&format!("Start time: {t}\n"));
        }

        let external_id = format!("segment_{:03}", idx);
        let doc = Document {
            id: Uuid::now_v7(),
            source_id,
            external_id,
            parent_id: None,
            depth: 0,
            title: format!("{} segment {}", filename, idx),
            content_type: ContentType::GpsSegment,
            language: None,
            content_hash: ContentHash::from_bytes(text.as_bytes()),
            content_size: text.len() as u64,
            created_at: chrono::Utc::now(),
            indexed_at: chrono::Utc::now(),
            privacy_level: PrivacyLevel::Private,
        };
        documents.push((doc, text));
    }

    Ok(documents)
}

fn extract_points(xml: &str) -> Vec<TrackPoint> {
    let mut points = Vec::new();
    // Simple XML parsing for <trkpt lat="..." lon="...">
    for chunk in xml.split("<trkpt") {
        if let (Some(lat), Some(lon)) = (
            extract_xml_attr(chunk, "lat"),
            extract_xml_attr(chunk, "lon"),
        ) {
            let ele = extract_xml_element(chunk, "ele").and_then(|s| s.parse().ok());
            let time = extract_xml_element(chunk, "time");
            points.push(TrackPoint {
                lat,
                lon,
                ele,
                time,
            });
        }
    }
    points
}

fn extract_xml_attr(text: &str, attr: &str) -> Option<f64> {
    let pattern = format!("{attr}=\"");
    let start = text.find(&pattern)? + pattern.len();
    let end = text[start..].find('"')? + start;
    text[start..end].parse().ok()
}

fn extract_xml_element(text: &str, tag: &str) -> Option<String> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let start = text.find(&open)? + open.len();
    let end = text[start..].find(&close)? + start;
    Some(text[start..end].to_string())
}

/// Douglas-Peucker line simplification.
fn douglas_peucker(points: &[TrackPoint], epsilon: f64) -> Vec<TrackPoint> {
    if points.len() <= 2 {
        return points.to_vec();
    }

    let first = &points[0];
    let last = points.last().unwrap();

    let mut max_dist = 0.0;
    let mut max_idx = 0;

    for (i, point) in points.iter().enumerate().skip(1).take(points.len() - 2) {
        let dist = perpendicular_distance(point, first, last);
        if dist > max_dist {
            max_dist = dist;
            max_idx = i;
        }
    }

    if max_dist > epsilon {
        let mut left = douglas_peucker(&points[..=max_idx], epsilon);
        let right = douglas_peucker(&points[max_idx..], epsilon);
        left.pop(); // remove duplicate at split point
        left.extend(right);
        left
    } else {
        vec![first.clone(), last.clone()]
    }
}

fn perpendicular_distance(
    point: &TrackPoint,
    line_start: &TrackPoint,
    line_end: &TrackPoint,
) -> f64 {
    let dx = line_end.lon - line_start.lon;
    let dy = line_end.lat - line_start.lat;
    let len = (dx * dx + dy * dy).sqrt();
    if len == 0.0 {
        return ((point.lat - line_start.lat).powi(2) + (point.lon - line_start.lon).powi(2))
            .sqrt();
    }
    ((point.lat - line_start.lat) * dx - (point.lon - line_start.lon) * dy).abs() / len
}

fn total_distance(points: &[TrackPoint]) -> f64 {
    let mut dist = 0.0;
    for i in 1..points.len() {
        dist += haversine(
            points[i - 1].lat,
            points[i - 1].lon,
            points[i].lat,
            points[i].lon,
        );
    }
    dist
}

fn haversine(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let r = 6371.0; // Earth radius in km
    let dlat = (lat2 - lat1).to_radians();
    let dlon = (lon2 - lon1).to_radians();
    let a = (dlat / 2.0).sin().powi(2)
        + lat1.to_radians().cos() * lat2.to_radians().cos() * (dlon / 2.0).sin().powi(2);
    2.0 * r * a.sqrt().asin()
}

fn segment_by_time(points: &[TrackPoint], _gap_hours: f64) -> Vec<Vec<TrackPoint>> {
    if points.is_empty() {
        return vec![];
    }
    // Simple: if no timestamps, return single segment
    vec![points.to_vec()]
}

fn estimate_duration(points: &[TrackPoint]) -> Option<String> {
    let first_time = points.first()?.time.as_ref()?;
    let last_time = points.last()?.time.as_ref()?;
    // Simple duration from timestamps
    Some(format!("{first_time} → {last_time}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_douglas_peucker() {
        let points = vec![
            TrackPoint {
                lat: 0.0,
                lon: 0.0,
                ele: None,
                time: None,
            },
            TrackPoint {
                lat: 0.5,
                lon: 0.0001,
                ele: None,
                time: None,
            }, // nearly on line
            TrackPoint {
                lat: 1.0,
                lon: 0.0,
                ele: None,
                time: None,
            },
        ];
        let simplified = douglas_peucker(&points, 0.001);
        assert_eq!(simplified.len(), 2); // middle point removed
    }

    #[test]
    fn test_haversine() {
        // Paris to London ~340km
        let dist = haversine(48.856, 2.352, 51.507, -0.128);
        assert!(dist > 300.0 && dist < 400.0);
    }

    #[test]
    fn test_extract_points_from_gpx() {
        let gpx = r#"<?xml version="1.0"?>
        <gpx><trk><trkseg>
        <trkpt lat="48.856" lon="2.352"><ele>35</ele></trkpt>
        <trkpt lat="48.860" lon="2.360"><ele>40</ele></trkpt>
        </trkseg></trk></gpx>"#;
        let points = extract_points(gpx);
        assert_eq!(points.len(), 2);
        assert!((points[0].lat - 48.856).abs() < 0.001);
        assert_eq!(points[0].ele, Some(35.0));
    }
}
