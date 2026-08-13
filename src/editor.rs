use std::path::Path;

use crate::draw::{self, Annotation, Style};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CropRect {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

/// 撤销栈条目：标注操作用轻量快照，图片级操作（裁剪/调整/滤镜）用完整原图快照。
enum UndoEntry {
    Annotations(Vec<Annotation>),
    Image {
        image: image::DynamicImage,
        annotations: Vec<Annotation>,
        brightness: i32,
        contrast: f32,
        saturation: f32,
    },
}

pub struct EditorDoc {
    pub image: image::DynamicImage,
    pub annotations: Vec<Annotation>,
    pub style: Style,
    pub crop: Option<CropRect>,
    /// 当前生效的调整值（撤销时一并恢复，使滑块跟着回退）
    pub brightness: i32,
    pub contrast: f32,
    pub saturation: f32,
    undo_stack: Vec<UndoEntry>,
    redo_stack: Vec<UndoEntry>,
}

impl EditorDoc {
    pub fn new(image: image::DynamicImage) -> Self {
        Self {
            image,
            annotations: Vec::new(),
            style: Style {
                color: [255, 0, 0, 255],
                width: 2.0,
            },
            crop: None,
            brightness: 0,
            contrast: 1.0,
            saturation: 1.0,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
        }
    }

    pub fn open(path: &Path) -> Result<Self, image::ImageError> {
        Ok(Self::new(image::open(path)?))
    }

    pub fn push_annotation(&mut self, a: Annotation) {
        self.undo_stack
            .push(UndoEntry::Annotations(self.annotations.clone()));
        self.redo_stack.clear();
        self.annotations.push(a);
    }

    pub fn undo(&mut self) {
        if let Some(entry) = self.undo_stack.pop() {
            match entry {
                UndoEntry::Annotations(prev) => {
                    self.redo_stack
                        .push(UndoEntry::Annotations(std::mem::take(&mut self.annotations)));
                    self.annotations = prev;
                }
                UndoEntry::Image {
                    image,
                    annotations,
                    brightness,
                    contrast,
                    saturation,
                } => {
                    self.redo_stack.push(UndoEntry::Image {
                        image: self.image.clone(),
                        annotations: std::mem::take(&mut self.annotations),
                        brightness: self.brightness,
                        contrast: self.contrast,
                        saturation: self.saturation,
                    });
                    self.image = image;
                    self.annotations = annotations;
                    self.brightness = brightness;
                    self.contrast = contrast;
                    self.saturation = saturation;
                }
            }
        }
    }

    #[allow(dead_code)] // 模型保留撤销/重做配对能力，UI 目前只暴露撤销
    pub fn redo(&mut self) {
        if let Some(entry) = self.redo_stack.pop() {
            match entry {
                UndoEntry::Annotations(next) => {
                    self.undo_stack
                        .push(UndoEntry::Annotations(std::mem::take(&mut self.annotations)));
                    self.annotations = next;
                }
                UndoEntry::Image {
                    image,
                    annotations,
                    brightness,
                    contrast,
                    saturation,
                } => {
                    self.undo_stack.push(UndoEntry::Image {
                        image: self.image.clone(),
                        annotations: std::mem::take(&mut self.annotations),
                        brightness: self.brightness,
                        contrast: self.contrast,
                        saturation: self.saturation,
                    });
                    self.image = image;
                    self.annotations = annotations;
                    self.brightness = brightness;
                    self.contrast = contrast;
                    self.saturation = saturation;
                }
            }
        }
    }

    /// 图片级编辑：保存当前快照（供后续调整/滤镜/裁剪撤销）。
    fn begin_image_edit(&mut self) {
        self.undo_stack.push(UndoEntry::Image {
            image: self.image.clone(),
            annotations: self.annotations.clone(),
            brightness: self.brightness,
            contrast: self.contrast,
            saturation: self.saturation,
        });
        self.redo_stack.clear();
    }

    pub fn apply_crop(&mut self) {
        if let Some(c) = self.crop.take() {
            self.begin_image_edit();
            let cropped = self.image.crop_imm(c.x, c.y, c.w, c.h);
            self.image = cropped;
            self.annotations.clear();
        }
    }

    pub fn rotate90(&mut self) {
        self.begin_image_edit();
        self.image = self.image.rotate90();
    }
    pub fn rotate180(&mut self) {
        self.begin_image_edit();
        self.image = self.image.rotate180();
    }
    pub fn rotate270(&mut self) {
        self.begin_image_edit();
        self.image = self.image.rotate270();
    }
    pub fn flip_horizontal(&mut self) {
        self.begin_image_edit();
        self.image = self.image.fliph();
    }
    pub fn flip_vertical(&mut self) {
        self.begin_image_edit();
        self.image = self.image.flipv();
    }
    pub fn grayscale(&mut self) {
        self.begin_image_edit();
        self.image = self.image.grayscale();
    }
    pub fn invert(&mut self) {
        self.begin_image_edit();
        self.image.invert();
    }
    /// 实时预览调整：从基准图重新计算，不记录撤销。
    pub fn preview_adjustments(
        &mut self,
        base: &image::DynamicImage,
        brightness: i32,
        contrast: f32,
        saturation: f32,
    ) {
        let mut img = base.clone();
        img = img.brighten(brightness);
        img = img.adjust_contrast(contrast);
        img = saturate_image(&img, saturation);
        self.image = img;
    }
    /// 提交调整：把拖动前状态作为撤销快照，并确认新调整值（使滑块跟着回退）。
    pub fn commit_adjustments(
        &mut self,
        base: image::DynamicImage,
        old: (i32, f32, f32),
        new: (i32, f32, f32),
    ) {
        self.undo_stack.push(UndoEntry::Image {
            image: base,
            annotations: self.annotations.clone(),
            brightness: old.0,
            contrast: old.1,
            saturation: old.2,
        });
        self.redo_stack.clear();
        self.brightness = new.0;
        self.contrast = new.1;
        self.saturation = new.2;
    }
    pub fn blur(&mut self, sigma: f32) {
        self.begin_image_edit();
        self.image = self.image.blur(sigma);
    }
    pub fn unsharpen(&mut self, sigma: f32, threshold: i32) {
        self.begin_image_edit();
        self.image = self.image.unsharpen(sigma, threshold);
    }
    pub fn pixelate(&mut self, block_size: u32) {
        self.begin_image_edit();
        self.image = pixelate_image(&self.image, block_size);
    }

    pub fn render(&self, font: Option<&fontdue::Font>) -> image::RgbaImage {
        let mut out = self.image.to_rgba8();
        draw::rasterize_all(&self.annotations, &self.style, &mut out, font);
        out
    }
}

fn rgb_to_hsv(r: u8, g: u8, b: u8) -> (f32, f32, f32) {
    let (r, g, b) = (r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let v = max;
    let delta = max - min;
    let s = if max == 0.0 { 0.0 } else { delta / max };
    let h = if delta == 0.0 {
        0.0
    } else if max == r {
        60.0 * (((g - b) / delta) % 6.0)
    } else if max == g {
        60.0 * ((b - r) / delta + 2.0)
    } else {
        60.0 * ((r - g) / delta + 4.0)
    };
    (if h < 0.0 { h + 360.0 } else { h }, s, v)
}

fn hsv_to_rgb(h: f32, s: f32, v: f32) -> (u8, u8, u8) {
    let c = v * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let m = v - c;
    let (r, g, b) = match h {
        h if h < 60.0 => (c, x, 0.0),
        h if h < 120.0 => (x, c, 0.0),
        h if h < 180.0 => (0.0, c, x),
        h if h < 240.0 => (0.0, x, c),
        h if h < 300.0 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    (
        ((r + m) * 255.0).round() as u8,
        ((g + m) * 255.0).round() as u8,
        ((b + m) * 255.0).round() as u8,
    )
}

fn saturate_image(img: &image::DynamicImage, factor: f32) -> image::DynamicImage {
    let mut out = img.to_rgb8();
    for p in out.pixels_mut() {
        let (h, s, v) = rgb_to_hsv(p[0], p[1], p[2]);
        let s2 = (s * factor).clamp(0.0, 1.0);
        let (r, g, b) = hsv_to_rgb(h, s2, v);
        p[0] = r;
        p[1] = g;
        p[2] = b;
    }
    image::DynamicImage::ImageRgb8(out)
}

fn pixelate_image(img: &image::DynamicImage, block_size: u32) -> image::DynamicImage {
    let src = img.to_rgba8();
    let (w, h) = (src.width(), src.height());
    let mut out = image::RgbaImage::new(w, h);
    let bs = block_size.max(1);
    let mut by = 0;
    while by < h {
        let mut bx = 0;
        while bx < w {
            let ex = (bx + bs).min(w);
            let ey = (by + bs).min(h);
            let mut sum = [0u32; 4];
            let mut cnt = 0u32;
            for y in by..ey {
                for x in bx..ex {
                    let p = src.get_pixel(x, y);
                    sum[0] += p[0] as u32;
                    sum[1] += p[1] as u32;
                    sum[2] += p[2] as u32;
                    sum[3] += p[3] as u32;
                    cnt += 1;
                }
            }
            let avg = [
                (sum[0] / cnt) as u8,
                (sum[1] / cnt) as u8,
                (sum[2] / cnt) as u8,
                (sum[3] / cnt) as u8,
            ];
            for y in by..ey {
                for x in bx..ex {
                    out.put_pixel(x, y, image::Rgba(avg));
                }
            }
            bx += bs;
        }
        by += bs;
    }
    image::DynamicImage::ImageRgba8(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::draw::Annotation;

    fn doc() -> EditorDoc {
        let img = image::DynamicImage::ImageRgba8(image::RgbaImage::new(10, 10));
        EditorDoc::new(img)
    }

    #[test]
    fn undo_redo_annotation() {
        let mut d = doc();
        d.push_annotation(Annotation::Line {
            start: [0.0, 0.0],
            end: [5.0, 5.0],
        });
        assert_eq!(d.annotations.len(), 1);
        d.undo();
        assert_eq!(d.annotations.len(), 0);
        d.redo();
        assert_eq!(d.annotations.len(), 1);
    }

    #[test]
    fn apply_crop_shrinks_image() {
        let mut d = doc();
        d.crop = Some(CropRect {
            x: 2,
            y: 2,
            w: 4,
            h: 4,
        });
        d.apply_crop();
        assert_eq!(d.image.width(), 4);
        assert_eq!(d.image.height(), 4);
        assert!(d.crop.is_none());
    }

    #[test]
    fn crop_can_be_undone() {
        let mut d = doc();
        // 先加一个标注，验证裁剪撤销时标注也一并恢复
        d.push_annotation(Annotation::Line {
            start: [0.0, 0.0],
            end: [5.0, 5.0],
        });
        d.crop = Some(CropRect {
            x: 2,
            y: 2,
            w: 4,
            h: 4,
        });
        d.apply_crop();
        assert_eq!(d.image.width(), 4);
        assert_eq!(d.annotations.len(), 0);
        // 撤销裁剪：图像和标注都应恢复
        d.undo();
        assert_eq!(d.image.width(), 10);
        assert_eq!(d.image.height(), 10);
        assert_eq!(d.annotations.len(), 1);
        // 重做：再次裁剪
        d.redo();
        assert_eq!(d.image.width(), 4);
        assert_eq!(d.annotations.len(), 0);
    }

    #[test]
    fn render_composites_annotation() {
        let mut d = doc();
        d.style.color = [255, 0, 0, 255];
        d.style.width = 1.0;
        d.push_annotation(Annotation::Brush(vec![[1.0, 1.0], [2.0, 1.0]]));
        let out = d.render(None);
        assert_eq!(out.get_pixel(1, 1), &image::Rgba([255, 0, 0, 255]));
    }

    #[test]
    fn rotate90_swaps_dimensions_and_undo() {
        let img = image::DynamicImage::ImageRgb8(image::RgbImage::new(4, 2));
        let mut d = EditorDoc::new(img);
        d.rotate90();
        assert_eq!(d.image.width(), 2);
        assert_eq!(d.image.height(), 4);
        d.undo();
        assert_eq!(d.image.width(), 4);
        assert_eq!(d.image.height(), 2);
    }

    #[test]
    fn grayscale_reduces_color() {
        // 纯红图灰度后应接近灰
        let mut img = image::RgbImage::new(2, 2);
        for p in img.pixels_mut() {
            *p = image::Rgb([255, 0, 0]);
        }
        let mut d = EditorDoc::new(image::DynamicImage::ImageRgb8(img));
        d.grayscale();
        let out = d.image.to_rgb8();
        let p = out.get_pixel(0, 0);
        // 灰度后 r/g/b 相等（或近似）
        assert!((p[0] as i32 - p[1] as i32).abs() <= 2, "got {:?}", p);
        assert!((p[1] as i32 - p[2] as i32).abs() <= 2, "got {:?}", p);
    }

    #[test]
    fn commit_adjustments_undo_restores_values() {
        let img = image::DynamicImage::ImageRgb8(image::RgbImage::new(4, 4));
        let mut d = EditorDoc::new(img);
        let base = d.image.clone();
        // 亮度从 0 拖到 10
        d.preview_adjustments(&base, 10, 1.0, 1.0);
        d.commit_adjustments(base.clone(), (0, 1.0, 1.0), (10, 1.0, 1.0));
        assert_eq!(d.brightness, 10);
        // 撤销后亮度恢复 0，图像也恢复
        d.undo();
        assert_eq!(d.brightness, 0);
        assert_eq!(d.contrast, 1.0);
        assert_eq!(d.saturation, 1.0);
    }
}
