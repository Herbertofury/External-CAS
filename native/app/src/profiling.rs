//! Measurements of the real app's UI/render-submission frames. These are CPU
//! frame costs and presentation intervals, not GPU timestamps or game speedups.
use std::time::Instant;
#[derive(Default)]
pub struct Profile {
    started: bool,
    previous: Option<Instant>,
    cpu_ms: Vec<f64>,
    intervals_ms: Vec<f64>,
}
impl Profile {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn frames(&self) -> usize {
        self.cpu_ms.len()
    }
    pub fn running(&self, count: usize) -> bool {
        count > 0 && self.started && self.cpu_ms.len() < count
    }
    pub fn finished(&self, count: usize) -> bool {
        count == 0 || (self.started && self.cpu_ms.len() >= count)
    }
    pub fn frame(&mut self, count: usize, ready: bool, start: Instant) {
        if count == 0 {
            return;
        }
        if !self.started {
            if ready {
                self.started = true;
                self.previous = Some(start);
            }
            return;
        }
        if self.cpu_ms.len() >= count {
            return;
        }
        if let Some(previous) = self.previous.replace(start) {
            self.intervals_ms
                .push(start.duration_since(previous).as_secs_f64() * 1000.);
        }
        self.cpu_ms.push(start.elapsed().as_secs_f64() * 1000.);
    }
    pub fn report(&self) -> serde_json::Value {
        serde_json::json!({"frames":self.cpu_ms.len(),"ui_and_render_submission_cpu_ms":stats(&self.cpu_ms),"app_frame_interval_ms":stats(&self.intervals_ms),"gpu_timing":false,"notes":"Actual native UI/wgpu workload; intervals include scheduler/presentation. Controlled diagnostic scrolling is optional. No in-game comparison or speedup ratio is implied."})
    }
}
fn stats(values: &[f64]) -> serde_json::Value {
    if values.is_empty() {
        return serde_json::Value::Null;
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    let percentile = |p: f64| sorted[((sorted.len() - 1) as f64 * p).round() as usize];
    serde_json::json!({"mean":sorted.iter().sum::<f64>()/sorted.len()as f64,"p50":percentile(0.5),"p95":percentile(0.95),"p99":percentile(0.99),"max":sorted.last()})
}
