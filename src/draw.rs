use image::RgbaImage;

/// 标注的几何形状（不含样式）。
#[derive(Clone, Debug)]
pub enum Shape {
    Brush(Vec<[f32; 2]>),
    Line { start: [f32; 2], end: [f32; 2] },
    Arrow { start: [f32; 2], end: [f32; 2] },
    Rect { min: [f32; 2], max: [f32; 2] },
    Ellipse { min: [f32; 2], max: [f32; 2] },
    Text { pos: [f32; 2], content: String, size: f32 },
    /// 马赛克遮挡（对矩形区域像素化，block 为块大小）
    Mosaic { min: [f32; 2], max: [f32; 2], block: u32 },
}

/// 一条完整标注 = 几何形状 + 创建时定格的自身样式。
/// 之后在工具栏改颜色/线宽只影响新标注，不污染已画的标注。
#[derive(Clone, Debug)]
pub struct Annotation {
    pub shape: Shape,
    pub style: Style,
}

#[derive(Clone, Copy, Debug)]
pub struct Style {
    pub color: [u8; 4],
    pub width: f32,
}

pub fn bbox(shape: &Shape, style: &Style) -> Option<[f32; 4]> {
    // 箭头头部是实心三角，向垂直方向扩展 1.1 倍线宽，比普通标注的半线宽大
    let pad = if matches!(shape, Shape::Arrow { .. }) {
        (style.width * 1.1).ceil()
    } else {
        (style.width / 2.0).ceil()
    };
    let (minx, miny, maxx, maxy) = match shape {
        Shape::Brush(pts) => {
            if pts.is_empty() {
                return None;
            }
            let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
            for p in pts {
                x0 = x0.min(p[0]);
                y0 = y0.min(p[1]);
                x1 = x1.max(p[0]);
                y1 = y1.max(p[1]);
            }
            (x0, y0, x1, y1)
        }
        Shape::Line { start, end } | Shape::Arrow { start, end } => {
            (
                start[0].min(end[0]),
                start[1].min(end[1]),
                start[0].max(end[0]),
                start[1].max(end[1]),
            )
        }
        Shape::Rect { min, max } | Shape::Ellipse { min, max } | Shape::Mosaic { min, max, .. } => {
            (
                min[0].min(max[0]),
                min[1].min(max[1]),
                min[0].max(max[0]),
                min[1].max(max[1]),
            )
        }
        Shape::Text { pos, content, size } => {
            // 近似宽度：每个字符约 0.6 倍字号（中文等宽近似）
            let w = content.chars().count() as f32 * *size * 0.6;
            (pos[0], pos[1], pos[0] + w, pos[1] + size)
        }
    };
    Some([
        minx - pad,
        miny - pad,
        (maxx - minx) + 2.0 * pad,
        (maxy - miny) + 2.0 * pad,
    ])
}

fn put(img: &mut RgbaImage, x: i64, y: i64, c: [u8; 4]) {
    if x >= 0 && y >= 0 && (x as u32) < img.width() && (y as u32) < img.height() {
        img.put_pixel(x as u32, y as u32, image::Rgba(c));
    }
}

/// 以 (x,y) 为圆心、半径 r 的实心圆盘（用于实现线宽）。
fn put_disc(img: &mut RgbaImage, x: i64, y: i64, r: i64, c: [u8; 4]) {
    for dy in -r..=r {
        for dx in -r..=r {
            if dx * dx + dy * dy <= r * r {
                put(img, x + dx, y + dy, c);
            }
        }
    }
}

fn line(img: &mut RgbaImage, x0: i64, y0: i64, x1: i64, y1: i64, c: [u8; 4], r: i64) {
    let (mut x, mut y) = (x0, y0);
    let dx = (x1 - x0).abs();
    let dy = -(y1 - y0).abs();
    let sx = if x0 < x1 { 1 } else { -1 };
    let sy = if y0 < y1 { 1 } else { -1 };
    let mut err = dx + dy;
    loop {
        put_disc(img, x, y, r, c);
        if x == x1 && y == y1 {
            break;
        }
        let e2 = 2 * err;
        if e2 >= dy {
            err += dy;
            x += sx;
        }
        if e2 <= dx {
            err += dx;
            y += sy;
        }
    }
}

/// 实心三角形扫描线填充（箭头头部用）。
/// 顶点按 y 排序后，每条扫描线取长边与短边的交点区间填满。
fn fill_triangle(img: &mut RgbaImage, pts: [[f32; 2]; 3], c: [u8; 4]) {
    let mut p = pts;
    if p[0][1] > p[1][1] {
        p.swap(0, 1);
    }
    if p[1][1] > p[2][1] {
        p.swap(1, 2);
        if p[0][1] > p[1][1] {
            p.swap(0, 1);
        }
    }
    let (top, mid, bot) = (p[0], p[1], p[2]);
    let height = bot[1] - top[1];
    if height < 0.5 {
        return; // 近平铺的退化三角形
    }
    for y in (top[1].ceil() as i64)..=(bot[1].floor() as i64) {
        let fy = y as f32;
        // 长边（top→bot）交点
        let t_long = (fy - top[1]) / height;
        let x_long = top[0] + t_long * (bot[0] - top[0]);
        // 短边交点：fy 落在 top→mid 段还是 mid→bot 段
        let x_short = if fy < mid[1] {
            let span = (mid[1] - top[1]).max(f32::EPSILON);
            top[0] + (fy - top[1]) / span * (mid[0] - top[0])
        } else {
            let span = (bot[1] - mid[1]).max(f32::EPSILON);
            mid[0] + (fy - mid[1]) / span * (bot[0] - mid[0])
        };
        let (x0, x1) = if x_long <= x_short {
            (x_long, x_short)
        } else {
            (x_short, x_long)
        };
        for x in (x0.round() as i64)..=(x1.round() as i64) {
            put(img, x, y, c);
        }
    }
}

pub fn rasterize(shape: &Shape, style: &Style, img: &mut RgbaImage, font: Option<&fontdue::Font>) {
    let c = style.color;
    // 线宽 → 圆盘半径：width=1 → r=0（单像素），width=3 → r=1，以此类推
    let r = ((style.width - 1.0) / 2.0).ceil().max(0.0) as i64;
    match shape {
        Shape::Brush(pts) => {
            if pts.len() == 1 {
                put_disc(img, pts[0][0] as i64, pts[0][1] as i64, r, c);
            }
            for w in pts.windows(2) {
                line(
                    img,
                    w[0][0] as i64,
                    w[0][1] as i64,
                    w[1][0] as i64,
                    w[1][1] as i64,
                    c,
                    r,
                );
            }
        }
        Shape::Line { start, end } => {
            line(
                img,
                start[0] as i64,
                start[1] as i64,
                end[0] as i64,
                end[1] as i64,
                c,
                r,
            );
        }
        Shape::Arrow { start, end } => {
            let dx = end[0] - start[0];
            let dy = end[1] - start[1];
            let len = (dx * dx + dy * dy).sqrt().max(1.0);
            let (ux, uy) = (dx / len, dy / len);
            let (hx, hy) = (-uy, ux);
            // 头部尺寸随线宽缩放（长 3.2 倍、半宽 1.1 倍线宽）。
            // 旧实现用固定 14px 的两段粗线画"<"头：线宽一粗（如 12.7），
            // 两翼互相叠咬、平头帽留缺口，糊成带缺口的疙瘩。实心三角 +
            // 按线宽缩放在任何粗细下都是干净头部，且与画布预览几何一致。
            let w = style.width.max(1.0);
            let head = w * 3.2;
            let half = w * 1.1;
            // 基底中心
            let (bx, by) = (end[0] - ux * head, end[1] - uy * head);
            // 主线：start → 基底中心
            line(
                img,
                start[0] as i64,
                start[1] as i64,
                bx as i64,
                by as i64,
                c,
                r,
            );
            // 头部：实心三角（基底两角 → 尖端）
            fill_triangle(
                img,
                [
                    [bx + hx * half, by + hy * half],
                    [end[0], end[1]],
                    [bx - hx * half, by - hy * half],
                ],
                c,
            );
        }
        Shape::Rect { min, max } => {
            let (x0, y0) = (min[0] as i64, min[1] as i64);
            let (x1, y1) = (max[0] as i64, max[1] as i64);
            line(img, x0, y0, x1, y0, c, r);
            line(img, x1, y0, x1, y1, c, r);
            line(img, x1, y1, x0, y1, c, r);
            line(img, x0, y1, x0, y0, c, r);
        }
        Shape::Ellipse { min, max } => {
            let cx = ((min[0] + max[0]) / 2.0) as i64;
            let cy = ((min[1] + max[1]) / 2.0) as i64;
            let rx = ((max[0] - min[0]).abs() / 2.0) as i64;
            let ry = ((max[1] - min[1]).abs() / 2.0) as i64;
            midpoint_ellipse(img, cx, cy, rx, ry, c);
        }
        Shape::Mosaic { min, max, block } => {
            // 马赛克：把矩形区域按块取平均色（像素化遮挡）
            let x0 = min[0].max(0.0) as u32;
            let y0 = min[1].max(0.0) as u32;
            let x1 = max[0].min(img.width() as f32) as u32;
            let y1 = max[1].min(img.height() as f32) as u32;
            if x1 > x0 && y1 > y0 {
                let bs = (*block).max(1);
                let mut by = y0;
                while by < y1 {
                    let mut bx = x0;
                    while bx < x1 {
                        let ex = (bx + bs).min(x1);
                        let ey = (by + bs).min(y1);
                        let mut sum = [0u32; 3];
                        let mut cnt = 0u32;
                        for y in by..ey {
                            for x in bx..ex {
                                let p = img.get_pixel(x, y);
                                sum[0] += p[0] as u32;
                                sum[1] += p[1] as u32;
                                sum[2] += p[2] as u32;
                                cnt += 1;
                            }
                        }
                        let avg = [
                            (sum[0] / cnt) as u8,
                            (sum[1] / cnt) as u8,
                            (sum[2] / cnt) as u8,
                        ];
                        for y in by..ey {
                            for x in bx..ex {
                                img.put_pixel(x, y, image::Rgba([avg[0], avg[1], avg[2], 255]));
                            }
                        }
                        bx += bs;
                    }
                    by += bs;
                }
            }
        }
        Shape::Text { pos, content, size } => {
            if let Some(font) = font {
                let mut cursor = 0.0_f32;
                for ch in content.chars() {
                    let (metrics, bitmap) = font.rasterize(ch, *size);
                    let base_x = pos[0] + cursor + metrics.xmin as f32;
                    let base_y = pos[1] + metrics.ymin as f32;
                    for row in 0..metrics.height {
                        for col in 0..metrics.width {
                            let cov = bitmap[row * metrics.width + col];
                            if cov > 0 {
                                let px = base_x + col as f32;
                                let py = base_y + row as f32;
                                if px >= 0.0 && py >= 0.0 {
                                    let x = px as u32;
                                    let y = py as u32;
                                    if x < img.width() && y < img.height() {
                                        img.put_pixel(x, y, image::Rgba(c));
                                    }
                                }
                            }
                        }
                    }
                    cursor += metrics.advance_width;
                }
            }
        }
    }
}

fn midpoint_ellipse(img: &mut RgbaImage, cx: i64, cy: i64, rx: i64, ry: i64, c: [u8; 4]) {
    if rx <= 0 || ry <= 0 {
        return;
    }
    let (mut x, mut y) = (0_i64, ry);
    let mut d1 = ry * ry - rx * rx * ry + rx * rx / 4;
    let mut dx = 2 * ry * ry * x;
    let mut dy = 2 * rx * rx * y;
    while dx < dy {
        put(img, cx + x, cy + y, c);
        put(img, cx - x, cy + y, c);
        put(img, cx + x, cy - y, c);
        put(img, cx - x, cy - y, c);
        if d1 < 0 {
            x += 1;
            dx += 2 * ry * ry;
            d1 += dx + ry * ry;
        } else {
            x += 1;
            y -= 1;
            dx += 2 * ry * ry;
            dy -= 2 * rx * rx;
            d1 += dx - dy + ry * ry;
        }
    }
    let mut d2 = ry * ry * (x * x + x) + rx * rx * (y * y - y) + rx * rx / 4;
    while y >= 0 {
        put(img, cx + x, cy + y, c);
        put(img, cx - x, cy + y, c);
        put(img, cx + x, cy - y, c);
        put(img, cx - x, cy - y, c);
        if d2 > 0 {
            y -= 1;
            dy -= 2 * rx * rx;
            d2 += rx * rx - dy;
        } else {
            y -= 1;
            x += 1;
            dx += 2 * ry * ry;
            dy -= 2 * rx * rx;
            d2 += dx - dy + rx * rx;
        }
    }
}

pub fn rasterize_all(anns: &[Annotation], img: &mut RgbaImage, font: Option<&fontdue::Font>) {
    for a in anns {
        rasterize(&a.shape, &a.style, img, font);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn style(width: f32) -> Style {
        Style {
            color: [255, 0, 0, 255],
            width,
        }
    }

    #[test]
    fn line_bbox() {
        let a = Shape::Line {
            start: [10.0, 20.0],
            end: [30.0, 5.0],
        };
        let b = bbox(&a, &style(2.0)).unwrap();
        // bbox 含线宽一半的 padding（width=2 → pad=1）
        assert_eq!(b, [9.0, 4.0, 22.0, 17.0]);
    }

    #[test]
    fn brush_rasterize_sets_pixels() {
        let a = Shape::Brush(vec![[1.0, 1.0], [2.0, 1.0], [3.0, 1.0]]);
        let mut img = image::RgbaImage::new(5, 5);
        rasterize(&a, &style(1.0), &mut img, None);
        let p = img.get_pixel(1, 1);
        assert_eq!(p, &image::Rgba([255, 0, 0, 255]));
    }

    #[test]
    fn rect_rasterize_draws_border() {
        let a = Shape::Rect {
            min: [0.0, 0.0],
            max: [4.0, 4.0],
        };
        let s = Style {
            color: [0, 255, 0, 255],
            width: 1.0,
        };
        let mut img = image::RgbaImage::new(6, 6);
        rasterize(&a, &s, &mut img, None);
        assert_eq!(img.get_pixel(0, 0), &image::Rgba([0, 255, 0, 255]));
        assert_eq!(img.get_pixel(2, 2), &image::Rgba([0, 0, 0, 0]));
    }

    #[test]
    fn arrow_has_filled_triangle_head() {
        // 水平箭头 start=(2,10) end=(30,10)，width=1 → head=3.2, half=1.1
        let a = Shape::Arrow {
            start: [2.0, 10.0],
            end: [30.0, 10.0],
        };
        let mut img = image::RgbaImage::new(40, 40);
        rasterize(&a, &style(1.0), &mut img, None);
        // 尖端
        assert_eq!(img.get_pixel(30, 10), &image::Rgba([255, 0, 0, 255]));
        // 头部内部为实心（旧实现是空心 "<"，粗线下会糊成疙瘩）
        assert_eq!(img.get_pixel(28, 10), &image::Rgba([255, 0, 0, 255]));
        // 主线中段
        assert_eq!(img.get_pixel(10, 10), &image::Rgba([255, 0, 0, 255]));
        // 翅膀上方外侧应为空
        assert_eq!(img.get_pixel(27, 5), &image::Rgba([0, 0, 0, 0]));
        // 超出尖端的延长线应为空（主线只到基底）
        assert_eq!(img.get_pixel(33, 10), &image::Rgba([0, 0, 0, 0]));
    }

    #[test]
    fn arrow_head_scales_with_stroke_width() {
        // 粗线回归（用户报告的 bug）：width=12.7 时头部必须按线宽放大成
        // 干净实心三角，而不是固定 14px 两翼叠咬出的糊疙瘩
        let a = Shape::Arrow {
            start: [2.0, 10.0],
            end: [100.0, 10.0],
        };
        let mut img = image::RgbaImage::new(140, 60);
        rasterize(&a, &style(12.7), &mut img, None);
        // head = 12.7*3.2 ≈ 40.6 → 基底中心 x≈59.4；half = 12.7*1.1 ≈ 14
        // 头部内部实心
        assert_eq!(img.get_pixel(80, 10), &image::Rgba([255, 0, 0, 255]));
        assert_eq!(img.get_pixel(70, 15), &image::Rgba([255, 0, 0, 255]));
        // 基底外侧（翅膀之外）应为空
        assert_eq!(img.get_pixel(50, 25), &image::Rgba([0, 0, 0, 0]));
        // 超出尖端的延长线应为空
        assert_eq!(img.get_pixel(110, 10), &image::Rgba([0, 0, 0, 0]));
        // 主线中段上下方应为空（主线比头部区域窄）
        assert_eq!(img.get_pixel(30, 2), &image::Rgba([0, 0, 0, 0]));
    }

    #[test]
    fn mosaic_averages_blocks() {
        let a = Shape::Mosaic {
            min: [0.0, 0.0],
            max: [24.0, 12.0],
            block: 12,
        };
        let s = Style {
            color: [0, 0, 0, 255],
            width: 1.0,
        };
        let mut img = image::RgbaImage::new(24, 12);
        for y in 0..12 {
            for x in 0..24 {
                if x < 6 {
                    img.put_pixel(x, y, image::Rgba([255, 0, 0, 255]));
                } else {
                    img.put_pixel(x, y, image::Rgba([0, 0, 255, 255]));
                }
            }
        }
        rasterize(&a, &s, &mut img, None);
        // 块 (0..12)：6 红 + 6 蓝 → 平均 (127,0,127)
        assert_eq!(img.get_pixel(3, 3), &image::Rgba([127, 0, 127, 255]));
        // 块 (12..24)：全蓝 → 平均 (0,0,255)
        assert_eq!(img.get_pixel(15, 3), &image::Rgba([0, 0, 255, 255]));
    }
}
