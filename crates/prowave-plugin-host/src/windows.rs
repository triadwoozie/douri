#![allow(dead_code)]
//! The plug-in's window on Windows, in the plug-in's own process.

use std::time::Duration;

pub struct Window {
    hwnd: *mut std::ffi::c_void,
    pub width: u32,
    pub height: u32,
    pub open: bool,
}

impl Window {
    pub fn new(_title: &str) -> Option<Self> {
        // Win32 child window embedding handle
        None
    }

    #[inline]
    pub fn parent(&self) -> *mut std::ffi::c_void {
        self.hwnd
    }

    pub fn set_plugin_size(&mut self, width: u32, height: u32) {
        self.width = width.max(100);
        self.height = height.max(100);
    }

    pub fn set_title(&mut self, _title: &str) {}

    pub fn set_power(&mut self, _power: bool) {}

    pub fn show(&mut self) {
        self.open = true;
    }

    pub fn hide(&mut self) {
        self.open = false;
    }

    pub fn tick(&mut self, _on_close: impl FnMut()) {}
}

pub fn run(mut tick: Box<dyn FnMut()>) {
    loop {
        tick();
        std::thread::sleep(Duration::from_millis(4));
    }
}
