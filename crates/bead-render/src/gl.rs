// SPDX-License-Identifier: MIT
//! Shared native EGL context, retained subtree rasters and direct vector primitives.
use crate::{Damage, RenderTarget, Renderer, Result, Scene, ShmTarget};
use glutin::{
    config::{Config, ConfigTemplateBuilder},
    context::{
        ContextApi, ContextAttributesBuilder, NotCurrentContext, NotCurrentGlContext,
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
    swap_damage: unsafe extern "system" fn(
        *const std::ffi::c_void,
        *const std::ffi::c_void,
        *const i32,
        i32,
    ) -> u32,
    /// EGL_KHR_surfaceless_context: the upload thread's shared context needs it (B0317).
    surfaceless: bool,
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
    config: Config,
    pub display: Display,
    swap_damage: DamageSwap,
    surfaceless: bool,
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
    /// OpenGL 3.3 context without requiring a window handle.
    ///
    /// # Safety
    /// The native display/connection must remain alive until this value and all
    /// surfaces created from it are destroyed. Do not initialize another
    /// transport concurrently for the same native display; the caller must
    /// serialize setup and later convert this value into the sole `Transport`.
    pub unsafe fn new(display: RawDisplayHandle) -> Result<Self> {
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
        let context = if supports_surfaceless {
            match warm_surfaceless(context, &display) {
                Ok(context) => context,
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
            context
        };
        Ok(Self {
            context: Some(context),
            config,
            display,
            swap_damage,
            surfaceless: supports_surfaceless,
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
            swap_damage: self.swap_damage,
            surfaceless: self.surfaceless,
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
            config,
            display,
            ..
        } = self;
        drop(context);
        drop(config);
        unsafe { terminate_exclusive(&display) }
    }
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

fn warm_surfaceless(context: NotCurrentContext, display: &Display) -> Result<NotCurrentContext> {
    match context {
        NotCurrentContext::Egl(context) => {
            let current = context.make_current_surfaceless()?;
            let warmed = (|| -> Result<()> {
                let _span = bead_core::profile::Span::new("panel_shader_prewarm");
                // SAFETY: this worker owns the current context. These temporary
                // resources are dropped here; no !Send Canvas crosses threads.
                let renderer = unsafe {
                    femtovg::renderer::OpenGl::new_from_function_cstr(|name| {
                        display.get_proc_address(name).cast()
                    })
                }?;
                let _canvas = femtovg::Canvas::new(renderer)?;
                Ok(())
            })();
            // Unbind even if shader setup failed, before the caller retires EGL.
            let context = current.make_not_current()?;
            warmed?;
            Ok(NotCurrentContext::Egl(context))
        }
        #[allow(unreachable_patterns)]
        _ => Err("surfaceless warmup requires EGL".into()),
    }
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
}
impl Transport {
    /// Release a process-owned EGL display, including the driver's display pool.
    ///
    /// # Safety
    /// Every renderer, context and surface using this native display must already
    /// be destroyed. No other library may own EGL objects on this display.
    pub unsafe fn shutdown_exclusive(mut self) -> Result<()> {
        if let Some(context) = self.context.take() {
            context.make_not_current_in_place()?;
            drop(context);
        }
        unsafe { terminate_exclusive(&self.display) }
    }
    /// # Safety
    /// Display must remain alive until the context and all surfaces are destroyed.
    pub unsafe fn new(display: RawDisplayHandle) -> Result<Self> {
        let (display, config, swap_damage, surfaceless) = unsafe { initialize_display(display) }?;
        Ok(Self {
            display,
            config,
            context: None,
            swap_damage,
            surfaceless,
        })
    }
    /// B0317: start the upload thread on a context shared with this transport's. It lives until
    /// `Uploader::shutdown`, which must run before the transport is shut down.
    pub fn uploader(&self, done: impl Fn(Uploaded) + Send + 'static) -> Result<Uploader> {
        // shortcut: no pbuffer fallback for drivers without surfaceless contexts, the caller
        // keeps uploading on the main thread; add one if such a driver shows up.
        if !self.surfaceless {
            return Err("EGL_KHR_surfaceless_context missing".into());
        }
        let shared = self.context.as_ref().ok_or("missing EGL context")?;
        let attrs = ContextAttributesBuilder::new()
            .with_context_api(ContextApi::OpenGl(Some(Version::new(3, 3))))
            .with_sharing(shared)
            .build(None);
        // SAFETY: config belongs to this display; the context is moved to the upload thread,
        // which is joined before the display is terminated.
        let context = unsafe { self.display.create_context(&self.config, &attrs) }?;
        Uploader::spawn(self.display.clone(), context, Box::new(done))
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
        let ok = unsafe {
            (self.swap_damage)(
                display,
                window,
                rectangles.as_ptr(),
                i32::try_from(damage.len())?,
            )
        };
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
fn gl_proc<T: Copy>(display: &Display, name: &std::ffi::CStr) -> Result<T> {
    let address = display.get_proc_address(name);
    if address.is_null() {
        return Err(format!("missing {}", name.to_string_lossy()).into());
    }
    assert_eq!(std::mem::size_of::<T>(), std::mem::size_of_val(&address));
    // SAFETY: T is the extern "system" signature GL defines for `name`; sizes checked above.
    Ok(unsafe { std::mem::transmute_copy(&address) })
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
    delete: GlDelete,
    /// Wall milliseconds of `new`: the renderer and canvas, then the two uploads (B0316).
    pub setup_ms: (f64, f64),
}
impl WallpaperFader {
    /// The EGL context must be current (`Transport::activate`). Both uploads happen here, so the
    /// first moving frame costs like the others.
    pub fn new(display: &Display, size: (u32, u32), from: &[u8], to: &[u8]) -> Result<Self> {
        let began = std::time::Instant::now();
        // SAFETY: the caller made the transport's context current on this thread.
        let renderer = unsafe {
            femtovg::renderer::OpenGl::new_from_function_cstr(|name| {
                display.get_proc_address(name).cast()
            })
        }?;
        let canvas = femtovg::Canvas::new(renderer)?;
        let canvas_ms = began.elapsed().as_secs_f64() * 1e3;
        let mut fader = Self {
            canvas,
            from: None,
            to: None,
            size,
            last: (0., WallpaperBlend::Fade),
            delete: gl_proc(display, c"glDeleteTextures")?,
            setup_ms: (0., 0.),
        };
        // An error drops the fader, whose Drop frees whichever image exists by then.
        fader.from = Some(fader.make(Pixels::Rgba(from))?);
        fader.to = Some(fader.make(Pixels::Rgba(to))?);
        fader.setup_ms = (canvas_ms, began.elapsed().as_secs_f64() * 1e3 - canvas_ms);
        Ok(fader)
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
            Pixels::Texture(texture) => {
                let info = femtovg::ImageInfo::new(
                    femtovg::ImageFlags::empty(),
                    w as usize,
                    h as usize,
                    femtovg::PixelFormat::Rgba8,
                );
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
                        self.delete_texture(texture);
                        Err(error.into())
                    }
                }
            }
        }
    }
    fn delete_texture(&self, texture: u32) {
        // SAFETY: the context is current (callers activate it) and in the texture's share group.
        unsafe { (self.delete)(1, &texture) };
        count(-1);
    }
    fn free(&mut self, image: Img) {
        self.canvas.delete_image(image.id);
        match image.native {
            Some(texture) => self.delete_texture(texture),
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
    pub fn draw(&mut self, blend: f32, mode: WallpaperBlend) {
        self.last = (blend, mode);
        self.canvas.set_size(self.size.0, self.size.1, 1.);
        self.canvas.set_render_target(femtovg::RenderTarget::Screen);
        self.paint(blend, mode);
        self.canvas.flush();
    }
    /// Make the last drawn blend the new `from` (at rest the shown image already is) and take
    /// `to`: a retarget with no jump.
    pub fn retarget(&mut self, to: Pixels) -> Result<()> {
        let to = self.make(to)?;
        if self.to.is_some() {
            let (w, h) = self.size;
            let baked = self.canvas.create_image_empty(
                w as usize,
                h as usize,
                femtovg::PixelFormat::Rgba8,
                femtovg::ImageFlags::PREMULTIPLIED | femtovg::ImageFlags::FLIP_Y,
            );
            let baked = match baked {
                Ok(baked) => baked,
                Err(error) => {
                    self.free(to);
                    return Err(error.into());
                }
            };
            count(1);
            self.canvas
                .set_render_target(femtovg::RenderTarget::Image(baked));
            let (blend, mode) = self.last;
            self.paint(blend, mode);
            self.canvas.flush();
            self.canvas.set_render_target(femtovg::RenderTarget::Screen);
            for old in [self.from.take(), self.to.take()].into_iter().flatten() {
                self.free(old);
            }
            self.from = Some(Img {
                id: baked,
                native: None,
            });
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
    },
    /// The main thread took the texture over and deletes it.
    Adopt(u32),
    /// A texture nobody wants any more (a superseded frame).
    Delete(u32),
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
    fn upload(&self, (w, h): (u32, u32), rgba: &[u8]) -> std::result::Result<u32, String> {
        const TEXTURE_2D: u32 = 0x0DE1;
        // LINEAR filters and CLAMP_TO_EDGE: femtovg's defaults for ImageFlags::empty().
        const PARAMETERS: [(u32, i32); 4] = [
            (0x2801, 0x2601),
            (0x2800, 0x2601),
            (0x2802, 0x812F),
            (0x2803, 0x812F),
        ];
        if rgba.len() != w as usize * h as usize * 4 {
            return Err("wallpaper texture size mismatch".into());
        }
        let mut texture = 0;
        // SAFETY: this thread's context is current; `rgba` holds w*h RGBA pixels.
        let error = unsafe {
            (self.gen_textures)(1, &mut texture);
            (self.bind)(TEXTURE_2D, texture);
            for (name, value) in PARAMETERS {
                (self.parameter)(TEXTURE_2D, name, value);
            }
            (self.image)(
                TEXTURE_2D,
                0,
                0x8058,
                w as i32,
                h as i32,
                0,
                0x1908,
                0x1401,
                rgba.as_ptr().cast(),
            );
            (self.bind)(TEXTURE_2D, 0);
            (self.finish)();
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
                        (Job::Upload { token, size, rgba }, setup) => {
                            let began = std::time::Instant::now();
                            let texture =
                                match setup {
                                    Ok((_, gl)) => std::panic::catch_unwind(
                                        std::panic::AssertUnwindSafe(|| gl.upload(size, &rgba)),
                                    )
                                    .unwrap_or_else(|_| Err("upload panicked".into())),
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
    /// Queue an upload. The pixels come back when the thread is gone.
    pub fn upload(
        &self,
        token: u64,
        size: (u32, u32),
        rgba: Vec<u8>,
    ) -> std::result::Result<(), Vec<u8>> {
        let job = Job::Upload { token, size, rgba };
        match self.jobs.as_ref().map(|jobs| jobs.send(job)) {
            Some(Err(std::sync::mpsc::SendError(Job::Upload { rgba, .. }))) => Err(rgba),
            _ => Ok(()),
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
struct Texture {
    key: u64,
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
    images: std::collections::BTreeMap<(String, u32), Texture>,
    serial: u64,
    group_budget: Option<bead_core::cache::Budget>,
    group_charge: Option<bead_core::cache::Charge>,
    group: Option<(femtovg::ImageId, u32, u32, u64)>,
}
impl GlRenderer {
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
    fn upload(&mut self, display: &Display) -> Result<()> {
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
                let slot = (item.id.clone(), frame.scale.to_bits());
                (self
                    .images
                    .get(&slot)
                    .is_none_or(|image| image.key != payload.key)
                    && payload.pending()
                    && pending_keys.insert(payload.key))
                .then(|| payload.clone())
            })
            .collect::<Vec<_>>();
        if !pending.is_empty() {
            return Err(Box::new(crate::tree::ResourcesPending { images: pending }));
        }

        if self.canvas.is_none() {
            // SAFETY: Transport made its retained context current on this thread.
            let renderer = unsafe {
                femtovg::renderer::OpenGl::new_from_function_cstr(|name| {
                    display.get_proc_address(name).cast()
                })
            }?;
            self.canvas = Some(femtovg::Canvas::new(renderer)?);
        }
        let frame = self.cache.frame.as_ref().ok_or("unprepared layer tree")?;
        let canvas = self.canvas.as_mut().ok_or("GL canvas missing")?;
        self.serial += 1;
        let active: std::collections::BTreeSet<_> = frame
            .items
            .iter()
            .filter(|item| item.raster.is_some() || item.image.is_some())
            .map(|item| (item.id.clone(), frame.scale.to_bits()))
            .collect();
        let mut upload_uses = std::collections::BTreeMap::new();
        for payload in frame.items.iter().filter_map(|item| item.image.as_ref()) {
            *upload_uses.entry(payload.key).or_insert(0usize) += 1;
        }
        for item in &frame.items {
            if let Some(payload) = &item.image {
                let slot = (item.id.clone(), frame.scale.to_bits());
                if let Some(old) = self.images.get_mut(&slot)
                    && old.key == payload.key
                {
                    old.touched = self.serial;
                    Self::finish_image_upload(payload, &mut upload_uses)?;
                    continue;
                }
                let upload = payload.upload.lock().map_err(|_| "image upload poisoned")?;
                let pixels = &upload.as_ref().ok_or("image upload not prepared")?.pixels;
                let (width, height) = (pixels.width(), pixels.height());
                let charge =
                    payload.reserve_texture(pixels.data().len() + slot.0.capacity() + 192)?;
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
                if let Some(old) = self.images.insert(
                    slot,
                    Texture {
                        key: payload.key,
                        image,
                        width,
                        height,
                        touched: self.serial,
                        _charge: charge,
                    },
                ) {
                    canvas.delete_image(old.image);
                }
                Self::finish_image_upload(payload, &mut upload_uses)?;
                continue;
            }
            let Some(raster) = &item.raster else {
                continue;
            };
            let slot = (item.id.clone(), frame.scale.to_bits());
            let (w, h) = (raster.pixels.width(), raster.pixels.height());
            if self
                .images
                .get(&slot)
                .is_none_or(|old| old.width != w || old.height != h)
            {
                let charge = loop {
                    match raster
                        ._charge
                        .reserve_related(w as usize * h as usize * 4 + slot.0.capacity() + 192)
                    {
                        Ok(c) => break c,
                        Err(e) => {
                            let victim = self
                                .images
                                .iter()
                                .filter(|(key, t)| *key != &slot && t.touched != self.serial)
                                .min_by_key(|(_, t)| t.touched)
                                .map(|(k, _)| k.clone());
                            if let Some(victim) = victim {
                                if let Some(old) = self.images.remove(&victim) {
                                    canvas.delete_image(old.image);
                                }
                            } else {
                                return Err(format!(
                                    "{e}: native-gl-images requested={} global={}",
                                    w as usize * h as usize * 4 + slot.0.capacity() + 192,
                                    bead_core::cache::report().used_bytes,
                                )
                                .into());
                            }
                        }
                    }
                };
                let image = canvas.create_image(
                    femtovg::ImageSource::Rgba(imgref::Img::new(
                        raster.pixels.data().as_rgba(),
                        w as usize,
                        h as usize,
                    )),
                    femtovg::ImageFlags::PREMULTIPLIED,
                )?;
                if let Some(old) = self.images.remove(&slot) {
                    canvas.delete_image(old.image);
                }
                self.images.insert(
                    slot.clone(),
                    Texture {
                        _charge: charge,
                        key: raster.key,
                        image,
                        width: w,
                        height: h,
                        touched: self.serial,
                    },
                );
                bead_core::vprintln!(
                    "PROFILE stage=layer_upload id={} pixels={} region=full",
                    item.id,
                    w * h
                );
            } else if let Some(old) = self.images.get_mut(&slot) {
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
                old.touched = self.serial;
            }
        }
        // Retire absent slots only after every current image upload succeeds;
        // an upload error does not additionally discard old off-frame slots.
        // The size-based LRU otherwise retains visited scroll rows up to its
        // much larger cap.
        let stale: Vec<_> = self
            .images
            .keys()
            .filter(|slot| !active.contains(*slot))
            .cloned()
            .collect();
        for slot in stale {
            if let Some(texture) = self.images.remove(&slot) {
                canvas.delete_image(texture.image);
            }
        }
        while self.images.len() > 1024
            || self
                .images
                .values()
                .map(|t| t.width as usize * t.height as usize * 4)
                .sum::<usize>()
                > crate::tree::STEADY_RASTER_LIMIT
        {
            let victim = self
                .images
                .iter()
                .filter(|(_, t)| t.touched != self.serial)
                .min_by_key(|(_, t)| t.touched)
                .map(|(id, _)| id.clone());
            let Some(victim) = victim else {
                return Err("active GL images exceed budget".into());
            };
            if let Some(texture) = self.images.remove(&victim) {
                canvas.delete_image(texture.image);
            }
        }
        Ok(())
    }
    fn paint(
        canvas: &mut femtovg::Canvas<femtovg::renderer::OpenGl>,
        frame: &crate::tree::Frame,
        images: &std::collections::BTreeMap<(String, u32), Texture>,
        offset: (f32, f32),
    ) -> Result<()> {
        for item in &frame.items {
            canvas.reset_scissor();
            if let Some(clip) = item.clip {
                canvas.scissor(
                    (clip.x + offset.0) * frame.scale,
                    (clip.y + offset.1) * frame.scale,
                    clip.w * frame.scale,
                    clip.h * frame.scale,
                );
            }
            let r = crate::tree::rect(&item.primitive);
            let opacity = item.primitive.opacity();
            let (x, y) = (
                (r.x + offset.0) * frame.scale,
                (r.y + offset.1) * frame.scale,
            );
            if let Some(payload) = &item.image {
                let texture = images
                    .get(&(item.id.clone(), frame.scale.to_bits()))
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
                    .get(&(item.id.clone(), frame.scale.to_bits()))
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
    /// Release GL image and canvas allocations while the renderer's EGL context
    /// is current. `TreeCache` is deliberately retained so the CPU rasterized
    /// layers can be uploaded again without rerasterizing them.
    fn release_gpu_resources(&mut self) {
        let image_bytes: usize = self
            .images
            .values()
            .map(|t| t.width as usize * t.height as usize * 4)
            .sum();
        let group_bytes = self
            .group
            .map_or(0, |(_, w, h, _)| u64::from(w) * u64::from(h) * 4);
        println!(
            "GL_CACHE_RELEASE images={} image_bytes={image_bytes} composite_bytes={group_bytes}",
            self.images.len()
        );
        if let Some(canvas) = self.canvas.as_mut() {
            for (_, texture) in std::mem::take(&mut self.images) {
                canvas.delete_image(texture.image);
            }
            if let Some((image, _, _, _)) = self.group.take() {
                canvas.delete_image(image);
            }
        } else {
            self.images.clear();
            self.group = None;
        }
        self.group_charge = None;
        // Dropping femtovg's canvas also releases renderer-owned GL state.
        // Keep both budgets so a later upload remains accounted against the
        // same process-wide cache accounting.
        self.canvas = None;
        self.cache.trim_idle();
    }

    fn prepared_frame(&self) -> Option<std::sync::Arc<crate::tree::Frame>> {
        self.cache.frame.clone()
    }
    fn resource_counts(&self) -> (usize, usize) {
        (
            self.cache.frame.as_ref().map_or(0, |f| f.layer_count),
            self.images.len() + usize::from(self.group.is_some()),
        )
    }
    fn stage_resources(&mut self, target: crate::ResourceTarget<'_>) -> Result<()> {
        #[allow(irrefutable_let_patterns)]
        let crate::ResourceTarget::Gl { display } = target else {
            return Err("GL resource target missing".into());
        };
        self.upload(display)?;
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
        self.upload(target.display)?;
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
                Self::paint(canvas, frame, &self.images, scene.offset)?;
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
                let budget = frame.native_resource_budget.as_ref().unwrap_or_else(|| {
                    self.group_budget.get_or_insert_with(|| {
                        bead_core::cache::Budget::new(
                            "native-gl-composite",
                            64 * bead_core::cache::MIB,
                        )
                    })
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
        canvas.flush();
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
