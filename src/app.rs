use eframe::egui;

use crate::convert_view::ConvertView;
use crate::editor_view::EditorView;

#[derive(PartialEq, Clone, Copy)]
pub enum Tab {
    Convert,
    Editor,
}

pub struct ImageApp {
    tab: Tab,
    convert: ConvertView,
    editor: EditorView,
}

/// egui 默认字体不含中文字形，从系统加载中文 TTC/TTF 并设为最高优先级。
fn setup_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    for path in [
        "C:\\Windows\\Fonts\\msyh.ttc",
        "C:\\Windows\\Fonts\\simhei.ttf",
        "C:\\Windows\\Fonts\\simsun.ttc",
    ] {
        if let Ok(bytes) = std::fs::read(path) {
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
            break;
        }
    }
    ctx.set_fonts(fonts);
}

impl ImageApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        setup_fonts(&cc.egui_ctx);
        Self {
            tab: Tab::Convert,
            convert: ConvertView::new(),
            editor: EditorView::new(),
        }
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
                Tab::Convert => {
                    self.convert.ui(ui);
                }
                Tab::Editor => {
                    self.editor.ui(ui);
                }
            }
        });
    }
}
