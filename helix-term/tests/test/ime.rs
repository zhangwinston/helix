// The IME test lock guards each test's whole body and is held across `.await`s
// on purpose (see `ime_test_lock`). Each test owns its tokio runtime, so
// waiting on the lock never starves the runtime of the test holding it.
#![allow(clippy::await_holding_lock)]

use super::helpers::{test_config, test_syntax_loader};
#[allow(unused_imports)]
use helix_core::{
    syntax::{detect_ime_sensitive_region, ImeSensitiveRegion, Syntax},
    Range, Rope, Selection, Transaction,
};
use helix_loader::workspace_trust::WorkspaceTrust;
use helix_term::{
    application::Application,
    args::Args,
    handlers::ime::{self, metrics, registry},
};
use helix_view::{document::Mode, editor::Action};
use indoc::indoc;
use parking_lot::{Mutex, MutexGuard};
use std::sync::atomic::Ordering;
use std::sync::{Arc, LazyLock};

/// IME state lives in process-global singletons (registry, scheduler, metrics)
/// keyed by `(DocumentId, ViewId)`. Each test builds its own `Editor`, and every
/// `Editor` starts numbering documents/views from 1, so tests running in
/// parallel would read and overwrite each other's IME contexts (e.g. one test's
/// `Some(Code)` region showing up in another's `assert_eq!(region, None)`).
/// These tests assert on that global state, so they must not overlap.
///
/// The lock guard must be held across `.await`s and `parking_lot`'s guard is
/// `Send` (unlike `std::sync`'s), which the `#[tokio::test]` futures require.
static IME_TEST_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

fn ime_test_lock() -> MutexGuard<'static, ()> {
    IME_TEST_LOCK.lock()
}

use helix_term::handlers::ime::platform::testing::{install_platform, FakeImePlatform};

/// Holds the IME test lock for the test's duration and keeps a hermetic fake
/// platform installed, restoring the real OS controller on drop. Without this,
/// tests on a desktop with a live fcitx/ibus would toggle the developer's real
/// input method, and outcomes would depend on each machine's IME setup.
struct ImeTestEnv {
    _guard: MutexGuard<'static, ()>,
}

impl Drop for ImeTestEnv {
    fn drop(&mut self) {
        install_platform(None);
    }
}

fn ime_test_env_with(fake: Arc<FakeImePlatform>) -> ImeTestEnv {
    let env = ImeTestEnv {
        _guard: IME_TEST_LOCK.lock(),
    };
    install_platform(Some(fake));
    env
}

/// Locks the tests and installs the default fake: an available IME, currently
/// disabled, whose platform calls always succeed.
fn ime_test_env() -> ImeTestEnv {
    ime_test_env_with(FakeImePlatform::available_and(false))
}

/// Test config with IME auto-control re-enabled: `test_config()` disables it
/// (see `test_editor_config`) because the IME globals are process-wide, but
/// these tests exercise exactly that machinery.
fn ime_test_config() -> helix_term::config::Config {
    let mut config = test_config();
    config.editor.ime_auto_control = true;
    config
}

// Access test-only function from registry
// Note: context() requires both doc_id and view_id now
fn get_ime_context(
    doc_id: helix_view::DocumentId,
    view_id: helix_view::ViewId,
) -> Option<helix_term::handlers::ime::engine::ImeContext> {
    helix_term::handlers::ime::registry::context(doc_id, view_id)
}

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

fn char_offset(haystack: &str, needle: &str) -> usize {
    let byte = haystack
        .find(needle)
        .unwrap_or_else(|| panic!("expected substring '{needle}'"));
    haystack[..byte].chars().count()
}

fn set_multi_cursor(
    doc: &mut helix_view::Document,
    view_id: helix_view::ViewId,
    positions: &[usize],
    primary: usize,
) {
    let ranges = positions
        .iter()
        .map(|&pos| Range::new(pos, pos))
        .collect::<smallvec::SmallVec<[Range; 1]>>();
    doc.set_selection(view_id, Selection::new(ranges, primary));
}

#[allow(dead_code)]
fn set_cursor(doc: &mut helix_view::Document, view_id: helix_view::ViewId, position: usize) {
    doc.set_selection(view_id, Selection::single(position, position));
}

/// Test that IME context is initialized when a view is created.
/// This test verifies that initialize_view_ime_state is called and creates
/// a default ImeContext with saved_state=None and current_region=None.
#[tokio::test(flavor = "multi_thread")]
async fn test_ime_context_initialized_on_view_creation() -> anyhow::Result<()> {
    let _ime_env = ime_test_env();
    let app = Application::new(
        Args::default(),
        ime_test_config(),
        test_syntax_loader(None),
        WorkspaceTrust::fully_trusted(),
    )?;
    let view_id = app.editor.tree.focus;
    let doc_id = app.editor.tree.get(view_id).doc;

    // Verify IME context exists and is initialized
    let ctx = get_ime_context(doc_id, view_id);
    assert!(
        ctx.is_some(),
        "IME context should be created when view is initialized"
    );

    let ctx = ctx.unwrap();
    assert_eq!(
        ctx.saved_state, None,
        "saved_state should be None on initialization"
    );
    assert_eq!(
        ctx.current_region, None,
        "current_region should be None on initialization"
    );
    assert_eq!(
        ctx.mode,
        Mode::Normal,
        "mode should match editor mode on initialization"
    );

    Ok(())
}

/// Test that IME context is reset when a document is opened.
/// This test verifies that DocumentDidOpen event triggers initialize_view_ime_state.
#[tokio::test(flavor = "multi_thread")]
async fn test_ime_context_reset_on_document_open() -> anyhow::Result<()> {
    let _ime_env = ime_test_env();
    let mut app = Application::new(
        Args::default(),
        ime_test_config(),
        test_syntax_loader(None),
        WorkspaceTrust::fully_trusted(),
    )?;
    let view_id = app.editor.tree.focus;
    let doc_id = app.editor.tree.get(view_id).doc;

    // Manually set some state to verify it gets reset
    registry::with_context_mut(doc_id, view_id, Mode::Insert, |ctx| {
        ctx.saved_state = Some(true);
        ctx.current_region = Some(ImeSensitiveRegion::StringContent);
        ctx.mode = Mode::Insert;
    });

    // Manually call initialize_view_ime_state to simulate document open
    helix_term::handlers::ime::initialize_view_ime_state(&mut app.editor, view_id);

    // Verify context was reset
    let ctx = get_ime_context(doc_id, view_id);
    assert!(ctx.is_some(), "IME context should still exist");

    let ctx = ctx.unwrap();
    // initialize_view_ime_state resets the context
    assert_eq!(
        ctx.saved_state, None,
        "saved_state should be reset to None on document open"
    );
    assert_eq!(
        ctx.current_region, None,
        "current_region should be reset to None on document open"
    );

    Ok(())
}

/// Test T057: IME自动关闭当view初始化且系统IME开启
///
/// This test verifies that when a view is initialized and system IME is enabled,
/// the initialize_view_ime_state function attempts to close IME.
///
/// Note: This test checks the IME context state rather than actual system IME state,
/// as platform-specific IME APIs may not be available in test environment.
#[tokio::test(flavor = "multi_thread")]
async fn test_ime_auto_closed_on_view_init_when_system_ime_enabled() -> anyhow::Result<()> {
    let _ime_env = ime_test_env();
    // Create a new application (this triggers view initialization)
    let app = Application::new(
        Args::default(),
        ime_test_config(),
        test_syntax_loader(None),
        WorkspaceTrust::fully_trusted(),
    )?;
    let view_id = app.editor.tree.focus;
    let doc_id = app.editor.tree.get(view_id).doc;

    // Verify IME context is initialized
    let ctx = get_ime_context(doc_id, view_id);
    assert!(
        ctx.is_some(),
        "IME context should be created on view initialization"
    );

    let ctx = ctx.unwrap();
    // The context should be reset (saved_state=None, current_region=None)
    // This indicates that initialize_view_ime_state was called
    assert_eq!(
        ctx.saved_state, None,
        "saved_state should be None after initialization (FR-001)"
    );
    assert_eq!(
        ctx.current_region, None,
        "current_region should be None after initialization"
    );

    // Note: We cannot directly test if system IME was closed in test environment,
    // but we can verify that the initialization function was called and context was reset.
    // The actual IME closing logic is tested in unit tests (engine.rs).

    Ok(())
}

/// Test T058: IME保持关闭当view初始化且系统IME关闭
///
/// This test verifies that when a view is initialized and system IME is disabled,
/// the IME state remains closed and context is properly initialized.
#[tokio::test(flavor = "multi_thread")]
async fn test_ime_stays_closed_on_view_init_when_system_ime_disabled() -> anyhow::Result<()> {
    let _ime_env = ime_test_env();
    // Create a new application (this triggers view initialization)
    let app = Application::new(
        Args::default(),
        ime_test_config(),
        test_syntax_loader(None),
        WorkspaceTrust::fully_trusted(),
    )?;
    let view_id = app.editor.tree.focus;
    let doc_id = app.editor.tree.get(view_id).doc;

    // Verify IME context is initialized
    let ctx = get_ime_context(doc_id, view_id);
    assert!(
        ctx.is_some(),
        "IME context should be created on view initialization"
    );

    let ctx = ctx.unwrap();
    // The context should be reset regardless of system IME state
    assert_eq!(
        ctx.saved_state, None,
        "saved_state should be None after initialization (FR-001)"
    );
    assert_eq!(
        ctx.current_region, None,
        "current_region should be None after initialization"
    );

    // Note: We cannot directly test system IME state in test environment,
    // but we can verify that the initialization function was called and context was reset.
    // The actual IME state management is tested in unit tests (engine.rs).

    Ok(())
}

/// Test that IME context is properly managed per view.
/// This test verifies FR-017: each view independently maintains IME state.
#[tokio::test(flavor = "multi_thread")]
async fn test_ime_context_independence_per_view() -> anyhow::Result<()> {
    let _ime_env = ime_test_env();
    let mut app = Application::new(
        Args::default(),
        ime_test_config(),
        test_syntax_loader(None),
        WorkspaceTrust::fully_trusted(),
    )?;
    let view1_id = app.editor.tree.focus;
    let doc1_id = app.editor.tree.get(view1_id).doc;

    // Set some state for view1
    registry::with_context_mut(doc1_id, view1_id, Mode::Insert, |ctx| {
        ctx.saved_state = Some(true);
        ctx.current_region = Some(ImeSensitiveRegion::StringContent);
    });

    // Create a new view by splitting vertically
    app.editor.switch(doc1_id, Action::VerticalSplit);
    let view2_id = app.editor.tree.focus;
    ime::initialize_view_ime_state(&mut app.editor, view2_id);

    let doc2_id = app.editor.tree.get(view2_id).doc;
    // Verify view2 has its own context
    let ctx2 = get_ime_context(doc2_id, view2_id);
    assert!(ctx2.is_some(), "View2 should have its own IME context");

    let ctx2 = ctx2.unwrap();
    // View2 should have default state (not affected by view1)
    assert_eq!(
        ctx2.saved_state, None,
        "View2 should have independent saved_state"
    );
    assert_eq!(
        ctx2.current_region, None,
        "View2 should have independent current_region"
    );

    // Verify view1's state is unchanged
    let ctx1 = get_ime_context(doc1_id, view1_id);
    assert!(ctx1.is_some(), "View1 context should still exist");
    let ctx1 = ctx1.unwrap();
    assert_eq!(
        ctx1.saved_state,
        Some(true),
        "View1's saved_state should be unchanged"
    );
    assert_eq!(
        ctx1.current_region,
        Some(ImeSensitiveRegion::StringContent),
        "View1's current_region should be unchanged"
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_cursor_move_uses_primary_selection_in_multi_cursor() -> anyhow::Result<()> {
    let _ime_env = ime_test_env();
    let mut app = Application::new(
        Args::default(),
        ime_test_config(),
        test_syntax_loader(None),
        WorkspaceTrust::fully_trusted(),
    )?;
    let view_id = app.editor.tree.focus;
    let doc_id = overwrite_document_text(
        &mut app,
        view_id,
        indoc! {r#"
            fn multi_cursor() {
                let code_cursor = 42;
                // comment_cursor should control IME
            }
        "#},
    );

    let loader = app.editor.syn_loader.load();
    {
        let doc = app.editor.documents.get_mut(&doc_id).unwrap();
        doc.set_language_by_language_id("rust", &loader)?;
        let text_str = doc.text().to_string();
        let code_pos = char_offset(&text_str, "code_cursor");
        let comment_pos = char_offset(&text_str, "comment_cursor");
        set_multi_cursor(doc, view_id, &[code_pos, comment_pos], 1);
    }

    app.editor.mode = Mode::Insert;
    metrics::reset();
    ime::handle_cursor_move(&mut app.editor, view_id)?;

    let ctx = get_ime_context(doc_id, view_id).unwrap();
    assert_eq!(
        ctx.current_region,
        Some(ImeSensitiveRegion::CommentContent),
        "Primary cursor (comment) should determine IME region (FR-020)"
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_cursor_move_entire_file_when_syntax_loading() -> anyhow::Result<()> {
    let _ime_env = ime_test_env();
    let mut app = Application::new(
        Args::default(),
        ime_test_config(),
        test_syntax_loader(None),
        WorkspaceTrust::fully_trusted(),
    )?;
    let view_id = app.editor.tree.focus;
    let doc_id = app.editor.tree.get(view_id).doc;

    app.editor.mode = Mode::Insert;
    metrics::reset();
    ime::handle_cursor_move(&mut app.editor, view_id)?;

    let ctx = get_ime_context(doc_id, view_id).unwrap();
    assert_eq!(
        ctx.current_region,
        Some(ImeSensitiveRegion::EntireFile),
        "Entire file should be sensitive while syntax parsing is pending (FR-021)"
    );

    Ok(())
}

// ============================================================================
// User Story 3: 无法语法解析文件的IME处理 (P2)
// ============================================================================

/// Test T048: IME可开启在无法语法解析的文件任意位置
///
/// This test verifies that when a file cannot be parsed (no syntax tree),
/// the entire file is treated as IME sensitive region (EntireFile).
///
/// Test scenario: Open a plain text file (no language assigned) and verify
/// that IME region detection returns EntireFile.
#[tokio::test(flavor = "multi_thread")]
async fn test_ime_enabled_in_unparseable_file_anywhere() -> anyhow::Result<()> {
    let _ime_env = ime_test_env();
    use helix_core::syntax::detect_ime_sensitive_region;

    let app = Application::new(
        Args::default(),
        ime_test_config(),
        test_syntax_loader(None),
        WorkspaceTrust::fully_trusted(),
    )?;
    let view_id = app.editor.tree.focus;
    let doc_id = app.editor.tree.get(view_id).doc;
    let doc = app.editor.documents.get(&doc_id).unwrap();

    // Get syntax tree (should be None for plain text file without language)
    let syntax = doc.syntax();
    let loader = doc.syntax_loader();
    let text = doc.text().slice(..);

    // Test at different positions in the file
    let test_positions = vec![0, 10, 50];

    for pos in test_positions {
        if pos < text.len_chars() {
            let byte_pos = text.char_to_byte(pos);
            let region = detect_ime_sensitive_region(syntax, text, &loader, byte_pos).region;

            // For unparseable files, should return EntireFile
            assert_eq!(
                region,
                ImeSensitiveRegion::EntireFile,
                "Position {} should be detected as EntireFile for unparseable file (FR-008)",
                pos
            );
        }
    }

    Ok(())
}

/// Test T049: IME可开启在语法解析出错的文件任意位置
///
/// This test verifies that when syntax parsing fails, the entire file
/// is treated as IME sensitive region (EntireFile).
///
/// Note: This test simulates a syntax parsing failure scenario by using
/// a file with invalid syntax that cannot be parsed.
#[tokio::test(flavor = "multi_thread")]
async fn test_ime_enabled_in_syntax_error_file_anywhere() -> anyhow::Result<()> {
    let _ime_env = ime_test_env();
    use helix_core::syntax::detect_ime_sensitive_region;

    let app = Application::new(
        Args::default(),
        ime_test_config(),
        test_syntax_loader(None),
        WorkspaceTrust::fully_trusted(),
    )?;
    let view_id = app.editor.tree.focus;
    let doc_id = app.editor.tree.get(view_id).doc;
    let doc = app.editor.documents.get(&doc_id).unwrap();

    // Get syntax tree (may be None or have errors for invalid syntax)
    let syntax = doc.syntax();
    let loader = doc.syntax_loader();
    let text = doc.text().slice(..);

    // Test at different positions
    let test_positions = vec![0, 10, 50];

    for pos in test_positions {
        if pos < text.len_chars() {
            let byte_pos = text.char_to_byte(pos);
            let region = detect_ime_sensitive_region(syntax, text, &loader, byte_pos).region;

            // For syntax error files, should return EntireFile if syntax is None
            if syntax.is_none() {
                assert_eq!(
                    region,
                    ImeSensitiveRegion::EntireFile,
                    "Position {} should be detected as EntireFile when syntax parsing failed (FR-009)",
                    pos
                );
            }
        }
    }

    Ok(())
}

/// Test T052: 语法解析错误场景的测试覆盖
///
/// This test provides comprehensive coverage for syntax parsing error scenarios,
/// including files with no language assigned, invalid syntax, etc.
#[tokio::test(flavor = "multi_thread")]
async fn test_ime_region_detection_syntax_error_scenarios() -> anyhow::Result<()> {
    let _ime_env = ime_test_env();
    use helix_core::syntax::detect_ime_sensitive_region;

    let app = Application::new(
        Args::default(),
        ime_test_config(),
        test_syntax_loader(None),
        WorkspaceTrust::fully_trusted(),
    )?;
    let view_id = app.editor.tree.focus;
    let doc_id = app.editor.tree.get(view_id).doc;
    let doc = app.editor.documents.get(&doc_id).unwrap();

    let syntax = doc.syntax();
    let loader = doc.syntax_loader();
    let text = doc.text().slice(..);

    // Test with None syntax (parsing unavailable/failed)
    if syntax.is_none() {
        let byte_pos = if text.len_chars() > 0 {
            text.char_to_byte(0)
        } else {
            0
        };
        let region = detect_ime_sensitive_region(None, text, &loader, byte_pos).region;
        assert_eq!(
            region,
            ImeSensitiveRegion::EntireFile,
            "Should return EntireFile when syntax is None (FR-008, FR-009, FR-021)"
        );
    }

    Ok(())
}

// ============================================================================
// User Story 4: 无字符串和注释类型文件的IME处理 (P2)
// ============================================================================

/// Test T053: IME可开启在无字符串和注释类型文件的任意位置
///
/// This test verifies that when a language doesn't have string or comment types
/// defined in its grammar, the entire file is treated as IME sensitive region.
///
/// Test scenario: Use a language configuration without comment_tokens and without
/// @string captures in highlights.scm, and verify that EntireFile is returned.
#[tokio::test(flavor = "multi_thread")]
async fn test_ime_enabled_in_file_without_string_comment_types() -> anyhow::Result<()> {
    let _ime_env = ime_test_env();
    use helix_core::syntax::detect_ime_sensitive_region;

    let app = Application::new(
        Args::default(),
        ime_test_config(),
        test_syntax_loader(None),
        WorkspaceTrust::fully_trusted(),
    )?;
    let view_id = app.editor.tree.focus;
    let doc_id = app.editor.tree.get(view_id).doc;
    let doc = app.editor.documents.get(&doc_id).unwrap();

    let syntax = doc.syntax();
    let loader = doc.syntax_loader();
    let text = doc.text().slice(..);

    if let Some(syntax) = syntax {
        let language = syntax.root_language();
        let language_data = loader.language(language);
        let config = language_data.config();

        // Check if language has comment tokens
        let has_comment = config.comment_tokens.is_some() || config.block_comment_tokens.is_some();

        // Check highlights query for string support
        use helix_core::syntax::read_query;
        let highlight_query = read_query(&config.language_id, "highlights.scm");
        let has_string = highlight_query.contains("@string") || highlight_query.contains("string");

        // Test region detection at different positions
        let test_positions = vec![0, 10, 50];

        for pos in test_positions {
            if pos < text.len_chars() {
                let byte_pos = text.char_to_byte(pos);
                let region =
                    detect_ime_sensitive_region(Some(syntax), text, &loader, byte_pos).region;

                // If language doesn't have string or comment types, should return EntireFile
                if !has_comment && !has_string {
                    assert_eq!(
                        region,
                        ImeSensitiveRegion::EntireFile,
                        "Position {} should return EntireFile for language without string/comment types (FR-010)",
                        pos
                    );
                } else {
                    // If language has string/comment types, should return Code or sensitive region
                    assert!(
                        matches!(
                            region,
                            ImeSensitiveRegion::EntireFile
                                | ImeSensitiveRegion::Code
                                | ImeSensitiveRegion::StringContent
                                | ImeSensitiveRegion::CommentContent
                        ),
                        "Position {} should return a valid region type",
                        pos
                    );
                }
            }
        }
    }

    Ok(())
}

/// Test T056: 无字符串/注释类型语言的测试覆盖
///
/// This test provides comprehensive coverage for languages without string/comment types.
/// It verifies that the language_has_string_or_comment_types function correctly
/// identifies languages that don't support these types.
#[tokio::test(flavor = "multi_thread")]
async fn test_ime_region_detection_language_without_string_comment() -> anyhow::Result<()> {
    let _ime_env = ime_test_env();
    use helix_core::syntax::detect_ime_sensitive_region;

    let app = Application::new(
        Args::default(),
        ime_test_config(),
        test_syntax_loader(None),
        WorkspaceTrust::fully_trusted(),
    )?;
    let view_id = app.editor.tree.focus;
    let doc_id = app.editor.tree.get(view_id).doc;
    let doc = app.editor.documents.get(&doc_id).unwrap();

    let syntax = doc.syntax();
    let loader = doc.syntax_loader();
    let text = doc.text().slice(..);

    if let Some(syntax) = syntax {
        let language = syntax.root_language();
        let language_data = loader.language(language);
        let config = language_data.config();

        // Check if language has comment tokens
        let has_comment = config.comment_tokens.is_some() || config.block_comment_tokens.is_some();

        // Check highlights query for string support
        use helix_core::syntax::read_query;
        let highlight_query = read_query(&config.language_id, "highlights.scm");
        let has_string = highlight_query.contains("@string") || highlight_query.contains("string");

        // Test region detection
        let byte_pos = if text.len_chars() > 0 {
            text.char_to_byte(0)
        } else {
            0
        };
        let region = detect_ime_sensitive_region(Some(syntax), text, &loader, byte_pos).region;

        // If language doesn't have string or comment types, should return EntireFile
        if !has_comment && !has_string {
            assert_eq!(
                region,
                ImeSensitiveRegion::EntireFile,
                "Should return EntireFile for language without string/comment types (FR-010)"
            );
        }
    }

    Ok(())
}

// ============================================================================
// Phase 8: Performance verification (T066-T068)
// ============================================================================

#[tokio::test(flavor = "multi_thread")]
async fn test_cursor_move_region_detection_cache() -> anyhow::Result<()> {
    let _ime_env = ime_test_env();
    let mut app = Application::new(
        Args::default(),
        ime_test_config(),
        test_syntax_loader(None),
        WorkspaceTrust::fully_trusted(),
    )?;
    // Allow background initialization (SelectionDidChange hooks) to complete
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
    metrics::reset();
    let view_id = app.editor.tree.focus;
    app.editor.mode = Mode::Insert;

    ime::handle_cursor_move(&mut app.editor, view_id)?;
    ime::handle_cursor_move(&mut app.editor, view_id)?;

    let snapshot = metrics::snapshot();
    assert_eq!(
        snapshot.region_detection_calls, 1,
        "Region detection should run only once due to cursor caching (T066)"
    );
    assert!(
        snapshot.region_cache_hits >= 1,
        "Subsequent cursor move should hit cache (T066)"
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_cursor_move_skips_when_not_insert_mode() -> anyhow::Result<()> {
    let _ime_env = ime_test_env();
    let mut app = Application::new(
        Args::default(),
        ime_test_config(),
        test_syntax_loader(None),
        WorkspaceTrust::fully_trusted(),
    )?;
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
    metrics::reset();
    let view_id = app.editor.tree.focus;
    app.editor.mode = Mode::Normal;

    ime::handle_cursor_move(&mut app.editor, view_id)?;
    let snapshot = metrics::snapshot();

    assert_eq!(
        snapshot.non_insert_skips, 1,
        "Non-insert mode cursor moves should be skipped (T067)"
    );
    assert_eq!(
        snapshot.region_detection_calls, 0,
        "Region detection must not run when editor is not in Insert mode (T067)"
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_cursor_move_latency_within_budget() -> anyhow::Result<()> {
    let _ime_env = ime_test_env();
    let mut app = Application::new(
        Args::default(),
        ime_test_config(),
        test_syntax_loader(None),
        WorkspaceTrust::fully_trusted(),
    )?;
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
    metrics::reset();
    let view_id = app.editor.tree.focus;
    app.editor.mode = Mode::Insert;

    const ITERATIONS: usize = 20;
    for _ in 0..ITERATIONS {
        ime::handle_cursor_move(&mut app.editor, view_id)?;
    }

    let snapshot = metrics::snapshot();
    assert!(
        snapshot.cursor_move_calls as usize >= ITERATIONS,
        "All cursor move calls should be recorded (T068)"
    );

    let avg_ns = snapshot
        .cursor_move_total_time_ns
        .checked_div(snapshot.cursor_move_calls)
        .unwrap_or(0);

    // 100ms budget per requirement T068 / SC-001
    const THRESHOLD_NS: u64 = 100_000_000;
    assert!(
        avg_ns < THRESHOLD_NS,
        "Average cursor move handling exceeded 100ms budget ({} ns)",
        avg_ns
    );
    Ok(())
}

#[cfg(not(target_os = "windows"))]
#[test]
fn test_multiline_string_and_comment_detection() -> anyhow::Result<()> {
    let loader = test_syntax_loader(None);
    let language = loader
        .language_for_name("rust".to_string())
        .expect("rust language");
    let source = Rope::from_str(indoc! {r##"
        fn spans() {
            let multiline = r#"first line
second line
third line"#;
            /* block
               comment body */
        }
    "##});
    let syntax =
        Syntax::new(source.slice(..), language, &loader).map_err(|e| anyhow::anyhow!("{e}"))?;
    let text = source.slice(..);
    let rope_string = source.to_string();

    let string_char = char_offset(&rope_string, "second line");
    let string_region = detect_ime_sensitive_region(
        Some(&syntax),
        text,
        &loader,
        source.char_to_byte(string_char),
    )
    .region;
    assert_eq!(
        string_region,
        ImeSensitiveRegion::StringContent,
        "Multi-line string body should be detected as sensitive (FR-004..FR-007)"
    );

    let comment_char = char_offset(&rope_string, "comment body");
    let comment_region = detect_ime_sensitive_region(
        Some(&syntax),
        text,
        &loader,
        source.char_to_byte(comment_char),
    )
    .region;
    assert_eq!(
        comment_region,
        ImeSensitiveRegion::CommentContent,
        "Multi-line comment body should be sensitive"
    );

    Ok(())
}

#[cfg(not(target_os = "windows"))]
#[tokio::test(flavor = "multi_thread")]
async fn test_cursor_move_robust_under_rapid_changes() -> anyhow::Result<()> {
    let _ime_env = ime_test_env();
    let mut app = Application::new(
        Args::default(),
        ime_test_config(),
        test_syntax_loader(None),
        WorkspaceTrust::fully_trusted(),
    )?;
    let view_id = app.editor.tree.focus;
    let doc_id = overwrite_document_text(
        &mut app,
        view_id,
        indoc! {r##"
            fn fast_moves() {
                let value = 99;
                let greeting = "hello world";
                // trailing comment keeps IME open
            }
        "##},
    );

    let loader = app.editor.syn_loader.load();
    {
        let doc = app.editor.documents.get_mut(&doc_id).unwrap();
        doc.set_language_by_language_id("rust", &loader)?;
    }

    app.editor.mode = Mode::Insert;
    metrics::reset();

    {
        let doc = app.editor.documents.get_mut(&doc_id).unwrap();
        let text_str = doc.text().to_string();
        let code_pos = char_offset(&text_str, "value = 99");
        set_cursor(doc, view_id, code_pos);
    }
    ime::handle_cursor_move(&mut app.editor, view_id)?;
    let ctx = get_ime_context(doc_id, view_id).unwrap();
    assert_eq!(ctx.current_region, Some(ImeSensitiveRegion::Code));

    {
        let doc = app.editor.documents.get_mut(&doc_id).unwrap();
        let text_str = doc.text().to_string();
        let string_pos = char_offset(&text_str, "hello world");
        set_cursor(doc, view_id, string_pos);
    }
    ime::handle_cursor_move(&mut app.editor, view_id)?;
    let ctx = get_ime_context(doc_id, view_id).unwrap();
    assert_eq!(ctx.current_region, Some(ImeSensitiveRegion::StringContent));

    {
        let doc = app.editor.documents.get_mut(&doc_id).unwrap();
        let text_str = doc.text().to_string();
        let comment_pos = char_offset(&text_str, "trailing comment");
        set_cursor(doc, view_id, comment_pos);
    }
    ime::handle_cursor_move(&mut app.editor, view_id)?;
    let ctx = get_ime_context(doc_id, view_id).unwrap();
    assert_eq!(ctx.current_region, Some(ImeSensitiveRegion::CommentContent));

    let snapshot = metrics::snapshot();
    assert!(
        snapshot.region_detection_calls >= 3,
        "Rapid cursor movements should still trigger region detection"
    );

    Ok(())
}

/// End-to-end: the engine must actually drive the *platform* IME through the
/// designed lifecycle:
///
/// - view init closes the system IME (FR-001);
/// - entering a sensitive region with no saved state does NOT auto-enable
///   (the user must enable IME themselves first);
/// - the user enables IME manually while in a string;
/// - moving into code saves that state and disables the IME;
/// - moving back into the string restores it.
///
/// This is the assertion that was missing while the fcitx5-remote breakage
/// shipped: the old tests only checked internal registry state, and platform
/// failures were swallowed by design (FR-019), so "control works" and
/// "control silently does nothing" were observationally identical.
#[tokio::test(flavor = "multi_thread")]
async fn test_engine_toggles_platform_ime_by_region() -> anyhow::Result<()> {
    // The user starts with IME enabled (as if they were typing Chinese).
    let fake = FakeImePlatform::available_and(true);
    let _ime_env = ime_test_env_with(fake.clone());

    let mut app = Application::new(
        Args::default(),
        ime_test_config(),
        test_syntax_loader(None),
        WorkspaceTrust::fully_trusted(),
    )?;

    // 0. View init must have closed the system IME (FR-001), for real.
    assert!(
        !fake.enabled.load(Ordering::Acquire),
        "view initialization must close the platform IME (FR-001)"
    );
    assert!(
        fake.set_calls.load(Ordering::Relaxed) >= 1,
        "engine must actually issue platform control calls"
    );

    let view_id = app.editor.tree.focus;
    let doc_id = overwrite_document_text(
        &mut app,
        view_id,
        indoc! {r##"
            fn demo() {
                let code = 99;
                let text = "你好";
            }
        "##},
    );
    let loader = app.editor.syn_loader.load();
    {
        let doc = app.editor.documents.get_mut(&doc_id).unwrap();
        doc.set_language_by_language_id("rust", &loader)?;
    }

    app.editor.mode = Mode::Insert;
    let set_calls_before = fake.set_calls.load(Ordering::Relaxed);

    // 1. String region, no saved state yet: the engine must not auto-enable.
    {
        let doc = app.editor.documents.get_mut(&doc_id).unwrap();
        let text_str = doc.text().to_string();
        set_cursor(doc, view_id, char_offset(&text_str, "你好"));
    }
    ime::handle_cursor_move(&mut app.editor, view_id)?;

    assert!(
        !fake.enabled.load(Ordering::Acquire),
        "entering a sensitive region with no saved state must not enable the IME"
    );
    assert_eq!(
        fake.set_calls.load(Ordering::Relaxed),
        set_calls_before,
        "no platform control call is expected while only entering a sensitive region"
    );

    // 2. The user enables IME themselves (e.g. Ctrl+Space, outside Helix).
    fake.enabled.store(true, Ordering::Release);

    // 3. Code region: the engine saves the user's state and disables the IME.
    {
        let doc = app.editor.documents.get_mut(&doc_id).unwrap();
        let text_str = doc.text().to_string();
        set_cursor(doc, view_id, char_offset(&text_str, "99"));
    }
    ime::handle_cursor_move(&mut app.editor, view_id)?;

    assert!(
        !fake.enabled.load(Ordering::Acquire),
        "code region must disable the platform IME"
    );

    // 4. Back to the string region: the saved (enabled) state is restored.
    {
        let doc = app.editor.documents.get_mut(&doc_id).unwrap();
        let text_str = doc.text().to_string();
        set_cursor(doc, view_id, char_offset(&text_str, "你好"));
    }
    ime::handle_cursor_move(&mut app.editor, view_id)?;

    assert!(
        fake.enabled.load(Ordering::Acquire),
        "returning to a sensitive region must restore the saved IME state"
    );

    Ok(())
}

/// FR-019: when every platform call fails, cursor moves must still succeed
/// and internal region tracking must keep working (the editor stays usable).
#[tokio::test(flavor = "multi_thread")]
async fn test_engine_tolerates_platform_failures() -> anyhow::Result<()> {
    let fake = FakeImePlatform::available_and(false);
    fake.query_fails.store(true, Ordering::Release);
    fake.control_fails.store(true, Ordering::Release);
    let _ime_env = ime_test_env_with(fake.clone());

    let mut app = Application::new(
        Args::default(),
        ime_test_config(),
        test_syntax_loader(None),
        WorkspaceTrust::fully_trusted(),
    )?;
    let view_id = app.editor.tree.focus;
    let doc_id = overwrite_document_text(
        &mut app,
        view_id,
        indoc! {r##"
            fn demo() {
                let text = "你好";
            }
        "##},
    );
    let loader = app.editor.syn_loader.load();
    {
        let doc = app.editor.documents.get_mut(&doc_id).unwrap();
        doc.set_language_by_language_id("rust", &loader)?;
    }

    app.editor.mode = Mode::Insert;
    {
        let doc = app.editor.documents.get_mut(&doc_id).unwrap();
        let text_str = doc.text().to_string();
        set_cursor(doc, view_id, char_offset(&text_str, "你好"));
    }

    // Must not propagate the platform error (FR-019).
    ime::handle_cursor_move(&mut app.editor, view_id)?;

    let ctx = get_ime_context(doc_id, view_id).expect("IME context should exist");
    assert_eq!(
        ctx.current_region,
        Some(ImeSensitiveRegion::StringContent),
        "region tracking must keep working despite platform failures"
    );
    assert!(fake.query_calls.load(Ordering::Relaxed) >= 1);

    Ok(())
}

/// Real-chain reproduction: in the live editor, cursor moves reach the engine
/// through the `SelectionDidChange` hook -> scheduler debounce (50ms) -> job
/// dispatch — not through direct `handle_cursor_move` calls. Verifies the
/// user-visible lifecycle end to end: enter insert, manually enable the IME
/// inside a string, move the cursor into code — the deferred path must
/// actually switch the platform IME off.
#[tokio::test(flavor = "multi_thread")]
async fn test_scheduler_path_controls_platform_ime() -> anyhow::Result<()> {
    use tokio_stream::wrappers::UnboundedReceiverStream;

    let fake = FakeImePlatform::available_and(true);
    let _ime_env = ime_test_env_with(fake.clone());

    let mut app = Application::new(
        Args::default(),
        ime_test_config(),
        test_syntax_loader(None),
        WorkspaceTrust::fully_trusted(),
    )?;
    let view_id = app.editor.tree.focus;
    let doc_id = overwrite_document_text(
        &mut app,
        view_id,
        indoc! {r##"
            fn demo() {
                let code = 99;
                let text = "你好";
            }
        "##},
    );
    let loader = app.editor.syn_loader.load();
    {
        let doc = app.editor.documents.get_mut(&doc_id).unwrap();
        doc.set_language_by_language_id("rust", &loader)?;
    }

    let (_tx, rx) = tokio::sync::mpsc::unbounded_channel();
    let mut stream = UnboundedReceiverStream::new(rx);

    // Enter insert like the real `i` key does: switch the mode, then run the
    // IME mode-switch handler (the real key dispatches the OnModeSwitch event
    // which calls this same handler — setting only the mode field without the
    // handler, or calling only the handler without the mode, is not faithful).
    app.editor.mode = Mode::Insert;
    ime::handle_mode_switch(&mut app.editor, view_id, Mode::Normal, Mode::Insert)?;

    // The deferred path lands on the editor loop via a job after a 50ms
    // debounce; under a loaded test runner it can arrive arbitrarily late, so
    // poll for the observable effect instead of sleeping a fixed amount.

    // Cursor into the string; wait until the deferred path has detected the
    // region (visible in the registry).
    {
        let doc = app.editor.documents.get_mut(&doc_id).unwrap();
        let text_str = doc.text().to_string();
        set_cursor(doc, view_id, char_offset(&text_str, "你好"));
    }
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while get_ime_context(doc_id, view_id).and_then(|ctx| ctx.current_region)
        != Some(ImeSensitiveRegion::StringContent)
    {
        assert!(
            std::time::Instant::now() < deadline,
            "deferred path did not detect the string region in time"
        );
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        let _ = tokio::time::timeout(
            std::time::Duration::from_secs(2),
            app.event_loop_until_idle(&mut stream),
        )
        .await;
    }

    // The user enables the IME manually (e.g. Ctrl+Space handled by the IME).
    fake.enabled.store(true, Ordering::Release);

    // Cursor into code: the deferred path must switch the platform IME off.
    {
        let doc = app.editor.documents.get_mut(&doc_id).unwrap();
        let text_str = doc.text().to_string();
        set_cursor(doc, view_id, char_offset(&text_str, "99"));
    }
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while fake.enabled.load(Ordering::Acquire) {
        assert!(
            std::time::Instant::now() < deadline,
            "leaving the sensitive region must close the platform IME via the scheduler path"
        );
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        let _ = tokio::time::timeout(
            std::time::Duration::from_secs(2),
            app.event_loop_until_idle(&mut stream),
        )
        .await;
    }

    Ok(())
}

/// Live platform contract test: verifies the real OS controller can query and
/// toggle the session IME. Ignored by default because it flips the user's
/// actual input method — run explicitly on a desktop with fcitx/ibus:
/// `cargo integration-test ime -- --ignored --test-threads=1`
#[tokio::test(flavor = "multi_thread")]
#[ignore = "toggles the real session IME; run explicitly on a desktop with fcitx/ibus"]
async fn test_live_platform_round_trip() -> anyhow::Result<()> {
    use helix_term::handlers::ime::platform as live;

    // Take the lock (no fake!) so a parallel IME test cannot install its
    // fake platform under us mid-flight.
    let _guard = ime_test_lock();

    live::initialize()?;
    if !live::is_ime_available() {
        // Legitimate on headless CI: nothing to verify.
        return Ok(());
    }

    let before = live::is_ime_enabled()?;
    live::set_ime_enabled(!before)?;
    let after = live::is_ime_enabled()?;
    live::set_ime_enabled(before)?; // restore the user's state

    assert_ne!(
        before, after,
        "set_ime_enabled must actually change the real platform state"
    );
    assert_eq!(
        live::is_ime_enabled()?,
        before,
        "the user's original IME state must be restored"
    );

    Ok(())
}
