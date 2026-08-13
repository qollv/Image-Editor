use std::path::Path;

use crate::draw::{self, Annotation, Style};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CropRect {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

pub struct EditorDoc {
    pub image: image::DynamicImage,
    pub annotations: Vec<Annotation>,
    pub style: Style,
    pub crop: Option<CropRect>,
    undo_stack: Vec<Vec<Annotation>>,
    redo_stack: Vec<Vec<Annotation>>,
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
        self.undo_stack.push(self.annotations.clone());
        self.redo_stack.clear();
        self.annotations.push(a);
    }

    pub fn undo(&mut self) {
        if let Some(prev) = self.undo_stack.pop() {
            self.redo_stack.push(std::mem::take(&mut self.annotations));
            self.annotations = prev;
        }
    }

    pub fn redo(&mut self) {
        if let Some(next) = self.redo_stack.pop() {
            self.undo_stack.push(std::mem::take(&mut self.annotations));
            self.annotations = next;
        }
    }

    pub fn remove_last(&mut self) {
        if !self.annotations.is_empty() {
            self.undo_stack.push(self.annotations.clone());
            self.redo_stack.clear();
            self.annotations.pop();
        }
    }

    pub fn apply_crop(&mut self) {
        if let Some(c) = self.crop.take() {
            let cropped = self.image.crop_imm(c.x, c.y, c.w, c.h);
            self.image = cropped;
            self.annotations.clear();
            self.undo_stack.clear();
            self.redo_stack.clear();
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
    fn render_composites_annotation() {
        let mut d = doc();
        d.style.color = [255, 0, 0, 255];
        d.style.width = 1.0;
        d.push_annotation(Annotation::Brush(vec![[1.0, 1.0], [2.0, 1.0]]));
        let out = d.render(None);
        assert_eq!(out.get_pixel(1, 1), &image::Rgba([255, 0, 0, 255]));
    }
}
