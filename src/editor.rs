use std::path::Path;

use crate::draw::{self, Annotation, Style};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CropRect {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

/// 撤销栈条目。
enum UndoEntry {
    /// 标注操作（轻量快照）
    Annotations(Vec<Annotation>),
    /// 调整值变更（轻量快照，只存三个数值）
    Adjust {
        brightness: i32,
        contrast: f32,
        saturation: f32,
    },
    /// 图片级操作（旋转/裁剪/滤镜）：保存底层图 + 完整状态
    Image {
        base_image: image::DynamicImage,
        annotations: Vec<Annotation>,
        brightness: i32,
        contrast: f32,
        saturation: f32,
    },
}

pub struct EditorDoc {
    /// 底层原图：旋转/裁剪/滤镜作用于此，调整值不改变它
    base_image: image::DynamicImage,
    /// 显示图 = base_image + 亮度/对比度/饱和度（实时计算）
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
            base_image: image.clone(),
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

    /// 从底层图 + 调整值重新计算显示图。
    pub fn recompute(&mut self) {
        let mut img = self.base_image.clone();
        img = img.brighten(self.brightness);
        img = img.adjust_contrast(self.contrast);
        img = saturate_image(&img, self.saturation);
        self.image = img;
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
                UndoEntry::Adjust {
                    brightness,
                    contrast,
                    saturation,
                } => {
                    self.redo_stack.push(UndoEntry::Adjust {
                        brightness: self.brightness,
                        contrast: self.contrast,
                        saturation: self.saturation,
                    });
                    self.brightness = brightness;
                    self.contrast = contrast;
                    self.saturation = saturation;
                    self.recompute();
                }
                UndoEntry::Image {
                    base_image,
                    annotations,
                    brightness,
                    contrast,
                    saturation,
                } => {
                    self.redo_stack.push(UndoEntry::Image {
                        base_image: self.base_image.clone(),
                        annotations: std::mem::take(&mut self.annotations),
                        brightness: self.brightness,
                        contrast: self.contrast,
                        saturation: self.saturation,
                    });
                    self.base_image = base_image;
                    self.annotations = annotations;
                    self.brightness = brightness;
                    self.contrast = contrast;
                    self.saturation = saturation;
                    self.recompute();
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
                UndoEntry::Adjust {
                    brightness,
                    contrast,
                    saturation,
                } => {
                    self.undo_stack.push(UndoEntry::Adjust {
                        brightness: self.brightness,
                        contrast: self.contrast,
                        saturation: self.saturation,
                    });
                    self.brightness = brightness;
                    self.contrast = contrast;
                    self.saturation = saturation;
                    self.recompute();
                }
                UndoEntry::Image {
                    base_image,
                    annotations,
                    brightness,
                    contrast,
                    saturation,
                } => {
                    self.undo_stack.push(UndoEntry::Image {
                        base_image: self.base_image.clone(),
                        annotations: std::mem::take(&mut self.annotations),
                        brightness: self.brightness,
                        contrast: self.contrast,
                        saturation: self.saturation,
                    });
                    self.base_image = base_image;
                    self.annotations = annotations;
                    self.brightness = brightness;
                    self.contrast = contrast;
                    self.saturation = saturation;
                    self.recompute();
                }
            }
        }
    }

    /// 图片级编辑：保存底层图快照（供后续旋转/裁剪/滤镜撤销）。
    fn begin_image_edit(&mut self) {
        self.undo_stack.push(UndoEntry::Image {
            base_image: self.base_image.clone(),
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
            self.base_image = self.base_image.crop_imm(c.x, c.y, c.w, c.h);
            self.annotations.clear();
            self.recompute();
        }
    }

    pub fn rotate90(&mut self) {
        self.begin_image_edit();
        self.base_image = self.base_image.rotate90();
        self.recompute();
    }
    pub fn rotate180(&mut self) {
        self.begin_image_edit();
        self.base_image = self.base_image.rotate180();
        self.recompute();
    }
    pub fn rotate270(&mut self) {
        self.begin_image_edit();
        self.base_image = self.base_image.rotate270();
        self.recompute();
    }
    pub fn flip_horizontal(&mut self) {
        self.begin_image_edit();
        self.base_image = self.base_image.fliph();
        self.recompute();
    }
    pub fn flip_vertical(&mut self) {
        self.begin_image_edit();
        self.base_image = self.base_image.flipv();
        self.recompute();
    }
    pub fn grayscale(&mut self) {
        self.begin_image_edit();
        self.base_image = self.base_image.grayscale();
        self.recompute();
    }
    pub fn invert(&mut self) {
        self.begin_image_edit();
        self.base_image.invert();
        self.recompute();
    }
    /// 提交调整：把拖动前的调整值作为撤销快照（新值已由滑块写入并实时预览）。
    pub fn commit_adjust(&mut self, old: (i32, f32, f32)) {
        self.undo_stack.push(UndoEntry::Adjust {
            brightness: old.0,
            contrast: old.1,
            saturation: old.2,
        });
        self.redo_stack.clear();
    }
    pub fn blur(&mut self, sigma: f32) {
        self.begin_image_edit();
        self.base_image = self.base_image.blur(sigma);
        self.recompute();
    }
    pub fn unsharpen(&mut self, sigma: f32, threshold: i32) {
        self.begin_image_edit();
        self.base_image = self.base_image.unsharpen(sigma, threshold);
        self.recompute();
    }
    pub fn pixelate(&mut self, block_size: u32) {
        self.begin_image_edit();
        self.base_image = pixelate_image(&self.base_image, block_size);
        self.recompute();
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
    fn commit_adjust_undo_restores_values() {
        let img = image::DynamicImage::ImageRgb8(image::RgbImage::new(4, 4));
        let mut d = EditorDoc::new(img);
        // 亮度从 0 拖到 10
        d.brightness = 10;
        d.recompute();
        d.commit_adjust((0, 1.0, 1.0));
        assert_eq!(d.brightness, 10);
        // 撤销后亮度恢复 0
        d.undo();
        assert_eq!(d.brightness, 0);
        assert_eq!(d.contrast, 1.0);
        assert_eq!(d.saturation, 1.0);
    }

    #[test]
    fn zero_adjustments_equals_base_image() {
        let img = image::DynamicImage::ImageRgb8(image::RgbImage::new(4, 4));
        let base = img.clone();
        let mut d = EditorDoc::new(img);
        // 调亮度 10、对比度 2、饱和度 1.5
        d.brightness = 10;
        d.contrast = 2.0;
        d.saturation = 1.5;
        d.recompute();
        // 再全部归位
        d.brightness = 0;
        d.contrast = 1.0;
        d.saturation = 1.0;
        d.recompute();
        // 归位后应精确等于原图
        assert_eq!(d.image.to_rgb8().as_raw(), base.to_rgb8().as_raw());
    }
}
