// SPDX-License-Identifier: GPL-3.0-only
//! Minimal libmpv binding, loaded with dlopen so the build needs no libmpv and a missing
//! library only disables motion wallpapers. Two render APIs: the OpenGL one draws the
//! (hardware-decoded, dmabuf-imported) frame into the caller's current EGL surface; the
//! software one scales and converts into a caller-supplied (shared-memory) buffer.
#![allow(non_camel_case_types, clippy::missing_safety_doc)]
use bead_wallpaper::protocol::Fit;
use std::{
    ffi::{CStr, CString, c_char, c_double, c_int, c_void},
    sync::Arc,
};

#[repr(C)]
struct Event {
    id: c_int,
    error: c_int,
    reply_userdata: u64,
    data: *mut c_void,
}
#[repr(C)]
struct EndFile {
    reason: c_int,
    error: c_int,
}
#[repr(C)]
struct RenderParam {
    kind: c_int,
    data: *mut c_void,
}
const FORMAT_DOUBLE: c_int = 5;
const EVENT_SHUTDOWN: c_int = 1;
const EVENT_END_FILE: c_int = 7;
const EVENT_FILE_LOADED: c_int = 8;
const END_FILE_ERROR: c_int = 4;
const PARAM_API_TYPE: c_int = 1;
const PARAM_OPENGL_INIT: c_int = 2;
const PARAM_OPENGL_FBO: c_int = 3;
const PARAM_FLIP_Y: c_int = 4;
const PARAM_WL_DISPLAY: c_int = 9;
const PARAM_SKIP_RENDERING: c_int = 13;
const PARAM_SW_SIZE: c_int = 17;
const PARAM_SW_FORMAT: c_int = 18;
const PARAM_SW_STRIDE: c_int = 19;
const PARAM_SW_POINTER: c_int = 20;
const UPDATE_FRAME: u64 = 1;

#[repr(C)]
struct OpenglInit {
    get_proc_address: unsafe extern "C" fn(*mut c_void, *const c_char) -> *mut c_void,
    ctx: *mut c_void,
}
#[repr(C)]
struct OpenglFbo {
    fbo: c_int,
    w: c_int,
    h: c_int,
    internal_format: c_int,
}
/// What the OpenGL render API needs from the caller; the EGL context must be current
/// whenever a `Player` made with it renders or is dropped.
pub struct GlInit {
    pub get_proc_address: unsafe extern "C" fn(*mut c_void, *const c_char) -> *mut c_void,
    pub ctx: *mut c_void,
    pub wl_display: *mut c_void,
}

type Callback = unsafe extern "C" fn(*mut c_void);
#[derive(Clone, Copy)]
struct Api {
    lib: *mut c_void,
    create: unsafe extern "C" fn() -> *mut c_void,
    initialize: unsafe extern "C" fn(*mut c_void) -> c_int,
    destroy: unsafe extern "C" fn(*mut c_void),
    set_option: unsafe extern "C" fn(*mut c_void, *const c_char, *const c_char) -> c_int,
    set_property: unsafe extern "C" fn(*mut c_void, *const c_char, *const c_char) -> c_int,
    get_property: unsafe extern "C" fn(*mut c_void, *const c_char, c_int, *mut c_void) -> c_int,
    get_property_string: unsafe extern "C" fn(*mut c_void, *const c_char) -> *mut c_char,
    free: unsafe extern "C" fn(*mut c_void),
    command: unsafe extern "C" fn(*mut c_void, *mut *const c_char) -> c_int,
    wait_event: unsafe extern "C" fn(*mut c_void, c_double) -> *mut Event,
    set_wakeup: unsafe extern "C" fn(*mut c_void, Option<Callback>, *mut c_void),
    render_create: unsafe extern "C" fn(*mut *mut c_void, *mut c_void, *mut RenderParam) -> c_int,
    render_callback: unsafe extern "C" fn(*mut c_void, Option<Callback>, *mut c_void),
    render_update: unsafe extern "C" fn(*mut c_void) -> u64,
    render: unsafe extern "C" fn(*mut c_void, *mut RenderParam) -> c_int,
    render_free: unsafe extern "C" fn(*mut c_void),
}
/// Loads libmpv; the matching `dlclose` happens when the owning `Player` is dropped, which
/// unmaps the decoder libraries while the renderer only shows its last frame.
fn load() -> Result<Api, String> {
    // SAFETY: dlopen/dlsym with NUL-terminated names; closed in `Player::drop`.
    unsafe {
        let mut lib = std::ptr::null_mut();
        for name in [c"libmpv.so.2", c"libmpv.so.1", c"libmpv.so"] {
            lib = libc::dlopen(name.as_ptr(), libc::RTLD_NOW | libc::RTLD_LOCAL);
            if !lib.is_null() {
                break;
            }
        }
        if lib.is_null() {
            return Err("libmpv is not installed".into());
        }
        macro_rules! sym {
            ($name:literal) => {
                match symbol(lib, concat!($name, "\0")) {
                    Some(function) => function,
                    None => return Err(concat!("libmpv lacks ", $name).into()),
                }
            };
        }
        Ok(Api {
            lib,
            create: sym!("mpv_create"),
            initialize: sym!("mpv_initialize"),
            destroy: sym!("mpv_terminate_destroy"),
            set_option: sym!("mpv_set_option_string"),
            set_property: sym!("mpv_set_property_string"),
            get_property: sym!("mpv_get_property"),
            get_property_string: sym!("mpv_get_property_string"),
            free: sym!("mpv_free"),
            command: sym!("mpv_command"),
            wait_event: sym!("mpv_wait_event"),
            set_wakeup: sym!("mpv_set_wakeup_callback"),
            render_create: sym!("mpv_render_context_create"),
            render_callback: sym!("mpv_render_context_set_update_callback"),
            render_update: sym!("mpv_render_context_update"),
            render: sym!("mpv_render_context_render"),
            render_free: sym!("mpv_render_context_free"),
        })
    }
}
/// One exported function of `lib`, as the fn-pointer type the caller names.
///
/// # Safety
/// `T` must be the `extern "C"` fn-pointer type of the named libmpv function.
unsafe fn symbol<T: Copy>(lib: *mut c_void, name: &'static str) -> Option<T> {
    debug_assert!(name.ends_with('\0') && size_of::<T>() == size_of::<*mut c_void>());
    // SAFETY: NUL-terminated name; the pointer is reinterpreted as the requested fn pointer.
    unsafe {
        let pointer = libc::dlsym(lib, name.as_ptr().cast());
        (!pointer.is_null()).then(|| std::mem::transmute_copy::<*mut c_void, T>(&pointer))
    }
}
pub fn available() -> Result<(), String> {
    let api = load()?;
    // SAFETY: closes the handle opened by `load`.
    unsafe { libc::dlclose(api.lib) };
    Ok(())
}

#[derive(Debug, PartialEq, Eq)]
pub enum Notice {
    Loaded,
    /// Playback ended with an error or the library shut down.
    Failed(String),
}

/// The mpv `hwdec` value for a requested decoder: dmabuf-sharing modes with the GL renderer,
/// copy-back modes (decoded frames read back into system memory) with the software renderer.
fn hwdec_value(requested: &str, gl: bool) -> &str {
    match (requested, gl) {
        ("vaapi", false) => "vaapi-copy",
        ("auto", false) => "auto-copy",
        ("auto", true) => "auto-safe",
        (other, _) => other,
    }
}

/// One decoder instance. Dropping it frees every mpv resource.
pub struct Player {
    api: Api,
    handle: *mut c_void,
    context: *mut c_void,
    gl: bool,
    // Keeps the wakeup target alive for the callbacks registered above.
    _wake: Arc<calloop::ping::Ping>,
}
unsafe extern "C" fn wake(data: *mut c_void) {
    // SAFETY: `data` is `Arc::as_ptr` of a Ping held by the owning Player.
    unsafe { &*(data as *const calloop::ping::Ping) }.ping();
}
impl Player {
    /// `hwdec` is `vaapi`, `auto` or `no`. With `gl` the decoder hands frames to the GL
    /// renderer as dmabufs (zero copy); without it a copy-back mode renders into shared
    /// memory. A hardware mode that fails falls back to software decoding on its own.
    pub fn new(
        hwdec: &str,
        vaapi_device: Option<&str>,
        gl: Option<&GlInit>,
        wake_up: Arc<calloop::ping::Ping>,
    ) -> Result<Self, String> {
        let hwdec = hwdec_value(hwdec, gl.is_some());
        let api = load()?;
        // SAFETY: documented libmpv call sequence; every pointer comes from libmpv.
        unsafe {
            let handle = (api.create)();
            if handle.is_null() {
                libc::dlclose(api.lib);
                return Err("mpv_create failed".into());
            }
            let mut player = Self {
                api,
                handle,
                context: std::ptr::null_mut(),
                gl: gl.is_some(),
                _wake: wake_up,
            };
            // No user configuration, scripts, network helpers or playlist indirection.
            for (name, value) in [
                ("config", "no"),
                ("load-scripts", "no"),
                ("ytdl", "no"),
                ("osc", "no"),
                ("input-default-bindings", "no"),
                ("input-vo-keyboard", "no"),
                ("terminal", "no"),
                ("access-references", "no"),
                ("vo", "libmpv"),
                ("ao", "null"),
                ("aid", "no"),
                ("sid", "no"),
                ("osd-level", "0"),
                ("audio-display", "no"),
                ("idle", "yes"),
                ("keep-open", "yes"),
                ("cache", "no"),
                ("demuxer-max-bytes", "8MiB"),
                ("demuxer-max-back-bytes", "1MiB"),
                ("vd-lavc-threads", "2"),
                ("scale", "bilinear"),
                ("cscale", "bilinear"),
                ("dither-depth", "no"),
                ("hwdec", hwdec),
            ] {
                // Unknown options in older libmpv versions are ignored on purpose.
                player.option(name, value);
            }
            if gl.is_some() && hwdec == "vaapi" {
                // Avoid probing CUDA interop on Intel/AMD VA-API render nodes.
                player.option("gpu-hwdec-interop", "vaapi");
            }
            // Diagnostics only: a verbose libmpv log (hwdec selection, fallbacks) in a file.
            if let Ok(path) = std::env::var("BEAD_WALLPAPER_MPV_LOG") {
                player.option("log-file", &path);
                player.option("msg-level", "all=v");
            }
            if let Some(device) = vaapi_device {
                player.option("vaapi-device", device);
            }
            if (api.initialize)(handle) < 0 {
                return Err("mpv_initialize failed".into());
            }
            let mut init = gl.map(|gl| OpenglInit {
                get_proc_address: gl.get_proc_address,
                ctx: gl.ctx,
            });
            let mut params = vec![RenderParam {
                kind: PARAM_API_TYPE,
                data: if gl.is_some() { c"opengl" } else { c"sw" }.as_ptr() as *mut c_void,
            }];
            if let (Some(init), Some(gl)) = (init.as_mut(), gl) {
                params.push(RenderParam {
                    kind: PARAM_OPENGL_INIT,
                    data: (init as *mut OpenglInit).cast(),
                });
                // Lets the VA-API/dmabuf interop find the compositor's GPU.
                params.push(RenderParam {
                    kind: PARAM_WL_DISPLAY,
                    data: gl.wl_display,
                });
            }
            params.push(RenderParam {
                kind: 0,
                data: std::ptr::null_mut(),
            });
            let mut context = std::ptr::null_mut();
            if (api.render_create)(&mut context, handle, params.as_mut_ptr()) < 0 {
                return Err("render context unavailable".into());
            }
            player.context = context;
            let data = Arc::as_ptr(&player._wake) as *mut c_void;
            (api.set_wakeup)(handle, Some(wake), data);
            (api.render_callback)(context, Some(wake), data);
            Ok(player)
        }
    }
    fn option(&self, name: &str, value: &str) -> bool {
        let (Ok(name), Ok(value)) = (CString::new(name), CString::new(value)) else {
            return false;
        };
        // SAFETY: valid handle and NUL-terminated strings.
        unsafe { (self.api.set_option)(self.handle, name.as_ptr(), value.as_ptr()) >= 0 }
    }
    fn property(&self, name: &str, value: &str) -> bool {
        let (Ok(name), Ok(value)) = (CString::new(name), CString::new(value)) else {
            return false;
        };
        // SAFETY: valid handle and NUL-terminated strings.
        unsafe { (self.api.set_property)(self.handle, name.as_ptr(), value.as_ptr()) >= 0 }
    }
    /// Start `path` (an absolute local path) at `start_s`, with the fit and loop settings.
    pub fn load(&self, path: &str, fit: Fit, looped: bool, start_s: f64) -> Result<(), String> {
        if !path.starts_with('/') || path.contains('\0') || path.contains("://") {
            return Err("not an absolute local path".into());
        }
        let (panscan, keep_aspect) = match fit {
            Fit::Crop => ("1.0", "yes"),
            Fit::Fit => ("0.0", "yes"),
            Fit::Fill => ("0.0", "no"),
        };
        self.property("panscan", panscan);
        self.property("keepaspect", keep_aspect);
        self.property("loop-file", if looped { "inf" } else { "no" });
        self.property("start", &format!("{start_s:.3}"));
        let path = CString::new(path).map_err(|_| "invalid path")?;
        let mut args = [
            c"loadfile".as_ptr(),
            path.as_ptr(),
            c"replace".as_ptr(),
            std::ptr::null(),
        ];
        // SAFETY: NUL-terminated argv array whose strings outlive the call.
        let status = unsafe { (self.api.command)(self.handle, args.as_mut_ptr()) };
        if status < 0 {
            return Err(format!("loadfile failed ({status})"));
        }
        Ok(())
    }
    pub fn set_paused(&self, paused: bool) {
        self.property("pause", if paused { "yes" } else { "no" });
    }
    /// Stop demuxing and decoding while retaining libmpv and its render context.
    pub fn stop(&self) {
        let mut args = [c"stop".as_ptr(), std::ptr::null()];
        // SAFETY: NUL-terminated command array is valid for the duration of the call.
        unsafe { (self.api.command)(self.handle, args.as_mut_ptr()) };
    }
    pub fn position(&self) -> Option<f64> {
        let mut value: c_double = 0.0;
        // SAFETY: FORMAT_DOUBLE writes one double.
        let status = unsafe {
            (self.api.get_property)(
                self.handle,
                c"time-pos".as_ptr(),
                FORMAT_DOUBLE,
                (&mut value as *mut c_double).cast(),
            )
        };
        (status >= 0 && value.is_finite() && value >= 0.0).then_some(value)
    }
    /// The hardware decoder in use (`no` for software), once a file is loaded.
    pub fn hwdec(&self) -> Option<String> {
        // SAFETY: the returned string is copied and freed with mpv_free.
        unsafe {
            let raw = (self.api.get_property_string)(self.handle, c"hwdec-current".as_ptr());
            if raw.is_null() {
                return None;
            }
            let text = CStr::from_ptr(raw).to_string_lossy().into_owned();
            (self.api.free)(raw.cast());
            Some(text)
        }
    }
    pub fn notices(&self) -> Vec<Notice> {
        let mut out = Vec::new();
        loop {
            // SAFETY: wait_event with zero timeout never blocks; the event is valid until
            // the next call.
            let event = unsafe { &*(self.api.wait_event)(self.handle, 0.0) };
            match event.id {
                0 => return out,
                EVENT_SHUTDOWN => out.push(Notice::Failed("mpv shut down".into())),
                EVENT_FILE_LOADED => out.push(Notice::Loaded),
                EVENT_END_FILE => {
                    // SAFETY: END_FILE events carry an EndFile payload.
                    let end = unsafe { event.data.cast::<EndFile>().as_ref() };
                    if let Some(end) = end.filter(|e| e.reason == END_FILE_ERROR) {
                        out.push(Notice::Failed(format!("playback error {}", end.error)));
                    }
                }
                _ => {}
            }
        }
    }
    /// True when a new video frame is ready to be rendered (or skipped).
    pub fn frame_ready(&self) -> bool {
        // SAFETY: valid render context.
        unsafe { (self.api.render_update)(self.context) & UPDATE_FRAME != 0 }
    }
    pub fn is_gl(&self) -> bool {
        self.gl
    }
    /// Render the current frame into the current EGL surface (`width` x `height` pixels).
    pub fn render_gl(&self, width: u32, height: u32) -> bool {
        let mut fbo = OpenglFbo {
            fbo: 0,
            w: width as c_int,
            h: height as c_int,
            internal_format: 0,
        };
        let mut flip: c_int = 1;
        let mut params = [
            RenderParam {
                kind: PARAM_OPENGL_FBO,
                data: (&mut fbo as *mut OpenglFbo).cast(),
            },
            RenderParam {
                kind: PARAM_FLIP_Y,
                data: (&mut flip as *mut c_int).cast(),
            },
            RenderParam {
                kind: 0,
                data: std::ptr::null_mut(),
            },
        ];
        // SAFETY: valid GL render context with its EGL context current; params outlive the call.
        unsafe { (self.api.render)(self.context, params.as_mut_ptr()) >= 0 }
    }
    /// Render the current frame into `pixels` (bgr0, `stride` bytes per row).
    pub fn render(&self, width: u32, height: u32, stride: usize, pixels: &mut [u8]) -> bool {
        if pixels.len() < stride * height as usize || stride < width as usize * 4 {
            return false;
        }
        let mut size = [width as c_int, height as c_int];
        let mut stride = stride;
        let mut params = [
            RenderParam {
                kind: PARAM_SW_SIZE,
                data: size.as_mut_ptr().cast(),
            },
            RenderParam {
                kind: PARAM_SW_FORMAT,
                data: c"bgr0".as_ptr() as *mut c_void,
            },
            RenderParam {
                kind: PARAM_SW_STRIDE,
                data: (&mut stride as *mut usize).cast(),
            },
            RenderParam {
                kind: PARAM_SW_POINTER,
                data: pixels.as_mut_ptr().cast(),
            },
            RenderParam {
                kind: 0,
                data: std::ptr::null_mut(),
            },
        ];
        // SAFETY: the buffer is at least stride*height bytes and outlives the call.
        unsafe { (self.api.render)(self.context, params.as_mut_ptr()) >= 0 }
    }
    /// Consume the current frame without drawing it.
    pub fn skip(&self) {
        let mut yes: c_int = 1;
        let mut params = [
            RenderParam {
                kind: PARAM_SKIP_RENDERING,
                data: (&mut yes as *mut c_int).cast(),
            },
            RenderParam {
                kind: 0,
                data: std::ptr::null_mut(),
            },
        ];
        // SAFETY: valid context and parameter list.
        unsafe { (self.api.render)(self.context, params.as_mut_ptr()) };
    }
}
impl Drop for Player {
    fn drop(&mut self) {
        // SAFETY: the render context must be freed before the core; after both calls no
        // callback can fire, so the wakeup target may then be released.
        unsafe {
            if !self.context.is_null() {
                (self.api.render_callback)(self.context, None, std::ptr::null_mut());
                (self.api.render_free)(self.context);
            }
            (self.api.set_wakeup)(self.handle, None, std::ptr::null_mut());
            (self.api.destroy)(self.handle);
            libc::dlclose(self.api.lib);
            // Hand the decoder's freed heap back to the system.
            libc::malloc_trim(0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::hwdec_value;

    #[test]
    fn gpu_renderer_shares_frames_and_the_shm_renderer_copies_back() {
        assert_eq!(hwdec_value("vaapi", true), "vaapi");
        assert_eq!(hwdec_value("vaapi", false), "vaapi-copy");
        assert_eq!(hwdec_value("auto", true), "auto-safe");
        assert_eq!(hwdec_value("auto", false), "auto-copy");
        assert_eq!(hwdec_value("no", true), "no");
        assert_eq!(hwdec_value("no", false), "no");
    }
}
