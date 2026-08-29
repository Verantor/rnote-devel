use std::cmp::min;

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

fn fuzzy_contains(text: &str, query: &str, max_distance: usize) -> bool {
    let text_chars: Vec<char> = text.chars().collect();
    let query_chars: Vec<char> = query.chars().collect();

    let text_len = text_chars.len();
    let query_len = query_chars.len();

    if query_len == 0 {
        return true;
    }
    if text_len == 0 {
        return query_len <= max_distance;
    }

    // arrays track the edit distance/cost for the current and previous query characters
    let mut prev_row = vec![0; text_len + 1];
    let mut curr_row = vec![0; text_len + 1];

    for query_idx in 1..=query_len {
        // matching a query against an empty text prefix requires deletions
        curr_row[0] = query_idx;

        for text_idx in 1..=text_len {
            let substitution_cost = if query_chars[query_idx - 1] == text_chars[text_idx - 1] {
                0
            } else {
                1
            };

            curr_row[text_idx] = min(
                min(
                    prev_row[text_idx] + 1,     // Deletion from query
                    curr_row[text_idx - 1] + 1, // Insertion into query
                ),
                prev_row[text_idx - 1] + substitution_cost, // Substitution
            );
        }
        prev_row.copy_from_slice(&curr_row);
    }

    // if any ending position in the text has an edit distance <= max_distance -> match
    prev_row.into_iter().any(|dist| dist <= max_distance)
}
impl StrokeStore {
    pub fn search(&self, query: &str) -> Vec<SearchResult> {
        let query_lower = query.to_lowercase();
        let mut results = Vec::new();

        if query.trim().is_empty() {
            return results;
        }

        // allows for 1 error every 4 chars
        let max_errors = if query_lower.len() <= 3 {
            0
        } else {
            // querrys under 4 need to be exact to prevend flood of false prositives
            query_lower.len() / 4
        };

        // search for textstrokes
        for (key, stroke) in self.stroke_components.iter() {
            if self.trashed(key).unwrap_or(false) {
                continue;
            }

            if let Stroke::TextStroke(text_stroke) = &**stroke {
                // consider if TextStrokes need fuzzy matching but its still easier because the user may not remember exactly what they are searching for?????
                if fuzzy_contains(&text_stroke.text.to_lowercase(), &query_lower, max_errors) {
                    results.push(SearchResult {
                        text: text_stroke.text.clone(),
                        bounds: stroke.bounds(),
                        stroke_keys: vec![key.clone()],
                    });
                }
            }
        }

        // search for handwriting
        for line in &self.recognized_text {
            if fuzzy_contains(&line.text.to_lowercase(), &query_lower, max_errors) {
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
