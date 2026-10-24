// SPDX-License-Identifier: GPL-3.0-only
//! EGL presentation for the video renderer: one EGL display and context, one window surface
//! per output. libmpv renders the decoded (hardware, dmabuf-imported) frame straight into the
//! surface and `eglSwapBuffers` hands the compositor a dmabuf, so no pixel crosses the CPU.
//! Unavailable on hosts without a real GPU; the caller then falls back to shared memory.
use glutin::{
    config::{Config, ConfigTemplateBuilder},
    context::{ContextApi, ContextAttributesBuilder, PossiblyCurrentContext},
    display::{Display, DisplayApiPreference},
    prelude::*,
    surface::{Surface, SurfaceAttributesBuilder, SwapInterval, WindowSurface},
};
use raw_window_handle::{
    RawDisplayHandle, RawWindowHandle, WaylandDisplayHandle, WaylandWindowHandle,
};
use std::{
    ffi::{CStr, CString, c_char, c_void},
    num::NonZeroU32,
    ptr::NonNull,
};

/// Whether the host has a GPU render node at all. Without one, Mesa would only offer a
/// software rasteriser, and loading it costs tens of MiB for nothing.
pub fn render_node_present() -> bool {
    std::fs::read_dir("/dev/dri").is_ok_and(|entries| {
        entries
            .flatten()
            .any(|entry| entry.file_name().to_string_lossy().starts_with("renderD"))
    })
}

pub struct Gl {
    display: Display,
    config: Config,
    context: PossiblyCurrentContext,
    wl_display: *mut c_void,
    /// Boxed so libmpv's `get_proc_address` context keeps a stable address.
    proc_display: Box<Display>,
}

/// An EGL window surface and the pixel size it was created or last resized to.
pub struct Window {
    surface: Surface<WindowSurface>,
    pub size: (u32, u32),
}

impl Gl {
    /// Create the display, a config and a context, and make the context current on a first
    /// window surface (EGL needs one). `wl_display` and `wl_surface` are the live libwayland
    /// proxies, which must outlive the returned objects.
    ///
    /// # Safety
    /// Both pointers must be valid libwayland objects that outlive `Gl` and its windows.
    pub unsafe fn new(
        wl_display: *mut c_void,
        wl_surface: *mut c_void,
        size: (u32, u32),
    ) -> Result<(Self, Window), String> {
        let display_handle = RawDisplayHandle::Wayland(WaylandDisplayHandle::new(
            NonNull::new(wl_display).ok_or("no native display")?,
        ));
        // SAFETY: the caller keeps the Wayland connection alive.
        let display = unsafe { Display::new(display_handle, DisplayApiPreference::Egl) }
            .map_err(|e| format!("EGL display: {e}"))?;
        let template = ConfigTemplateBuilder::new().build();
        // SAFETY: enumeration does not touch a native window.
        let config = unsafe { display.find_configs(template) }
            .map_err(|e| format!("EGL configs: {e}"))?
            .next()
            .ok_or("no EGL config")?;
        let window = window_handle(wl_surface)?;
        // Desktop GL first (mpv's best path), GLES as the alternative.
        let mut context = None;
        for api in [ContextApi::OpenGl(None), ContextApi::Gles(None)] {
            let attributes = ContextAttributesBuilder::new()
                .with_context_api(api)
                .build(Some(window));
            // SAFETY: config and window belong to this display.
            if let Ok(created) = unsafe { display.create_context(&config, &attributes) } {
                context = Some(created);
                break;
            }
        }
        let context = context.ok_or("no EGL context")?;
        let surface = create_surface(&display, &config, window, size)?;
        let context = context
            .make_current(&surface)
            .map_err(|e| format!("EGL make current: {e}"))?;
        // Frame callbacks pace the output; a swap must never wait for one.
        let _ = surface.set_swap_interval(&context, SwapInterval::DontWait);
        let gl = Self {
            proc_display: Box::new(display.clone()),
            display,
            config,
            context,
            wl_display,
        };
        let renderer = gl.renderer();
        if ["llvmpipe", "softpipe", "software"]
            .iter()
            .any(|marker| renderer.to_lowercase().contains(marker))
        {
            return Err(format!("software GL adapter ({renderer})"));
        }
        eprintln!("bead-wallpaper-video: EGL presentation, GL_RENDERER={renderer}");
        Ok((gl, Window { surface, size }))
    }

    fn renderer(&self) -> String {
        let address = self.display.get_proc_address(c"glGetString");
        if address.is_null() {
            return String::new();
        }
        // SAFETY: the standard glGetString signature on a current context; GL_RENDERER is 0x1F01.
        unsafe {
            let get: unsafe extern "system" fn(u32) -> *const c_char = std::mem::transmute(address);
            let text = get(0x1F01);
            if text.is_null() {
                String::new()
            } else {
                CStr::from_ptr(text).to_string_lossy().into_owned()
            }
        }
    }

    pub fn wl_display(&self) -> *mut c_void {
        self.wl_display
    }

    /// libmpv's `get_proc_address` callback target (`ctx` is the `Display`).
    pub fn proc_context(&self) -> *mut c_void {
        std::ptr::from_ref(&*self.proc_display) as *mut c_void
    }

    /// # Safety
    /// `wl_surface` must be a live libwayland proxy that outlives the window.
    pub unsafe fn window(
        &self,
        wl_surface: *mut c_void,
        size: (u32, u32),
    ) -> Result<Window, String> {
        let surface = create_surface(
            &self.display,
            &self.config,
            window_handle(wl_surface)?,
            size,
        )?;
        Ok(Window { surface, size })
    }

    /// Bind the context to `window`, resizing its buffers first when `size` changed.
    pub fn bind(&self, window: &mut Window, size: (u32, u32)) -> bool {
        if window.size != size {
            let (Some(w), Some(h)) = (NonZeroU32::new(size.0), NonZeroU32::new(size.1)) else {
                return false;
            };
            window.surface.resize(&self.context, w, h);
            window.size = size;
        }
        if self.context.make_current(&window.surface).is_err() {
            return false;
        }
        let _ = window
            .surface
            .set_swap_interval(&self.context, SwapInterval::DontWait);
        true
    }

    pub fn swap(&self, window: &Window) -> bool {
        window.surface.swap_buffers(&self.context).is_ok()
    }
}

fn window_handle(wl_surface: *mut c_void) -> Result<RawWindowHandle, String> {
    Ok(RawWindowHandle::Wayland(WaylandWindowHandle::new(
        NonNull::new(wl_surface).ok_or("no native surface")?,
    )))
}

fn create_surface(
    display: &Display,
    config: &Config,
    window: RawWindowHandle,
    size: (u32, u32),
) -> Result<Surface<WindowSurface>, String> {
    let (Some(w), Some(h)) = (NonZeroU32::new(size.0), NonZeroU32::new(size.1)) else {
        return Err("zero surface size".into());
    };
    let attributes = SurfaceAttributesBuilder::<WindowSurface>::new().build(window, w, h);
    // SAFETY: the caller keeps the wl_surface and the display alive.
    unsafe { display.create_window_surface(config, &attributes) }
        .map_err(|e| format!("EGL surface: {e}"))
}

/// `mpv_opengl_init_params::get_proc_address`.
///
/// # Safety
/// `ctx` is `Gl::proc_context()` of a live `Gl`; `name` is NUL-terminated.
pub unsafe extern "C" fn get_proc_address(ctx: *mut c_void, name: *const c_char) -> *mut c_void {
    // SAFETY: per the contract above.
    let (display, name) = unsafe { (&*(ctx as *const Display), CStr::from_ptr(name)) };
    let name = CString::from(name);
    display.get_proc_address(&name) as *mut c_void
}
