use std::path::PathBuf;

use eframe::egui;

use crate::draw::{Annotation, Style};
use crate::editor::{CropRect, EditorDoc};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tool {
    Select,
    Brush,
    Line,
    Arrow,
    Rect,
    Ellipse,
    Text,
    Mosaic,
    Highlight,
    Crop,
    CropRatio,
}

pub struct EditorView {
    doc: Option<EditorDoc>,
    path: Option<PathBuf>,
    dirty: bool,
    tool: Tool,
    crop_ratio: (u32, u32),
    brightness: i32,
    contrast: f32,
    saturation: f32,
    mosaic_block: u32,
    zoom: f32,
    pan: egui::Vec2,
    font: fontdue::Font,
    texture: Option<egui::TextureHandle>,
    drawing: bool,
    start_img: egui::Pos2,
    cur_img: egui::Pos2,
    brush_pts: Vec<[f32; 2]>,
    text_input: String,
    text_pos: Option<[f32; 2]>,
    text_screen_pos: Option<egui::Pos2>,
    selected: Option<usize>,
    drag_annot: bool,
    drag_start_img: egui::Pos2,
    drag_orig_pos: [f32; 2],
    save_dialog: bool,
    confirm_open: bool,
    status: String,
}

fn load_font() -> fontdue::Font {
    let paths = [
        "C:\\Windows\\Fonts\\msyh.ttc",
        "C:\\Windows\\Fonts\\simhei.ttf",
        "C:\\Windows\\Fonts\\simsun.ttc",
    ];
    for p in paths {
        if let Ok(bytes) = std::fs::read(p) {
            let settings = fontdue::FontSettings {
                collection_index: 0,
                ..Default::default()
            };
            if let Ok(f) = fontdue::Font::from_bytes(bytes, settings) {
                return f;
            }
        }
    }
    // 回退：从 arial 复制（Windows 一定有）
    if let Ok(bytes) = std::fs::read("C:\\Windows\\Fonts\\arial.ttf") {
        if let Ok(f) = fontdue::Font::from_bytes(bytes, fontdue::FontSettings::default()) {
            return f;
        }
    }
    panic!("无法加载任何字体");
}

impl EditorView {
    pub fn new() -> Self {
        Self {
            doc: None,
            path: None,
            dirty: false,
            tool: Tool::Select,
            crop_ratio: (1, 1),
            brightness: 0,
            contrast: 1.0,
            saturation: 1.0,
            mosaic_block: 16,
            zoom: 1.0,
            pan: egui::Vec2::ZERO,
            font: load_font(),
            texture: None,
            drawing: false,
            start_img: egui::Pos2::ZERO,
            cur_img: egui::Pos2::ZERO,
            brush_pts: Vec::new(),
            text_input: String::new(),
            text_pos: None,
            text_screen_pos: None,
            selected: None,
            drag_annot: false,
            drag_start_img: egui::Pos2::ZERO,
            drag_orig_pos: [0.0, 0.0],
            save_dialog: false,
            confirm_open: false,
            status: "未打开图片".to_string(),
        }
    }

    fn pick_and_open(&mut self, ctx: &egui::Context) {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("图片", &["png", "jpg", "jpeg", "webp", "gif", "bmp", "tiff", "tga"])
            .pick_file()
        {
            self.open_image(ctx, path);
        }
    }

    fn open_image(&mut self, ctx: &egui::Context, path: PathBuf) {
        match EditorDoc::open(&path) {
            Ok(doc) => {
                self.texture = Some(self.make_texture(ctx, &doc.image));
                self.doc = Some(doc);
                self.path = Some(path);
                self.dirty = false;
                self.zoom = 1.0;
                self.pan = egui::Vec2::ZERO;
                self.brush_pts.clear();
                self.text_pos = None;
                self.status = "已打开图片".to_string();
            }
            Err(e) => {
                self.status = format!("打开失败: {}", e);
            }
        }
    }

    fn make_texture(&self, ctx: &egui::Context, img: &image::DynamicImage) -> egui::TextureHandle {
        let rgba = img.to_rgba8();
        let size = [rgba.width() as usize, rgba.height() as usize];
        let color = egui::ColorImage::from_rgba_unmultiplied(size, rgba.as_raw());
        ctx.load_texture("editor_img", color, egui::TextureOptions::LINEAR)
    }

    fn refresh_texture(&mut self, ctx: &egui::Context) {
        if let Some(doc) = &self.doc {
            self.texture = Some(self.make_texture(ctx, &doc.image));
        }
    }

    fn img_to_screen(&self, p: egui::Pos2, origin: egui::Pos2) -> egui::Pos2 {
        origin + self.pan + egui::vec2(p.x, p.y) * self.zoom
    }

    fn screen_to_img(&self, p: egui::Pos2, origin: egui::Pos2) -> egui::Pos2 {
        egui::pos2(
            (p.x - origin.x - self.pan.x) / self.zoom,
            (p.y - origin.y - self.pan.y) / self.zoom,
        )
    }

    /// 命中检测：返回给定图片坐标命中的文字标注索引（从后往前）。
    fn hit_test(&self, p: egui::Pos2) -> Option<usize> {
        let doc = self.doc.as_ref()?;
        let style = doc.style;
        for (i, a) in doc.annotations.iter().enumerate().rev() {
            if let Some(b) = crate::draw::bbox(a, &style) {
                if p.x >= b[0] && p.x <= b[0] + b[2] && p.y >= b[1] && p.y <= b[1] + b[3] {
                    return Some(i);
                }
            }
        }
        None
    }

    /// 生成图片内居中的、能容纳的最大比例裁剪框。
    fn make_ratio_crop(ratio: (u32, u32), doc: &EditorDoc) -> CropRect {
        let (rw, rh) = ratio;
        let img_w = doc.image.width() as f32;
        let img_h = doc.image.height() as f32;
        let scale = (img_w / rw as f32).min(img_h / rh as f32);
        let w = (scale * rw as f32).round() as u32;
        let h = (scale * rh as f32).round() as u32;
        CropRect {
            x: (doc.image.width() - w) / 2,
            y: (doc.image.height() - h) / 2,
            w,
            h,
        }
    }

    fn tool_label(t: Tool) -> &'static str {
        match t {
            Tool::Select => "选择",
            Tool::Brush => "画笔",
            Tool::Line => "直线",
            Tool::Arrow => "箭头",
            Tool::Rect => "矩形",
            Tool::Ellipse => "椭圆",
            Tool::Text => "文字",
            Tool::Mosaic => "马赛克",
            Tool::Highlight => "荧光笔",
            Tool::Crop => "裁剪",
            Tool::CropRatio => "比例裁剪",
        }
    }

    fn style(&self) -> Style {
        self.doc
            .as_ref()
            .map(|d| d.style)
            .unwrap_or(Style {
                color: [255, 0, 0, 255],
                width: 2.0,
            })
    }

    fn color32(c: [u8; 4]) -> egui::Color32 {
        egui::Color32::from_rgba_unmultiplied(c[0], c[1], c[2], c[3])
    }

    fn draw_annotation(&self, painter: &egui::Painter, a: &Annotation, style: &Style, origin: egui::Pos2) {
        let c = Self::color32(style.color);
        let stroke = egui::Stroke::new(style.width.max(1.0), c);
        match a {
            Annotation::Brush(pts) => {
                if pts.len() == 1 {
                    let p = self.img_to_screen(egui::pos2(pts[0][0], pts[0][1]), origin);
                    painter.circle_stroke(p, style.width.max(1.0) / 2.0, stroke);
                }
                for w in pts.windows(2) {
                    let p0 = self.img_to_screen(egui::pos2(w[0][0], w[0][1]), origin);
                    let p1 = self.img_to_screen(egui::pos2(w[1][0], w[1][1]), origin);
                    painter.line_segment([p0, p1], stroke);
                }
            }
            Annotation::Line { start, end } => {
                let p0 = self.img_to_screen(egui::pos2(start[0], start[1]), origin);
                let p1 = self.img_to_screen(egui::pos2(end[0], end[1]), origin);
                painter.line_segment([p0, p1], stroke);
            }
            Annotation::Arrow { start, end } => {
                let p0 = self.img_to_screen(egui::pos2(start[0], start[1]), origin);
                let p1 = self.img_to_screen(egui::pos2(end[0], end[1]), origin);
                let dir = (p1 - p0).normalized();
                let head = 14.0;
                let perp = egui::vec2(-dir.y, dir.x);
                let base = p1 - dir * head;
                // 主线：start → 基底中心
                painter.line_segment([p0, base], stroke);
                // 头部三角：两个底角 → 尖端
                let c1 = base + perp * head * 0.5;
                let c2 = base - perp * head * 0.5;
                painter.line_segment([c1, p1], stroke);
                painter.line_segment([c2, p1], stroke);
            }
            Annotation::Rect { min, max } => {
                let a = self.img_to_screen(egui::pos2(min[0], min[1]), origin);
                let b = self.img_to_screen(egui::pos2(max[0], max[1]), origin);
                painter.rect_stroke(egui::Rect::from_two_pos(a, b), 0.0, stroke, egui::StrokeKind::Middle);
            }
            Annotation::Ellipse { min, max } => {
                let a = self.img_to_screen(egui::pos2(min[0], min[1]), origin);
                let b = self.img_to_screen(egui::pos2(max[0], max[1]), origin);
                let center = egui::pos2((a.x + b.x) / 2.0, (a.y + b.y) / 2.0);
                let radius = egui::vec2((b.x - a.x).abs() / 2.0, (b.y - a.y).abs() / 2.0);
                painter.add(egui::Shape::ellipse_stroke(center, radius, stroke));
            }
            Annotation::Mosaic { min, max, .. } => {
                let a = self.img_to_screen(egui::pos2(min[0], min[1]), origin);
                let b = self.img_to_screen(egui::pos2(max[0], max[1]), origin);
                let rect = egui::Rect::from_two_pos(a, b);
                // 半透明填充表示遮挡区域
                painter.rect_filled(rect, 0.0, egui::Color32::from_rgba_unmultiplied(120, 120, 120, 90));
                painter.rect_stroke(rect, 0.0, stroke, egui::StrokeKind::Middle);
            }
            Annotation::Highlight(pts) => {
                let hc = egui::Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), 180);
                let hstroke = egui::Stroke::new(style.width.max(8.0), hc);
                if pts.len() == 1 {
                    let p = self.img_to_screen(egui::pos2(pts[0][0], pts[0][1]), origin);
                    painter.circle_stroke(p, style.width.max(8.0) / 2.0, hstroke);
                }
                for w in pts.windows(2) {
                    let p0 = self.img_to_screen(egui::pos2(w[0][0], w[0][1]), origin);
                    let p1 = self.img_to_screen(egui::pos2(w[1][0], w[1][1]), origin);
                    painter.line_segment([p0, p1], hstroke);
                }
            }
            Annotation::Text { pos, content, size } => {
                let p = self.img_to_screen(egui::pos2(pos[0], pos[1]), origin);
                painter.text(
                    p,
                    egui::Align2::LEFT_BOTTOM,
                    content,
                    egui::FontId::proportional(*size * self.zoom),
                    c,
                );
            }
        }
    }

    fn draw_preview(&self, painter: &egui::Painter, origin: egui::Pos2) {
        if !self.drawing {
            return;
        }
        let style = self.style();
        let c = Self::color32(style.color);
        let stroke = egui::Stroke::new(style.width.max(1.0), c);
        let a = self.img_to_screen(self.start_img, origin);
        let b = self.img_to_screen(self.cur_img, origin);
        match self.tool {
            Tool::Line => {
                painter.line_segment([a, b], stroke);
            }
            Tool::Arrow => {
                let dir = (b - a).normalized();
                let perp = egui::vec2(-dir.y, dir.x);
                let head = 14.0;
                let base = b - dir * head;
                painter.line_segment([a, base], stroke);
                let c1 = base + perp * head * 0.5;
                let c2 = base - perp * head * 0.5;
                painter.line_segment([c1, b], stroke);
                painter.line_segment([c2, b], stroke);
            }
            Tool::Rect | Tool::Ellipse | Tool::Mosaic => {
                let rect = egui::Rect::from_two_pos(a, b);
                if self.tool == Tool::Ellipse {
                    let center = rect.center();
                    let radius = egui::vec2(rect.width() / 2.0, rect.height() / 2.0);
                    painter.add(egui::Shape::ellipse_stroke(center, radius, stroke));
                } else {
                    painter.rect_stroke(rect, 0.0, stroke, egui::StrokeKind::Middle);
                }
            }
            Tool::Crop | Tool::CropRatio => {
                painter.rect_stroke(
                    egui::Rect::from_two_pos(a, b),
                    0.0,
                    egui::Stroke::new(1.5_f32, egui::Color32::from_rgb(0, 120, 255)),
                    egui::StrokeKind::Middle,
                );
            }
            Tool::Brush => {
                // 实时画轨迹（按下即可见，无需等松开）
                let pts: Vec<egui::Pos2> = self
                    .brush_pts
                    .iter()
                    .map(|p| self.img_to_screen(egui::pos2(p[0], p[1]), origin))
                    .collect();
                if pts.len() == 1 {
                    painter.circle_stroke(pts[0], style.width.max(1.0) / 2.0, stroke);
                }
                for w in pts.windows(2) {
                    painter.line_segment([w[0], w[1]], stroke);
                }
            }
            Tool::Highlight => {
                let hc = egui::Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), 180);
                let hstroke = egui::Stroke::new(style.width.max(8.0), hc);
                let pts: Vec<egui::Pos2> = self
                    .brush_pts
                    .iter()
                    .map(|p| self.img_to_screen(egui::pos2(p[0], p[1]), origin))
                    .collect();
                if pts.len() == 1 {
                    painter.circle_stroke(pts[0], style.width.max(8.0) / 2.0, hstroke);
                }
                for w in pts.windows(2) {
                    painter.line_segment([w[0], w[1]], hstroke);
                }
            }
            _ => {}
        }
    }

    fn commit_drawing(&mut self) {
        if !self.drawing {
            return;
        }
        self.drawing = false;
        let doc = match &mut self.doc {
            Some(d) => d,
            None => return,
        };
        let (sx, sy) = (self.start_img.x, self.start_img.y);
        let (cx, cy) = (self.cur_img.x, self.cur_img.y);
        let ann = match self.tool {
            Tool::Brush => {
                if self.brush_pts.is_empty() {
                    return;
                }
                let pts = std::mem::take(&mut self.brush_pts);
                Annotation::Brush(pts)
            }
            Tool::Line => Annotation::Line {
                start: [sx, sy],
                end: [cx, cy],
            },
            Tool::Arrow => Annotation::Arrow {
                start: [sx, sy],
                end: [cx, cy],
            },
            Tool::Rect => Annotation::Rect {
                min: [sx.min(cx), sy.min(cy)],
                max: [sx.max(cx), sy.max(cy)],
            },
            Tool::Ellipse => Annotation::Ellipse {
                min: [sx.min(cx), sy.min(cy)],
                max: [sx.max(cx), sy.max(cy)],
            },
            Tool::Mosaic => Annotation::Mosaic {
                min: [sx.min(cx), sy.min(cy)],
                max: [sx.max(cx), sy.max(cy)],
                block: self.mosaic_block,
            },
            Tool::Highlight => {
                if self.brush_pts.is_empty() {
                    return;
                }
                let pts = std::mem::take(&mut self.brush_pts);
                Annotation::Highlight(pts)
            }
            Tool::Crop | Tool::CropRatio => {
                let (x0, y0) = (sx.min(cx), sy.min(cy));
                let (x1, y1) = (sx.max(cx), sy.max(cy));
                // 四舍五入到像素，使裁剪框边缘与鼠标位置对齐
                let x0 = x0.max(0.0).round() as u32;
                let y0 = y0.max(0.0).round() as u32;
                let x1 = x1.min(doc.image.width() as f32).round() as u32;
                let y1 = y1.min(doc.image.height() as f32).round() as u32;
                doc.crop = Some(CropRect {
                    x: x0,
                    y: y0,
                    w: x1.saturating_sub(x0).max(1),
                    h: y1.saturating_sub(y0).max(1),
                });
                return;
            }
            _ => return,
        };
        doc.push_annotation(ann);
        self.dirty = true;
    }

    fn save_as(&mut self, path: &PathBuf) {
        let doc = match &self.doc {
            Some(d) => d,
            None => return,
        };
        let out = doc.render(Some(&self.font));
        match out.save(path) {
            Ok(()) => {
                self.status = format!("已保存: {}", path.display());
                self.dirty = false;
            }
            Err(e) => {
                self.status = format!("保存失败: {}", e);
            }
        }
    }

    fn save_overwrite(&mut self) {
        let path = match &self.path {
            Some(p) => p.clone(),
            None => return,
        };
        self.save_as(&path);
    }

    /// 执行图片级操作并刷新画布纹理。
    fn apply_image_op(&mut self, ctx: &egui::Context, op: impl FnOnce(&mut EditorDoc)) {
        if self.doc.is_none() {
            return;
        }
        {
            let doc = self.doc.as_mut().unwrap();
            op(doc);
            self.dirty = true;
        }
        self.refresh_texture(ctx);
    }

    fn show_adjust_panel(&mut self, ui: &mut egui::Ui) {
        if self.doc.is_none() {
            return;
        }
        let ctx = ui.ctx().clone();
        egui::CollapsingHeader::new("图片调整与滤镜")
            .default_open(false)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    if ui.button("旋转 90°").clicked() {
                        self.apply_image_op(&ctx, |d| d.rotate90());
                    }
                    if ui.button("旋转 180°").clicked() {
                        self.apply_image_op(&ctx, |d| d.rotate180());
                    }
                    if ui.button("旋转 270°").clicked() {
                        self.apply_image_op(&ctx, |d| d.rotate270());
                    }
                    if ui.button("水平翻转").clicked() {
                        self.apply_image_op(&ctx, |d| d.flip_horizontal());
                    }
                    if ui.button("垂直翻转").clicked() {
                        self.apply_image_op(&ctx, |d| d.flip_vertical());
                    }
                });
                ui.horizontal(|ui| {
                    if ui.button("灰度").clicked() {
                        self.apply_image_op(&ctx, |d| d.grayscale());
                    }
                    if ui.button("反色").clicked() {
                        self.apply_image_op(&ctx, |d| d.invert());
                    }
                });
                ui.horizontal(|ui| {
                    ui.label("亮度:");
                    ui.add(egui::Slider::new(&mut self.brightness, -100..=100));
                    ui.label("对比度:");
                    ui.add(egui::Slider::new(&mut self.contrast, 0.0..=3.0));
                    ui.label("饱和度:");
                    ui.add(egui::Slider::new(&mut self.saturation, 0.0..=3.0));
                    if ui.button("应用调整").clicked() {
                        let (b, c, s) = (self.brightness, self.contrast, self.saturation);
                        self.apply_image_op(&ctx, |d| d.apply_adjustments(b, c, s));
                        self.brightness = 0;
                        self.contrast = 1.0;
                        self.saturation = 1.0;
                    }
                });
                ui.horizontal(|ui| {
                    if ui.button("高斯模糊").clicked() {
                        self.apply_image_op(&ctx, |d| d.blur(2.0));
                    }
                    if ui.button("锐化").clicked() {
                        self.apply_image_op(&ctx, |d| d.unsharpen(1.5, 10));
                    }
                    if ui.button("像素化").clicked() {
                        self.apply_image_op(&ctx, |d| d.pixelate(8));
                    }
                });
            });
    }

    fn show_save_dialog(&mut self, ctx: &egui::Context) {
        let mut close = false;
        egui::Window::new("保存图片")
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                if ui.button("另存为新文件…").clicked() {
                    if let Some(path) = rfd::FileDialog::new()
                        .add_filter("PNG", &["png"])
                        .add_filter("JPEG", &["jpg", "jpeg"])
                        .add_filter("WebP", &["webp"])
                        .save_file()
                    {
                        self.save_as(&path);
                        close = true;
                    }
                }
                if self.path.is_some() && ui.button("覆盖原图").clicked() {
                    self.save_overwrite();
                    close = true;
                }
                if ui.button("取消").clicked() {
                    close = true;
                }
            });
        if close {
            self.save_dialog = false;
        }
    }

    pub fn ui(&mut self, ui: &mut egui::Ui) {
        // 工具栏
        ui.horizontal(|ui| {
            if ui.button("打开图片…").clicked() {
                if self.dirty {
                    self.confirm_open = true;
                } else {
                    self.pick_and_open(ui.ctx());
                }
            }
            ui.separator();
            for t in [
                Tool::Select,
                Tool::Brush,
                Tool::Line,
                Tool::Arrow,
                Tool::Rect,
                Tool::Ellipse,
                Tool::Text,
                Tool::Mosaic,
                Tool::Highlight,
                Tool::Crop,
                Tool::CropRatio,
            ] {
                ui.selectable_value(&mut self.tool, t, Self::tool_label(t));
            }
        });

        ui.horizontal(|ui| {
            if let Some(doc) = &mut self.doc {
                let mut color = egui::Color32::from_rgba_unmultiplied(
                    doc.style.color[0],
                    doc.style.color[1],
                    doc.style.color[2],
                    doc.style.color[3],
                );
                ui.label("颜色:");
                ui.color_edit_button_srgba(&mut color);
                doc.style.color = color.to_array();
                ui.label("线宽:");
                ui.add(egui::Slider::new(&mut doc.style.width, 1.0..=20.0));
            }
            if self.tool == Tool::CropRatio {
                ui.label("比例:");
                let ratios = [(1, 1), (4, 3), (16, 9)];
                for (w, h) in ratios {
                    if ui.selectable_label(self.crop_ratio == (w, h), format!("{}:{}", w, h)).clicked() {
                        self.crop_ratio = (w, h);
                        // 选比例后立即生成居中预览框，等用户点「应用裁剪」才真正裁剪
                        if let Some(doc) = &mut self.doc {
                            doc.crop = Some(Self::make_ratio_crop((w, h), doc));
                        }
                    }
                }
            }
            ui.separator();
            let has_doc = self.doc.is_some();
            if self.tool == Tool::Mosaic {
                ui.label("马赛克强度:");
                ui.add(egui::Slider::new(&mut self.mosaic_block, 4..=64));
            }
            if ui.add_enabled(has_doc, egui::Button::new("撤销")).clicked() {
                {
                    let d = self.doc.as_mut().unwrap();
                    d.undo();
                    self.dirty = true;
                }
                // 撤销的可能是裁剪，恢复原图后需要刷新画布纹理
                self.refresh_texture(ui.ctx());
            }
            if ui.add_enabled(has_doc, egui::Button::new("清空标注")).clicked() {
                if let Some(d) = &mut self.doc {
                    d.annotations.clear();
                    self.dirty = true;
                }
            }
            if self.tool == Tool::Crop || self.tool == Tool::CropRatio {
                if ui.add_enabled(has_doc, egui::Button::new("应用裁剪")).clicked() {
                    if let Some(d) = &mut self.doc {
                        if d.crop.is_some() {
                            d.apply_crop();
                            self.dirty = true;
                            self.refresh_texture(ui.ctx());
                        }
                    }
                }
            }
            ui.separator();
            if ui.add_enabled(has_doc, egui::Button::new("保存")).clicked() {
                self.save_dialog = true;
            }
        });

        // 图片调整与滤镜面板
        self.show_adjust_panel(ui);

        // 就地文字输入框（显示在点击位置）
        if let Some(screen_pos) = self.text_screen_pos {
            let mut done = false;
            let mut cancel = false;
            egui::Area::new(egui::Id::new("text_input_area"))
                .fixed_pos(screen_pos)
                .order(egui::Order::Foreground)
                .show(ui.ctx(), |ui| {
                    egui::Frame::popup(ui.style()).show(ui, |ui| {
                        ui.set_min_width(220.0);
                        ui.label("输入文字（回车确认）:");
                        let resp = ui.add(
                            egui::TextEdit::singleline(&mut self.text_input).hint_text("文字内容"),
                        );
                        resp.request_focus();
                        let enter = resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                        ui.horizontal(|ui| {
                            if ui.button("确定").clicked() || enter {
                                done = true;
                            }
                            if ui.button("取消").clicked() {
                                cancel = true;
                            }
                        });
                    });
                });
            if done {
                if !self.text_input.is_empty() {
                    if let (Some(doc), Some(pos)) = (&mut self.doc, self.text_pos) {
                        doc.push_annotation(Annotation::Text {
                            pos,
                            content: std::mem::take(&mut self.text_input),
                            size: 24.0,
                        });
                        self.dirty = true;
                        self.selected = Some(doc.annotations.len() - 1);
                    }
                }
                self.text_screen_pos = None;
                self.text_pos = None;
            }
            if cancel {
                self.text_screen_pos = None;
                self.text_pos = None;
            }
        }

        // 画布
        let available = ui.available_size();
        let (response, painter) =
            ui.allocate_painter(available.max(egui::vec2(200.0, 200.0)), egui::Sense::click_and_drag());
        let rect = response.rect;

        // 缩放（围绕鼠标位置）；选中文字标注时滚轮改为调整字号
        let zoom_delta = ui.input(|i| i.zoom_delta());
        if response.hovered() && (zoom_delta - 1.0).abs() > 1e-4 {
            if self.selected.is_some() {
                if let (Some(doc), Some(i)) = (&mut self.doc, self.selected) {
                    if let Some(Annotation::Text { size, .. }) = doc.annotations.get_mut(i) {
                        *size = (*size * zoom_delta).clamp(6.0, 200.0);
                        self.dirty = true;
                    }
                }
            } else if let Some(hover) = response.hover_pos() {
                let old_origin = rect.min + self.pan;
                let before = self.screen_to_img(hover, old_origin);
                self.zoom = (self.zoom * zoom_delta).clamp(0.05, 32.0);
                let after = self.img_to_screen(before, old_origin);
                self.pan += hover - after;
            }
        }
        // 平移（右键拖动）
        if response.dragged_by(egui::PointerButton::Secondary) {
            self.pan += response.drag_delta();
        }

        // 最终画布原点（缩放/平移之后）
        let origin = rect.min + self.pan;

        // 鼠标交互：按下立即开始绘制（无拖动阈值），先更新坐标再绘制，消除一帧滞后
        let is_draw_tool = matches!(
            self.tool,
            Tool::Brush
                | Tool::Line
                | Tool::Arrow
                | Tool::Rect
                | Tool::Ellipse
                | Tool::Mosaic
                | Tool::Highlight
                | Tool::Crop
                | Tool::CropRatio
        );
        let (pressed, released, down) = ui.input(|i| {
            (
                i.pointer.primary_pressed(),
                i.pointer.primary_released(),
                i.pointer.primary_down(),
            )
        });

        if is_draw_tool && response.hovered() && pressed {
            if let Some(pos) = response.interact_pointer_pos() {
                self.drawing = true;
                self.start_img = self.screen_to_img(pos, origin);
                self.cur_img = self.start_img;
                self.brush_pts.clear();
                self.brush_pts.push([self.start_img.x, self.start_img.y]);
            }
        }
        if self.drawing && down {
            if let Some(pos) = ui.input(|i| i.pointer.interact_pos()) {
                self.cur_img = self.screen_to_img(pos, origin);
                if self.tool == Tool::Brush {
                    let dup = self
                        .brush_pts
                        .last()
                        .map(|p| {
                            (p[0] - self.cur_img.x).abs() < 0.01
                                && (p[1] - self.cur_img.y).abs() < 0.01
                        })
                        .unwrap_or(false);
                    if !dup {
                        self.brush_pts.push([self.cur_img.x, self.cur_img.y]);
                    }
                }
                // 比例裁剪：约束宽高比
                if self.tool == Tool::CropRatio {
                    let (rw, rh) = self.crop_ratio;
                    let dx = self.cur_img.x - self.start_img.x;
                    let dy = self.cur_img.y - self.start_img.y;
                    let scale = (dx.abs() / rw as f32).max(dy.abs() / rh as f32);
                    self.cur_img = egui::pos2(
                        self.start_img.x + scale * rw as f32 * dx.signum(),
                        self.start_img.y + scale * rh as f32 * dy.signum(),
                    );
                }
            }
        }
        if self.drawing && released {
            self.commit_drawing();
        }
        // Select：选中/拖动文字标注
        if self.tool == Tool::Select {
            if response.hovered() && pressed {
                if let Some(pos) = response.interact_pointer_pos() {
                    let p = self.screen_to_img(pos, origin);
                    self.selected = self.hit_test(p);
                    if self.selected.is_some() {
                        self.drag_annot = true;
                        self.drag_start_img = p;
                        let orig = self.doc.as_ref().and_then(|doc| {
                            self.selected.and_then(|i| match doc.annotations.get(i) {
                                Some(Annotation::Text { pos, .. }) => Some(*pos),
                                _ => None,
                            })
                        });
                        self.drag_orig_pos = orig.unwrap_or([0.0, 0.0]);
                    }
                }
            }
            if self.drag_annot && down {
                if let Some(pos) = ui.input(|i| i.pointer.interact_pos()) {
                    let p = self.screen_to_img(pos, origin);
                    let dx = p.x - self.drag_start_img.x;
                    let dy = p.y - self.drag_start_img.y;
                    if let (Some(doc), Some(i)) = (&mut self.doc, self.selected) {
                        if let Some(Annotation::Text { pos, .. }) = doc.annotations.get_mut(i) {
                            *pos = [self.drag_orig_pos[0] + dx, self.drag_orig_pos[1] + dy];
                        }
                    }
                }
            }
            if self.drag_annot && released {
                self.drag_annot = false;
            }
        }
        // 文字：单击放置（就地弹出输入框）
        if self.tool == Tool::Text && response.clicked() {
            if let Some(pos) = response.interact_pointer_pos() {
                let p = self.screen_to_img(pos, origin);
                self.text_pos = Some([p.x, p.y]);
                self.text_screen_pos = Some(pos);
                self.text_input.clear();
            }
        }

        // 背景
        painter.rect_filled(rect, 0.0, egui::Color32::from_gray(32));

        if let Some(doc) = &self.doc {
            // 画图片纹理
            if let Some(tex) = &self.texture {
                let img_size = egui::vec2(doc.image.width() as f32, doc.image.height() as f32);
                let img_rect = egui::Rect::from_min_size(origin, img_size * self.zoom);
                painter.image(tex.id(), img_rect, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), egui::Color32::WHITE);
            }
            // 画标注
            let style = doc.style;
            for a in &doc.annotations {
                self.draw_annotation(&painter, a, &style, origin);
            }
            // 选中文字标注的指示框
            if let Some(i) = self.selected {
                if let Some(a) = doc.annotations.get(i) {
                    if let Some(b) = crate::draw::bbox(a, &style) {
                        let min = self.img_to_screen(egui::pos2(b[0], b[1]), origin);
                        let max = self.img_to_screen(egui::pos2(b[0] + b[2], b[1] + b[3]), origin);
                        painter.rect_stroke(
                            egui::Rect::from_two_pos(min, max),
                            0.0,
                            egui::Stroke::new(1.0_f32, egui::Color32::from_rgb(0, 180, 255)),
                            egui::StrokeKind::Middle,
                        );
                    }
                }
            }
            // 裁剪框
            if let Some(c) = doc.crop {
                let a = self.img_to_screen(egui::pos2(c.x as f32, c.y as f32), origin);
                let b = self.img_to_screen(egui::pos2((c.x + c.w) as f32, (c.y + c.h) as f32), origin);
                painter.rect_stroke(
                    egui::Rect::from_two_pos(a, b),
                    0.0,
                    egui::Stroke::new(1.5_f32, egui::Color32::from_rgb(0, 120, 255)),
                    egui::StrokeKind::Middle,
                );
            }
            // 预览
            self.draw_preview(&painter, origin);
        }

        // 状态栏
        ui.label(&self.status);

        if self.save_dialog {
            self.show_save_dialog(ui.ctx());
        }

        if self.confirm_open {
            let mut do_open = false;
            egui::Window::new("未保存的修改")
                .collapsible(false)
                .resizable(false)
                .show(ui.ctx(), |ui| {
                    ui.label("当前图片有未保存的修改，确定放弃？");
                    if ui.button("放弃并打开").clicked() {
                        self.confirm_open = false;
                        do_open = true;
                    }
                    if ui.button("取消").clicked() {
                        self.confirm_open = false;
                    }
                });
            if do_open {
                self.pick_and_open(ui.ctx());
            }
        }
    }
}
