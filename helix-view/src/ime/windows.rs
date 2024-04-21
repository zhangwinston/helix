use super::ImeManager;
use windows::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
use windows::Win32::UI::Input::Ime::{ImmGetDefaultIMEWnd, IMC_SETOPENSTATUS};
use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, SendMessageA, WM_IME_CONTROL};

pub struct WindowsImeManager {}

impl WindowsImeManager {
    pub fn new() -> Self {
        Self {}
    }
}

impl ImeManager for WindowsImeManager {
    fn disable_and_get_status(&mut self) -> bool {
        const IMC_GETOPENSTATUS: usize = 0x0005;
        let mut was_ime_enabled = false;
        if let Some(hwnd) = unsafe { GetForegroundWindow().into() } {
            let ime_wnd = unsafe { ImmGetDefaultIMEWnd(hwnd) };
            if ime_wnd.0 == std::ptr::null_mut() {
                return false;
            }

            // Step 1: Get the current status without changing it
            let current_status = unsafe { SendMessageA(ime_wnd, WM_IME_CONTROL, WPARAM(IMC_GETOPENSTATUS), LPARAM(0)) };
            was_ime_enabled = current_status != LRESULT(0);

            // Step 2: Set the IME to closed
            unsafe { SendMessageA(ime_wnd, WM_IME_CONTROL, WPARAM(IMC_SETOPENSTATUS as usize), LPARAM(0)) };
        }
        was_ime_enabled
    }

    fn enable_with_status(&mut self, status: Option<bool>) {
        if let Some(should_enable) = status {
            if let Some(hwnd) = unsafe { GetForegroundWindow().into() } {
                let ime_wnd = unsafe { ImmGetDefaultIMEWnd(hwnd) };
                if ime_wnd.0 == std::ptr::null_mut() {
                    return;
                }
                // Restore IME status based on the passed-in status
                unsafe { SendMessageA(ime_wnd, WM_IME_CONTROL, WPARAM(IMC_SETOPENSTATUS as usize), LPARAM(should_enable as isize)) };
            }
        }
    }
}

