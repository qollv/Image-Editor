//! 「暗房工作台」设计系统：深色主题 + 一套设计 token。
//!
//! 设计理由：图片工具采用深色环境是行业惯例（Photoshop / Lightroom /
//! Affinity 同款）——环境越暗，人眼对图片本身明暗与色彩的判断越准，
//! 界面不抢画面的注意力。所有颜色/圆角/间距集中在此，全应用统一引用。

use eframe::egui::{self, Color32, CornerRadius, Margin, Stroke, Vec2};

/// 全局设计 token（暗房主题）。
pub struct Theme {
    // 背景层次：deep（最底层）→ panel（工具栏/面板）→ surface（卡片/输入）
    pub bg_deep: Color32,
    pub bg_panel: Color32,
    pub bg_surface: Color32,
    pub bg_surface_hover: Color32,
    pub border: Color32,
    pub border_strong: Color32,

    // 文字层次
    pub text_primary: Color32,
    pub text_secondary: Color32,
    pub text_muted: Color32,

    // 品牌强调色（紫）：交互焦点
    pub accent: Color32,
    pub accent_hover: Color32,
    /// 选中态底色的淡化版本（如选中的工具按钮、列表选中行）
    pub accent_soft: Color32,

    // 语义色
    pub success: Color32,
    pub error: Color32,
    pub warning: Color32,

    // 画布
    pub canvas_bg: Color32,

    // 圆角体系
    pub radius_button: CornerRadius,
    pub radius_panel: CornerRadius,
    pub radius_pill: CornerRadius,
}

impl Theme {
    pub const fn dark() -> Self {
        Self {
            bg_deep: Color32::from_rgb(0x14, 0x15, 0x19),
            bg_panel: Color32::from_rgb(0x1D, 0x1E, 0x24),
            bg_surface: Color32::from_rgb(0x26, 0x27, 0x2F),
            bg_surface_hover: Color32::from_rgb(0x30, 0x31, 0x3B),
            border: Color32::from_rgb(0x34, 0x35, 0x3F),
            border_strong: Color32::from_rgb(0x45, 0x46, 0x52),

            text_primary: Color32::from_rgb(0xE9, 0xEA, 0xF0),
            text_secondary: Color32::from_rgb(0xA8, 0xAB, 0xB8),
            text_muted: Color32::from_rgb(0x71, 0x73, 0x82),

            accent: Color32::from_rgb(0x6E, 0x62, 0xE8),
            accent_hover: Color32::from_rgb(0x86, 0x7B, 0xF0),
            accent_soft: Color32::from_rgb(0x2E, 0x2A, 0x4E),

            success: Color32::from_rgb(0x37, 0xB9, 0x8A),
            error: Color32::from_rgb(0xE2, 0x5C, 0x5C),
            warning: Color32::from_rgb(0xE5, 0xA2, 0x3C),

            canvas_bg: Color32::from_rgb(0x10, 0x11, 0x14),

            radius_button: CornerRadius::same(6),
            radius_panel: CornerRadius::same(10),
            radius_pill: CornerRadius::same(16),
        }
    }

    /// 卡片框架：surface 底 + 1px 边框 + 圆角 + 内边距。
    pub fn card(&self) -> egui::Frame {
        egui::Frame::NONE
            .fill(self.bg_surface)
            .stroke(Stroke::new(1.0_f32, self.border))
            .corner_radius(self.radius_panel)
            .inner_margin(Margin::symmetric(12, 10))
    }

    /// 面板框架（顶栏等）：panel 底，无边框。
    pub fn panel(&self) -> egui::Frame {
        egui::Frame::NONE
            .fill(self.bg_panel)
            .inner_margin(Margin::symmetric(14, 10))
    }

    /// 状态徽标：淡色底 + 同色系文字 + 细边。
    pub fn badge(&self, color: Color32) -> egui::Frame {
        egui::Frame::NONE
            .fill(color.gamma_multiply(0.14))
            .stroke(Stroke::new(1.0_f32, color.gamma_multiply(0.45)))
            .corner_radius(CornerRadius::same(10))
            .inner_margin(Margin::symmetric(9, 3))
    }
}

/// 把暗房主题应用到 egui Context（启动时调用一次）。
///
/// 注意：egui 0.31 起样式分 dark/light 两个槽，默认跟随系统主题
/// （ThemePreference::System）。若只 set_style，在亮色系统的 Windows 上
/// ctx.style() 会返回默认亮色样式——所有主题控件（按钮/滑块/下拉）渲染成
/// 亮色系，只有显式 fill 的部分是暗色。因此这里双槽都写入并强制暗色偏好。
pub fn apply(ctx: &egui::Context) {
    let style = build_style();
    ctx.set_style_of(egui::Theme::Dark, style.clone());
    ctx.set_style_of(egui::Theme::Light, style);
    ctx.options_mut(|opt| opt.theme_preference = egui::ThemePreference::Dark);
}

/// 基于 egui 默认暗色样式构建暗房主题。
fn build_style() -> egui::Style {
    let theme = Theme::dark();
    let mut style = egui::Style::default();
    let v = &mut style.visuals;

    v.dark_mode = true;
    v.override_text_color = Some(theme.text_primary);

    // 全局背景与容器
    v.panel_fill = theme.bg_deep;
    v.window_fill = theme.bg_surface;
    v.window_stroke = Stroke::new(1.0_f32, theme.border);
    v.window_corner_radius = theme.radius_panel;
    v.extreme_bg_color = theme.bg_deep; // 文本输入框、滚动条槽
    v.faint_bg_color = theme.bg_surface; // 网格斑马纹
    v.code_bg_color = theme.bg_deep;
    v.warn_fg_color = theme.warning;
    v.error_fg_color = theme.error;
    v.hyperlink_color = theme.accent;

    // 选中（selectable_value / 文本选择）
    v.selection.bg_fill = theme.accent_soft;
    v.selection.stroke = Stroke::new(1.0_f32, theme.accent);

    // 控件五态。注意 egui 按钮用 weak_bg_fill（可选背景），bg_fill 用于
    // 滑块槽/复选框等必须有底的控件。
    v.widgets.noninteractive = egui::style::WidgetVisuals {
        bg_fill: theme.bg_panel,
        weak_bg_fill: Color32::TRANSPARENT,
        bg_stroke: Stroke::NONE,
        fg_stroke: Stroke::new(1.0_f32, theme.text_secondary),
        corner_radius: CornerRadius::same(4),
        expansion: 0.0,
    };
    v.widgets.inactive = egui::style::WidgetVisuals {
        bg_fill: theme.bg_surface,
        weak_bg_fill: theme.bg_surface,
        bg_stroke: Stroke::new(1.0_f32, theme.border),
        fg_stroke: Stroke::new(1.0_f32, theme.text_primary),
        corner_radius: theme.radius_button,
        expansion: 0.0,
    };
    v.widgets.hovered = egui::style::WidgetVisuals {
        bg_fill: theme.bg_surface_hover,
        weak_bg_fill: theme.bg_surface_hover,
        bg_stroke: Stroke::new(1.0_f32, theme.border_strong),
        fg_stroke: Stroke::new(1.2_f32, Color32::WHITE),
        corner_radius: theme.radius_button,
        expansion: 0.0,
    };
    v.widgets.active = egui::style::WidgetVisuals {
        bg_fill: theme.accent,
        weak_bg_fill: theme.accent,
        bg_stroke: Stroke::new(1.0_f32, theme.accent_hover),
        fg_stroke: Stroke::new(1.2_f32, Color32::WHITE),
        corner_radius: theme.radius_button,
        expansion: 0.0,
    };
    v.widgets.open = egui::style::WidgetVisuals {
        bg_fill: theme.bg_surface_hover,
        weak_bg_fill: theme.bg_surface_hover,
        bg_stroke: Stroke::new(1.0_f32, theme.border_strong),
        fg_stroke: Stroke::new(1.0_f32, theme.text_primary),
        corner_radius: theme.radius_button,
        expansion: 0.0,
    };

    // 间距体系：宽松呼吸感 + 足够的点击热区
    style.spacing.item_spacing = Vec2::new(10.0, 8.0);
    style.spacing.button_padding = Vec2::new(12.0, 6.0);
    style.spacing.menu_margin = Margin::same(8);
    style.spacing.window_margin = Margin::symmetric(12, 10);
    style.spacing.interact_size = Vec2::new(40.0, 24.0); // 最小可点击高度 24
    style.spacing.slider_width = 160.0;
    style.spacing.combo_width = 120.0;
    style.spacing.icon_width = 16.0;

    style
}

/// 生成应用图标（64×64，程序化绘制，与界面 logo 同一视觉语言）：
/// 暗底圆角方块 + 右上白日 + 左下白山。
pub fn app_icon() -> egui::IconData {
    const S: u32 = 64;
    let mut rgba = vec![0u8; (S * S * 4) as usize];
    let mut put = |x: u32, y: u32, r: u8, g: u8, b: u8, a: u8| {
        let i = ((y * S + x) * 4) as usize;
        rgba[i] = r;
        rgba[i + 1] = g;
        rgba[i + 2] = b;
        rgba[i + 3] = a;
    };
    // 圆角方底（品牌紫），圆角半径 14
    let radius = 14.0_f32;
    for y in 0..S {
        for x in 0..S {
            let (fx, fy) = (x as f32, y as f32);
            // 到最近角的距离超出半径的部分需要裁掉
            let cx = fx.min(S as f32 - 1.0 - fx).max(0.0);
            let cy = fy.min(S as f32 - 1.0 - fy).max(0.0);
            let corner = (cx - (radius - 1.0)).hypot(cy - (radius - 1.0));
            let inside = cx <= radius || cy <= radius || corner <= radius;
            if inside {
                put(x, y, 0x6E, 0x62, 0xE8, 255);
            }
        }
    }
    // 太阳（右上白色圆）
    let (sx, sy, sr) = (44.0_f32, 20.0_f32, 8.0_f32);
    for y in 0..S {
        for x in 0..S {
            let d = ((x as f32 - sx).powi(2) + (y as f32 - sy).powi(2)).sqrt();
            if d <= sr {
                put(x, y, 0xFF, 0xFF, 0xFF, 255);
            }
        }
    }
    // 山（左下白色三角形，用两条白边模拟轮廓 + 内部填充）
    let (ax, ay) = (14.0_f32, 48.0_f32); // 左峰底
    let (bx, by) = (32.0_f32, 24.0_f32); // 主峰顶
    let (cx, cy) = (52.0_f32, 48.0_f32); // 右峰底
    for y in 0..S {
        for x in 0..S {
            let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
            // 三角形内部判定（边函数同号）
            let side = |x1: f32, y1: f32, x2: f32, y2: f32| (x2 - x1) * (py - y1) - (y2 - y1) * (px - x1);
            let d1 = side(ax, ay, bx, by);
            let d2 = side(bx, by, cx, cy);
            let d3 = side(cx, cy, ax, ay);
            let all_pos = d1 >= 0.0 && d2 >= 0.0 && d3 >= 0.0;
            let all_neg = d1 <= 0.0 && d2 <= 0.0 && d3 <= 0.0;
            if all_pos || all_neg {
                put(x, y, 0xFF, 0xFF, 0xFF, 235);
            }
        }
    }
    egui::IconData { rgba, width: S, height: S }
}
