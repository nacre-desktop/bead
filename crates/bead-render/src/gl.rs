// SPDX-License-Identifier: MIT
//! Shared native EGL context, retained subtree rasters and direct vector primitives.
use crate::tree::{Shift, placement};
use crate::{Damage, Rect, RenderTarget, Renderer, Result, Scene, ShmTarget};
use glutin::{
    config::{Config, ConfigTemplateBuilder},
    context::{
        AsRawContext, ContextApi, ContextAttributesBuilder, NotCurrentContext, NotCurrentGlContext,
        PossiblyCurrentContext, Version,
    },
    display::{AsRawDisplay, Display, DisplayApiPreference},
    prelude::*,
    surface::{
        AsRawSurface, Surface as EglSurface, SurfaceAttributesBuilder, SwapInterval, WindowSurface,
    },
};
use raw_window_handle::{RawDisplayHandle, RawWindowHandle};
use rgb::FromSlice;
use std::num::NonZeroU32;
type DamageSwap = unsafe extern "system" fn(
    *const std::ffi::c_void,
    *const std::ffi::c_void,
    *const i32,
    i32,
) -> u32;
pub struct Transport {
    pub display: Display,
    config: Config,
    context: Option<PossiblyCurrentContext>,
    /// B0398: the upload thread's shared context, made by the preparation, until `uploader`.
    upload: Option<NotCurrentContext>,
    swap_damage: unsafe extern "system" fn(
        *const std::ffi::c_void,
        *const std::ffi::c_void,
        *const i32,
        i32,
    ) -> u32,
    /// EGL_KHR_surfaceless_context: the upload thread's shared context needs it (B0317).
    surfaceless: bool,
    /// B0334: process-unique per transport (so per context): a kept canvas names its context.
    pub generation: u64,
    /// B0376: the prewarm's canvas until the first renderer drawing here claims it (B0406: one per
    /// wallpaper output the preparation was made for; `take_warm` claims one).
    pub warm: std::cell::Cell<Vec<WarmCanvas>>,
}
/// B0406: claim one of a transport's prewarmed canvases, if any is left.
pub(crate) fn take_warm<C>(warm: &std::cell::Cell<Vec<C>>) -> Option<C> {
    let mut canvases = warm.take();
    let canvas = canvases.pop();
    warm.set(canvases);
    canvas
}
/// B0376: the B0342 prewarm's canvas: its programs are compiled and have drawn once on this
/// context, so the renderer that claims it compiles nothing on its first frame. Never dropped (a
/// drop deletes GL names in whatever context is current); an unclaimed one goes with its context.
// shortcut: an unclaimed canvas leaks its small heap; drop it bound if prewarms go unclaimed often.
pub struct WarmCanvas(std::mem::ManuallyDrop<femtovg::Canvas<femtovg::renderer::OpenGl>>);
// SAFETY: the canvas moves whole with its context, which is current on no thread while it moves,
// and no clone of its Rc state lives outside it.
unsafe impl Send for WarmCanvas {}
fn next_generation() -> u64 {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}
/// EGL display/config/context setup that can be created on a worker thread and
/// handed to the render thread before any window surface exists.
///
/// The context is not current, so this type can move between threads without
/// an unsafe `Send` implementation. Keep the native connection alive until this
/// value is converted or shut down. Serialize initialization against other
/// setup for the same native display; EGL driver initialization is not performed
/// concurrently by this API.
pub struct PreparedTransport {
    // Keep context/config before display so normal field drop order releases
    // EGL children before the display handle.
    context: Option<NotCurrentContext>,
    upload: Option<NotCurrentContext>,
    config: Config,
    pub display: Display,
    swap_damage: DamageSwap,
    surfaceless: bool,
    warm: Vec<WarmCanvas>,
}
fn assert_send<T: Send>() {}
const _: fn() = assert_send::<PreparedTransport>;
pub struct Surface {
    surface: EglSurface<WindowSurface>,
    height: std::cell::Cell<u32>,
}
impl Drop for Transport {
    fn drop(&mut self) {
        // EGL defers deletion of a context that is still current. Unbind it
        // before destroying the last owner so idle policy really frees it.
        if let Some(context) = self.context.take() {
            if let Err(error) = context.make_not_current_in_place() {
                eprintln!("GL_RELEASE_ERROR {error}");
            }
            drop(context);
        }
    }
}
impl PreparedTransport {
    /// Prepare the EGL display, config, extension function, and non-current
    /// OpenGL 3.3 context without requiring a window handle. `upload` also makes
    /// the context `Transport::uploader` shares (B0398), when surfaceless contexts exist; `canvases`
    /// prewarms that many canvases (B0406: one per wallpaper output to prime, at least one).
    ///
    /// # Safety
    /// The native display/connection must remain alive until this value and all
    /// surfaces created from it are destroyed. Do not initialize another
    /// transport concurrently for the same native display; the caller must
    /// serialize setup and later convert this value into the sole `Transport`.
    pub unsafe fn new(display: RawDisplayHandle, upload: bool, canvases: usize) -> Result<Self> {
        let (display, config, swap_damage, supports_surfaceless) =
            unsafe { initialize_display(display) }?;
        let attrs = ContextAttributesBuilder::new()
            .with_context_api(ContextApi::OpenGl(Some(Version::new(3, 3))))
            .build(None);
        // SAFETY: config belongs to this retained EGL display; no native window is needed.
        let context = match unsafe { display.create_context(&config, &attrs) } {
            Ok(context) => context,
            Err(error) => {
                drop(config);
                if let Err(cleanup_error) = unsafe { terminate_exclusive(&display) } {
                    eprintln!(
                        "GL_PREPARE_CLEANUP_ERROR context_error={error} cleanup_error={cleanup_error}"
                    );
                }
                return Err(Box::new(error));
            }
        };
        let (context, warm) = if supports_surfaceless {
            match warm_surfaceless(context, &display, canvases) {
                Ok((context, warm)) => (context, warm),
                Err(error) => {
                    drop(config);
                    if let Err(cleanup_error) = unsafe { terminate_exclusive(&display) } {
                        eprintln!(
                            "GL_PREPARE_CLEANUP_ERROR context_error={error} cleanup_error={cleanup_error}"
                        );
                    }
                    return Err(error);
                }
            }
        } else {
            (context, Vec::new())
        };
        // Best effort: without it `uploader` makes the shared context on its caller's thread.
        let upload = (upload && supports_surfaceless)
            .then(|| shared_context(&display, &config, &context).ok())
            .flatten();
        Ok(Self {
            context: Some(context),
            upload,
            config,
            display,
            swap_damage,
            surfaceless: supports_surfaceless,
            warm,
        })
    }

    /// Convert setup into the render-thread transport. The context remains
    /// non-current until `create_surface` binds it to the first window.
    pub fn into_transport(mut self) -> Transport {
        let context = self
            .context
            .take()
            .expect("prepared EGL context already consumed")
            .treat_as_possibly_current();
        Transport {
            display: self.display,
            config: self.config,
            context: Some(context),
            upload: self.upload.take(),
            swap_damage: self.swap_damage,
            surfaceless: self.surfaceless,
            generation: next_generation(),
            warm: std::mem::take(&mut self.warm).into(),
        }
    }

    /// Release an unused prepared context and the EGL display pool.
    ///
    /// # Safety
    /// No other transport, surface, renderer, or library may use this native
    /// display. The prepared context is destroyed before `eglTerminate` runs.
    pub unsafe fn shutdown_exclusive(self) -> Result<()> {
        let Self {
            context,
            upload,
            config,
            display,
            ..
        } = self;
        drop(upload);
        drop(context);
        drop(config);
        unsafe { terminate_exclusive(&display) }
    }
}
/// B0398: a context sharing `shared`'s objects, for the upload thread.
fn shared_context(
    display: &Display,
    config: &Config,
    shared: &impl AsRawContext,
) -> Result<NotCurrentContext> {
    let attrs = ContextAttributesBuilder::new()
        .with_context_api(ContextApi::OpenGl(Some(Version::new(3, 3))))
        .with_sharing(shared)
        .build(None);
    // SAFETY: config belongs to this display; the caller joins the thread that makes the context
    // current before the display is terminated.
    unsafe { display.create_context(config, &attrs) }.map_err(Into::into)
}
/// # Safety
/// The caller must retain the native display for the returned EGL objects.
unsafe fn initialize_display(
    raw_display: RawDisplayHandle,
) -> Result<(Display, Config, DamageSwap, bool)> {
    // SAFETY: the public caller guarantees the native display remains alive.
    let display = unsafe { Display::new(raw_display, DisplayApiPreference::Egl) }?;
    let template = ConfigTemplateBuilder::new()
        .with_alpha_size(8)
        // Femtovg's path fill and stroke pipeline uses stencil, but this
        // renderer does not use a depth buffer. Ask EGL to admit zero-depth
        // configs, then rank the available configs below.
        .with_depth_size(0)
        .with_stencil_size(8)
        .with_transparency(true)
        .build();
    // SAFETY: config enumeration does not access a native window.
    let config = unsafe { display.find_configs(template) }?
        .min_by_key(|config| (config.depth_size(), config.num_samples()))
        .ok_or("no transparent EGL config")?;
    bead_core::vprintln!(
        "GL_CONFIG alpha={} depth={} stencil={} samples={} transparency={:?}",
        config.alpha_size(),
        config.depth_size(),
        config.stencil_size(),
        config.num_samples(),
        config.supports_transparency()
    );
    let query = display.get_proc_address(c"eglQueryString");
    if query.is_null() {
        return Err("missing EGL extension query".into());
    }
    let raw_display = match display.raw_display() {
        glutin::display::RawDisplay::Egl(display) => display,
        #[allow(unreachable_patterns)]
        _ => return Err("native EGL required".into()),
    };
    // SAFETY: initialized EGL display; EGL_EXTENSIONS returns a driver-owned string.
    let query: unsafe extern "system" fn(*const std::ffi::c_void, i32) -> *const std::ffi::c_char =
        unsafe { std::mem::transmute(query) };
    // SAFETY: raw_display belongs to this live Display, and 0x3055 is EGL_EXTENSIONS.
    let extensions = unsafe { query(raw_display, 0x3055) };
    if extensions.is_null() {
        return Err("missing EGL extension string".into());
    }
    // SAFETY: validated non-null driver-owned extension string.
    let extensions = unsafe { std::ffi::CStr::from_ptr(extensions) }.to_string_lossy();
    let supports_surfaceless = extensions
        .split_whitespace()
        .any(|extension| extension == "EGL_KHR_surfaceless_context");
    let name = if extensions
        .split_whitespace()
        .any(|e| e == "EGL_KHR_swap_buffers_with_damage")
    {
        c"eglSwapBuffersWithDamageKHR"
    } else if extensions
        .split_whitespace()
        .any(|e| e == "EGL_EXT_swap_buffers_with_damage")
    {
        c"eglSwapBuffersWithDamageEXT"
    } else {
        return Err("EGL damage swaps unavailable; use CPU fallback".into());
    };
    let address = display.get_proc_address(name);
    if address.is_null() {
        return Err("missing EGL damage swap function".into());
    }
    // SAFETY: both advertised extensions use the standard EGL damage-swap signature.
    let swap_damage: DamageSwap = unsafe { std::mem::transmute(address) };
    Ok((display, config, swap_damage, supports_surfaceless))
}

fn warm_surfaceless(
    context: NotCurrentContext,
    display: &Display,
    count: usize,
) -> Result<(NotCurrentContext, Vec<WarmCanvas>)> {
    match context {
        NotCurrentContext::Egl(context) => {
            let current = context.make_current_surfaceless()?;
            let warm = || -> Result<_> {
                let _span = bead_core::profile::Span::new("panel_shader_prewarm");
                // SAFETY: this worker owns the current context. The canvas leaves with it
                // (B0376), as a WarmCanvas.
                let renderer = unsafe {
                    femtovg::renderer::OpenGl::new_from_function_cstr(|name| {
                        display.get_proc_address(name).cast()
                    })
                }?;
                let mut canvas = femtovg::Canvas::new(renderer)?;
                // B0342: best effort; a failed warm-up draw never costs the GL path.
                let _ = warm_draw(&mut canvas, display);
                Ok(WarmCanvas(std::mem::ManuallyDrop::new(canvas)))
            };
            let warmed = (0..count.max(1))
                .map(|_| warm())
                .collect::<Result<Vec<_>>>();
            // Unbind even if shader setup failed, before the caller retires EGL.
            let context = current.make_not_current()?;
            Ok((NotCurrentContext::Egl(context), warmed?))
        }
        #[allow(unreachable_patterns)]
        _ => Err("surfaceless warmup requires EGL".into()),
    }
}

/// B0342: one draw with each femtovg program `GlRenderer::paint` uses (FillColor fill and stencil
/// stroke, the concave fill's Stencil pass, FillImage, the anti-alias-off TextureCopyUnclipped
/// blit; it draws no gradients or text) into a tiny image, so the context's first-draw driver work
/// (draw-time shader variants, texture upload and clear paths, first submit) runs on this worker.
/// The images go with the canvas, which deletes them on drop.
fn warm_draw(
    canvas: &mut femtovg::Canvas<femtovg::renderer::OpenGl>,
    display: &Display,
) -> Result<()> {
    let finish: unsafe extern "system" fn() = gl_proc(display, c"glFinish")?;
    let get_error: unsafe extern "system" fn() -> u32 = gl_proc(display, c"glGetError")?;
    let flags = femtovg::ImageFlags::PREMULTIPLIED;
    let target = canvas.create_image_empty(
        16,
        16,
        femtovg::PixelFormat::Rgba8,
        flags | femtovg::ImageFlags::FLIP_Y,
    )?;
    let pixels = [255u8; 16 * 16 * 4];
    let source = canvas.create_image(
        femtovg::ImageSource::Rgba(imgref::Img::new(pixels.as_rgba(), 16, 16)),
        flags,
    )?;
    canvas.set_size(16, 16, 1.);
    canvas.set_render_target(femtovg::RenderTarget::Image(target));
    canvas.clear_rect(0, 0, 16, 16, femtovg::Color::rgba(0, 0, 0, 0));
    let mut shape = femtovg::Path::new();
    shape.rounded_rect(1., 1., 10., 10., 3.);
    let mut paint = femtovg::Paint::color(femtovg::Color::rgba(255, 255, 255, 255));
    canvas.fill_path(&shape, &paint);
    paint.set_line_width(2.);
    canvas.stroke_path(&shape, &paint);
    let mut concave = femtovg::Path::new();
    concave.move_to(0., 0.);
    for (x, y) in [(16., 0.), (8., 8.), (16., 16.), (0., 16.)] {
        concave.line_to(x, y);
    }
    concave.close();
    canvas.fill_path(&concave, &paint);
    let mut rect = femtovg::Path::new();
    rect.rect(0., 0., 16., 16.);
    let mut image = femtovg::Paint::image(source, 0., 0., 16., 16., 0., 1.);
    canvas.fill_path(&rect, &image);
    image.set_anti_alias(false);
    canvas.fill_path(&rect, &image);
    canvas.flush();
    // SAFETY: this worker's context is current; GL defines both signatures.
    unsafe {
        finish();
        // Leave no error flag behind for the first frame's staging check.
        for _ in 0..8 {
            if get_error() == 0 {
                break;
            }
        }
    }
    Ok(())
}

/// # Safety
/// The caller must have exclusive ownership of the EGL display.
unsafe fn terminate_exclusive(display: &Display) -> Result<()> {
    #[allow(unreachable_patterns)]
    let raw = match display.raw_display() {
        glutin::display::RawDisplay::Egl(raw) => raw,
        _ => return Err("exclusive shutdown requires EGL".into()),
    };
    let address = display.get_proc_address(c"eglTerminate");
    if address.is_null() {
        return Err("missing eglTerminate".into());
    }
    // SAFETY: EGL exports this signature; the caller guarantees exclusive ownership.
    let terminate: unsafe extern "system" fn(*const std::ffi::c_void) -> u32 =
        unsafe { std::mem::transmute(address) };
    if unsafe { terminate(raw) } == 0 {
        return Err("eglTerminate failed".into());
    }
    Ok(())
}
pub struct GlTarget<'a> {
    pub display: &'a Display,
    pub width: u32,
    pub height: u32,
    /// The transport's `generation`.
    pub context: u64,
    /// The transport's `warm`.
    pub warm: &'a std::cell::Cell<Vec<WarmCanvas>>,
}
impl Transport {
    /// Release a process-owned EGL display, including the driver's display pool.
    ///
    /// # Safety
    /// Every renderer, context and surface using this native display must already
    /// be destroyed. No other library may own EGL objects on this display.
    pub unsafe fn shutdown_exclusive(mut self) -> Result<()> {
        drop(self.upload.take());
        if let Some(context) = self.context.take() {
            context.make_not_current_in_place()?;
            drop(context);
        }
        unsafe { terminate_exclusive(&self.display) }
    }
    /// B0398: the inverse of `PreparedTransport::into_transport`: the context is made not current
    /// on this thread, so the value can move to another one for `shutdown_exclusive`.
    pub fn into_prepared(mut self) -> Result<PreparedTransport> {
        let context = self
            .context
            .take()
            .map(|c| c.make_not_current())
            .transpose()?;
        Ok(PreparedTransport {
            context,
            upload: self.upload.take(),
            config: self.config.clone(),
            display: self.display.clone(),
            swap_damage: self.swap_damage,
            surfaceless: self.surfaceless,
            warm: self.warm.take(),
        })
    }
    /// # Safety
    /// Display must remain alive until the context and all surfaces are destroyed.
    pub unsafe fn new(display: RawDisplayHandle) -> Result<Self> {
        let (display, config, swap_damage, surfaceless) = unsafe { initialize_display(display) }?;
        Ok(Self {
            display,
            config,
            context: None,
            upload: None,
            swap_damage,
            surfaceless,
            generation: next_generation(),
            warm: Vec::new().into(),
        })
    }
    /// B0317: start the upload thread on a context shared with this transport's (the prepared one
    /// when there is one, B0398). It lives until `Uploader::shutdown`, which must run before the
    /// transport is shut down.
    pub fn uploader(&mut self, done: impl Fn(Uploaded) + Send + 'static) -> Result<Uploader> {
        // shortcut: no pbuffer fallback for drivers without surfaceless contexts, the caller
        // keeps uploading on the main thread; add one if such a driver shows up.
        if !self.surfaceless {
            return Err("EGL_KHR_surfaceless_context missing".into());
        }
        let context = match self.upload.take() {
            Some(context) => context,
            None => shared_context(
                &self.display,
                &self.config,
                self.context.as_ref().ok_or("missing EGL context")?,
            )?,
        };
        Uploader::spawn(self.display.clone(), context, Box::new(done))
    }
    /// B0325: make the shared context before any window surface exists, so the upload thread can
    /// share it while the first run is still being primed.
    pub fn ensure_context(&mut self) -> Result<()> {
        if self.context.is_some() {
            return Ok(());
        }
        if !self.surfaceless {
            return Err("EGL_KHR_surfaceless_context missing".into());
        }
        let attrs = ContextAttributesBuilder::new()
            .with_context_api(ContextApi::OpenGl(Some(Version::new(3, 3))))
            .build(None);
        // SAFETY: config belongs to this retained display.
        let context = unsafe { self.display.create_context(&self.config, &attrs) }?;
        #[allow(unreachable_patterns)]
        match context {
            NotCurrentContext::Egl(context) => {
                self.context = Some(PossiblyCurrentContext::Egl(
                    context.make_current_surfaceless()?,
                ));
            }
            _ => return Err("surfaceless context requires EGL".into()),
        }
        Ok(())
    }
    /// # Safety
    /// Window must outlive the returned surface, and belong to this display.
    pub unsafe fn create_surface(
        &mut self,
        window: RawWindowHandle,
        width: u32,
        height: u32,
    ) -> Result<Surface> {
        let attrs = SurfaceAttributesBuilder::<WindowSurface>::new().build(
            window,
            NonZeroU32::new(width).ok_or("zero width")?,
            NonZeroU32::new(height).ok_or("zero height")?,
        );
        // SAFETY: the caller retains wl_surface and the display for this lifetime.
        let surface = unsafe { self.display.create_window_surface(&self.config, &attrs) }?;
        if let Some(context) = &self.context {
            context.make_current(&surface)?;
        } else {
            let attrs = ContextAttributesBuilder::new()
                .with_context_api(ContextApi::OpenGl(Some(Version::new(3, 3))))
                .build(Some(window));
            // SAFETY: config and window belong to this retained display.
            let context = unsafe { self.display.create_context(&self.config, &attrs) }?;
            self.context = Some(context.make_current(&surface)?);
        }
        let address = self.display.get_proc_address(c"glGetString");
        if address.is_null() {
            return Err("missing glGetString".into());
        }
        // SAFETY: current desktop GL context provides this standard function signature.
        let get_string: unsafe extern "system" fn(u32) -> *const std::ffi::c_char =
            unsafe { std::mem::transmute(address) };
        // SAFETY: GL_RENDERER returns a context-owned, NUL-terminated static string.
        let renderer = unsafe { get_string(0x1F01) };
        if renderer.is_null() {
            return Err("missing GL renderer identity".into());
        }
        // SAFETY: validated GL string above remains alive in the current context.
        let name = unsafe { std::ffi::CStr::from_ptr(renderer) }.to_string_lossy();
        println!("GPU_INFO api=OpenGL renderer={name}");
        // BEAD_GL_ALLOW_SOFTWARE is the private-host test knob; production keeps the CPU path.
        if std::env::var_os("BEAD_GL_ALLOW_SOFTWARE").is_none()
            && ["llvmpipe", "softpipe", "software"]
                .iter()
                .any(|marker| name.to_lowercase().contains(marker))
        {
            return Err("software GL adapter; use tiny-skia fallback".into());
        }
        // Each wl_surface.frame callback grants that surface its next draw.
        // Waiting here would serialize surfaces on the shared drawing thread.
        surface.set_swap_interval(
            self.context.as_ref().ok_or("missing EGL context")?,
            SwapInterval::DontWait,
        )?;
        println!("GPU_SHARED devices=1 device_id=0 api=OpenGL");
        Ok(Surface {
            surface,
            height: std::cell::Cell::new(height),
        })
    }
    pub fn activate(&self, surface: &Surface, w: u32, h: u32) -> Result<()> {
        let context = self.context.as_ref().ok_or("missing EGL context")?;
        // EGL can acquire its backbuffer on bind; resize the native window first.
        surface.surface.resize(
            context,
            NonZeroU32::new(w).ok_or("zero width")?,
            NonZeroU32::new(h).ok_or("zero height")?,
        );
        context.make_current(&surface.surface)?;
        surface.height.set(h);
        Ok(())
    }
    /// Unbind before destroying window surfaces so EGL can release their buffers.
    pub fn unbind(&self) -> Result<()> {
        if let Some(context) = &self.context {
            context.make_not_current_in_place()?;
        }
        Ok(())
    }
    /// B0334: make the context current without a window, so a closed panel's kept renderer can
    /// delete its GL objects after its EGL surface is gone.
    pub fn bind_surfaceless(&self) -> Result<()> {
        #[allow(unreachable_patterns)]
        match self.context.as_ref() {
            Some(PossiblyCurrentContext::Egl(context)) if self.surfaceless => {
                Ok(context.make_current_surfaceless()?)
            }
            _ => Err("surfaceless context unavailable".into()),
        }
    }
    pub fn present(&self, surface: &Surface, damage: &[Damage]) -> Result<()> {
        if damage.is_empty() {
            return Err("empty EGL damage must use a callback-only commit".into());
        }
        let display = match self.display.raw_display() {
            glutin::display::RawDisplay::Egl(display) => display,
            #[allow(unreachable_patterns)]
            _ => return Err("native EGL display missing".into()),
        };
        let window = match surface.surface.raw_surface() {
            glutin::surface::RawSurface::Egl(window) => window,
            #[allow(unreachable_patterns)]
            _ => return Err("native EGL surface missing".into()),
        };
        let mut rectangles = Vec::with_capacity(damage.len() * 4);
        for region in damage {
            rectangles.extend([
                i32::try_from(region.x)?,
                i32::try_from(
                    surface
                        .height
                        .get()
                        .checked_sub(region.y + region.h)
                        .ok_or("damage outside EGL surface")?,
                )?,
                i32::try_from(region.w)?,
                i32::try_from(region.h)?,
            ]);
        }
        // SAFETY: extension was advertised, handles are retained and current; rectangles
        // use EGL's bottom-left origin, with validated physical-buffer coordinates.
        let swap_profile = bead_core::profile::Span::new("gl_swap");
        let ok = unsafe {
            (self.swap_damage)(
                display,
                window,
                rectangles.as_ptr(),
                i32::try_from(damage.len())?,
            )
        };
        drop(swap_profile);
        if ok == 0 {
            return Err("EGL damage swap failed".into());
        }
        Ok(())
    }
}
/// How a wallpaper fader reveals its target image.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum WallpaperBlend {
    Fade,
    /// A soft-edged disc growing from `origin` (0..1 of the output) until it covers it.
    Grow {
        origin: (f32, f32),
    },
}
/// Textures alive in all faders and the upload thread (femtovg images and raw uploads): zero
/// once every wallpaper GL object was released (B0317).
static LIVE_TEXTURES: std::sync::atomic::AtomicIsize = std::sync::atomic::AtomicIsize::new(0);
pub fn live_textures() -> isize {
    LIVE_TEXTURES.load(std::sync::atomic::Ordering::Relaxed)
}
fn count(delta: isize) {
    LIVE_TEXTURES.fetch_add(delta, std::sync::atomic::Ordering::Relaxed);
}
type GlDelete = unsafe extern "system" fn(i32, *const u32);
const GL_RGBA: u32 = 0x1908;
const GL_BGRA: u32 = 0x80E1;
fn gl_proc<T: Copy>(display: &Display, name: &std::ffi::CStr) -> Result<T> {
    let address = display.get_proc_address(name);
    if address.is_null() {
        return Err(format!("missing {}", name.to_string_lossy()).into());
    }
    assert_eq!(std::mem::size_of::<T>(), std::mem::size_of_val(&address));
    // SAFETY: T is the extern "system" signature GL defines for `name`; sizes checked above.
    Ok(unsafe { std::mem::transmute_copy(&address) })
}
/// B0347: GPU time of traced draws (`BEAD_FRAME_TRACE`), from two GL timestamp queries per draw.
/// Results are read on a later draw once available, never waited for; nothing runs when tracing
/// is off or a query proc is missing.
const GPU_RING: usize = 8;
const GL_TIMESTAMP: u32 = 0x8E28;
const GL_QUERY_RESULT: u32 = 0x8866;
const GL_QUERY_RESULT_AVAILABLE: u32 = 0x8867;
static GPU_TIMER_DISABLED: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);
struct QueryProcs {
    generate: unsafe extern "system" fn(i32, *mut u32),
    delete: GlDelete,
    counter: unsafe extern "system" fn(u32, u32),
    available: unsafe extern "system" fn(u32, u32, *mut i32),
    result: unsafe extern "system" fn(u32, u32, *mut u64),
}
/// Pending items, oldest first, capped at `GPU_RING`: a full ring drops its oldest and counts it.
struct Ring<T> {
    items: std::collections::VecDeque<T>,
    dropped: u64,
}
impl<T> Default for Ring<T> {
    fn default() -> Self {
        Self {
            items: std::collections::VecDeque::new(),
            dropped: 0,
        }
    }
}
impl<T> Ring<T> {
    fn push(&mut self, item: T) -> Option<T> {
        let old = (self.items.len() >= GPU_RING).then(|| {
            self.dropped += 1;
            self.items.pop_front()
        });
        self.items.push_back(item);
        old.flatten()
    }
    /// The oldest item if `ready`: later ones wait behind it, so results come out in order.
    fn take_ready(&mut self, ready: impl FnOnce(&T) -> bool) -> Option<T> {
        if self.items.front().is_some_and(ready) {
            self.items.pop_front()
        } else {
            None
        }
    }
}
struct Pending {
    queries: [u32; 2],
    label: String,
    draw: u64,
}
#[derive(Default)]
struct GpuTimer {
    procs: Option<QueryProcs>,
    ring: Ring<Pending>,
    open: Option<Pending>,
    draws: u64,
}
impl GpuTimer {
    /// The context must be current. Disables the probe for the process when a proc is missing.
    fn load(&mut self, display: &Display) {
        use std::sync::atomic::Ordering::Relaxed;
        if self.procs.is_some()
            || !bead_core::profile::frame_trace()
            || GPU_TIMER_DISABLED.load(Relaxed)
        {
            return;
        }
        let procs = (|| -> Result<QueryProcs> {
            Ok(QueryProcs {
                generate: gl_proc(display, c"glGenQueries")?,
                delete: gl_proc(display, c"glDeleteQueries")?,
                counter: gl_proc(display, c"glQueryCounter")?,
                available: gl_proc(display, c"glGetQueryObjectiv")?,
                result: gl_proc(display, c"glGetQueryObjectui64v")?,
            })
        })();
        match procs {
            Ok(procs) => self.procs = Some(procs),
            Err(error) => {
                if !GPU_TIMER_DISABLED.swap(true, Relaxed) {
                    println!("GPU_TIMER disabled reason={error}");
                }
            }
        }
    }
    /// Stamp the start of a draw and emit the earlier draws that finished.
    fn begin(&mut self, stage: &'static str, label: impl FnOnce() -> String) {
        let Some(procs) = self.procs.as_ref() else {
            return;
        };
        self.draws += 1;
        while let Some(done) = self.ring.take_ready(|p| {
            let mut ready = 0;
            // SAFETY: the context is current and `p.queries[1]` is a live query of it.
            unsafe { (procs.available)(p.queries[1], GL_QUERY_RESULT_AVAILABLE, &mut ready) };
            ready != 0
        }) {
            let mut ns = 0u64;
            // SAFETY: as above; the result is available, so this does not wait.
            unsafe { (procs.result)(done.queries[1], GL_QUERY_RESULT, &mut ns) };
            let mut start = 0u64;
            // SAFETY: as above.
            unsafe { (procs.result)(done.queries[0], GL_QUERY_RESULT, &mut start) };
            // SAFETY: the queries belong to this current context.
            unsafe { (procs.delete)(2, done.queries.as_ptr()) };
            bead_core::profile::trace(stage, || {
                format!(
                    "surface={} gpu_ms={:.3} frames_late={} dropped_total={}",
                    done.label,
                    ns.saturating_sub(start) as f64 / 1e6,
                    self.draws - done.draw,
                    self.ring.dropped
                )
            });
        }
        // A draw that failed before `end` left its start stamp behind.
        if let Some(stale) = self.open.take() {
            // SAFETY: the queries belong to this current context.
            unsafe { (procs.delete)(2, stale.queries.as_ptr()) };
        }
        let mut queries = [0u32; 2];
        // SAFETY: the context is current; two names are written.
        unsafe {
            (procs.generate)(2, queries.as_mut_ptr());
            (procs.counter)(queries[0], GL_TIMESTAMP);
        }
        self.open = Some(Pending {
            queries,
            label: label(),
            draw: self.draws,
        });
    }
    /// Stamp the end of the draw `begin` opened.
    fn end(&mut self) {
        let (Some(procs), Some(open)) = (self.procs.as_ref(), self.open.take()) else {
            return;
        };
        // SAFETY: the context is current and the query was generated by `begin`.
        unsafe { (procs.counter)(open.queries[1], GL_TIMESTAMP) };
        if let Some(old) = self.ring.push(open) {
            // SAFETY: as above.
            unsafe { (procs.delete)(2, old.queries.as_ptr()) };
        }
    }
    /// The context changed under the renderer: its query names mean nothing now.
    fn forget(&mut self) {
        self.ring.items.clear();
        self.open = None;
    }
}
impl Drop for GpuTimer {
    fn drop(&mut self) {
        if let Some(procs) = self.procs.as_ref() {
            for pending in self.ring.items.iter().chain(self.open.iter()) {
                // SAFETY: dropped with its renderer while the context is current, like its
                // other GL objects.
                unsafe { (procs.delete)(2, pending.queries.as_ptr()) };
            }
        }
    }
}
/// Pixels for a fader image: uploaded here, or a texture the upload thread already made (the
/// fader owns it from then on and deletes it).
pub enum Pixels<'a> {
    Rgba(&'a [u8]),
    Texture(u32),
}
/// A fader image: a femtovg texture, plus the raw GL texture femtovg does not delete itself.
struct Img {
    id: femtovg::ImageId,
    native: Option<u32>,
}
/// Two output-sized textures blended on the GPU. A fade can end at rest (`settle`): only the
/// shown image stays, and the next `retarget` brings the new one.
pub struct WallpaperFader {
    canvas: femtovg::Canvas<femtovg::renderer::OpenGl>,
    from: Option<Img>,
    to: Option<Img>,
    size: (u32, u32),
    last: (f32, WallpaperBlend),
    gl: UploadGl,
    gpu: GpuTimer,
    /// Wall milliseconds of `new`: the renderer and canvas, then the two uploads (B0316).
    pub setup_ms: (f64, f64),
}
impl WallpaperFader {
    /// The EGL context must be current (`Transport::activate`). Both uploads happen here, so the
    /// first moving frame costs like the others.
    pub fn new(transport: &Transport, size: (u32, u32), from: &[u8], to: &[u8]) -> Result<Self> {
        let began = std::time::Instant::now();
        let mut fader = Self::bare(transport, size)?;
        let canvas_ms = fader.setup_ms.0;
        // An error drops the fader, whose Drop frees whichever image exists by then.
        fader.from = Some(fader.make(Pixels::Rgba(from))?);
        fader.to = Some(fader.make(Pixels::Rgba(to))?);
        fader.setup_ms = (canvas_ms, began.elapsed().as_secs_f64() * 1e3 - canvas_ms);
        Ok(fader)
    }
    /// B0325: a fader at rest, as `settle` leaves one: only the shown image (`from`), no fade yet.
    pub fn at_rest(transport: &Transport, size: (u32, u32), shown: Pixels) -> Result<Self> {
        let mut fader = Self::bare(transport, size)?;
        fader.from = Some(fader.make(shown)?);
        Ok(fader)
    }
    fn bare(transport: &Transport, size: (u32, u32)) -> Result<Self> {
        let began = std::time::Instant::now();
        let display = &transport.display;
        let canvas = match take_warm(&transport.warm) {
            // B0398: the prewarm's canvas, as a renderer's first one (B0376): nothing compiles here.
            Some(WarmCanvas(canvas)) => std::mem::ManuallyDrop::into_inner(canvas),
            None => {
                // SAFETY: the caller made the transport's context current on this thread.
                let renderer = unsafe {
                    femtovg::renderer::OpenGl::new_from_function_cstr(|name| {
                        display.get_proc_address(name).cast()
                    })
                }?;
                femtovg::Canvas::new(renderer)?
            }
        };
        let mut gpu = GpuTimer::default();
        gpu.load(display);
        Ok(Self {
            canvas,
            gpu,
            from: None,
            to: None,
            size,
            last: (0., WallpaperBlend::Fade),
            gl: UploadGl::load(display)?,
            setup_ms: (began.elapsed().as_secs_f64() * 1e3, 0.),
        })
    }
    fn make(&mut self, pixels: Pixels) -> Result<Img> {
        let (w, h) = self.size;
        match pixels {
            Pixels::Rgba(rgba) => {
                if rgba.len() != w as usize * h as usize * 4 {
                    return Err("wallpaper texture size mismatch".into());
                }
                let id = self.canvas.create_image(
                    femtovg::ImageSource::Rgba(imgref::Img::new(
                        rgba.as_rgba(),
                        w as usize,
                        h as usize,
                    )),
                    femtovg::ImageFlags::empty(),
                )?;
                count(1);
                Ok(Img { id, native: None })
            }
            Pixels::Texture(texture) => self.wrap(texture, femtovg::ImageFlags::empty()),
        }
    }
    /// Give a raw GL texture to femtovg; the fader owns it (and deletes it) from then on.
    fn wrap(&mut self, texture: u32, flags: femtovg::ImageFlags) -> Result<Img> {
        let (w, h) = self.size;
        let info =
            femtovg::ImageInfo::new(flags, w as usize, h as usize, femtovg::PixelFormat::Rgba8);
        let handle = std::num::NonZeroU32::new(texture).ok_or("texture 0")?;
        // SAFETY: femtovg's native texture is glow's `NativeTexture(NonZeroU32)`; glow is
        // not a direct dependency here, and transmute checks that the sizes agree.
        let native = unsafe {
            std::mem::transmute::<
                std::num::NonZeroU32,
                <femtovg::renderer::OpenGl as femtovg::Renderer>::NativeTexture,
            >(handle)
        };
        match self.canvas.create_image_from_native_texture(native, info) {
            Ok(id) => Ok(Img {
                id,
                native: Some(texture),
            }),
            Err(error) => {
                self.gl.delete(texture);
                Err(error.into())
            }
        }
    }
    fn free(&mut self, image: Img) {
        self.canvas.delete_image(image.id);
        match image.native {
            Some(texture) => self.gl.delete(texture),
            None => count(-1),
        }
    }
    /// Textures held: two while fading, one at rest.
    pub fn textures(&self) -> usize {
        usize::from(self.from.is_some()) + usize::from(self.to.is_some())
    }
    fn paint(&mut self, blend: f32, mode: WallpaperBlend) {
        let (w, h) = (self.size.0 as f32, self.size.1 as f32);
        let canvas = &mut self.canvas;
        let mut full = femtovg::Path::new();
        full.rect(0., 0., w, h);
        let layer = |image, alpha| {
            let mut paint = femtovg::Paint::image(image, 0., 0., w, h, 0., alpha);
            paint.set_anti_alias(false);
            paint
        };
        let Some(from) = self.from.as_ref().map(|image| image.id) else {
            return;
        };
        canvas.fill_path(&full, &layer(from, 1.));
        let Some(to) = self.to.as_ref().map(|image| image.id) else {
            return;
        };
        match mode {
            WallpaperBlend::Fade => canvas.fill_path(&full, &layer(to, blend)),
            WallpaperBlend::Grow { origin } => {
                let (cx, cy) = (origin.0 * w, origin.1 * h);
                let far = [(0., 0.), (w, 0.), (0., h), (w, h)]
                    .iter()
                    .map(|(x, y)| (x - cx).hypot(y - cy))
                    .fold(0., f32::max);
                let feather = far * 0.15;
                let radius = blend * (far + feather);
                // Stacked translucent discs approximate a soft edge; the last one is opaque.
                for (step, alpha) in [(0., 0.3), (1., 0.3), (2., 0.3), (3., 1.)] {
                    let r = radius - feather * step / 3.;
                    if r > 0. {
                        let mut disc = femtovg::Path::new();
                        disc.circle(cx, cy, r);
                        canvas.fill_path(&disc, &layer(to, alpha));
                    }
                }
            }
        }
    }
    /// Draw the blend to the current EGL surface; the caller presents it.
    pub fn draw(&mut self, output: &str, blend: f32, mode: WallpaperBlend) {
        self.last = (blend, mode);
        self.gpu.begin("gpu_fade", || output.to_owned());
        self.canvas.set_size(self.size.0, self.size.1, 1.);
        self.canvas.set_render_target(femtovg::RenderTarget::Screen);
        self.paint(blend, mode);
        self.canvas.flush();
        self.gpu.end();
    }
    /// Make the last drawn blend the new `from` (at rest the shown image already is) and take
    /// `to`: a retarget with no jump.
    pub fn retarget(&mut self, to: Pixels) -> Result<()> {
        let to = self.make(to)?;
        if self.to.is_some() {
            // An uninitialised texture: femtovg's create_image_empty would zero-fill and upload
            // ~20 MB on this thread (B0387).
            let baked = self
                .gl
                .texture(self.size, std::ptr::null(), false, GL_RGBA)
                .map_err(Into::into)
                .and_then(|texture| {
                    self.wrap(
                        texture,
                        femtovg::ImageFlags::PREMULTIPLIED | femtovg::ImageFlags::FLIP_Y,
                    )
                });
            let baked = match baked {
                Ok(baked) => baked,
                Err(error) => {
                    self.free(to);
                    return Err(error);
                }
            };
            let target = baked.id;
            self.canvas
                .set_render_target(femtovg::RenderTarget::Image(target));
            // The texture starts undefined: define every pixel before the bake paints.
            self.canvas.clear_rect(
                0,
                0,
                self.size.0,
                self.size.1,
                femtovg::Color::rgba(0, 0, 0, 0),
            );
            let (blend, mode) = self.last;
            self.paint(blend, mode);
            self.canvas.flush();
            self.canvas.set_render_target(femtovg::RenderTarget::Screen);
            for old in [self.from.take(), self.to.take()].into_iter().flatten() {
                self.free(old);
            }
            self.from = Some(baked);
        }
        self.to = Some(to);
        Ok(())
    }
    /// The fade ended: `from` goes, `to` is the shown image and the next fade's `from`.
    pub fn settle(&mut self) {
        let Some(shown) = self.to.take() else {
            return;
        };
        if let Some(old) = self.from.replace(shown) {
            self.free(old);
        }
    }
}
impl Drop for WallpaperFader {
    fn drop(&mut self) {
        for image in [self.from.take(), self.to.take()].into_iter().flatten() {
            self.free(image);
        }
        self.canvas.flush();
    }
}

/// One finished upload of the worker: the pixels come back so the caller keeps them.
pub struct Uploaded {
    pub token: u64,
    pub rgba: Vec<u8>,
    pub texture: std::result::Result<u32, String>,
    pub ms: f64,
}
enum Job {
    Upload {
        token: u64,
        size: (u32, u32),
        rgba: Vec<u8>,
        bgra: bool,
        /// B0403: uploaded instead of `rgba`, in place.
        shared: Option<Shared>,
    },
    /// B0403: CPU work (an SHM write) in order with the uploads.
    Run(u64, Box<dyn FnOnce() + Send>),
    /// B0403: answered once every job queued before it has finished.
    Flush(std::sync::mpsc::Sender<()>),
    /// The main thread took the texture over and deletes it.
    Adopt(u32),
    /// A texture nobody wants any more (a superseded frame).
    Delete(u32),
}
/// B0403: bytes the upload thread uses in place (an SHM slot's), without a copy.
pub struct Shared(*mut u8, usize);
// SAFETY: `Shared::new`'s caller keeps the bytes valid and exclusive to the job.
unsafe impl Send for Shared {}
impl Shared {
    /// # Safety
    /// Until the job holding it has answered (or `Uploader::flush` or the drop returned), `bytes`
    /// stays mapped and nothing else in this process reads or writes it.
    pub unsafe fn new(bytes: &mut [u8]) -> Self {
        Self(bytes.as_mut_ptr(), bytes.len())
    }
    pub fn bytes(&mut self) -> &mut [u8] {
        // SAFETY: `new`'s contract.
        unsafe { std::slice::from_raw_parts_mut(self.0, self.1) }
    }
}
/// The one upload thread (B0317): a context shared with the transport's, current on it without a
/// surface. It owns every texture it made until the main thread adopts it, and deletes the ones
/// never adopted when it stops (drop).
pub struct Uploader {
    jobs: Option<std::sync::mpsc::Sender<Job>>,
    thread: Option<std::thread::JoinHandle<()>>,
}
struct UploadGl {
    gen_textures: unsafe extern "system" fn(i32, *mut u32),
    bind: unsafe extern "system" fn(u32, u32),
    parameter: unsafe extern "system" fn(u32, u32, i32),
    image:
        unsafe extern "system" fn(u32, i32, i32, i32, i32, i32, u32, u32, *const std::ffi::c_void),
    finish: unsafe extern "system" fn(),
    error: unsafe extern "system" fn() -> u32,
    delete: GlDelete,
}
impl UploadGl {
    fn load(display: &Display) -> Result<Self> {
        Ok(Self {
            gen_textures: gl_proc(display, c"glGenTextures")?,
            bind: gl_proc(display, c"glBindTexture")?,
            parameter: gl_proc(display, c"glTexParameteri")?,
            image: gl_proc(display, c"glTexImage2D")?,
            finish: gl_proc(display, c"glFinish")?,
            error: gl_proc(display, c"glGetError")?,
            delete: gl_proc(display, c"glDeleteTextures")?,
        })
    }
    /// Upload and wait until it is done (glFinish), so the main thread can sample the texture.
    fn upload(
        &self,
        (w, h): (u32, u32),
        rgba: &[u8],
        bgra: bool,
    ) -> std::result::Result<u32, String> {
        if rgba.len() != w as usize * h as usize * 4 {
            return Err("wallpaper texture size mismatch".into());
        }
        let format = if bgra { GL_BGRA } else { GL_RGBA };
        self.texture((w, h), rgba.as_ptr().cast(), true, format)
    }
    /// A w*h RGBA texture; `data` is w*h pixels in `format` (GL_RGBA or GL_BGRA, which GL
    /// reorders itself), or null for an uninitialised one. `wait` glFinishes so another context
    /// can sample it.
    fn texture(
        &self,
        (w, h): (u32, u32),
        data: *const std::ffi::c_void,
        wait: bool,
        format: u32,
    ) -> std::result::Result<u32, String> {
        const TEXTURE_2D: u32 = 0x0DE1;
        // LINEAR filters and CLAMP_TO_EDGE: femtovg's defaults for ImageFlags::empty().
        const PARAMETERS: [(u32, i32); 4] = [
            (0x2801, 0x2601),
            (0x2800, 0x2601),
            (0x2802, 0x812F),
            (0x2803, 0x812F),
        ];
        let mut texture = 0;
        // SAFETY: this thread's context is current; `data` is null or holds w*h RGBA pixels.
        let error = unsafe {
            (self.gen_textures)(1, &mut texture);
            (self.bind)(TEXTURE_2D, texture);
            for (name, value) in PARAMETERS {
                (self.parameter)(TEXTURE_2D, name, value);
            }
            (self.image)(
                TEXTURE_2D, 0, 0x8058, w as i32, h as i32, 0, format, 0x1401, data,
            );
            (self.bind)(TEXTURE_2D, 0);
            if wait {
                (self.finish)();
            }
            (self.error)()
        };
        count(1);
        if error != 0 {
            self.delete(texture);
            return Err(format!("glTexImage2D error {error:#x}"));
        }
        Ok(texture)
    }
    fn delete(&self, texture: u32) {
        // SAFETY: this thread's context is current.
        unsafe { (self.delete)(1, &texture) };
        count(-1);
    }
}
impl Uploader {
    fn spawn(
        display: Display,
        context: NotCurrentContext,
        done: Box<dyn Fn(Uploaded) + Send>,
    ) -> Result<Self> {
        let (jobs, queue) = std::sync::mpsc::channel::<Job>();
        let thread = std::thread::Builder::new()
            .name("bead-wallpaper-upload".into())
            .spawn(move || {
                // A setup failure is not fatal: every upload then answers with its reason and the
                // main thread falls back.
                let setup = (|| -> Result<_> {
                    let context = match context {
                        NotCurrentContext::Egl(context) => context,
                        #[allow(unreachable_patterns)]
                        _ => return Err("upload context requires EGL".into()),
                    };
                    let current = context.make_current_surfaceless()?;
                    Ok((current, UploadGl::load(&display)?))
                })();
                let mut outstanding = std::collections::BTreeSet::new();
                for job in queue {
                    match (job, &setup) {
                        (
                            Job::Upload {
                                token,
                                size,
                                rgba,
                                bgra,
                                mut shared,
                            },
                            setup,
                        ) => {
                            let began = std::time::Instant::now();
                            let pixels = shared.as_mut().map_or(&rgba[..], |s| &*s.bytes());
                            let texture = match setup {
                                Ok((_, gl)) => {
                                    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                                        gl.upload(size, pixels, bgra)
                                    }))
                                    .unwrap_or_else(|_| Err("upload panicked".into()))
                                }
                                Err(error) => Err(error.to_string()),
                            };
                            if let Ok(texture) = texture {
                                outstanding.insert(texture);
                            }
                            done(Uploaded {
                                token,
                                rgba,
                                texture,
                                ms: began.elapsed().as_secs_f64() * 1e3,
                            });
                        }
                        (Job::Run(token, work), _) => {
                            let began = std::time::Instant::now();
                            let work = std::panic::AssertUnwindSafe(work);
                            let texture = std::panic::catch_unwind(work)
                                .map(|()| 0)
                                .map_err(|_| "job panicked".to_string());
                            let ms = began.elapsed().as_secs_f64() * 1e3;
                            let rgba = Vec::new();
                            done(Uploaded {
                                token,
                                rgba,
                                texture,
                                ms,
                            });
                        }
                        (Job::Flush(answer), _) => {
                            let _ = answer.send(());
                        }
                        (Job::Adopt(texture), _) => {
                            outstanding.remove(&texture);
                        }
                        (Job::Delete(texture), Ok((_, gl))) => {
                            if outstanding.remove(&texture) {
                                gl.delete(texture);
                            }
                        }
                        (Job::Delete(_), Err(_)) => {}
                    }
                }
                if let Ok((current, gl)) = setup {
                    for texture in outstanding {
                        gl.delete(texture);
                    }
                    // SAFETY: still this thread's context.
                    unsafe { (gl.finish)() };
                    if let Err(error) = current.make_not_current() {
                        eprintln!("GL_RELEASE_ERROR {error}");
                    }
                }
            })?;
        Ok(Self {
            jobs: Some(jobs),
            thread: Some(thread),
        })
    }
    /// Queue an upload of RGBA pixels, or BGRA ones (an SHM buffer's, B0398): GL reorders them
    /// on the upload thread. The pixels come back when the thread is gone.
    pub fn upload(
        &self,
        token: u64,
        size: (u32, u32),
        rgba: Vec<u8>,
        bgra: bool,
    ) -> std::result::Result<(), Vec<u8>> {
        let job = Job::Upload {
            token,
            size,
            rgba,
            bgra,
            shared: None,
        };
        match self.jobs.as_ref().map(|jobs| jobs.send(job)) {
            Some(Err(std::sync::mpsc::SendError(Job::Upload { rgba, .. }))) => Err(rgba),
            _ => Ok(()),
        }
    }
    /// B0403: queue an upload of BGRA bytes in place (an SHM slot's): no copy, and the answer's
    /// `rgba` is empty. False when the thread is gone.
    pub fn upload_shared(&self, token: u64, size: (u32, u32), bytes: Shared) -> bool {
        let job = Job::Upload {
            token,
            size,
            rgba: Vec::new(),
            bgra: true,
            shared: Some(bytes),
        };
        self.jobs
            .as_ref()
            .is_some_and(|jobs| jobs.send(job).is_ok())
    }
    /// B0403: run CPU work (an SHM write) on this thread, answered in order with the uploads
    /// (`texture: Ok(0)`, or the panic). The work comes back when the thread is gone.
    pub fn run(
        &self,
        token: u64,
        work: Box<dyn FnOnce() + Send>,
    ) -> std::result::Result<(), Box<dyn FnOnce() + Send>> {
        match self
            .jobs
            .as_ref()
            .map(|jobs| jobs.send(Job::Run(token, work)))
        {
            Some(Err(std::sync::mpsc::SendError(Job::Run(_, work)))) => Err(work),
            _ => Ok(()),
        }
    }
    /// B0403: wait until every job queued before this call has finished.
    pub fn flush(&self) {
        let (answer, finished) = std::sync::mpsc::channel();
        if let Some(jobs) = &self.jobs
            && jobs.send(Job::Flush(answer)).is_ok()
        {
            let _ = finished.recv();
        }
    }
    /// The texture of a finished upload now belongs to the caller.
    pub fn adopt(&self, texture: u32) {
        if let Some(jobs) = &self.jobs {
            let _ = jobs.send(Job::Adopt(texture));
        }
    }
    /// Drop a finished upload nobody uses.
    pub fn discard(&self, texture: u32) {
        if let Some(jobs) = &self.jobs {
            let _ = jobs.send(Job::Delete(texture));
        }
    }
}
impl Drop for Uploader {
    /// Close the queue and join: the thread deletes what was never adopted and releases its
    /// context, so the transport can terminate the display afterwards.
    fn drop(&mut self) {
        self.jobs = None;
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
/// A texture's identity: an image payload's by its key, so every item drawing it shares one
/// texture (B0375); a layer raster's by item and scale.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Slot {
    Image(u64),
    Layer(String, u32),
}
impl Slot {
    fn of(item: &crate::tree::Item, scale: f32) -> Self {
        match &item.image {
            Some(payload) => Self::Image(payload.key),
            None => Self::Layer(item.id.clone(), scale.to_bits()),
        }
    }
    fn heap(&self) -> usize {
        match self {
            Self::Image(_) => 0,
            Self::Layer(id, _) => id.capacity(),
        }
    }
}
struct Texture {
    key: u64,
    /// An image payload's texture (not a layer raster): `release_gpu_resources` may keep it.
    payload: bool,
    image: femtovg::ImageId,
    width: u32,
    height: u32,
    touched: u64,
    _charge: bead_core::cache::Charge,
}
#[derive(Default)]
pub struct GlRenderer {
    cache: crate::tree::TreeCache,
    canvas: Option<femtovg::Canvas<femtovg::renderer::OpenGl>>,
    images: std::collections::BTreeMap<Slot, Texture>,
    serial: u64,
    group_budget: Option<bead_core::cache::Budget>,
    group_charge: Option<bead_core::cache::Charge>,
    group: Option<(femtovg::ImageId, u32, u32, u64)>,
    /// The transport generation the canvas draws with (B0334).
    context: Option<u64>,
    /// B0338: every texture's charge, apart from the views that baked the pixels.
    texture_budget: Option<bead_core::cache::Budget>,
    texture_peak: usize,
    gpu: GpuTimer,
    /// B0419: the scroll planes' shifts the next paints apply (empty: paint as baked).
    shifts: Vec<Shift>,
}
/// B0338: one `native-gl-images` budget per renderer: its textures live and die with its canvas.
const TEXTURE_LIMIT: usize = 64 * bead_core::cache::MIB;
/// B0338: the renderer's texture budget refused a texture and nothing was left to evict.
#[derive(Debug)]
pub struct TextureBudgetRefused {
    pub need: usize,
    pub cap: usize,
}
impl std::fmt::Display for TextureBudgetRefused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "native-gl-images texture budget exceeded need={} cap={}",
            self.need, self.cap
        )
    }
}
impl std::error::Error for TextureBudgetRefused {}
/// Reserve `bytes` on `budget`; on a refusal `evict` frees a texture until one fits or none is left.
fn reserve_texture(
    budget: &bead_core::cache::Budget,
    peak: &mut usize,
    bytes: usize,
    mut evict: impl FnMut() -> bool,
) -> Result<bead_core::cache::Charge> {
    loop {
        if let Ok(charge) = budget.reserve(bytes) {
            *peak = (*peak).max(budget.used());
            return Ok(charge);
        }
        if !evict() {
            let (_, cap) = budget.refusal(bytes);
            return Err(Box::new(TextureBudgetRefused { need: bytes, cap }));
        }
    }
}
/// Delete the least recently drawn texture outside `active` (the slots the frame draws); false when none is.
fn evict_lru(
    images: &mut std::collections::BTreeMap<Slot, Texture>,
    canvas: &mut femtovg::Canvas<impl femtovg::Renderer>,
    active: &std::collections::BTreeSet<Slot>,
) -> bool {
    let victim = images
        .iter()
        .filter(|(slot, _)| !active.contains(*slot))
        .min_by_key(|(_, texture)| texture.touched)
        .map(|(slot, _)| slot.clone());
    let Some(texture) = victim.and_then(|slot| images.remove(&slot)) else {
        return false;
    };
    canvas.delete_image(texture.image);
    true
}
/// The texture pass of one frame: upload what it draws, retire what it no longer does.
fn upload_textures<R: femtovg::Renderer>(
    images: &mut std::collections::BTreeMap<Slot, Texture>,
    canvas: &mut femtovg::Canvas<R>,
    budget: &bead_core::cache::Budget,
    peak: &mut usize,
    serial: u64,
    frame: &crate::tree::Frame,
) -> Result<()> {
    let active: std::collections::BTreeSet<_> = frame
        .items
        .iter()
        .filter(|item| item.raster.is_some() || item.image.is_some())
        .map(|item| Slot::of(item, frame.scale))
        .collect();
    let mut upload_uses = std::collections::BTreeMap::new();
    for payload in frame.items.iter().filter_map(|item| item.image.as_ref()) {
        *upload_uses.entry(payload.key).or_insert(0usize) += 1;
    }
    let upload_profile = bead_core::profile::Span::new("gl_image_upload");
    for item in &frame.items {
        if let Some(payload) = &item.image {
            let slot = Slot::of(item, frame.scale);
            if let Some(old) = images.get_mut(&slot) {
                old.touched = serial;
                GlRenderer::finish_image_upload(payload, &mut upload_uses)?;
                continue;
            }
            let upload = payload.upload.lock().map_err(|_| "image upload poisoned")?;
            let pixels = &upload.as_ref().ok_or("image upload not prepared")?.pixels;
            let (width, height) = (pixels.width(), pixels.height());
            let charge = reserve_texture(
                budget,
                peak,
                pixels.data().len() + slot.heap() + 192,
                || evict_lru(images, canvas, &active),
            )?;
            let result = canvas.create_image(
                femtovg::ImageSource::Rgba(imgref::Img::new(
                    pixels.data().as_rgba(),
                    width as usize,
                    height as usize,
                )),
                femtovg::ImageFlags::PREMULTIPLIED,
            );
            let image = match result {
                Ok(image) => image,
                Err(error) => {
                    return Err(error.into());
                }
            };
            drop(upload);
            images.insert(
                slot,
                Texture {
                    key: payload.key,
                    payload: true,
                    image,
                    width,
                    height,
                    touched: serial,
                    _charge: charge,
                },
            );
            GlRenderer::finish_image_upload(payload, &mut upload_uses)?;
            continue;
        }
        let Some(raster) = &item.raster else {
            continue;
        };
        let slot = Slot::of(item, frame.scale);
        let (w, h) = (raster.pixels.width(), raster.pixels.height());
        if images
            .get(&slot)
            .is_none_or(|old| old.width != w || old.height != h)
        {
            if let Some(old) = images.remove(&slot) {
                canvas.delete_image(old.image);
            }
            let charge = reserve_texture(
                budget,
                peak,
                w as usize * h as usize * 4 + slot.heap() + 192,
                || evict_lru(images, canvas, &active),
            )?;
            let image = canvas.create_image(
                femtovg::ImageSource::Rgba(imgref::Img::new(
                    raster.pixels.data().as_rgba(),
                    w as usize,
                    h as usize,
                )),
                femtovg::ImageFlags::PREMULTIPLIED,
            )?;
            images.insert(
                slot.clone(),
                Texture {
                    _charge: charge,
                    key: raster.key,
                    payload: false,
                    image,
                    width: w,
                    height: h,
                    touched: serial,
                },
            );
            bead_core::vprintln!(
                "PROFILE stage=layer_upload id={} pixels={} region=full",
                item.id,
                w * h
            );
        } else if let Some(old) = images.get_mut(&slot) {
            if old.key != raster.key {
                let changed = if raster.base_key == Some(old.key) {
                    raster.changed
                } else {
                    Damage::full(w, h)
                };
                // CPU damage may include a removed tail beyond the new
                // raster. Upload only texels inside the current texture;
                // size changes already reallocate it above.
                let changed = Damage {
                    x: changed.x.min(w),
                    y: changed.y.min(h),
                    w: changed.w.min(w.saturating_sub(changed.x)),
                    h: changed.h.min(h.saturating_sub(changed.y)),
                };
                if changed.w > 0 && changed.h > 0 {
                    let crop = raster
                        .pixels
                        .clone_rect(
                            tiny_skia::IntRect::from_xywh(
                                changed.x as i32,
                                changed.y as i32,
                                changed.w,
                                changed.h,
                            )
                            .ok_or("invalid upload rectangle")?,
                        )
                        .ok_or("upload rectangle outside raster")?;
                    canvas.update_image(
                        old.image,
                        femtovg::ImageSource::Rgba(imgref::Img::new(
                            crop.data().as_rgba(),
                            crop.width() as usize,
                            crop.height() as usize,
                        )),
                        changed.x as usize,
                        changed.y as usize,
                    )?;
                    bead_core::vprintln!(
                        "PROFILE stage=layer_upload id={} pixels={} region=partial",
                        item.id,
                        changed.w * changed.h
                    );
                }
                old.key = raster.key;
            }
            old.touched = serial;
        }
    }
    drop(upload_profile);
    // Retire absent slots only after every current image upload succeeds;
    // an upload error does not additionally discard old off-frame slots.
    // The size-based LRU otherwise retains visited scroll rows up to its
    // much larger cap.
    let stale: Vec<_> = images
        .keys()
        .filter(|slot| !active.contains(*slot))
        .cloned()
        .collect();
    for slot in stale {
        if let Some(texture) = images.remove(&slot) {
            canvas.delete_image(texture.image);
        }
    }
    while images.len() > 1024
        || images
            .values()
            .map(|t| t.width as usize * t.height as usize * 4)
            .sum::<usize>()
            > crate::tree::STEADY_RASTER_LIMIT
    {
        if !evict_lru(images, canvas, &active) {
            return Err("active GL images exceed budget".into());
        }
    }
    Ok(())
}
impl GlRenderer {
    fn texture_budget(&mut self) -> bead_core::cache::Budget {
        self.texture_budget
            .get_or_insert_with(|| bead_core::cache::Budget::new("native-gl-images", TEXTURE_LIMIT))
            .clone()
    }
    fn finish_image_upload(
        payload: &crate::tree::ImagePayload,
        uses: &mut std::collections::BTreeMap<u64, usize>,
    ) -> Result<()> {
        let remaining = uses
            .get_mut(&payload.key)
            .ok_or("image upload use missing")?;
        *remaining -= 1;
        if *remaining == 0 {
            drop(
                payload
                    .upload
                    .lock()
                    .map_err(|_| "image upload poisoned")?
                    .take(),
            );
        }
        Ok(())
    }
    fn upload(&mut self, display: &Display, warm: &std::cell::Cell<Vec<WarmCanvas>>) -> Result<()> {
        let frame = self.cache.frame.as_ref().ok_or("unprepared layer tree")?;
        for payload in frame.items.iter().filter_map(|item| item.image.as_ref()) {
            if let Some(error) = payload.failure() {
                return Err(Box::new(crate::tree::ResourcePreparationFailed(error)));
            }
        }
        let mut pending_keys = std::collections::BTreeSet::new();
        let pending = frame
            .items
            .iter()
            .filter_map(|item| {
                let payload = item.image.as_ref()?;
                (!self.images.contains_key(&Slot::of(item, frame.scale))
                    && payload.pending()
                    && pending_keys.insert(payload.key))
                .then(|| payload.clone())
            })
            .collect::<Vec<_>>();
        if !pending.is_empty() {
            return Err(Box::new(crate::tree::ResourcesPending { images: pending }));
        }

        claim_canvas(
            &mut self.canvas,
            || take_warm(warm).map(|WarmCanvas(canvas)| std::mem::ManuallyDrop::into_inner(canvas)),
            || {
                let _span = bead_core::profile::Span::new("gl_canvas_new");
                // SAFETY: Transport made its retained context current on this thread.
                let renderer = unsafe {
                    femtovg::renderer::OpenGl::new_from_function_cstr(|name| {
                        display.get_proc_address(name).cast()
                    })
                }?;
                Ok(femtovg::Canvas::new(renderer)?)
            },
        )?;
        bead_core::profile::split("gl_canvas");
        let budget = self.texture_budget();
        let frame = self.cache.frame.as_ref().ok_or("unprepared layer tree")?;
        let canvas = self.canvas.as_mut().ok_or("GL canvas missing")?;
        self.serial += 1;
        let uploaded = upload_textures(
            &mut self.images,
            canvas,
            &budget,
            &mut self.texture_peak,
            self.serial,
            frame,
        );
        bead_core::profile::split("gl_upload");
        uploaded
    }
    fn paint(
        canvas: &mut femtovg::Canvas<femtovg::renderer::OpenGl>,
        frame: &crate::tree::Frame,
        images: &std::collections::BTreeMap<Slot, Texture>,
        offset: (f32, f32),
        shifts: &[Shift],
    ) -> Result<()> {
        for (index, item) in frame.items.iter().enumerate() {
            let Some((shift, clip)) =
                placement(index, item.clip, crate::tree::rect(&item.primitive), shifts)
            else {
                continue;
            };
            canvas.reset_scissor();
            if let Some(clip) = clip {
                canvas.scissor(
                    (clip.x + offset.0) * frame.scale,
                    (clip.y + offset.1) * frame.scale,
                    clip.w * frame.scale,
                    clip.h * frame.scale,
                );
            }
            // The scissor is already placed (the plane clip stays put), so only the item moves.
            let offset = (offset.0 + shift.0, offset.1 + shift.1);
            let r = crate::tree::rect(&item.primitive);
            let opacity = item.primitive.opacity();
            let (x, y) = (
                (r.x + offset.0) * frame.scale,
                (r.y + offset.1) * frame.scale,
            );
            if let Some(payload) = &item.image {
                let texture = images
                    .get(&Slot::of(item, frame.scale))
                    .ok_or("GL image missing")?;
                let (w, h) = crate::dimensions(r, frame.scale);
                let (x, y) = (x.round(), y.round());
                let (w, h) = (w as f32, h as f32);
                let compact = payload
                    .compact
                    .lock()
                    .map_err(|_| "image layout poisoned")?;
                if let Some(layout) = compact.as_ref() {
                    for fill in &layout.fills {
                        let r = fill.rect;
                        let a = fill.atlas;
                        let mut path = femtovg::Path::new();
                        path.rect(x + r.x as f32, y + r.y as f32, r.w as f32, r.h as f32);
                        let mut paint = femtovg::Paint::image(
                            texture.image,
                            x + r.x as f32 - a.x as f32 * r.w as f32,
                            y + r.y as f32 - a.y as f32 * r.h as f32,
                            texture.width as f32 * r.w as f32,
                            texture.height as f32 * r.h as f32,
                            0.,
                            opacity,
                        );
                        paint.set_anti_alias(false);
                        canvas.fill_path(&path, &paint);
                    }
                    for region in &layout.regions {
                        let r = region.source;
                        let a = region.atlas;
                        let mut path = femtovg::Path::new();
                        path.rect(x + r.x as f32, y + r.y as f32, r.w as f32, r.h as f32);
                        let mut paint = femtovg::Paint::image(
                            texture.image,
                            x + r.x as f32 - a.x as f32,
                            y + r.y as f32 - a.y as f32,
                            texture.width as f32,
                            texture.height as f32,
                            0.,
                            opacity,
                        );
                        paint.set_anti_alias(false);
                        canvas.fill_path(&path, &paint);
                    }
                    continue;
                }
                let mut path = femtovg::Path::new();
                path.rect(x, y, w, h);
                let (px, py, pw, ph) = if payload.native_cover {
                    let zoom = (w / payload.width as f32).max(h / payload.height as f32);
                    let (pw, ph) = (payload.width as f32 * zoom, payload.height as f32 * zoom);
                    (x + (w - pw) * 0.5, y + (h - ph) * 0.5, pw, ph)
                } else {
                    (x, y, w, h)
                };
                canvas.fill_path(
                    &path,
                    &femtovg::Paint::image(texture.image, px, py, pw, ph, 0., opacity),
                );
            } else if let Some(raster) = &item.raster {
                let (x, y) = (
                    x.round() + raster.origin.0 as f32,
                    y.round() + raster.origin.1 as f32,
                );
                let texture = images
                    .get(&Slot::of(item, frame.scale))
                    .ok_or("GL layer image missing")?;
                let mut path = femtovg::Path::new();
                let (w, h) = (raster.pixels.width() as f32, raster.pixels.height() as f32);
                path.rect(x.round(), y.round(), w, h);
                canvas.fill_path(
                    &path,
                    &femtovg::Paint::image(texture.image, x.round(), y.round(), w, h, 0., opacity),
                );
            } else if let crate::Primitive::Arc {
                start_angle,
                sweep_angle,
                stroke_width,
                color,
                round_caps,
                ..
            } = &item.primitive
            {
                if color[3] == 0 {
                    continue;
                }
                if let Some(points) = crate::arc_points(
                    r.translated(offset.0, offset.1),
                    *stroke_width,
                    *start_angle,
                    *sweep_angle,
                ) {
                    let mut path = femtovg::Path::new();
                    path.move_to(points[0].0 * frame.scale, points[0].1 * frame.scale);
                    for &(px, py) in &points[1..] {
                        path.line_to(px * frame.scale, py * frame.scale);
                    }
                    if sweep_angle.abs() == 360. {
                        path.close();
                    }
                    let mut paint = femtovg::Paint::color(femtovg::Color::rgba(
                        color[0], color[1], color[2], color[3],
                    ));
                    paint.set_line_width(*stroke_width * frame.scale);
                    if *round_caps {
                        paint.set_line_cap(femtovg::LineCap::Round);
                    }
                    canvas.stroke_path(&path, &paint);
                }
            } else if let crate::Primitive::Polyline {
                points,
                closed,
                stroke_width,
                color,
                round_caps,
                fill,
                ..
            } = &item.primitive
            {
                if let Some(points) =
                    crate::polyline_points(r.translated(offset.0, offset.1), *stroke_width, points)
                {
                    let mut path = femtovg::Path::new();
                    path.move_to(points[0].0 * frame.scale, points[0].1 * frame.scale);
                    for &(px, py) in &points[1..] {
                        path.line_to(px * frame.scale, py * frame.scale);
                    }
                    if *closed {
                        path.close();
                    }
                    if let Some(fill) = fill.filter(|c| *closed && c[3] != 0) {
                        canvas.fill_path(
                            &path,
                            &femtovg::Paint::color(femtovg::Color::rgba(
                                fill[0], fill[1], fill[2], fill[3],
                            )),
                        );
                    }
                    if *stroke_width > 0. && color[3] != 0 {
                        let mut paint = femtovg::Paint::color(femtovg::Color::rgba(
                            color[0], color[1], color[2], color[3],
                        ));
                        paint.set_line_width(*stroke_width * frame.scale);
                        paint.set_line_join(femtovg::LineJoin::Round);
                        if *round_caps {
                            paint.set_line_cap(femtovg::LineCap::Round);
                        }
                        canvas.stroke_path(&path, &paint);
                    }
                }
            } else if let crate::Primitive::Rounded {
                radius,
                fill,
                border,
                ..
            } = &item.primitive
            {
                let mut path = femtovg::Path::new();
                path.rounded_rect(
                    x,
                    y,
                    r.w * frame.scale,
                    r.h * frame.scale,
                    radius.min(r.w / 2.).min(r.h / 2.) * frame.scale,
                );
                canvas.fill_path(
                    &path,
                    &femtovg::Paint::color(femtovg::Color::rgba(
                        fill[0], fill[1], fill[2], fill[3],
                    )),
                );
                if let Some((color, line)) = border {
                    let mut paint = femtovg::Paint::color(femtovg::Color::rgba(
                        color[0], color[1], color[2], color[3],
                    ));
                    paint.set_line_width(*line * frame.scale);
                    canvas.stroke_path(&path, &paint);
                }
            }
        }
        canvas.reset_scissor();
        Ok(())
    }
}

impl Renderer for GlRenderer {
    fn scroll_shifts(&mut self, shifts: &[(std::ops::Range<usize>, Rect, (f32, f32))]) -> bool {
        self.shifts = shifts.to_vec();
        true
    }
    /// Release GL image allocations while the renderer's EGL context is current.
    /// `TreeCache` is deliberately retained so the CPU rasterized layers can be
    /// uploaded again without rerasterizing them, and the canvas (its compiled
    /// shaders) so the next draw on this context compiles nothing (B0334). The
    /// canvas goes with the renderer: drop it while its context is current.
    fn release_gpu_resources(&mut self, keep_images: bool) {
        let bytes = |t: &Texture| t.width as usize * t.height as usize * 4;
        let image_bytes: usize = self.images.values().map(bytes).sum();
        let group_bytes = self
            .group
            .map_or(0, |(_, w, h, _)| u64::from(w) * u64::from(h) * 4);
        // Layer rasters upload again from `TreeCache`; image payloads would need the worker.
        // B0339: only those the last frame drew stay (a glide leaves earlier ones behind).
        let serial = self.serial;
        let (kept, released): (
            std::collections::BTreeMap<_, _>,
            std::collections::BTreeMap<_, _>,
        ) = std::mem::take(&mut self.images)
            .into_iter()
            .partition(|(_, texture)| keep_images && texture.payload && texture.touched == serial);
        println!(
            "GL_CACHE_RELEASE images={} image_bytes={image_bytes} composite_bytes={group_bytes} kept_image_bytes={}",
            kept.len() + released.len(),
            kept.values().map(bytes).sum::<usize>()
        );
        self.images = kept;
        if let Some(canvas) = self.canvas.as_mut() {
            for (_, texture) in released {
                canvas.delete_image(texture.image);
            }
            if let Some((image, _, _, _)) = self.group.take() {
                canvas.delete_image(image);
            }
        } else {
            self.group = None;
        }
        self.group_charge = None;
        // Keep both budgets so a later upload remains accounted against the
        // same process-wide cache accounting.
        self.cache.trim_idle();
    }

    fn prepared_frame(&self) -> Option<std::sync::Arc<crate::tree::Frame>> {
        self.cache.frame.clone()
    }
    fn raster_builds(&self) -> usize {
        self.cache.builds
    }
    fn resource_counts(&self) -> (usize, usize) {
        (
            self.cache.frame.as_ref().map_or(0, |f| f.layer_count),
            self.images.len() + usize::from(self.group.is_some()),
        )
    }
    fn take_texture_peak(&mut self) -> usize {
        let held = self
            .texture_budget
            .as_ref()
            .map_or(0, |budget| budget.used());
        std::mem::replace(&mut self.texture_peak, held).max(held)
    }
    fn stage_resources(&mut self, target: crate::ResourceTarget<'_>) -> Result<()> {
        #[allow(irrefutable_let_patterns)]
        let crate::ResourceTarget::Gl { display, warm } = target else {
            return Err("GL resource target missing".into());
        };
        self.upload(display, warm)?;
        let address = display.get_proc_address(c"glGetError");
        if address.is_null() {
            return Err("missing glGetError".into());
        }
        // SAFETY: a current desktop GL context supplies the standard signature.
        let get_error: unsafe extern "system" fn() -> u32 = unsafe { std::mem::transmute(address) };
        // SAFETY: this context remains current throughout staging.
        let error = unsafe { get_error() };
        if error != 0 {
            return Err(format!("GL resource staging error 0x{error:x}").into());
        }
        Ok(())
    }
    fn wants_gpu(&self) -> bool {
        true
    }
    fn prepare(&mut self, scene: &Scene, scale: f32) -> Result<()> {
        self.cache
            .set_image_policy(crate::tree::ImagePolicy::DeferredUpload);
        self.cache.prepare(scene, scale)
    }
    fn render(&mut self, _: &Scene, _: ShmTarget<'_>, _: Damage) -> Result<()> {
        Err("GL renderer needs a GPU surface".into())
    }
    fn draw(&mut self, scene: &Scene, target: RenderTarget<'_>, _: Damage) -> Result<()> {
        let RenderTarget::Gl(target) = target else {
            return Err("GL target missing".into());
        };
        // B0334: a kept canvas names its context's objects; under another transport those names
        // mean other objects, so forget it (its own context took them) instead of deleting.
        if self
            .context
            .is_some_and(|context| context != target.context)
        {
            std::mem::forget(self.canvas.take());
            self.images.clear();
            self.group = None;
            self.group_charge = None;
            self.gpu.forget();
        }
        self.context = Some(target.context);
        self.upload(target.display, target.warm)?;
        self.gpu.load(target.display);
        self.gpu
            .begin("gpu_draw", || format!("{}x{}", target.width, target.height));
        let frame = self.cache.frame.as_ref().ok_or("unprepared layer tree")?;
        let canvas = self.canvas.as_mut().ok_or("GL canvas missing")?;
        canvas.set_size(target.width, target.height, 1.);
        canvas.set_render_target(femtovg::RenderTarget::Screen);
        canvas.clear_rect(
            0,
            0,
            target.width,
            target.height,
            femtovg::Color::rgba(0, 0, 0, 0),
        );
        let fade = scene.motion.as_ref().is_some_and(|motion| motion.fade);
        let opening = fade && scene.motion.as_ref().is_some_and(|motion| motion.open);
        let group_current = self
            .group
            .is_some_and(|(_, w, h, key)| key == frame.key && (w, h) == group_size(scene, frame));
        let (build_group, composite_group) = group_use(scene.opacity, fade, opening, group_current);
        if !build_group && !composite_group {
            if scene.opacity >= 1. {
                Self::paint(canvas, frame, &self.images, scene.offset, &self.shifts)?;
            }
        } else {
            let (w, h) = group_size(scene, frame);
            if u64::from(w) * u64::from(h) * 4 > 64 * 1024 * 1024 {
                return Err("GL opacity group exceeds budget".into());
            }
            if self.group.is_none_or(|(_, ow, oh, _)| ow != w || oh != h) {
                if let Some((old, _, _, _)) = self.group.take() {
                    canvas.delete_image(old);
                }
                self.group_charge = None;
                // B0338: a texture, so never the view's budget.
                let budget = self.group_budget.get_or_insert_with(|| {
                    bead_core::cache::Budget::new("native-gl-composite", 64 * bead_core::cache::MIB)
                });
                self.group_charge = Some(budget.reserve(w as usize * h as usize * 4 + 64)?);
                self.group = Some((
                    canvas.create_image_empty(
                        w as usize,
                        h as usize,
                        femtovg::PixelFormat::Rgba8,
                        femtovg::ImageFlags::PREMULTIPLIED | femtovg::ImageFlags::FLIP_Y,
                    )?,
                    w,
                    h,
                    0,
                ));
            }
            let (image, _, _, key) = self.group.as_mut().ok_or("GL opacity group missing")?;
            if *key != frame.key {
                canvas.set_render_target(femtovg::RenderTarget::Image(*image));
                canvas.clear_rect(0, 0, w, h, femtovg::Color::rgba(0, 0, 0, 0));
                Self::paint(
                    canvas,
                    frame,
                    &self.images,
                    (-scene.bounds.x, -scene.bounds.y),
                    &self.shifts,
                )?;
                *key = frame.key;
            }
            canvas.set_render_target(femtovg::RenderTarget::Screen);
            if composite_group {
                // Whole pixels, so the settle frame matches a direct paint.
                let (x, y) = (
                    ((scene.offset.0 + scene.bounds.x) * frame.scale).round(),
                    ((scene.offset.1 + scene.bounds.y) * frame.scale).round(),
                );
                let mut path = femtovg::Path::new();
                path.rect(x, y, w as f32, h as f32);
                canvas.fill_path(
                    &path,
                    &femtovg::Paint::image(*image, x, y, w as f32, h as f32, 0., scene.opacity),
                );
            }
        }
        bead_core::profile::split("gl_paint");
        let flush_profile = bead_core::profile::Span::new("gl_flush");
        canvas.flush();
        drop(flush_profile);
        self.gpu.end();
        // A non-fading motion never samples an opacity group. A renderer reused
        // from a fading view must not keep that view's full-size target alive.
        if scene.opacity >= 1. && scene.motion.as_ref().is_some_and(|motion| !motion.fade) {
            if let Some((image, _, _, _)) = self.group.take() {
                canvas.delete_image(image);
            }
            self.group_charge = None;
        }
        Ok(())
    }
}

/// B0376: a renderer's first canvas is its transport's prewarm canvas while that is unclaimed;
/// only without one does `new` compile the programs on this thread.
fn claim_canvas<C>(
    canvas: &mut Option<C>,
    warm: impl FnOnce() -> Option<C>,
    new: impl FnOnce() -> Result<C>,
) -> Result<()> {
    if canvas.is_none() {
        *canvas = Some(match warm() {
            Some(warm) => warm,
            None => new()?,
        });
    }
    Ok(())
}
fn group_size(scene: &Scene, frame: &crate::tree::Frame) -> (u32, u32) {
    (
        (scene.bounds.w * frame.scale).ceil() as u32,
        (scene.bounds.h * frame.scale).ceil() as u32,
    )
}

/// (build, composite) the opacity group. An opening fade builds it while still hidden (the held
/// first pose) so its first visible frame only composites, and its settle frame composites the
/// group it already has instead of painting the whole body again.
fn group_use(opacity: f32, fade: bool, opening: bool, group_current: bool) -> (bool, bool) {
    let build = opacity < 1. && (opacity > 0. || opening);
    (
        build,
        opacity > 0. && (opacity < 1. || (fade && group_current)),
    )
}

#[cfg(test)]
mod group_tests {
    use super::group_use;

    #[test]
    fn a_fade_builds_hidden_and_settles_from_its_group() {
        let hidden = (0., true, true);
        let closed = (0., true, false);
        let mid = (0.4, true, true);
        let settle = (1., true, true);
        for ((opacity, fade, opening), current, want) in [
            (hidden, false, (true, false)),
            (closed, false, (false, false)),
            (mid, true, (true, true)),
            (settle, true, (false, true)),
            (settle, false, (false, false)),
            ((1., false, false), true, (false, false)),
            ((0.5, false, false), false, (true, true)),
        ] {
            assert_eq!(
                group_use(opacity, fade, opening, current),
                want,
                "{opacity} {fade} {opening} {current}"
            );
        }
    }
}
#[cfg(test)]
mod warm_canvas_tests {
    use super::{claim_canvas, take_warm};

    /// B0376: a cold or post-grace open's first frame draws with the prewarm canvas, so the main
    /// thread compiles no programs; the transport's one warm canvas goes to one renderer.
    #[test]
    fn the_first_renderer_draws_with_the_prewarm_canvas_and_compiles_nothing() {
        let warm = std::cell::Cell::new(vec!["prewarm"]);
        let compiled = std::cell::Cell::new(0);
        let new = || {
            compiled.set(compiled.get() + 1);
            Ok("compiled")
        };
        let mut first = None;
        claim_canvas(&mut first, || take_warm(&warm), new).unwrap();
        claim_canvas(&mut first, || take_warm(&warm), new).unwrap();
        assert_eq!((first, compiled.get()), (Some("prewarm"), 0));
        let mut second = None;
        claim_canvas(&mut second, || take_warm(&warm), new).unwrap();
        assert_eq!((second, compiled.get()), (Some("compiled"), 1));
    }
    /// B0406: a preparation for two wallpaper outputs warms two canvases, so neither output's
    /// resting run compiles on the main thread; a third renderer still compiles its own.
    #[test]
    fn each_output_a_preparation_was_made_for_claims_its_own_prewarm_canvas() {
        let warm = std::cell::Cell::new(vec!["b", "a"]);
        let claimed: Vec<_> = (0..3).map(|_| take_warm(&warm)).collect();
        assert_eq!(claimed, [Some("a"), Some("b"), None]);
        assert!(warm.take().is_empty());
    }
}

#[cfg(test)]
mod texture_budget_tests {
    use super::*;
    use bead_core::cache::{Budget, MIB};
    use std::collections::{BTreeMap, BTreeSet};

    /// A 1 MiB-charged texture on a femtovg Void canvas, so no GL context is needed.
    fn charged_texture(
        canvas: &mut femtovg::Canvas<femtovg::renderer::Void>,
        budget: &Budget,
        peak: &mut usize,
        payload: bool,
        touched: u64,
    ) -> Texture {
        let image = canvas
            .create_image_empty(
                1,
                1,
                femtovg::PixelFormat::Rgba8,
                femtovg::ImageFlags::empty(),
            )
            .unwrap();
        Texture {
            key: 0,
            payload,
            image,
            width: 512,
            height: 512,
            touched,
            _charge: reserve_texture(budget, peak, MIB, || false).unwrap(),
        }
    }
    #[test]
    fn textures_charge_the_renderers_budget_not_the_view_that_baked_them() {
        let mut scene = Scene::empty(16., 16.);
        scene.primitives.push(crate::Primitive::Image {
            opacity: 1.,
            rect: crate::Rect::new(0., 0., 16., 16.),
            path: "proof.svg".into(),
            blur: 0.,
            fit: crate::ImageFit::Fill,
            clip_polygon: None,
            bake_size: None,
            radius: 0.,
            tint: None,
        });
        scene.asset_data.insert("proof.svg".into(), std::sync::Arc::from(&br##"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16"><rect width="16" height="16" fill="#123456"/></svg>"##[..]));
        let mut view = crate::tree::TreeCache::default()
            .with_image_policy(crate::tree::ImagePolicy::UploadOnly);
        view.prepare(&scene, 1.).unwrap();
        let baked = view.retained_bytes();
        let mut renderer = GlRenderer::default();
        let budget = renderer.texture_budget();
        let charge =
            reserve_texture(&budget, &mut renderer.texture_peak, 3 * MIB, || false).unwrap();
        assert_eq!(budget.used(), 3 * MIB);
        assert_eq!(
            view.retained_bytes(),
            baked,
            "retained-view-resources is untouched"
        );
        assert!(bead_core::cache::report().caches.iter().any(|cache| {
            cache.name == "native-gl-images"
                && cache.limit_bytes == 64 * MIB
                && cache.used_bytes >= 3 * MIB
        }));
        drop(charge);
        assert_eq!(budget.used(), 0);
    }
    #[test]
    fn kept_textures_stay_charged_until_released_and_nothing_leaks_across_a_reopen() {
        let mut canvas = femtovg::Canvas::new(femtovg::renderer::Void).unwrap();
        // No GL canvas: releasing only drops the textures and their charges.
        let mut renderer = GlRenderer::default();
        let budget = renderer.texture_budget();
        for (slot, payload) in [("thumbnail", true), ("label", false)] {
            let texture =
                charged_texture(&mut canvas, &budget, &mut renderer.texture_peak, payload, 1);
            renderer.images.insert(Slot::Layer(slot.into(), 0), texture);
        }
        // The last frame (1) drew both.
        renderer.serial = 1;
        // A B0334 close keeps the image payload's texture, still charged; the layer raster goes.
        renderer.release_gpu_resources(true);
        assert_eq!(budget.used(), MIB);
        // The reopen adds a texture; a release keeping nothing, then the renderer's drop, free all.
        let texture = charged_texture(&mut canvas, &budget, &mut renderer.texture_peak, true, 2);
        renderer.images.insert(Slot::Image(1), texture);
        assert_eq!(budget.used(), 2 * MIB);
        renderer.release_gpu_resources(false);
        assert_eq!(budget.used(), 0);
        let texture = charged_texture(&mut canvas, &budget, &mut renderer.texture_peak, true, 3);
        renderer.images.insert(Slot::Image(2), texture);
        drop(renderer);
        assert_eq!(budget.used(), 0);
    }
    #[test]
    fn a_close_keeps_only_the_image_textures_the_last_frame_drew() {
        let mut canvas = femtovg::Canvas::new(femtovg::renderer::Void).unwrap();
        let mut renderer = GlRenderer::default();
        let budget = renderer.texture_budget();
        // B0339: a glide drew "passed" at frame 1; frame 3, the last, drew "visible" and a label.
        for (slot, payload, touched) in [
            ("passed", true, 1),
            ("visible", true, 3),
            ("label", false, 3),
        ] {
            let texture = charged_texture(
                &mut canvas,
                &budget,
                &mut renderer.texture_peak,
                payload,
                touched,
            );
            renderer.images.insert(Slot::Layer(slot.into(), 0), texture);
        }
        renderer.serial = 3;
        renderer.release_gpu_resources(true);
        assert_eq!(
            renderer.images.keys().collect::<Vec<_>>(),
            [&Slot::Layer("visible".into(), 0)]
        );
        assert_eq!(budget.used(), MIB, "the other textures' charges are freed");
    }
    #[test]
    fn a_full_budget_evicts_textures_the_frame_does_not_use_then_refuses() {
        let mut canvas = femtovg::Canvas::new(femtovg::renderer::Void).unwrap();
        let budget = Budget::new("native-gl-images-test", 2 * MIB);
        let (mut peak, mut images) = (0, BTreeMap::new());
        images.insert(
            Slot::Layer("old".into(), 0),
            charged_texture(&mut canvas, &budget, &mut peak, true, 1),
        );
        images.insert(
            Slot::Layer("drawn".into(), 0),
            charged_texture(&mut canvas, &budget, &mut peak, true, 2),
        );
        let drawn = BTreeSet::from([Slot::Layer("drawn".into(), 0)]);
        let fits = reserve_texture(&budget, &mut peak, MIB, || {
            evict_lru(&mut images, &mut canvas, &drawn)
        });
        assert!(fits.is_ok());
        assert_eq!(
            images.keys().collect::<Vec<_>>(),
            [&Slot::Layer("drawn".into(), 0)]
        );
        let Err(error) = reserve_texture(&budget, &mut peak, MIB, || {
            evict_lru(&mut images, &mut canvas, &drawn)
        }) else {
            panic!("nothing is left to evict");
        };
        let refused = error.downcast_ref::<TextureBudgetRefused>().unwrap();
        assert_eq!((refused.need, refused.cap), (MIB, 2 * MIB));
        assert!(
            error.to_string().contains("budget"),
            "the host's cache-pressure test matches it"
        );
        assert_eq!((peak, budget.used()), (2 * MIB, 2 * MIB));
    }
    /// The texture pass on a Void canvas, so no GL context is needed.
    struct Pass {
        canvas: femtovg::Canvas<femtovg::renderer::Void>,
        images: BTreeMap<Slot, Texture>,
        budget: Budget,
        peak: usize,
        serial: u64,
    }
    impl Pass {
        /// A budget of `textures` 32x32 textures.
        fn new(textures: usize) -> Self {
            Self {
                canvas: femtovg::Canvas::new(femtovg::renderer::Void).unwrap(),
                images: BTreeMap::new(),
                budget: Budget::new("native-gl-images-test", textures * (32 * 32 * 4 + 192)),
                peak: 0,
                serial: 0,
            }
        }
        /// One frame of layers, each drawing the named SVGs baked at 32x32 (equal names share a payload).
        fn draw(
            &mut self,
            cache: &mut crate::tree::TreeCache,
            layers: &[(&str, Vec<String>)],
        ) -> Result<()> {
            let mut scene = Scene::empty(64., 64.);
            for (id, names) in layers {
                let start = scene.primitives.len();
                for name in names {
                    let fill = name
                        .bytes()
                        .fold(7u32, |h, b| h.wrapping_mul(31).wrapping_add(u32::from(b)))
                        & 0xffffff;
                    let svg = format!(
                        r##"<svg xmlns="http://www.w3.org/2000/svg" width="32" height="32"><rect width="32" height="32" fill="#{fill:06x}"/></svg>"##
                    );
                    scene
                        .asset_data
                        .insert(name.into(), std::sync::Arc::from(svg.as_bytes()));
                    scene.primitives.push(crate::Primitive::Image {
                        opacity: 1.,
                        rect: crate::Rect::new(0., 0., 32., 32.),
                        path: name.into(),
                        blur: 0.,
                        fit: crate::ImageFit::Fill,
                        clip_polygon: None,
                        bake_size: Some((32., 32.)),
                        radius: 0.,
                        tint: None,
                    });
                }
                scene.layers.push(crate::SceneLayer {
                    id: (*id).into(),
                    parent: None,
                    own: start..scene.primitives.len(),
                    subtree_end: scene.primitives.len(),
                    bounds: crate::Rect::new(0., 0., 32., 32.),
                    scroll: None,
                });
            }
            cache.prepare(&scene, 1.).unwrap();
            self.serial += 1;
            let frame = cache.frame.clone().unwrap();
            upload_textures(
                &mut self.images,
                &mut self.canvas,
                &self.budget,
                &mut self.peak,
                self.serial,
                &frame,
            )
        }
    }
    fn cache() -> crate::tree::TreeCache {
        crate::tree::TreeCache::default().with_image_policy(crate::tree::ImagePolicy::UploadOnly)
    }
    #[test]
    fn a_shuffle_of_pearls_holds_one_texture_per_distinct_image() {
        // 35 pearls: a thumbnail of their own plus three images every pearl shares.
        let pearls = |first: usize| -> Vec<(String, Vec<String>)> {
            (first..first + 35)
                .map(|n| {
                    (
                        format!("p{n}"),
                        vec![
                            format!("t{n}.svg"),
                            "glow.svg".into(),
                            "shade.svg".into(),
                            "sheen.svg".into(),
                        ],
                    )
                })
                .collect()
        };
        let (mut pass, mut cache) = (Pass::new(60), cache());
        for step in 0..7 {
            let layers = pearls(step * 6);
            let layers: Vec<_> = layers
                .iter()
                .map(|(id, names)| (id.as_str(), names.clone()))
                .collect();
            pass.draw(&mut cache, &layers).unwrap();
            assert_eq!(
                pass.images.len(),
                35 + 3,
                "one texture per payload, not 4 per pearl"
            );
        }
        assert!(pass.peak <= 60 * (32 * 32 * 4 + 192));
    }
    #[test]
    fn eviction_spares_what_the_frame_draws_and_refuses_when_it_cannot_fit() {
        let layer = |id: &'static str, n: usize| {
            (
                id,
                (0..n).map(|i| format!("{id}{i}.svg")).collect::<Vec<_>>(),
            )
        };
        let (mut pass, mut cache) = (Pass::new(8), cache());
        // Frame A fills the budget and drops the pixels of its four "d" and four "s" textures.
        pass.draw(&mut cache, &[layer("d", 4), layer("s", 4)])
            .unwrap();
        // Frame B draws the four "n" textures first, then "d" again, whose pixels are gone:
        // the "s" textures must go, never the "d" ones the frame still needs.
        pass.draw(&mut cache, &[layer("n", 4), layer("d", 4)])
            .unwrap();
        assert_eq!(pass.images.len(), 8);
        // Frame C draws nine textures into a budget of eight.
        let Err(error) = pass.draw(&mut cache, &[layer("c", 9)]) else {
            panic!("nine textures cannot fit eight");
        };
        assert!(
            error.downcast_ref::<TextureBudgetRefused>().is_some(),
            "{error}"
        );
    }
    #[test]
    fn the_texture_peak_covers_one_open_and_restarts_from_what_is_held() {
        let mut renderer = GlRenderer::default();
        assert_eq!(renderer.take_texture_peak(), 0);
        let budget = renderer.texture_budget();
        let kept = reserve_texture(&budget, &mut renderer.texture_peak, MIB, || false).unwrap();
        drop(reserve_texture(&budget, &mut renderer.texture_peak, 2 * MIB, || false).unwrap());
        assert_eq!(renderer.take_texture_peak(), 3 * MIB, "this open's peak");
        assert_eq!(
            renderer.take_texture_peak(),
            MIB,
            "the next open starts at the kept texture"
        );
        drop(kept);
    }
}

#[cfg(test)]
mod gpu_ring_tests {
    use super::{GPU_RING, Ring};

    #[test]
    fn results_come_out_in_order_and_only_when_available() {
        let mut ring = Ring::default();
        for n in 0..3 {
            assert_eq!(ring.push(n), None);
        }
        // 1 is ready but 0 is not: nothing is emitted out of order.
        assert_eq!(ring.take_ready(|n| *n != 0), None);
        assert_eq!(ring.take_ready(|_| true), Some(0));
        assert_eq!(ring.take_ready(|_| true), Some(1));
        assert_eq!(ring.take_ready(|n| *n != 2), None);
        assert_eq!(ring.take_ready(|_| true), Some(2));
        assert_eq!(ring.take_ready(|_| true), None);
    }

    #[test]
    fn a_full_ring_drops_the_oldest_and_counts_it() {
        let mut ring = Ring::default();
        for n in 0..GPU_RING {
            assert_eq!(ring.push(n), None);
        }
        assert_eq!(ring.push(100), Some(0));
        assert_eq!(ring.push(101), Some(1));
        assert_eq!((ring.items.len(), ring.dropped), (GPU_RING, 2));
        assert_eq!(ring.take_ready(|_| true), Some(2));
    }
}
#[cfg(test)]
mod shift_tests {
    use crate::Rect;
    use crate::tree::{placement, shift_for};

    #[test]
    fn nested_planes_compose_their_shifts() {
        let v = Rect::new(0., 0., 100., 100.);
        let shifts = [(0..10, v, (0., -20.)), (4..6, v, (0., -5.))];
        assert_eq!(shift_for(5, &shifts), (0., -25.));
        assert_eq!(shift_for(2, &shifts), (0., -20.));
        assert_eq!(shift_for(12, &shifts), (0., 0.));
    }

    #[test]
    fn items_outside_the_plane_clip_are_culled_and_partial_ones_are_cut() {
        let v = Rect::new(0., 0., 100., 100.);
        let shifts = [(0..10, v, (0., -20.))];
        assert!(placement(1, None, Rect::new(0., 0., 50., 10.), &shifts).is_none());
        let (shift, clip) = placement(1, None, Rect::new(0., 15., 50., 10.), &shifts).unwrap();
        assert_eq!((shift, clip), ((0., -20.), Some(v)));
        assert_eq!(
            placement(11, None, Rect::new(0., 0., 5., 5.), &shifts)
                .unwrap()
                .1,
            None
        );
    }

    #[test]
    fn the_viewport_clip_stays_and_an_inner_clip_moves() {
        let v = Rect::new(0., 0., 100., 100.);
        let shifts = [(0..10, v, (0., -20.))];
        let b = Rect::new(0., 40., 50., 10.);
        assert_eq!(placement(1, Some(v), b, &shifts).unwrap().1, Some(v));
        let inner = Rect::new(10., 50., 40., 40.);
        assert_eq!(
            placement(1, Some(inner), b, &shifts).unwrap().1,
            Some(Rect::new(10., 30., 40., 40.))
        );
    }
}
