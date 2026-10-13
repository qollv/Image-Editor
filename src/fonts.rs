//! 系统字体加载：界面字体 / 文本栅格化字体 / 水印字体三处需求统一收口。
//!
//! 背景：msyh.ttc 约 18.8MB，fontdue 完整解析实测约 1.8s——
//! - 水印字体原先每文件加载一次（批量转换 = 每张多付 1.8s），改 OnceLock 全程一次；
//! - 编辑器字体原先在启动时加载（不用编辑器也付 1.8s），改懒加载（首次保存时才付）。

use std::sync::OnceLock;

use eframe::egui;

/// Windows 常见中文字体路径，按优先级探测。
const FONT_CANDIDATES: [&str; 3] = [
    "C:\\Windows\\Fonts\\msyh.ttc",
    "C:\\Windows\\Fonts\\simhei.ttf",
    "C:\\Windows\\Fonts\\simsun.ttc",
];

/// 读取第一个可用的字体字节；中文字体都找不到时回退 Arial（保证界面至少有字形）。
fn read_font_bytes() -> Option<Vec<u8>> {
    for p in FONT_CANDIDATES {
        if let Ok(bytes) = std::fs::read(p) {
            return Some(bytes);
        }
    }
    std::fs::read("C:\\Windows\\Fonts\\arial.ttf").ok()
}

/// 解析 fontdue 文本字体（文字标注栅格化 / 水印共用）；失败返回 None，由调用方降级。
pub fn load_text_font() -> Option<fontdue::Font> {
    let bytes = read_font_bytes()?;
    fontdue::Font::from_bytes(
        bytes,
        fontdue::FontSettings {
            collection_index: 0,
            ..Default::default()
        },
    )
    .ok()
}

/// egui 界面字体定义：注入中文字体并设为最高优先级（egui 默认字体不含中文字形）。
pub fn egui_font_definitions() -> egui::FontDefinitions {
    let mut fonts = egui::FontDefinitions::default();
    if let Some(bytes) = read_font_bytes() {
        fonts.font_data.insert(
            "chinese".to_owned(),
            std::sync::Arc::new(egui::FontData::from_owned(bytes)),
        );
        if let Some(fam) = fonts.families.get_mut(&egui::FontFamily::Proportional) {
            fam.insert(0, "chinese".to_owned());
        }
        if let Some(fam) = fonts.families.get_mut(&egui::FontFamily::Monospace) {
            fam.insert(0, "chinese".to_owned());
        }
    }
    fonts
}

/// 水印字体：全程只解析一次（首次带水印转换时付 1.8s，之后所有文件零成本）。
static WATERMARK_FONT: OnceLock<Option<fontdue::Font>> = OnceLock::new();

pub fn watermark_font() -> Option<&'static fontdue::Font> {
    WATERMARK_FONT.get_or_init(load_text_font).as_ref()
}
