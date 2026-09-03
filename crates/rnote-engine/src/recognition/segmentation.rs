use super::RecognitionStroke;
use p2d::bounding_volume::{Aabb, BoundingVolume};

/// Configuration parameters for handwriting line segmentation.
/// All distance thresholds are evaluated relative to the median stroke height.
#[derive(Debug)]
pub struct SegmentConfig {
    /// Minimum size (relative to median line height) for a stroke to be considered
    /// an "anchor" (core text) rather than an "outlier" (like an 'i' dot or comma).
    pub anchor_size_ratio: f64,

    /// The base vertical tolerance for assigning a stroke to an existing line.
    /// 0.8 means a stroke can deviate up to 80% of the median height from the line's center.
    pub base_vertical_ratio: f64,

    /// How close horizontally (in median heights) a stroke must be to its neighbor
    /// to use the more forgiving "local" vertical distance instead of the line's
    /// global average. This helps group slanted handwriting.
    pub local_neighbor_ratio: f64,

    /// A tiny weight added to horizontal distance to act as a tie-breaker when a stroke
    /// is vertically equidistant between two candidate lines.
    pub h_dist_tiebreaker: f64,

    /// The maximum horizontal gap (in median heights) allowed between two line fragments
    /// before we refuse to merge them. (e.g., 1.5 allows for wide word spaces).
    pub merge_h_gap_ratio: f64,

    /// The minimum percentage (0.0 to 1.0) of a smaller bounding box that must vertically
    /// overlap with a larger bounding box to trigger a line merge.
    pub merge_overlap_factor: f64,

    /// Fallback multiplier if the bounding boxes don't overlap by the required factor,
    /// but they are vertically aligned very closely (e.g., 1.5x the dynamic threshold).
    pub merge_v_dist_ratio: f64,

    /// How severely to penalize horizontal distance when attaching outliers.
    /// A high penalty forces dots and commas to snap to the line directly beneath/above them.
    pub outlier_h_penalty: f64,

    /// How far an outlier is allowed to be from a line's center before it is rejected
    /// and forced to form its own new line.
    pub outlier_vertical_ratio: f64,

    /// A fallback threshold multiplier for outliers used against the hard `vertical_threshold`
    /// passed into the segmentation function.
    pub outlier_vertical_fallback: f64,

    /// Looseness multiplier for the final acceptance of an outlier into an existing line.
    pub outlier_acceptance_mult: f64,

    /// The maximum horizontal space (in median heights) allowed between strokes
    /// on the same line before it splits into a separate sentence/block.
    /// (e.g., 4.0 usually represents a large gap between columns or paragraphs).
    pub split_h_gap_ratio: f64,
}

impl Default for SegmentConfig {
    fn default() -> Self {
        Self {
            anchor_size_ratio: 0.25,
            base_vertical_ratio: 0.8,
            local_neighbor_ratio: 3.0,
            h_dist_tiebreaker: 0.05,
            merge_h_gap_ratio: 5.0,
            merge_overlap_factor: 0.80,
            merge_v_dist_ratio: 1.5,
            outlier_h_penalty: 3.0,
            outlier_vertical_ratio: 1.5,
            outlier_vertical_fallback: 2.0,
            outlier_acceptance_mult: 3.0,
            split_h_gap_ratio: 5.5,
        }
    }
}

fn h_gap(a_min: f64, a_max: f64, b_min: f64, b_max: f64) -> f64 {
    (a_min - b_max).max(b_min - a_max).max(0.0)
}

fn v_overlap(a_min: f64, a_max: f64, b_min: f64, b_max: f64) -> f64 {
    (a_max.min(b_max) - a_min.max(b_min)).max(0.0)
}

#[derive(Clone)]
struct StrokeMeta {
    original_idx: usize,
    bounds: Aabb,
    median_y: f64,
}

struct LineTracker {
    strokes_meta: Vec<StrokeMeta>,
    sum_y: f64,
    weight: f64,
    min_x: f64,
    max_x: f64,
    min_y: f64,
    max_y: f64,
}

impl LineTracker {
    fn new(meta: StrokeMeta, weight: f64) -> Self {
        let mut lt = Self {
            strokes_meta: Vec::new(),
            sum_y: 0.0,
            weight: 0.0,
            min_x: f64::MAX,
            max_x: f64::MIN,
            min_y: f64::MAX,
            max_y: f64::MIN,
        };
        lt.add(meta, weight);
        lt
    }

    fn avg_y(&self) -> f64 {
        if self.weight == 0.0 {
            0.0
        } else {
            self.sum_y / self.weight
        }
    }

    fn add(&mut self, meta: StrokeMeta, stroke_weight: f64) {
        self.sum_y += meta.median_y * stroke_weight;
        self.weight += stroke_weight;
        self.min_x = self.min_x.min(meta.bounds.mins.x);
        self.max_x = self.max_x.max(meta.bounds.maxs.x);
        self.min_y = self.min_y.min(meta.bounds.mins.y);
        self.max_y = self.max_y.max(meta.bounds.maxs.y);
        self.strokes_meta.push(meta);
    }

    fn score_anchor(
        &self,
        anchor: &StrokeMeta,
        cfg: &SegmentConfig,
        median_height: f64,
    ) -> Option<f64> {
        let mut min_dist_h = f64::MAX;
        let mut local_v_dist = f64::MAX;

        for s in &self.strokes_meta {
            let gap = h_gap(
                s.bounds.mins.x,
                s.bounds.maxs.x,
                anchor.bounds.mins.x,
                anchor.bounds.maxs.x,
            );
            if gap < min_dist_h {
                min_dist_h = gap;
                local_v_dist = (anchor.median_y - s.median_y).abs();
            }
        }

        let global_v_dist = (anchor.median_y - self.avg_y()).abs();
        let effective_v = if min_dist_h < (median_height * cfg.local_neighbor_ratio) {
            local_v_dist.min(global_v_dist)
        } else {
            global_v_dist
        };

        Some(effective_v + (min_dist_h * cfg.h_dist_tiebreaker))
    }

    fn score_outlier(&self, outlier: &StrokeMeta, cfg: &SegmentConfig) -> f64 {
        let v_dist = (outlier.median_y - self.avg_y()).abs();
        let min_h_dist = self
            .strokes_meta
            .iter()
            .map(|s| {
                h_gap(
                    s.bounds.mins.x,
                    s.bounds.maxs.x,
                    outlier.bounds.mins.x,
                    outlier.bounds.maxs.x,
                )
            })
            .fold(f64::MAX, f64::min);

        v_dist + (min_h_dist * cfg.outlier_h_penalty)
    }

    fn should_merge(
        &self,
        other: &Self,
        cfg: &SegmentConfig,
        median_h: f64,
        dynamic_threshold: f64,
    ) -> bool {
        let v_dist = (self.avg_y() - other.avg_y()).abs();
        let gap_h = h_gap(self.min_x, self.max_x, other.min_x, other.max_x);

        let overlap_y = v_overlap(self.min_y, self.max_y, other.min_y, other.max_y);
        let min_height = (self.max_y - self.min_y).min(other.max_y - other.min_y);
        let overlap_factor = if min_height > 0.0 {
            overlap_y / min_height
        } else {
            0.0
        };

        gap_h < (median_h * cfg.merge_h_gap_ratio)
            && (overlap_factor >= cfg.merge_overlap_factor
                || v_dist < dynamic_threshold * cfg.merge_v_dist_ratio)
    }

    fn split_into_blocks(
        mut self,
        max_h_gap: f64,
        strokes: &[RecognitionStroke],
    ) -> Vec<Vec<RecognitionStroke>> {
        self.strokes_meta
            .sort_unstable_by(|a, b| a.bounds.mins.x.total_cmp(&b.bounds.mins.x));

        let mut blocks = Vec::new();
        let mut current_block = Vec::new();
        let mut current_max_x = self
            .strokes_meta
            .first()
            .map(|m| m.bounds.maxs.x)
            .unwrap_or(0.0);

        for meta in self.strokes_meta {
            if !current_block.is_empty() && (meta.bounds.mins.x - current_max_x) > max_h_gap {
                current_block.sort_unstable_by_key(|m: &StrokeMeta| m.original_idx);
                blocks.push(
                    current_block
                        .drain(..)
                        .map(|m| strokes[m.original_idx].clone())
                        .collect(),
                );
            }
            current_max_x = current_max_x.max(meta.bounds.maxs.x);
            current_block.push(meta);
        }

        if !current_block.is_empty() {
            current_block.sort_unstable_by_key(|m| m.original_idx);
            blocks.push(
                current_block
                    .into_iter()
                    .map(|m| strokes[m.original_idx].clone())
                    .collect(),
            );
        }

        blocks
    }
}

pub fn segment_into_lines(
    strokes: &[RecognitionStroke],
    vertical_threshold: f64,
) -> Vec<Vec<RecognitionStroke>> {
    if strokes.is_empty() {
        return vec![];
    }
    let cfg = SegmentConfig::default();

    // Compute Metadata & Median Heights
    let mut meta_list: Vec<StrokeMeta> = Vec::with_capacity(strokes.len());
    let mut heights: Vec<f64> = Vec::with_capacity(strokes.len());
    let mut y_coords_buf: Vec<f64> = Vec::new();

    for (i, stroke) in strokes.iter().enumerate() {
        if stroke.points.is_empty() {
            continue;
        }

        y_coords_buf.clear();
        y_coords_buf.extend(stroke.points.iter().map(|pt| pt.pos.y as f64));
        let mid = y_coords_buf.len() / 2;
        let (_, &mut median_y, _) = y_coords_buf.select_nth_unstable_by(mid, |a, b| a.total_cmp(b));

        let bounds = stroke.bounds();
        heights.push(bounds.maxs.y - bounds.mins.y);
        meta_list.push(StrokeMeta {
            original_idx: i,
            bounds,
            median_y,
        });
    }

    if meta_list.is_empty() {
        return vec![];
    }

    let mid_height_idx = heights.len() / 2;
    let (_, &mut median_height, _) =
        heights.select_nth_unstable_by(mid_height_idx, |a, b| a.total_cmp(b));

    let dynamic_threshold = (median_height * cfg.base_vertical_ratio).max(vertical_threshold);
    let outlier_threshold = (median_height * cfg.outlier_vertical_ratio)
        .max(vertical_threshold * cfg.outlier_vertical_fallback);

    // Separate Anchors from Outliers
    let mut anchors = Vec::with_capacity(meta_list.len());
    let mut outliers = Vec::new();

    for meta in meta_list {
        let h = meta.bounds.maxs.y - meta.bounds.mins.y;
        let w = meta.bounds.maxs.x - meta.bounds.mins.x;
        if h > median_height * cfg.anchor_size_ratio || w > median_height * cfg.anchor_size_ratio {
            anchors.push(meta);
        } else {
            outliers.push(meta);
        }
    }
    if anchors.is_empty() {
        std::mem::swap(&mut anchors, &mut outliers);
    }

    // Group Anchors into Core Lines
    anchors.sort_unstable_by(|a, b| a.median_y.total_cmp(&b.median_y));
    let mut lines: Vec<LineTracker> = Vec::new();

    for anchor in anchors {
        let anchor_w = strokes[anchor.original_idx].points.len() as f64;
        let best_line = lines
            .iter_mut()
            .filter_map(|line| {
                line.score_anchor(&anchor, &cfg, median_height)
                    .map(|s| (line, s))
            })
            .filter(|(_, score)| *score < dynamic_threshold)
            .min_by(|(_, s1), (_, s2)| s1.total_cmp(s2));

        if let Some((line, _)) = best_line {
            line.add(anchor, anchor_w);
        } else {
            lines.push(LineTracker::new(anchor, anchor_w));
        }
    }

    // Merge Fragmented Lines
    let mut opt_lines: Vec<Option<LineTracker>> = lines.into_iter().map(Some).collect();
    'merge_loop: loop {
        for i in 0..opt_lines.len() {
            for j in (i + 1)..opt_lines.len() {
                if let (Some(li), Some(lj)) = (&opt_lines[i], &opt_lines[j]) {
                    if li.should_merge(lj, &cfg, median_height, dynamic_threshold) {
                        let line_j = opt_lines[j].take().unwrap();
                        let line_i = opt_lines[i].as_mut().unwrap();
                        for meta in line_j.strokes_meta {
                            let w = strokes[meta.original_idx].points.len() as f64;
                            line_i.add(meta, w);
                        }
                        continue 'merge_loop; // Restart checking from top
                    }
                }
            }
        }
        break; // No more merges found
    }
    let mut lines: Vec<LineTracker> = opt_lines.into_iter().flatten().collect();

    // Snap Outliers
    for outlier in outliers {
        let outlier_w = strokes[outlier.original_idx].points.len() as f64;
        let best_line = lines
            .iter_mut()
            .map(|line| (line.score_outlier(&outlier, &cfg), line))
            .min_by(|(s1, _), (s2, _)| s1.total_cmp(s2));

        match best_line {
            Some((score, line)) if score < outlier_threshold * cfg.outlier_acceptance_mult => {
                line.add(outlier, outlier_w);
            }
            _ => lines.push(LineTracker::new(outlier, outlier_w)),
        }
    }

    // Split Horizontally & Return
    lines.sort_unstable_by(|a, b| a.avg_y().total_cmp(&b.avg_y()));

    lines
        .into_iter()
        .filter(|line| !line.strokes_meta.is_empty())
        .flat_map(|line| line.split_into_blocks(median_height * cfg.split_h_gap_ratio, strokes))
        .collect()
}
