//! IME (Input Method Editor) automatic control handler.
//!
//! This module provides automatic IME state management based on cursor position
//! and editor mode. IME is automatically enabled in string/comment regions and
//! disabled in code regions.

pub mod platform;

pub mod engine;
pub mod metrics;
pub mod performance;
#[cfg(any(test, feature = "integration"))]
pub mod registry;
#[cfg(not(any(test, feature = "integration")))]
mod registry;
mod scheduler;
pub mod cache;
#[cfg(feature = "async")]
pub mod async_handler;
#[cfg(not(feature = "async"))]
mod async_handler;

use anyhow::Result;
use helix_core::syntax::{detect_ime_sensitive_region, ImeSensitiveRegion};
use helix_event::register_hook;
use helix_view::{
    document::Mode,
    events::{DocumentDidClose, DocumentDidOpen, SelectionDidChange},
    Editor, ViewId,
};

use crate::events::OnModeSwitch;

use crate::handlers::ime::platform::{is_ime_enabled, set_ime_enabled, initialize, ImeDetector, ImeSettings};
use engine::{is_sensitive, ImeEngine, ImeRegionSpan};
use helix_view::document::Document;
use std::time::{Duration, Instant};

/// Initialize IME support when the application starts.
/// This should be called early in the application lifecycle.
pub fn initialize_ime_support() -> Result<()> {
    match initialize() {
        Ok(()) => {
            log::info!("IME support initialized successfully");

            // Log detected IME information
            match platform::get_ime_info() {
                Ok(ime_info) => {
                    log::info!(
                        "Detected IME: {} (capabilities: {:?})",
                        ime_info.name, ime_info.capabilities
                    );
                }
                Err(e) => {
                    log::warn!("Failed to get IME info: {}", e);
                }
            }

            // Start cleanup task
            start_cleanup_task();

            Ok(())
        }
        Err(e) => {
            log::error!("Failed to initialize IME support: {}", e);
            Err(e)
        }
    }
}

/// Get the primary cursor byte position from a document and view.
///
/// This function extracts the cursor position from the primary selection
/// and converts it to a byte offset for use in region detection.
///
/// # Arguments
/// * `doc` - The document containing the text
/// * `view_id` - The view ID to get the selection from
///
/// # Returns
/// The cursor position as a byte offset
fn get_cursor_byte_pos(doc: &Document, view_id: ViewId) -> usize {
    let text = doc.text().slice(..);
    let selection = doc.selection(view_id);
    let primary_selection = selection.primary();
    let cursor_pos = primary_selection.cursor(text);
    text.char_to_byte(cursor_pos)
}

/// Update the cached IME state in the registry.
///
/// This function updates the cached IME state after a successful state change
/// or when the cache needs to be refreshed (e.g., after detecting a manual toggle).
///
/// # Arguments
/// * `doc_id` - Document ID for cache lookup
/// * `view_id` - View ID for cache lookup
/// * `mode` - Current editor mode
/// * `new_state` - The new IME state to cache
fn update_ime_cache(
    doc_id: helix_view::DocumentId,
    view_id: ViewId,
    mode: Mode,
    new_state: bool,
) {
    registry::with_context_mut(doc_id, view_id, mode, |ctx| {
        ctx.cached_ime_state = Some(new_state);
    });
}

/// Initialize IME context for a document+view combination.
///
/// This function resets the ImeContext and closes IME if it's currently enabled.
/// Called when a view is created or a document is opened.
pub fn initialize_view_ime_state(editor: &mut Editor, view_id: ViewId) {
    if !editor.tree.contains(view_id) {
        return;
    }

    let doc_id = editor.tree.get(view_id).doc;
    if let Some(doc) = editor.documents.get_mut(&doc_id) {
        doc.ensure_view_init(view_id);
    }

    // Reset IME context for this document+view combination
    registry::with_context_mut(doc_id, view_id, editor.mode(), |ctx| {
        ctx.reset(editor.mode())
    });

    // Close IME if it's currently enabled (FR-001)
    // Query system state during initialization (cache may not be valid yet)
    let ime_was_enabled = read_ime_enabled("view initialization");
    if ime_was_enabled {
        if let Err(e) = set_ime_enabled(false) {
            log::error!("Failed to close IME during view initialization: {}", e);
        } else {
            // Update cache after successful state change
            update_ime_cache(doc_id, view_id, editor.mode(), false);
            return;
        }
    }

    // Update cache even if IME was already disabled (or we failed to close it)
    update_ime_cache(doc_id, view_id, editor.mode(), false);
}

/// Handle cursor movement and update IME state accordingly.
///
/// This function detects the IME-sensitive region at the cursor position and
/// updates IME state if the region has changed.
pub fn handle_cursor_move(editor: &mut Editor, view_id: ViewId) -> Result<()> {
    metrics::record_cursor_move_call();

    // Only process in Insert mode (FR-002)
    if editor.mode() != Mode::Insert {
        metrics::record_cursor_move_skip_not_insert();
        return Ok(());
    }

    if !editor.tree.contains(view_id) {
        metrics::record_cursor_move_skip_invalid_view();
        return Ok(());
    }

    // No need to prune orphans on every cursor move - it's expensive
    // Orphans are cleaned up on DocumentDidClose and DocumentDidOpen events

    let timer = Instant::now();
    let editor_mode = editor.mode();
    let doc_id = editor.tree.get(view_id).doc;
    let doc = match editor.documents.get_mut(&doc_id) {
        Some(doc) => doc,
        None => {
            metrics::record_cursor_move_duration(timer.elapsed());
            return Err(anyhow::anyhow!("Document not found"));
        }
    };
    doc.ensure_view_init(view_id);
    let doc_version = doc.version();

    // Get primary cursor position (FR-020)
    let cursor_byte_pos = get_cursor_byte_pos(doc, view_id);

    // Determine if we need to re-run region detection based on cursor position changes
    let needs_detection = registry::with_context_mut(doc_id, view_id, editor_mode, |ctx| {
        let cursor_changed = ctx.cached_cursor_byte_pos != Some(cursor_byte_pos);
        let region_unknown = ctx.current_region.is_none();
        if cursor_changed || region_unknown {
            ctx.cached_cursor_byte_pos = Some(cursor_byte_pos);
            true
        } else {
            false
        }
    });

    // First check if we have a cached region even if cursor changed
    let cached_region = registry::with_context_mut(doc_id, view_id, editor_mode, |ctx| {
        if let Some(span) = ctx.cached_region_span {
            if span.doc_version == doc_version
                && cursor_byte_pos >= span.start
                && cursor_byte_pos < span.end
            {
                return Some(span.region);
            }
            // Clear stale cache, but needs_detection will trigger recalculation
            ctx.cached_region_span = None;
        }
        None
    });

    let new_region = if !needs_detection {
        if cached_region.is_some() {
            metrics::record_region_cache_hit();
        }
        registry::with_context_mut(doc_id, view_id, editor_mode, |ctx| {
            ctx.current_region.unwrap_or(ImeSensitiveRegion::Code)
        })
    } else if let Some(region) = cached_region {
        // We have a cached region even though needs_detection was true
        // This can happen when cursor moved but stayed within the same region
        metrics::record_region_cache_hit();
        region
    } else {
        let cached_region = registry::with_context_mut(doc_id, view_id, editor_mode, |ctx| {
            if let Some(span) = ctx.cached_region_span {
                if span.doc_version == doc_version
                    && cursor_byte_pos >= span.start
                    && cursor_byte_pos < span.end
                {
                    return Some(span.region);
                }
                ctx.cached_region_span = None;
            }
            None
        });

        if let Some(region) = cached_region {
            metrics::record_region_cache_hit();
            region
        } else {
            metrics::record_region_detection();
            let text = doc.text().slice(..);
            let syntax = doc.syntax();
            let loader = doc.syntax_loader();
            let detection = detect_ime_sensitive_region(syntax, text, &*loader, cursor_byte_pos);
            let region = detection.region;
            registry::with_context_mut(doc_id, view_id, editor_mode, |ctx| {
                ctx.cached_region_span = detection.node_range.and_then(|(start, end)| {
                    (start < end).then_some(engine::ImeRegionSpan {
                        doc_version,
                        start,
                        end,
                        region,
                    })
                });
            });
            region
        }
    };

    // Get cached IME state and current region (fast, in lock)
    let (cached_ime_state, current_region, is_sensitive_region) = registry::with_context_mut(
        doc_id,
        view_id,
        editor_mode,
        |ctx| {
            let is_sensitive_region = ctx
                .current_region
                .map(is_sensitive)
                .unwrap_or(false);
            (ctx.cached_ime_state, ctx.current_region, is_sensitive_region)
        },
    );

    // Query system IME state:
    // 1. If cache is empty (cache miss)
    // 2. If we're in a sensitive region and region hasn't changed (user may have manually toggled IME)
    //    In this case, we need to verify the cache is still valid
    let region_unchanged = current_region == Some(new_region);
    let is_new_region_sensitive = is_sensitive(new_region);
    let should_verify_cache = cached_ime_state.is_some()
        && is_sensitive_region
        && is_new_region_sensitive
        && region_unchanged;

    let current_ime_enabled = if should_verify_cache {
        // Region unchanged in sensitive area: verify cache by querying system
        // This detects manual IME toggles by the user
        let system_state = read_ime_enabled("cursor move (cache verification)");
        // Update cache if it differs from system state
        if cached_ime_state != Some(system_state) {
            log::trace!(
                "IME: Cache mismatch detected (cached={:?}, system={}), updating cache",
                cached_ime_state,
                system_state
            );
            update_ime_cache(doc_id, view_id, editor_mode, system_state);
        }
        system_state
    } else {
        // Cache miss or region changed: use cache if available, otherwise query system
        cached_ime_state.unwrap_or_else(|| read_ime_enabled("cursor move"))
    };

    // Decide what action to take (in lock, fast)
    let target_state = registry::with_context_mut(doc_id, view_id, editor_mode, |ctx| {
        let mut engine = ImeEngine::new(ctx);
        engine.on_region_change(new_region, current_ime_enabled)
    });

    // Execute system API call (outside lock to avoid blocking other operations)
    if let Some(target) = target_state {
        if let Err(e) = set_ime_enabled(target) {
            log::error!("Failed to toggle IME on cursor move: {}", e);
        } else {
            // Update cache after successful state change
            update_ime_cache(doc_id, view_id, editor_mode, target);
        }
    }

    metrics::record_cursor_move_duration(timer.elapsed());
    Ok(())
}

/// Handle mode switch and update IME state accordingly.
///
/// This function saves IME state when exiting Insert mode and restores it
/// when entering Insert mode (if cursor is in a sensitive region).
pub fn handle_mode_switch(
    editor: &mut Editor,
    view_id: ViewId,
    old_mode: Mode,
    new_mode: Mode,
) -> Result<()> {
    if !editor.tree.contains(view_id) {
        return Ok(());
    }

    // No need to prune orphans on every mode switch - it's expensive
    // Orphans are cleaned up on DocumentDidClose and DocumentDidOpen events

    let doc_id = editor.tree.get(view_id).doc;

    if old_mode == Mode::Insert {
        // Always query the actual IME state when exiting Insert mode, don't rely on cache.
        // This is important because the user may have manually enabled IME during Insert mode,
        // and the cache might be stale (e.g., set to false during initialization).
        let current_ime_enabled = read_ime_enabled("mode switch exit");

        // Check if we need to detect the region (if current_region is None)
        let needs_region_detection = registry::with_context_mut(doc_id, view_id, old_mode, |ctx| {
            ctx.current_region.is_none()
        });

        // If current_region is None, detect it before exiting Insert mode
        // This ensures we can correctly decide whether to save the IME state
        if needs_region_detection {
            let doc = editor
                .documents
                .get_mut(&doc_id)
                .ok_or_else(|| anyhow::anyhow!("Document not found"))?;
            doc.ensure_view_init(view_id);

            let cursor_byte_pos = get_cursor_byte_pos(doc, view_id);

            // Get syntax tree and loader
            let text = doc.text().slice(..);
            let syntax = doc.syntax();
            let loader = doc.syntax_loader();

            // Detect IME sensitive region
            let detection = detect_ime_sensitive_region(syntax, text, &*loader, cursor_byte_pos);

            // Update context with detected region
            registry::with_context_mut(doc_id, view_id, old_mode, |ctx| {
                ctx.current_region = Some(detection.region);
            });
        }

        // Decide what action to take (in lock, fast)
        let target_state = registry::with_context_mut(doc_id, view_id, old_mode, |ctx| {
            let mut engine = ImeEngine::new(ctx);
            engine.on_exit_insert(current_ime_enabled)
        });

        // Execute system API call (outside lock to avoid blocking other operations)
        if let Some(target) = target_state {
            if let Err(e) = set_ime_enabled(target) {
                log::error!("Failed to close IME when exiting Insert mode: {}", e);
            } else {
                // Update cache after successful state change
                update_ime_cache(doc_id, view_id, old_mode, target);
            }
        }
    }

    registry::with_context_mut(doc_id, view_id, new_mode, |ctx| ctx.mode = new_mode);

    if new_mode == Mode::Insert {
        // Entering Insert mode: restore IME state if cursor is in sensitive region (FR-002, FR-013, FR-014)
        let doc = editor
            .documents
            .get_mut(&doc_id)
            .ok_or_else(|| anyhow::anyhow!("Document not found"))?;
        doc.ensure_view_init(view_id);

        let cursor_byte_pos = get_cursor_byte_pos(doc, view_id);

        // Update cached cursor position
        registry::with_context_mut(doc_id, view_id, new_mode, |ctx| {
            ctx.cached_cursor_byte_pos = Some(cursor_byte_pos);
        });
        let doc_version = doc.version();

        // Get syntax tree and loader
        let text = doc.text().slice(..);
        let syntax = doc.syntax();
        let loader = doc.syntax_loader();

        // Detect IME sensitive region (always detect on mode switch)
        let detection = detect_ime_sensitive_region(syntax, text, &*loader, cursor_byte_pos);
        registry::with_context_mut(doc_id, view_id, new_mode, |ctx| {
            ctx.cached_region_span = detection.node_range.and_then(|(start, end)| {
                (start < end).then_some(ImeRegionSpan {
                    doc_version,
                    start,
                    end,
                    region: detection.region,
                })
            });
        });

        // Get cached IME state (fast, in lock)
        let cached_ime_state =
            registry::with_context_mut(doc_id, view_id, new_mode, |ctx| ctx.cached_ime_state);

        // Query system if cache miss (outside lock to avoid blocking)
        let current_ime_enabled =
            cached_ime_state.unwrap_or_else(|| read_ime_enabled("mode switch"));

        // Decide what action to take (in lock, fast)
        let target_state = registry::with_context_mut(doc_id, view_id, new_mode, |ctx| {
            let mut engine = ImeEngine::new(ctx);
            engine.on_enter_insert(detection.region, current_ime_enabled)
        });

        // Execute system API call (outside lock to avoid blocking other operations)
        if let Some(target) = target_state {
            if let Err(e) = set_ime_enabled(target) {
                log::error!(
                    "Failed to restore IME state when entering Insert mode: {}",
                    e
                );
            } else {
                // Update cache after successful state change
                update_ime_cache(doc_id, view_id, new_mode, target);
            }
        }
    }

    Ok(())
}

/// Start the background cleanup task for IME contexts.
/// This task runs periodically to clean up stale contexts and prevent memory leaks.
pub fn start_cleanup_task() {
    // Spawn a background task that runs cleanup every 5 minutes
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(300)); // 5 minutes

        loop {
            interval.tick().await;

            // Clean up contexts that haven't been accessed for 30 minutes
            registry::cleanup_old_contexts(Duration::from_secs(1800));

            // Log metrics
            let metrics = registry::get_registry_metrics();
            log::debug!(
                "IME registry metrics: current={}, created={}, removed={}, max={}, cleanups={}",
                metrics.current_contexts(),
                metrics.total_contexts_created(),
                metrics.total_contexts_removed(),
                metrics.max_concurrent_contexts(),
                metrics.cleanup_count()
            );

            // Periodic consistency check every hour
            if metrics.cleanup_count() % 12 == 0 {
                match registry::verify_all_cached_states() {
                    Ok(0) => log::debug!("All IME cached states are consistent"),
                    Ok(count) => log::warn!("Found {} inconsistent IME states", count),
                    Err(e) => log::error!("Failed to verify IME states: {}", e),
                }
            }
        }
    });
}

/// Register IME handler event hooks.
pub fn register_hooks(_handlers: &crate::handlers::Handlers) {
    // Initialize IME support first
    if let Err(e) = initialize_ime_support() {
        log::warn!("IME support initialization failed: {}", e);
    } else {
    // Register hook for view initialization (DocumentDidOpen)
    register_hook!(move |event: &mut DocumentDidOpen<'_>| {
        // Collect view IDs that use this document first to avoid borrow conflicts
        let view_ids: Vec<_> = event
            .editor
            .tree
            .views()
            .filter_map(|(view, _is_focused)| {
                if view.doc == event.doc {
                    Some(view.id)
                } else {
                    None
                }
            })
            .collect();

        // Initialize IME state for all views that use this document
        for view_id in view_ids {
            initialize_view_ime_state(event.editor, view_id);
        }
        Ok(())
    });

    register_hook!(move |event: &mut DocumentDidClose<'_>| {
        // Remove all IME contexts for the closed document
        registry::remove_document(event.doc.id());
        registry::prune_orphans(event.editor);
        Ok(())
    });

    // Register hook for cursor movement (SelectionDidChange)
    register_hook!(move |event: &mut SelectionDidChange<'_>| {
        scheduler::schedule(event.view);
        Ok(())
    });

    // Register hook for mode switch (OnModeSwitch)
    register_hook!(move |event: &mut OnModeSwitch<'_, '_>| {
        // Get current view ID
        let view_id = event.cx.editor.tree.focus;

        // Handle mode switch for IME state update
        if let Err(e) = handle_mode_switch(event.cx.editor, view_id, event.old_mode, event.new_mode)
        {
            // Silently fail and log errors (FR-019)
            log::error!("Failed to handle mode switch for IME: {}", e);
        }
        Ok(())
    });
    }
}

/// Helper to query IME state with comprehensive error logging and retry mechanism.
/// Always returns a boolean fallback to keep editor responsive (FR-019).
fn read_ime_enabled(context: &str) -> bool {
    match read_ime_enabled_with_retry(context) {
        Ok(state) => state,
        Err(e) => {
            log::error!("Failed to read IME state during {} after retries: {}", context, e);
            false // Fallback to disabled to avoid unexpected IME behavior
        }
    }
}

/// Query IME state with retry mechanism and platform-specific settings.
///
/// This function will retry using settings optimized for the detected IME type.
fn read_ime_enabled_with_retry(context: &str) -> Result<bool> {
    // Get optimal settings for current IME if available
    let settings = match platform::get_ime_info() {
        Ok(ime_info) => {
            let ime_type = ImeDetector::detect_ime_type(&ime_info.name);
            ImeDetector::get_optimal_settings(ime_type)
        }
        Err(_) => ImeSettings::default(),
    };

    let mut last_error = None;

    for attempt in 1..=settings.retry_count {
        match is_ime_enabled() {
            Ok(state) => {
                if attempt > 1 {
                    log::debug!("IME state query succeeded on attempt {}", attempt);
                }
                return Ok(state);
            }
            Err(e) => {
                last_error = Some(e);

                // Check if this is a temporary error that should be retried
                if is_temporary_ime_error(last_error.as_ref().unwrap()) && attempt < settings.retry_count {
                    log::debug!(
                        "Temporary IME error during {} (attempt {}/{}), retrying: {}",
                        context, attempt, settings.retry_count, last_error.as_ref().unwrap()
                    );
                    std::thread::sleep(Duration::from_millis(settings.retry_delay_ms));
                    continue;
                }

                // Permanent error or max retries reached
                log::error!(
                    "Failed to read IME state during {} after {} attempts: {}",
                    context, attempt, last_error.as_ref().unwrap()
                );
                break;
            }
        }
    }

    Err(last_error.unwrap())
}

/// Determine if an IME error is temporary and should be retried.
///
/// Temporary errors might include:
/// - System resource temporarily unavailable
/// - IME initialization in progress
/// - Window focus transitions
fn is_temporary_ime_error(error: &anyhow::Error) -> bool {
    let error_str = error.to_string().to_lowercase();

    // Check for common temporary error patterns
    error_str.contains("temporarily unavailable") ||
    error_str.contains("try again") ||
    error_str.contains("in progress") ||
    error_str.contains("busy") ||
    error_str.contains("timeout")
}

/// Verify IME state consistency between cache and system.
///
/// This function checks if the cached IME state matches the actual system state.
/// If there's a mismatch, it updates the cache and returns false.
/// Returns true if states are consistent.
#[allow(dead_code)]
pub fn verify_ime_state_consistency(
    editor: &Editor,
    view_id: ViewId,
) -> Result<bool> {
    if !editor.tree.contains(view_id) {
        return Ok(false);
    }

    let doc_id = editor.tree.get(view_id).doc;
    let mode = editor.mode();

    registry::with_context_mut(doc_id, view_id, mode, |ctx| {
        if let Some(cached_state) = ctx.cached_ime_state {
            // Verify system state
            match read_ime_enabled_with_retry("state verification") {
                Ok(actual_state) => {
                    if cached_state != actual_state {
                        log::warn!(
                            "IME state inconsistency detected (context verification): cached={}, actual={}",
                            cached_state, actual_state
                        );
                        // Don't update cache here, just report inconsistency
                        // The caller can decide whether to update
                        Ok(false)
                    } else {
                        Ok(true)
                    }
                }
                Err(e) => {
                    log::error!("Failed to verify IME state consistency: {}", e);
                    // Assume inconsistent if we can't verify
                    Ok(false)
                }
            }
        } else {
            // No cached state, so not inconsistent
            Ok(true)
        }
    })
}
