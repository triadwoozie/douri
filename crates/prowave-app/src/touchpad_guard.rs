// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Dōri contributors.

//! Focus-scoped Linux touchpad guard to temporarily suspend `disable-while-typing` (DWT)
//! during Musical Typing, allowing simultaneous keyboard playing and trackpad control.
//!
//! Automatically records the user's desktop DWT state when activated and guarantees
//! restoration on close, unfocus, toggle-off, or application exit (Drop).

#[derive(Default, Debug)]
pub struct TouchpadGuard {
    #[cfg(target_os = "linux")]
    original_gnome_dwt: Option<bool>,
    #[cfg(target_os = "linux")]
    is_suspended: bool,
}

impl TouchpadGuard {
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether DWT is currently suspended by this guard.
    pub fn is_suspended(&self) -> bool {
        #[cfg(target_os = "linux")]
        {
            self.is_suspended
        }
        #[cfg(not(target_os = "linux"))]
        {
            false
        }
    }

    /// Updates the DWT suspension state based on whether Musical Typing is open,
    /// focused, and has the guard enabled.
    pub fn update(&mut self, active: bool) {
        #[cfg(target_os = "linux")]
        {
            if active {
                if !self.is_suspended {
                    if let Some(current) = Self::query_gnome_dwt() {
                        self.original_gnome_dwt = Some(current);
                        if current {
                            Self::set_gnome_dwt(false);
                            self.is_suspended = true;
                        }
                    }
                }
            } else if self.is_suspended {
                self.restore();
            }
        }
        #[cfg(not(target_os = "linux"))]
        {
            let _ = active;
        }
    }

    /// Restores the user's original desktop setting if it was suspended.
    pub fn restore(&mut self) {
        #[cfg(target_os = "linux")]
        {
            if self.is_suspended {
                if let Some(orig) = self.original_gnome_dwt.take() {
                    if orig {
                        Self::set_gnome_dwt(true);
                    }
                }
                self.is_suspended = false;
            }
        }
    }

    #[cfg(target_os = "linux")]
    fn query_gnome_dwt() -> Option<bool> {
        let output = std::process::Command::new("gsettings")
            .args([
                "get",
                "org.gnome.desktop.peripherals.touchpad",
                "disable-while-typing",
            ])
            .output()
            .ok()?;
        if output.status.success() {
            let s = String::from_utf8_lossy(&output.stdout)
                .trim()
                .to_lowercase();
            if s == "true" {
                return Some(true);
            } else if s == "false" {
                return Some(false);
            }
        }
        None
    }

    #[cfg(target_os = "linux")]
    fn set_gnome_dwt(enable: bool) -> bool {
        let val = if enable { "true" } else { "false" };
        let status = std::process::Command::new("gsettings")
            .args([
                "set",
                "org.gnome.desktop.peripherals.touchpad",
                "disable-while-typing",
                val,
            ])
            .status();
        status.map_or(false, |s| s.success())
    }
}

impl Drop for TouchpadGuard {
    fn drop(&mut self) {
        self.restore();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_touchpad_guard_state_transitions() {
        let mut guard = TouchpadGuard::new();
        assert!(!guard.is_suspended());
        guard.update(false);
        assert!(!guard.is_suspended());
        guard.restore();
        assert!(!guard.is_suspended());
    }

    #[test]
    fn test_touchpad_guard_mock_cycle() {
        struct MockGuard {
            original_dwt: Option<bool>,
            is_suspended: bool,
            restored: bool,
        }
        impl MockGuard {
            fn activate(&mut self, current: bool) {
                if !self.is_suspended {
                    self.original_dwt = Some(current);
                    if current {
                        self.is_suspended = true;
                    }
                }
            }
            fn restore(&mut self) {
                if self.is_suspended {
                    if let Some(orig) = self.original_dwt.take() {
                        if orig {
                            self.restored = true;
                        }
                    }
                    self.is_suspended = false;
                }
            }
        }
        impl Drop for MockGuard {
            fn drop(&mut self) {
                self.restore();
            }
        }

        let mut mock = MockGuard {
            original_dwt: None,
            is_suspended: false,
            restored: false,
        };
        mock.activate(true);
        assert!(mock.is_suspended);
        mock.restore();
        assert!(!mock.is_suspended);
        assert!(mock.restored);
    }
}
