use image::ImageFormat;

#[derive(PartialEq, Eq, Clone, Copy, Debug)]
pub enum TargetFormat {
    Png,
    Jpeg,
    Webp,
    Gif,
    Bmp,
    Ico,
    Tiff,
    Tga,
    Avif,
    Pnm,
    Qoi,
}

#[derive(PartialEq, Eq, Clone, Copy, Debug)]
pub enum OnConflict {
    Skip,
    Overwrite,
}

#[derive(PartialEq, Eq, Clone, Copy, Debug)]
pub enum PngCompression {
    Fast,
    Default,
    Best,
}

#[derive(Clone)]
pub struct ConvertOptions {
    pub format: TargetFormat,
    pub jpeg_quality: u8,
    pub png_compression: PngCompression,
    pub background: [u8; 3],
    pub on_conflict: OnConflict,
}

impl Default for ConvertOptions {
    fn default() -> Self {
        Self {
            format: TargetFormat::Png,
            jpeg_quality: 90,
            png_compression: PngCompression::Default,
            background: [255, 255, 255],
            on_conflict: OnConflict::Skip,
        }
    }
}

impl TargetFormat {
    pub fn all() -> &'static [TargetFormat] {
        use TargetFormat::*;
        &[Png, Jpeg, Webp, Gif, Bmp, Ico, Tiff, Tga, Avif, Pnm, Qoi]
    }

    pub fn label(&self) -> &'static str {
        match self {
            TargetFormat::Png => "PNG",
            TargetFormat::Jpeg => "JPEG",
            TargetFormat::Webp => "WebP",
            TargetFormat::Gif => "GIF",
            TargetFormat::Bmp => "BMP",
            TargetFormat::Ico => "ICO",
            TargetFormat::Tiff => "TIFF",
            TargetFormat::Tga => "TGA",
            TargetFormat::Avif => "AVIF",
            TargetFormat::Pnm => "PNM",
            TargetFormat::Qoi => "QOI",
        }
    }

    pub fn extension(&self) -> &'static str {
        match self {
            TargetFormat::Png => "png",
            TargetFormat::Jpeg => "jpg",
            TargetFormat::Webp => "webp",
            TargetFormat::Gif => "gif",
            TargetFormat::Bmp => "bmp",
            TargetFormat::Ico => "ico",
            TargetFormat::Tiff => "tiff",
            TargetFormat::Tga => "tga",
            TargetFormat::Avif => "avif",
            TargetFormat::Pnm => "pnm",
            TargetFormat::Qoi => "qoi",
        }
    }

    pub fn to_image_format(&self) -> ImageFormat {
        match self {
            TargetFormat::Png => ImageFormat::Png,
            TargetFormat::Jpeg => ImageFormat::Jpeg,
            TargetFormat::Webp => ImageFormat::WebP,
            TargetFormat::Gif => ImageFormat::Gif,
            TargetFormat::Bmp => ImageFormat::Bmp,
            TargetFormat::Ico => ImageFormat::Ico,
            TargetFormat::Tiff => ImageFormat::Tiff,
            TargetFormat::Tga => ImageFormat::Tga,
            TargetFormat::Avif => ImageFormat::Avif,
            TargetFormat::Pnm => ImageFormat::Pnm,
            TargetFormat::Qoi => ImageFormat::Qoi,
        }
    }

    pub fn supports_alpha(&self) -> bool {
        match self {
            TargetFormat::Png
            | TargetFormat::Webp
            | TargetFormat::Gif
            | TargetFormat::Tiff
            | TargetFormat::Tga
            | TargetFormat::Avif
            | TargetFormat::Ico
            | TargetFormat::Pnm
            | TargetFormat::Qoi => true,
            TargetFormat::Jpeg | TargetFormat::Bmp => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_options_are_jpeg_quality_90_skip() {
        let o = ConvertOptions::default();
        assert_eq!(o.format, TargetFormat::Png);
        assert_eq!(o.jpeg_quality, 90);
        assert_eq!(o.on_conflict, OnConflict::Skip);
    }

    #[test]
    fn jpeg_does_not_support_alpha() {
        assert!(!TargetFormat::Jpeg.supports_alpha());
        assert!(!TargetFormat::Bmp.supports_alpha());
        assert!(TargetFormat::Png.supports_alpha());
    }

    #[test]
    fn extension_mapping() {
        assert_eq!(TargetFormat::Jpeg.extension(), "jpg");
        assert_eq!(TargetFormat::Webp.extension(), "webp");
    }
}
