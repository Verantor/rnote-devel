use crate::store::StrokeKey;
use p2d::bounding_volume::Aabb;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextLine {
    pub text: String,
    pub stroke_keys: Vec<StrokeKey>,
    pub bounds: Aabb,
}

impl TextLine {
    pub fn new(text: String, stroke_keys: Vec<StrokeKey>, bounds: Aabb) -> Self {
        Self {
            text,
            stroke_keys,
            bounds,
        }
    }

    pub fn contains_stroke(&self, key: &StrokeKey) -> bool {
        self.stroke_keys.contains(key)
    }
}
