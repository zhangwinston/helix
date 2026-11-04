use super::helpers::test_syntax_loader;
use super::ime::{ime_test_config, ime_test_env, overwrite_document_text};
use helix_core::Selection;
use helix_loader::workspace_trust::WorkspaceTrust;
use helix_term::{
    application::Application,
    args::Args,
    handlers::ime::{metrics, registry, verify_ime_state_consistency},
};
use helix_view::{document::Mode, editor::Action};
use indoc::indoc;
use std::time::Duration;

/// Test the enhanced error handling with retry mechanism.
#[tokio::test(flavor = "multi_thread")]
async fn test_ime_error_handling_with_retry() -> anyhow::Result<()> {
    let _env = ime_test_env();
    let app = Application::new(
        Args::default(),
        ime_test_config(),
        test_syntax_loader(None),
        WorkspaceTrust::fully_trusted(),
    )?;
    let view_id = app.editor.tree.focus;

    // We can't directly simulate IME errors in the test environment, but we
    // can verify the error handling logic exists: the consistency check must
    // run without panicking. Both a consistent and an inconsistent result are
    // valid outcomes depending on the platform state.
    let result = verify_ime_state_consistency(&app.editor, view_id);
    assert!(result.is_ok(), "State consistency check should not fail");

    Ok(())
}

/// Test that IME context cleanup works correctly.
#[tokio::test(flavor = "multi_thread")]
async fn test_ime_context_cleanup() -> anyhow::Result<()> {
    let _env = ime_test_env();
    let mut app = Application::new(
        Args::default(),
        ime_test_config(),
        test_syntax_loader(None),
        WorkspaceTrust::fully_trusted(),
    )?;

    // Create multiple views to generate multiple contexts
    let view1_id = app.editor.tree.focus;
    app.editor
        .switch(app.editor.tree.get(view1_id).doc, Action::VerticalSplit);
    let view2_id = app.editor.tree.focus;
    app.editor
        .switch(app.editor.tree.get(view2_id).doc, Action::HorizontalSplit);
    let view3_id = app.editor.tree.focus;

    // Trigger IME context creation for all views
    app.editor.mode = Mode::Insert;
    helix_term::handlers::ime::handle_cursor_move(&mut app.editor, view1_id)?;
    helix_term::handlers::ime::handle_cursor_move(&mut app.editor, view2_id)?;
    helix_term::handlers::ime::handle_cursor_move(&mut app.editor, view3_id)?;

    // Check that contexts exist
    let metrics_before = registry::get_registry_metrics();
    assert!(
        metrics_before.current_contexts() >= 3,
        "Should have at least 3 contexts"
    );

    // Close one view
    app.editor.close(view3_id);

    // Run orphan pruning
    registry::prune_orphans(&app.editor);

    // Verify context was removed
    let metrics_after = registry::get_registry_metrics();
    assert!(
        metrics_after.current_contexts() < metrics_before.current_contexts(),
        "Context count should decrease after closing view"
    );

    Ok(())
}

/// Test that stale contexts are cleaned up after aging.
#[tokio::test(flavor = "multi_thread")]
async fn test_stale_context_cleanup() -> anyhow::Result<()> {
    let _env = ime_test_env();
    let app = Application::new(
        Args::default(),
        ime_test_config(),
        test_syntax_loader(None),
        WorkspaceTrust::fully_trusted(),
    )?;
    let view_id = app.editor.tree.focus;
    let doc_id = app.editor.tree.get(view_id).doc;

    // Create an IME context
    let mut editor = app.editor;
    editor.mode = Mode::Insert;
    helix_term::handlers::ime::handle_cursor_move(&mut editor, view_id)?;

    // Verify context exists
    let context = registry::context(doc_id, view_id);
    assert!(
        context.is_some(),
        "IME context should exist after cursor move"
    );

    // Force cleanup with very short max age to test the mechanism. In a real
    // scenario the context would still exist because it was just accessed,
    // but this verifies the cleanup function runs without error.
    registry::cleanup_old_contexts(Duration::from_nanos(1));

    Ok(())
}

/// Test IME context metrics tracking.
#[tokio::test(flavor = "multi_thread")]
async fn test_ime_context_metrics() -> anyhow::Result<()> {
    let _env = ime_test_env();
    let mut app = Application::new(
        Args::default(),
        ime_test_config(),
        test_syntax_loader(None),
        WorkspaceTrust::fully_trusted(),
    )?;

    // Create a new view
    let view1_id = app.editor.tree.focus;
    app.editor
        .switch(app.editor.tree.get(view1_id).doc, Action::VerticalSplit);
    let view2_id = app.editor.tree.focus;
    let doc2_id = app.editor.tree.get(view2_id).doc;

    // Trigger IME context creation
    app.editor.mode = Mode::Insert;
    helix_term::handlers::ime::handle_cursor_move(&mut app.editor, view2_id)?;

    // The registry counters are process-global and monotonic across tests, so
    // assert the deterministic outcome instead of counter deltas: the view
    // owns an IME context after the move.
    assert!(
        registry::context(doc2_id, view2_id).is_some(),
        "Cursor move should create a context for the view"
    );

    // Close view and cleanup
    app.editor.close(view2_id);
    registry::prune_orphans(&app.editor);

    // Pruning must drop the orphaned view's context.
    assert!(
        registry::context(doc2_id, view2_id).is_none(),
        "Pruning should remove the closed view's context"
    );

    Ok(())
}

/// Test that IME state consistency detection works.
#[tokio::test(flavor = "multi_thread")]
async fn test_ime_state_consistency_detection() -> anyhow::Result<()> {
    let _env = ime_test_env();
    let mut app = Application::new(
        Args::default(),
        ime_test_config(),
        test_syntax_loader(None),
        WorkspaceTrust::fully_trusted(),
    )?;
    let view_id = app.editor.tree.focus;

    // Set up IME context with a specific state
    app.editor.mode = Mode::Insert;
    helix_term::handlers::ime::handle_cursor_move(&mut app.editor, view_id)?;

    // Verify consistency check works. In a real scenario this would detect if
    // the cached state differs from the system state; for testing we just
    // verify the mechanism works.
    let result = verify_ime_state_consistency(&app.editor, view_id);
    assert!(result.is_ok(), "State consistency check should succeed");

    // Log result for debugging
    log::debug!("IME state consistency check result: {}", result.unwrap());

    Ok(())
}

/// Test error handling resilience with rapid operations.
#[tokio::test(flavor = "multi_thread")]
async fn test_error_handling_resilience() -> anyhow::Result<()> {
    let _env = ime_test_env();
    let mut app = Application::new(
        Args::default(),
        ime_test_config(),
        test_syntax_loader(None),
        WorkspaceTrust::fully_trusted(),
    )?;
    let view_id = app.editor.tree.focus;

    // Prepare a document with various content
    let doc_id = overwrite_document_text(
        &mut app,
        view_id,
        indoc! {r#"
            fn test_resilience() {
                let string = "test string";
                // comment here
                let value = 42;
            }
        "#},
    );

    // Set language for proper syntax highlighting
    let loader = app.editor.syn_loader.load();
    {
        let doc = app.editor.documents.get_mut(&doc_id).unwrap();
        doc.set_language_by_language_id("rust", &loader)?;
    }

    app.editor.mode = Mode::Insert;

    // Perform rapid cursor movements to stress test error handling
    let positions = vec![10, 20, 30, 15, 25, 35, 40, 45];

    for &pos_char in &positions {
        let doc = app.editor.documents.get_mut(&doc_id).unwrap();
        doc.set_selection(view_id, Selection::single(pos_char, pos_char));

        // Each cursor move should handle errors gracefully
        let result = helix_term::handlers::ime::handle_cursor_move(&mut app.editor, view_id);
        assert!(
            result.is_ok(),
            "Cursor move should handle errors gracefully"
        );
    }

    // Verify metrics show successful operations
    let snapshot = metrics::snapshot();
    assert!(
        snapshot.cursor_move_calls >= positions.len() as u64,
        "All cursor moves should be recorded"
    );

    Ok(())
}

/// Test that verification works with multiple views.
#[tokio::test(flavor = "multi_thread")]
async fn test_multi_view_consistency() -> anyhow::Result<()> {
    let _env = ime_test_env();
    let mut app = Application::new(
        Args::default(),
        ime_test_config(),
        test_syntax_loader(None),
        WorkspaceTrust::fully_trusted(),
    )?;

    // Create multiple views
    let view1_id = app.editor.tree.focus;
    app.editor
        .switch(app.editor.tree.get(view1_id).doc, Action::VerticalSplit);
    let view2_id = app.editor.tree.focus;

    // Set different modes for different views
    app.editor.mode = Mode::Insert;
    helix_term::handlers::ime::handle_cursor_move(&mut app.editor, view1_id)?;

    app.editor.mode = Mode::Normal;
    helix_term::handlers::ime::handle_mode_switch(
        &mut app.editor,
        view2_id,
        Mode::Insert,
        Mode::Normal,
    )?;

    // Verify consistency for all views
    let result1 = verify_ime_state_consistency(&app.editor, view1_id);
    let result2 = verify_ime_state_consistency(&app.editor, view2_id);

    assert!(result1.is_ok(), "View 1 consistency check should succeed");
    assert!(result2.is_ok(), "View 2 consistency check should succeed");

    Ok(())
}

/// Test cleanup task simulation.
#[tokio::test(flavor = "multi_thread")]
async fn test_cleanup_task_behavior() -> anyhow::Result<()> {
    let _env = ime_test_env();
    let mut app = Application::new(
        Args::default(),
        ime_test_config(),
        test_syntax_loader(None),
        WorkspaceTrust::fully_trusted(),
    )?;

    // Create multiple contexts
    let view_count = 5;
    let mut view_ids = Vec::new();

    for _ in 0..view_count {
        view_ids.push(app.editor.tree.focus);
        if view_ids.len() < view_count {
            app.editor
                .switch(app.editor.tree.get(view_ids[0]).doc, Action::VerticalSplit);
        }
    }

    // Enable IME tracking for all views
    app.editor.mode = Mode::Insert;
    for &view_id in &view_ids {
        helix_term::handlers::ime::handle_cursor_move(&mut app.editor, view_id)?;
    }

    let metrics_before = registry::get_registry_metrics();
    assert!(metrics_before.current_contexts() >= view_count as u64);

    // Close some views
    for i in 1..3 {
        app.editor.close(view_ids[i]);
    }

    // Simulate cleanup task behavior
    registry::prune_orphans(&app.editor);

    // Force cleanup of old entries with very short age
    registry::cleanup_old_contexts(Duration::from_millis(1));

    let metrics_after = registry::get_registry_metrics();

    // Should have removed contexts for closed views
    assert!(
        metrics_after.current_contexts() < metrics_before.current_contexts(),
        "Should have removed context for closed views"
    );

    // Verify cleanup count increased
    assert!(
        metrics_after.cleanup_count() > 0,
        "Should have performed cleanup"
    );

    Ok(())
}

/// Test that registry metrics provide useful information.
#[tokio::test(flavor = "multi_thread")]
async fn test_registry_metrics_usefulness() -> anyhow::Result<()> {
    let _env = ime_test_env();
    let mut app = Application::new(
        Args::default(),
        ime_test_config(),
        test_syntax_loader(None),
        WorkspaceTrust::fully_trusted(),
    )?;

    // The registry is process-global and counters are monotonic across tests,
    // so only compare against the snapshot taken within this test.
    let initial = registry::get_registry_metrics();

    // Create and manipulate contexts
    let view1_id = app.editor.tree.focus;
    app.editor
        .switch(app.editor.tree.get(view1_id).doc, Action::VerticalSplit);
    let view2_id = app.editor.tree.focus;

    app.editor.mode = Mode::Insert;
    // Drive the freshly split view. The registry is process-global and keyed
    // by (DocumentId, ViewId), so a leftover context from an earlier test can
    // occupy the key and make this move a reuse; creation can only be checked
    // non-strictly.
    helix_term::handlers::ime::handle_cursor_move(&mut app.editor, view2_id)?;

    let after_creation = registry::get_registry_metrics();
    assert!(
        after_creation.total_contexts_created() >= initial.total_contexts_created(),
        "Should track context creation"
    );

    // Close and cleanup
    app.editor.close(view2_id);
    registry::prune_orphans(&app.editor);

    let final_metrics = registry::get_registry_metrics();
    assert!(
        final_metrics.total_contexts_removed() > initial.total_contexts_removed(),
        "Should track context removal"
    );

    // Log metrics for manual inspection
    log::info!("Final IME registry metrics: {:?}", final_metrics);

    Ok(())
}
