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
                Tab::Convert => {
                    ui.centered_and_justified(|ui| {
                        ui.label("转换页（待实现）");
                    });
                }
                Tab::Editor => {
                    ui.centered_and_justified(|ui| {
                        ui.label("编辑页（待实现）");
                    });
                }
            }
        });
    }
}
