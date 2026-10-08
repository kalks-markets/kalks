//! Photo pipeline: decode (JPEG, PNG, WebP, GIF first frame) with size limits, apply the EXIF orientation, drop
//! all metadata, and encode three WebP sizes plus a small JPEG preview for the AI check.

use image::codecs::jpeg::JpegEncoder;
use image::imageops::FilterType;
use image::{DynamicImage, GenericImageView, ImageDecoder, ImageReader, Limits};
use std::io::Cursor;

/// Longest side of each variant for a purpose: (thumb, small, large).
pub fn sizes(purpose: &str) -> [(&'static str, u32); 3] {
    match purpose {
        "avatar" => [("thumb", 96), ("small", 256), ("large", 512)],
        "cover" => [("thumb", 640), ("small", 1280), ("large", 1920)],
        _ => [("thumb", 320), ("small", 1080), ("large", 2048)],
    }
}

pub struct Processed {
    pub width: u32,
    pub height: u32,
    /// (variant, WebP bytes)
    pub variants: Vec<(&'static str, Vec<u8>)>,
    /// ≤ 1024 px JPEG for moderation.
    pub preview: Vec<u8>,
}

pub fn decode(bytes: &[u8]) -> anyhow::Result<DynamicImage> {
    let mut reader = ImageReader::new(Cursor::new(bytes)).with_guessed_format()?;
    let mut limits = Limits::default();
    limits.max_image_width = Some(12_000);
    limits.max_image_height = Some(12_000);
    limits.max_alloc = Some(512 * 1024 * 1024);
    reader.limits(limits);
    let mut decoder = reader.into_decoder()?;
    let orientation = decoder.orientation()?;
    let mut img = DynamicImage::from_decoder(decoder)?;
    img.apply_orientation(orientation);
    Ok(img)
}

fn fit(img: &DynamicImage, max: u32) -> DynamicImage {
    let (w, h) = img.dimensions();
    if w.max(h) <= max { img.clone() } else { img.resize(max, max, FilterType::Lanczos3) }
}

pub fn webp(img: &DynamicImage, quality: f32) -> Vec<u8> {
    let (w, h) = img.dimensions();
    if img.color().has_alpha() {
        let rgba = img.to_rgba8();
        webp::Encoder::from_rgba(rgba.as_raw(), w, h).encode(quality).to_vec()
    } else {
        let rgb = img.to_rgb8();
        webp::Encoder::from_rgb(rgb.as_raw(), w, h).encode(quality).to_vec()
    }
}

pub fn jpeg(img: &DynamicImage, quality: u8) -> anyhow::Result<Vec<u8>> {
    let mut out = Vec::new();
    let rgb = DynamicImage::ImageRgb8(img.to_rgb8());
    JpegEncoder::new_with_quality(&mut out, quality).encode_image(&rgb)?;
    Ok(out)
}

pub fn process(bytes: &[u8], purpose: &str) -> anyhow::Result<Processed> {
    let img = decode(bytes)?;
    let (width, height) = img.dimensions();
    if width < 16 || height < 16 {
        anyhow::bail!("image too small ({width}x{height})");
    }
    let mut variants = Vec::new();
    for (name, max) in sizes(purpose) {
        let q = if name == "thumb" { 72.0 } else { 80.0 };
        variants.push((name, webp(&fit(&img, max), q)));
    }
    let preview = jpeg(&fit(&img, 1024), 80)?;
    let (w, h) = fit(&img, 2048).dimensions();
    Ok(Processed { width: w, height: h, variants, preview })
}

/// A JPEG preview (≤ 1024 px) of any decodable image (video posters / frames for moderation).
pub fn preview(bytes: &[u8]) -> anyhow::Result<Vec<u8>> {
    jpeg(&fit(&decode(bytes)?, 1024), 80)
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{ImageBuffer, Rgb};

    pub fn sample_png(w: u32, h: u32) -> Vec<u8> {
        let img = ImageBuffer::from_fn(w, h, |x, y| Rgb([(x % 256) as u8, (y % 256) as u8, 128u8]));
        let mut out = Vec::new();
        DynamicImage::ImageRgb8(img).write_to(&mut Cursor::new(&mut out), image::ImageFormat::Png).unwrap();
        out
    }

    #[test]
    fn makes_three_webp_sizes() {
        let p = process(&sample_png(3000, 1500), "post").unwrap();
        assert_eq!((p.width, p.height), (2048, 1024));
        assert_eq!(p.variants.iter().map(|(n, _)| *n).collect::<Vec<_>>(), vec!["thumb", "small", "large"]);
        for (_, b) in &p.variants {
            assert_eq!(&b[..4], b"RIFF");
            assert_eq!(&b[8..12], b"WEBP");
        }
        let small = decode(&p.variants[1].1).unwrap();
        assert_eq!(small.dimensions(), (1080, 540));
        assert_eq!(&p.preview[..2], &[0xFF, 0xD8], "JPEG preview");
        // small images are never upscaled
        let s = process(&sample_png(200, 100), "post").unwrap();
        assert_eq!(decode(&s.variants[2].1).unwrap().dimensions(), (200, 100));
        assert!(process(b"not an image", "post").is_err());
        assert!(process(&sample_png(8, 8), "post").is_err());
        assert_eq!(sizes("avatar")[0], ("thumb", 96));
    }
}
