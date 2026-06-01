//! Rendering performance metrics for helix-tui
//!
//! Tracks Buffer::diff() statistics to help measure rendering optimization impact.
//!
//! # Metrics Explanation
//!
//! - `diff_calls`: Number of Buffer::diff() invocations (roughly equals render frames)
//! - `cells_traversed`: Total cells checked during diffing
//! - `cells_updated`: Cells that actually changed (emitted to terminal)
//! - `wide_chars`: Wide characters (CJK) encountered
//! - `width_calls_saved`: Number of symbol.width() calls avoided by caching
//! - `width_compute_count`: Number of symbol.width() calls actually made

use once_cell::sync::Lazy;
use std::sync::atomic::{AtomicU64, Ordering};

pub struct RenderMetricsInner {
    /// Total number of Buffer::diff() calls
    diff_calls: AtomicU64,
    /// Total number of cells traversed during diffing
    cells_traversed: AtomicU64,
    /// Total number of cells that actually changed (updates emitted)
    cells_updated: AtomicU64,
    /// Total number of wide characters encountered
    wide_chars: AtomicU64,
    /// Number of width() calls saved by caching (equals cells_traversed)
    width_calls_saved: AtomicU64,
    /// Number of width() calls that still needed computation
    width_compute_count: AtomicU64,
}

impl Default for RenderMetricsInner {
    fn default() -> Self {
        Self {
            diff_calls: AtomicU64::new(0),
            cells_traversed: AtomicU64::new(0),
            cells_updated: AtomicU64::new(0),
            wide_chars: AtomicU64::new(0),
            width_calls_saved: AtomicU64::new(0),
            width_compute_count: AtomicU64::new(0),
        }
    }
}

static RENDER_METRICS: Lazy<RenderMetricsInner> = Lazy::new(RenderMetricsInner::default);

impl RenderMetricsInner {
    pub fn record_diff(
        &self,
        cells_traversed: usize,
        cells_updated: usize,
        wide_chars: usize,
        width_compute_count: usize,
    ) {
        self.diff_calls.fetch_add(1, Ordering::Relaxed);
        self.cells_traversed.fetch_add(cells_traversed as u64, Ordering::Relaxed);
        self.cells_updated.fetch_add(cells_updated as u64, Ordering::Relaxed);
        self.wide_chars
            .fetch_add(wide_chars as u64, Ordering::Relaxed);
        // Each traversed cell saves one width() call
        self.width_calls_saved
            .fetch_add(cells_traversed as u64, Ordering::Relaxed);
        self.width_compute_count
            .fetch_add(width_compute_count as u64, Ordering::Relaxed);
    }
}

pub fn record_diff(
    cells_traversed: usize,
    cells_updated: usize,
    wide_chars: usize,
    width_compute_count: usize,
) {
    RENDER_METRICS.record_diff(
        cells_traversed,
        cells_updated,
        wide_chars,
        width_compute_count,
    );
}

#[derive(Default, Debug, Clone, Copy)]
pub struct RenderMetricsSnapshot {
    pub diff_calls: u64,
    pub cells_traversed: u64,
    pub cells_updated: u64,
    pub wide_chars: u64,
    pub width_calls_saved: u64,
    pub width_compute_count: u64,
}

impl RenderMetricsSnapshot {
    /// Calculate time saved (estimated)
    /// Assuming each width() call takes ~50ns
    pub fn estimated_width_time_saved_ns(&self) -> u64 {
        self.width_calls_saved * 50
    }

    /// Print analysis of optimization impact
    pub fn analyze(&self) -> String {
        let total_width_ops = self.width_calls_saved + self.width_compute_count;
        let cache_effectiveness = if total_width_ops > 0 {
            (self.width_calls_saved as f64 / total_width_ops as f64) * 100.0
        } else {
            0.0
        };

        let update_rate = if self.cells_traversed > 0 {
            (self.cells_updated as f64 / self.cells_traversed as f64) * 100.0
        } else {
            0.0
        };

        let wide_char_ratio = if self.cells_updated > 0 {
            (self.wide_chars as f64 / self.cells_updated as f64) * 100.0
        } else {
            0.0
        };

        format!(
            "render-stats analysis:\n\
             │ Metric                  │ Value\n\
             │ diff_calls             │ {}\n\
             │ cells_traversed        │ {} (width calls saved: {})\n\
             │ cells_updated          │ {} ({:.1}% of traversed)\n\
             │ wide_chars             │ {} ({:.1}% of updated)\n\
             │ width_compute_count    │ {} (width() calls made)\n\
             │ ──────────────────────────────────────────\n\
             │ Cell width cache effectiveness: {:.1}%\n\
             │ (Each traversal saves one symbol.width() call)\n\
             │ Estimated time saved: ~{:.3}ms",
            self.diff_calls,
            self.cells_traversed,
            self.width_calls_saved,
            self.cells_updated,
            update_rate,
            self.wide_chars,
            wide_char_ratio,
            self.width_compute_count,
            cache_effectiveness,
            self.estimated_width_time_saved_ns() as f64 / 1_000_000.0
        )
    }
}

pub fn snapshot() -> RenderMetricsSnapshot {
    RenderMetricsSnapshot {
        diff_calls: RENDER_METRICS.diff_calls.load(Ordering::Relaxed),
        cells_traversed: RENDER_METRICS.cells_traversed.load(Ordering::Relaxed),
        cells_updated: RENDER_METRICS.cells_updated.load(Ordering::Relaxed),
        wide_chars: RENDER_METRICS.wide_chars.load(Ordering::Relaxed),
        width_calls_saved: RENDER_METRICS.width_calls_saved.load(Ordering::Relaxed),
        width_compute_count: RENDER_METRICS.width_compute_count.load(Ordering::Relaxed),
    }
}

pub fn reset() {
    RENDER_METRICS.diff_calls.store(0, Ordering::Relaxed);
    RENDER_METRICS.cells_traversed.store(0, Ordering::Relaxed);
    RENDER_METRICS.cells_updated.store(0, Ordering::Relaxed);
    RENDER_METRICS.wide_chars.store(0, Ordering::Relaxed);
    RENDER_METRICS.width_calls_saved.store(0, Ordering::Relaxed);
    RENDER_METRICS.width_compute_count.store(0, Ordering::Relaxed);
}
