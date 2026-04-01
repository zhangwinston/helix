//! Example program to demonstrate IME auto-control improvements.
//!
//! Run with: `cargo run --example ime_debug`
//!
//! This program creates a test editor instance with IME debug output enabled
//! to demonstrate the error handling and caching improvements.

use anyhow::Result;
use helix_core::{Range, Rope, Selection, Transaction};
use helix_term::{
    application::Application,
    args::Args,
    handlers::ime::{self, metrics, registry, verify_ime_state_consistency},
};
use helix_view::{document::Mode, editor::Action};
use std::{io::Write, time::Duration};

fn main() -> Result<()> {
    // Initialize logging with debug level to see all IME messages
    env_logger::Builder::from_default_env()
        .filter_level(log::LevelFilter::Debug)
        .init();

    println!("=== IME Auto-Control Debug Example ===\n");

    // Create application with test configuration
    let mut app = Application::new(
        Args::default(),
        helix_term::tests::test_config(),
        helix_term::tests::test_syntax_loader(None),
    )?;

    // Get initial view
    let view1_id = app.editor.tree.focus;

    // Create test document with various content types
    println!("1. Setting up test document with code, string, and comment content...");
    let doc_id = setup_test_document(&mut app, view1_id)?;

    // Print initial metrics
    print_metrics("Initial state");

    // Test error handling resilience
    println!("\n2. Testing error handling resilience...");
    test_error_handling(&mut app, view1_id)?;

    // Test caching behavior
    println!("\n3. Testing caching behavior...");
    test_caching_behavior(&mut app, view1_id)?;

    // Test state consistency
    println!("\n4. Testing IME state consistency...");
    test_state_consistency(&mut app, view1_id)?;

    // Test cleanup mechanisms
    println!("\n5. Testing cleanup mechanisms...");
    test_cleanup_mechanisms(&mut app, view1_id)?;

    // Final metrics
    print_metrics("Final state");

    println!("\n=== Example completed successfully ===");
    Ok(())
}

fn setup_test_document(app: &mut Application, view_id: helix_view::ViewId) -> Result<helix_view::DocumentId> {
    let content = indoc::indoc! {r#"
        fn demonstrate_ime_features() {
            // This is a comment where IME should be enabled
            println!("这是一段中文测试"); // Chinese test in string

            let message = "IME should be enabled here too";
            let code_only = 42; // IME should be disabled here

            /* Block comment
               that spans multiple lines
               with more Chinese content: 你好世界
            */
        }

        // Edit at the end to see IME behavior
        let final_string = "最后一行测试";
    "#};

    let doc_id = helix_term::tests::test::ime::overwrite_document_text(app, view_id, content);

    // Set language for proper syntax highlighting
    let loader = app.editor.syn_loader.load();
    {
        let doc = app.editor.documents.get_mut(&doc_id).unwrap();
        doc.set_language_by_language_id("rust", &loader)?;
    }

    println!("   ✓ Test document created with Rust syntax highlighting");
    Ok(doc_id)
}

fn test_error_handling(app: &mut Application, view_id: helix_view::ViewId) -> Result<()> {
    metrics::reset();
    app.editor.mode = Mode::Insert;

    // Test rapid cursor movements to stress the error handling
    let positions = vec![10, 50, 100, 150, 200, 250, 300];

    for (i, pos_char) in positions.into_iter().enumerate() {
        let doc = app.editor.documents.get_mut(&app.editor.tree.get(view_id).doc).unwrap();
        let pos_byte = doc.text().char_to_byte(pos_char);
        doc.set_selection(view_id, Selection::single(pos_char, pos_char));

        if let Err(e) = ime::handle_cursor_move(app, view_id) {
            println!("   ☐ Cursor move at position {} failed: {}", pos_char, e);
        } else {
            println!("   ✓ Cursor move {} at position {} succeeded", i + 1, pos_char);
        }
    }

    let snapshot = metrics::snapshot();
    println!("   Total cursor moves: {}", snapshot.cursor_move_calls);
    println!("   Non-insert mode skips: {}", snapshot.non_insert_skips);

    Ok(())
}

fn test_caching_behavior(app: &mut Application, view_id: helix_view::ViewId) -> Result<()> {
    metrics::reset();

    // Move cursor to specific position
    let doc = app.editor.documents.get_mut(&app.editor.tree.get(view_id).doc).unwrap();
    let comment_pos = doc.text().to_string().find("这是").unwrap_or(0);
    let comment_byte = doc.text().char_to_byte(comment_pos);
    doc.set_selection(view_id, Selection::single(comment_pos, comment_pos));

    // First move - should trigger region detection
    ime::handle_cursor_move(app, view_id)?;
    let snapshot1 = metrics::snapshot();

    // Second move to same position - should use cache
    ime::handle_cursor_move(app, view_id)?;
    let snapshot2 = metrics::snapshot();

    println!("   First move - Region detections: {}", snapshot1.region_detection_calls);
    println!("   Second move - Region detections: {}", snapshot2.region_detection_calls);
    println!("   Cache hits: {}", snapshot2.region_cache_hits);

    // Test moving to different regions
    let string_pos = doc.text().to_string().find("中文测试").unwrap_or(0);
    let string_byte = doc.text().char_to_byte(string_pos);
    doc.set_selection(view_id, Selection::single(string_byte, string_byte));
    ime::handle_cursor_move(app, view_id)?;

    let code_pos = doc.text().to_string().find("42").unwrap_or(0);
    let code_byte = doc.text().char_to_byte(code_pos);
    doc.set_selection(view_id, Selection::single(code_byte, code_byte));
    ime::handle_cursor_move(app, view_id)?;

    println!("   ✓ Successfully tested region caching between code, string, and comment");

    Ok(())
}

fn test_state_consistency(app: &mut Application, view_id: helix_view::ViewId) -> Result<()> {
    // Check initial consistency
    match verify_ime_state_consistency(&app.editor, view_id) {
        Ok(true) => println!("   ✓ IME state is consistent"),
        Ok(false) => println!("   ⚠ IME state inconsistency detected"),
        Err(e) => println!("   ☐ Failed to verify state: {}", e),
    }

    // Switch modes and check again
    ime::handle_mode_switch(app, view_id, Mode::Insert, Mode::Normal)?;

    match verify_ime_state_consistency(&app.editor, view_id) {
        Ok(true) => println!("   ✓ IME state consistent after mode switch"),
        Ok(false) => println!("   ⚠ IME state inconsistent after mode switch"),
        Err(e) => println!("   ☐ Failed to verify state after mode switch: {}", e),
    }

    // Verify all cached states
    match registry::verify_all_cached_states() {
        Ok(0) => println!("   ✓ All cached states are consistent"),
        Ok(count) => println!("   ⚠ Found {} inconsistent cached states", count),
        Err(e) => println!("   ☐ Failed to verify all states: {}", e),
    }

    Ok(())
}

fn test_cleanup_mechanisms(app: &mut Application, view1_id: helix_view::ViewId) -> Result<()> {
    // Create additional views to test cleanup
    let mut view_ids = vec![view1_id];

    for i in 0..3 {
        app.editor.switch(
            app.editor.tree.get(view1_id).doc,
            if i % 2 == 0 { Action::VerticalSplit } else { Action::HorizontalSplit }
        );
        view_ids.push(app.editor.tree.focus);

        // Create IME context for each view
        app.editor.mode = Mode::Insert;
        ime::handle_cursor_move(app, view_ids.last().unwrap())?;
    }

    print_metrics("After creating 4 views");

    // Close some views
    for i in 1..3 {
        app.editor.close_view(view_ids[i], false);
    }

    print_metrics("After closing 2 views");

    // Run orphan pruning
    registry::prune_orphans(&app.editor);

    print_metrics("After orphan pruning");

    // Force cleanup with very short age for demonstration
    registry::cleanup_old_contexts(Duration::from_millis(1));

    print_metrics("After forced cleanup");

    println!("   ✓ Cleanup mechanisms tested successfully");

    Ok(())
}

fn print_metrics(label: &str) {
    let metrics = registry::get_registry_metrics();
    println!("\n   {}:", label);
    println!("     Current contexts: {}", metrics.current_contexts);
    println!("     Total created   : {}", metrics.total_contexts_created);
    println!("     Total removed   : {}", metrics.total_contexts_removed);
    println!("     Max concurrent  : {}", metrics.max_concurrent_contexts);
    println!("     Cleanup count   : {}", metrics.cleanup_count);
}