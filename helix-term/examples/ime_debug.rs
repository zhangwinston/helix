//! Example program to demonstrate IME auto-control improvements.
//!
//! Run with: `cargo run --example ime_debug --features integration`
//!
//! This program creates a test editor instance with IME debug output enabled
//! to demonstrate the error handling and caching improvements.

#[cfg(any(test, feature = "integration"))]
mod ime_debug_demo {
    use anyhow::Result;
    use helix_core::{Selection, Transaction};
    use helix_loader::workspace_trust::WorkspaceTrust;
    use helix_term::{
        application::Application,
        args::Args,
        handlers::ime::{self, metrics, registry, verify_ime_state_consistency},
    };
    use helix_view::{document::Mode, editor::Action};
    use std::time::Duration;

    /// Test configuration equivalent to the integration tests' `test_config()`:
    /// default config with LSP disabled so the demo stays self-contained.
    fn example_config() -> helix_term::config::Config {
        helix_term::config::Config {
            editor: helix_view::editor::Config {
                lsp: helix_view::editor::LspConfig {
                    enable: false,
                    ..Default::default()
                },
                ..Default::default()
            },
            ..Default::default()
        }
    }

    /// Syntax loader built from the repo's default language config
    /// (equivalent to the integration tests' `test_syntax_loader(None)`).
    fn example_syntax_loader() -> helix_core::syntax::Loader {
        let lang = helix_loader::config::default_lang_config();
        helix_core::syntax::Loader::new(lang.try_into().unwrap()).unwrap()
    }

    /// Replace the whole document text, keeping the same document (same helper
    /// as the IME integration tests).
    fn overwrite_document_text(
        app: &mut Application,
        view_id: helix_view::ViewId,
        text: &str,
    ) -> helix_view::DocumentId {
        let doc_id = app.editor.tree.get(view_id).doc;
        let doc = app.editor.documents.get_mut(&doc_id).unwrap();
        let selection = doc.selection(view_id).clone();
        let transaction = Transaction::change_by_selection(doc.text(), &selection, |_| {
            (0, doc.text().len_chars(), Some(text.into()))
        })
        .with_selection(Selection::single(0, 0));
        doc.apply(&transaction, view_id);
        doc.ensure_view_init(view_id);
        doc_id
    }

    /// Char offset of `needle` in `haystack` (`str::find` returns a byte
    /// offset, while selections are char-indexed).
    fn char_offset(haystack: &str, needle: &str) -> usize {
        let byte = haystack
            .find(needle)
            .unwrap_or_else(|| panic!("expected substring '{needle}'"));
        haystack[..byte].chars().count()
    }

    pub fn run() -> Result<()> {
        // Initialize logging at debug level to see all IME messages. The level
        // can be overridden with HELIX_LOG_LEVEL; the first initializer wins,
        // so `Application::new`'s own setup below will not override this.
        helix_term::logging::init_stdout(log::LevelFilter::Debug);

        println!("=== IME Auto-Control Debug Example ===\n");

        // Create application with test configuration
        let mut app = Application::new(
            Args::default(),
            example_config(),
            example_syntax_loader(),
            WorkspaceTrust::fully_trusted(),
        )?;

        // Get initial view
        let view1_id = app.editor.tree.focus;

        // Create test document with various content types
        println!("1. Setting up test document with code, string, and comment content...");
        let _doc_id = setup_test_document(&mut app, view1_id)?;

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

    fn setup_test_document(
        app: &mut Application,
        view_id: helix_view::ViewId,
    ) -> Result<helix_view::DocumentId> {
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

        let doc_id = overwrite_document_text(app, view_id, content);

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
            let doc = app
                .editor
                .documents
                .get_mut(&app.editor.tree.get(view_id).doc)
                .unwrap();
            doc.set_selection(view_id, Selection::single(pos_char, pos_char));

            if let Err(e) = ime::handle_cursor_move(&mut app.editor, view_id) {
                println!("   ☐ Cursor move at position {} failed: {}", pos_char, e);
            } else {
                println!(
                    "   ✓ Cursor move {} at position {} succeeded",
                    i + 1,
                    pos_char
                );
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
        let doc = app
            .editor
            .documents
            .get_mut(&app.editor.tree.get(view_id).doc)
            .unwrap();
        let text = doc.text().to_string();
        let comment_pos = char_offset(&text, "这是");
        doc.set_selection(view_id, Selection::single(comment_pos, comment_pos));

        // First move - should trigger region detection
        ime::handle_cursor_move(&mut app.editor, view_id)?;
        let snapshot1 = metrics::snapshot();

        // Second move to same position - should use cache
        ime::handle_cursor_move(&mut app.editor, view_id)?;
        let snapshot2 = metrics::snapshot();

        println!(
            "   First move - Region detections: {}",
            snapshot1.region_detection_calls
        );
        println!(
            "   Second move - Region detections: {}",
            snapshot2.region_detection_calls
        );
        println!("   Cache hits: {}", snapshot2.region_cache_hits);

        // Test moving to different regions
        let doc = app
            .editor
            .documents
            .get_mut(&app.editor.tree.get(view_id).doc)
            .unwrap();
        let text = doc.text().to_string();
        let string_pos = char_offset(&text, "中文测试");
        doc.set_selection(view_id, Selection::single(string_pos, string_pos));
        ime::handle_cursor_move(&mut app.editor, view_id)?;

        let doc = app
            .editor
            .documents
            .get_mut(&app.editor.tree.get(view_id).doc)
            .unwrap();
        let text = doc.text().to_string();
        let code_pos = char_offset(&text, "42");
        doc.set_selection(view_id, Selection::single(code_pos, code_pos));
        ime::handle_cursor_move(&mut app.editor, view_id)?;

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
        ime::handle_mode_switch(&mut app.editor, view_id, Mode::Insert, Mode::Normal)?;

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
                if i % 2 == 0 {
                    Action::VerticalSplit
                } else {
                    Action::HorizontalSplit
                },
            );
            view_ids.push(app.editor.tree.focus);

            // Create IME context for each view
            app.editor.mode = Mode::Insert;
            ime::handle_cursor_move(&mut app.editor, *view_ids.last().unwrap())?;
        }

        print_metrics("After creating 4 views");

        // Close some views
        for id in view_ids.iter().take(3).skip(1) {
            app.editor.close(*id);
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
        println!("     Current contexts: {}", metrics.current_contexts());
        println!(
            "     Total created   : {}",
            metrics.total_contexts_created()
        );
        println!(
            "     Total removed   : {}",
            metrics.total_contexts_removed()
        );
        println!(
            "     Max concurrent  : {}",
            metrics.max_concurrent_contexts()
        );
        println!("     Cleanup count   : {}", metrics.cleanup_count());
    }
}

#[cfg(any(test, feature = "integration"))]
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    ime_debug_demo::run()
}

#[cfg(not(any(test, feature = "integration")))]
fn main() {
    println!("需要使用 --features integration 来运行此演示");
    println!("请运行: cargo run --example ime_debug --features integration");
}
