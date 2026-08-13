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

impl ImageApp {
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
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
