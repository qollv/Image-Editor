use std::fmt;
use std::path::Path;

use crate::options::{ConvertOptions, PngCompression, TargetFormat};

#[derive(Debug)]
pub enum ConvertError {
    Io(std::io::Error),
    Image(image::ImageError),
    Unsupported(String),
}

impl fmt::Display for ConvertError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConvertError::Io(e) => write!(f, "IO 错误: {}", e),
            ConvertError::Image(e) => write!(f, "图像错误: {}", e),
            ConvertError::Unsupported(s) => write!(f, "不支持: {}", s),
        }
    }
}

impl std::error::Error for ConvertError {}

impl From<std::io::Error> for ConvertError {
    fn from(e: std::io::Error) -> Self {
        ConvertError::Io(e)
    }
}
impl From<image::ImageError> for ConvertError {
    fn from(e: image::ImageError) -> Self {
        ConvertError::Image(e)
    }
}

/// 把带 alpha 的图合成到纯色背景上（用于 JPEG/BMP 等不支持透明的格式）。
fn flatten_alpha(img: image::DynamicImage, bg: [u8; 3]) -> image::DynamicImage {
    let rgba = img.to_rgba8();
    let mut rgb = image::RgbImage::new(rgba.width(), rgba.height());
    for (x, y, p) in rgba.enumerate_pixels() {
        let a = p[3] as f32 / 255.0;
        let r = (p[0] as f32 * a + bg[0] as f32 * (1.0 - a)) as u8;
        let g = (p[1] as f32 * a + bg[1] as f32 * (1.0 - a)) as u8;
        let b = (p[2] as f32 * a + bg[2] as f32 * (1.0 - a)) as u8;
        rgb.put_pixel(x, y, image::Rgb([r, g, b]));
    }
    image::DynamicImage::ImageRgb8(rgb)
}

pub fn convert_file(input: &Path, output: &Path, opts: &ConvertOptions) -> Result<(), ConvertError> {
    let mut img = image::open(input)?;

    if !opts.format.supports_alpha() && img.color().has_alpha() {
        img = flatten_alpha(img, opts.background);
    }

    match opts.format {
        TargetFormat::Png => save_png(&img, output, opts),
        TargetFormat::Jpeg => save_jpeg(&img, output, opts),
        TargetFormat::Webp => save_webp(&img, output),
        other => {
            img.save_with_format(output, other.to_image_format())?;
            Ok(())
        }
    }
}

fn save_png(img: &image::DynamicImage, output: &Path, opts: &ConvertOptions) -> Result<(), ConvertError> {
    let f = std::fs::File::create(output)?;
    let compression = match opts.png_compression {
        PngCompression::Fast => image::codecs::png::CompressionType::Fast,
        PngCompression::Default => image::codecs::png::CompressionType::Default,
        PngCompression::Best => image::codecs::png::CompressionType::Best,
    };
    let enc = image::codecs::png::PngEncoder::new_with_quality(
        f,
        compression,
        image::codecs::png::FilterType::Adaptive,
    );
    img.write_with_encoder(enc)?;
    Ok(())
}

fn save_jpeg(img: &image::DynamicImage, output: &Path, opts: &ConvertOptions) -> Result<(), ConvertError> {
    let f = std::fs::File::create(output)?;
    let enc = image::codecs::jpeg::JpegEncoder::new_with_quality(f, opts.jpeg_quality.clamp(1, 100));
    img.write_with_encoder(enc)?;
    Ok(())
}

/// WebP：`image` crate 仅支持无损编码（有损需 libwebp 第三方 C 库，为保持零依赖不做）。
fn save_webp(img: &image::DynamicImage, output: &Path) -> Result<(), ConvertError> {
    let f = std::fs::File::create(output)?;
    let enc = image::codecs::webp::WebPEncoder::new_lossless(f);
    img.write_with_encoder(enc)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::options::*;
    use image::{Rgba, RgbaImage};

    fn sample_rgba() -> RgbaImage {
        let mut img = RgbaImage::new(2, 2);
        img.put_pixel(0, 0, Rgba([255, 0, 0, 255]));
        img.put_pixel(1, 0, Rgba([0, 255, 0, 128]));
        img.put_pixel(0, 1, Rgba([0, 0, 255, 255]));
        img.put_pixel(1, 1, Rgba([0, 0, 0, 0]));
        img
    }

    fn tmp_path(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("ic_test_{}", name))
    }

    #[test]
    fn convert_transparent_png_to_jpeg_flattens_to_white() {
        let src = tmp_path("src.png");
        let dst = tmp_path("out.jpg");
        // 全透明 16x16，避免 JPEG 块压缩串扰
        let img = RgbaImage::from_pixel(16, 16, Rgba([0, 0, 0, 0]));
        img.save(&src).unwrap();

        let mut opts = ConvertOptions::default();
        opts.format = TargetFormat::Jpeg;
        convert_file(&src, &dst, &opts).unwrap();

        let out = image::open(&dst).unwrap().to_rgb8();
        for p in out.pixels() {
            assert!(p[0] > 240 && p[1] > 240 && p[2] > 240, "got {:?}", p);
        }
        std::fs::remove_file(&src).ok();
        std::fs::remove_file(&dst).ok();
    }

    #[test]
    fn convert_png_to_png_preserves_alpha() {
        let src = tmp_path("src2.png");
        let dst = tmp_path("out2.png");
        sample_rgba().save(&src).unwrap();
        let mut opts = ConvertOptions::default();
        opts.format = TargetFormat::Png;
        convert_file(&src, &dst, &opts).unwrap();
        let out = image::open(&dst).unwrap().to_rgba8();
        assert_eq!(out.get_pixel(1, 1), &Rgba([0, 0, 0, 0]));
        std::fs::remove_file(&src).ok();
        std::fs::remove_file(&dst).ok();
    }

    #[test]
    fn convert_png_to_webp_roundtrip() {
        let src = tmp_path("src3.png");
        let dst = tmp_path("out3.webp");
        sample_rgba().save(&src).unwrap();
        let mut opts = ConvertOptions::default();
        opts.format = TargetFormat::Webp;
        convert_file(&src, &dst, &opts).unwrap();
        let out = image::open(&dst).unwrap().to_rgba8();
        assert_eq!(out.dimensions(), (2, 2));
        std::fs::remove_file(&src).ok();
        std::fs::remove_file(&dst).ok();
    }

    #[test]
    fn missing_input_returns_error() {
        let r = convert_file(
            std::path::Path::new("/nonexistent/xx.png"),
            std::path::Path::new("/tmp/yy.jpg"),
            &ConvertOptions::default(),
        );
        assert!(r.is_err());
    }
}
