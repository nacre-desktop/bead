// SPDX-License-Identifier: MIT OR Apache-2.0
//! One native EGL/OpenGL context and one retained image shared by all surfaces.
use crate::{Damage, RenderTarget, Renderer, Result, Scene, ShmTarget, retained::LayerCache};
use glutin::{
    config::{Config, ConfigTemplateBuilder},
    context::{ContextApi, ContextAttributesBuilder, PossiblyCurrentContext, Version},
    display::{Display, DisplayApiPreference},
    prelude::*,
    surface::{Surface as EglSurface, SurfaceAttributesBuilder, SwapInterval, WindowSurface},
};
use raw_window_handle::{RawDisplayHandle, RawWindowHandle};
use rgb::FromSlice;
use std::num::NonZeroU32;
pub struct Transport {
    pub display: Display,
    config: Config,
    context: Option<PossiblyCurrentContext>,
}
pub struct Surface {
    surface: EglSurface<WindowSurface>,
}
pub struct GlTarget<'a> {
    pub display: &'a Display,
    pub width: u32,
    pub height: u32,
}
impl Transport {
    /// # Safety
    /// Display must remain alive until the context and all surfaces are destroyed.
    pub unsafe fn new(display: RawDisplayHandle) -> Result<Self> {
        // SAFETY: the caller retains the native Wayland connection.
        let display = unsafe { Display::new(display, DisplayApiPreference::Egl) }?;
        let template = ConfigTemplateBuilder::new()
            .with_alpha_size(8)
            .with_stencil_size(8)
            .with_transparency(true)
            .build();
        // SAFETY: configuration enumeration does not access a native window.
        let config = unsafe { display.find_configs(template) }?
            .next()
            .ok_or("no transparent EGL config")?;
        Ok(Self {
            display,
            config,
            context: None,
        })
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
        if ["llvmpipe", "softpipe", "software"]
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
        Ok(Surface { surface })
    }
    pub fn activate(&self, surface: &Surface, w: u32, h: u32) -> Result<()> {
        let context = self.context.as_ref().ok_or("missing EGL context")?;
        context.make_current(&surface.surface)?;
        surface.surface.resize(
            context,
            NonZeroU32::new(w).ok_or("zero width")?,
            NonZeroU32::new(h).ok_or("zero height")?,
        );
        Ok(())
    }
    pub fn present(&self, surface: &Surface) -> Result<()> {
        surface
            .surface
            .swap_buffers(self.context.as_ref().ok_or("missing EGL context")?)?;
        Ok(())
    }
}
#[derive(Default)]
pub struct GlRenderer {
    cache: LayerCache,
    canvas: Option<femtovg::Canvas<femtovg::renderer::OpenGl>>,
    image: Option<femtovg::ImageId>,
    images: std::collections::BTreeMap<u32, (u64, femtovg::ImageId)>,
}
impl GlRenderer {
    fn upload(&mut self, display: &Display) -> Result<()> {
        if self.canvas.is_none() {
            // SAFETY: Transport has made its single retained context current on this thread.
            let renderer = unsafe {
                femtovg::renderer::OpenGl::new_from_function_cstr(|name| {
                    display.get_proc_address(name).cast()
                })
            }?;
            self.canvas = Some(femtovg::Canvas::new(renderer)?);
        }
        let layer = self
            .cache
            .layer
            .as_ref()
            .ok_or("unprepared retained layer")?;
        let canvas = self.canvas.as_mut().unwrap();
        let (w, h) = (layer.pixels.width(), layer.pixels.height());
        if self
            .images
            .get(&layer.scale.to_bits())
            .is_none_or(|(key, _)| *key != layer.key)
        {
            let next_image = canvas.create_image(
                femtovg::ImageSource::Rgba(imgref::Img::new(
                    layer.pixels.data().as_rgba(),
                    w as usize,
                    h as usize,
                )),
                femtovg::ImageFlags::PREMULTIPLIED,
            )?;
            if let Some((_, image)) = self.images.remove(&layer.scale.to_bits()) {
                canvas.delete_image(image);
            }
            self.image = Some(next_image);
            self.images
                .insert(layer.scale.to_bits(), (layer.key, self.image.unwrap()));
            println!("PROFILE stage=layer_upload uploads={}", self.cache.builds);
        }
        while self.images.len() > 8 {
            let scale = *self
                .images
                .keys()
                .find(|&&s| s != layer.scale.to_bits())
                .unwrap();
            if let Some((_, image)) = self.images.remove(&scale) {
                canvas.delete_image(image);
            }
        }
        self.image = Some(self.images[&layer.scale.to_bits()].1);
        Ok(())
    }
}
impl Renderer for GlRenderer {
    fn resource_counts(&self) -> (usize, usize) {
        (self.cache.layer_count(), self.images.len())
    }
    fn stage_resources(&mut self, target: crate::ResourceTarget<'_>) -> Result<()> {
        let crate::ResourceTarget::Gl { display } = target else {
            return Err("GL resource target missing".into());
        };
        self.upload(display)?;
        let address = display.get_proc_address(c"glGetError");
        if address.is_null() {
            return Err("missing glGetError".into());
        }
        // SAFETY: a current desktop GL context provides this standard signature.
        let get_error: unsafe extern "system" fn() -> u32 = unsafe { std::mem::transmute(address) };
        // SAFETY: this context is current throughout upload/staging.
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
        let layer = self
            .cache
            .layer
            .as_ref()
            .ok_or("unprepared retained layer")?;
        let (w, h) = (layer.pixels.width(), layer.pixels.height());
        let canvas = self.canvas.as_mut().unwrap();
        canvas.set_size(target.width, target.height, 1.);
        canvas.clear_rect(
            0,
            0,
            target.width,
            target.height,
            femtovg::Color::rgba(0, 0, 0, 0),
        );
        let x = (scene.offset.0 + layer.origin.0) * layer.scale;
        let y = (scene.offset.1 + layer.origin.1) * layer.scale;
        let mut path = femtovg::Path::new();
        path.rect(x, y, w as f32, h as f32);
        let paint = femtovg::Paint::image(
            self.image.unwrap(),
            x,
            y,
            w as f32,
            h as f32,
            0.,
            scene.opacity,
        );
        canvas.fill_path(&path, &paint);
        canvas.flush();
        Ok(())
    }
}
