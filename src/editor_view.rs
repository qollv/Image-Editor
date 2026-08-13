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
    Crop,
    CropRatio,
}

pub struct EditorView {
    doc: Option<EditorDoc>,
    path: Option<PathBuf>,
    dirty: bool,
    tool: Tool,
    crop_ratio: (u32, u32),
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
    save_dialog: bool,
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
            save_dialog: false,
            status: "未打开图片".to_string(),
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

    fn tool_label(t: Tool) -> &'static str {
        match t {
            Tool::Select => "选择",
            Tool::Brush => "画笔",
            Tool::Line => "直线",
            Tool::Arrow => "箭头",
            Tool::Rect => "矩形",
            Tool::Ellipse => "椭圆",
            Tool::Text => "文字",
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
                painter.line_segment([p0, p1], stroke);
                let dir = (p1 - p0).normalized();
                let head = 12.0;
                let perp = egui::vec2(-dir.y, dir.x);
                let base = p1 - dir * head;
                painter.line_segment([base, base + dir * head * 0.0 + perp * head * 0.5], stroke);
                painter.line_segment([base, base - perp * head * 0.5], stroke);
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
            Tool::Line | Tool::Arrow => {
                painter.line_segment([a, b], stroke);
                if self.tool == Tool::Arrow {
                    let dir = (b - a).normalized();
                    let perp = egui::vec2(-dir.y, dir.x);
                    let head = 12.0;
                    let base = b - dir * head;
                    painter.line_segment([base, base + perp * head * 0.5], stroke);
                    painter.line_segment([base, base - perp * head * 0.5], stroke);
                }
            }
            Tool::Rect | Tool::Ellipse => {
                let rect = egui::Rect::from_two_pos(a, b);
                if self.tool == Tool::Rect {
                    painter.rect_stroke(rect, 0.0, stroke, egui::StrokeKind::Middle);
                } else {
                    let center = rect.center();
                    let radius = egui::vec2(rect.width() / 2.0, rect.height() / 2.0);
                    painter.add(egui::Shape::ellipse_stroke(center, radius, stroke));
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
            Tool::Crop | Tool::CropRatio => {
                let (x0, y0) = (sx.min(cx), sy.min(cy));
                let (x1, y1) = (sx.max(cx), sy.max(cy));
                let w = (x1 - x0).max(1.0);
                let h = (y1 - y0).max(1.0);
                doc.crop = Some(CropRect {
                    x: x0.max(0.0) as u32,
                    y: y0.max(0.0) as u32,
                    w: w as u32,
                    h: h as u32,
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
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter("图片", &["png", "jpg", "jpeg", "webp", "gif", "bmp", "tiff", "tga"])
                    .pick_file()
                {
                    self.open_image(ui.ctx(), path);
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
                    }
                }
            }
            ui.separator();
            let has_doc = self.doc.is_some();
            if ui.add_enabled(has_doc, egui::Button::new("撤销")).clicked() {
                if let Some(d) = &mut self.doc {
                    d.undo();
                    self.dirty = true;
                }
            }
            if ui.add_enabled(has_doc, egui::Button::new("重做")).clicked() {
                if let Some(d) = &mut self.doc {
                    d.redo();
                    self.dirty = true;
                }
            }
            if ui.add_enabled(has_doc, egui::Button::new("删除")).clicked() {
                if let Some(d) = &mut self.doc {
                    d.remove_last();
                    self.dirty = true;
                }
            }
            if ui.add_enabled(has_doc, egui::Button::new("清空标注")).clicked() {
                if let Some(d) = &mut self.doc {
                    d.annotations.clear();
                    self.dirty = true;
                }
            }
            if ui.add_enabled(has_doc, egui::Button::new("应用裁剪")).clicked() {
                if let Some(d) = &mut self.doc {
                    if d.crop.is_some() {
                        d.apply_crop();
                        self.dirty = true;
                        self.refresh_texture(ui.ctx());
                    }
                }
            }
            ui.separator();
            if ui.add_enabled(has_doc, egui::Button::new("保存")).clicked() {
                self.save_dialog = true;
            }
        });

        // 文字输入
        if self.text_pos.is_some() {
            ui.horizontal(|ui| {
                ui.label("文字:");
                ui.text_edit_singleline(&mut self.text_input);
                let mut add = ui.button("添加").clicked();
                if add && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    add = true;
                }
                if add && !self.text_input.is_empty() {
                    if let (Some(doc), Some(pos)) = (&mut self.doc, self.text_pos) {
                        doc.push_annotation(Annotation::Text {
                            pos,
                            content: std::mem::take(&mut self.text_input),
                            size: 24.0,
                        });
                        self.dirty = true;
                    }
                    self.text_pos = None;
                }
                if ui.button("取消").clicked() {
                    self.text_pos = None;
                }
            });
        }

        // 画布
        let available = ui.available_size();
        let (response, painter) =
            ui.allocate_painter(available.max(egui::vec2(200.0, 200.0)), egui::Sense::click_and_drag());
        let rect = response.rect;
        let origin = rect.min + self.pan;

        // 缩放
        let zoom_delta = ui.input(|i| i.zoom_delta());
        if response.hovered() && (zoom_delta - 1.0).abs() > 1e-4 {
            if let Some(hover) = response.hover_pos() {
                let before = self.screen_to_img(hover, origin);
                self.zoom = (self.zoom * zoom_delta).clamp(0.05, 32.0);
                let new_origin = rect.min + self.pan;
                let after = self.img_to_screen(before, new_origin);
                self.pan += hover - after;
            }
        }
        // 平移（右键拖动）
        if response.dragged_by(egui::PointerButton::Secondary) {
            self.pan += response.drag_delta();
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

        // 鼠标交互
        if response.drag_started_by(egui::PointerButton::Primary) {
            if let Some(pos) = response.interact_pointer_pos() {
                self.drawing = true;
                self.start_img = self.screen_to_img(pos, origin);
                self.cur_img = self.start_img;
                self.brush_pts.clear();
                self.brush_pts.push([self.start_img.x, self.start_img.y]);
            }
        }
        if response.dragged() {
            if let Some(pos) = response.interact_pointer_pos() {
                self.cur_img = self.screen_to_img(pos, origin);
                if self.tool == Tool::Brush {
                    self.brush_pts.push([self.cur_img.x, self.cur_img.y]);
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
        if response.drag_stopped_by(egui::PointerButton::Primary) {
            self.commit_drawing();
        }
        // 文字：单击放置
        if self.tool == Tool::Text && response.clicked() {
            if let Some(pos) = response.interact_pointer_pos() {
                let p = self.screen_to_img(pos, origin);
                self.text_pos = Some([p.x, p.y]);
                self.text_input.clear();
            }
        }

        // 状态栏
        ui.label(&self.status);

        if self.save_dialog {
            self.show_save_dialog(ui.ctx());
        }
    }
}
