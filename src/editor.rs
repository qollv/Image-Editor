use std::path::Path;

use crate::draw::{self, Annotation, Style};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CropRect {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

/// 撤销栈条目：标注操作用轻量快照，裁剪用完整原图快照（避免每笔都深拷贝整图）。
enum UndoEntry {
    Annotations(Vec<Annotation>),
    Crop {
        image: image::DynamicImage,
        annotations: Vec<Annotation>,
    },
}

pub struct EditorDoc {
    pub image: image::DynamicImage,
    pub annotations: Vec<Annotation>,
    pub style: Style,
    pub crop: Option<CropRect>,
    undo_stack: Vec<UndoEntry>,
    redo_stack: Vec<UndoEntry>,
}

impl EditorDoc {
    pub fn new(image: image::DynamicImage) -> Self {
        Self {
            image,
            annotations: Vec::new(),
            style: Style {
                color: [255, 0, 0, 255],
                width: 2.0,
            },
            crop: None,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
        }
    }

    pub fn open(path: &Path) -> Result<Self, image::ImageError> {
        Ok(Self::new(image::open(path)?))
    }

    pub fn push_annotation(&mut self, a: Annotation) {
        self.undo_stack
            .push(UndoEntry::Annotations(self.annotations.clone()));
        self.redo_stack.clear();
        self.annotations.push(a);
    }

    pub fn undo(&mut self) {
        if let Some(entry) = self.undo_stack.pop() {
            match entry {
                UndoEntry::Annotations(prev) => {
                    self.redo_stack
                        .push(UndoEntry::Annotations(std::mem::take(&mut self.annotations)));
                    self.annotations = prev;
                }
                UndoEntry::Crop { image, annotations } => {
                    self.redo_stack.push(UndoEntry::Crop {
                        image: self.image.clone(),
                        annotations: std::mem::take(&mut self.annotations),
                    });
                    self.image = image;
                    self.annotations = annotations;
                }
            }
        }
    }

    #[allow(dead_code)] // 模型保留撤销/重做配对能力，UI 目前只暴露撤销
    pub fn redo(&mut self) {
        if let Some(entry) = self.redo_stack.pop() {
            match entry {
                UndoEntry::Annotations(next) => {
                    self.undo_stack
                        .push(UndoEntry::Annotations(std::mem::take(&mut self.annotations)));
                    self.annotations = next;
                }
                UndoEntry::Crop { image, annotations } => {
                    self.undo_stack.push(UndoEntry::Crop {
                        image: self.image.clone(),
                        annotations: std::mem::take(&mut self.annotations),
                    });
                    self.image = image;
                    self.annotations = annotations;
                }
            }
        }
    }

    pub fn apply_crop(&mut self) {
        if let Some(c) = self.crop.take() {
            let prev_image = self.image.clone();
            let prev_annotations = self.annotations.clone();
            self.undo_stack.push(UndoEntry::Crop {
                image: prev_image,
                annotations: prev_annotations,
            });
            self.redo_stack.clear();
            let cropped = self.image.crop_imm(c.x, c.y, c.w, c.h);
            self.image = cropped;
            self.annotations.clear();
        }
    }

    pub fn render(&self, font: Option<&fontdue::Font>) -> image::RgbaImage {
        let mut out = self.image.to_rgba8();
        draw::rasterize_all(&self.annotations, &self.style, &mut out, font);
        out
    }
}

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
        d.push_annotation(Annotation::Line {
            start: [0.0, 0.0],
            end: [5.0, 5.0],
        });
        assert_eq!(d.annotations.len(), 1);
        d.undo();
        assert_eq!(d.annotations.len(), 0);
        d.redo();
        assert_eq!(d.annotations.len(), 1);
    }

    #[test]
    fn apply_crop_shrinks_image() {
        let mut d = doc();
        d.crop = Some(CropRect {
            x: 2,
            y: 2,
            w: 4,
            h: 4,
        });
        d.apply_crop();
        assert_eq!(d.image.width(), 4);
        assert_eq!(d.image.height(), 4);
        assert!(d.crop.is_none());
    }

    #[test]
    fn crop_can_be_undone() {
        let mut d = doc();
        // 先加一个标注，验证裁剪撤销时标注也一并恢复
        d.push_annotation(Annotation::Line {
            start: [0.0, 0.0],
            end: [5.0, 5.0],
        });
        d.crop = Some(CropRect {
            x: 2,
            y: 2,
            w: 4,
            h: 4,
        });
        d.apply_crop();
        assert_eq!(d.image.width(), 4);
        assert_eq!(d.annotations.len(), 0);
        // 撤销裁剪：图像和标注都应恢复
        d.undo();
        assert_eq!(d.image.width(), 10);
        assert_eq!(d.image.height(), 10);
        assert_eq!(d.annotations.len(), 1);
        // 重做：再次裁剪
        d.redo();
        assert_eq!(d.image.width(), 4);
        assert_eq!(d.annotations.len(), 0);
    }

    #[test]
    fn render_composites_annotation() {
        let mut d = doc();
        d.style.color = [255, 0, 0, 255];
        d.style.width = 1.0;
        d.push_annotation(Annotation::Brush(vec![[1.0, 1.0], [2.0, 1.0]]));
        let out = d.render(None);
        assert_eq!(out.get_pixel(1, 1), &image::Rgba([255, 0, 0, 255]));
    }
}
