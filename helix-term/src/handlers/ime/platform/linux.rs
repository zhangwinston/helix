//! Linux IME control implementation using IBus/FCITX D-Bus interfaces.

use super::{ImeController, ImeInfo, ImeCapabilities, ImeDetector, ImeType, ImeSettings};
use anyhow::{Context, Result};
use std::process::Command;
use std::time::Duration;

/// Linux IME controller using IBus/FCITX D-Bus interfaces.
pub struct LinuxImeController;

impl LinuxImeController {
    /// Check if IBus daemon is running
    fn is_ibus_running() -> bool {
        Command::new("pgrep")
            .arg("ibus-daemon")
            .output()
            .map(|o| !o.stdout.is_empty())
            .unwrap_or(false)
    }

    /// Check if FCITX daemon is running
    fn is_fcitx_running() -> bool {
        Command::new("pgrep")
            .arg("fcitx")
            .output()
            .map(|o| !o.stdout.is_empty())
            .unwrap_or(false)
    }

    /// Get active IME engine using IBus
    fn get_ibus_engine() -> Result<String> {
        let output = Command::new("ibus")
            .arg("engine")
            .output()
            .context("Failed to run ibus engine")?;

        if output.status.success() {
            let engine = String::from_utf8_lossy(&output.stdout);
            Ok(engine.trim().to_string())
        } else {
            Err(anyhow::anyhow!("ibus engine command failed"))
        }
    }

    /// Get active IME engine using FCITX
    fn get_fcitx_engine() -> Result<String> {
        let output = Command::new("fcitx-remote")
            .output()
            .context("Failed to run fcitx-remote")?;

        if output.status.success() {
            let status = String::from_utf8_lossy(&output.stdout);
            // FCITX returns different codes for active engine
            match status.trim() {
                "1" => Ok("Active".to_string()),
                "2" => Ok("Inactive".to_string()),
                _ => Ok("Unknown".to_string()),
            }
        } else {
            Err(anyhow::anyhow!("fcitx-remote command failed"))
        }
    }
}

impl ImeController for LinuxImeController {
    fn is_ime_enabled() -> Result<bool> {
        if Self::is_ibus_running() {
            match Self::get_ibus_engine() {
                Ok(_engine) => Ok(true), // Assume IME is enabled if we can get engine
                Err(_) => Ok(false),
            }
        } else if Self::is_fcitx_running() {
            match Self::get_fcitx_engine() {
                Ok(status) => Ok(status != "Inactive"),
                Err(_) => Ok(false),
            }
        } else {
            // No IME daemon running
            Ok(false)
        }
    }

    fn set_ime_enabled(enabled: bool) -> Result<()> {
        if Self::is_ibus_running() {
            // IBus doesn't have a direct enable/disable command
            // We can only switch engines for now
            log::info!("IBus detected - IME state control is limited on Linux");
            Ok(())
        } else if Self::is_fcitx_running() {
            // FCITX has enable/disable commands
            let result = Command::new("fcitx-remote")
                .arg(if enabled { "-e" } else { "-d" })
                .output()
                .context("Failed to run fcitx-remote for IME control")?;

            if result.status.success() {
                Ok(())
            } else {
                Err(anyhow::anyhow!("fcitx-remote failed: {}",
                    String::from_utf8_lossy(&result.stderr)))
            }
        } else {
            log::warn!("No IME daemon (IBus/FCITX) detected");
            Ok(())
        }
    }

    fn get_ime_info() -> Result<ImeInfo> {
        if Self::is_ibus_running() {
            match Self::get_ibus_engine() {
                Ok(engine) => {
                    let ime_type = ImeDetector::detect_ime_type(&engine);
                    Ok(ImeInfo {
                        name: format!("IBus - {}", engine),
                        version: None,
                        capabilities: ImeCapabilities::WithState,
                    })
                }
                Err(e) => {
                    log::error!("Failed to get IBus engine: {}", e);
                    Ok(ImeInfo {
                        name: "IBus".to_string(),
                        version: None,
                        capabilities: ImeCapabilities::Basic,
                    })
                }
            }
        } else if Self::is_fcitx_running() {
            match Self::get_fcitx_engine() {
                Ok(status) => {
                    Ok(ImeInfo {
                        name: format!("FCITX - {}", status),
                        version: None,
                        capabilities: ImeCapabilities::WithState,
                    })
                }
                Err(e) => {
                    log::error!("Failed to get FCITX status: {}", e);
                    Ok(ImeInfo {
                        name: "FCITX".to_string(),
                        version: None,
                        capabilities: ImeCapabilities::Basic,
                    })
                }
            }
        } else {
            Ok(ImeInfo {
                name: "No IME".to_string(),
                version: None,
                capabilities: ImeCapabilities::Basic,
            })
        }
    }

    fn is_ime_available() -> bool {
        Self::is_ibus_running() || Self::is_fcitx_running()
    }

    fn reset_if_needed() -> Result<()> {
        // On Linux, we can restart the IME daemon if it's unresponsive
        if Self::is_ibus_running() {
            // Try to ping IBus
            if let Err(_) = Self::get_ibus_engine() {
                log::warn!("IBus is unresponsive, consider restarting ibus-daemon");
            }
        }
        Ok(())
    }

    fn initialize() -> Result<()> {
        log::info!("Initializing Linux IME support");

        if Self::is_ibus_running() {
            log::info!("IBus daemon detected");
        } else if Self::is_fcitx_running() {
            log::info!("FCITX daemon detected");
        } else {
            log::warn!("No IME daemon (IBus/FCITX) detected");
            log::info!("Consider installing IBus or FCITX for Chinese input support");
        }

        Ok(())
    }
}