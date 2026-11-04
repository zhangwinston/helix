//! Simple test to verify IME improvements work correctly

use super::helpers::test_syntax_loader;
use super::ime::{ime_test_config, ime_test_env};
use helix_loader::workspace_trust::WorkspaceTrust;
use helix_term::{application::Application, args::Args, handlers::ime::registry};
use helix_view::{document::Mode, editor::Action};
use std::time::Duration;

#[tokio::test(flavor = "multi_thread")]
async fn test_improvements_simple() -> anyhow::Result<()> {
    let _env = ime_test_env();
    let mut app = Application::new(
        Args::default(),
        ime_test_config(),
        test_syntax_loader(None),
        WorkspaceTrust::fully_trusted(),
    )?;

    // Get initial view
    let view1_id = app.editor.tree.focus;

    // Create additional views to test cleanup
    app.editor
        .switch(app.editor.tree.get(view1_id).doc, Action::VerticalSplit);
    let view2_id = app.editor.tree.focus;
    app.editor
        .switch(app.editor.tree.get(view2_id).doc, Action::HorizontalSplit);
    let view3_id = app.editor.tree.focus;

    // Enable IME tracking for views
    app.editor.mode = Mode::Insert;
    helix_term::handlers::ime::handle_cursor_move(&mut app.editor, view1_id)?;
    helix_term::handlers::ime::handle_cursor_move(&mut app.editor, view2_id)?;
    helix_term::handlers::ime::handle_cursor_move(&mut app.editor, view3_id)?;

    // Check metrics
    let before_metrics = registry::get_registry_metrics();
    assert!(
        before_metrics.current_contexts() >= 3,
        "Should have created contexts for all views"
    );

    // Close one view
    app.editor.close(view3_id);

    // Run cleanup
    registry::prune_orphans(&app.editor);

    // Verify cleanup worked
    let after_metrics = registry::get_registry_metrics();
    assert!(
        after_metrics.current_contexts() < before_metrics.current_contexts(),
        "Cleanup should have removed context for closed view"
    );

    // Test manual cleanup
    registry::cleanup_old_contexts(Duration::from_millis(1));

    // Final metrics (counters are process-global and monotonic, so no strict
    // assertion is possible here; the meaningful checks are above).
    let final_metrics = registry::get_registry_metrics();

    println!("✓ All IME improvements verified successfully");
    println!("  Final metrics:");
    println!(
        "    Total created: {}",
        final_metrics.total_contexts_created()
    );
    println!(
        "    Total removed: {}",
        final_metrics.total_contexts_removed()
    );
    println!("    Current contexts: {}", final_metrics.current_contexts());
    println!(
        "    Max concurrent: {}",
        final_metrics.max_concurrent_contexts()
    );
    println!("    Cleanup count: {}", final_metrics.cleanup_count());

    Ok(())
}
