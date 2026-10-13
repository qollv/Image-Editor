//! 编辑器页 UI 编排：struct 定义、打开/保存、对话框、快捷键、纹理管理。
//! 工具栏实现在 toolbar.rs，画布实现在 canvas.rs（均为本模块的子模块）。

use std::path::PathBuf;

use eframe::egui;

use crate::draw::Shape;
use crate::editor::{CropRect, EditorDoc};

mod canvas;
mod toolbar;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Tool {
    Select,
    Brush,
    Line,
    Arrow,
    Rect,
    Ellipse,
    Text,
    Mosaic,
    Crop,
    CropRatio,
}

/// 裁剪框手柄：拖动边/角/整体调整裁剪范围
#[derive(Clone, Copy, PartialEq, Eq)]
enum CropHandle {
    Top,
    Bottom,
    Left,
    Right,
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
    Move,
}

/// 纹理源图指纹 = (image_version, 马赛克键列表)。
/// 与当前文档一致说明源图没变，纹理仍然有效。
type TextureFingerprint = (u64, Vec<([f32; 2], [f32; 2], u32)>);

pub struct EditorView {
    doc: Option<EditorDoc>,
    path: Option<PathBuf>,
    dirty: bool,
    tool: Tool,
    crop_ratio: (u32, u32),
    /// 调整拖动前的旧值 (亮度, 对比度, 饱和度)
    adjust_base: Option<(i32, f32, f32)>,
    /// 调整滑块拖动中的低分预览（松手后置 None，走全质量路径）
    adjust_preview: Option<crate::editor::AdjustPreview>,
    mosaic_block: u32,
    needs_center: bool,
    text_size: f32,
    crop_handle: Option<CropHandle>,
    zoom: f32,
    pan: egui::Vec2,
    /// 文本栅格化字体：懒加载（首次保存时才解析，约 1.8s，启动不付这个成本）
    font: Option<fontdue::Font>,
    texture: Option<egui::TextureHandle>,
    /// 纹理源图指纹 = (image_version, 马赛克键列表)：与当前一致说明源图没变，
    /// 纹理仍然有效，跳过重建与 GPU 重传（12MP 图一次重传几十 MB）。
    texture_fingerprint: Option<TextureFingerprint>,
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

impl EditorView {
    pub fn new() -> Self {
        Self {
            doc: None,
            path: None,
            dirty: false,
            tool: Tool::Select,
            crop_ratio: (1, 1),
            adjust_base: None,
            adjust_preview: None,
            mosaic_block: 16,
            needs_center: false,
            text_size: 24.0,
            crop_handle: None,
            zoom: 1.0,
            pan: egui::Vec2::ZERO,
            font: None,
            texture: None,
            texture_fingerprint: None,
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
        let mut dialog = rfd::FileDialog::new();
        for f in crate::options::TargetFormat::all() {
            dialog = dialog.add_filter(f.label(), &[f.extension()]);
        }
        dialog = dialog.add_filter("JPEG", &["jpg", "jpeg"]);
        if let Some(path) = dialog.pick_file() {
            self.open_image(ctx, path);
        }
    }

    fn open_image(&mut self, ctx: &egui::Context, path: PathBuf) {
        match EditorDoc::open(&path) {
            Ok(doc) => {
                self.doc = Some(doc);
                self.path = Some(path);
                self.dirty = false;
                self.zoom = 1.0;
                self.pan = egui::Vec2::ZERO;
                self.needs_center = true;
                self.brush_pts.clear();
                self.text_pos = None;
                // 新文档：指纹作废，强制重建纹理；预览模式一并退出
                self.texture_fingerprint = None;
                self.adjust_preview = None;
                self.refresh_texture(ctx);
                self.status = "已打开图片".to_string();
            }
            Err(e) => {
                self.status = format!("打开失败: {}", e);
            }
        }
    }

    /// 马赛克标注的键列表（纹理内容除底图外只受它影响）。
    fn mosaic_keys(doc: &EditorDoc) -> Vec<([f32; 2], [f32; 2], u32)> {
        doc.annotations
            .iter()
            .filter_map(|a| match &a.shape {
                Shape::Mosaic { min, max, block } => Some((*min, *max, *block)),
                _ => None,
            })
            .collect()
    }

    /// 把 RGBA 图上传为画布纹理。
    fn upload_texture(&mut self, ctx: &egui::Context, rgba: &image::RgbaImage) {
        let size = [rgba.width() as usize, rgba.height() as usize];
        let color = egui::ColorImage::from_rgba_unmultiplied(size, rgba.as_raw());
        self.texture = Some(ctx.load_texture("editor_img", color, egui::TextureOptions::LINEAR));
    }

    /// 重建画布纹理（马赛克烘焙进纹理，让预览实时显示像素化）。
    /// 指纹一致说明源图没变：保留原纹理，跳过重建与 GPU 重传。
    fn refresh_texture(&mut self, ctx: &egui::Context) {
        let Some(doc) = &self.doc else { return };
        let version = doc.image_version;
        let keys = Self::mosaic_keys(doc);
        let fresh = matches!(&self.texture_fingerprint, Some((v, k)) if *v == version && *k == keys);
        if fresh {
            return;
        }
        let mut rgba = doc.image.to_rgba8();
        for a in &doc.annotations {
            if let Shape::Mosaic { .. } = &a.shape {
                crate::draw::rasterize(&a.shape, &a.style, &mut rgba, None);
            }
        }
        // 透明区域垫暗色棋盘格（仅显示用，不进保存结果）
        crate::editor::composite_checkerboard(&mut rgba);
        self.upload_texture(ctx, &rgba);
        self.texture_fingerprint = Some((version, keys));
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
        let shape = match self.tool {
            Tool::Brush => {
                if self.brush_pts.is_empty() {
                    return;
                }
                Shape::Brush(std::mem::take(&mut self.brush_pts))
            }
            Tool::Line => Shape::Line {
                start: [sx, sy],
                end: [cx, cy],
            },
            Tool::Arrow => Shape::Arrow {
                start: [sx, sy],
                end: [cx, cy],
            },
            Tool::Rect => Shape::Rect {
                min: [sx.min(cx), sy.min(cy)],
                max: [sx.max(cx), sy.max(cy)],
            },
            Tool::Ellipse => Shape::Ellipse {
                min: [sx.min(cx), sy.min(cy)],
                max: [sx.max(cx), sy.max(cy)],
            },
            Tool::Mosaic => Shape::Mosaic {
                min: [sx.min(cx), sy.min(cy)],
                max: [sx.max(cx), sy.max(cy)],
                block: self.mosaic_block,
            },
            _ => return,
        };
        doc.push_annotation(shape);
        self.dirty = true;
    }

    /// 文本栅格化字体懒加载：首次保存时解析（约 1.8s），之后复用；
    /// 找不到字体时文字标注不写入结果，并在状态栏提示。
    fn ensure_font(&mut self) {
        if self.font.is_some() {
            return;
        }
        self.font = crate::fonts::load_text_font();
        if self.font.is_none() {
            self.status = "警告: 未找到可用字体，文字标注未写入保存结果".to_string();
        }
    }

    fn save_as(&mut self, path: &PathBuf) {
        if self.doc.is_none() {
            return;
        }
        self.ensure_font();
        let doc = self.doc.as_ref().unwrap();
        let font = self.font.as_ref();
        let out = doc.render(font);
        // JPEG/BMP 等不支持透明的格式先合成白底，与转换页行为一致
        let img = crate::editor::prepare_for_save(out, path);
        match img.save(path) {
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

    /// 撤销（工具栏按钮与 Ctrl+Z 共用）。
    fn do_undo(&mut self, ctx: &egui::Context) {
        let can = self.doc.as_ref().map(|d| d.can_undo()).unwrap_or(false);
        if can {
            if let Some(d) = &mut self.doc {
                d.undo();
            }
            self.dirty = true;
            // 撤销的可能是裁剪/图片级操作，需要刷新画布纹理
            self.refresh_texture(ctx);
        }
    }

    /// 重做（工具栏按钮与 Ctrl+Y / Ctrl+Shift+Z 共用）。
    fn do_redo(&mut self, ctx: &egui::Context) {
        let can = self.doc.as_ref().map(|d| d.can_redo()).unwrap_or(false);
        if can {
            if let Some(d) = &mut self.doc {
                d.redo();
            }
            self.dirty = true;
            self.refresh_texture(ctx);
        }
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

    /// 就地文字输入框（显示在点击位置）。
    fn text_input_overlay(&mut self, ui: &mut egui::Ui) {
        let Some(screen_pos) = self.text_screen_pos else {
            return;
        };
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
                    doc.push_annotation(Shape::Text {
                        pos,
                        content: std::mem::take(&mut self.text_input),
                        size: self.text_size,
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

    fn show_save_dialog(&mut self, ctx: &egui::Context) {
        let mut close = false;
        egui::Window::new("保存图片")
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                if ui.button("另存为新文件…").clicked() {
                    // 滤镜覆盖转换页支持的全部格式
                    let mut dialog = rfd::FileDialog::new();
                    if let Some(p) = &self.path {
                        if let Some(stem) = p.file_stem() {
                            dialog = dialog.set_file_name(stem.to_string_lossy().as_ref());
                        }
                    }
                    for f in crate::options::TargetFormat::all() {
                        dialog = dialog.add_filter(f.label(), &[f.extension()]);
                    }
                    if let Some(path) = dialog.save_file() {
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

    /// 键盘快捷键：Ctrl+Z 撤销 / Ctrl+Shift+Z 或 Ctrl+Y 重做 / Ctrl+S 保存 / Ctrl+O 打开。
    /// 文字输入框聚焦时不拦截（让文本编辑自己处理按键）。
    fn handle_shortcuts(&mut self, ui: &egui::Ui) {
        let ctx = ui.ctx();
        if ctx.wants_keyboard_input() {
            return;
        }
        let (cmd, shift, z, y, s, o) = ctx.input(|i| {
            (
                i.modifiers.command,
                i.modifiers.shift,
                i.key_pressed(egui::Key::Z),
                i.key_pressed(egui::Key::Y),
                i.key_pressed(egui::Key::S),
                i.key_pressed(egui::Key::O),
            )
        });
        if !cmd {
            return;
        }
        if z && shift {
            self.do_redo(ui.ctx());
        } else if z {
            self.do_undo(ui.ctx());
        } else if y {
            self.do_redo(ui.ctx());
        } else if s {
            // 已有路径直接覆盖原图；否则弹出保存对话框
            if self.path.is_some() {
                self.save_overwrite();
            } else {
                self.save_dialog = true;
            }
        } else if o {
            if self.dirty {
                self.confirm_open = true;
            } else {
                self.pick_and_open(ui.ctx());
            }
        }
    }

    pub fn ui(&mut self, ui: &mut egui::Ui) {
        self.toolbar_ui(ui);
        self.show_adjust_panel(ui);
        self.text_input_overlay(ui);
        self.canvas_ui(ui);

        // 状态栏
        let theme = crate::theme::Theme::dark();
        ui.label(
            egui::RichText::new(&self.status)
                .small()
                .color(theme.text_muted),
        );

        self.handle_shortcuts(ui);

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
