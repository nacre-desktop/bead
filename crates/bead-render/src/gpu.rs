// SPDX-License-Identifier: MIT OR Apache-2.0
//! GPU composition of the identical raster layer used by the CPU fallback.
//! Content is baked once; animation changes only image position and opacity.
use crate::{
    Damage, RenderTarget, Renderer, RendererKind, Result, Scene, ShmTarget, retained::LayerCache,
};
use raw_window_handle::{RawDisplayHandle, RawWindowHandle};
use rgb::FromSlice;
use std::{sync::Arc, time::Instant};
pub use wgpu;

pub struct Device {
    adapter: wgpu::Adapter,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
}
impl Device {
    pub fn adapter(&self) -> &wgpu::Adapter {
        &self.adapter
    }
}
pub struct Context {
    pub instance: wgpu::Instance,
    pub devices: Vec<Device>,
}
pub struct Transport {
    pub context: Context,
    memory_usage: bool,
}
impl Transport {
    pub fn new(memory_usage: bool) -> Self {
        Self {
            context: Context {
                instance: wgpu::Instance::new(wgpu::InstanceDescriptor {
                    backends: wgpu::Backends::VULKAN,
                    display: None,
                    flags: wgpu::InstanceFlags::from_build_config(),
                    memory_budget_thresholds: Default::default(),
                    backend_options: Default::default(),
                }),
                devices: Vec::new(),
            },
            memory_usage,
        }
    }
}
pub struct Surface {
    pub surface: wgpu::Surface<'static>,
    pub config: wgpu::SurfaceConfiguration,
    pub dev_id: usize,
    pub target: Option<(wgpu::Texture, wgpu::TextureView, wgpu::util::TextureBlitter)>,
}
impl Transport {
    /// # Safety
    /// The native Wayland display and surface must outlive the returned GPU surface.
    pub unsafe fn create_surface(
        &mut self,
        display: RawDisplayHandle,
        window: RawWindowHandle,
        w: u32,
        h: u32,
        kind: RendererKind,
    ) -> Result<Surface> {
        let start = Instant::now();
        // SAFETY: callers keep protocol objects alive and drop GPU surfaces first.
        let surface = unsafe {
            self.context
                .instance
                .create_surface_unsafe(wgpu::SurfaceTargetUnsafe::RawHandle {
                    raw_display_handle: Some(display),
                    raw_window_handle: window,
                })
        }?;
        if self.context.devices.is_empty() {
            let adapter = pollster::block_on(self.context.instance.request_adapter(
                &wgpu::RequestAdapterOptions {
                    compatible_surface: Some(&surface),
                    ..Default::default()
                },
            ))?;
            let (device, queue) =
                pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
                    required_features: adapter.features()
                        & (wgpu::Features::CLEAR_TEXTURE | wgpu::Features::PIPELINE_CACHE),
                    memory_hints: if self.memory_usage {
                        wgpu::MemoryHints::MemoryUsage
                    } else {
                        wgpu::MemoryHints::default()
                    },
                    ..Default::default()
                }))?;
            self.context.devices.push(Device {
                adapter,
                device,
                queue,
            });
        }
        let dev_id = 0;
        if !self.context.devices[0]
            .adapter
            .is_surface_supported(&surface)
        {
            return Err("surface cannot use the existing shared GPU device".into());
        }
        println!(
            "GPU_SHARED devices={} device_id=0 memory_usage={}",
            self.context.devices.len(),
            self.memory_usage
        );
        let dev = &self.context.devices[dev_id];
        let info = dev.adapter().get_info();
        if info.device_type == wgpu::DeviceType::Cpu {
            return Err("software adapter cannot be counted as a GPU variant".into());
        }
        let caps = surface.get_capabilities(dev.adapter());
        let format = caps
            .formats
            .into_iter()
            .find(|f| {
                matches!(
                    f,
                    wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Rgba8Unorm
                )
            })
            .ok_or("unsupported swapchain format")?;
        if !caps
            .alpha_modes
            .contains(&wgpu::CompositeAlphaMode::PreMultiplied)
        {
            return Err("surface lacks premultiplied alpha support".into());
        }
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: w,
            height: h,
            present_mode: wgpu::PresentMode::Fifo,
            desired_maximum_frame_latency: 2,
            alpha_mode: wgpu::CompositeAlphaMode::PreMultiplied,
            view_formats: vec![],
            color_space: wgpu::SurfaceColorSpace::Auto,
        };
        surface.configure(&dev.device, &config);
        let target = if kind == RendererKind::Vello {
            Some(intermediate(&dev.device, &config))
        } else {
            None
        };
        bead_core::vprintln!(
            "PROFILE stage=gpu_context ms={:.3} api={:?} device_type={:?} vello_intermediate={}",
            start.elapsed().as_secs_f64() * 1000.,
            info.backend,
            info.device_type,
            target.is_some()
        );
        Ok(Surface {
            surface,
            config,
            dev_id,
            target,
        })
    }
    pub fn resize(&self, surface: &mut Surface, w: u32, h: u32) {
        if surface.config.width == w && surface.config.height == h {
            return;
        }
        surface.config.width = w;
        surface.config.height = h;
        let dev = &self.context.devices[surface.dev_id];
        surface.surface.configure(&dev.device, &surface.config);
        if surface.target.is_some() {
            surface.target = Some(intermediate(&dev.device, &surface.config));
        }
    }
}
fn intermediate(
    device: &wgpu::Device,
    config: &wgpu::SurfaceConfiguration,
) -> (wgpu::Texture, wgpu::TextureView, wgpu::util::TextureBlitter) {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("bead-vello-output"),
        size: wgpu::Extent3d {
            width: config.width,
            height: config.height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let view = texture.create_view(&Default::default());
    // Vello's output texture contains straight alpha. Wayland's swapchain
    // requires premultiplied alpha, so convert once during the existing blit.
    let blitter = wgpu::util::TextureBlitterBuilder::new(device, config.format)
        .blend_state(wgpu::BlendState {
            color: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::SrcAlpha,
                dst_factor: wgpu::BlendFactor::Zero,
                operation: wgpu::BlendOperation::Add,
            },
            alpha: wgpu::BlendComponent::REPLACE,
        })
        .build();
    (texture, view, blitter)
}
pub struct GpuTarget<'a> {
    pub device: &'a wgpu::Device,
    pub queue: &'a wgpu::Queue,
    pub texture: &'a wgpu::Texture,
    pub target_view: Option<&'a wgpu::TextureView>,
    pub width: u32,
    pub height: u32,
}
struct LayerBytes {
    pixels: Arc<tiny_skia::Pixmap>,
    _charge: bead_core::cache::Charge,
}
impl AsRef<[u8]> for LayerBytes {
    fn as_ref(&self) -> &[u8] {
        self.pixels.data()
    }
}
pub struct GpuRenderer {
    kind: RendererKind,
    cache: LayerCache,
    femto: Option<femtovg::Canvas<femtovg::renderer::WGPURenderer>>,
    femto_image: Option<femtovg::ImageId>,
    femto_images:
        std::collections::BTreeMap<u32, (u64, femtovg::ImageId, bead_core::cache::Charge, u64)>,
    image_budget: bead_core::cache::Budget,
    image_clock: u64,
    vello: Option<vello::Renderer>,
    image: Option<vello::peniko::ImageData>,
    uploaded_key: Option<u64>,
}
impl GpuRenderer {
    pub fn new(kind: RendererKind) -> Self {
        Self {
            kind,
            cache: LayerCache::default(),
            femto: None,
            femto_image: None,
            femto_images: Default::default(),
            image_budget: bead_core::cache::Budget::new(
                "wgpu-femtovg-images",
                64 * bead_core::cache::MIB,
            ),
            image_clock: 0,
            vello: None,
            image: None,
            uploaded_key: None,
        }
    }
}
impl GpuRenderer {
    fn upload(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) -> Result<()> {
        let layer = self
            .cache
            .layer
            .as_ref()
            .ok_or("unprepared retained layer")?;
        let (w, h) = (layer.pixels.width(), layer.pixels.height());
        if self.femto.is_none() {
            let start = Instant::now();
            self.femto = Some(femtovg::Canvas::new(femtovg::renderer::WGPURenderer::new(
                device.clone(),
                queue.clone(),
            ))?);
            bead_core::vprintln!(
                "PROFILE stage=femtovg_init ms={:.3}",
                start.elapsed().as_secs_f64() * 1000.
            );
        }
        let canvas = self.femto.as_mut().unwrap();
        if self
            .femto_images
            .get(&layer.scale.to_bits())
            .is_none_or(|(key, _, _, _)| *key != layer.key)
        {
            let charge = loop {
                match self.image_budget.reserve(w as usize * h as usize * 4 + 128) {
                    Ok(c) => break c,
                    Err(e) => {
                        let key = self
                            .femto_images
                            .iter()
                            .filter(|(scale, _)| **scale != layer.scale.to_bits())
                            .min_by_key(|(_, (_, _, _, age))| age)
                            .map(|(k, _)| *k);
                        if let Some(key) = key {
                            if let Some((_, old, _, _)) = self.femto_images.remove(&key) {
                                canvas.delete_image(old);
                            }
                        } else {
                            return Err(e.into());
                        }
                    }
                }
            };
            let start = Instant::now();
            let next_image = canvas.create_image(
                femtovg::ImageSource::Rgba(imgref::Img::new(
                    layer.pixels.data().as_rgba(),
                    w as usize,
                    h as usize,
                )),
                femtovg::ImageFlags::PREMULTIPLIED,
            )?;
            if let Some((_, image, _, _)) = self.femto_images.remove(&layer.scale.to_bits()) {
                canvas.delete_image(image);
            }
            self.femto_image = Some(next_image);
            self.femto_images.insert(
                layer.scale.to_bits(),
                (
                    layer.key,
                    self.femto_image.unwrap(),
                    charge,
                    self.image_clock,
                ),
            );
            bead_core::vprintln!(
                "PROFILE stage=layer_upload ms={:.3} uploads={}",
                start.elapsed().as_secs_f64() * 1000.,
                self.cache.builds
            );
        }
        self.image_clock += 1;
        if let Some((_, _, _, age)) = self.femto_images.get_mut(&layer.scale.to_bits()) {
            *age = self.image_clock;
        }
        self.femto_image = Some(self.femto_images[&layer.scale.to_bits()].1);
        while self.femto_images.len() > 8 {
            let scale = *self
                .femto_images
                .iter()
                .min_by_key(|(_, (_, _, _, age))| age)
                .map(|(scale, _)| scale)
                .unwrap();
            if let Some((_, image, _, _)) = self.femto_images.remove(&scale) {
                canvas.delete_image(image);
            }
        }
        Ok(())
    }
}
impl Renderer for GpuRenderer {
    fn resource_counts(&self) -> (usize, usize) {
        (self.cache.layer_count(), self.femto_images.len())
    }
    fn stage_resources(&mut self, target: crate::ResourceTarget<'_>) -> Result<()> {
        #[allow(irrefutable_let_patterns)]
        let crate::ResourceTarget::Wgpu { device, queue } = target else {
            return Err("wgpu resource target missing".into());
        };
        if self.kind != RendererKind::Femtovg {
            return Err("Vello is a comparison path; UI reload uses femtovg or CPU".into());
        }
        let allocation_scope = device.push_error_scope(wgpu::ErrorFilter::OutOfMemory);
        let validation_scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
        let result = self.upload(device, queue);
        let validation = pollster::block_on(validation_scope.pop());
        let allocation = pollster::block_on(allocation_scope.pop());
        result?;
        if let Some(error) = validation.or(allocation) {
            return Err(error.to_string().into());
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
        Err("GPU renderer needs a GPU target; CPU fallback uses the same Scene".into())
    }
    fn draw(&mut self, scene: &Scene, target: RenderTarget<'_>, _: Damage) -> Result<()> {
        let RenderTarget::Gpu(target) = target else {
            return Err("wrong renderer target".into());
        };
        if self.kind == RendererKind::Femtovg {
            self.upload(target.device, target.queue)?;
        }
        let layer = self.cache.layer.as_ref().ok_or("layer not prepared")?;
        let x = ((scene.offset.0 + layer.origin.0) * layer.scale).round();
        let y = ((scene.offset.1 + layer.origin.1) * layer.scale).round();
        let w = layer.pixels.width();
        let h = layer.pixels.height();
        match self.kind {
            RendererKind::Femtovg => {
                let canvas = self.femto.as_mut().unwrap();
                canvas.set_size(target.width, target.height, 1.);
                canvas.clear_rect(
                    0,
                    0,
                    target.width,
                    target.height,
                    femtovg::Color::rgba(0, 0, 0, 0),
                );
                let mut path = femtovg::Path::new();
                path.rect(x, y, w as f32, h as f32);
                let paint = femtovg::Paint::image(
                    self.femto_image.unwrap(),
                    x,
                    y,
                    w as f32,
                    h as f32,
                    0.,
                    scene.opacity,
                );
                canvas.fill_path(&path, &paint);
                target.queue.submit(canvas.flush_to_output(target.texture));
            }
            RendererKind::Vello => {
                if self.vello.is_none() {
                    let start = Instant::now();
                    self.vello = Some(vello::Renderer::new(
                        target.device,
                        vello::RendererOptions {
                            antialiasing_support: vello::AaSupport::area_only(),
                            ..vello::RendererOptions::default()
                        },
                    )?);
                    bead_core::vprintln!(
                        "PROFILE stage=vello_init ms={:.3}",
                        start.elapsed().as_secs_f64() * 1000.
                    );
                }
                if self.uploaded_key != Some(layer.key) {
                    self.image = Some(vello::peniko::ImageData {
                        data: vello::peniko::Blob::new(Arc::new(LayerBytes {
                            pixels: layer.pixels.clone(),
                            _charge: layer._charge.clone(),
                        })),
                        format: vello::peniko::ImageFormat::Rgba8,
                        alpha_type: vello::peniko::ImageAlphaType::AlphaPremultiplied,
                        width: w,
                        height: h,
                    });
                    self.uploaded_key = Some(layer.key);
                    bead_core::vprintln!(
                        "PROFILE stage=layer_upload uploads={} shared_cpu_bytes=true",
                        self.cache.builds
                    );
                }
                let mut frame = vello::Scene::new();
                frame.draw_image(
                    vello::peniko::ImageBrush::new(self.image.as_ref().unwrap().clone())
                        .with_alpha(scene.opacity)
                        .as_ref(),
                    vello::kurbo::Affine::translate((f64::from(x), f64::from(y))),
                );
                self.vello.as_mut().unwrap().render_to_texture(
                    target.device,
                    target.queue,
                    &frame,
                    target.target_view.ok_or("Vello output texture missing")?,
                    &vello::RenderParams {
                        base_color: vello::peniko::Color::TRANSPARENT,
                        width: target.width,
                        height: target.height,
                        antialiasing_method: vello::AaConfig::Area,
                    },
                )?;
            }
            RendererKind::Cpu | RendererKind::FemtovgGl => {
                return Err("CPU renderer selected for GPU".into());
            }
        }
        Ok(())
    }
}
