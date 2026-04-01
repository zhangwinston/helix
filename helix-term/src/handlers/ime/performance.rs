//! Performance monitoring and metrics for IME operations.
//!
//! This module provides comprehensive performance monitoring for IME operations
//! including latency tracking, cache hit rates, and resource usage.

use std::{
    collections::{HashMap, VecDeque},
    time::{Duration, Instant},
};

/// Performance metrics collector for IME operations
#[allow(dead_code)]
pub struct ImePerformanceMonitor {
    // Latency tracking
    latency_samples: VecDeque<Duration>,
    max_latency_samples: usize,

    // Operation counts
    operation_counts: HashMap<String, u64>,

    // Cache statistics
    cache_hits: u64,
    cache_misses: u64,
    total_queries: u64,
    incremental_updates: u64,
    full_rebuilds: u64,
    nodes_cached: u64,

    // Performance thresholds
    thresholds: PerformanceThresholds,
}

/// Cache performance statistics
#[derive(Debug, Default, Clone)]
pub struct CacheStats {
    pub hits: u64,
    pub misses: u64,
    pub total_queries: u64,
    pub incremental_updates: u64,
}

impl CacheStats {
    /// Get cache hit rate as a percentage
    pub fn hit_rate(&self) -> f64 {
        if self.total_queries == 0 {
            0.0
        } else {
            (self.hits as f64 / self.total_queries as f64) * 100.0
        }
    }
}

/// Performance thresholds for alerts
#[derive(Debug, Clone)]
pub struct PerformanceThresholds {
    pub max_latency: Duration,
    pub min_cache_hit_rate: f64,
    pub max_memory_kb: f64,
}

impl Default for PerformanceThresholds {
    fn default() -> Self {
        Self {
            max_latency: Duration::from_millis(10),
            min_cache_hit_rate: 85.0,
            max_memory_kb: 1024.0, // 1MB
        }
    }
}

/// Performance report
#[derive(Debug, Clone)]
pub struct PerformanceReport {
    pub timestamp: Instant,
    pub average_latency: Duration,
    pub p95_latency: Duration,
    pub p99_latency: Duration,
    pub cache_hit_rate: f64,
    pub alerts: Vec<PerformanceAlert>,
}

/// Performance alert when thresholds are exceeded
#[derive(Debug, Clone)]
pub enum PerformanceAlert {
    HighLatency {
        operation: String,
        latency: Duration,
        threshold: Duration,
    },
    LowCacheHitRate {
        hit_rate: f64,
        threshold: f64,
    },
    HighMemoryUsage {
        memory_kb: f64,
        threshold: f64,
    },
}

impl ImePerformanceMonitor {
    /// Create a new performance monitor
    pub fn new() -> Self {
        Self {
            latency_samples: VecDeque::with_capacity(1000),
            max_latency_samples: 1000,
            operation_counts: HashMap::new(),
            cache_hits: 0,
            cache_misses: 0,
            total_queries: 0,
            incremental_updates: 0,
            full_rebuilds: 0,
            nodes_cached: 0,
            thresholds: PerformanceThresholds::default(),
        }
    }

    /// Create a performance monitor with custom thresholds
    pub fn with_thresholds(thresholds: PerformanceThresholds) -> Self {
        Self {
            thresholds,
            ..Self::new()
        }
    }

    /// Record operation latency
    pub fn record_latency(&mut self, operation: &str, latency: Duration) {
        // Record latency
        if self.latency_samples.len() >= self.max_latency_samples {
            self.latency_samples.pop_front();
        }
        self.latency_samples.push_back(latency);

        // Count operation
        *self.operation_counts.entry(operation.to_string()).or_insert(0) += 1;

        // Check for high latency alert
        if latency > self.thresholds.max_latency {
            log::warn!(
                "High latency for {}: {:?} (threshold: {:?})",
                operation, latency, self.thresholds.max_latency
            );
        }
    }

    /// Record cache hit
    pub fn record_cache_hit(&mut self) {
        self.cache_hits += 1;
        self.total_queries += 1;
    }

    /// Record cache miss
    pub fn record_cache_miss(&mut self) {
        self.cache_misses += 1;
        self.total_queries += 1;
    }

    /// Record incremental cache update
    pub fn record_incremental_update(&mut self) {
        self.incremental_updates += 1;
    }

    /// Generate a performance report
    pub fn generate_report(&self) -> anyhow::Result<PerformanceReport> {
        let mut alerts = Vec::new();

        // Calculate latency statistics
        let (avg, p95, p99) = if self.latency_samples.is_empty() {
            (Duration::ZERO, Duration::ZERO, Duration::ZERO)
        } else {
            let _sorted_samples: Vec<_> = self.latency_samples.iter().cloned().collect();
            let sorted_samples = {
                let mut v: Vec<_> = self.latency_samples.iter().cloned().collect();
                v.sort();
                v
            };

            let avg = sorted_samples.iter().sum::<Duration>() / sorted_samples.len() as u32;
            let p95_idx = (sorted_samples.len() as f64 * 0.95) as usize;
            let p99_idx = (sorted_samples.len() as f64 * 0.99) as usize;
            let p95 = sorted_samples.get(p95_idx).copied().unwrap_or(Duration::ZERO);
            let p99 = sorted_samples.get(p99_idx).copied().unwrap_or(Duration::ZERO);

            (avg, p95, p99)
        };

        // Get cache statistics
        let cache_hit_rate = if self.total_queries == 0 {
            0.0
        } else {
            (self.cache_hits as f64 / self.total_queries as f64) * 100.0
        };

        // Check cache hit rate
        if cache_hit_rate < self.thresholds.min_cache_hit_rate {
            alerts.push(PerformanceAlert::LowCacheHitRate {
                hit_rate: cache_hit_rate,
                threshold: self.thresholds.min_cache_hit_rate,
            });
        }

        Ok(PerformanceReport {
            timestamp: Instant::now(),
            average_latency: avg,
            p95_latency: p95,
            p99_latency: p99,
            cache_hit_rate,
            alerts,
        })
    }

    /// Print performance report
    pub fn print_report(&self) -> anyhow::Result<()> {
        let report = self.generate_report()?;

        println!("\n=== IME Performance Report ===");
        println!("Timestamp: {:?}", report.timestamp);
        println!("\nLatency Statistics:");
        println!("  Average: {:?}", report.average_latency);
        println!("  P95: {:?}", report.p95_latency);
        println!("  P99: {:?}", report.p99_latency);
        println!("  Max threshold: {:?}", self.thresholds.max_latency);

        println!("\nCache Performance:");
        println!("  Hit rate: {:.2}% (min: {:.2}%)", report.cache_hit_rate, self.thresholds.min_cache_hit_rate);

        if !report.alerts.is_empty() {
            println!("\n⚠️  Performance Alerts:");
            for alert in &report.alerts {
                match alert {
                    PerformanceAlert::HighLatency { operation, latency, threshold } => {
                        println!("  - High latency for {}: {:?} (threshold: {:?})", operation, latency, threshold);
                    }
                    PerformanceAlert::LowCacheHitRate { hit_rate, threshold } => {
                        println!("  - Low cache hit rate: {:.2}% (threshold: {:.2}%)", hit_rate, threshold);
                    }
                    PerformanceAlert::HighMemoryUsage { memory_kb, threshold } => {
                        println!("  - High memory usage: {:.2}KB (threshold: {:.2}KB)", memory_kb, threshold);
                    }
                }
            }
        } else {
            println!("\n✅ All performance metrics within thresholds");
        }

        println!("===================================\n");

        Ok(())
    }

    /// Reset all statistics
    pub fn reset(&mut self) {
        self.latency_samples.clear();
        self.operation_counts.clear();
        self.cache_hits = 0;
        self.cache_misses = 0;
        self.total_queries = 0;
        self.incremental_updates = 0;
        self.full_rebuilds = 0;
        self.nodes_cached = 0;
    }

    /// Get current cache statistics
    pub fn get_cache_stats(&self) -> CacheStats {
        CacheStats {
            hits: self.cache_hits,
            misses: self.cache_misses,
            total_queries: self.total_queries,
            incremental_updates: self.incremental_updates,
        }
    }
}

impl Default for ImePerformanceMonitor {
    fn default() -> Self {
        Self::new()
    }
}

/// Performance profiler for IME operations
pub struct Profiler {
    start_time: Instant,
    operation_name: String,
}

impl Profiler {
    /// Create a new profiler for an operation
    pub fn new(operation: &str) -> Self {
        Self {
            start_time: Instant::now(),
            operation_name: operation.to_string(),
        }
    }

    /// Finish profiling and record the result
    pub fn finish(self) {
        let elapsed = self.start_time.elapsed();
        log::debug!("IME operation '{}' took {:?}", self.operation_name, elapsed);
    }
}

impl Drop for Profiler {
    fn drop(&mut self) {
        let elapsed = self.start_time.elapsed();
        log::debug!("IME operation '{}' took {:?}", self.operation_name, elapsed);
    }
}

