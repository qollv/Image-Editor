# 图片格式转换 + 轻量编辑器 — 设计文档

日期：2026-08-13
状态：已批准（待用户审阅）

## 1. 目标

一个 Windows 桌面 GUI 应用，提供两大能力：

1. **格式转换**：单个或批量把图片从一种格式转成另一种。
2. **轻量编辑**：打开单张图片，进行裁剪和绘制标注，再保存。

成品形态：单文件可执行程序（`.exe`），双击即用，无需安装任何运行时。

## 2. 技术选型

- 语言/界面：Rust + `egui`（经 `eframe` 启动原生窗口）
- 图像处理：`image` crate（开启全部格式 feature），纯 Rust 实现、零系统依赖
- 文件对话框：`rfd`
- 并行转换：`rayon`
- 文字栅格化：`fontdue`

## 3. 架构

三层结构，核心原则：**转换与编辑逻辑完全与 UI 解耦**，各自可独立单元测试。

```
main.rs                  应用入口，启动 eframe 窗口
  └─ app.rs              根组件 + 顶部 Tab 切换（转换页 / 编辑页）
      ├─ convert_view.rs 转换页（UI）
      └─ editor_view.rs  编辑页（UI）
  convert.rs             转换核心（纯逻辑）
  options.rs             转换参数定义
  editor.rs              编辑器文档模型（纯逻辑）
  draw.rs                标注几何与栅格化（纯逻辑）
```

## 4. 转换页

- 文件列表：支持多选添加、添加整个文件夹。
- 目标格式下拉框 + 每格式参数：
  - JPEG：质量 1–100（默认 90）
  - PNG：压缩级别
  - WebP：质量 + 无损开关
- 输出目录选择。
- 命名冲突策略：同名改扩展名；目标已存在时默认「跳过」，可切换「覆盖」。
- 透明通道处理：转 JPEG/BMP 等不支持透明的格式时，自动合成白色背景，可配置。
- 并行处理：`rayon` 线程池，进度经通道回传，UI 逐帧刷新。
- 错误处理：单文件失败不中断批量，失败原因写入该文件的结果行。

### 支持格式

PNG、JPEG、WebP、GIF、BMP、ICO、TIFF、TGA、AVIF、PNM、QOI 等（读写能力取决于 `image` 各格式 encoder/decoder 支持）。

### 核心接口

```rust
convert_file(input: &Path, output: &Path, opts: &ConvertOptions) -> Result<(), ConvertError>
```

## 5. 编辑页

顶部工具栏 + 画布 + 底部操作区。

### 5.1 工具

- 选择/移动
- 绘制：自由画笔、直线、箭头、矩形、椭圆、文字标注
- 裁剪：自由选区裁剪、固定比例裁剪（1:1 / 4:3 / 16:9 / 自由）
- 撤销 / 重做、删除选中、清空全部

### 5.2 画布

- 滚轮缩放、拖拽平移
- 实时预览（egui 直接画矢量，所见即所得）

### 5.3 保存

点「保存」弹对话框，用户二选一：

1. **另存为新文件**：选格式（PNG/JPEG/WebP 等）+ 质量参数 + 路径
2. **覆盖原图**：原格式、原路径写回

### 5.4 编辑器核心模型

```rust
struct EditorDoc {
    image: DynamicImage,        // 原始图像
    annotations: Vec<Annotation>, // 标注列表
    crop: Option<CropRect>,     // 裁剪框
    undo_stack: Vec<EditAction>,  // 撤销栈
    redo_stack: Vec<EditAction>,  // 重做栈
}
```

- 每个 `Annotation` 记录类型、颜色、线宽、坐标/文字内容。
- 预览用 egui 画矢量；导出时用同一份指令栅格化合成到像素，保证预览与最终结果一致。
- 裁剪作为独立操作，执行后改变画布范围。

## 6. 依赖清单

`eframe`、`image`（全 feature）、`rfd`、`rayon`、`fontdue` —— 全部零系统依赖。

文字标注的中文支持：Windows 从系统加载微软雅黑（`C:\Windows\Fonts\msyh.ttc`），找不到则回退内置字体。

## 7. 测试策略

- `convert.rs`：内存生成测试图，转换到各目标格式再读回验证。覆盖：PNG→JPEG、带 alpha 的 PNG→JPEG（白底合成）、WebP 质量参数、GIF。
- `draw.rs` / `editor.rs`：标注包围盒、裁剪矩形计算、箭头几何、撤销/重做。
- 合成导出：内存生成测试图 + 叠加标注 → 导出 PNG → 读回验证尺寸与像素。

## 8. 范围边界（v1）

- 编辑页一次只编辑一张图片；打开新图前提示保存当前。
- 不做拖拽打开（v1 用原生文件对话框）。
- 不做批量编辑，只做批量转换。
