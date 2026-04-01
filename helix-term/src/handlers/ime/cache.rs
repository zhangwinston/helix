//! Incremental IME cache for performance optimization.
//!
//! This module provides an incremental caching system for IME regions
//! that avoids redundant syntax tree queries and provides fine-grained
//! caching at the syntax node level.

use helix_core::syntax::{ImeSensitiveRegion, detect_ime_sensitive_region};
use helix_view::document::Document;
use std::collections::HashMap;
use std::time::{Duration, Instant};
use helix_core::tree_sitter::Node;

/// Incremental IME region cache
pub struct IncrementalImeCache {
    /// Cache entries keyed by document ID
    entries: HashMap<u64, DocumentCache>,
    /// Performance statistics
    stats: CacheStats,
}

/// Cache entries for a specific document
struct DocumentCache {
    /// Node-level cache entries with byte ranges and regions
    node_regions: HashMap<usize, CachedRegion>,
    /// Last document version this cache was updated for
    document_version: i32,
    /// Last time this cache was accessed
    last_access: Instant,
}

/// A cached IME region with its byte range
struct CachedRegion {
    region: ImeSensitiveRegion,
    byte_range: (usize, usize),
}

/// Cache performance statistics
#[derive(Debug, Default)]
pub struct CacheStats {
    pub total_queries: u64,
    pub cache_hits: u64,
    pub cache_misses: u64,
    pub incremental_updates: u64,
    pub full_rebuilds: u64,
    pub nodes_cached: u64,
}

impl IncrementalImeCache {
    /// Create a new incremental cache
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
            stats: CacheStats::default(),
        }
    }

    /// Get or detect IME region for a cursor position
    pub fn get_or_detect_region(
        &mut self,
        doc: &Document,
        cursor_pos: usize,
    ) -> ImeSensitiveRegion {
        let start_time = Instant::now();
        self.stats.total_queries += 1;

        // Use document pointer as ID (simplified approach)
        let doc_id = doc as *const _ as u64;
        let doc_version = doc.version();

        let doc_cache = self.entries.entry(doc_id).or_insert_with(|| DocumentCache {
            node_regions: HashMap::new(),
            document_version: doc_version,
            last_access: Instant::now(),
        });

        // Check if we need to refresh cache due to document version change
        if doc_cache.document_version != doc_version {
            doc_cache.node_regions.clear();
            doc_cache.document_version = doc_version;
            self.stats.full_rebuilds += 1;
        }

        // Try to find a cached node that contains the cursor position
        if let Some(cached_region) = self.find_cached_region(doc_id, cursor_pos, doc_version) {
            self.stats.cache_hits += 1;

            let duration = start_time.elapsed();
            log::trace!("IME cache hit: pos={}, region={:?}, took={:?}",
                cursor_pos, cached_region, duration);

            cached_region
        } else {
            self.stats.cache_misses += 1;

            // Perform full detection and cache the result
            let detection = self.detect_and_cache(doc_id, cursor_pos, doc);

            let duration = start_time.elapsed();
            log::trace!("IME cache miss: pos={}, region={:?}, took={:?}",
                cursor_pos, detection.region, duration);

            detection.region
        }
    }

    /// Find cached region for cursor position
    fn find_cached_region(
        &self,
        doc_id: u64,
        cursor_pos: usize,
        _doc_version: i32,
    ) -> Option<ImeSensitiveRegion> {
        if let Some(doc_cache) = self.entries.get(&doc_id) {
            // Find a cached region that contains the cursor position
            for cached_region in doc_cache.node_regions.values() {
                if cached_region.byte_range.0 <= cursor_pos
                    && cursor_pos < cached_region.byte_range.1 {
                    return Some(cached_region.region);
                }
            }
        }
        None
    }

    /// Detect region and cache it
    fn detect_and_cache(
        &mut self,
        doc_id: u64,
        cursor_pos: usize,
        doc: &Document,
    ) -> helix_core::syntax::ImeRegionDetection {
        let syntax = doc.syntax();
        let text = doc.text().slice(..);
        let loader = doc.syntax_loader();

        // Perform detection
        let detection = detect_ime_sensitive_region(syntax, text, &*loader, cursor_pos);

        // If we got a node range, cache it
        if let Some(range) = detection.node_range {
            let node_id = 0; // Simplified - use 0 as default node ID
            self.cache_node_region(
                doc_id,
                node_id,
                detection.region,
                (range.0, range.1),
                doc.version(),
            );
        }

        detection
    }

    /// Cache a node's region
    fn cache_node_region(
        &mut self,
        doc_id: u64,
        node_id: usize,
        region: ImeSensitiveRegion,
        byte_range: (usize, usize),
        document_version: i32,
    ) {
        let doc_cache = self.entries.entry(doc_id).or_insert_with(|| DocumentCache {
            node_regions: HashMap::new(),
            document_version,
            last_access: Instant::now(),
        });

        // Store the region with its byte range
        let cached = CachedRegion {
            region,
            byte_range,
        };
        doc_cache.node_regions.insert(node_id, cached);
        self.stats.nodes_cached += 1;
    }

    /// Invalidate cache for a document
    fn invalidate_document(&mut self, doc_id: u64, new_version: i32) {
        if let Some(doc_cache) = self.entries.get_mut(&doc_id) {
            doc_cache.node_regions.clear();
            doc_cache.document_version = new_version;
            self.stats.incremental_updates += 1;
        }
    }

    /// Perform incremental update after document edit
    pub fn incremental_update(
        &mut self,
        doc: &Document,
        _changed_range: (usize, usize),
    ) {
        let doc_version = doc.version();
        let doc_id = doc as *const _ as u64;

        if let Some(doc_cache) = self.entries.get_mut(&doc_id) {
            // Simplified version - just clear the cache on any edit
            doc_cache.node_regions.clear();
            doc_cache.document_version = doc_version;
            self.stats.incremental_updates += 1;
        }
    }

    /// Clean up old cache entries
    pub fn cleanup_old_entries(&mut self, max_age: Duration) {
        let now = Instant::now();
        let mut to_remove = Vec::new();

        for (doc_id, doc_cache) in &self.entries {
            if now.duration_since(doc_cache.last_access) > max_age {
                to_remove.push(*doc_id);
            }
        }

        for doc_id in to_remove {
            self.entries.remove(&doc_id);
        }
    }

    /// Get cache statistics
    pub fn get_stats(&self) -> &CacheStats {
        &self.stats
    }

    /// Reset statistics
    pub fn reset_stats(&mut self) {
        self.stats = CacheStats::default();
    }

    /// Get average cache size per document
    pub fn get_average_cache_size(&self) -> f64 {
        if self.entries.is_empty() {
            0.0
        } else {
            let total: usize = self.entries.values()
                .map(|dc| dc.node_regions.len())
                .sum();
            total as f64 / self.entries.len() as f64
        }
    }
}

impl Default for IncrementalImeCache {
    fn default() -> Self {
        Self::new()
    }
}

/// Helper to extract node ID from tree-sitter node
pub fn get_node_id(node: &Node) -> usize {
    node.id() as usize
}