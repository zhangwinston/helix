//! Windows IME control implementation using window messages.

use super::ImeController;
use anyhow::Result;

#[cfg(windows)]
use windows_sys::Win32::{
    UI::Input::Ime::ImmGetDefaultIMEWnd,
    UI::WindowsAndMessaging::{GetForegroundWindow, SendMessageA, WM_IME_CONTROL},
};

/// IME control message constants
#[cfg(windows)]
const IMC_SETOPENSTATUS: u32 = 0x0006;
#[cfg(windows)]
const IMC_GETOPENSTATUS: u32 = 0x0005;

/// Windows IME controller using window messages (WM_IME_CONTROL).
/// This approach is more reliable than using ImmSetOpenStatus directly.
pub struct WindowsImeController;

impl ImeController for WindowsImeController {
    fn is_ime_enabled() -> Result<bool> {
        unsafe {
            // Get the foreground window handle (the application window)
            let hwnd = GetForegroundWindow();
            if hwnd.is_null() {
                log::trace!("IME: GetForegroundWindow returned null");
                return Err(anyhow::anyhow!("Failed to get window handle"));
            }
            log::debug!("IME: Got foreground window handle: {:p}", hwnd);

            // Get the IME window handle directly
            let ime_wnd = ImmGetDefaultIMEWnd(hwnd);
            if ime_wnd.is_null() {
                log::trace!("IME: ImmGetDefaultIMEWnd returned null, IME likely closed");
                // If IME window is not available, IME is likely closed
                return Ok(false);
            }
            log::debug!("IME: Got IME window handle: {:p}", ime_wnd);

            // Send WM_IME_CONTROL message directly to the IME window to get IME status
            // According to Windows API documentation:
            // - wParam: IMC_GETOPENSTATUS (0x0005)
            // - lParam: 0 (not used)
            // Returns: non-zero if IME is open, 0 if closed
            // Use SendMessageA (ANSI) instead of SendMessageW for better IME compatibility
            let result = SendMessageA(ime_wnd, WM_IME_CONTROL, IMC_GETOPENSTATUS as usize, 0);

            let is_enabled = result != 0;
            log::trace!(
                "IME: is_ime_enabled result: {} (raw: {})",
                is_enabled,
                result
            );
            // If result is non-zero, IME is open
            Ok(is_enabled)
        }
    }

    fn set_ime_enabled(enabled: bool) -> Result<()> {
        unsafe {
            log::trace!("IME: set_ime_enabled called with enabled={}", enabled);

            // Get the foreground window handle (the application window)
            let hwnd = GetForegroundWindow();
            if hwnd.is_null() {
                log::warn!("IME: GetForegroundWindow returned null");
                return Err(anyhow::anyhow!("Failed to get window handle"));
            }
            log::debug!("IME: Got foreground window handle: {:p}", hwnd);

            // Get the IME window handle directly
            let ime_wnd = ImmGetDefaultIMEWnd(hwnd);
            if ime_wnd.is_null() {
                log::warn!("IME: ImmGetDefaultIMEWnd returned null, IME window not available");
                // If IME window is not available, silently return success
                // This allows the function to work even when IME is not active
                return Ok(());
            }
            log::debug!("IME: Got IME window handle: {:p}", ime_wnd);

            // Send WM_IME_CONTROL message directly to the IME window to set IME status
            // According to Windows API documentation:
            // - wParam: IMC_SETOPENSTATUS (0x0006)
            // - lParam: 1 to enable, 0 to disable
            // Use SendMessageA (ANSI) instead of SendMessageW for better IME compatibility
            let lparam = if enabled { 1 } else { 0 };
            log::trace!(
                "IME: Sending WM_IME_CONTROL to IME window with wParam={}, lParam={}",
                IMC_SETOPENSTATUS,
                lparam
            );

            let result = SendMessageA(ime_wnd, WM_IME_CONTROL, IMC_SETOPENSTATUS as usize, lparam);

            log::trace!("IME: SendMessageA result: {}", result);

            // Also try sending to the application window as a fallback
            // Some IME implementations require sending to the app window first
            let app_result = SendMessageA(hwnd, WM_IME_CONTROL, IMC_SETOPENSTATUS as usize, lparam);
            log::trace!("IME: SendMessageA to app window result: {}", app_result);

            Ok(())
        }
    }
}
