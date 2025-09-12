use super::ImeManager;
use core_foundation::base::TCFType;
use core_foundation::string::{CFString, CFStringRef};

use core_services::tis::{
    kTISPropertyInputSourceID, kTISPropertyInputSourceIsSelectCapable,
    TISCopyCurrentKeyboardInputSource, TISGetInputSourceProperty, TISSelectInputSource,
};

// The bundle ID for the default U.S. keyboard layout.
const US_KEYBOARD_LAYOUT: &str = "com.apple.keylayout.US";

pub struct MacosImeManager {
    previous_input_source_id: Option<CFString>,
}

impl MacosImeManager {
    pub fn new() -> Self {
        Self {
            previous_input_source_id: None,
        }
    }
}

impl ImeManager for MacosImeManager {
    fn disable_and_get_status(&mut self) -> bool {
        let mut was_ime_enabled = false;
        unsafe {
            let current_source = TISCopyCurrentKeyboardInputSource();
            if current_source.is_null() {
                return false;
            }

            let is_selectable =
                TISGetInputSourceProperty(current_source, kTISPropertyInputSourceIsSelectCapable);
            if is_selectable.is_null()
                || core_foundation::boolean::CFBooleanGetValue(is_selectable.cast())
            {
                let source_id_ref =
                    TISGetInputSourceProperty(current_source, kTISPropertyInputSourceID)
                        as CFStringRef;
                if !source_id_ref.is_null() {
                    let source_id = CFString::wrap_under_get_rule(source_id_ref);
                    if source_id.to_string() != US_KEYBOARD_LAYOUT {
                        // If current is not US layout, it means IME was enabled
                        was_ime_enabled = true;
                        self.previous_input_source_id = Some(source_id);
                    }
                }
            }

            let target_id = CFString::new(US_KEYBOARD_LAYOUT);
            let input_source = core_foundation::dictionary::CFDictionary::from_CFType_pairs(&[(
                kTISPropertyInputSourceID,
                target_id.as_CFType(),
            )]);

            TISSelectInputSource(input_source.as_concrete_TypeRef());
        }
        was_ime_enabled
    }

    fn enable_with_status(&mut self, status: Option<bool>) {
        if let Some(true) = status {
            if let Some(previous_id) = &self.previous_input_source_id {
                unsafe {
                    let input_source =
                        core_foundation::dictionary::CFDictionary::from_CFType_pairs(&[(
                            CFString::new(kch::kCTInputSourceID),
                            previous_id.as_CFType(),
                        )]);
                    TISSelectInputSource(input_source.as_concrete_TypeRef());
                }
            }
        }
    }
}
