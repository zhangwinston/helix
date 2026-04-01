//! Simple test to verify IME improvements work correctly

use super::helpers::{test_config, test_syntax_loader};
use helix_term::{
    application::Application,
    args::Args,
    handlers::ime::{metrics, registry},
};
use helix_view::{document::Mode, editor::Action};
use std::time::Duration;

#[tokio::test(flavor = "multi_thread")]
async fn test_improvements_simple() -> anyhow::Result<()> {
    let mut app = Application::new(Args::default(), test_config(), test_syntax_loader(None))?;

    // Get initial view
    let view1_id = app.editor.tree.focus;

    // Create additional views to test cleanup
    app.editor.switch(app.editor.tree.get(view1_id).doc, Action::VerticalSplit);
    let view2_id = app.editor.tree.focus;
    app.editor.switch(app.editor.tree.get(view2_id).doc, Action::HorizontalSplit);
    let view3_id = app.editor.tree.focus;

    // Enable IME tracking for views
    app.editor.mode = Mode::Insert;
    helix_term::handlers::ime::handle_cursor_move(&mut app.editor, view1_id)?;
    helix_term::handlers::ime::handle_cursor_move(&mut app.editor, view2_id)?;
    helix_term::handlers::ime::handle_cursor_move(&mut app.editor, view3_id)?;

    // Check metrics
    let before_metrics = registry::get_registry_metrics();
    assert!(before_metrics.current_contexts() >= 3,
        "Should have created contexts for all views");

    // Close one view
    app.editor.close_view(view3_id, true);

    // Run cleanup
    registry::prune_orphans(&app.editor);

    // Verify cleanup worked
    let after_metrics = registry::get_registry_metrics();
    assert!(after_metrics.current_contexts() < before_metrics.current_contexts(),
        "Cleanup should have removed context for closed view");

    // Test manual cleanup
    registry::cleanup_old_contexts(Duration::from_millis(1));

    // Final metrics check
    let final_metrics = registry::get_registry_metrics();
    assert!(final_metrics.total_contexts_created() >= 3,
        "Should track total contexts created");

    println!("✓ All IME improvements verified successfully");
    println!("  Final metrics:");
    println!("    Total created: {}", final_metrics.total_contexts_created());
    println!("    Total removed: {}", final_metrics.total_contexts_removed());
    println!("    Current contexts: {}", final_metrics.current_contexts());
    println!("    Max concurrent: {}", final_metrics.max_concurrent_contexts());
    println!("    Cleanup count: {}", final_metrics.cleanup_count());

    Ok(())
}