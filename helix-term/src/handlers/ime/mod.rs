//! IME (Input Method Editor) automatic control handler.
//!
//! This module provides automatic IME state management based on cursor position
//! and editor mode. IME is automatically enabled in string/comment regions and
//! disabled in code regions.

pub mod platform;

pub mod engine;
pub mod metrics;
#[cfg(any(test, feature = "integration"))]
pub mod registry;
#[cfg(not(any(test, feature = "integration")))]
mod registry;
mod scheduler;

use anyhow::Result;
use helix_core::syntax::{detect_ime_sensitive_region, ImeSensitiveRegion};
use helix_event::register_hook;
use helix_view::{
    document::Mode,
    events::{DocumentDidClose, DocumentDidOpen, SelectionDidChange},
    Editor, ViewId,
};

use crate::events::OnModeSwitch;

use crate::handlers::ime::platform::{is_ime_enabled, set_ime_enabled};
use engine::{is_sensitive, ImeEngine, ImeRegionSpan};
use helix_view::document::Document;
use std::time::Instant;

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

    let new_region = if !needs_detection {
        metrics::record_region_cache_hit();
        registry::with_context_mut(doc_id, view_id, editor_mode, |ctx| {
            ctx.current_region.unwrap_or(ImeSensitiveRegion::Code)
        })
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

/// Register IME handler event hooks.
pub fn register_hooks(_handlers: &crate::handlers::Handlers) {
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

/// Helper to query IME state with comprehensive error logging.
/// Always returns a boolean fallback to keep editor responsive (FR-019).
fn read_ime_enabled(context: &str) -> bool {
    match is_ime_enabled() {
        Ok(state) => state,
        Err(e) => {
            log::error!("Failed to read IME state during {}: {}", context, e);
            false
        }
    }
}
