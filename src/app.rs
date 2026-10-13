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
    ctx.set_fonts(crate::fonts::egui_font_definitions());
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

    /// 应用标识：品牌紫圆角方块 + 白日 + 白山（与窗口图标同一视觉语言）。
    fn logo(ui: &mut egui::Ui, theme: &crate::theme::Theme) {
        let size = egui::vec2(24.0, 24.0);
        let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
        let p = ui.painter();
        p.rect_filled(rect, egui::CornerRadius::same(7), theme.accent);
        // 太阳
        p.circle_filled(
            rect.min + egui::vec2(17.0, 7.5),
            3.2,
            egui::Color32::WHITE,
        );
        // 山（三角）
        let a = rect.min + egui::vec2(5.5, 18.0);
        let b = rect.min + egui::vec2(12.0, 9.0);
        let c = rect.min + egui::vec2(19.5, 18.0);
        p.add(egui::Shape::convex_polygon(
            vec![a, b, c],
            egui::Color32::from_rgba_unmultiplied(255, 255, 255, 235),
            egui::Stroke::NONE,
        ));
    }

    /// 胶囊 Tab：选中态品牌紫实心 + 白字，未选中透明 + 次级文字色。
    fn tab_button(ui: &mut egui::Ui, theme: &crate::theme::Theme, label: &str, selected: bool) -> egui::Response {
        let text_color = if selected {
            egui::Color32::WHITE
        } else {
            theme.text_secondary
        };
        ui.add(
            egui::Button::new(egui::RichText::new(label).color(text_color))
                .fill(if selected {
                    theme.accent
                } else {
                    egui::Color32::TRANSPARENT
                })
                .corner_radius(theme.radius_pill)
                .min_size(egui::vec2(104.0, 30.0)),
        )
    }

    /// 自绘窗口按钮（无边框窗口用）：矢量图标，不依赖字体字形。
    fn window_button(
        ui: &mut egui::Ui,
        theme: &crate::theme::Theme,
        kind: WindowButton,
    ) -> egui::Response {
        let (rect, resp) =
            ui.allocate_exact_size(egui::vec2(38.0, 26.0), egui::Sense::click());
        if resp.hovered() {
            let hover = if kind == WindowButton::Close {
                theme.error.gamma_multiply(0.28)
            } else {
                theme.bg_surface_hover
            };
            ui.painter()
                .rect_filled(rect, theme.radius_button, hover);
        }
        let c = theme.text_secondary;
        let p = ui.painter();
        let center = rect.center();
        let cx = center.x;
        let cy = center.y;
        let stroke = egui::Stroke::new(1.2_f32, c);
        match kind {
            WindowButton::Minimize => {
                p.line_segment(
                    [egui::pos2(cx - 5.0, cy), egui::pos2(cx + 5.0, cy)],
                    stroke,
                );
            }
            WindowButton::Maximize => {
                let r = egui::Rect::from_center_size(center, egui::vec2(10.0, 10.0));
                p.rect_stroke(r, egui::CornerRadius::same(2), stroke, egui::StrokeKind::Middle);
            }
            WindowButton::Restore => {
                // 两个错位矩形（前实后虚的简化表达）
                let back = egui::Rect::from_min_size(
                    egui::pos2(cx - 5.0, cy - 5.0),
                    egui::vec2(8.0, 8.0),
                );
                let front = egui::Rect::from_min_size(
                    egui::pos2(cx - 3.0, cy - 3.0),
                    egui::vec2(8.0, 8.0),
                );
                p.rect_stroke(back, egui::CornerRadius::same(1), stroke, egui::StrokeKind::Middle);
                p.rect_stroke(front, egui::CornerRadius::same(1), stroke, egui::StrokeKind::Middle);
            }
            WindowButton::Close => {
                p.line_segment(
                    [egui::pos2(cx - 5.0, cy - 5.0), egui::pos2(cx + 5.0, cy + 5.0)],
                    stroke,
                );
                p.line_segment(
                    [egui::pos2(cx + 5.0, cy - 5.0), egui::pos2(cx - 5.0, cy + 5.0)],
                    stroke,
                );
            }
        }
        resp
    }
}

#[derive(PartialEq, Clone, Copy)]
enum WindowButton {
    Minimize,
    Maximize,
    Restore,
    Close,
}

impl eframe::App for ImageApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let theme = crate::theme::Theme::dark();

        // 顶部品牌栏 = 自绘标题栏：logo + 产品名 + 胶囊 Tab + 窗口按钮，
        // 底部 1px 发丝线分区；空白处按住拖动窗口，双击切换最大化
        let header = egui::TopBottomPanel::top("header")
            .frame(theme.panel())
            .show(ctx, |ui| {
                // 先注册整条顶栏为拖动区，保证后添加的 Tab/按钮拿得上层的点击
                let drag = ui.interact(
                    ui.max_rect(),
                    ui.id().with("titlebar_drag"),
                    egui::Sense::click_and_drag(),
                );
                if drag.dragged() {
                    ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
                }
                if drag.double_clicked() {
                    let maximized = ctx.input(|i| i.viewport().maximized).unwrap_or(false);
                    ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!maximized));
                }
                ui.horizontal(|ui| {
                    Self::logo(ui, &theme);
                    ui.add_space(6.0);
                    ui.label(
                        egui::RichText::new("图片工作台")
                            .strong()
                            .color(theme.text_primary),
                    );
                    ui.add_space(28.0);
                    for (tab, label) in [
                        (Tab::Convert, "格式转换"),
                        (Tab::Editor, "图片编辑"),
                    ] {
                        if Self::tab_button(ui, &theme, label, self.tab == tab).clicked() {
                            self.tab = tab;
                        }
                    }
                    // 右侧窗口按钮（右往左排：关闭 / 最大化 / 最小化）
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let maximized = ctx.input(|i| i.viewport().maximized).unwrap_or(false);
                        if Self::window_button(ui, &theme, WindowButton::Close).clicked() {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                        let max_kind = if maximized {
                            WindowButton::Restore
                        } else {
                            WindowButton::Maximize
                        };
                        if Self::window_button(ui, &theme, max_kind).clicked() {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!maximized));
                        }
                        if Self::window_button(ui, &theme, WindowButton::Minimize).clicked() {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
                        }
                    });
                });
            });
        // 面板画在 background 层，发丝线后画即可覆盖在其上
        let r = header.response.rect;
        ctx.layer_painter(egui::LayerId::background()).line_segment(
            [
                egui::pos2(r.left(), r.bottom()),
                egui::pos2(r.right(), r.bottom()),
            ],
            egui::Stroke::new(1.0_f32, theme.border),
        );
        // 无边框窗口的外缘轮廓：1px 发丝线勾边，避免暗色窗口融进桌面
        let screen = ctx.screen_rect();
        ctx.layer_painter(egui::LayerId::background()).rect_stroke(
            screen,
            egui::CornerRadius::ZERO,
            egui::Stroke::new(1.0_f32, theme.border_strong),
            egui::StrokeKind::Inside,
        );

        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(theme.bg_deep).inner_margin(egui::Margin::symmetric(14, 12)))
            .show(ctx, |ui| {
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
