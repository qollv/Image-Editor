use std::path::Path;

use crate::draw::{self, Annotation, Shape, Style};

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
    /// 显示图版本号：每次 recompute +1。UI 层据此判断纹理是否需要重建。
    pub image_version: u64,
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
            image_version: 0,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
        }
    }

    /// 从底层图 + 调整值重新计算显示图。
    /// - 调整值全默认 (0, 1.0, 1.0) 时直接拷贝底层图（实测 12MP 默认值空跑
    ///   也要 320ms/帧，且旧实现在默认值下还会引入约 2% 的对比度偏移）；
    /// - 否则一次拷贝后原位应用各通道（只跑非默认项），三次全图遍历合并为
    ///   一次拷贝 + 按需原位处理（实测 12MP 从 979ms/帧降到约百毫秒级）；
    /// - 三个通道都只写 RGB（index 0..3），alpha 原样保留——调整不改变透明通道。
    pub fn recompute(&mut self) {
        let is_default =
            self.brightness == 0 && self.contrast == 1.0 && self.saturation == 1.0;
        self.image = if is_default {
            self.base_image.clone()
        } else {
            let mut out = self.base_image.to_rgba8();
            if self.brightness != 0 {
                apply_brightness(&mut out, self.brightness);
            }
            if self.contrast != 1.0 {
                apply_contrast(&mut out, self.contrast);
            }
            if self.saturation != 1.0 {
                apply_saturation(&mut out, self.saturation);
            }
            image::DynamicImage::ImageRgba8(out)
        };
        self.image_version += 1;
    }

    pub fn open(path: &Path) -> Result<Self, image::ImageError> {
        Ok(Self::new(image::open(path)?))
    }

    /// 新增一条标注：颜色/线宽取当前工具栏设置并自此定格，
    /// 之后改工具栏不影响已画的标注。
    pub fn push_annotation(&mut self, shape: Shape) {
        let style = self.style;
        self.undo_stack
            .push(UndoEntry::Annotations(self.annotations.clone()));
        self.redo_stack.clear();
        self.annotations.push(Annotation { shape, style });
    }

    /// 标注编辑前打快照（改马赛克强度等标注属性时用；一次拖动只打一个）。
    pub fn snapshot_annotations(&mut self) {
        self.undo_stack
            .push(UndoEntry::Annotations(self.annotations.clone()));
        self.redo_stack.clear();
    }

    pub fn can_undo(&self) -> bool {
        !self.undo_stack.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo_stack.is_empty()
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
        draw::rasterize_all(&self.annotations, &mut out, font);
        out
    }

    /// 底图只读访问（供调整滑块的低分预览降采样用）。
    pub fn base_image(&self) -> &image::DynamicImage {
        &self.base_image
    }

    /// 低分预览渲染：把当前调整值套在降采样底图上，马赛克坐标按比例缩放。
    /// 不动 `doc.image`——画布几何（标注/裁剪框）仍基于全分辨率坐标，
    /// 纹理被拉伸到全图尺寸显示，拖动期间观感与全质量一致（仅分辨率低）。
    pub fn render_preview(&self, preview: &AdjustPreview) -> image::RgbaImage {
        let mut img = preview.base.clone();
        apply_brightness(&mut img, self.brightness);
        apply_contrast(&mut img, self.contrast);
        apply_saturation(&mut img, self.saturation);
        let s = preview.scale;
        for a in &self.annotations {
            if let Shape::Mosaic { min, max, block } = &a.shape {
                let scaled = Shape::Mosaic {
                    min: [min[0] * s, min[1] * s],
                    max: [max[0] * s, max[1] * s],
                    // 块大小同步缩放，保持视觉占比
                    block: ((*block as f32) * s).round().max(1.0) as u32,
                };
                crate::draw::rasterize(&scaled, &a.style, &mut img, None);
            }
        }
        img
    }
}

/// 调整滑块拖动时的低分预览：拖动开始时把底图降采样一次（Lanczos3），
/// 之后每帧只在这张小图上套调整值——12MP 图从 ~161ms/帧 降到 ~15ms/帧。
/// 松手后由 UI 层走全质量路径（recompute + 全分辨率纹理）。
pub struct AdjustPreview {
    /// 降采样后的底图（RGBA）
    base: image::RgbaImage,
    /// 预览宽 / 原图宽（马赛克等坐标缩放用）
    scale: f32,
}

impl AdjustPreview {
    /// 最长边超过 1280 时降采样；否则原图拷贝（小图不降，避免无谓损耗）。
    pub fn new(base: &image::DynamicImage) -> Self {
        const PREVIEW_MAX_LONGEST: u32 = 1280;
        let (w, h) = (base.width(), base.height());
        let longest = w.max(h);
        let resized = if longest > PREVIEW_MAX_LONGEST {
            // thumbnail：快速降采样路径（整数二分+收尾），比 resize 快数倍
            base.thumbnail(PREVIEW_MAX_LONGEST, PREVIEW_MAX_LONGEST)
        } else {
            base.clone()
        };
        Self {
            scale: resized.width() as f32 / w as f32,
            base: resized.to_rgba8(),
        }
    }
}

/// 保存前预处理：JPEG/BMP 等不支持透明的格式，先把 alpha 合成到白底，
/// 避免透明区域直接变成不透明原色（与转换页的透明处理保持一致）。
pub fn prepare_for_save(img: image::RgbaImage, path: &Path) -> image::DynamicImage {
    let needs_flatten = matches!(
        image::ImageFormat::from_path(path),
        Ok(image::ImageFormat::Jpeg) | Ok(image::ImageFormat::Bmp)
    );
    let dyn_img = image::DynamicImage::ImageRgba8(img);
    if needs_flatten {
        crate::convert::flatten_alpha(dyn_img, [255, 255, 255])
    } else {
        dyn_img
    }
}

/// 亮度：整通道加减（数学同 image crate `brighten`，但只写 RGB，alpha 不动）。
fn apply_brightness(out: &mut image::RgbaImage, value: i32) {
    for p in out.pixels_mut() {
        for i in 0..3 {
            p[i] = (p[i] as i32 + value).clamp(0, 255) as u8;
        }
    }
}

/// 对比度：围绕中灰拉伸（数学同 image crate `contrast`，但只写 RGB，alpha 不动）。
/// 代数简化：((c/255 - 0.5) * percent + 0.5) * 255 = (c - 127.5) * percent + 127.5。
/// 无需 clamp：f32→u8 转换自身饱和（负→0，>255→255），截断语义与旧实现一致。
fn apply_contrast(out: &mut image::RgbaImage, contrast: f32) {
    let percent = ((100.0 + contrast) / 100.0).powi(2);
    for p in out.pixels_mut() {
        for i in 0..3 {
            let d = (p[i] as f32 - 127.5) * percent + 127.5;
            p[i] = d as u8;
        }
    }
}

/// 饱和度：HSV 空间缩放 S 的数学等价直算公式——
/// 记 min/max 为像素 RGB 的最小/最大值，则
/// `out = m1 + f * (in - min)`，其中 `m1 = f * min + (1 - f) * max`。
/// 与「rgb_to_hsv → 缩放 S → hsv_to_rgb」逐像素等价（含取整方式），
/// 但无除法、无取模（旧 HSV 实现含 f32 取模，实测 12MP 单遍 1148ms）。
/// 灰度像素（max==min）时公式自然退化为恒等，无需特判分支；
/// factor > 1 时按 S*factor 钳到 1 防溢出偏色（条件循环不变，无逐像素分支）。
/// 只写 RGB，alpha 不动。
fn apply_saturation(out: &mut image::RgbaImage, factor: f32) {
    if factor == 1.0 {
        return;
    }
    let boost = factor > 1.0;
    for p in out.pixels_mut() {
        let (r, g, b) = (p[0] as f32, p[1] as f32, p[2] as f32);
        let v = r.max(g).max(b);
        let m = r.min(g).min(b);
        // boost 时把等效饱和度钳到 1；d=0（灰度/黑色）时 v/d 为 inf 或 NaN，
        // f32::min 取非 NaN 侧得到 factor，公式退化为恒等，行为正确
        let f = if boost {
            factor.min(v / (v - m))
        } else {
            factor
        };
        let m1 = f * m + (1.0 - f) * v;
        // 值恒在 [0,255]，(x + 0.5) 截断即四舍五入；
        // 不用 f32::round()：默认 x86-64 目标无 SSE4.1，它会编译成 roundf 函数调用
        p[0] = (m1 + f * (r - m) + 0.5) as u8;
        p[1] = (m1 + f * (g - m) + 0.5) as u8;
        p[2] = (m1 + f * (b - m) + 0.5) as u8;
    }
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
    use crate::draw::Shape;

    fn doc() -> EditorDoc {
        let img = image::DynamicImage::ImageRgba8(image::RgbaImage::new(10, 10));
        EditorDoc::new(img)
    }

    #[test]
    fn undo_redo_annotation() {
        let mut d = doc();
        d.push_annotation(Shape::Line {
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
        d.push_annotation(Shape::Line {
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
        d.push_annotation(Shape::Brush(vec![[1.0, 1.0], [2.0, 1.0]]));
        let out = d.render(None);
        assert_eq!(out.get_pixel(1, 1), &image::Rgba([255, 0, 0, 255]));
    }

    // BUG1 回归：调整（亮度/对比度/饱和度）后透明通道必须保留
    #[test]
    fn recompute_preserves_alpha() {
        let mut img = image::RgbaImage::new(4, 4);
        for p in img.pixels_mut() {
            *p = image::Rgba([255, 0, 0, 0]); // 全透明红
        }
        let src = std::env::temp_dir().join("ic_alpha_src.png");
        img.save(&src).unwrap();

        let mut d = EditorDoc::open(&src).unwrap();
        assert!(d.image.color().has_alpha());
        // 模拟用户把三个调整滑块都拖离默认值
        d.brightness = 10;
        d.contrast = 2.0;
        d.saturation = 0.5;
        d.recompute();

        assert!(
            d.image.color().has_alpha(),
            "调整后颜色类型必须仍带 alpha"
        );
        let out = d.render(None);
        let p = out.get_pixel(0, 0);
        assert_eq!(p[3], 0, "全透明像素调整后必须仍全透明");
        // 饱和度 0.5 → 纯红被拉向灰色（R 仍最高，G/B 抬升且相等），说明调整确实生效
        assert_eq!(p[0], 255, "got {:?}", p);
        assert!(p[1] > 0 && p[1] < 200 && p[1] == p[2], "got {:?}", p);

        std::fs::remove_file(&src).ok();
    }

    // BUG2 回归：每条标注自带创建时的颜色/线宽，后续改工具栏不污染历史标注
    #[test]
    fn annotations_keep_their_own_style() {
        let mut d = doc();
        d.style.color = [255, 0, 0, 255];
        d.push_annotation(Shape::Line {
            start: [0.0, 0.0],
            end: [5.0, 5.0],
        });
        // 用户换成蓝色 + 粗线宽，再画一条
        d.style.color = [0, 0, 255, 255];
        d.style.width = 4.0;
        d.push_annotation(Shape::Line {
            start: [0.0, 6.0],
            end: [5.0, 9.0],
        });
        let out = d.render(None);
        // 第一条线必须还是红色
        assert_eq!(out.get_pixel(2, 2), &image::Rgba([255, 0, 0, 255]));
        // 第二条线是蓝色
        assert_eq!(out.get_pixel(2, 7), &image::Rgba([0, 0, 255, 255]));
    }

    // BUG3 回归：保存到 JPEG 前透明区域合成白底；PNG 不动 alpha
    #[test]
    fn prepare_for_save_flattens_jpeg_only() {
        let mut img = image::RgbaImage::new(4, 4);
        for p in img.pixels_mut() {
            *p = image::Rgba([0, 255, 0, 0]); // 全透明绿
        }

        let jpg = std::env::temp_dir().join("ic_save_probe.jpg");
        prepare_for_save(img.clone(), &jpg).save(&jpg).unwrap();
        let back = image::open(&jpg).unwrap().to_rgb8();
        let p = back.get_pixel(0, 0);
        assert!(
            p[0] > 240 && p[1] > 240 && p[2] > 240,
            "JPEG 透明区域应合成为白底, got {:?}",
            p
        );

        let png = std::env::temp_dir().join("ic_save_probe.png");
        prepare_for_save(img, &png).save(&png).unwrap();
        let back = image::open(&png).unwrap().to_rgba8();
        assert_eq!(back.get_pixel(0, 0)[3], 0, "PNG 必须保留透明通道");

        std::fs::remove_file(&jpg).ok();
        std::fs::remove_file(&png).ok();
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
        // 渐变图（不能再用全黑图：黑色是旧实现 2% 对比度偏移的固定点，测不出问题）
        let mut img = image::RgbImage::new(16, 4);
        for (x, y, p) in img.enumerate_pixels_mut() {
            *p = image::Rgb([(x * 16) as u8, (y * 60) as u8, 128]);
        }
        let base = img.clone();
        let mut d = EditorDoc::new(image::DynamicImage::ImageRgb8(img));
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
        // 归位后应精确等于原图（默认值必须是真正的无操作）
        assert_eq!(d.image.to_rgb8().as_raw(), base.as_raw());
    }

    #[test]
    fn adjustments_do_not_touch_alpha() {
        // 半透明像素：调整后 alpha 必须逐值不变（对比度也不能动 alpha）
        let mut img = image::RgbaImage::new(2, 1);
        img.put_pixel(0, 0, image::Rgba([200, 100, 50, 77]));
        img.put_pixel(1, 0, image::Rgba([10, 220, 90, 200]));
        let mut d = EditorDoc::new(image::DynamicImage::ImageRgba8(img));
        d.brightness = -20;
        d.contrast = 2.5;
        d.saturation = 0.3;
        d.recompute();
        let out = d.render(None);
        assert_eq!(out.get_pixel(0, 0)[3], 77);
        assert_eq!(out.get_pixel(1, 0)[3], 200);
    }

    #[test]
    fn recompute_bumps_image_version() {
        let mut d = EditorDoc::new(image::DynamicImage::ImageRgb8(image::RgbImage::new(4, 4)));
        let v0 = d.image_version;
        d.recompute();
        assert_eq!(d.image_version, v0 + 1);
        d.rotate90();
        assert!(d.image_version > v0 + 1);
    }

    #[test]
    fn mosaic_block_change_is_undoable() {
        let mut d = EditorDoc::new(image::DynamicImage::ImageRgba8(image::RgbaImage::new(20, 20)));
        d.push_annotation(Shape::Mosaic {
            min: [0.0, 0.0],
            max: [10.0, 10.0],
            block: 8,
        });
        // 模拟用户在工具栏把马赛克强度从 8 拖到 32
        d.snapshot_annotations();
        for a in &mut d.annotations {
            if let Shape::Mosaic { block, .. } = &mut a.shape {
                *block = 32;
            }
        }
        assert!(matches!(&d.annotations[0].shape, Shape::Mosaic { block: 32, .. }));
        // 一次撤销恢复拖动前
        d.undo();
        assert!(matches!(&d.annotations[0].shape, Shape::Mosaic { block: 8, .. }));
    }

    #[test]
    fn adjust_preview_downscales_large_image() {
        let base = image::DynamicImage::ImageRgba8(image::RgbaImage::new(4000, 3000));
        let pv = AdjustPreview::new(&base);
        // 最长边压到 1280，比例保持
        assert_eq!(pv.base.dimensions(), (1280, 960));
        assert!((pv.scale - 0.32).abs() < 1e-4);
    }

    #[test]
    fn adjust_preview_keeps_small_image() {
        let base = image::DynamicImage::ImageRgba8(image::RgbaImage::new(800, 600));
        let pv = AdjustPreview::new(&base);
        // 本来就不大：不降采样，scale=1
        assert_eq!(pv.base.dimensions(), (800, 600));
        assert_eq!(pv.scale, 1.0);
    }

    #[test]
    fn render_preview_matches_full_render_at_scale_one() {
        // scale=1（小图）时，预览渲染必须与全质量渲染逐像素一致
        let mut img = image::RgbaImage::new(64, 48);
        for (x, y, p) in img.enumerate_pixels_mut() {
            *p = image::Rgba([
                (x * 4) as u8,
                (y * 5) as u8,
                128,
                if x % 3 == 0 { 0 } else { 255 },
            ]);
        }
        let mut d = EditorDoc::new(image::DynamicImage::ImageRgba8(img));
        d.brightness = 15;
        d.contrast = 1.8;
        d.saturation = 0.6;
        d.recompute();
        d.push_annotation(Shape::Mosaic {
            min: [4.0, 4.0],
            max: [40.0, 30.0],
            block: 6,
        });
        let pv = AdjustPreview::new(d.base_image());
        assert_eq!(pv.scale, 1.0);
        let preview = d.render_preview(&pv);
        let full = d.render(None);
        assert_eq!(preview.dimensions(), full.dimensions());
        assert_eq!(preview.as_raw(), full.as_raw());
    }

    #[test]
    fn render_preview_applies_scaled_mosaic() {
        // 棋盘格大图（最长边超 1280 触发降采样）：马赛克区域块内应均匀，区域外保留细节
        let mut img = image::RgbaImage::new(2000, 1500);
        for (x, y, p) in img.enumerate_pixels_mut() {
            let c = if (x / 2 + y / 2) % 2 == 0 { 255 } else { 0 };
            *p = image::Rgba([c, c, c, 255]);
        }
        let mut d = EditorDoc::new(image::DynamicImage::ImageRgba8(img));
        d.push_annotation(Shape::Mosaic {
            min: [100.0, 100.0],
            max: [300.0, 200.0],
            block: 20,
        });
        let pv = AdjustPreview::new(d.base_image());
        assert!(pv.scale < 1.0);
        let preview = d.render_preview(&pv);
        let s = pv.scale;
        // 马赛克区左上角块（预览坐标 64..77 方形，块大小 20*0.64≈13）内必须同色
        let bx = (100.0 * s) as u32;
        let by = (100.0 * s) as u32;
        let p00 = *preview.get_pixel(bx + 3, by + 3);
        assert_eq!(p00, *preview.get_pixel(bx + 8, by + 6), "马赛克块内应均匀");
        assert_eq!(p00, *preview.get_pixel(bx + 6, by + 9), "马赛克块内应均匀");
        // 对照：区域外一行像素不应全部相同（证明图没被整体均匀化）
        let oy = (10.0 * s) as u32;
        let row: Vec<u8> = (2..60)
            .map(|x| preview.get_pixel((x as f32 / s) as u32, oy)[0])
            .collect();
        assert!(row.windows(2).any(|w| w[0] != w[1]), "区域外应保留细节");
    }

}
