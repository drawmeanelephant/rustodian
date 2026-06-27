//! Thread-safe append-only ring buffer for log capture.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

/// Default maximum number of lines retained in memory.
const DEFAULT_MAX_LINES: usize = 10_000;

/// Inner state of the log buffer.
struct LogBufferInner {
    lines: VecDeque<String>,
    max_lines: usize,
}

/// A thread-safe, append-only ring buffer for capturing log output.
///
/// Lines beyond `max_lines` are evicted from the front (oldest first).
/// The buffer is `Clone + Send + Sync` (via `Arc`).
#[derive(Clone)]
pub struct LogBuffer {
    inner: Arc<Mutex<LogBufferInner>>,
}

impl LogBuffer {
    /// Create a new log buffer with the default capacity.
    #[must_use]
    pub fn new() -> Self {
        Self::with_capacity(DEFAULT_MAX_LINES)
    }

    /// Create a new log buffer with the specified maximum line count.
    #[must_use]
    pub fn with_capacity(max_lines: usize) -> Self {
        Self {
            inner: Arc::new(Mutex::new(LogBufferInner {
                lines: VecDeque::with_capacity(max_lines.min(1024)),
                max_lines,
            })),
        }
    }

    /// Append a single line to the buffer.
    ///
    /// If the buffer is at capacity, the oldest line is evicted.
    pub fn push_line(&self, line: String) {
        if let Ok(mut inner) = self.inner.lock() {
            if inner.lines.len() >= inner.max_lines {
                inner.lines.pop_front();
            }
            inner.lines.push_back(line);
        }
    }

    /// Return a snapshot of all buffered lines joined by newlines.
    #[must_use]
    pub fn snapshot(&self) -> String {
        self.inner
            .lock()
            .map(|inner| {
                let mut s = String::new();
                for line in &inner.lines {
                    s.push_str(line);
                    s.push('\n');
                }
                s
            })
            .unwrap_or_default()
    }

    /// Return the number of lines currently in the buffer.
    #[must_use]
    pub fn line_count(&self) -> usize {
        self.inner
            .lock()
            .map(|inner| inner.lines.len())
            .unwrap_or(0)
    }

    /// Drain all lines from the buffer and return them joined by newlines.
    ///
    /// The buffer is empty after this call.
    pub fn drain_all(&self) -> String {
        self.inner
            .lock()
            .map(|mut inner| {
                let mut s = String::new();
                for line in inner.lines.drain(..) {
                    s.push_str(&line);
                    s.push('\n');
                }
                s
            })
            .unwrap_or_default()
    }
}

impl Default for LogBuffer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_push_and_snapshot() {
        let buf = LogBuffer::new();
        buf.push_line("hello".to_string());
        buf.push_line("world".to_string());
        assert_eq!(buf.snapshot(), "hello\nworld\n");
        assert_eq!(buf.line_count(), 2);
    }

    #[test]
    fn test_eviction() {
        let buf = LogBuffer::with_capacity(3);
        for i in 0..5 {
            buf.push_line(format!("line {i}"));
        }
        assert_eq!(buf.line_count(), 3);
        // Should contain lines 2, 3, 4 (oldest evicted)
        let snap = buf.snapshot();
        assert!(snap.contains("line 2"));
        assert!(snap.contains("line 4"));
        assert!(!snap.contains("line 0"));
    }

    #[test]
    fn test_drain_all() {
        let buf = LogBuffer::new();
        buf.push_line("a".to_string());
        buf.push_line("b".to_string());
        let drained = buf.drain_all();
        assert_eq!(drained, "a\nb\n");
        assert_eq!(buf.line_count(), 0);
        assert_eq!(buf.snapshot(), "");
    }

    #[test]
    fn test_clone_shares_state() {
        let buf1 = LogBuffer::new();
        let buf2 = buf1.clone();
        buf1.push_line("from buf1".to_string());
        assert_eq!(buf2.line_count(), 1);
    }
}
