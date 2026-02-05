use parking_lot::Mutex;
use std::collections::VecDeque;
use std::io::{self, Write};
use std::sync::Arc;
use tracing_subscriber::fmt::MakeWriter;

pub struct LogStore {
    lines: VecDeque<String>,
    max_lines: usize,
}

impl LogStore {
    pub fn new(max_lines: usize) -> Self {
        Self {
            lines: VecDeque::new(),
            max_lines: max_lines.max(1),
        }
    }

    pub fn push_line(&mut self, line: impl Into<String>) {
        let line = line.into();
        if line.is_empty() {
            return;
        }
        self.lines.push_back(line);
        while self.lines.len() > self.max_lines {
            self.lines.pop_front();
        }
    }

    pub fn snapshot(&self) -> Vec<String> {
        self.lines.iter().cloned().collect()
    }
}

#[derive(Clone)]
pub struct LogMakeWriter {
    store: Arc<Mutex<LogStore>>,
}

impl LogMakeWriter {
    pub fn new(store: Arc<Mutex<LogStore>>) -> Self {
        Self { store }
    }
}

impl<'a> MakeWriter<'a> for LogMakeWriter {
    type Writer = LogWriter;

    fn make_writer(&'a self) -> Self::Writer {
        LogWriter {
            store: self.store.clone(),
            partial: String::new(),
        }
    }
}

pub struct LogWriter {
    store: Arc<Mutex<LogStore>>,
    partial: String,
}

impl Write for LogWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let chunk = String::from_utf8_lossy(buf);
        self.partial.push_str(&chunk);
        while let Some(idx) = self.partial.find('\n') {
            let line = self.partial[..idx].trim_end_matches('\r').to_string();
            self.partial.drain(..=idx);
            if !line.is_empty() {
                self.store.lock().push_line(line);
            }
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl Drop for LogWriter {
    fn drop(&mut self) {
        let line = self.partial.trim().to_string();
        if !line.is_empty() {
            self.store.lock().push_line(line);
        }
    }
}
