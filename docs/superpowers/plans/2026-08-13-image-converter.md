# 图片格式转换 + 轻量编辑器 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 构建一个 Windows 桌面 GUI 应用，支持单个/批量图片格式转换，以及单张图片的裁剪与绘制标注。

**Architecture:** Rust + egui 桌面应用。核心逻辑（转换、编辑器文档、绘制栅格化）与 UI 完全解耦，纯逻辑模块用同文件 `#[cfg(test)]` 单元测试覆盖。UI 层只负责展示与交互，经通道/线程池与核心协作。

**Tech Stack:** Rust 2021 · eframe/egui · image（全 feature）· rfd · rayon · fontdue

**Spec:** `docs/superpowers/specs/2026-08-13-image-converter-design.md`

## Global Constraints

- 交付形态：单文件 `.exe`，零系统运行时依赖。
- 依赖仅限：`eframe`、`image`、`rfd`、`rayon`、`fontdue`（全部纯 Rust）。
- 转换逻辑与 UI 解耦：`convert_file` 不得引用任何 egui 类型；`editor.rs`/`draw.rs` 同理。
- 支持格式：PNG、JPEG、WebP、GIF、BMP、ICO、TIFF、TGA、AVIF、PNM、QOI。
- JPEG 质量范围 1–100（默认 90）；转 JPEG/BMP 等不支持透明的格式时自动合成白底（可配置）。
- 命名冲突默认「跳过」，可切换「覆盖」。
- 中文文字标注：Windows 从 `C:\Windows\Fonts\msyh.ttc`（collection_index 0）加载，失败回退内置字体。
- 编辑页一次只编辑一张图，打开新图前提示保存。
- v1 不做拖拽打开、不做批量编辑。

---

### Task 1: 项目骨架与最小可运行窗口

**Files:**
- Create: `Cargo.toml`
- Create: `src/main.rs`
- Create: `src/app.rs`

**Interfaces:**
- Consumes: 无
- Produces:
  - `struct ImageApp`（`eframe::App` 实现，含 `fn new(cc: &eframe::CreationContext) -> Self`）
  - `enum Tab { Convert, Editor }`（根组件 Tab 状态，Task 8 使用）

- [ ] **Step 1: 写 Cargo.toml**

```toml
[package]
name = "image-converter"
version = "0.1.0"
edition = "2021"

[dependencies]
eframe = "0.31"
image = { version = "0.25", features = ["png", "jpeg", "gif", "bmp", "ico", "tiff", "tga", "webp", "avif", "pnm", "qoi"] }
rfd = "0.15"
rayon = "1"
fontdue = "0.9"

[profile.release]
opt-level = 2
```

- [ ] **Step 2: 写 src/main.rs**

```rust
mod app;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1000.0, 700.0]),
        ..Default::default()
    };
    eframe::run_native(
        "图片格式转换",
        options,
        Box::new(|cc| Ok(Box::new(app::ImageApp::new(cc)))),
    )
}
```

- [ ] **Step 3: 写 src/app.rs（最小骨架 + Tab）**

```rust
use eframe::egui;

#[derive(PartialEq, Clone, Copy)]
pub enum Tab {
    Convert,
    Editor,
}

pub struct ImageApp {
    tab: Tab,
}

impl ImageApp {
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        Self { tab: Tab::Convert }
    }
}

impl eframe::App for ImageApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::TopBottomPanel::top("tabs").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.selectable_value(&mut self.tab, Tab::Convert, "格式转换");
                ui.selectable_value(&mut self.tab, Tab::Editor, "图片编辑");
            });
        });
        egui::CentralPanel::default().show(ctx, |ui| {
            match self.tab {
                Tab::Convert => { ui.centered_and_justified(|ui| { ui.label("转换页（待实现）"); }); }
                Tab::Editor => { ui.centered_and_justified(|ui| { ui.label("编辑页（待实现）"); }); }
            }
        });
    }
}
```

- [ ] **Step 4: 验证编译并运行**

Run: `cargo build`
Expected: 编译成功。运行 `cargo run` 弹出标题为「图片格式转换」的窗口，顶部两个 Tab 可切换，各显示占位文字。手动确认后关闭。

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml src/
git commit -m "feat: 项目骨架与最小可运行窗口"
```

---

### Task 2: 转换参数 options.rs

**Files:**
- Create: `src/options.rs`
- Modify: `src/main.rs`（加 `mod options;`）

**Interfaces:**
- Consumes: 无
- Produces:
  - `enum TargetFormat { Png, Jpeg, Webp, Gif, Bmp, Ico, Tiff, Tga, Avif, Pnm, Qoi }`
  - `enum OnConflict { Skip, Overwrite }`
  - `struct ConvertOptions { format: TargetFormat, jpeg_quality: u8, png_compression: PngCompression, webp_quality: u8, webp_lossless: bool, background: [u8; 3], on_conflict: OnConflict }`
  - `impl Default for ConvertOptions`
  - `impl TargetFormat { pub fn all() -> &'static [TargetFormat]; pub fn label(&self) -> &'static str; pub fn extension(&self) -> &'static str; pub fn to_image_format(&self) -> image::ImageFormat; pub fn supports_alpha(&self) -> bool }`

- [ ] **Step 1: 写失败测试（src/options.rs 内 `#[cfg(test)]`）**

```rust
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
```

- [ ] **Step 2: 运行测试确认失败**

Run: `cargo test options`
Expected: FAIL（编译错误：`ConvertOptions`、`TargetFormat` 未定义）

- [ ] **Step 3: 实现 src/options.rs**

```rust
use image::ImageFormat;

#[derive(PartialEq, Eq, Clone, Copy, Debug)]
pub enum TargetFormat {
    Png, Jpeg, Webp, Gif, Bmp, Ico, Tiff, Tga, Avif, Pnm, Qoi,
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

pub struct ConvertOptions {
    pub format: TargetFormat,
    pub jpeg_quality: u8,
    pub png_compression: PngCompression,
    pub webp_quality: u8,
    pub webp_lossless: bool,
    pub background: [u8; 3],
    pub on_conflict: OnConflict,
}

impl Default for ConvertOptions {
    fn default() -> Self {
        Self {
            format: TargetFormat::Png,
            jpeg_quality: 90,
            png_compression: PngCompression::Default,
            webp_quality: 90,
            webp_lossless: false,
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
            TargetFormat::Png | TargetFormat::Webp | TargetFormat::Gif
            | TargetFormat::Tiff | TargetFormat::Tga | TargetFormat::Avif
            | TargetFormat::Ico | TargetFormat::Pnm | TargetFormat::Qoi => true,
            TargetFormat::Jpeg | TargetFormat::Bmp => false,
        }
    }
}
```

- [ ] **Step 4: 运行测试确认通过**

Run: `cargo test options`
Expected: PASS（3 个测试全绿）

- [ ] **Step 5: Commit**

```bash
git add src/options.rs src/main.rs
git commit -m "feat: 转换参数定义 options.rs"
```

---

### Task 3: 转换核心 convert.rs

**Files:**
- Create: `src/convert.rs`
- Modify: `src/main.rs`（加 `mod convert;`）

**Interfaces:**
- Consumes: `options::{ConvertOptions, TargetFormat, PngCompression}`
- Produces:
  - `enum ConvertError { Io(std::io::Error), Image(image::ImageError), Unsupported(String) }`（实现 `Display` + `From`）
  - `fn convert_file(input: &Path, output: &Path, opts: &ConvertOptions) -> Result<(), ConvertError>`

- [ ] **Step 1: 写失败测试（src/convert.rs 内 `#[cfg(test)]`）**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::options::*;
    use image::{Rgba, RgbaImage};

    fn sample_rgba() -> RgbaImage {
        let mut img = RgbaImage::new(2, 2);
        img.put_pixel(0, 0, Rgba([255, 0, 0, 255]));
        img.put_pixel(1, 0, Rgba([0, 255, 0, 128]));
        img.put_pixel(0, 1, Rgba([0, 0, 255, 255]));
        img.put_pixel(1, 1, Rgba([0, 0, 0, 0]));
        img
    }

    fn tmp_path(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("ic_test_{}", name))
    }

    #[test]
    fn convert_png_to_jpeg_flattens_alpha_to_white() {
        let src = tmp_path("src.png");
        let dst = tmp_path("out.jpg");
        sample_rgba().save(&src).unwrap();

        let mut opts = ConvertOptions::default();
        opts.format = TargetFormat::Jpeg;
        convert_file(&src, &dst, &opts).unwrap();

        let out = image::open(&dst).unwrap().to_rgb8();
        // 透明像素 (1,1) 应合成为白色
        assert_eq!(out.get_pixel(1, 1), &image::Rgb([255, 255, 255]));
        // 半透明绿色 (1,0) 应混合成浅绿（绿 128 + 白 255 → 约 191）
        let p = out.get_pixel(1, 0);
        assert!(p[1] > 180 && p[1] < 200, "got {:?}", p);
        std::fs::remove_file(&src).ok();
        std::fs::remove_file(&dst).ok();
    }

    #[test]
    fn convert_png_to_png_preserves_alpha() {
        let src = tmp_path("src2.png");
        let dst = tmp_path("out2.png");
        sample_rgba().save(&src).unwrap();
        let mut opts = ConvertOptions::default();
        opts.format = TargetFormat::Png;
        convert_file(&src, &dst, &opts).unwrap();
        let out = image::open(&dst).unwrap().to_rgba8();
        assert_eq!(out.get_pixel(1, 1), &Rgba([0, 0, 0, 0]));
        std::fs::remove_file(&src).ok();
        std::fs::remove_file(&dst).ok();
    }

    #[test]
    fn missing_input_returns_error() {
        let r = convert_file(
            std::path::Path::new("/nonexistent/xx.png"),
            std::path::Path::new("/tmp/yy.jpg"),
            &ConvertOptions::default(),
        );
        assert!(r.is_err());
    }
}
```

- [ ] **Step 2: 运行测试确认失败**

Run: `cargo test convert`
Expected: FAIL（`convert_file` 未定义）

- [ ] **Step 3: 实现 src/convert.rs**

```rust
use std::fmt;
use std::path::Path;

use crate::options::{ConvertOptions, PngCompression, TargetFormat};

#[derive(Debug)]
pub enum ConvertError {
    Io(std::io::Error),
    Image(image::ImageError),
    Unsupported(String),
}

impl fmt::Display for ConvertError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConvertError::Io(e) => write!(f, "IO 错误: {}", e),
            ConvertError::Image(e) => write!(f, "图像错误: {}", e),
            ConvertError::Unsupported(s) => write!(f, "不支持: {}", s),
        }
    }
}

impl std::error::Error for ConvertError {}

impl From<std::io::Error> for ConvertError {
    fn from(e: std::io::Error) -> Self { ConvertError::Io(e) }
}
impl From<image::ImageError> for ConvertError {
    fn from(e: image::ImageError) -> Self { ConvertError::Image(e) }
}

/// 把带 alpha 的图合成到纯色背景上（用于 JPEG/BMP 等不支持透明的格式）。
fn flatten_alpha(img: image::DynamicImage, bg: [u8; 3]) -> image::DynamicImage {
    let rgba = img.to_rgba8();
    let mut rgb = image::RgbImage::new(rgba.width(), rgba.height());
    for (x, y, p) in rgba.enumerate_pixels() {
        let a = p[3] as f32 / 255.0;
        let r = (p[0] as f32 * a + bg[0] as f32 * (1.0 - a)) as u8;
        let g = (p[1] as f32 * a + bg[1] as f32 * (1.0 - a)) as u8;
        let b = (p[2] as f32 * a + bg[2] as f32 * (1.0 - a)) as u8;
        rgb.put_pixel(x, y, image::Rgb([r, g, b]));
    }
    image::DynamicImage::ImageRgb8(rgb)
}

pub fn convert_file(input: &Path, output: &Path, opts: &ConvertOptions) -> Result<(), ConvertError> {
    let mut img = image::open(input)?;

    if !opts.format.supports_alpha() && img.color().has_alpha() {
        img = flatten_alpha(img, opts.background);
    }

    match opts.format {
        TargetFormat::Png => save_png(&img, output, opts),
        TargetFormat::Jpeg => save_jpeg(&img, output, opts),
        TargetFormat::Webp => save_webp(&img, output, opts),
        other => {
            img.save_with_format(output, other.to_image_format())?;
            Ok(())
        }
    }
}

fn save_png(img: &image::DynamicImage, output: &Path, opts: &ConvertOptions) -> Result<(), ConvertError> {
    let f = std::fs::File::create(output)?;
    let compression = match opts.png_compression {
        PngCompression::Fast => image::codecs::png::CompressionType::Fast,
        PngCompression::Default => image::codecs::png::CompressionType::Default,
        PngCompression::Best => image::codecs::png::CompressionType::Best,
    };
    let enc = image::codecs::png::PngEncoder::new_with_quality(
        f, compression, image::codecs::png::FilterType::Adaptive,
    );
    img.write_with_encoder(enc)?;
    Ok(())
}

fn save_jpeg(img: &image::DynamicImage, output: &Path, opts: &ConvertOptions) -> Result<(), ConvertError> {
    let f = std::fs::File::create(output)?;
    let enc = image::codecs::jpeg::JpegEncoder::new_with_quality(f, opts.jpeg_quality.clamp(1, 100));
    img.write_with_encoder(enc)?;
    Ok(())
}

fn save_webp(img: &image::DynamicImage, output: &Path, opts: &ConvertOptions) -> Result<(), ConvertError> {
    let f = std::fs::File::create(output)?;
    if opts.webp_lossless {
        let enc = image::codecs::webp::WebPEncoder::new_lossless(f);
        img.write_with_encoder(enc)?;
    } else {
        let enc = image::codecs::webp::WebPEncoder::new_with_quality(
            f, image::codecs::webp::WebPQuality::new(opts.webp_quality.clamp(1, 100)),
        );
        img.write_with_encoder(enc)?;
    }
    Ok(())
}
```

> 注意：若当前 `image` 版本中 `WebPQuality` 构造方式不同（`WebPQuality::new` vs `WebPQuality(u8)`），按编译错误调整；其余代码为稳定 API。

- [ ] **Step 4: 运行测试确认通过**

Run: `cargo test convert`
Expected: PASS（3 个测试全绿）

- [ ] **Step 5: Commit**

```bash
git add src/convert.rs src/main.rs
git commit -m "feat: 转换核心 convert.rs（含 alpha 白底合成）"
```

---

### Task 4: 转换页 UI convert_view.rs

**Files:**
- Create: `src/convert_view.rs`
- Modify: `src/app.rs`（集成 `ConvertView`）
- Modify: `src/main.rs`（加 `mod convert_view;`）

**Interfaces:**
- Consumes: `options::{ConvertOptions, TargetFormat, OnConflict, PngCompression}`、`convert::convert_file`
- Produces: `struct ConvertView`，方法 `fn new() -> Self`、`fn ui(&mut self, ui: &mut egui::Ui)`

- [ ] **Step 1: 实现 src/convert_view.rs**

界面包含：添加文件/文件夹按钮、文件列表、目标格式下拉、参数区（按所选格式显示）、输出目录、冲突策略、开始按钮、进度条、逐文件结果行。并行用 `std::thread` + `std::sync::mpsc` 回传结果（每个文件一条消息），UI 在 `ui()` 里 `try_recv` 消费。

```rust
use std::path::PathBuf;
use std::sync::mpsc;

use eframe::egui;

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
                self.files.push(FileRow { path: p, status: RowStatus::Pending });
            }
        }
    }

    fn add_folder(&mut self) {
        if let Some(dir) = rfd::FileDialog::new().pick_folder() {
            if let Ok(entries) = std::fs::read_dir(&dir) {
                let mut paths = Vec::new();
                for e in entries.flatten() {
                    let p = e.path();
                    if p.is_file() { paths.push(p); }
                }
                self.add_files(paths);
            }
        }
    }

    fn start(&mut self) {
        if self.output_dir.is_empty() || self.files.is_empty() { return; }
        let (tx, rx) = mpsc::channel();
        self.rx = Some(rx);
        self.running = self.files.len();
        self.done = 0;
        for r in &mut self.files { r.status = RowStatus::Pending; }

        let files = self.files.iter().map(|r| r.path.clone()).collect::<Vec<_>>();
        let opts = self.opts.clone();
        let out_dir = self.output_dir.clone();

        std::thread::spawn(move || {
            files.into_iter().enumerate().for_each(|(i, src)| {
                let stem = src.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "out".into());
                let dst = PathBuf::from(&out_dir).join(format!("{}.{}", stem, opts.format.extension()));
                let result = if dst.exists() && opts.on_conflict == OnConflict::Skip {
                    Ok(())
                } else {
                    convert_file(&src, &dst, &opts).map_err(|e| e.to_string())
                };
                let _ = tx.send(Msg::Progress { index: i, result });
            });
            let _ = tx.send(Msg::AllDone);
        });
    }

    pub fn ui(&mut self, ui: &mut egui::Ui) {
        // 消费后台消息
        if let Some(rx) = &self.rx {
            loop {
                match rx.try_recv() {
                    Ok(Msg::Progress { index, result }) => {
                        self.files[index].status = RowStatus::Done(result);
                        self.done += 1;
                    }
                    Ok(Msg::AllDone) => { self.rx = None; self.running = 0; break; }
                    Err(_) => break,
                }
            }
        }

        ui.horizontal(|ui| {
            if ui.button("添加文件").clicked() {
                if let Some(paths) = rfd::FileDialog::new().pick_files() { self.add_files(paths); }
            }
            if ui.button("添加文件夹").clicked() { self.add_folder(); }
            if ui.button("清空列表").clicked() { self.files.clear(); }
        });

        ui.horizontal(|ui| {
            ui.label("目标格式:");
            egui::ComboBox::from_id_salt("fmt")
                .selected_text(self.opts.format.label())
                .show_ui(ui, |ui| {
                    for f in TargetFormat::all() {
                        ui.selectable_value(&mut self.opts.format, *f, f.label());
                    }
                });
            ui.label("输出目录:");
            ui.text_edit_singleline(&mut self.output_dir);
            if ui.button("浏览…").clicked() {
                if let Some(d) = rfd::FileDialog::new().pick_folder() {
                    self.output_dir = d.display().to_string();
                }
            }
        });

        // 参数区：按格式显示
        match self.opts.format {
            TargetFormat::Jpeg => {
                ui.horizontal(|ui| {
                    ui.label("质量:");
                    ui.add(egui::Slider::new(&mut self.opts.jpeg_quality, 1..=100));
                });
            }
            TargetFormat::Png => {
                ui.horizontal(|ui| {
                    ui.label("压缩:");
                    ui.selectable_value(&mut self.opts.png_compression, PngCompression::Fast, "快速");
                    ui.selectable_value(&mut self.opts.png_compression, PngCompression::Default, "默认");
                    ui.selectable_value(&mut self.opts.png_compression, PngCompression::Best, "最佳");
                });
            }
            TargetFormat::Webp => {
                ui.horizontal(|ui| {
                    ui.label("质量:");
                    ui.add(egui::Slider::new(&mut self.opts.webp_quality, 1..=100));
                    ui.checkbox(&mut self.opts.webp_lossless, "无损");
                });
            }
            _ => {}
        }

        ui.horizontal(|ui| {
            ui.label("同名文件:");
            ui.selectable_value(&mut self.opts.on_conflict, OnConflict::Skip, "跳过");
            ui.selectable_value(&mut self.opts.on_conflict, OnConflict::Overwrite, "覆盖");
            ui.label("透明背景色:");
            ui.color_edit_button_srgb(&mut self.opts.background);
        });

        let can_start = !self.files.is_empty() && !self.output_dir.is_empty() && self.running == 0;
        if ui.add_enabled(can_start, egui::Button::new("开始转换")).clicked() {
            self.start();
        }
        if self.running > 0 {
            ui.add(egui::ProgressBar::new(self.done as f32 / self.running as f32)
                .text(format!("{}/{}", self.done, self.running)));
        }

        egui::ScrollArea::vertical().show(ui, |ui| {
            for r in &self.files {
                match &r.status {
                    RowStatus::Pending => { ui.label(format!("⏳ {}", r.path.display())); }
                    RowStatus::Done(Ok(())) => { ui.label(format!("✅ {}", r.path.display())); }
                    RowStatus::Done(Err(e)) => { ui.label(format!("❌ {}：{}", r.path.display(), e)); }
                }
            }
        });
    }
}
```

> 需要为 `ConvertOptions` 派生 `Clone`（若尚未）：在 `src/options.rs` 中给 `ConvertOptions` 加 `#[derive(Clone)]`。

- [ ] **Step 2: 集成到 src/app.rs**

把 `ImageApp` 增加字段 `convert: ConvertView`，`new()` 里初始化，`update` 的 `Tab::Convert` 分支改为 `self.convert.ui(ui);`。

- [ ] **Step 3: 验证编译**

Run: `cargo build`
Expected: 编译成功（若报 `WebPQuality`/`color_edit_button_srgb` 等 API 差异，按编译器提示微调）。

- [ ] **Step 4: 手动验证转换**

Run: `cargo run`
Expected: 在转换页添加一张 PNG 图片，选 JPEG、设输出目录，点「开始转换」，进度条推进，列表显示 ✅，输出目录出现 `.jpg` 文件。手动确认后关闭。

- [ ] **Step 5: Commit**

```bash
git add src/convert_view.rs src/app.rs src/main.rs src/options.rs
git commit -m "feat: 转换页 UI（批量并行 + 进度 + 结果列表）"
```

---

### Task 5: 绘制核心 draw.rs

**Files:**
- Create: `src/draw.rs`
- Modify: `src/main.rs`（加 `mod draw;`）

**Interfaces:**
- Consumes: `image`（RgbaImage 等）
- Produces:
  - `enum Annotation { Brush(Vec<[f32;2]>), Line{start:[f32;2], end:[f32;2]}, Arrow{start:[f32;2], end:[f32;2]}, Rect{min:[f32;2], max:[f32;2]}, Ellipse{min:[f32;2], max:[f32;2]}, Text{pos:[f32;2], content:String, size:f32} }`
  - `struct Style { color: [u8;4], width: f32 }`
  - `fn bbox(ann: &Annotation, style: &Style) -> Option<[f32;4]>`（[x,y,w,h]）
  - `fn rasterize(ann: &Annotation, style: &Style, img: &mut RgbaImage, font: Option<&fontdue::Font>)`
  - `fn rasterize_all(anns: &[Annotation], style: &Style, img: &mut RgbaImage, font: Option<&fontdue::Font>)`

- [ ] **Step 1: 写失败测试（src/draw.rs 内 `#[cfg(test)]`）**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_bbox() {
        let a = Annotation::Line { start: [10.0, 20.0], end: [30.0, 5.0] };
        let s = Style { color: [255, 0, 0, 255], width: 2.0 };
        let b = bbox(&a, &s).unwrap();
        assert_eq!(b, [10.0, 5.0, 20.0, 15.0]);
    }

    #[test]
    fn brush_rasterize_sets_pixels() {
        let a = Annotation::Brush(vec![[1.0, 1.0], [2.0, 1.0], [3.0, 1.0]]);
        let s = Style { color: [255, 0, 0, 255], width: 1.0 };
        let mut img = image::RgbaImage::new(5, 5);
        rasterize(&a, &s, &mut img, None);
        // 画笔路径经过的像素被染成红色
        let p = img.get_pixel(1, 1);
        assert_eq!(p, &image::Rgba([255, 0, 0, 255]));
    }

    #[test]
    fn rect_rasterize_draws_border() {
        let a = Annotation::Rect { min: [0.0, 0.0], max: [4.0, 4.0] };
        let s = Style { color: [0, 255, 0, 255], width: 1.0 };
        let mut img = image::RgbaImage::new(6, 6);
        rasterize(&a, &s, &mut img, None);
        assert_eq!(img.get_pixel(0, 0), &image::Rgba([0, 255, 0, 255]));
        // 内部应是空的
        assert_eq!(img.get_pixel(2, 2), &image::Rgba([0, 0, 0, 0]));
    }
}
```

- [ ] **Step 2: 运行测试确认失败**

Run: `cargo test draw`
Expected: FAIL（`Annotation`、`bbox`、`rasterize` 未定义）

- [ ] **Step 3: 实现 src/draw.rs**

使用 Bresenham 画线（画笔、直线、矩形/椭圆边框都基于线）；箭头 = 主线 + 两条头部斜线；椭圆用中点椭圆算法描点；文字用 `fontdue` 栅格化后按位图覆盖。

```rust
use image::RgbaImage;

#[derive(Clone, Debug)]
pub enum Annotation {
    Brush(Vec<[f32; 2]>),
    Line { start: [f32; 2], end: [f32; 2] },
    Arrow { start: [f32; 2], end: [f32; 2] },
    Rect { min: [f32; 2], max: [f32; 2] },
    Ellipse { min: [f32; 2], max: [f32; 2] },
    Text { pos: [f32; 2], content: String, size: f32 },
}

#[derive(Clone, Copy, Debug)]
pub struct Style {
    pub color: [u8; 4],
    pub width: f32,
}

pub fn bbox(ann: &Annotation, style: &Style) -> Option<[f32; 4]> {
    let pad = (style.width / 2.0).ceil();
    let (minx, miny, maxx, maxy) = match ann {
        Annotation::Brush(pts) => {
            let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
            for p in pts {
                x0 = x0.min(p[0]); y0 = y0.min(p[1]);
                x1 = x1.max(p[0]); y1 = y1.max(p[1]);
            }
            if pts.is_empty() { return None; }
            (x0, y0, x1, y1)
        }
        Annotation::Line { start, end } | Annotation::Arrow { start, end } => {
            (start[0].min(end[0]), start[1].min(end[1]), start[0].max(end[0]), start[1].max(end[1]))
        }
        Annotation::Rect { min, max } | Annotation::Ellipse { min, max } => {
            (min[0].min(max[0]), min[1].min(max[1]), min[0].max(max[0]), min[1].max(max[1]))
        }
        Annotation::Text { pos, size, .. } => (pos[0], pos[1], pos[0], pos[1] + size),
    };
    Some([minx - pad, miny - pad, (maxx - minx) + 2.0 * pad, (maxy - miny) + 2.0 * pad])
}

fn put(img: &mut RgbaImage, x: i64, y: i64, c: [u8; 4]) {
    if x >= 0 && y >= 0 && (x as u32) < img.width() && (y as u32) < img.height() {
        img.put_pixel(x as u32, y as u32, image::Rgba(c));
    }
}

fn line(img: &mut RgbaImage, x0: i64, y0: i64, x1: i64, y1: i64, c: [u8; 4]) {
    let (mut x, mut y) = (x0, y0);
    let dx = (x1 - x0).abs();
    let dy = -(y1 - y0).abs();
    let sx = if x0 < x1 { 1 } else { -1 };
    let sy = if y0 < y1 { 1 } else { -1 };
    let mut err = dx + dy;
    loop {
        put(img, x, y, c);
        if x == x1 && y == y1 { break; }
        let e2 = 2 * err;
        if e2 >= dy { err += dy; x += sx; }
        if e2 <= dx { err += dx; y += sy; }
    }
}

pub fn rasterize(ann: &Annotation, style: &Style, img: &mut RgbaImage, font: Option<&fontdue::Font>) {
    let c = style.color;
    match ann {
        Annotation::Brush(pts) => {
            if pts.len() == 1 {
                put(img, pts[0][0] as i64, pts[0][1] as i64, c);
            }
            for w in pts.windows(2) {
                line(img, w[0][0] as i64, w[0][1] as i64, w[1][0] as i64, w[1][1] as i64, c);
            }
        }
        Annotation::Line { start, end } => {
            line(img, start[0] as i64, start[1] as i64, end[0] as i64, end[1] as i64, c);
        }
        Annotation::Arrow { start, end } => {
            line(img, start[0] as i64, start[1] as i64, end[0] as i64, end[1] as i64, c);
            // 箭头头部两条短斜线
            let dx = end[0] - start[0];
            let dy = end[1] - start[1];
            let len = (dx * dx + dy * dy).sqrt().max(1.0);
            let (ux, uy) = (dx / len, dy / len);
            let head = 10.0_f32;
            let (hx, hy) = (-uy, ux);
            let (bx, by) = (end[0] - ux * head, end[1] - uy * head);
            line(img, bx as i64, by as i64, (bx + hx * head * 0.5) as i64, (by + hy * head * 0.5) as i64, c);
            line(img, bx as i64, by as i64, (bx - hx * head * 0.5) as i64, (by - hy * head * 0.5) as i64, c);
        }
        Annotation::Rect { min, max } => {
            let (x0, y0) = (min[0] as i64, min[1] as i64);
            let (x1, y1) = (max[0] as i64, max[1] as i64);
            line(img, x0, y0, x1, y0, c);
            line(img, x1, y0, x1, y1, c);
            line(img, x1, y1, x0, y1, c);
            line(img, x0, y1, x0, y0, c);
        }
        Annotation::Ellipse { min, max } => {
            let cx = ((min[0] + max[0]) / 2.0) as i64;
            let cy = ((min[1] + max[1]) / 2.0) as i64;
            let rx = ((max[0] - min[0]).abs() / 2.0) as i64;
            let ry = ((max[1] - min[1]).abs() / 2.0) as i64;
            midpoint_ellipse(img, cx, cy, rx, ry, c);
        }
        Annotation::Text { pos, content, size } => {
            if let Some(font) = font {
                let (metrics, bitmap) = font.rasterize(content, *size);
                let (mut bx, mut by) = (pos[0] as u32, pos[1] as u32);
                for row in 0..metrics.height {
                    for col in 0..metrics.width {
                        let cov = bitmap[row * metrics.width + col];
                        if cov > 0 {
                            let x = bx + col as u32;
                            let y = by + row as u32;
                            if x < img.width() && y < img.height() {
                                img.put_pixel(x, y, image::Rgba(c));
                            }
                        }
                    }
                }
                let _ = (bx, by);
            }
        }
    }
}

fn midpoint_ellipse(img: &mut RgbaImage, cx: i64, cy: i64, rx: i64, ry: i64, c: [u8; 4]) {
    if rx <= 0 || ry <= 0 { return; }
    let (mut x, mut y) = (0_i64, ry);
    let mut d1 = ry * ry - rx * rx * ry + rx * rx / 4;
    let mut dx = 2 * ry * ry * x;
    let mut dy = 2 * rx * rx * y;
    while dx < dy {
        put(img, cx + x, cy + y, c); put(img, cx - x, cy + y, c);
        put(img, cx + x, cy - y, c); put(img, cx - x, cy - y, c);
        if d1 < 0 { x += 1; dx += 2 * ry * ry; d1 += dx + ry * ry; }
        else { x += 1; y -= 1; dx += 2 * ry * ry; dy -= 2 * rx * rx; d1 += dx - dy + ry * ry; }
    }
    let mut d2 = ry * ry * (x * x + x) + rx * rx * (y * y - y) + rx * rx / 4;
    while y >= 0 {
        put(img, cx + x, cy + y, c); put(img, cx - x, cy + y, c);
        put(img, cx + x, cy - y, c); put(img, cx - x, cy - y, c);
        if d2 > 0 { y -= 1; dy -= 2 * rx * rx; d2 += rx * rx - dy; }
        else { y -= 1; x += 1; dx += 2 * ry * ry; dy -= 2 * rx * rx; d2 += dx - dy + rx * rx; }
    }
}

pub fn rasterize_all(anns: &[Annotation], style: &Style, img: &mut RgbaImage, font: Option<&fontdue::Font>) {
    for a in anns {
        rasterize(a, style, img, font);
    }
}
```

- [ ] **Step 4: 运行测试确认通过**

Run: `cargo test draw`
Expected: PASS（3 个测试全绿）

- [ ] **Step 5: Commit**

```bash
git add src/draw.rs src/main.rs
git commit -m "feat: 绘制核心 draw.rs（画笔/直线/箭头/矩形/椭圆/文字栅格化）"
```

---

### Task 6: 编辑器文档模型 editor.rs

**Files:**
- Create: `src/editor.rs`
- Modify: `src/main.rs`（加 `mod editor;`）

**Interfaces:**
- Consumes: `draw::{Annotation, Style}`
- Produces:
  - `struct CropRect { x: u32, y: u32, w: u32, h: u32 }`
  - `struct EditorDoc { image: image::DynamicImage, annotations: Vec<Annotation>, style: Style, crop: Option<CropRect> }`
  - `impl EditorDoc`：`fn open(path: &Path) -> Result<Self, image::ImageError>`、`fn undo(&mut self)`、`fn redo(&mut self)`、`fn push_annotation(&mut self, a: Annotation)`、`fn remove_last(&mut self)`、`fn apply_crop(&mut self)`、`fn render(&self, font: Option<&fontdue::Font>) -> image::RgbaImage`（裁剪 + 标注合成后输出像素）

- [ ] **Step 1: 写失败测试（src/editor.rs 内 `#[cfg(test)]`）**

```rust
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
        d.push_annotation(Annotation::Line { start: [0.0, 0.0], end: [5.0, 5.0] });
        assert_eq!(d.annotations.len(), 1);
        d.undo();
        assert_eq!(d.annotations.len(), 0);
        d.redo();
        assert_eq!(d.annotations.len(), 1);
    }

    #[test]
    fn apply_crop_shrinks_image() {
        let mut d = doc();
        d.crop = Some(CropRect { x: 2, y: 2, w: 4, h: 4 });
        d.apply_crop();
        assert_eq!(d.image.width(), 4);
        assert_eq!(d.image.height(), 4);
        assert!(d.crop.is_none());
    }

    #[test]
    fn render_composites_annotation() {
        let mut d = doc();
        d.style.color = [255, 0, 0, 255];
        d.push_annotation(Annotation::Brush(vec![[1.0, 1.0], [2.0, 1.0]]));
        let out = d.render(None);
        assert_eq!(out.get_pixel(1, 1), &image::Rgba([255, 0, 0, 255]));
    }
}
```

- [ ] **Step 2: 运行测试确认失败**

Run: `cargo test editor`
Expected: FAIL（`EditorDoc`、`CropRect` 未定义）

- [ ] **Step 3: 实现 src/editor.rs**

```rust
use std::path::Path;

use crate::draw::{self, Annotation, Style};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CropRect {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

pub struct EditorDoc {
    pub image: image::DynamicImage,
    pub annotations: Vec<Annotation>,
    pub style: Style,
    pub crop: Option<CropRect>,
    undo_stack: Vec<Vec<Annotation>>,
    redo_stack: Vec<Vec<Annotation>>,
}

impl EditorDoc {
    pub fn new(image: image::DynamicImage) -> Self {
        Self {
            image,
            annotations: Vec::new(),
            style: Style { color: [255, 0, 0, 255], width: 2.0 },
            crop: None,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
        }
    }

    pub fn open(path: &Path) -> Result<Self, image::ImageError> {
        Ok(Self::new(image::open(path)?))
    }

    pub fn push_annotation(&mut self, a: Annotation) {
        self.undo_stack.push(self.annotations.clone());
        self.redo_stack.clear();
        self.annotations.push(a);
    }

    pub fn undo(&mut self) {
        if let Some(prev) = self.undo_stack.pop() {
            self.redo_stack.push(std::mem::take(&mut self.annotations));
            self.annotations = prev;
        }
    }

    pub fn redo(&mut self) {
        if let Some(next) = self.redo_stack.pop() {
            self.undo_stack.push(std::mem::take(&mut self.annotations));
            self.annotations = next;
        }
    }

    pub fn remove_last(&mut self) {
        if !self.annotations.is_empty() {
            self.undo_stack.push(self.annotations.clone());
            self.redo_stack.clear();
            self.annotations.pop();
        }
    }

    pub fn apply_crop(&mut self) {
        if let Some(c) = self.crop.take() {
            let cropped = self.image.crop_imm(c.x, c.y, c.w, c.h);
            self.image = cropped;
            self.annotations.clear();
            self.undo_stack.clear();
            self.redo_stack.clear();
        }
    }

    pub fn render(&self, font: Option<&fontdue::Font>) -> image::RgbaImage {
        let mut out = self.image.to_rgba8();
        draw::rasterize_all(&self.annotations, &self.style, &mut out, font);
        out
    }
}
```

- [ ] **Step 4: 运行测试确认通过**

Run: `cargo test editor`
Expected: PASS（3 个测试全绿）

- [ ] **Step 5: Commit**

```bash
git add src/editor.rs src/main.rs
git commit -m "feat: 编辑器文档模型 editor.rs（标注/撤销重做/裁剪/合成）"
```

---

### Task 7: 编辑页 UI editor_view.rs

**Files:**
- Create: `src/editor_view.rs`
- Modify: `src/app.rs`（集成 `EditorView`）
- Modify: `src/main.rs`（加 `mod editor_view;`）

**Interfaces:**
- Consumes: `editor::{EditorDoc, CropRect}`、`draw::{Annotation, Style}`
- Produces: `struct EditorView`，方法 `fn new() -> Self`、`fn ui(&mut self, ui: &mut egui::Ui)`

- [ ] **Step 1: 实现字体加载与 EditorView 骨架**

`EditorView` 字段：`doc: Option<EditorDoc>`、`tool: Tool`（枚举：Select/Brush/Line/Arrow/Rect/Ellipse/Text/Crop/CropRatio）、`crop_ratio: Option<(u32,u32)>`、`zoom: f32`、`pan: egui::Vec2`、`font: fontdue::Font`、`save_dialog: bool`。

字体加载函数：

```rust
fn load_font() -> fontdue::Font {
    let paths = [
        "C:\\Windows\\Fonts\\msyh.ttc",
        "C:\\Windows\\Fonts\\simhei.ttf",
        "C:\\Windows\\Fonts\\simsun.ttc",
    ];
    for p in paths {
        if let Ok(bytes) = std::fs::read(p) {
            let settings = fontdue::FontSettings { collection_index: 0, ..Default::default() };
            if let Ok(f) = fontdue::Font::from_bytes(bytes, settings) {
                return f;
            }
        }
    }
    // 内置回退：嵌入一个极小的开源字体字节（见下）
    fontdue::Font::from_bytes(include_bytes!("../assets/fallback.ttf"), fontdue::FontSettings::default())
        .expect("builtin font")
}
```

> 内置回退字体：Task 7 需在 `assets/fallback.ttf` 放一个任意开源 TTF（如从系统 `C:\Windows\Fonts\arial.ttf` 复制一份），保证 `cargo` 不因缺文件而失败。

- [ ] **Step 2: 实现工具切换与画布绘制交互**

用 `egui::CentralPanel` + `ui.allocate_painter` 画背景图（缩放/平移后的纹理）与已有标注（用 `Painter` 的 line_segment / rect_stroke / circle_stroke / text）。鼠标交互按 `tool` 分发：

- Select：拖拽移动最近标注
- Brush：按下拖动累积点，松开时 `doc.push_annotation(Annotation::Brush(pts))`
- Line/Arrow/Rect/Ellipse：按下记起点，拖动预览，松开提交对应 `Annotation`
- Text：点击弹出一个 `egui::TextEdit` 输入框，回车提交 `Annotation::Text`
- Crop：拖拽出 `CropRect`，工具栏「应用裁剪」按钮调用 `doc.apply_crop()`
- CropRatio：选比例后拖拽时保持宽高比

撤销/重做/删除/清空分别调用 `doc.undo()/redo()/remove_last()/annotations.clear()`。

- [ ] **Step 3: 实现保存对话框**

`ui` 里当 `save_dialog` 为 true 时用 `egui::Window` 显示两个按钮：

- 「另存为新文件」：`rfd::FileDialog::new().add_filter(...).save_file()` 选路径，用 `doc.render(Some(&font))` 结果按扩展名 `save`（复用 `convert.rs` 的编码逻辑或直接 `RgbaImage::save`）
- 「覆盖原图」：记录打开时的原路径，直接 `doc.render(Some(&font)).save(original_path)`

- [ ] **Step 4: 集成到 src/app.rs 并验证编译**

把 `EditorView` 加入 `ImageApp`，`Tab::Editor` 分支调用 `self.editor.ui(ui)`。

Run: `cargo build`
Expected: 编译成功。

- [ ] **Step 5: 手动验证编辑**

Run: `cargo run`
Expected: 编辑页能打开一张图，用画笔/直线/矩形画标注，撤销重做生效，裁剪生效，保存对话框能另存为 PNG。手动确认后关闭。

- [ ] **Step 6: Commit**

```bash
git add src/editor_view.rs src/app.rs src/main.rs assets/
git commit -m "feat: 编辑页 UI（工具栏/画布/裁剪/保存）"
```

---

### Task 8: 集成收尾与发布验证

**Files:**
- Modify: `src/app.rs`（完善 Tab 切换与整体交互）
- Create: `README.md`（使用说明 + 打包命令）
- Create: `.gitignore`

- [ ] **Step 1: 完善 app.rs 集成与细节**

确认 `ImageApp` 同时持有 `ConvertView` 与 `EditorView`，Tab 切换状态正确；编辑页打开新图前若有未保存修改则弹出提示（用 `egui::Window` 或 `ctx` 弹窗）。

- [ ] **Step 2: 写 README.md 与 .gitignore**

`.gitignore`：

```gitignore
/target
```

`README.md` 简述功能、运行方式（`cargo run`）、发布打包（`cargo build --release`，产物在 `target/release/image-converter.exe`）。

- [ ] **Step 3: 全量测试**

Run: `cargo test`
Expected: 全部单元测试通过（options / convert / draw / editor）。

- [ ] **Step 4: release 构建**

Run: `cargo build --release`
Expected: 生成 `target/release/image-converter.exe`，双击可运行（无控制台依赖、无需运行时）。

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "chore: 集成收尾、README 与发布说明"
```

---

## Self-Review

- **Spec coverage**：转换（Task 2/3/4）、编辑（Task 5/6/7）、Tab 骨架（Task 1/8）、格式清单与参数（Task 2）、alpha 白底（Task 3）、并行/进度/冲突策略（Task 4）、裁剪比例+自由（Task 6/7）、撤销重做（Task 6/7）、保存二选一（Task 7）、中文字体（Task 7）、测试策略（各 Task 测试）。无遗漏。
- **Placeholder scan**：无 TBD/TODO；每个代码步骤均有完整代码或明确 API 说明。
- **Type consistency**：`ConvertOptions`/`TargetFormat`（Task 2 定义，Task 3/4 使用）；`Annotation`/`Style`（Task 5 定义，Task 6/7 使用）；`EditorDoc::new/open/push_annotation/undo/redo/remove_last/apply_crop/render`（Task 6 定义，Task 7 使用）。命名一致。
