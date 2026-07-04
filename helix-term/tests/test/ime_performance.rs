//! Performance tests for IME auto-control functionality

use super::helpers::{test_config, test_syntax_loader};
use helix_core::{Range, Rope, Selection, Transaction};
use helix_term::{
    application::Application,
    args::Args,
    handlers::ime::{self, metrics, registry},
};
use helix_view::{document::Mode, editor::Action};
use std::time::Duration;
use indoc::indoc;

#[tokio::test(flavor = "multi_thread")]
async fn test_performance_cursor_moves() -> anyhow::Result<()> {
    let mut app = Application::new(Args::default(), test_config(), test_syntax_loader(None))?;
    let view_id = app.editor.tree.focus;

    // Create a large document with various content
    let doc_id = helix_term::tests::test::ime::overwrite_document_text(
        &mut app,
        view_id,
        indoc! {r##"
            fn performance_test() {
                for i in 0..1000 {
                    let msg = "测试消息";
                    println!("处理第 {} 条记录: {}", i, msg);
                    // More code here
                    let value = i * 2;
                    if value > 100 {
                        break;
                    }
                    // Comment section for testing
                    /* Multi-line comment
                       with Chinese content: 你好世界
                       More text here
                    */
                }

                // Function with string
                let greeting = "Hello, 世界!";
                let pattern = r#"
                    Raw string literal
                    with content: 内容测试
                "#;

                let array = [
                    "元素一",
                    "元素二",
                    "元素三",
                ];

                return Ok(());
            }
        "##},
    );

    // Set language for proper syntax highlighting
    let loader = app.editor.syn_loader.load();
    {
        let doc = app.editor.documents.get_mut(&doc_id).unwrap();
        doc.set_language_by_language_id("rust", &loader)?;
    }

    app.editor.mode = Mode::Insert;
    metrics::reset();

    // Perform rapid cursor movements to stress test performance
    let positions = vec![
        100,   // Start
        500,   // String
        1500,  // Code
        2500,  // Comment
        3500,  // String
        4500,  // Code
        5500,  // Process loop
        6500,  // Inside loop
        7500,  // Array
        8500,  // Function end
    ];

    let start_time = std::time::Instant::now();

    for &pos in &positions {
        let doc = app.editor.documents.get_mut(&doc_id).unwrap();
        let pos_byte = doc.text().char_to_byte(pos);
        doc.set_selection(view_id, Selection::single(pos, pos));

        // This should use cached results when possible
        helix_term::handlers::ime::handle_cursor_move(&mut app.editor, view_id)?;
    }

    let elapsed = start_time.elapsed();
    let snapshot = metrics::snapshot();

    println!("Performance test results:");
    println!("  Total cursor moves: {}", positions.len());
    println!("  Total time: {:?}", elapsed);
    println!("  Average time per move: {:?}", elapsed / positions.len() as u32);
    println!("  Operations per second: {:.2}", positions.len() as f64 / elapsed.as_secs_f64());

    // Verify performance metrics
    assert!(
        elapsed < Duration::from_secs(1),
        "Performance test exceeded 1 second threshold"
    );

    assert_eq!(
        snapshot.cursor_move_calls,
        positions.len() as u32,
        "All cursor moves should be recorded"
    );

    // Cache should have improved performance
    if snapshot.region_detection_calls < positions.len() as u32 {
        println!("  Cache effectiveness: {} cached vs {} total",
            snapshot.region_detection_calls, positions.len());
    }

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_memory_usage_with_multiple_views() -> anyhow::Result<()> {
    let mut app = Application::new(Args::default(), test_config(), test_syntax_loader(None))?;

    // Create multiple views to test memory management
    let mut view_ids = Vec::new();
    for i in 0..10 {
        app.editor.switch(app.editor.tree.get(view_ids.last().unwrap_or(&app.editor.tree.focus).doc, Action::VerticalSplit);
        let view_id = app.editor.tree.focus;
        view_ids.push(view_id);

        // Trigger IME context creation
        app.editor.mode = Mode::Insert;
        helix_term::handlers::ime::handle_cursor_move(&mut app.editor, view_id)?;
    }

    // Get memory usage metrics
    let metrics = registry::get_registry_metrics();
    println!("Memory usage with 10 views:");
    println!("  Current contexts: {}", metrics.current_contexts());
    println!("  Total created: {}", metrics.total_contexts_created());

    // Memory usage should be reasonable
    assert!(metrics.current_contexts() <= 20, "Too many active IME contexts");

    // Close half of the views
    for i in (0..5).map(|n| n * 2) {
        if view_ids.len() > i {
            app.editor.close_view(view_ids[i], false);
        }
    }

    // Run cleanup
    registry::prune_orphans(&app.editor);

    // Verify cleanup was effective
    let after_cleanup = registry::get_registry_metrics();
    assert!(
        after_cleanup.current_contexts() < metrics.current_contexts(),
        "Cleanup should have reduced active contexts"
    );

    println!("  After cleanup: {}", after_cleanup.current_contexts());

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_large_file_performance() -> anyhow::Result<()> {
    let mut app = Application::new(Args::default(), test_config(), test_syntax_loader(None))?;
    let view_id = app.editor.tree.focus;

    // Create a very large file
    let large_content = std::iter::repeat("let str = \"测试内容\"; // String content\n")
        .take(5000)
        .collect::<String>()
        + "\n\nfn main() {\n    println!(\"Large file test\");\n}";

    let doc_id = helix_term::tests::test::ime::overwrite_document_text(
        &mut app,
        view_id,
        large_content,
    );

    // Set language
    let loader = app.editor.syn_loader.load();
    {
        let doc = app.editor.documents.get_mut(&doc_id).unwrap();
        doc.set_language_by_language_id("rust", &loader)?;
    }

    app.editor.mode = Mode::Insert;
    metrics::reset();

    // Test cursor movement in large file
    let test_positions = vec![100, 1000, 5000, 10000, 20000, 50000];

    let start_time = std::time::Instant::now();

    for &pos in &test_positions {
        let doc = app.editor.documents.get_mut(&doc_id).unwrap();
        let pos_byte = doc.text().char_to_byte(pos);
        doc.set_selection(view_id, Selection::single(pos, pos));

        helix_term::handlers::ime::handle_cursor_move(&mut app.editor, view_id)?;
    }

    let elapsed = start_time.elapsed();
    println!("Large file performance test:");
    println!("  File size: {} bytes", large_content.len());
    println!("  Test moves: {}", test_positions.len());
    println!("  Total time: {:?}", elapsed);
    println!("  Time per move: {:?}", elapsed / test_positions.len() as u32);

    // Performance should remain good even with large files
    assert!(
        elapsed < Duration::from_millis(500),
        "Large file test exceeded 500ms threshold"
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_cache_effectiveness() -> anyhow::Result<()> {
    let mut app = Application::new(Args::default(), test_config(), test_syntax_loader(None))?;
    let view_id = app.editor.tree.focus;

    // Create document with mixed regions
    let doc_id = helix_term::tests::test::ime::overwrite_document_text(
        &mut app,
        view_id,
        indoc! {r#"
            fn cache_test() {
                // Code region 1
                let code1 = 42;

                // String region
                let string = "中文测试";

                // Code region 2
                let code2 = 100;

                // Comment
                /* Multi-line comment
                   with Chinese: 测试内容
                */

                // Back to code
                let end = true;
            }
        "#},
    );

    // Set language
    let loader = app.editor.syn_loader.load();
    {
        let doc = app.editor.documents.get_mut(&doc_id).unwrap();
        doc.set_language_by_language_id("rust", &loader)?;
    }

    app.editor.mode = Mode::Insert;
    metrics::reset();

    // Define test positions for each region type
    let test_cases = vec![
        (25, "code"),     // "let code1 = 42"
        (45, "string"),   // "let string = \"中文测试\""
        (65, "code"),     // "let code2 = 100"
        (85, "comment"),  // comment
        (120, "code"),    // "let end = true"
    ];

    // First pass - should trigger cache misses
    for &(pos, region) in &test_cases {
        let doc = app.editor.documents.get_mut(&doc_id).unwrap();
        let pos_byte = doc.text().char_to_byte(pos);
        doc.set_selection(view_id, Selection::single(pos, pos));
        helix_term::handlers::ime::handle_cursor_move(&mut app.editor, view_id)?;
    }

    let after_first = metrics::snapshot();

    // Second pass - should use cached results for repeated positions
    for &(pos, _region) in &test_cases {
        let doc = app.editor.documents.get_mut(&doc_id).unwrap();
        let pos_byte = doc.text().char_to_byte(pos);
        doc.set_selection(view_id, Selection::single(pos, pos));
        helix_term::handlers::ime::handle_cursor_move(&mut app.editor, view_id)?;
    }

    let after_second = metrics::snapshot();

    println!("Cache effectiveness test:");
    println!("  First pass - Region detections: {}", after_first.region_detection_calls);
    println!("  Second pass - Region detections: {}", after_second.region_detection_calls);

    // Second pass should have significantly fewer detection calls
    assert!(
        after_second.region_detection_calls < after_first.region_detection_calls,
        "Cache should reduce detection calls on second pass"
    );

    // Calculate cache hit rate
    let cache_hits = after_first.region_cache_hits + after_second.region_cache_hits;
    let total_ops = cache_hits + (after_first.region_detection_calls + after_second.region_detection_calls);
    let hit_rate = (cache_hits as f64 / total_ops as f64) * 100.0;

    println!("  Cache hit rate: {:.2}%", hit_rate);
    assert!(hit_rate > 50.0, "Cache hit rate should be greater than 50%");

    Ok(())
}