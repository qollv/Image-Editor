use std::path::PathBuf;
use std::sync::mpsc;

use eframe::egui;
use rayon::prelude::*;

use crate::convert::convert_file;
use crate::options::{ConvertOptions, OnConflict, PngCompression, TargetFormat};

enum RowStatus {
    Pending,
    Done(Result<(), String>),
}

struct FileRow {
    path: PathBuf,
    status: RowStatus,
}

enum Msg {
    Progress { index: usize, result: Result<(), String> },
    AllDone,
}

pub struct ConvertView {
    files: Vec<FileRow>,
    opts: ConvertOptions,
    output_dir: String,
    rx: Option<mpsc::Receiver<Msg>>,
    running: usize,
    done: usize,
}

impl ConvertView {
    pub fn new() -> Self {
        Self {
            files: Vec::new(),
            opts: ConvertOptions::default(),
            output_dir: String::new(),
            rx: None,
            running: 0,
            done: 0,
        }
    }

    fn add_files(&mut self, paths: Vec<PathBuf>) {
        for p in paths {
            if p.is_file() {
                self.files.push(FileRow {
                    path: p,
                    status: RowStatus::Pending,
                });
            }
        }
    }

    fn add_folder(&mut self) {
        if let Some(dir) = rfd::FileDialog::new().pick_folder() {
            if let Ok(entries) = std::fs::read_dir(&dir) {
                let mut paths = Vec::new();
                for e in entries.flatten() {
                    let p = e.path();
                    if p.is_file() {
                        paths.push(p);
                    }
                }
                self.add_files(paths);
            }
        }
    }

    fn start(&mut self) {
        if self.output_dir.is_empty() || self.files.is_empty() {
            return;
        }
        let (tx, rx) = mpsc::channel();
        self.rx = Some(rx);
        self.running = self.files.len();
        self.done = 0;
        for r in &mut self.files {
            r.status = RowStatus::Pending;
        }

        let files = self.files.iter().map(|r| r.path.clone()).collect::<Vec<_>>();
        let opts = self.opts.clone();
        let out_dir = self.output_dir.clone();

        std::thread::spawn(move || {
            files.par_iter().enumerate().for_each(|(i, src)| {
                let stem = src
                    .file_stem()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_else(|| "out".into());
                let dst = PathBuf::from(&out_dir)
                    .join(format!("{}{}.{}", opts.name_prefix, stem, opts.format.extension()));
                let result = if dst.exists() && opts.on_conflict == OnConflict::Skip {
                    Ok(())
                } else {
                    convert_file(src, &dst, &opts).map_err(|e| e.to_string())
                };
                let _ = tx.send(Msg::Progress { index: i, result });
            });
            let _ = tx.send(Msg::AllDone);
        });
    }

    /// 状态徽标：淡色底 + 同色系文字 + 细边（等待/完成/失败三态）。
    fn status_badge(ui: &mut egui::Ui, theme: &crate::theme::Theme, label: &str, color: egui::Color32) {
        theme.badge(color).show(ui, |ui| {
            ui.label(egui::RichText::new(label).small().color(color));
        });
    }

    pub fn ui(&mut self, ui: &mut egui::Ui) {
        let theme = crate::theme::Theme::dark();

        // 后台进度消息回传（逐帧抽干，UI 即时刷新）
        if let Some(rx) = &self.rx {
            loop {
                match rx.try_recv() {
                    Ok(Msg::Progress { index, result }) => {
                        if let Some(row) = self.files.get_mut(index) {
                            row.status = RowStatus::Done(result);
                        }
                        self.done += 1;
                    }
                    Ok(Msg::AllDone) => {
                        self.rx = None;
                        self.running = 0;
                        break;
                    }
                    Err(_) => break,
                }
            }
        }

        // ===== 操作卡：增删列表 + 主动作 =====
        let full_w = ui.available_width();
        theme.card().show(ui, |ui| {
            ui.set_width(full_w);
            ui.horizontal(|ui| {
                if ui.button("添加文件").clicked() {
                    if let Some(paths) = rfd::FileDialog::new().pick_files() {
                        self.add_files(paths);
                    }
                }
                if ui.button("添加文件夹").clicked() {
                    self.add_folder();
                }
                if ui.button("清空列表").clicked() {
                    self.files.clear();
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let can_start =
                        !self.files.is_empty() && !self.output_dir.is_empty() && self.running == 0;
                    let btn = egui::Button::new(
                        egui::RichText::new("开始转换").color(egui::Color32::WHITE),
                    )
                    .fill(theme.accent)
                    .corner_radius(theme.radius_button)
                    .min_size(egui::vec2(108.0, 30.0));
                    if ui.add_enabled(can_start, btn).clicked() {
                        self.start();
                    }
                });
            });
        });

        ui.add_space(10.0);

        // ===== 参数卡：网格对齐的标签 + 控件 =====
        let full_w = ui.available_width();
        theme.card().show(ui, |ui| {
            ui.set_width(full_w);
            egui::Grid::new("convert_params")
                .num_columns(4)
                .spacing([12.0, 8.0])
                .show(ui, |ui| {
                    // 行 1：目标格式 + 输出目录
                    ui.label("目标格式:");
                    egui::ComboBox::from_id_salt("fmt")
                        .selected_text(self.opts.format.label())
                        .width(110.0)
                        .show_ui(ui, |ui| {
                            for f in TargetFormat::all() {
                                ui.selectable_value(&mut self.opts.format, *f, f.label());
                            }
                        });
                    ui.label("输出目录:");
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::TextEdit::singleline(&mut self.output_dir)
                                .desired_width(260.0)
                                .hint_text("选择转换结果存放目录"),
                        );
                        if ui.button("浏览…").clicked() {
                            if let Some(d) = rfd::FileDialog::new().pick_folder() {
                                self.output_dir = d.display().to_string();
                            }
                        }
                    });
                    ui.end_row();

                    // 行 2：随格式变化的专属参数
                    ui.label("格式参数:");
                    match self.opts.format {
                        TargetFormat::Jpeg => {
                            ui.horizontal(|ui| {
                                ui.label("质量");
                                ui.add(egui::Slider::new(&mut self.opts.jpeg_quality, 1..=100));
                            });
                        }
                        TargetFormat::Png => {
                            ui.horizontal(|ui| {
                                ui.label("压缩");
                                for (v, label) in [
                                    (PngCompression::Fast, "快速"),
                                    (PngCompression::Default, "默认"),
                                    (PngCompression::Best, "最佳"),
                                ] {
                                    ui.selectable_value(&mut self.opts.png_compression, v, label);
                                }
                            });
                        }
                        TargetFormat::Webp => {
                            ui.label(
                                egui::RichText::new("WebP 为无损编码（image 库限制）")
                                    .color(theme.text_muted),
                            );
                        }
                        _ => {
                            ui.label(
                                egui::RichText::new("该格式无额外参数")
                                    .color(theme.text_muted),
                            );
                        }
                    }
                    ui.end_row();

                    // 行 3：同名策略 + 透明背景色
                    ui.label("同名文件:");
                    ui.horizontal(|ui| {
                        ui.selectable_value(&mut self.opts.on_conflict, OnConflict::Skip, "跳过");
                        ui.selectable_value(&mut self.opts.on_conflict, OnConflict::Overwrite, "覆盖");
                    });
                    ui.label("透明背景:");
                    ui.color_edit_button_srgb(&mut self.opts.background);
                    ui.end_row();

                    // 行 4：缩放宽度 + 前缀 + 水印
                    ui.label("缩放宽度:");
                    ui.horizontal(|ui| {
                        ui.add(egui::DragValue::new(&mut self.opts.resize_width).range(0..=100_000));
                        ui.label(
                            egui::RichText::new("0 = 不缩放").color(theme.text_muted),
                        );
                    });
                    ui.label("文件名前缀:");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.opts.name_prefix).desired_width(100.0),
                    );
                    ui.end_row();

                    // 行 5：水印文字（整行）
                    ui.label("水印文字:");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.opts.watermark)
                            .desired_width(260.0)
                            .hint_text("留空则不加水印"),
                    );
                    ui.end_row();
                });
        });

        ui.add_space(10.0);

        // ===== 列表卡：进度 + 文件行 =====
        let full_w = ui.available_width();
        theme.card().show(ui, |ui| {
            ui.set_width(full_w);
            ui.set_min_height(ui.available_height() - 8.0);
            if self.running > 0 {
                ui.add(
                    egui::ProgressBar::new(self.done as f32 / self.running as f32)
                        .text(format!("{}/{}", self.done, self.running))
                        .animate(true),
                );
            } else if self.files.is_empty() {
                ui.label(
                    egui::RichText::new("还没有文件——点上方「添加文件」或「添加文件夹」开始")
                        .color(theme.text_muted),
                );
            }

            egui::ScrollArea::vertical().show(ui, |ui| {
                for (i, r) in self.files.iter().enumerate() {
                    // 斑马纹：偶数行 surface 底，奇数行透明
                    let zebra = if i % 2 == 0 {
                        theme.bg_surface
                    } else {
                        egui::Color32::TRANSPARENT
                    };
                    egui::Frame::NONE
                        .fill(zebra)
                        .corner_radius(egui::CornerRadius::same(4))
                        .inner_margin(egui::Margin::symmetric(8, 4))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                let (label, color) = match &r.status {
                                    RowStatus::Pending => ("等待中", theme.text_muted),
                                    RowStatus::Done(Ok(())) => ("已完成", theme.success),
                                    RowStatus::Done(Err(_)) => ("失败", theme.error),
                                };
                                // 文件名（主）+ 所在目录（次）
                                let name = r
                                    .path
                                    .file_name()
                                    .map(|s| s.to_string_lossy().to_string())
                                    .unwrap_or_else(|| r.path.display().to_string());
                                ui.label(
                                    egui::RichText::new(name)
                                        .strong()
                                        .color(theme.text_primary),
                                );
                                if let Some(parent) = r.path.parent() {
                                    ui.label(
                                        egui::RichText::new(format!("{}", parent.display()))
                                            .small()
                                            .color(theme.text_muted),
                                    );
                                }
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        Self::status_badge(ui, &theme, label, color);
                                        if let RowStatus::Done(Err(e)) = &r.status {
                                            ui.label(
                                                egui::RichText::new(e.as_str())
                                                    .small()
                                                    .color(theme.error),
                                            );
                                        }
                                    },
                                );
                            });
                        });
                }
            });
        });
    }
}
