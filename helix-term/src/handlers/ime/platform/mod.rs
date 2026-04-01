//! Platform-specific IME (Input Method Editor) control implementation.
//!
//! This module provides a trait-based abstraction for controlling IME across
//! different platforms (Windows, Linux, macOS).

use anyhow::Result;
use std::collections::HashMap;

/// Information about the current IME
#[derive(Debug, Clone)]
pub struct ImeInfo {
    pub name: String,
    pub version: Option<String>,
    pub capabilities: ImeCapabilities,
}

/// IME capability levels
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImeCapabilities {
    /// Only basic on/off control
    Basic,
    /// Can query current state
    WithState,
    /// Full control capabilities (custom settings, etc.)
    FullControl,
}

/// Trait for platform-specific IME control operations.
pub trait ImeController {
    /// Query whether IME is currently enabled.
    ///
    /// Returns `Ok(true)` if IME is enabled, `Ok(false)` if disabled.
    /// Errors may occur due to platform-specific issues (permissions, API unavailable, etc.).
    fn is_ime_enabled() -> Result<bool>;

    /// Set IME enabled/disabled state.
    ///
    /// # Arguments
    /// * `enabled` - `true` to enable IME, `false` to disable
    ///
    /// Returns `Ok(())` on success.
    /// Errors may occur due to platform-specific issues (permissions, API unavailable, etc.).
    fn set_ime_enabled(enabled: bool) -> Result<()>;

    /// Get information about the current IME.
    ///
    /// Returns details about the active IME engine including name, version,
    /// and supported capabilities.
    fn get_ime_info() -> Result<ImeInfo>;

    /// Check if an IME is available and functional.
    ///
    /// Some systems may not have any IME installed or configured.
    fn is_ime_available() -> bool;

    /// Reset IME state if needed.
    ///
    /// Some IMEs may need to be reset explicitly after certain operations
    /// to ensure proper functionality.
    fn reset_if_needed() -> Result<()> {
        // Default implementation is no-op
        Ok(())
    }

    /// Perform platform-specific initialization.
    ///
    /// Called once during application startup to initialize any required
    /// platform-specific resources.
    fn initialize() -> Result<()> {
        // Default implementation is no-op
        Ok(())
    }
}

/// IME detection and optimization utilities
pub struct ImeDetector;

impl ImeDetector {
    /// Detect common IME engines by name patterns
    pub fn detect_ime_type(ime_name: &str) -> ImeType {
        let name_lower = ime_name.to_lowercase();

        if name_lower.contains("sogou") {
            ImeType::Sogou
        } else if name_lower.contains("microsoft") || name_lower.contains("ms") {
            ImeType::Microsoft
        } else if name_lower.contains("google") || name_lower.contains("pinyin") {
            ImeType::GooglePinyin
        } else if name_lower.contains("fcitx") {
            ImeType::Fcitx
        } else if name_lower.contains("ibus") {
            ImeType::IBus
        } else if name_lower.contains("baidu") {
            ImeType::Baidu
        } else if name_lower.contains("tencent") || name_lower.contains("qq") {
            ImeType::Tencent
        } else if name_lower.contains("scim") {
            ImeType::SCIM
        } else {
            ImeType::Unknown
        }
    }

    /// Get optimal settings for a specific IME type
    pub fn get_optimal_settings(ime_type: ImeType) -> ImeSettings {
        match ime_type {
            ImeType::Sogou => ImeSettings {
                retry_count: 2,
                retry_delay_ms: 20,
                reset_threshold: 5,
                use_fallback_api: false,
                custom_settings: HashMap::from([
                    ("disable_animation".to_string(), "true".to_string()),
                    ("fast_switch".to_string(), "true".to_string()),
                ]),
            },
            ImeType::Microsoft => ImeSettings {
                retry_count: 3,
                retry_delay_ms: 10,
                reset_threshold: 10,
                use_fallback_api: false,
                custom_settings: HashMap::new(),
            },
            ImeType::GooglePinyin => ImeSettings {
                retry_count: 2,
                retry_delay_ms: 15,
                reset_threshold: 7,
                use_fallback_api: true,
                custom_settings: HashMap::from([
                    ("enhanced_compatibility".to_string(), "true".to_string()),
                ]),
            },
            ImeType::Unknown => ImeSettings::default(),
            _ => ImeSettings::default(),
        }
    }
}

/// Known IME types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImeType {
    Sogou,
    Microsoft,
    GooglePinyin,
    Fcitx,
    IBus,
    Baidu,
    Tencent,
    SCIM,
    Unknown,
}

/// Platform-specific IME settings
#[derive(Debug, Clone)]
pub struct ImeSettings {
    pub retry_count: u32,
    pub retry_delay_ms: u64,
    pub reset_threshold: u32,
    pub use_fallback_api: bool,
    pub custom_settings: HashMap<String, String>,
}

impl Default for ImeSettings {
    fn default() -> Self {
        Self {
            retry_count: 3,
            retry_delay_ms: 10,
            reset_threshold: 10,
            use_fallback_api: false,
            custom_settings: HashMap::new(),
        }
    }
}

#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "windows")]
pub use windows::WindowsImeController as PlatformImeController;

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
pub use linux::LinuxImeController as PlatformImeController;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub use macos::MacosImeController as PlatformImeController;

#[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
mod fallback;
#[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
pub use fallback::FallbackImeController as PlatformImeController;

/// Convenience function to query IME enabled state using platform-specific implementation.
pub fn is_ime_enabled() -> Result<bool> {
    PlatformImeController::is_ime_enabled()
}

/// Convenience function to set IME enabled state using platform-specific implementation.
pub fn set_ime_enabled(enabled: bool) -> Result<()> {
    PlatformImeController::set_ime_enabled(enabled)
}

/// Get information about the current IME
pub fn get_ime_info() -> Result<ImeInfo> {
    PlatformImeController::get_ime_info()
}

/// Check if any IME is available
pub fn is_ime_available() -> bool {
    PlatformImeController::is_ime_available()
}

/// Initialize platform-specific IME support
pub fn initialize() -> Result<()> {
    PlatformImeController::initialize()
}

/// Reset IME if needed
pub fn reset_if_needed() -> Result<()> {
    PlatformImeController::reset_if_needed()
}