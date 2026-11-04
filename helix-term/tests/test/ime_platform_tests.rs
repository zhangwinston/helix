//! Platform-specific IME testing

use super::helpers::test_syntax_loader;
use super::ime::{
    ime_test_config, ime_test_env, ime_test_env_with, ime_test_lock, overwrite_document_text,
};
use helix_loader::workspace_trust::WorkspaceTrust;
use helix_term::{
    application::Application,
    args::Args,
    handlers::ime::platform::{
        self, testing::FakeImePlatform, ImeController, ImeDetector, ImeType, PlatformImeController,
    },
};
use helix_view::document::Mode;
use std::sync::atomic::Ordering;

#[tokio::test(flavor = "multi_thread")]
async fn test_ime_platform_detection() -> anyhow::Result<()> {
    // Probe the real OS platform. Hold the IME test lock so no other test has
    // a fake platform installed while we read.
    let _guard = ime_test_lock();

    // Check if IME is available
    let is_available = platform::is_ime_available();
    println!("  IME Available: {}", is_available);

    if is_available {
        // Get IME info
        match platform::get_ime_info() {
            Ok(ime_info) => {
                println!("  IME Name: {}", ime_info.name);
                println!("  Capabilities: {:?}", ime_info.capabilities);
                if let Some(version) = ime_info.version {
                    println!("  Version: {}", version);
                }
            }
            Err(e) => println!("  Failed to get IME info: {}", e),
        }

        // Test IME state queries
        match platform::is_ime_enabled() {
            Ok(enabled) => println!(
                "  Current State: {}",
                if enabled { "Enabled" } else { "Disabled" }
            ),
            Err(e) => println!("  Failed to query state: {}", e),
        }
    }

    // Test initialization
    match platform::initialize() {
        Ok(()) => println!("  Platform initialized successfully"),
        Err(e) => println!("  Initialization failed: {}", e),
    }

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_ime_type_detection() {
    // Test IME type detection with various names
    let test_cases = vec![
        ("Sogou Pinyin", ImeType::Sogou),
        ("Microsoft IME", ImeType::Microsoft),
        ("Google Pinyin", ImeType::GooglePinyin),
        ("fcitx", ImeType::Fcitx),
        ("IBus", ImeType::IBus),
        ("Baidu IME", ImeType::Baidu),
        ("QQ Pinyin", ImeType::Tencent),
        ("Unknown IME", ImeType::Unknown),
    ];

    for (name, expected_type) in test_cases {
        let detected = ImeDetector::detect_ime_type(name);
        assert_eq!(
            detected, expected_type,
            "Failed to detect IME type for: {}",
            name
        );

        // Get settings for this type
        let settings = ImeDetector::get_optimal_settings(detected);
        println!(
            "  {}: retry_count={}, retry_delay_ms={}",
            name, settings.retry_count, settings.retry_delay_ms
        );
    }
}

/// Toggle IME state through the installed fake platform instead of the real
/// OS input method, and verify the round trip is observable.
#[tokio::test(flavor = "multi_thread")]
async fn test_ime_state_operations() -> anyhow::Result<()> {
    let fake = FakeImePlatform::available_and(true);
    let _env = ime_test_env_with(fake.clone());

    let initial = platform::is_ime_enabled()?;
    assert!(
        initial,
        "Fake platform should report the initial enabled state"
    );

    // Try to toggle state
    platform::set_ime_enabled(false)?;

    // Verify state changed
    let actual = platform::is_ime_enabled()?;
    assert!(!actual, "State change should be observable through queries");
    assert!(
        fake.set_calls.load(Ordering::SeqCst) >= 1,
        "Set operations should be recorded by the fake platform"
    );

    // Restore original state
    platform::set_ime_enabled(initial)?;
    assert_eq!(platform::is_ime_enabled()?, initial);

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_ime_integration_with_editor() -> anyhow::Result<()> {
    let _env = ime_test_env();
    let mut app = Application::new(
        Args::default(),
        ime_test_config(),
        test_syntax_loader(None),
        WorkspaceTrust::fully_trusted(),
    )?;
    let view_id = app.editor.tree.focus;

    // Create test content with different regions
    let doc_id = overwrite_document_text(
        &mut app,
        view_id,
        r#"fn test_function() {
    // This is a comment
    let string = "IME should work here";
    let value = 42; // IME should be off here
}
"#,
    );

    // Set language for proper syntax highlighting
    let loader = app.editor.syn_loader.load();
    {
        let doc = app.editor.documents.get_mut(&doc_id).unwrap();
        doc.set_language_by_language_id("rust", &loader)?;
    }

    // Test different positions
    app.editor.mode = Mode::Insert;

    // Move to string
    {
        let doc = app.editor.documents.get_mut(&doc_id).unwrap();
        let pos = doc.text().to_string().find("IME should work here").unwrap();
        doc.set_selection(view_id, helix_core::Selection::single(pos, pos));
    }

    helix_term::handlers::ime::handle_cursor_move(&mut app.editor, view_id)?;
    println!("Cursor moved to string position");

    // Move to code
    {
        let doc = app.editor.documents.get_mut(&doc_id).unwrap();
        let pos = doc.text().to_string().find("42").unwrap();
        doc.set_selection(view_id, helix_core::Selection::single(pos, pos));
    }

    helix_term::handlers::ime::handle_cursor_move(&mut app.editor, view_id)?;
    println!("Cursor moved to code position");

    // Verify IME state consistency
    match helix_term::handlers::ime::verify_ime_state_consistency(&app.editor, view_id) {
        Ok(consistent) => println!("IME state consistency: {}", consistent),
        Err(e) => println!("Failed to verify consistency: {}", e),
    }

    Ok(())
}

/// Smoke-test the OS-specific controller compiled for this platform through
/// its public trait surface. Read-only queries only: this must never toggle
/// the developer's real input method.
#[tokio::test(flavor = "multi_thread")]
async fn test_os_ime_controller_read_only() -> anyhow::Result<()> {
    let _guard = ime_test_lock();

    let available = PlatformImeController::is_ime_available();
    println!("  OS IME available: {}", available);

    if available {
        match PlatformImeController::get_ime_info() {
            Ok(info) => println!("  IME Name: {}", info.name),
            Err(e) => println!("  Failed to get OS IME info: {}", e),
        }
    }

    match PlatformImeController::is_ime_enabled() {
        Ok(state) => println!("  OS IME enabled: {}", state),
        Err(e) => println!("  Failed to query OS IME state: {}", e),
    }

    Ok(())
}
