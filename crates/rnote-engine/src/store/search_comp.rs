use crate::store::{StrokeKey, StrokeStore};
use crate::strokes::Stroke;
use p2d::bounding_volume::Aabb;
use rnote_compose::shapes::Shapeable;
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub text: String,
    pub bounds: Aabb,
    pub stroke_keys: Vec<StrokeKey>,
}

impl StrokeStore {
    pub fn search(&self, query: &str) -> Vec<SearchResult> {
        let query_lower = query.to_lowercase();
        let mut results = Vec::new();

        if query.trim().is_empty() {
            return results;
        }

        // search for typed text 
        for (key, stroke) in self.stroke_components.iter() {
            if self.trashed(key).unwrap_or(false) {
                continue;
            }

            if let Stroke::TextStroke(text_stroke) = &**stroke {
                if text_stroke.text.to_lowercase().contains(&query_lower) {
                    results.push(SearchResult {
                        text: text_stroke.text.clone(),
                        bounds: stroke.bounds(),
                        stroke_keys: vec![key],
                    });
                }
            }
        }

        // search for handwriting
        for line in &self.recognized_text {
            if line.text.to_lowercase().contains(&query_lower) {
                results.push(SearchResult {
                    text: line.text.clone(),
                    bounds: line.bounds, 
                    stroke_keys: line.stroke_keys.clone(),
                });
            }
        }

        results
    }
}
