//! Linux IME control implementation using IBus/FCITX D-Bus interfaces.

use super::ImeController;
use anyhow::{Context, Result};

/// Linux IME controller using IBus/FCITX D-Bus interfaces.
pub struct LinuxImeController;

impl ImeController for LinuxImeController {
    fn is_ime_enabled() -> Result<bool> {
        // TODO: Implement using IBus/FCITX D-Bus
        // For now, return a placeholder that logs and returns false
        log::warn!("Linux IME status query not yet implemented");
        Ok(false)
    }

    fn set_ime_enabled(enabled: bool) -> Result<()> {
        // TODO: Implement using IBus/FCITX D-Bus
        // For now, log and return Ok
        log::warn!(
            "Linux IME control not yet implemented (enabled: {})",
            enabled
        );
        Ok(())
    }
}
