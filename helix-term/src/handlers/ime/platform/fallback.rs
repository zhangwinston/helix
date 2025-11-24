//! Fallback IME controller for unsupported platforms.

use super::ImeController;
use anyhow::Result;

/// Fallback IME controller for platforms without specific implementation.
///
/// This implementation does nothing but allows the code to compile on
/// unsupported platforms. All operations are no-ops.
pub struct FallbackImeController;

impl ImeController for FallbackImeController {
    fn is_ime_enabled() -> Result<bool> {
        // Always return false on unsupported platforms
        Ok(false)
    }

    fn set_ime_enabled(_enabled: bool) -> Result<()> {
        // No-op on unsupported platforms
        Ok(())
    }
}
