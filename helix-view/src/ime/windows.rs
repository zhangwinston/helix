use super::ImeManager;
use windows::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
use windows::Win32::UI::Input::Ime::{ImmGetDefaultIMEWnd, IMC_SETOPENSTATUS};
use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, SendMessageA, WM_IME_CONTROL};

pub struct WindowsImeManager {
    previous_ime_open_status: Option<bool>,
}

impl WindowsImeManager {
    pub fn new() -> Self {
        Self { previous_ime_open_status: None }
    }
}

impl ImeManager for WindowsImeManager {
    fn disable_and_get_status(&mut self) -> bool {
        let mut was_ime_enabled = false;
        if let Some(hwnd) = unsafe { GetForegroundWindow().into() } {
            let ime_wnd = unsafe { ImmGetDefaultIMEWnd(hwnd) };
            if ime_wnd.0 == std::ptr::null_mut() {
                return false;
            }

            // Get current IME open status
            let current_status = unsafe { SendMessageA(ime_wnd, WM_IME_CONTROL, WPARAM(IMC_SETOPENSTATUS as usize), LPARAM(0)) };
            was_ime_enabled = current_status != LRESULT(0);
            self.previous_ime_open_status = Some(was_ime_enabled);

            // Set IME to closed (0)
            unsafe { SendMessageA(ime_wnd, WM_IME_CONTROL, WPARAM(IMC_SETOPENSTATUS as usize), LPARAM(0)) };
        }
        was_ime_enabled
    }

    fn enable_with_status(&mut self, status: Option<bool>) {
        if let Some(true) = status {
            if let Some(param) = self.previous_ime_open_status.take() {
            if let Some(hwnd) = unsafe { GetForegroundWindow().into() } {
                let ime_wnd = unsafe { ImmGetDefaultIMEWnd(hwnd) };
                if ime_wnd.0 == std::ptr::null_mut() {
                    return;
                }

                // Restore previous IME open status
                unsafe { SendMessageA(ime_wnd, WM_IME_CONTROL, WPARAM(IMC_SETOPENSTATUS as usize), LPARAM(param as isize)) };
            }
            }
        }
    }
}
