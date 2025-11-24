//! Platform-specific IME (Input Method Editor) control implementation.
//!
//! This module provides a trait-based abstraction for controlling IME across
//! different platforms (Windows, Linux, macOS).

use anyhow::Result;

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
