use super::ReturnMode;
use std::collections::VecDeque;

#[derive(Debug, Clone)]
pub struct RollingSma {
    window: usize,
    buf: VecDeque<f64>,
    sum: f64,
}

impl RollingSma {
    pub fn new(window: usize) -> Self {
        Self {
            window,
            buf: VecDeque::new(),
            sum: 0.0,
        }
    }

    pub fn update(&mut self, value: f64) -> Option<f64> {
        if self.window == 0 {
            return None;
        }

        self.buf.push_back(value);
        self.sum += value;
        while self.buf.len() > self.window {
            if let Some(front) = self.buf.pop_front() {
                self.sum -= front;
            }
        }

        if self.buf.len() == self.window {
            Some(self.sum / self.window as f64)
        } else {
            None
        }
    }
}

#[derive(Debug, Clone)]
pub struct RollingVar {
    window: usize,
    buf: VecDeque<f64>,
    sum: f64,
    sum_sq: f64,
}

impl RollingVar {
    pub fn new(window: usize) -> Self {
        Self {
            window,
            buf: VecDeque::new(),
            sum: 0.0,
            sum_sq: 0.0,
        }
    }

    pub fn update(&mut self, value: f64) -> Option<f64> {
        if self.window == 0 {
            return None;
        }

        self.buf.push_back(value);
        self.sum += value;
        self.sum_sq += value * value;
        while self.buf.len() > self.window {
            if let Some(front) = self.buf.pop_front() {
                self.sum -= front;
                self.sum_sq -= front * front;
            }
        }

        if self.buf.len() == self.window {
            let n = self.window as f64;
            let mean = self.sum / n;
            let var = (self.sum_sq / n) - mean * mean;
            Some(var.max(0.0).sqrt())
        } else {
            None
        }
    }
}

#[derive(Debug, Clone)]
pub struct RollingRsi {
    window: usize,
    prev_close: Option<f64>,
    diffs: VecDeque<f64>,
    sum_gains: f64,
    sum_losses: f64,
    return_mode: ReturnMode,
}

impl RollingRsi {
    pub fn new(window: usize, return_mode: ReturnMode) -> Self {
        Self {
            window,
            prev_close: None,
            diffs: VecDeque::new(),
            sum_gains: 0.0,
            sum_losses: 0.0,
            return_mode,
        }
    }

    pub fn update(&mut self, close: f64) -> Option<f64> {
        if self.window == 0 {
            self.prev_close = Some(close);
            return None;
        }

        let Some(prev) = self.prev_close else {
            self.prev_close = Some(close);
            return None;
        };
        self.prev_close = Some(close);

        if prev <= 0.0 || !prev.is_finite() || !close.is_finite() {
            return None;
        }

        let diff = match self.return_mode {
            ReturnMode::Log => (close / prev).ln(),
            ReturnMode::Pct => close / prev - 1.0,
        };

        self.diffs.push_back(diff);
        if diff > 0.0 {
            self.sum_gains += diff;
        } else {
            self.sum_losses += -diff;
        }

        while self.diffs.len() > self.window {
            if let Some(front) = self.diffs.pop_front() {
                if front > 0.0 {
                    self.sum_gains -= front;
                } else {
                    self.sum_losses -= -front;
                }
            }
        }

        if self.diffs.len() < self.window {
            return None;
        }

        if self.sum_gains + self.sum_losses == 0.0 {
            return Some(50.0);
        }

        let rs = self.sum_gains / self.sum_losses.max(1e-9);
        Some(100.0 - (100.0 / (1.0 + rs)))
    }
}
