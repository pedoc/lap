use image::{DynamicImage, Rgb, RgbImage};
// ArcFace's 112x112 five-point template. Coordinates refer to the supplied image.
pub const ARCFACE_POINTS: [(f32, f32); 5] = [
    (38.2946, 51.6963),
    (73.5318, 51.5014),
    (56.0252, 71.7366),
    (41.5493, 92.3655),
    (70.7299, 92.2041),
];
pub fn align_arcface(image: &DynamicImage, points: &[(f32, f32)]) -> Result<RgbImage, String> {
    if points.len() != 5 || points.iter().any(|p| !p.0.is_finite() || !p.1.is_finite()) {
        return Err("ArcFace alignment requires five finite landmarks".into());
    }
    let mean = |p: &[(f32, f32)]| {
        p.iter().fold((0., 0.), |a, p| {
            (a.0 + p.0 as f64 / 5., a.1 + p.1 as f64 / 5.)
        })
    };
    let (mx, my) = mean(points);
    let (ux, uy) = mean(&ARCFACE_POINTS);
    let (mut den, mut a, mut b) = (0., 0., 0.);
    for (s, d) in points.iter().zip(ARCFACE_POINTS) {
        let (x, y) = (s.0 as f64 - mx, s.1 as f64 - my);
        let (u, v) = (d.0 as f64 - ux, d.1 as f64 - uy);
        den += x * x + y * y;
        a += x * u + y * v;
        b += x * v - y * u;
    }
    if !den.is_finite() || den < 1e-8 {
        return Err("Degenerate face landmarks".into());
    }
    a /= den;
    b /= den;
    let scale = a * a + b * b;
    if !scale.is_finite() || scale < 1e-12 {
        return Err("Degenerate alignment transform".into());
    }
    let (tx, ty) = (ux - a * mx + b * my, uy - b * mx - a * my);
    let source = image.to_rgb8();
    let mut output = RgbImage::new(112, 112);
    let sample = |x: i64, y: i64, c: usize| {
        if x >= 0 && y >= 0 && x < source.width() as i64 && y < source.height() as i64 {
            source.get_pixel(x as u32, y as u32)[c] as f64
        } else {
            0.
        }
    };
    for y in 0..112 {
        for x in 0..112 {
            let (u, v) = (x as f64 - tx, y as f64 - ty);
            let (sx, sy) = ((a * u + b * v) / scale, (-b * u + a * v) / scale);
            let (ix, iy) = (sx.floor() as i64, sy.floor() as i64);
            let (fx, fy) = (sx - ix as f64, sy - iy as f64);
            let mut pixel = [0; 3];
            for c in 0..3 {
                let value = sample(ix, iy, c) * (1. - fx) * (1. - fy)
                    + sample(ix + 1, iy, c) * fx * (1. - fy)
                    + sample(ix, iy + 1, c) * (1. - fx) * fy
                    + sample(ix + 1, iy + 1, c) * fx * fy;
                pixel[c] = value.round().clamp(0., 255.) as u8;
            }
            output.put_pixel(x, y, Rgb(pixel));
        }
    }
    Ok(output)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn identity_and_translation_are_recovered() {
        let image = DynamicImage::ImageRgb8(RgbImage::from_fn(144, 144, |x, y| {
            Rgb([x as u8, y as u8, 50])
        }));
        let aligned = align_arcface(&image, &ARCFACE_POINTS).unwrap();
        assert_eq!(aligned.get_pixel(50, 60), &Rgb([50, 60, 50]));
        let translated = ARCFACE_POINTS.map(|(x, y)| (x + 10., y + 20.));
        let aligned = align_arcface(&image, &translated).unwrap();
        assert_eq!(aligned.get_pixel(50, 60), &Rgb([60, 80, 50]));
    }
    #[test]
    fn malformed_and_degenerate_landmarks_are_rejected() {
        let image = DynamicImage::new_rgb8(112, 112);
        assert!(align_arcface(&image, &[(1., 1.); 5]).is_err());
        assert!(align_arcface(&image, &[(1., 1.); 4]).is_err());
    }
}
