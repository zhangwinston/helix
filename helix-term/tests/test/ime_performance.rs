//! Performance tests for IME auto-control functionality

use super::helpers::test_syntax_loader;
use super::ime::{ime_test_config, ime_test_env, overwrite_document_text};
use helix_core::Selection;
use helix_loader::workspace_trust::WorkspaceTrust;
use helix_term::{
    application::Application,
    args::Args,
    handlers::ime::{metrics, registry},
};
use helix_view::{document::Mode, editor::Action};
use indoc::indoc;
use std::time::Duration;

#[tokio::test(flavor = "multi_thread")]
async fn test_performance_cursor_moves() -> anyhow::Result<()> {
    let _env = ime_test_env();
    let mut app = Application::new(
        Args::default(),
        ime_test_config(),
        test_syntax_loader(None),
        WorkspaceTrust::fully_trusted(),
    )?;
    let view_id = app.editor.tree.focus;

    // Create a large document with various content
    let doc_id = overwrite_document_text(
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

    // Allow background initialization (SelectionDidChange hooks) to complete
    // before resetting metrics, mirroring the ime module's tests.
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
    app.editor.mode = Mode::Insert;
    metrics::reset();

    // Perform rapid cursor movements to stress test performance. Derive the
    // positions from the document so they can never exceed its length.
    let doc_len = app
        .editor
        .documents
        .get(&doc_id)
        .unwrap()
        .text()
        .len_chars();
    let positions: Vec<usize> = (1..=10).map(|i| doc_len * i / 11).collect();

    let start_time = std::time::Instant::now();

    for &pos in &positions {
        let doc = app.editor.documents.get_mut(&doc_id).unwrap();
        doc.set_selection(view_id, Selection::single(pos, pos));

        // This should use cached results when possible
        helix_term::handlers::ime::handle_cursor_move(&mut app.editor, view_id)?;
    }

    let elapsed = start_time.elapsed();
    let snapshot = metrics::snapshot();

    println!("Performance test results:");
    println!("  Total cursor moves: {}", positions.len());
    println!("  Total time: {:?}", elapsed);
    println!(
        "  Average time per move: {:?}",
        elapsed / positions.len() as u32
    );
    println!(
        "  Operations per second: {:.2}",
        positions.len() as f64 / elapsed.as_secs_f64()
    );

    // Verify performance metrics. The budget is deliberately generous: it is
    // a smoke threshold to catch order-of-magnitude regressions without
    // false-failing on a loaded CI machine.
    assert!(
        elapsed < Duration::from_secs(5),
        "Performance test exceeded 5 second budget"
    );

    assert!(
        snapshot.cursor_move_calls >= positions.len() as u64,
        "All cursor moves should be recorded"
    );

    // Cache should have improved performance
    if snapshot.region_detection_calls < positions.len() as u64 {
        println!(
            "  Cache effectiveness: {} cached vs {} total",
            snapshot.region_detection_calls,
            positions.len()
        );
    }

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_memory_usage_with_multiple_views() -> anyhow::Result<()> {
    let _env = ime_test_env();
    let mut app = Application::new(
        Args::default(),
        ime_test_config(),
        test_syntax_loader(None),
        WorkspaceTrust::fully_trusted(),
    )?;

    // Create multiple views to test memory management
    let mut view_ids = Vec::new();
    for _ in 0..10 {
        let doc = app.editor.tree.get(app.editor.tree.focus).doc;
        app.editor.switch(doc, Action::VerticalSplit);
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

    // Every view should own a context
    assert!(
        metrics.current_contexts() >= view_ids.len() as u64,
        "Each view should have an active IME context"
    );

    // Close half of the views
    for i in (0..10).step_by(2) {
        app.editor.close(view_ids[i]);
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
    let _env = ime_test_env();
    let mut app = Application::new(
        Args::default(),
        ime_test_config(),
        test_syntax_loader(None),
        WorkspaceTrust::fully_trusted(),
    )?;
    let view_id = app.editor.tree.focus;

    // Create a very large file
    let large_content = std::iter::repeat("let str = \"测试内容\"; // String content\n")
        .take(5000)
        .collect::<String>()
        + "\n\nfn main() {\n    println!(\"Large file test\");\n}";

    let doc_id = overwrite_document_text(&mut app, view_id, &large_content);

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
        doc.set_selection(view_id, Selection::single(pos, pos));

        helix_term::handlers::ime::handle_cursor_move(&mut app.editor, view_id)?;
    }

    let elapsed = start_time.elapsed();
    println!("Large file performance test:");
    println!("  Test moves: {}", test_positions.len());
    println!("  Total time: {:?}", elapsed);
    println!(
        "  Time per move: {:?}",
        elapsed / test_positions.len() as u32
    );

    // Performance should remain good even with large files. Generous budget
    // to avoid false failures on a loaded CI machine.
    assert!(
        elapsed < Duration::from_secs(2),
        "Large file test exceeded 2 second budget"
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_cache_effectiveness() -> anyhow::Result<()> {
    let _env = ime_test_env();
    let mut app = Application::new(
        Args::default(),
        ime_test_config(),
        test_syntax_loader(None),
        WorkspaceTrust::fully_trusted(),
    )?;
    let view_id = app.editor.tree.focus;

    // Create document with mixed regions
    let doc_id = overwrite_document_text(
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

    // Allow background initialization (SelectionDidChange hooks) to complete
    // before resetting metrics, mirroring the ime module's cache test.
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
    app.editor.mode = Mode::Insert;
    metrics::reset();

    // Define test positions for each region type
    let test_cases = vec![
        (25, "code"),    // "let code1 = 42"
        (45, "string"),  // "let string = \"中文测试\""
        (65, "code"),    // "let code2 = 100"
        (85, "comment"), // comment
        (120, "code"),   // "let end = true"
    ];

    // The region cache is keyed per view by cursor position: a move to a new
    // position runs detection, an immediate move to the same position hits
    // the cache. Visit every position twice in a row.
    for &(pos, _region) in &test_cases {
        for _ in 0..2 {
            let doc = app.editor.documents.get_mut(&doc_id).unwrap();
            doc.set_selection(view_id, Selection::single(pos, pos));
            helix_term::handlers::ime::handle_cursor_move(&mut app.editor, view_id)?;
        }
    }

    let snapshot = metrics::snapshot();

    println!("Cache effectiveness test:");
    println!("  Region detections: {}", snapshot.region_detection_calls);
    println!("  Cache hits: {}", snapshot.region_cache_hits);

    assert_eq!(
        snapshot.region_detection_calls,
        test_cases.len() as u64,
        "Each distinct position should be detected exactly once"
    );
    assert!(
        snapshot.region_cache_hits >= 1,
        "Immediate repeat visits should hit the cache"
    );

    Ok(())
}
