use crate::engine::{EngineTask, EngineTaskSender};
use crate::strokes::Stroke;
use crate::tasks::{OneOffTaskError, OneOffTaskHandle};
use p2d::math::Vector2;
use std::sync::Arc;
use std::time::Duration;
use tracing::error;

#[derive(Debug, Clone)]
pub struct RecognitionPoint {
    pub pos: Vector2,
    pub pressure: f64,
}

#[derive(Debug, Clone)]
pub struct RecognitionStroke {
    pub points: Vec<RecognitionPoint>,
}

pub fn extract_stroke_data(strokes: &[Arc<Stroke>]) -> Vec<RecognitionStroke> {
    strokes
        .iter()
        .filter_map(|stroke| {
            let Stroke::BrushStroke(brush_stroke) = stroke.as_ref() else {
                return None;
            };

            let mut points = Vec::new();
            points.push(RecognitionPoint {
                pos: brush_stroke.path.start.pos,
                pressure: brush_stroke.path.start.pressure,
            });

            for segment in &brush_stroke.path.segments {
                let el = segment.end();
                points.push(RecognitionPoint {
                    pos: el.pos,
                    pressure: el.pressure,
                });
            }

            Some(RecognitionStroke { points })
        })
        .collect()
}

/// Manager struct to handle debounced background handwriting recognition
#[derive(Debug, Clone)]
pub struct HandwritingRecognizer {
    task_handle: Arc<std::sync::Mutex<Option<OneOffTaskHandle>>>,
    tasks_tx: EngineTaskSender,
}

impl HandwritingRecognizer {
    pub fn new(tasks_tx: EngineTaskSender) -> Self {
        Self {
            task_handle: Arc::new(std::sync::Mutex::new(None)),
            tasks_tx,
        }
    }

    /// Triggers recognition after an inactivity timeout (e.g., 600ms after the user stops writing)
    pub fn trigger_recognition_debounced(&self, strokes: Vec<Arc<Stroke>>) {
        const TIMEOUT: Duration = Duration::from_millis(600);
        let strokes_clone = strokes;
        let mut reinstall_task = false;

        let tasks_tx = self.tasks_tx.clone();
        let recognition_task = move || {
            let stroke_data = extract_stroke_data(&strokes_clone);
            if stroke_data.is_empty() {
                return;
            }

            tracing::info!(
                "Background Recognition triggered for {} brush strokes.",
                stroke_data.len()
            );

            //TODO i guess implement the engine here whats so difficult
            let dummy_recognized_text = String::from("Hello, World!");

            tasks_tx.send(EngineTask::HandwritingRecognitionResult {
                text: dummy_recognized_text,
            });
        };

        let mut handle_lock = self.task_handle.lock().unwrap();

        if let Some(handle) = handle_lock.as_mut() {
            match handle.replace_task(recognition_task.clone()) {
                Ok(()) => {}
                Err(OneOffTaskError::TimeoutReached) => {
                    reinstall_task = true;
                }
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
