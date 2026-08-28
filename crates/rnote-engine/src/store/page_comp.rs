use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Page {
    pub strokes: HashMap<StrokeKey, Stroke>,

    pub recognized_text: Vec<TextLine>,
}

impl Page {
    pub fn add_recognized_line(&mut self, text: String, keys: Vec<StrokeKey>) {
        let mut combined_bounds: Option<Aabb> = None;

        for key in &keys {
            if let Some(stroke) = self.strokes.get(key) {
                let bounds = stroke.bounds();
                combined_bounds = Some(match combined_bounds {
                    Some(existing) => existing.merged(&bounds),
                    None => bounds,
                });
            }
        }

        if let Some(bounds) = combined_bounds {
            let line = TextLine::new(text, keys, bounds);
            self.recognized_text.push(line);
        }
    }
    pub fn search_text(&self, query: &str) -> Vec<Aabb> {
        let query_lower = query.to_lowercase();
        let mut match_bounds = Vec::new();

        for line in &self.recognized_text {
            if line.text.to_lowercase().contains(&query_lower) {
                match_bounds.push(line.bounds);
            }
        }

        match_bounds
    }
    pub fn get_text_at_point(&self, point: Vector2) -> Option<&TextLine> {
        self.recognized_text
            .iter()
            .find(|line| line.bounds.contains(point))
    }
}
