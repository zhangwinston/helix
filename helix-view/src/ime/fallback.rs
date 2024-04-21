use super::ImeManager;

/// A no-op IME manager for platforms where IME control is not implemented.
pub struct NoOpImeManager;

impl ImeManager for NoOpImeManager {
    fn disable_and_get_status(&mut self) -> bool {
        false
    }

    fn enable_with_status(&mut self, _status: Option<bool>) {
        // No operation
    }
}
