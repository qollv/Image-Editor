//! 编辑器工具栏：工具选择、颜色/线宽、裁剪比例、马赛克强度、撤销/重做、保存。
//! 图片调整与滤镜折叠面板也在此文件。

use eframe::egui;

use super::{EditorView, Tool};
use crate::draw::Shape;

impl EditorView {
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
            Tool::Crop => "裁剪",
            Tool::CropRatio => "比例裁剪",
        }
    }

    pub(super) fn toolbar_ui(&mut self, ui: &mut egui::Ui) {
        let theme = crate::theme::Theme::dark();
        // 第一行：打开 + 工具选择（工具按钮定宽，选中态用主题 selection 视觉）
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
                Tool::Crop,
                Tool::CropRatio,
            ] {
                let selected = self.tool == t;
                let btn = egui::Button::new(
                    egui::RichText::new(Self::tool_label(t)).color(if selected {
                        egui::Color32::WHITE
                    } else {
                        theme.text_secondary
                    }),
                )
                .selected(selected)
                .corner_radius(theme.radius_button)
                .min_size(egui::vec2(52.0, 26.0));
                if ui.add(btn).clicked() {
                    self.tool = t;
                }
            }
        });

        // 第二行：当前工具的上下文控件 + 编辑操作
        ui.horizontal(|ui| {
            // 颜色/线宽只在绘制图形类工具时显示
            let show_style = matches!(
                self.tool,
                Tool::Brush | Tool::Line | Tool::Arrow | Tool::Rect | Tool::Ellipse
            );
            if show_style {
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
            }
            if self.tool == Tool::Text {
                ui.label("字号:");
                ui.add(egui::Slider::new(&mut self.text_size, 8.0..=120.0));
            }
            if self.tool == Tool::CropRatio {
                ui.label("比例:");
                let ratios = [(1, 1), (4, 3), (16, 9)];
                for (w, h) in ratios {
                    if ui
                        .selectable_label(self.crop_ratio == (w, h), format!("{}:{}", w, h))
                        .clicked()
                    {
                        self.crop_ratio = (w, h);
                        // 选比例后立即生成居中预览框，等用户点「应用裁剪」才真正裁剪
                        if let Some(doc) = &mut self.doc {
                            doc.crop = Some(Self::make_ratio_crop((w, h), doc));
                        }
                    }
                }
                // 自定义比例
                let mut cw = self.crop_ratio.0;
                let mut ch = self.crop_ratio.1;
                ui.label("自定义:");
                let wr = ui.add(egui::DragValue::new(&mut cw).range(1..=10000).speed(0.1));
                ui.label(":");
                let hr = ui.add(egui::DragValue::new(&mut ch).range(1..=10000).speed(0.1));
                if (wr.changed() || hr.changed()) && cw > 0 && ch > 0 {
                    self.crop_ratio = (cw, ch);
                    if let Some(doc) = &mut self.doc {
                        doc.crop = Some(Self::make_ratio_crop((cw, ch), doc));
                    }
                }
            }
            ui.separator();
            let has_doc = self.doc.is_some();
            let (can_undo, can_redo) = self
                .doc
                .as_ref()
                .map(|d| (d.can_undo(), d.can_redo()))
                .unwrap_or((false, false));
            if self.tool == Tool::Mosaic {
                ui.label("马赛克强度:");
                let resp = ui.add(egui::Slider::new(&mut self.mosaic_block, 4..=64));
                if resp.drag_started() {
                    // 拖动前打一次快照：整段拖动一次撤销恢复。
                    // 没有马赛克标注时不打，避免白占一个撤销步。
                    let has_mosaic = self
                        .doc
                        .as_ref()
                        .map(|d| {
                            d.annotations
                                .iter()
                                .any(|a| matches!(&a.shape, Shape::Mosaic { .. }))
                        })
                        .unwrap_or(false);
                    if has_mosaic {
                        if let Some(d) = &mut self.doc {
                            d.snapshot_annotations();
                        }
                    }
                }
                if resp.changed() {
                    let block = self.mosaic_block;
                    if let Some(doc) = &mut self.doc {
                        for a in &mut doc.annotations {
                            if let Shape::Mosaic { block: b, .. } = &mut a.shape {
                                *b = block;
                            }
                        }
                        self.dirty = true;
                    }
                    // 实时刷新马赛克效果
                    self.refresh_texture(ui.ctx());
                }
            }
            if ui
                .add_enabled(has_doc && can_undo, egui::Button::new("撤销"))
                .clicked()
            {
                self.do_undo(ui.ctx());
            }
            if ui
                .add_enabled(has_doc && can_redo, egui::Button::new("重做"))
                .clicked()
            {
                self.do_redo(ui.ctx());
            }
            if ui
                .add_enabled(has_doc, egui::Button::new("清空标注"))
                .clicked()
            {
                if let Some(d) = &mut self.doc {
                    if !d.annotations.is_empty() {
                        // 清空也进撤销栈（与马赛克强度同一机制）
                        d.snapshot_annotations();
                        d.annotations.clear();
                        self.dirty = true;
                    }
                }
            }
            if (self.tool == Tool::Crop || self.tool == Tool::CropRatio)
                && ui
                    .add_enabled(has_doc, egui::Button::new("应用裁剪"))
                    .clicked()
            {
                if let Some(d) = &mut self.doc {
                    if d.crop.is_some() {
                        d.apply_crop();
                        self.dirty = true;
                        self.refresh_texture(ui.ctx());
                    }
                }
            }
            ui.separator();
            if ui
                .add_enabled(has_doc, egui::Button::new("保存").fill(theme.accent))
                .clicked()
            {
                self.save_dialog = true;
            }
        });
    }

    pub(super) fn show_adjust_panel(&mut self, ui: &mut egui::Ui) {
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
                    // 滑块直接绑定 doc 的调整值，撤销后会自动跟着恢复
                    let (b_resp, c_resp, s_resp) = {
                        let doc = self.doc.as_mut().unwrap();
                        ui.label("亮度:");
                        let br = ui.add(egui::Slider::new(&mut doc.brightness, -100..=100));
                        ui.label("对比度:");
                        let cr = ui.add(egui::Slider::new(&mut doc.contrast, 0.0..=3.0));
                        ui.label("饱和度:");
                        let sr = ui.add(egui::Slider::new(&mut doc.saturation, 0.0..=3.0));
                        (br, cr, sr)
                    };

                    let drag_started =
                        b_resp.drag_started() || c_resp.drag_started() || s_resp.drag_started();
                    let changed = b_resp.changed() || c_resp.changed() || s_resp.changed();
                    let drag_stopped =
                        b_resp.drag_stopped() || c_resp.drag_stopped() || s_resp.drag_stopped();

                    if drag_started {
                        // 记录拖动前的旧调整值
                        if let Some(doc) = &self.doc {
                            self.adjust_base = Some((
                                doc.brightness,
                                doc.contrast,
                                doc.saturation,
                            ));
                            // 进入低分预览：底图降采样一次，之后每帧只算小图。
                            // 12MP 图拖滑块从 ~161ms/帧 降到 ~15ms/帧；小图不降（scale=1）。
                            self.adjust_preview =
                                Some(crate::editor::AdjustPreview::new(doc.base_image()));
                        }
                    }
                    if changed {
                        if let Some(pv) = &self.adjust_preview {
                            // 预览模式：只渲染小图并上传，不动 doc.image
                            // （画布几何仍基于全分辨率，纹理拉伸显示）
                            if let Some(doc) = &self.doc {
                                let mut img = doc.render_preview(pv);
                                crate::editor::composite_checkerboard(&mut img);
                                self.upload_texture(&ctx, &img);
                            }
                        } else {
                            // 非拖动触发的变更（如键盘微调）：走全质量路径
                            if let Some(doc) = &mut self.doc {
                                // 滑块已写入新值，从底层图重新计算显示图
                                doc.recompute();
                            }
                            self.refresh_texture(&ctx);
                        }
                        self.dirty = true;
                    }
                    if drag_stopped {
                        // 退出预览：全质量重算 + 全分辨率纹理（recompute 必跳版本号，
                        // 指纹必然失效，不会残留低分纹理）
                        self.adjust_preview = None;
                        let old = self.adjust_base.take();
                        if let (Some(doc), Some(old)) = (&mut self.doc, old) {
                            doc.commit_adjust(old);
                            doc.recompute();
                            self.dirty = true;
                        }
                        self.refresh_texture(&ctx);
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
}
