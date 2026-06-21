//! Linux IME control implementation using IBus/FCITX command-line tools.

use super::{ImeCapabilities, ImeController, ImeDetector, ImeInfo};
use anyhow::{Context, Result};
use std::process::Command;

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

    /// Get FCITX active engine name.
    /// fcitx-remote returns: 0 = close, 1 = inactive, 2 = active
    fn get_fcitx_engine() -> Result<String> {
        let output = Command::new("fcitx-remote")
            .output()
            .context("Failed to run fcitx-remote")?;

        if output.status.success() {
            let status = String::from_utf8_lossy(&output.stdout);
            match status.trim() {
                "0" => Ok("Closed".to_string()),
                "1" => Ok("Inactive".to_string()),  // 1 = inactive (IME not active)
                "2" => Ok("Active".to_string()),   // 2 = active (IME ready)
                _ => Ok("Unknown".to_string()),
            }
        } else {
            Err(anyhow::anyhow!("fcitx-remote command failed"))
        }
    }

    /// Query FCITX IME open/active status.
    /// Returns: Ok(true) = active, Ok(false) = inactive/closed
    fn query_fcitx_status() -> Result<bool> {
        let output = Command::new("fcitx-remote")
            .output()
            .context("Failed to run fcitx-remote")?;

        // fcitx-remote returns: 0 = close, 1 = inactive, 2 = active
        // Only "active" (2) means IME is truly enabled
        // "inactive" (1) means fcitx is running but IME is not activated
        if output.status.success() {
            let status = String::from_utf8_lossy(&output.stdout);
            match status.trim() {
                "0" => Ok(false),  // Closed
                "1" => Ok(false),  // Inactive - fcitx running but IME not activated
                "2" => Ok(true),   // Active - IME is enabled
                _ => Err(anyhow::anyhow!("Unexpected fcitx-remote output: {}", status)),
            }
        } else {
            Err(anyhow::anyhow!("fcitx-remote failed (fcitx not running?)"))
        }
    }
}

impl ImeController for LinuxImeController {
    fn is_ime_enabled() -> Result<bool> {
        if Self::is_ibus_running() {
            match Self::get_ibus_engine() {
                Ok(engine) if !engine.is_empty() => Ok(true),
                _ => Ok(false),
            }
        } else if Self::is_fcitx_running() {
            // Use query_fcitx_status() which properly parses the return value
            Self::query_fcitx_status()
        } else {
            // No IME daemon running
            Ok(false)
        }
    }

    fn set_ime_enabled(enabled: bool) -> Result<()> {
        if Self::is_ibus_running() {
            // IBus doesn't have a direct enable/disable command
            // We can only switch engines for now
            log::debug!("IBus detected - IME state control is limited on Linux");
            Ok(())
        } else if Self::is_fcitx_running() {
            // FCITX: use -o to activate (enable), -c to inactivate (disable)
            // Note: -e means "Ask fcitx to exit" - do NOT use!
            let result = Command::new("fcitx-remote")
                .arg(if enabled { "-o" } else { "-c" })
                .output()
                .context("Failed to run fcitx-remote for IME control")?;

            if result.status.success() {
                Ok(())
            } else {
                Err(anyhow::anyhow!(
                    "fcitx-remote failed: {}",
                    String::from_utf8_lossy(&result.stderr)
                ))
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
                    let _ime_type = ImeDetector::detect_ime_type(&engine);
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
                Ok(status) => Ok(ImeInfo {
                    name: format!("FCITX - {}", status),
                    version: None,
                    capabilities: ImeCapabilities::WithState,
                }),
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

    fn initialize() -> Result<()> {
        log::info!("Initializing Linux IME support");

        if Self::is_ibus_running() {
            log::info!("IBus daemon detected");
            if Self::get_ibus_engine().is_err() {
                log::warn!("IBus is unresponsive, consider restarting ibus-daemon");
            }
        } else if Self::is_fcitx_running() {
            log::info!("FCITX daemon detected");
        } else {
            log::warn!("No IME daemon (IBus/FCITX) detected");
            log::info!("Consider installing IBus or FCITX for Chinese input support");
        }

        Ok(())
    }
}
