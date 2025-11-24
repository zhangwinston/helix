//! macOS IME control implementation using TIS (Text Input Source) API.

use super::ImeController;
use anyhow::{Context, Result};

/// macOS IME controller using TIS (Text Input Source) APIs.
pub struct MacosImeController;

impl ImeController for MacosImeController {
    fn is_ime_enabled() -> Result<bool> {
        // TODO: Implement using TIS API
        // For now, return a placeholder that logs and returns false
        log::warn!("macOS IME status query not yet implemented");
        Ok(false)
    }

    fn set_ime_enabled(enabled: bool) -> Result<()> {
        // TODO: Implement using TIS API
        // For now, log and return Ok
        log::warn!(
            "macOS IME control not yet implemented (enabled: {})",
            enabled
        );
        Ok(())
    }
}
