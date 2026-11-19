//! The plug-in's window on Linux, in the plug-in's own process.
//!
//! Uses `libX11.so.6` loaded dynamically via `libloading` so no compile-time linkage
//! or external C headers are required. Works seamlessly under X11 and XWayland.

use std::ffi::{c_char, c_int, c_long, c_uint, c_ulong, c_void, CString};
use std::sync::OnceLock;
use std::time::Duration;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct XEvent {
    pub pad: [c_long; 24],
}

impl XEvent {
    #[inline]
    pub fn event_type(&self) -> c_int {
        self.pad[0] as c_int
    }

    #[inline]
    pub fn client_data0(&self) -> c_ulong {
        // In XClientMessageEvent on 64-bit platforms, data.l[0] is at offset 56 (pad[7]).
        self.pad[7] as c_ulong
    }
}

const CLIENT_MESSAGE: c_int = 33;
const STRUCTURE_NOTIFY_MASK: c_long = 1 << 17;
const EXPOSURE_MASK: c_long = 1 << 15;

#[allow(dead_code)]
struct X11Lib {
    _lib: libloading::Library,
    init_threads: Option<unsafe extern "C" fn() -> c_int>,
    open_display: unsafe extern "C" fn(*const c_char) -> *mut c_void,
    close_display: unsafe extern "C" fn(*mut c_void) -> c_int,
    default_root_window: unsafe extern "C" fn(*mut c_void) -> c_ulong,
    create_simple_window: unsafe extern "C" fn(
        *mut c_void,
        c_ulong,
        c_int,
        c_int,
        c_uint,
        c_uint,
        c_uint,
        c_ulong,
        c_ulong,
    ) -> c_ulong,
    destroy_window: unsafe extern "C" fn(*mut c_void, c_ulong) -> c_int,
    map_window: unsafe extern "C" fn(*mut c_void, c_ulong) -> c_int,
    map_raised: unsafe extern "C" fn(*mut c_void, c_ulong) -> c_int,
    unmap_window: unsafe extern "C" fn(*mut c_void, c_ulong) -> c_int,
    resize_window: unsafe extern "C" fn(*mut c_void, c_ulong, c_uint, c_uint) -> c_int,
    store_name: unsafe extern "C" fn(*mut c_void, c_ulong, *const c_char) -> c_int,
    intern_atom: unsafe extern "C" fn(*mut c_void, *const c_char, c_int) -> c_ulong,
    set_wm_protocols: unsafe extern "C" fn(*mut c_void, c_ulong, *mut c_ulong, c_int) -> c_int,
    select_input: unsafe extern "C" fn(*mut c_void, c_ulong, c_long) -> c_int,
    flush: unsafe extern "C" fn(*mut c_void) -> c_int,
    pending: unsafe extern "C" fn(*mut c_void) -> c_int,
    next_event: unsafe extern "C" fn(*mut c_void, *mut XEvent) -> c_int,
}

static X11: OnceLock<Option<X11Lib>> = OnceLock::new();

fn get_x11() -> Option<&'static X11Lib> {
    X11.get_or_init(|| {
        let lib = unsafe {
            libloading::Library::new("libX11.so.6")
                .or_else(|_| libloading::Library::new("libX11.so"))
        }.ok()?;
        unsafe {
            let init_threads: Option<unsafe extern "C" fn() -> c_int> =
                lib.get::<unsafe extern "C" fn() -> c_int>(b"XInitThreads\0").ok().map(|s| *s);
            let open_display = *lib.get(b"XOpenDisplay\0").ok()?;
            let close_display = *lib.get(b"XCloseDisplay\0").ok()?;
            let default_root_window = *lib.get(b"XDefaultRootWindow\0").ok()?;
            let create_simple_window = *lib.get(b"XCreateSimpleWindow\0").ok()?;
            let destroy_window = *lib.get(b"XDestroyWindow\0").ok()?;
            let map_window = *lib.get(b"XMapWindow\0").ok()?;
            let map_raised = *lib.get(b"XMapRaised\0").ok()?;
            let unmap_window = *lib.get(b"XUnmapWindow\0").ok()?;
            let resize_window = *lib.get(b"XResizeWindow\0").ok()?;
            let store_name = *lib.get(b"XStoreName\0").ok()?;
            let intern_atom = *lib.get(b"XInternAtom\0").ok()?;
            let set_wm_protocols = *lib.get(b"XSetWMProtocols\0").ok()?;
            let select_input = *lib.get(b"XSelectInput\0").ok()?;
            let flush = *lib.get(b"XFlush\0").ok()?;
            let pending = *lib.get(b"XPending\0").ok()?;
            let next_event = *lib.get(b"XNextEvent\0").ok()?;

            if let Some(init) = init_threads {
                init();
            }

            Some(X11Lib {
                _lib: lib,
                init_threads,
                open_display,
                close_display,
                default_root_window,
                create_simple_window,
                destroy_window,
                map_window,
                map_raised,
                unmap_window,
                resize_window,
                store_name,
                intern_atom,
                set_wm_protocols,
                select_input,
                flush,
                pending,
                next_event,
            })
        }
    }).as_ref()
}

pub struct Window {
    display: *mut c_void,
    window: c_ulong,
    wm_delete: c_ulong,
    pub width: u32,
    pub height: u32,
    pub open: bool,
}

impl Window {
    pub fn new(title: &str) -> Option<Self> {
        let x = get_x11()?;
        let display = unsafe { (x.open_display)(std::ptr::null()) };
        if display.is_null() {
            return None;
        }
        let root = unsafe { (x.default_root_window)(display) };
        let width = 640u32;
        let height = 480u32;
        // Background color: dark graphite (0x1c1c1e) matching Dōri dark theme
        let bg = 0x1c1c1eu64;
        let window = unsafe {
            (x.create_simple_window)(
                display,
                root,
                100,
                100,
                width as c_uint,
                height as c_uint,
                0,
                0,
                bg as c_ulong,
            )
        };
        if window == 0 {
            unsafe { (x.close_display)(display) };
            return None;
        }

        let title_c = CString::new(title).unwrap_or_default();
        unsafe {
            (x.store_name)(display, window, title_c.as_ptr());
            let mut wm_delete = (x.intern_atom)(display, c"WM_DELETE_WINDOW".as_ptr(), 0);
            (x.set_wm_protocols)(display, window, &mut wm_delete, 1);
            (x.select_input)(display, window, STRUCTURE_NOTIFY_MASK | EXPOSURE_MASK);
            (x.flush)(display);
        }

        let wm_delete = unsafe { (x.intern_atom)(display, c"WM_DELETE_WINDOW".as_ptr(), 0) };

        Some(Window {
            display,
            window,
            wm_delete,
            width,
            height,
            open: false,
        })
    }

    #[inline]
    pub fn parent(&self) -> *mut c_void {
        self.window as *mut c_void
    }

    pub fn set_plugin_size(&mut self, width: u32, height: u32) {
        self.width = width.max(100);
        self.height = height.max(100);
        if let Some(x) = get_x11() {
            unsafe {
                (x.resize_window)(self.display, self.window, self.width as c_uint, self.height as c_uint);
                (x.flush)(self.display);
            }
        }
    }

    pub fn set_title(&mut self, title: &str) {
        if let Some(x) = get_x11() {
            let title_c = CString::new(title).unwrap_or_default();
            unsafe {
                (x.store_name)(self.display, self.window, title_c.as_ptr());
                (x.flush)(self.display);
            }
        }
    }

    pub fn set_power(&mut self, _power: bool) {}

    pub fn show(&mut self) {
        if let Some(x) = get_x11() {
            unsafe {
                (x.map_raised)(self.display, self.window);
                (x.flush)(self.display);
            }
            self.open = true;
        }
    }

    pub fn hide(&mut self) {
        if let Some(x) = get_x11() {
            unsafe {
                (x.unmap_window)(self.display, self.window);
                (x.flush)(self.display);
            }
            self.open = false;
        }
    }

    pub fn tick(&mut self, mut on_close: impl FnMut()) {
        let Some(x) = get_x11() else { return; };
        unsafe {
            while (x.pending)(self.display) > 0 {
                let mut ev = std::mem::zeroed::<XEvent>();
                (x.next_event)(self.display, &mut ev);
                if ev.event_type() == CLIENT_MESSAGE && ev.client_data0() == self.wm_delete {
                    on_close();
                }
            }
        }
    }
}

impl Drop for Window {
    fn drop(&mut self) {
        if let Some(x) = get_x11() {
            unsafe {
                (x.destroy_window)(self.display, self.window);
                (x.close_display)(self.display);
            }
        }
    }
}

pub fn run(mut tick: Box<dyn FnMut()>) {
    loop {
        tick();
        std::thread::sleep(Duration::from_millis(4));
    }
}
