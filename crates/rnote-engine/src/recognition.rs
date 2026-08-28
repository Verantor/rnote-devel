extern crate alloc;
use crate::store::text_comp::TextLine;

use crate::engine::{EngineTask, EngineTaskSender};
use crate::store::StrokeKey;
// use crate::strokes::Stroke;
use crate::tasks::{OneOffTaskError, OneOffTaskHandle};
use p2d::bounding_volume::{Aabb, BoundingVolume};
use p2d::math::Vector2;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tracing::{error, info};
// Import ORT Value/Tensor types
use ort::{
    session::{Session, builder::GraphOptimizationLevel},
    value::Tensor,
};

pub fn load_model_session(path: &str) -> Result<Session, Box<dyn std::error::Error>> {
    let session = Session::builder()?
        .with_optimization_level(GraphOptimizationLevel::Level3)?
        .with_intra_threads(4)?
        .commit_from_file(path)?;

    Ok(session)
}

#[derive(Debug, Clone)]
pub struct RecognitionPoint {
    pub pos: Vector2,
}

#[derive(Debug, Clone)]
pub struct RecognitionStroke {
    pub id: StrokeKey,
    pub points: Vec<RecognitionPoint>,
}

impl RecognitionStroke {
    pub fn bounds(&self) -> Aabb {
        let mut aabb = Aabb::new_invalid();
        for pt in &self.points {
            aabb.take_point(pt.pos);
        }
        aabb
    }
}

pub fn segment_into_lines(
    strokes: &[RecognitionStroke],
    vertical_threshold: f64,
) -> Vec<Vec<RecognitionStroke>> {
    if strokes.is_empty() {
        return vec![];
    }

    let mut strokes_with_bounds: Vec<(&RecognitionStroke, f64, f64, f64)> = strokes
        .iter()
        .map(|stroke| {
            let bounds = stroke.bounds();
            let min_y = bounds.mins.y;
            let max_y = bounds.maxs.y;
            let height = max_y - min_y;
            (stroke, min_y, max_y, height)
        })
        .collect();

    strokes_with_bounds.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));

    // Bit useless because big letters like h and g push the bounding box so no threshold needed (or negative but smart)
    let avg_height: f64 =
        strokes_with_bounds.iter().map(|s| s.3).sum::<f64>() / strokes_with_bounds.len() as f64;
    let dynamic_threshold = (avg_height * 0.4).max(vertical_threshold);

    let mut lines = Vec::new();
    let mut current_line = vec![strokes_with_bounds[0].0.clone()];
    let mut current_line_bottom = strokes_with_bounds[0].2;

    for (stroke, min_y, max_y, _) in strokes_with_bounds.into_iter().skip(1) {
        if min_y > current_line_bottom + dynamic_threshold {
            lines.push(current_line);
            current_line = vec![stroke.clone()];
            current_line_bottom = max_y;
        } else {
            current_line.push(stroke.clone());
            current_line_bottom = current_line_bottom.max(max_y);
        }
    }
    lines.push(current_line);

    // this destroys time order
    // for line in &mut lines {
    //     line.sort_by(|a, b| {
    //         let min_x_a = a.bounds().mins.x;
    //         let min_x_b = b.bounds().mins.x;
    //         min_x_a
    //             .partial_cmp(&min_x_b)
    //             .unwrap_or(std::cmp::Ordering::Equal)
    //     });
    // }

    lines
}

pub fn get_strokes_bounds(strokes: &[RecognitionStroke]) -> Option<Aabb> {
    if strokes.is_empty() {
        return None;
    }
    Some(
        strokes
            .iter()
            .fold(Aabb::new_invalid(), |acc, s| acc.merged(&s.bounds())),
    )
}

fn get_global_center(strokes: &[RecognitionStroke]) -> Vector2 {
    let mut sum_x = 0.0_f64;
    let mut sum_y = 0.0_f64;
    let mut count = 0.0_f64;

    for stroke in strokes {
        for pt in &stroke.points {
            sum_x += pt.pos.x as f64;
            sum_y += pt.pos.y as f64;
            count += 1.0;
        }
    }
    if count == 0.0 {
        return Vector2 { x: 0.0, y: 0.0 };
    }
    Vector2 {
        x: sum_x / count,
        y: sum_y / count,
    }
}

fn rotate_point(pt: &Vector2, origin: &Vector2, angle_rad: f64) -> Vector2 {
    let cos_a = angle_rad.cos();
    let sin_a = angle_rad.sin();
    let dx = pt.x as f64 - origin.x as f64;
    let dy = pt.y as f64 - origin.y as f64;

    Vector2 {
        x: origin.x as f64 + (dx * cos_a - dy * sin_a),
        y: origin.y as f64 + (dx * sin_a + dy * cos_a),
    }
}

fn get_stroke_centroid(stroke: &RecognitionStroke) -> Option<Vector2> {
    if stroke.points.is_empty() {
        return None;
    }
    let mut sum_x = 0.0_f64;
    let mut sum_y = 0.0_f64;
    let count = stroke.points.len() as f64;

    for pt in &stroke.points {
        sum_x += pt.pos.x as f64;
        sum_y += pt.pos.y as f64;
    }
    Some(Vector2 {
        x: sum_x / count,
        y: sum_y / count,
    })
}

pub fn calculate_skew_angle(strokes: &[RecognitionStroke]) -> f64 {
    let mut centroids = Vec::new();
    let mut sum_x = 0.0_f64;
    let mut sum_y = 0.0_f64;

    for stroke in strokes {
        if let Some(centroid) = get_stroke_centroid(stroke) {
            centroids.push(centroid.clone());
            sum_x += centroid.x as f64;
            sum_y += centroid.y as f64;
        }
    }

    let n = centroids.len() as f64;
    if n < 2.0 {
        return 0.0;
    }

    let mean_x = sum_x / n;
    let mean_y = sum_y / n;
    let mut numerator = 0.0_f64;
    let mut denominator = 0.0_f64;

    for centroid in &centroids {
        let dx = centroid.x as f64 - mean_x;
        let dy = centroid.y as f64 - mean_y;
        numerator += dx * dy;
        denominator += dx * dx;
    }

    if denominator.abs() < 1e-5 {
        return 0.0;
    }
    (numerator / denominator).atan()
}

pub fn deskew_strokes(strokes: &[RecognitionStroke]) -> Vec<RecognitionStroke> {
    if strokes.is_empty() {
        return vec![];
    }
    let angle_rad = calculate_skew_angle(strokes);
    if angle_rad.abs() < 0.05 {
        return strokes.to_vec();
    }

    let global_center = get_global_center(strokes);
    let rotation_angle = -angle_rad;

    strokes
        .iter()
        .map(|stroke| {
            let rotated_points = stroke
                .points
                .iter()
                .map(|pt| RecognitionPoint {
                    pos: rotate_point(&pt.pos, &global_center, rotation_angle),
                })
                .collect();
            RecognitionStroke {
                id: stroke.id,
                points: rotated_points,
            }
        })
        .collect()
}
// --- Preprocessing & Rendering ---

fn process_strokes_to_tensor(
    strokes: &[RecognitionStroke],
    max_w: usize,
    max_h: usize,
    padding: f32,
) -> Vec<f32> {
    let bounds_opt = get_strokes_bounds(strokes);
    if bounds_opt.is_none() {
        return vec![0.0; 3 * max_w * max_h];
    }

    let bounds = bounds_opt.unwrap();
    let min_x = bounds.mins.x as f32;
    let max_x = bounds.maxs.x as f32;
    let min_y = bounds.mins.y as f32;
    let max_y = bounds.maxs.y as f32;

    let total_points: usize = strokes.iter().map(|s| s.points.len()).sum();

    if total_points < 2 {
        return vec![0.0; 3 * max_w * max_h];
    }

    let w = (max_x - min_x).max(1e-5);
    let h = (max_y - min_y).max(1e-5);

    let mut scale = (max_h as f32 - 2.0 * padding) / h;
    if w * scale > (max_w as f32 - 2.0 * padding) {
        scale = (max_w as f32 - 2.0 * padding) / w;
    }

    let y_offset = (max_h as f32 - (h * scale)) / 2.0;
    let mut buffer = vec![0.0f32; 3 * max_w * max_h];

    let mut global_pt_idx = 0;
    let time_range = (total_points as f32 - 1.0).max(1e-5);

    for stroke in strokes {
        if stroke.points.len() < 2 {
            global_pt_idx += stroke.points.len();
            continue;
        }

        for i in 0..stroke.points.len() - 1 {
            let p1 = &stroke.points[i];
            let p2 = &stroke.points[i + 1];

            let x0 = (((p1.pos.x as f32) - min_x) * scale + padding).round() as i32;
            let y0 = (((p1.pos.y as f32) - min_y) * scale + y_offset).round() as i32;
            let x1 = (((p2.pos.x as f32) - min_x) * scale + padding).round() as i32;
            let y1 = (((p2.pos.y as f32) - min_y) * scale + y_offset).round() as i32;

            let dx = (p2.pos.x - p1.pos.x) as f32;
            let dy = (p2.pos.y - p1.pos.y) as f32;
            let angle = dy.atan2(dx);
            let norm_angle = (angle + std::f32::consts::PI) / (2.0 * std::f32::consts::PI);
            let norm_time = (global_pt_idx as f32) / time_range;

            draw_line(
                &mut buffer,
                max_w,
                max_h,
                x0,
                y0,
                x1,
                y1,
                1.0,
                norm_time,
                norm_angle,
            );
            global_pt_idx += 1;
        }
        global_pt_idx += 1;
    }

    for val in buffer.iter_mut() {
        *val = (*val - 0.5) * 2.0;
    }
    buffer
}

fn draw_line(
    buf: &mut [f32],
    w: usize,
    h: usize,
    x0: i32,
    y0: i32,
    x1: i32,
    y1: i32,
    v_mask: f32,
    v_time: f32,
    v_angle: f32,
) {
    let dx = (x1 - x0).abs();
    let dy = -(y1 - y0).abs();
    let sx = if x0 < x1 { 1 } else { -1 };
    let sy = if y0 < y1 { 1 } else { -1 };
    let mut err = dx + dy;
    let mut x = x0;
    let mut y = y0;

    let area = w * h;
    loop {
        if x >= 0 && x < w as i32 && y >= 0 && y < h as i32 {
            let idx = (y as usize) * w + (x as usize);
            buf[idx] = v_mask;
            buf[area + idx] = v_time;
            buf[2 * area + idx] = v_angle;
        }
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

#[derive(Debug, Clone)]
pub struct HandwritingRecognizer {
    task_handle: Arc<Mutex<Option<OneOffTaskHandle>>>,
    tasks_tx: EngineTaskSender,
    model_session: Arc<Mutex<Session>>,
}

impl HandwritingRecognizer {
    pub fn new(tasks_tx: EngineTaskSender, model_session: Session) -> Self {
        Self {
            task_handle: Arc::new(Mutex::new(None)),
            tasks_tx,
            model_session: Arc::new(Mutex::new(model_session)),
        }
    }

    // Notice that you now feed it `Vec<RecognitionStroke>` directly from the caller
    // (where `StrokeStore` will have extracted it), OR you can call `Stroke::extract_recognition_data` here
    // if you keep the method on `Stroke`.
    pub fn trigger_recognition_debounced(
        &self,
        raw_stroke_data: Vec<RecognitionStroke>,
        existing_text: Vec<TextLine>,
    ) {
        const TIMEOUT: Duration = Duration::from_millis(1200);
        let mut reinstall_task = false;
        let tasks_tx = self.tasks_tx.clone();
        let model_session = self.model_session.clone();

        let recognition_task = move || {
            if raw_stroke_data.is_empty() {
                return;
            }

            let lines = segment_into_lines(&raw_stroke_data, 0.0);
            let mut recognized_lines: Vec<TextLine> = Vec::new();
            let mut deskew_debug_infos: Vec<crate::engine::DeskewDebugData> = Vec::new();

            let max_w = 512;
            let max_h = 32;

            for (line_index, line_strokes) in lines.into_iter().enumerate() {
                if line_strokes.is_empty() {
                    continue;
                }

                // fingerprinting of strokes
                let mut current_stroke_ids: Vec<StrokeKey> =
                    line_strokes.iter().map(|s| s.id).collect();
                current_stroke_ids.sort(); // Sort to ensure consistent comparison

                // check if we know this line
                let already_recognized = existing_text.iter().find(|old_line| {
                    let mut old_ids = old_line.stroke_keys.clone();
                    old_ids.sort();
                    old_ids == current_stroke_ids
                });

                if let Some(reusable_line) = already_recognized {
                    // skip recogniton and reuse the old result
                    recognized_lines.push(reusable_line.clone());
                    continue;
                }

                let original_bounds =
                    get_strokes_bounds(&line_strokes).unwrap_or_else(|| Aabb::new_invalid());
                let angle_rad = calculate_skew_angle(&line_strokes);
                let center = get_global_center(&line_strokes);
                let deskewed_line = deskew_strokes(&line_strokes);
                let deskewed_bounds =
                    get_strokes_bounds(&deskewed_line).unwrap_or_else(|| Aabb::new_invalid());

                deskew_debug_infos.push(crate::engine::DeskewDebugData {
                    center,
                    aabb_deskewed: deskewed_bounds,
                    angle_rad,
                });

                let buffer = process_strokes_to_tensor(&deskewed_line, max_w, max_h, 2.0);

                let input_array = match ndarray::Array::from_shape_vec((1, 3, max_h, max_w), buffer)
                {
                    Ok(arr) => arr,
                    Err(e) => {
                        error!(
                            "Failed to build input tensor array for line {}: {}",
                            line_index, e
                        );
                        continue;
                    }
                };

                let input_tensor = match Tensor::from_array(input_array) {
                    Ok(tensor) => tensor,
                    Err(e) => {
                        error!("Failed to create ORT tensor for line {}: {}", line_index, e);
                        continue;
                    }
                };

                let recognized_text: Option<String> = {
                    let mut session = match model_session.lock() {
                        Ok(guard) => guard,
                        Err(e) => {
                            error!("Mutex poisoned: {}", e);
                            return;
                        }
                    };

                    let outputs = match session.run(ort::inputs![input_tensor]) {
                        Ok(res) => res,
                        Err(e) => {
                            error!("Inference failed for line {}: {}", line_index, e);
                            continue;
                        }
                    };

                    let output_tensor = match outputs[0].try_extract_tensor::<f32>() {
                        Ok(tensor) => tensor,
                        Err(e) => {
                            error!("Failed to extract tensor for line {}: {}", line_index, e);
                            continue;
                        }
                    };

                    let (shape, data) = output_tensor;

                    if shape.len() >= 3 {
                        let time_steps = shape[1] as usize;
                        let vocab_size = shape[2] as usize;

                        let vocab: &[char] = &[
                            '\0', // Letters: Lowercase (English + German)
                            'a', 'b', 'c', 'd', 'e', 'f', 'g', 'h', 'i', 'j', 'k', 'l', 'm', 'n',
                            'o', 'p', 'q', 'r', 's', 't', 'u', 'v', 'w', 'x', 'y', 'z', 'ä', 'ö',
                            'ü', 'ß', // Letters: Uppercase (English + German)
                            'A', 'B', 'C', 'D', 'E', 'F', 'G', 'H', 'I', 'J', 'K', 'L', 'M', 'N',
                            'O', 'P', 'Q', 'R', 'S', 'T', 'U', 'V', 'W', 'X', 'Y', 'Z', 'Ä', 'Ö',
                            'Ü', // Numbers
                            '0', '1', '2', '3', '4', '5', '6', '7', '8', '9',
                            // Basic Punctuation (Sentence structure)
                            '.', ',', ':', ';', '!', '?',
                            // Dashes and Quotes (Including en-dash)
                            '-', '"', // Brackets and Parentheses
                            '(', ')', '[', ']', // Math, Logic, and Paths
                            '+', '*', '/', '\\', '=', '<', '>', '^',
                            // Currency, Units, Tech, and Misc Symbols
                            '_', '%', '°', '$', '€', '@', '#', '&', '§',
                            // Whitespace (Space)
                            ' ',
                        ];
                        let mut decoded_chars = Vec::new();
                        let mut last_token = None;

                        for t in 0..time_steps {
                            let start_idx = t * vocab_size;
                            let logits_slice = &data[start_idx..start_idx + vocab_size];

                            let mut max_idx = 0;
                            let mut max_val = logits_slice[0];
                            for (vocab_idx, &val) in logits_slice.iter().enumerate().skip(1) {
                                if val > max_val {
                                    max_val = val;
                                    max_idx = vocab_idx;
                                }
                            }

                            if max_idx != 0 && Some(max_idx) != last_token {
                                if let Some(&ch) = vocab.get(max_idx) {
                                    decoded_chars.push(ch);
                                }
                            }
                            last_token = Some(max_idx);
                        }

                        Some(decoded_chars.into_iter().collect())
                    } else {
                        None
                    }
                };

                if let Some(recognized_text) = recognized_text {
                    recognized_lines.push(TextLine {
                        text: recognized_text,
                        bounds: original_bounds,
                        stroke_keys: current_stroke_ids,
                    });
                }
            }

            if !recognized_lines.is_empty() {
                if !deskew_debug_infos.is_empty() {
                    tasks_tx.send(EngineTask::DeskewDebugInfo {
                        data: deskew_debug_infos,
                    });
                }
                tasks_tx.send(EngineTask::HandwritingRecognitionResult {
                    lines: recognized_lines,
                });
            };
        };

        let mut handle_lock = self.task_handle.lock().unwrap();
        if let Some(handle) = handle_lock.as_mut() {
            match handle.replace_task(recognition_task.clone()) {
                Ok(()) => {}
                Err(OneOffTaskError::TimeoutReached) => reinstall_task = true,
                Err(e) => {
                    error!("Could not replace task for handwriting recognition, Err: {e:?}");
                    reinstall_task = true;
                }
            }
        } else {
            reinstall_task = true;
        }

        if reinstall_task {
            *handle_lock = Some(OneOffTaskHandle::new(recognition_task, TIMEOUT));
        }
    }
}
