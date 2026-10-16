//! Read-only OCR input transforms. Every returned box remains in source coordinates.
use super::{
    adapters::ocr::{Document, PaddleOcr},
    types::ResolvedModel,
};
use image::{DynamicImage, Rgb, RgbImage};
fn transformed(
    image: &DynamicImage,
    degrees: f64,
    check: &dyn Fn() -> Result<(), String>,
) -> Result<DynamicImage, String> {
    let angle = degrees.rem_euclid(360.);
    if angle.abs() < 1e-8 {
        return Ok(image.clone());
    }
    if (angle - 90.).abs() < 1e-8 {
        return Ok(image.rotate90());
    }
    if (angle - 180.).abs() < 1e-8 {
        return Ok(image.rotate180());
    }
    if (angle - 270.).abs() < 1e-8 {
        return Ok(image.rotate270());
    }
    let input = image.to_rgb8();
    let (w, h) = input.dimensions();
    let radians = degrees.to_radians();
    let (sin, cos) = radians.sin_cos();
    let ow = (w as f64 * cos.abs() + h as f64 * sin.abs()).ceil() as u32;
    let oh = (w as f64 * sin.abs() + h as f64 * cos.abs()).ceil() as u32;
    if u64::from(ow) * u64::from(oh) > 120_000_000 {
        return Err("Rotated image exceeds OCR memory limit".into());
    }
    let mut output = RgbImage::from_pixel(ow, oh, Rgb([255, 255, 255]));
    for y in 0..oh {
        if y % 64 == 0 {
            check()?;
        }
        for x in 0..ow {
            let dx = x as f64 + 0.5 - ow as f64 / 2.;
            let dy = y as f64 + 0.5 - oh as f64 / 2.;
            let sx = cos * dx + sin * dy + w as f64 / 2. - 0.5;
            let sy = -sin * dx + cos * dy + h as f64 / 2. - 0.5;
            if sx < 0. || sy < 0. || sx > w as f64 - 1. || sy > h as f64 - 1. {
                continue;
            }
            let x0 = sx.floor() as u32;
            let y0 = sy.floor() as u32;
            let x1 = (x0 + 1).min(w - 1);
            let y1 = (y0 + 1).min(h - 1);
            let fx = sx - x0 as f64;
            let fy = sy - y0 as f64;
            let mut channels = [0u8; 3];
            for (i, value) in channels.iter_mut().enumerate() {
                *value = ((1. - fy)
                    * ((1. - fx) * input.get_pixel(x0, y0)[i] as f64
                        + fx * input.get_pixel(x1, y0)[i] as f64)
                    + fy * ((1. - fx) * input.get_pixel(x0, y1)[i] as f64
                        + fx * input.get_pixel(x1, y1)[i] as f64))
                    .round()
                    .clamp(0., 255.) as u8;
            }
            output.put_pixel(x, y, Rgb(channels));
        }
    }
    Ok(DynamicImage::ImageRgb8(output))
}
pub(crate) fn original_bounds(
    bounds: [f32; 4],
    source: (u32, u32),
    rotated: (u32, u32),
    degrees: f64,
) -> Option<[f32; 4]> {
    if !bounds.iter().all(|v| v.is_finite())
        || source.0 == 0
        || source.1 == 0
        || rotated.0 == 0
        || rotated.1 == 0
    {
        return None;
    }
    let [x, y, w, h] = bounds;
    let (sin, cos) = degrees.to_radians().sin_cos();
    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut max_y = f64::NEG_INFINITY;
    for (x, y) in [(x, y), (x + w, y), (x, y + h), (x + w, y + h)] {
        let dx = x as f64 * rotated.0 as f64 - rotated.0 as f64 / 2.;
        let dy = y as f64 * rotated.1 as f64 - rotated.1 as f64 / 2.;
        let sx = (cos * dx + sin * dy + source.0 as f64 / 2.).clamp(0., source.0 as f64);
        let sy = (-sin * dx + cos * dy + source.1 as f64 / 2.).clamp(0., source.1 as f64);
        min_x = min_x.min(sx);
        max_x = max_x.max(sx);
        min_y = min_y.min(sy);
        max_y = max_y.max(sy);
    }
    if max_x <= min_x || max_y <= min_y {
        return None;
    }
    Some([
        (min_x / source.0 as f64) as f32,
        (min_y / source.1 as f64) as f32,
        ((max_x - min_x) / source.0 as f64) as f32,
        ((max_y - min_y) / source.1 as f64) as f32,
    ])
}
fn score(doc: &Document) -> f64 {
    doc.lines
        .iter()
        .map(|line| {
            line.text.chars().filter(|c| !c.is_whitespace()).count() as f64
                * f64::from(line.confidence).powi(2)
        })
        .sum()
}
fn confident(doc: &Document) -> bool {
    let n: usize = doc.lines.iter().map(|l| l.text.chars().count()).sum();
    n >= 4 && !doc.lines.is_empty() && doc.lines.iter().all(|l| l.confidence >= 0.9)
}
fn recognize_with(
    image: &DynamicImage,
    mode: &str,
    tilt: f64,
    check: &dyn Fn() -> Result<(), String>,
    mut infer: impl FnMut(&DynamicImage) -> Result<Document, String>,
) -> Result<Document, String> {
    if !tilt.is_finite() || tilt.abs() > 15. {
        return Err("OCR tilt correction must be between -15 and 15 degrees".into());
    }
    let angles: &[i32] = match mode {
        "0" => &[0],
        "90" => &[90],
        "180" => &[180],
        "270" => &[270],
        "auto" => &[0, 90, 180, 270],
        _ => return Err("Unsupported OCR direction".into()),
    };
    let mut best: Option<Document> = None;
    for angle in angles {
        check()?;
        let total = *angle as f64 + tilt;
        let mut doc = if total == 0. {
            infer(image)?
        } else {
            let rotated = transformed(image, total, check)?;
            let mut doc = infer(&rotated)?;
            doc.lines = doc
                .lines
                .into_iter()
                .filter_map(|mut line| {
                    line.bounds = original_bounds(
                        line.bounds,
                        (image.width(), image.height()),
                        (rotated.width(), rotated.height()),
                        total,
                    )?;
                    Some(line)
                })
                .collect();
            doc
        };
        doc.image_width = image.width();
        doc.image_height = image.height();
        doc.rotation_degrees = *angle;
        doc.deskew_degrees = tilt as f32;
        doc.text = doc
            .lines
            .iter()
            .map(|line| line.text.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        let stop = *angle == 0 && mode == "auto" && confident(&doc);
        if best.as_ref().is_none_or(|old| score(&doc) > score(old)) {
            best = Some(doc);
        }
        if stop {
            break;
        }
    }
    best.ok_or_else(|| "No OCR direction was evaluated".into())
}
pub fn recognize(
    backend: &mut PaddleOcr,
    model: &ResolvedModel,
    image: &DynamicImage,
    rotation: Option<&str>,
    check: &dyn Fn() -> Result<(), String>,
) -> Result<Document, String> {
    let mode = rotation
        .or_else(|| {
            model
                .values
                .get("ocr_rotation")
                .and_then(serde_json::Value::as_str)
        })
        .unwrap_or("0");
    recognize_with(
        image,
        mode,
        model.number("ocr_deskew_degrees"),
        check,
        |image| backend.recognize(image, check),
    )
}
#[cfg(test)]
mod tests {
    use super::super::adapters::ocr::TextLine;
    use super::*;
    fn doc(confidence: f32, text: &str) -> Document {
        Document {
            text: text.into(),
            lines: vec![TextLine {
                text: text.into(),
                confidence,
                bounds: [0.1, 0.2, 0.3, 0.4],
            }],
            ..Default::default()
        }
    }
    #[test]
    #[ignore = "Requires workspace-only LAP_AI_TEST_MODEL_ROOT OCR fixture"]
    fn real_rotated_fixture_is_corrected_without_changing_the_source() {
        let root = std::path::PathBuf::from(std::env::var_os("LAP_AI_TEST_MODEL_ROOT").unwrap());
        let image = image::open(root.join("ocr-fixture.png"))
            .unwrap()
            .rotate90();
        let bytes = image.as_bytes().to_vec();
        let mut configuration = super::super::settings::Configuration::default();
        let parameters = &mut configuration
            .models
            .get_mut("rapidocr-pp-ocrv5-server")
            .unwrap()
            .parameters;
        parameters.insert("device".into(), serde_json::json!("cpu"));
        parameters.insert("ocr_rotation".into(), serde_json::json!("270"));
        let model = configuration.resolve("rapidocr-pp-ocrv5-server").unwrap();
        let mut backend = PaddleOcr::new(&model).unwrap();
        let result = recognize(&mut backend, &model, &image, None, &|| Ok(())).unwrap();
        assert!(result.text.contains("LAP"));
        assert!(result.text.contains("中文"));
        assert_eq!(result.rotation_degrees, 270);
        assert_eq!(
            (result.image_width, result.image_height),
            (image.width(), image.height())
        );
        assert_eq!(image.as_bytes(), bytes);
        assert!(result.lines.iter().all(|line| {
            line.bounds
                .iter()
                .all(|v| v.is_finite() && *v >= 0. && *v <= 1.)
        }));
    }
    #[test]
    fn right_angle_boxes_map_back_to_original_photo() {
        for (angle, rotated, expected) in [
            (90., (80, 100), [0.2, 0.6, 0.4, 0.3]),
            (180., (100, 80), [0.6, 0.4, 0.3, 0.4]),
            (270., (80, 100), [0.4, 0.1, 0.4, 0.3]),
        ] {
            let actual = original_bounds([0.1, 0.2, 0.3, 0.4], (100, 80), rotated, angle).unwrap();
            for (i, value) in actual.iter().enumerate() {
                assert!((*value - expected[i]).abs() < 1e-5);
            }
        }
    }
    #[test]
    fn automatic_fallback_is_opt_in_and_skips_confident_upright_text() {
        let image = DynamicImage::new_rgb8(100, 80);
        let mut count = 0;
        let result = recognize_with(&image, "auto", 0., &|| Ok(()), |_| {
            count += 1;
            Ok(doc(0.99, "clear text"))
        })
        .unwrap();
        assert_eq!(count, 1);
        assert_eq!(result.rotation_degrees, 0);
        count = 0;
        let result = recognize_with(&image, "auto", 0., &|| Ok(()), |im| {
            count += 1;
            Ok(if im.width() == 80 {
                doc(0.95, "better text")
            } else {
                doc(0.5, "bad")
            })
        })
        .unwrap();
        assert_eq!(count, 4);
        assert_eq!(result.rotation_degrees, 90);
        assert_eq!((result.image_width, result.image_height), (100, 80));
    }
    #[test]
    fn deskew_preserves_source_pixels_and_boxes_and_honours_cancellation() {
        let image = DynamicImage::new_rgb8(100, 80);
        let original = image.as_bytes().to_vec();
        let rotated = transformed(&image, 8., &|| Ok(())).unwrap();
        assert!(rotated.width() > 100);
        assert_eq!(image.as_bytes(), original);
        let bounds = original_bounds(
            [0., 0., 1., 1.],
            (100, 80),
            (rotated.width(), rotated.height()),
            8.,
        )
        .unwrap();
        assert!(bounds.iter().all(|v| v.is_finite() && *v >= 0. && *v <= 1.));
        assert!(
            recognize_with(&image, "auto", 0., &|| Err("cancelled".into()), |_| panic!(
                "must not run"
            ))
            .is_err()
        );
        assert!(recognize_with(&image, "0", 16., &|| Ok(()), |_| Ok(Document::default())).is_err());
    }
}
