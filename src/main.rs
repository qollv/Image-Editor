mod app;
mod convert;
mod convert_view;
mod options;

use eframe::egui;

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
