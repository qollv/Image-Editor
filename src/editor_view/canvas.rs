//! 编辑器画布：缩放/平移、绘制交互、标注渲染、裁剪框手柄。
//! 坐标变换（图片坐标 ↔ 屏幕坐标）也在此文件。

use eframe::egui;

use super::{CropHandle, EditorView, Tool};
use crate::draw::{Annotation, Shape, Style};
use crate::editor::CropRect;

impl EditorView {
    fn color32(c: [u8; 4]) -> egui::Color32 {
        egui::Color32::from_rgba_unmultiplied(c[0], c[1], c[2], c[3])
    }

    /// 当前绘制样式（工具栏颜色/线宽；只影响新标注）。
    fn style(&self) -> Style {
        self.doc
            .as_ref()
            .map(|d| d.style)
            .unwrap_or(Style {
                color: [255, 0, 0, 255],
                width: 2.0,
            })
    }

    fn img_to_screen(&self, p: egui::Pos2, origin: egui::Pos2) -> egui::Pos2 {
        origin + egui::vec2(p.x, p.y) * self.zoom
    }

    fn screen_to_img(&self, p: egui::Pos2, origin: egui::Pos2) -> egui::Pos2 {
        egui::pos2(
            (p.x - origin.x) / self.zoom,
            (p.y - origin.y) / self.zoom,
        )
    }

    /// 命中检测：返回给定图片坐标命中的标注索引（从后往前）。
    fn hit_test(&self, p: egui::Pos2) -> Option<usize> {
        let doc = self.doc.as_ref()?;
        for (i, a) in doc.annotations.iter().enumerate().rev() {
            if let Some(b) = crate::draw::bbox(&a.shape, &a.style) {
                if p.x >= b[0] && p.x <= b[0] + b[2] && p.y >= b[1] && p.y <= b[1] + b[3] {
                    return Some(i);
                }
            }
        }
        None
    }

    /// 有效裁剪框（图片坐标 [x0, y0, x1, y1]）；无裁剪框时视为整图。
    fn effective_crop(&self) -> Option<[f32; 4]> {
        let doc = self.doc.as_ref()?;
        if let Some(c) = doc.crop {
            Some([
                c.x as f32,
                c.y as f32,
                (c.x + c.w) as f32,
                (c.y + c.h) as f32,
            ])
        } else {
            Some([
                0.0,
                0.0,
                doc.image.width() as f32,
                doc.image.height() as f32,
            ])
        }
    }

    /// 检测鼠标位置命中的裁剪框手柄。
    fn detect_crop_handle(
        &self,
        pos: egui::Pos2,
        rect: [f32; 4],
        origin: egui::Pos2,
        thr: f32,
    ) -> Option<CropHandle> {
        let p0 = self.img_to_screen(egui::pos2(rect[0], rect[1]), origin);
        let p1 = self.img_to_screen(egui::pos2(rect[2], rect[3]), origin);
        let corners = [
            (p0, CropHandle::TopLeft),
            (egui::pos2(p1.x, p0.y), CropHandle::TopRight),
            (p1, CropHandle::BottomRight),
            (egui::pos2(p0.x, p1.y), CropHandle::BottomLeft),
        ];
        for (c, h) in corners {
            if (pos - c).length() < thr {
                return Some(h);
            }
        }
        if (pos.y - p0.y).abs() < thr && pos.x >= p0.x && pos.x <= p1.x {
            return Some(CropHandle::Top);
        }
        if (pos.y - p1.y).abs() < thr && pos.x >= p0.x && pos.x <= p1.x {
            return Some(CropHandle::Bottom);
        }
        if (pos.x - p0.x).abs() < thr && pos.y >= p0.y && pos.y <= p1.y {
            return Some(CropHandle::Left);
        }
        if (pos.x - p1.x).abs() < thr && pos.y >= p0.y && pos.y <= p1.y {
            return Some(CropHandle::Right);
        }
        if pos.x >= p0.x && pos.x <= p1.x && pos.y >= p0.y && pos.y <= p1.y {
            return Some(CropHandle::Move);
        }
        None
    }

    /// 根据手柄和鼠标位置更新裁剪框（keep_ratio 时角拖动保持比例）。
    fn apply_crop_drag(
        &mut self,
        handle: CropHandle,
        pos: egui::Pos2,
        origin: egui::Pos2,
        keep_ratio: bool,
    ) {
        let p = self.screen_to_img(pos, origin);
        let Some(doc) = self.doc.as_mut() else {
            return;
        };
        let (img_w, img_h) = (doc.image.width() as f32, doc.image.height() as f32);
        let rect = doc
            .crop
            .map(|c| [c.x as f32, c.y as f32, (c.x + c.w) as f32, (c.y + c.h) as f32])
            .unwrap_or([0.0, 0.0, img_w, img_h]);
        let (mut x0, mut y0, mut x1, mut y1) = (rect[0], rect[1], rect[2], rect[3]);

        if keep_ratio {
            let (rw, rh) = (self.crop_ratio.0 as f32, self.crop_ratio.1 as f32);
            // 比例裁剪：角拖动以对角为锚保持比例
            match handle {
                CropHandle::BottomRight => {
                    let w = (p.x - x0).abs();
                    let h = (p.y - y0).abs();
                    let s = (w / rw).max(h / rh);
                    x1 = x0 + s * rw * (p.x - x0).signum();
                    y1 = y0 + s * rh * (p.y - y0).signum();
                }
                CropHandle::TopLeft => {
                    let w = (x1 - p.x).abs();
                    let h = (y1 - p.y).abs();
                    let s = (w / rw).max(h / rh);
                    x0 = x1 - s * rw * (x1 - p.x).signum();
                    y0 = y1 - s * rh * (y1 - p.y).signum();
                }
                CropHandle::BottomLeft => {
                    let w = (x1 - p.x).abs();
                    let h = (p.y - y0).abs();
                    let s = (w / rw).max(h / rh);
                    x0 = x1 - s * rw * (x1 - p.x).signum();
                    y1 = y0 + s * rh * (p.y - y0).signum();
                }
                CropHandle::TopRight => {
                    let w = (p.x - x0).abs();
                    let h = (y1 - p.y).abs();
                    let s = (w / rw).max(h / rh);
                    x1 = x0 + s * rw * (p.x - x0).signum();
                    y0 = y1 - s * rh * (y1 - p.y).signum();
                }
                _ => {}
            }
        } else {
            match handle {
                CropHandle::Top => y0 = p.y,
                CropHandle::Bottom => y1 = p.y,
                CropHandle::Left => x0 = p.x,
                CropHandle::Right => x1 = p.x,
                CropHandle::TopLeft => {
                    x0 = p.x;
                    y0 = p.y;
                }
                CropHandle::TopRight => {
                    x1 = p.x;
                    y0 = p.y;
                }
                CropHandle::BottomLeft => {
                    x0 = p.x;
                    y1 = p.y;
                }
                CropHandle::BottomRight => {
                    x1 = p.x;
                    y1 = p.y;
                }
                CropHandle::Move => {}
            }
        }

        // 边界与最小尺寸
        let (mut x0, mut y0, mut x1, mut y1) = (x0, y0, x1, y1);
        if x1 < x0 {
            std::mem::swap(&mut x0, &mut x1);
        }
        if y1 < y0 {
            std::mem::swap(&mut y0, &mut y1);
        }
        let min = 8.0_f32;
        x0 = x0.clamp(0.0, img_w - min);
        y0 = y0.clamp(0.0, img_h - min);
        x1 = x1.clamp(x0 + min, img_w);
        y1 = y1.clamp(y0 + min, img_h);

        doc.crop = Some(CropRect {
            x: x0.round() as u32,
            y: y0.round() as u32,
            w: (x1 - x0).round().max(1.0) as u32,
            h: (y1 - y0).round().max(1.0) as u32,
        });
    }

    fn draw_annotation(&self, painter: &egui::Painter, a: &Annotation, origin: egui::Pos2) {
        // 每条标注用自己创建时定格的样式，不受工具栏当前颜色/线宽影响
        let Annotation { shape, style } = a;
        let c = Self::color32(style.color);
        let stroke = egui::Stroke::new(style.width.max(1.0), c);
        match shape {
            Shape::Brush(pts) => {
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
            Shape::Line { start, end } => {
                let p0 = self.img_to_screen(egui::pos2(start[0], start[1]), origin);
                let p1 = self.img_to_screen(egui::pos2(end[0], end[1]), origin);
                painter.line_segment([p0, p1], stroke);
            }
            Shape::Arrow { start, end } => {
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
            Shape::Rect { min, max } => {
                let a = self.img_to_screen(egui::pos2(min[0], min[1]), origin);
                let b = self.img_to_screen(egui::pos2(max[0], max[1]), origin);
                painter.rect_stroke(
                    egui::Rect::from_two_pos(a, b),
                    0.0,
                    stroke,
                    egui::StrokeKind::Middle,
                );
            }
            Shape::Ellipse { min, max } => {
                let a = self.img_to_screen(egui::pos2(min[0], min[1]), origin);
                let b = self.img_to_screen(egui::pos2(max[0], max[1]), origin);
                let center = egui::pos2((a.x + b.x) / 2.0, (a.y + b.y) / 2.0);
                let radius = egui::vec2((b.x - a.x).abs() / 2.0, (b.y - a.y).abs() / 2.0);
                painter.add(egui::Shape::ellipse_stroke(center, radius, stroke));
            }
            Shape::Mosaic { min, max, .. } => {
                // 实际像素化烘焙在纹理里，这里只画边框提示区域
                let a = self.img_to_screen(egui::pos2(min[0], min[1]), origin);
                let b = self.img_to_screen(egui::pos2(max[0], max[1]), origin);
                painter.rect_stroke(
                    egui::Rect::from_two_pos(a, b),
                    0.0,
                    stroke,
                    egui::StrokeKind::Middle,
                );
            }
            Shape::Text { pos, content, size } => {
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
            _ => {}
        }
    }

    pub(super) fn canvas_ui(&mut self, ui: &mut egui::Ui) {
        let available = ui.available_size();
        let (response, painter) =
            ui.allocate_painter(available.max(egui::vec2(200.0, 200.0)), egui::Sense::click_and_drag());
        let rect = response.rect;

        // 缩放（围绕鼠标位置）；选中标注时滚轮改为调整文字字号
        let zoom_delta = ui.input(|i| i.zoom_delta());
        if response.hovered() && (zoom_delta - 1.0).abs() > 1e-4 {
            if self.selected.is_some() {
                if let (Some(doc), Some(i)) = (&mut self.doc, self.selected) {
                    if let Some(Annotation {
                        shape: Shape::Text { size, .. },
                        ..
                    }) = doc.annotations.get_mut(i)
                    {
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

        // 打开图片后居中显示一次
        if self.needs_center {
            if let Some(doc) = &self.doc {
                let scaled = egui::vec2(
                    doc.image.width() as f32 * self.zoom,
                    doc.image.height() as f32 * self.zoom,
                );
                self.pan = egui::vec2(
                    (rect.width() - scaled.x) / 2.0,
                    (rect.height() - scaled.y) / 2.0,
                );
            }
            self.needs_center = false;
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
            }
        }
        if self.drawing && released {
            self.commit_drawing();
            // 马赛克标注新增后刷新纹理，让预览立即显示像素化
            self.refresh_texture(ui.ctx());
        }
        // Select：选中/拖动标注
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
                                Some(Annotation {
                                    shape: Shape::Text { pos, .. },
                                    ..
                                }) => Some(*pos),
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
                        if let Some(Annotation {
                            shape: Shape::Text { pos, .. },
                            ..
                        }) = doc.annotations.get_mut(i)
                        {
                            *pos = [self.drag_orig_pos[0] + dx, self.drag_orig_pos[1] + dy];
                        }
                    }
                }
            }
            if self.drag_annot && released {
                self.drag_annot = false;
            }
        }
        // Crop/CropRatio：拖动裁剪框边缘/角调整范围
        if self.tool == Tool::Crop || self.tool == Tool::CropRatio {
            let keep_ratio = self.tool == Tool::CropRatio;
            let thr = 8.0;
            if response.hovered() && pressed {
                if let Some(pos) = response.interact_pointer_pos() {
                    if let Some(rect) = self.effective_crop() {
                        self.crop_handle = self.detect_crop_handle(pos, rect, origin, thr);
                    }
                }
            }
            if self.crop_handle.is_some() && down {
                if let Some(pos) = ui.input(|i| i.pointer.interact_pos()) {
                    if let Some(handle) = self.crop_handle {
                        self.apply_crop_drag(handle, pos, origin, keep_ratio);
                        self.dirty = true;
                    }
                }
            }
            if self.crop_handle.is_some() && released {
                self.crop_handle = None;
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
                painter.image(
                    tex.id(),
                    img_rect,
                    egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                    egui::Color32::WHITE,
                );
            }
            // 画标注（每条用自身样式）
            for a in &doc.annotations {
                self.draw_annotation(&painter, a, origin);
            }
            // 选中标注的指示框
            if let Some(i) = self.selected {
                if let Some(a) = doc.annotations.get(i) {
                    if let Some(b) = crate::draw::bbox(&a.shape, &a.style) {
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
    }
}
