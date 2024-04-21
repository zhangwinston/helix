
//! A trait for managing the Input Method Editor (IME).
//!
//! This module provides a generic `ImeManager` trait to abstract away
//! platform-specific details of controlling the IME state.

#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "linux")]
mod linux;

/// A generic interface for managing the IME.
pub trait ImeManager {
    /// Called when the editor enters Normal mode.
    /// Should disable the IME or switch to a non-converting input mode (e.g., English).
    /// Returns the previous IME open status.
    fn disable_and_get_status(&mut self) -> bool;

    /// Should restore the IME to a specified state.
    fn enable_with_status(&mut self, status: Option<bool>);
}

/// Creates a new, platform-specific IME manager.
///
/// This function acts as a factory, returning the appropriate `ImeManager`
/// implementation based on the target operating system.
pub fn new_ime_manager() -> Box<dyn ImeManager> {
    #[cfg(target_os = "windows")]
    {
        Box::new(windows::WindowsImeManager::new())
    }
    #[cfg(target_os = "macos")]
    {
        Box::new(macos::MacosImeManager::new())
    }
    #[cfg(target_os = "linux")]
    {
        Box::new(linux::LinuxImeManager::new())
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        panic!("IME support is not implemented for this platform");
    }
}
