//! Photo EXIF metadata extractor.

use lokb_core::{ContentHash, ContentType, Document, PrivacyLevel, Result};
use std::path::Path;
use uuid::Uuid;

/// Extract EXIF metadata from a photo and create a text description.
pub fn extract_exif(path: &Path, source_id: Uuid, external_id: &str) -> Result<(Document, String)> {
    let file = std::fs::File::open(path)?;
    let mut buf_reader = std::io::BufReader::new(file);
    let exif_reader = exif::Reader::new();
    let exif_data = exif_reader
        .read_from_container(&mut buf_reader)
        .map_err(|e| lokb_core::Error::Storage(format!("EXIF read failed: {e}")))?;

    let mut text = format!("# Photo: {}\n\n", external_id);

    // DateTime
    if let Some(field) = exif_data.get_field(exif::Tag::DateTimeOriginal, exif::In::PRIMARY) {
        text.push_str(&format!("Date: {}\n", field.display_value()));
    }

    // Camera
    if let Some(field) = exif_data.get_field(exif::Tag::Make, exif::In::PRIMARY) {
        let make = field.display_value().to_string().replace('"', "");
        if let Some(model_field) = exif_data.get_field(exif::Tag::Model, exif::In::PRIMARY) {
            let model = model_field.display_value().to_string().replace('"', "");
            text.push_str(&format!("Camera: {make} {model}\n"));
        } else {
            text.push_str(&format!("Camera: {make}\n"));
        }
    }

    // GPS
    let latitude = extract_gps_coord(
        &exif_data,
        exif::Tag::GPSLatitude,
        exif::Tag::GPSLatitudeRef,
    );
    let longitude = extract_gps_coord(
        &exif_data,
        exif::Tag::GPSLongitude,
        exif::Tag::GPSLongitudeRef,
    );
    if let (Some(lat), Some(lon)) = (latitude, longitude) {
        text.push_str(&format!("GPS: {lat:.6}°N, {lon:.6}°E\n"));
    }

    // Image size
    if let (Some(w), Some(h)) = (
        exif_data.get_field(exif::Tag::PixelXDimension, exif::In::PRIMARY),
        exif_data.get_field(exif::Tag::PixelYDimension, exif::In::PRIMARY),
    ) {
        text.push_str(&format!(
            "Size: {} × {}\n",
            w.display_value(),
            h.display_value()
        ));
    }

    // Exposure
    if let Some(field) = exif_data.get_field(exif::Tag::ExposureTime, exif::In::PRIMARY) {
        text.push_str(&format!("Exposure: {}\n", field.display_value()));
    }
    if let Some(field) = exif_data.get_field(exif::Tag::FNumber, exif::In::PRIMARY) {
        text.push_str(&format!("Aperture: f/{}\n", field.display_value()));
    }
    if let Some(field) = exif_data.get_field(exif::Tag::PhotographicSensitivity, exif::In::PRIMARY)
    {
        text.push_str(&format!("ISO: {}\n", field.display_value()));
    }

    let doc = Document {
        id: Uuid::now_v7(),
        source_id,
        external_id: external_id.to_string(),
        parent_id: None,
        depth: 0,
        title: format!("Photo {}", external_id),
        content_type: ContentType::MediaMeta,
        language: None,
        content_hash: ContentHash::from_bytes(text.as_bytes()),
        content_size: text.len() as u64,
        created_at: chrono::Utc::now(),
        indexed_at: chrono::Utc::now(),
        privacy_level: PrivacyLevel::Private,
    };

    Ok((doc, text))
}

fn extract_gps_coord(
    exif_data: &exif::Exif,
    coord_tag: exif::Tag,
    ref_tag: exif::Tag,
) -> Option<f64> {
    let field = exif_data.get_field(coord_tag, exif::In::PRIMARY)?;
    let values = match &field.value {
        exif::Value::Rational(v) if v.len() >= 3 => v,
        _ => return None,
    };

    let deg = values[0].to_f64();
    let min = values[1].to_f64();
    let sec = values[2].to_f64();
    let mut coord = deg + min / 60.0 + sec / 3600.0;

    if let Some(ref_field) = exif_data.get_field(ref_tag, exif::In::PRIMARY) {
        let ref_str = ref_field.display_value().to_string();
        if ref_str.contains('S') || ref_str.contains('W') {
            coord = -coord;
        }
    }

    Some(coord)
}
