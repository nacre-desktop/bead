// SPDX-License-Identifier: GPL-3.0-only
//! EGL presentation for the video renderer: one EGL display and context, one window surface
//! per output. libmpv renders the decoded (hardware, dmabuf-imported) frame straight into the
//! surface and `eglSwapBuffers` hands the compositor a dmabuf, so no pixel crosses the CPU.
//! Unavailable on hosts without a real GPU; the caller then falls back to shared memory.
use glow::HasContext as _;
use glutin::{
    config::{Config, ConfigTemplateBuilder},
    context::{ContextApi, ContextAttributesBuilder, PossiblyCurrentContext, Version},
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
    glow: glow::Context,
    opacity: Option<(glow::Program, glow::VertexArray)>,
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
        let template = ConfigTemplateBuilder::new()
            .with_alpha_size(8)
            .with_transparency(true)
            .build();
        // SAFETY: enumeration does not touch a native window.
        let config = unsafe { display.find_configs(template) }
            .map_err(|e| format!("EGL configs: {e}"))?
            .next()
            .ok_or("no EGL config")?;
        let window = window_handle(wl_surface)?;
        // Desktop GL first (mpv's best path), GLES as the alternative.
        let mut context = None;
        for api in [
            ContextApi::OpenGl(None),
            ContextApi::Gles(Some(Version::new(3, 0))),
        ] {
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
        // SAFETY: the EGL context is current and the loader returns this display's GL entry points.
        let glow = unsafe {
            glow::Context::from_loader_function(|name| {
                CString::new(name).ok().map_or(std::ptr::null(), |name| {
                    display.get_proc_address(&name).cast()
                })
            })
        };
        let opacity = match opacity_pass(&glow) {
            Ok(pass) => Some(pass),
            Err(error) => {
                eprintln!("bead-wallpaper-video: GL opacity fade unavailable ({error})");
                None
            }
        };
        let gl = Self {
            proc_display: Box::new(display.clone()),
            display,
            config,
            context,
            wl_display,
            glow,
            opacity,
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

    pub fn has_opacity_fade(&self) -> bool {
        self.opacity.is_some()
    }

    /// Scale the current default framebuffer using constant-alpha blending.
    pub fn apply_opacity(&self, factor: f32, size: (u32, u32)) {
        let Some((program, vao)) = self.opacity else {
            return;
        };
        let gl = &self.glow;
        // SAFETY: callers have bound this Gl's current context and the cached objects belong to it.
        unsafe {
            let old_program =
                std::num::NonZeroU32::new(gl.get_parameter_i32(glow::CURRENT_PROGRAM) as u32)
                    .map(glow::NativeProgram);
            let old_vao =
                std::num::NonZeroU32::new(gl.get_parameter_i32(glow::VERTEX_ARRAY_BINDING) as u32)
                    .map(glow::NativeVertexArray);
            let old_draw_framebuffer = std::num::NonZeroU32::new(
                gl.get_parameter_i32(glow::DRAW_FRAMEBUFFER_BINDING) as u32,
            )
            .map(glow::NativeFramebuffer);
            let mut viewport = [0; 4];
            let mut scissor = [0; 4];
            let mut blend_color = [0.0; 4];
            let color_mask = gl.get_parameter_bool_array::<4>(glow::COLOR_WRITEMASK);
            gl.get_parameter_i32_slice(glow::VIEWPORT, &mut viewport);
            gl.get_parameter_i32_slice(glow::SCISSOR_BOX, &mut scissor);
            gl.get_parameter_f32_slice(glow::BLEND_COLOR, &mut blend_color);
            let blend = gl.is_enabled(glow::BLEND);
            let scissor_enabled = gl.is_enabled(glow::SCISSOR_TEST);
            let depth = gl.is_enabled(glow::DEPTH_TEST);
            let stencil = gl.is_enabled(glow::STENCIL_TEST);
            let cull = gl.is_enabled(glow::CULL_FACE);
            let src_rgb = gl.get_parameter_i32(glow::BLEND_SRC_RGB) as u32;
            let dst_rgb = gl.get_parameter_i32(glow::BLEND_DST_RGB) as u32;
            let src_alpha = gl.get_parameter_i32(glow::BLEND_SRC_ALPHA) as u32;
            let dst_alpha = gl.get_parameter_i32(glow::BLEND_DST_ALPHA) as u32;
            let eq_rgb = gl.get_parameter_i32(glow::BLEND_EQUATION_RGB) as u32;
            let eq_alpha = gl.get_parameter_i32(glow::BLEND_EQUATION_ALPHA) as u32;
            gl.bind_framebuffer(glow::DRAW_FRAMEBUFFER, None);
            gl.disable(glow::SCISSOR_TEST);
            gl.disable(glow::DEPTH_TEST);
            gl.disable(glow::STENCIL_TEST);
            gl.disable(glow::CULL_FACE);
            gl.color_mask(true, true, true, true);
            gl.viewport(0, 0, size.0 as i32, size.1 as i32);
            gl.blend_equation(glow::FUNC_ADD);
            gl.blend_func(glow::ZERO, glow::CONSTANT_ALPHA);
            gl.blend_color(0.0, 0.0, 0.0, factor.clamp(0.0, 1.0));
            gl.enable(glow::BLEND);
            gl.use_program(Some(program));
            gl.bind_vertex_array(Some(vao));
            gl.draw_arrays(glow::TRIANGLES, 0, 3);
            gl.bind_framebuffer(glow::DRAW_FRAMEBUFFER, old_draw_framebuffer);
            gl.bind_vertex_array(old_vao);
            gl.use_program(old_program);
            gl.blend_func_separate(src_rgb, dst_rgb, src_alpha, dst_alpha);
            gl.blend_equation_separate(eq_rgb, eq_alpha);
            gl.blend_color(
                blend_color[0],
                blend_color[1],
                blend_color[2],
                blend_color[3],
            );
            gl.color_mask(color_mask[0], color_mask[1], color_mask[2], color_mask[3]);
            gl.viewport(viewport[0], viewport[1], viewport[2], viewport[3]);
            gl.scissor(scissor[0], scissor[1], scissor[2], scissor[3]);
            for (cap, enabled) in [
                (glow::BLEND, blend),
                (glow::SCISSOR_TEST, scissor_enabled),
                (glow::DEPTH_TEST, depth),
                (glow::STENCIL_TEST, stencil),
                (glow::CULL_FACE, cull),
            ] {
                if enabled {
                    gl.enable(cap)
                } else {
                    gl.disable(cap)
                }
            }
        }
    }
}

fn opacity_pass(gl: &glow::Context) -> Result<(glow::Program, glow::VertexArray), String> {
    // SAFETY: opacity_pass runs during Gl::new with its EGL context current.
    let es = unsafe { gl.get_parameter_string(glow::VERSION) }.contains("OpenGL ES");
    let (version, precision) = if es {
        ("#version 300 es\n", "precision mediump float;")
    } else {
        ("#version 330 core\n", "")
    };
    let vertex = format!(
        "{version}void main() {{ vec2 p = vec2((gl_VertexID << 1) & 2, gl_VertexID & 2); gl_Position = vec4(p * 2.0 - 1.0, 0.0, 1.0); }}"
    );
    let fragment =
        format!("{version}{precision}out vec4 color; void main() {{ color = vec4(0.0); }}");
    // SAFETY: shader and VAO creation uses the current context owned by this GL wrapper.
    unsafe {
        let compile = |kind, source: &str| -> Result<glow::Shader, String> {
            let shader = gl.create_shader(kind)?;
            gl.shader_source(shader, source);
            gl.compile_shader(shader);
            if gl.get_shader_compile_status(shader) {
                Ok(shader)
            } else {
                let log = gl.get_shader_info_log(shader);
                gl.delete_shader(shader);
                Err(log)
            }
        };
        let vs = compile(glow::VERTEX_SHADER, &vertex)?;
        let fs = match compile(glow::FRAGMENT_SHADER, &fragment) {
            Ok(shader) => shader,
            Err(error) => {
                gl.delete_shader(vs);
                return Err(error);
            }
        };
        let program = match gl.create_program() {
            Ok(program) => program,
            Err(error) => {
                gl.delete_shader(vs);
                gl.delete_shader(fs);
                return Err(error);
            }
        };
        gl.attach_shader(program, vs);
        gl.attach_shader(program, fs);
        gl.link_program(program);
        gl.delete_shader(vs);
        gl.delete_shader(fs);
        if !gl.get_program_link_status(program) {
            let log = gl.get_program_info_log(program);
            gl.delete_program(program);
            return Err(log);
        }
        match gl.create_vertex_array() {
            Ok(vao) => Ok((program, vao)),
            Err(error) => {
                gl.delete_program(program);
                Err(error)
            }
        }
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
