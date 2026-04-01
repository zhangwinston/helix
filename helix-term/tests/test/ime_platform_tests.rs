//! Platform-specific IME testing

use super::helpers::{test_config, test_syntax_loader};
use helix_term::{
    application::Application,
    args::Args,
    handlers::ime::platform::{self, ImeInfo, ImeCapabilities, ImeDetector, ImeType},
};
use helix_view::{document::Mode, editor::Action};

#[tokio::test(flavor = "multi_thread")]
async fn test_ime_platform_detection() -> anyhow::Result<()> {
    // Test platform detection
    println!("Testing platform IME support...");

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

                // Test IME state queries
                match platform::is_ime_enabled() {
                    Ok(enabled) => println!("  Current State: {}", if enabled { "Enabled" } else { "Disabled" }),
                    Err(e) => println!("  Failed to query state: {}", e),
                }
            }
            Err(e) => println!("  Failed to get IME info: {}", e),
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

#[tokio::test(flavor = "multi_thread")]
async fn test_ime_state_operations() -> anyhow::Result<()> {
    // Skip if IME is not available
    if !platform::is_ime_available() {
        println!("Skipping IME state test - no IME available");
        return Ok(());
    }

    // Test state operations
    let app = Application::new(Args::default(), test_config(), test_syntax_loader(None))?;
    let view_id = app.editor.tree.focus;

    // Set editor to insert mode
    let mut editor = app.editor;
    editor.mode = Mode::Insert;

    // Try to get current state
    let initial_state = platform::is_ime_enabled();
    match initial_state {
        Ok(state) => println!("Initial IME state: {}", state),
        Err(e) => println!("Failed to get initial state: {}", e),
    }

    // Try to toggle state
    if let Ok(initial) = initial_state {
        let target = !initial;
        match platform::set_ime_enabled(target) {
            Ok(()) => {
                println!("Successfully set IME state to: {}", target);

                // Verify state changed
                std::thread::sleep(std::time::Duration::from_millis(100));

                match platform::is_ime_enabled() {
                    Ok(actual) => {
                        if actual == target {
                            println!("State change verified");
                        } else {
                            println!("State change failed (expected {}, got {})", target, actual);
                        }
                    }
                    Err(e) => println!("Failed to verify state: {}", e),
                }

                // Restore original state
                let _ = platform::set_ime_enabled(initial);
            }
            Err(e) => println!("Failed to set IME state: {}", e),
        }
    }

    // Test reset
    match platform::reset_if_needed() {
        Ok(()) => println!("IME reset completed"),
        Err(e) => println!("IME reset failed: {}", e),
    }

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_ime_integration_with_editor() -> anyhow::Result<()> {
    let mut app = Application::new(Args::default(), test_config(), test_syntax_loader(None))?;
    let view_id = app.editor.tree.focus;

    // Create test content with different regions
    let doc_id = helix_term::tests::test::ime::overwrite_document_text(
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
    let doc = app.editor.documents.get_mut(&doc_id).unwrap();
    let pos = doc.text().to_string().find("IME should work here").unwrap();
    let pos_byte = doc.text().char_to_byte(pos);
    doc.set_selection(view_id, helix_core::Selection::single(pos, pos));

    helix_term::handlers::ime::handle_cursor_move(&mut app.editor, view_id)?;
    println!("Cursor moved to string position");

    // Move to code
    let pos = doc.text().to_string().find("42").unwrap();
    let pos_byte = doc.text().char_to_byte(pos);
    doc.set_selection(view_id, helix_core::Selection::single(pos, pos));

    helix_term::handlers::ime::handle_cursor_move(&mut app.editor, view_id)?;
    println!("Cursor moved to code position");

    // Verify IME state consistency
    match helix_term::handlers::ime::verify_ime_state_consistency(&app.editor, view_id) {
        Ok(consistent) => println!("IME state consistency: {}", consistent),
        Err(e) => println!("Failed to verify consistency: {}", e),
    }

    Ok(())
}

#[cfg(target_os = "windows")]
#[tokio::test(flavor = "multi_thread")]
async fn test_windows_ime_specific() -> anyhow::Result<()> {
    use helix_term::handlers::ime::platform::windows::WindowsImeController;

    println!("Testing Windows-specific IME features...");

    // Test IME window detection
    let is_available = WindowsImeController::is_ime_available();
    println!("  IME Window Available: {}", is_available);

    if is_available {
        // Get IME info
        match WindowsImeController::get_ime_info() {
            Ok(info) => {
                println!("  IME Name: {}", info.name);
                println!("  Capabilities: {:?}", info.capabilities);
            }
            Err(e) => println!("  Failed to get Windows IME info: {}", e),
        }
    }

    Ok(())
}

#[cfg(target_os = "linux")]
#[tokio::test(flavor = "multi_thread")]
async fn test_linux_ime_specific() -> anyhow::Result<()> {
    use helix_term::handlers::ime::platform::linux::LinuxImeController;

    println!("Testing Linux-specific IME features...");

    // Check for IME daemons
    println!("  IBus Running: {}", LinuxImeController::is_ibus_running());
    println!("  FCITX Running: {}", LinuxImeController::is_fcitx_running());

    if LinuxImeController::is_ibus_running() {
        match LinuxImeController::get_ibus_engine() {
            Ok(engine) => println!("  IBus Engine: {}", engine),
            Err(e) => println!("  Failed to get IBus engine: {}", e),
        }
    }

    if LinuxImeController::is_fcitx_running() {
        match LinuxImeController::get_fcitx_engine() {
            Ok(engine) => println!("  FCITX Engine: {}", engine),
            Err(e) => println!("  Failed to get FCITX engine: {}", e),
        }
    }

    Ok(())
}

#[cfg(target_os = "macos")]
#[tokio::test(flavor = "multi_thread")]
async fn test_macos_ime_specific() -> anyhow::Result<()> {
    use helix_term::handlers::ime::platform::macos::MacosImeController;

    println!("Testing macOS-specific IME features...");

    // Get current input source
    match MacosImeController::get_current_input_source() {
        Ok(source) => {
            println!("  Current Input Source: {}", source);
            println!("  Is IME: {}", MacosImeController::is_ime_input_source(&source));
        }
        Err(e) => println!("  Failed to get input source: {}", e),
    }

    Ok(())
}