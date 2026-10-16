// SPDX-License-Identifier: MIT OR Apache-2.0
//! Backend-neutral drawing commands and a CPU renderer over borrowed SHM memory.
use cosmic_text::{
    Attrs, Buffer, Color as TextColor, Family, FontSystem, Metrics, Shaping, SwashCache,
};
use std::{
    collections::{BTreeSet, HashMap},
    error::Error,
    path::Path,
    time::Instant,
};
use tiny_skia::{
    FillRule, Mask, Paint, PathBuilder, Pixmap, PixmapMut, PixmapPaint, Stroke, Transform,
};
pub mod damage;
#[cfg(feature = "native-gl")]
pub mod gl;
#[cfg(feature = "wgpu")]
pub mod gpu;
pub mod retained;
pub mod tree;
pub type Result<T> = std::result::Result<T, Box<dyn Error>>;

#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}
impl Rect {
    pub fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self { x, y, w, h }
    }
    pub fn union(self, other: Self) -> Self {
        let x = self.x.min(other.x);
        let y = self.y.min(other.y);
        Self::new(
            x,
            y,
            (self.x + self.w).max(other.x + other.w) - x,
            (self.y + self.h).max(other.y + other.h) - y,
        )
    }
    pub fn translated(self, x: f32, y: f32) -> Self {
        Self::new(self.x + x, self.y + y, self.w, self.h)
    }
    pub fn pixels(self, scale: f32, width: u32, height: u32) -> Damage {
        let x = (self.x * scale).floor().max(0.).min(width as f32) as u32;
        let y = (self.y * scale).floor().max(0.).min(height as f32) as u32;
        let right = ((self.x + self.w) * scale).ceil().max(0.).min(width as f32) as u32;
        let bottom = ((self.y + self.h) * scale)
            .ceil()
            .max(0.)
            .min(height as f32) as u32;
        Damage {
            x,
            y,
            w: right.saturating_sub(x),
            h: bottom.saturating_sub(y),
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Damage {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}
impl Damage {
    pub fn full(w: u32, h: u32) -> Self {
        Self { x: 0, y: 0, w, h }
    }
    pub fn union(self, b: Self) -> Self {
        if self.w == 0 || self.h == 0 {
            return b;
        }
        if b.w == 0 || b.h == 0 {
            return self;
        }
        let x = self.x.min(b.x);
        let y = self.y.min(b.y);
        Self {
            x,
            y,
            w: (self.x + self.w).max(b.x + b.w) - x,
            h: (self.y + self.h).max(b.y + b.h) - y,
        }
    }
}
#[derive(Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Primitive {
    Rounded {
        rect: Rect,
        radius: f32,
        fill: [u8; 4],
        border: Option<([u8; 4], f32)>,
    },
    Text {
        #[serde(default)]
        align: Option<TextAlign>,
        rect: Rect,
        text: String,
        family: String,
        size: f32,
        color: [u8; 4],
    },
    Image {
        #[serde(default)]
        blur: f32,
        rect: Rect,
        path: std::path::PathBuf,
    },
    Shadow {
        rect: Rect,
        radius: f32,
        blur: f32,
    },
}
#[derive(Clone, Copy, Debug, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TextAlign {
    Left,
    Center,
    Right,
}
/// Conservative retained primitive heap and metadata bytes, without raster payloads.
pub fn primitive_bytes(p: &Primitive) -> usize {
    std::mem::size_of::<Primitive>()
        + match p {
            Primitive::Text { text, family, .. } => text.capacity() + family.capacity(),
            Primitive::Image { path, .. } => path.as_os_str().len(),
            _ => 0,
        }
}
/// Positions are logical pixels. Resources stay independent of surface buffers.
#[derive(Clone, Copy)]
pub struct Slide {
    pub closed_y: f32,
    pub open_y: f32,
    pub period_seconds: f32,
}
#[derive(Clone)]
pub struct Motion {
    pub id: String,
    pub open: bool,
    pub duration: f64,
    pub spring: bool,
    pub edge: bead_core::Edge,
    pub slide_distance: Option<f32>,
}
/// Retained logical ownership; own primitives precede the ordered child layers.
#[derive(Clone, Debug)]
pub struct SceneLayer {
    pub id: String,
    pub parent: Option<String>,
    pub own: std::ops::Range<usize>,
    pub subtree_end: usize,
    pub bounds: Rect,
}
#[derive(Clone)]
pub struct Scene {
    pub opacity: f32,
    pub motion: Option<Motion>,
    pub resource_revision: u64,
    pub asset_data: std::collections::BTreeMap<std::path::PathBuf, std::sync::Arc<[u8]>>,
    pub clips: std::collections::BTreeMap<usize, Rect>,
    pub layers: Vec<SceneLayer>,
    pub prepared: std::collections::BTreeMap<u32, std::sync::Arc<tree::Frame>>,
    pub font_files: Vec<std::path::PathBuf>,
    pub width: f32,
    pub slide: Option<Slide>,
    pub primitives: Vec<Primitive>,
    pub offset: (f32, f32),
    pub bounds: Rect,
}
impl Scene {
    pub fn bounds(&self) -> Rect {
        self.bounds.translated(self.offset.0, self.offset.1)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PixelFormat {
    Rgba,
    Bgra,
}
/// CPU target is borrowed compositor memory, never a per-frame intermediate image.
/// A future GPU backend can add its own target while retaining the same Scene.
pub struct ShmTarget<'a> {
    pub bytes: &'a mut [u8],
    pub width: u32,
    pub height: u32,
    pub scale: f32,
    pub format: PixelFormat,
}
pub enum RenderTarget<'a> {
    Shm(ShmTarget<'a>),
    #[cfg(feature = "wgpu")]
    Gpu(gpu::GpuTarget<'a>),
    #[cfg(feature = "native-gl")]
    Gl(gl::GlTarget<'a>),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum RendererKind {
    #[default]
    Cpu,
    Femtovg,
    FemtovgGl,
    Vello,
}
pub fn renderer(kind: RendererKind) -> Result<Box<dyn Renderer>> {
    match kind {
        RendererKind::Cpu => Ok(Box::<tree::TreeCpuRenderer>::default()),
        #[cfg(feature = "native-gl")]
        RendererKind::FemtovgGl => Ok(Box::<gl::GlRenderer>::default()),
        #[cfg(feature = "wgpu")]
        RendererKind::Femtovg | RendererKind::Vello => Ok(Box::new(gpu::GpuRenderer::new(kind))),
        #[allow(unreachable_patterns)]
        _ => Err("GPU support disabled; use the tiny-skia fallback".into()),
    }
}
#[cfg(any(feature = "native-gl", feature = "wgpu"))]
pub enum ResourceTarget<'a> {
    #[cfg(feature = "wgpu")]
    Wgpu {
        device: &'a gpu::wgpu::Device,
        queue: &'a gpu::wgpu::Queue,
    },
    #[cfg(feature = "native-gl")]
    Gl {
        display: &'a glutin::display::Display,
    },
}
pub trait Renderer {
    /// Per-surface presentation hint, not global activity on another surface.
    fn presentation_motion(&mut self, _active: bool) {}
    fn prepared_frame(&self) -> Option<std::sync::Arc<tree::Frame>> {
        None
    }
    fn resource_counts(&self) -> (usize, usize) {
        (0, 0)
    }
    #[cfg(any(feature = "native-gl", feature = "wgpu"))]
    fn stage_resources(&mut self, _: ResourceTarget<'_>) -> Result<()> {
        Ok(())
    }

    fn wants_gpu(&self) -> bool {
        false
    }
    fn draw(&mut self, scene: &Scene, target: RenderTarget<'_>, damage: Damage) -> Result<()> {
        match target {
            RenderTarget::Shm(t) => self.render(scene, t, damage),
            #[cfg(any(feature = "native-gl", feature = "wgpu"))]
            _ => Err("GPU target unsupported by CPU renderer".into()),
        }
    }
    fn prepare(&mut self, scene: &Scene, scale: f32) -> Result<()>;
    fn render(&mut self, scene: &Scene, target: ShmTarget<'_>, damage: Damage) -> Result<()>;
}

pub struct CpuRenderer {
    fonts: Option<SharedFontSystem>,
    glyphs: SwashCache,
    textures: bead_core::cache::Lru<String, Pixmap>,
    glyph_budget: bead_core::cache::Budget,
    glyph_charge: Option<bead_core::cache::Charge>,
    glyph_uses: HashMap<cosmic_text::CacheKey, u64>,
    glyph_clock: u64,
    mask: Option<Mask>,
    old_mask: Option<Damage>,
    mask_charge: Option<bead_core::cache::Charge>,
    used_fonts: BTreeSet<String>,
    color_glyphs: usize,
}
impl Default for CpuRenderer {
    fn default() -> Self {
        Self {
            fonts: None,
            glyphs: SwashCache::new(),
            textures: bead_core::cache::Lru::new("legacy-cpu-textures", 64 * bead_core::cache::MIB),
            glyph_budget: bead_core::cache::Budget::new("swash-images", 8 * bead_core::cache::MIB),
            glyph_charge: None,
            glyph_uses: HashMap::new(),
            glyph_clock: 0,
            mask: None,
            old_mask: None,
            mask_charge: None,
            used_fonts: BTreeSet::new(),
            color_glyphs: 0,
        }
    }
}
impl CpuRenderer {
    fn cap_glyphs(&mut self) -> Result<()> {
        self.glyph_charge = None;
        // This renderer never requests outlines. Avoid retaining dependency-only data.
        self.glyphs.outline_command_cache.clear();
        loop {
            let bytes = self
                .glyphs
                .image_cache
                .values()
                .map(|v| v.as_ref().map_or(0, |i| i.data.capacity()))
                .sum::<usize>()
                + self.glyphs.image_cache.capacity() * 192
                + self.glyph_uses.capacity() * 64;
            if let Ok(charge) = self.glyph_budget.reserve(bytes) {
                self.glyph_charge = Some(charge);
                break;
            }
            let Some(key) = self
                .glyph_uses
                .iter()
                .min_by_key(|(_, age)| **age)
                .map(|(key, _)| *key)
            else {
                self.glyphs = SwashCache::new();
                self.glyph_uses = HashMap::new();
                break;
            };
            self.glyph_uses.remove(&key);
            self.glyphs.image_cache.remove(&key);
            if self.glyph_uses.is_empty() {
                self.glyph_uses.shrink_to_fit();
            }
        }
        Ok(())
    }
}
type SharedFontSystem = std::sync::Arc<std::sync::Mutex<FontSystem>>;
#[derive(PartialEq, Eq)]
struct FontStamp {
    path: std::path::PathBuf,
    length: u64,
    modified: std::time::SystemTime,
    #[cfg(unix)]
    identity: (u64, u64),
}
struct FontPoolIndex {
    stamp: Vec<FontStamp>,
    locale: String,
    fonts: std::sync::Weak<std::sync::Mutex<FontSystem>>,
    _charge: bead_core::cache::Charge,
}
std::thread_local! {
    static FONT_POOL_INDEX: std::cell::RefCell<Option<FontPoolIndex>> = const { std::cell::RefCell::new(None) };
}
fn scene_fonts(scene: &Scene) -> Result<SharedFontSystem> {
    let fresh =
        || load_scene_fonts(scene).map(|fonts| std::sync::Arc::new(std::sync::Mutex::new(fonts)));
    // Do not cache an implicit system font database whose file set is unknown.
    if scene.font_files.is_empty() {
        return fresh();
    }
    let locale = scene_locale();
    let mut stamp = Vec::with_capacity(scene.font_files.len());
    for path in &scene.font_files {
        let metadata = std::fs::metadata(path)?;
        let Ok(modified) = metadata.modified() else {
            return fresh();
        };
        stamp.push(FontStamp {
            path: path.clone(),
            length: metadata.len(),
            modified,
            #[cfg(unix)]
            identity: {
                use std::os::unix::fs::MetadataExt;
                (metadata.dev(), metadata.ino())
            },
        });
    }
    if let Some(fonts) = FONT_POOL_INDEX.with(|slot| {
        slot.borrow()
            .as_ref()
            .filter(|cached| cached.stamp == stamp && cached.locale == locale)
            .and_then(|cached| cached.fonts.upgrade())
    }) {
        return Ok(fonts);
    }
    let fonts = fresh()?;
    let bytes = std::mem::size_of::<FontPoolIndex>()
        + stamp.capacity() * std::mem::size_of::<FontStamp>()
        + stamp.iter().map(|item| item.path.capacity()).sum::<usize>()
        + locale.capacity()
        + std::mem::size_of::<std::sync::Mutex<FontSystem>>()
        + 2 * std::mem::size_of::<usize>();
    let budget = bead_core::cache::Budget::new("weak-font-pool-index", 512 * 1024);
    FONT_POOL_INDEX.with(|slot| {
        let mut slot = slot.borrow_mut();
        *slot = None;
        if let Ok(charge) = budget.reserve(bytes) {
            *slot = Some(FontPoolIndex {
                stamp,
                locale,
                fonts: std::sync::Arc::downgrade(&fonts),
                _charge: charge,
            });
        }
    });
    // Only the renderer/caller owns font data; the bounded index cannot keep it
    // alive after the existing idle/close teardown drops the renderer's pool.
    Ok(fonts)
}
fn load_scene_fonts(scene: &Scene) -> Result<FontSystem> {
    let start = Instant::now();
    if scene.font_files.is_empty() {
        return Ok(FontSystem::new());
    }
    let mut db = fontdb::Database::new();
    for path in &scene.font_files {
        db.load_font_file(path)?;
    }
    db.set_sans_serif_family("Noto Sans");
    db.set_monospace_family("Noto Sans Mono");
    db.set_serif_family("DejaVu Serif");
    println!(
        "PROFILE stage=font_load ms={:.3} files={} faces={}",
        start.elapsed().as_secs_f64() * 1000.,
        scene.font_files.len(),
        db.len()
    );
    Ok(FontSystem::new_with_locale_and_db(scene_locale(), db))
}
fn scene_locale() -> String {
    std::env::var("LC_ALL")
        .or_else(|_| std::env::var("LANG"))
        .unwrap_or_else(|_| "en-US".into())
}
fn key(p: &Primitive, scale: f32) -> String {
    match p {
        Primitive::Text {
            rect,
            text,
            family,
            size,
            color,
            align,
        } => format!(
            "t:{scale}:{}:{}:{family}:{size}:{color:?}:{align:?}:{text}",
            rect.w, rect.h
        ),
        Primitive::Image { rect, path, blur } => {
            format!("i:{blur}:{scale}:{}:{}:{}", rect.w, rect.h, path.display())
        }
        Primitive::Shadow { rect, radius, blur } => {
            format!("s:{scale}:{}:{}:{radius}:{blur}", rect.w, rect.h)
        }
        _ => String::new(),
    }
}
fn dimensions(rect: Rect, scale: f32) -> (u32, u32) {
    (
        (rect.w * scale).ceil().max(1.) as u32,
        (rect.h * scale).ceil().max(1.) as u32,
    )
}
fn rounded(rect: Rect, r: f32) -> Option<tiny_skia::Path> {
    // SVG path parsing is supplied by tiny-skia; no custom rasterizer or shaper.
    let r = r.min(rect.w / 2.).min(rect.h / 2.);
    let x = rect.x;
    let y = rect.y;
    let w = rect.w;
    let h = rect.h;
    let mut p = PathBuilder::new();
    let k = 0.5522848;
    p.move_to(x + r, y);
    p.line_to(x + w - r, y);
    p.cubic_to(x + w - r + k * r, y, x + w, y + r - k * r, x + w, y + r);
    p.line_to(x + w, y + h - r);
    p.cubic_to(
        x + w,
        y + h - r + k * r,
        x + w - r + k * r,
        y + h,
        x + w - r,
        y + h,
    );
    p.line_to(x + r, y + h);
    p.cubic_to(x + r - k * r, y + h, x, y + h - r + k * r, x, y + h - r);
    p.line_to(x, y + r);
    p.cubic_to(x, y + r - k * r, x + r - k * r, y, x + r, y);
    p.close();
    p.finish()
}
impl Renderer for CpuRenderer {
    fn prepare(&mut self, scene: &Scene, scale: f32) -> Result<()> {
        if self.fonts.is_none()
            && scene
                .primitives
                .iter()
                .any(|p| matches!(p, Primitive::Text { .. }))
        {
            self.fonts = Some(scene_fonts(scene)?);
        }
        let before = self.textures.len();
        let mut stages = [0f64; 3];
        for p in &scene.primitives {
            if matches!(p, Primitive::Rounded { .. }) {
                continue;
            }
            let k = key(p, scale);
            if self.textures.contains_key(&k) {
                continue;
            }
            let resource_start = Instant::now();
            let pixmap = match p {
                Primitive::Text {
                    rect,
                    text,
                    family,
                    size,
                    color,
                    align,
                } => {
                    let (w, h) = dimensions(*rect, scale);
                    let mut pix = Pixmap::new(w, h).ok_or("text allocation")?;
                    let shared = self.fonts.as_ref().ok_or("font system unavailable")?;
                    let mut guard = shared.lock().map_err(|_| "font system poisoned")?;
                    let fonts: &mut FontSystem = &mut guard;
                    let mut buf =
                        Buffer::new(fonts, Metrics::new(size * scale, size * scale * 1.4));
                    buf.set_size(Some(w as f32), Some(h as f32));
                    buf.set_text(
                        text,
                        &Attrs::new().family(Family::Name(family)),
                        Shaping::Advanced,
                        align.map(|a| match a {
                            TextAlign::Left => cosmic_text::Align::Left,
                            TextAlign::Center => cosmic_text::Align::Center,
                            TextAlign::Right => cosmic_text::Align::Right,
                        }),
                    );
                    let _profile = bead_core::profile::Span::new("shape");
                    buf.shape_until_scroll(fonts, false);
                    for run in buf.layout_runs() {
                        for glyph in run.glyphs {
                            if let Some(face) = fonts.db().face(glyph.font_id) {
                                for (family, _) in &face.families {
                                    self.used_fonts.insert(family.clone());
                                }
                            }
                            let physical = glyph.physical((0., run.line_y), 1.);
                            self.glyph_clock += 1;
                            self.glyph_uses.insert(physical.cache_key, self.glyph_clock);
                            if self
                                .glyphs
                                .get_image(fonts, physical.cache_key)
                                .as_ref()
                                .is_some_and(|image| {
                                    image.content == cosmic_text::SwashContent::Color
                                })
                            {
                                self.color_glyphs += 1;
                            }
                        }
                    }
                    buf.draw(
                        fonts,
                        &mut self.glyphs,
                        TextColor::rgba(color[0], color[1], color[2], color[3]),
                        |x, y, cw, ch, c| {
                            for dy in 0..ch {
                                for dx in 0..cw {
                                    let px = x + dx as i32;
                                    let py = y + dy as i32;
                                    if px >= 0 && py >= 0 && px < w as i32 && py < h as i32 {
                                        let a = c.a() as u16;
                                        let i = (py as usize * w as usize + px as usize) * 4;
                                        // Convert coverage/color to premultiplied RGBA and blend the cached glyph.
                                        let dst = &mut pix.data_mut()[i..i + 4];
                                        let inv = 255 - a;
                                        for (d, v) in dst[..3].iter_mut().zip([c.r(), c.g(), c.b()])
                                        {
                                            *d = ((u16::from(v) * a + u16::from(*d) * inv + 127)
                                                / 255)
                                                as u8;
                                        }
                                        dst[3] = (a + (u16::from(dst[3]) * inv + 127) / 255) as u8;
                                    }
                                }
                            }
                        },
                    );
                    pix
                }
                Primitive::Image { rect, path, blur } => {
                    let pix = load_image(
                        path,
                        *rect,
                        scale,
                        scene.asset_data.get(path).map(|data| data.as_ref()),
                    )?;
                    if *blur > 0. {
                        let size = tiny_skia::IntSize::from_wh(pix.width(), pix.height())
                            .ok_or("invalid blur dimensions")?;
                        let image = image::RgbaImage::from_raw(
                            pix.width(),
                            pix.height(),
                            pix.data().to_vec(),
                        )
                        .ok_or("invalid blur image")?;
                        let blurred = image::imageops::blur(&image, blur * scale);
                        Pixmap::from_vec(blurred.into_raw(), size)
                            .ok_or("invalid blurred pixels")?
                    } else {
                        pix
                    }
                }
                Primitive::Shadow { rect, radius, blur } => {
                    shadow_pixmap(*rect, *radius, *blur, scale)?
                }
                _ => unreachable!(),
            };
            let stage = match p {
                Primitive::Text { .. } => 0,
                Primitive::Image { .. } => 1,
                _ => 2,
            };
            stages[stage] += resource_start.elapsed().as_secs_f64() * 1000.;
            let bytes = pixmap.data().len() + k.capacity() + 128;
            self.textures.insert(k, pixmap, bytes)?;
            self.cap_glyphs()?;
        }
        if scene
            .primitives
            .iter()
            .filter(|p| !matches!(p, Primitive::Rounded { .. }))
            .any(|p| !self.textures.contains_key(&key(p, scale)))
        {
            return Err("active CPU textures exceed cache budget".into());
        }
        if self.textures.len() != before {
            println!(
                "PROFILE stage=resources text_ms={:.3} images_ms={:.3} shadow_ms={:.3}",
                stages[0], stages[1], stages[2]
            );
            println!(
                "RESOURCES scale={scale} textures={} color_glyphs={} fonts={:?}",
                self.textures.len(),
                self.color_glyphs,
                self.used_fonts
            );
        }
        Ok(())
    }
    fn render(&mut self, scene: &Scene, target: ShmTarget<'_>, d: Damage) -> Result<()> {
        let ShmTarget {
            bytes,
            width,
            height,
            scale,
            format,
        } = target;
        if d.w == 0 || d.h == 0 {
            return Ok(());
        }
        if d.x + d.w > width || d.y + d.h > height {
            return Err("damage outside target".into());
        }
        // Clear only the region being reconstructed. Thus fallback BGRA never
        // needs conversion back to RGBA: every changed destination is overwritten.
        for y in d.y..d.y + d.h {
            let i = (y as usize * width as usize + d.x as usize) * 4;
            bytes[i..i + d.w as usize * 4].fill(0);
        }
        if self
            .mask
            .as_ref()
            .is_none_or(|m| m.width() != width || m.height() != height)
        {
            self.mask = None;
            self.mask_charge = None;
            self.mask_charge = Some(
                bead_core::cache::Budget::new("legacy-cpu-mask", 64 * bead_core::cache::MIB)
                    .reserve(width as usize * height as usize + 64)?,
            );
            self.mask = Some(Mask::new(width, height).ok_or("clip allocation")?);
            self.old_mask = None;
        }
        let mask = self.mask.as_mut().unwrap();
        let mut pix = PixmapMut::from_bytes(bytes, width, height).ok_or("invalid SHM target")?;
        let transform = Transform::from_scale(scale, scale).post_translate(
            (scene.offset.0 * scale).round(),
            (scene.offset.1 * scale).round(),
        );
        let logical_damage = Rect::new(
            d.x as f32 / scale - scene.offset.0,
            d.y as f32 / scale - scene.offset.1,
            d.w as f32 / scale,
            d.h as f32 / scale,
        );
        for (index, p) in scene.primitives.iter().enumerate() {
            let clipped = scene.clips.get(&index).map(|clip| {
                clip.translated(scene.offset.0, scene.offset.1)
                    .pixels(scale, width, height)
            });
            let region = if let Some(c) = clipped {
                let x = d.x.max(c.x);
                let y = d.y.max(c.y);
                Damage {
                    x,
                    y,
                    w: (d.x + d.w).min(c.x + c.w).saturating_sub(x),
                    h: (d.y + d.h).min(c.y + c.h).saturating_sub(y),
                }
            } else {
                d
            };
            if region.w == 0 || region.h == 0 {
                continue;
            }
            if self.old_mask != Some(region) {
                if let Some(old) = self.old_mask {
                    for y in old.y..old.y + old.h {
                        let i = (y * width + old.x) as usize;
                        mask.data_mut()[i..i + old.w as usize].fill(0);
                    }
                }
                for y in region.y..region.y + region.h {
                    let i = (y * width + region.x) as usize;
                    mask.data_mut()[i..i + region.w as usize].fill(255);
                }
                self.old_mask = Some(region);
            }
            let bounds = match p {
                Primitive::Rounded { rect, border, .. } => {
                    let pad = border.map_or(1., |(_, w)| w + 1.);
                    Rect::new(
                        rect.x - pad,
                        rect.y - pad,
                        rect.w + pad * 2.,
                        rect.h + pad * 2.,
                    )
                }
                Primitive::Text { rect, .. }
                | Primitive::Image { rect, .. }
                | Primitive::Shadow { rect, .. } => *rect,
            };
            if bounds.x + bounds.w < logical_damage.x
                || bounds.y + bounds.h < logical_damage.y
                || bounds.x > logical_damage.x + logical_damage.w
                || bounds.y > logical_damage.y + logical_damage.h
            {
                continue;
            }
            match p {
                Primitive::Rounded {
                    rect,
                    radius,
                    fill,
                    border,
                } => {
                    if let Some(path) = rounded(*rect, *radius) {
                        let mut paint = Paint::default();
                        paint.set_color_rgba8(fill[0], fill[1], fill[2], fill[3]);
                        pix.fill_path(&path, &paint, FillRule::Winding, transform, Some(mask));
                        if let Some((c, w)) = border {
                            paint.set_color_rgba8(c[0], c[1], c[2], c[3]);
                            pix.stroke_path(
                                &path,
                                &paint,
                                &Stroke {
                                    width: *w,
                                    ..Stroke::default()
                                },
                                transform,
                                Some(mask),
                            );
                        }
                    }
                }
                _ => {
                    let rect = match p {
                        Primitive::Text { rect, .. }
                        | Primitive::Image { rect, .. }
                        | Primitive::Shadow { rect, .. } => *rect,
                        _ => unreachable!(),
                    };
                    let tex = self
                        .textures
                        .get(&key(p, scale))
                        .ok_or("resource not prepared")?;
                    pix.draw_pixmap(
                        ((rect.x + scene.offset.0) * scale).round() as i32,
                        ((rect.y + scene.offset.1) * scale).round() as i32,
                        tex.as_ref(),
                        &PixmapPaint::default(),
                        Transform::identity(),
                        Some(mask),
                    );
                }
            }
        }
        if format == PixelFormat::Bgra {
            swizzle(bytes, width, d);
        }
        Ok(())
    }
}
/// Only touched pixels are converted for mandatory ARGB8888 compositors.
pub fn swizzle(bytes: &mut [u8], width: u32, d: Damage) {
    for y in d.y..d.y + d.h {
        let i = (y * width + d.x) as usize * 4;
        for px in bytes[i..i + d.w as usize * 4]
            .as_chunks_mut::<4>()
            .0
            .iter_mut()
        {
            px.swap(0, 2);
        }
    }
}
fn shadow_svg(w: f32, h: f32, radius: f32, blur: f32) -> String {
    let pad = blur * 3.;
    format!(
        "<svg xmlns='http://www.w3.org/2000/svg' width='{w}' height='{h}'><defs><filter id='s' x='-50%' y='-50%' width='200%' height='200%'><feGaussianBlur stdDeviation='{blur}'/></filter></defs><rect x='{pad}' y='{pad}' width='{}' height='{}' rx='{radius}' fill='black' fill-opacity='.32' filter='url(#s)'/></svg>",
        w - 2. * pad,
        h - 2. * pad
    )
}
fn shadow_pixmap(rect: Rect, radius: f32, blur: f32, scale: f32) -> Result<Pixmap> {
    let (w, h) = dimensions(rect, scale);
    // Preserve the same Gaussian corners/edges, stretching only straight sections.
    // Bake a small template with enough room for radius + 3 sigma on either side.
    let cut = ((radius + 6. * blur) * scale).ceil() as u32;
    let side = cut * 2 + 2;
    if cut == 0 || w < side || h < side {
        return svg_pixmap(shadow_svg(rect.w, rect.h, radius, blur).as_bytes(), w, h);
    }
    let logical_side = side as f32 / scale;
    let template = svg_pixmap(
        shadow_svg(logical_side, logical_side, radius, blur).as_bytes(),
        side,
        side,
    )?;
    let mut result = Pixmap::new(w, h).ok_or("shadow allocation")?;
    let src = [0, cut, side - cut, side];
    let xs = [0, cut, w - cut, w];
    let ys = [0, cut, h - cut, h];
    for row in 0..3 {
        for col in 0..3 {
            let tile = template
                .clone_rect(
                    tiny_skia::IntRect::from_xywh(
                        src[col] as i32,
                        src[row] as i32,
                        src[col + 1] - src[col],
                        src[row + 1] - src[row],
                    )
                    .ok_or("shadow tile")?,
                )
                .ok_or("shadow crop")?;
            // Separable nine-patch: corners stay at native resolution, edge tangents stretch.
            let tr = Transform::from_scale(
                (xs[col + 1] - xs[col]) as f32 / tile.width() as f32,
                (ys[row + 1] - ys[row]) as f32 / tile.height() as f32,
            )
            .post_translate(xs[col] as f32, ys[row] as f32);
            result.draw_pixmap(0, 0, tile.as_ref(), &PixmapPaint::default(), tr, None);
        }
    }
    Ok(result)
}
fn svg_pixmap(data: &[u8], w: u32, h: u32) -> Result<Pixmap> {
    let tree = resvg::usvg::Tree::from_data(data, &resvg::usvg::Options::default())?;
    let mut pix = Pixmap::new(w, h).ok_or("SVG allocation")?;
    let s = tree.size();
    resvg::render(
        &tree,
        Transform::from_scale(w as f32 / s.width(), h as f32 / s.height()),
        &mut pix.as_mut(),
    );
    Ok(pix)
}
/// Validate an immutable asset snapshot with the same decoder used for drawing.
pub fn validate_asset(path: &Path, bytes: &[u8]) -> Result<()> {
    load_image(path, Rect::new(0., 0., 4., 4.), 1., Some(bytes)).map(|_| ())
}
fn load_image(path: &Path, rect: Rect, scale: f32, snapshot: Option<&[u8]>) -> Result<Pixmap> {
    let file;
    let bytes = if let Some(snapshot) = snapshot {
        snapshot
    } else {
        file = std::fs::read(path)?;
        &file
    };
    let (w, h) = dimensions(rect, scale);
    if path.extension().is_some_and(|e| e == "svg") {
        return svg_pixmap(bytes, w, h);
    }
    let raw = image::load_from_memory(bytes)?
        .resize_exact(w, h, image::imageops::FilterType::Lanczos3)
        .to_rgba8();
    let mut pix = Pixmap::new(w, h).ok_or("image allocation")?;
    for (dst, src) in pix
        .data_mut()
        .as_chunks_mut::<4>()
        .0
        .iter_mut()
        .zip(raw.as_raw().as_chunks::<4>().0.iter())
    {
        let a = u16::from(src[3]);
        for i in 0..3 {
            dst[i] = ((u16::from(src[i]) * a + 127) / 255) as u8;
        }
        dst[3] = src[3];
    }
    Ok(pix)
}
/// Reads Orient format 1 only. Unknown metadata is intentionally ignored.
#[derive(Clone)]
pub struct Palette(pub HashMap<String, [u8; 4]>);
impl Palette {
    pub fn read(path: &Path) -> Result<Self> {
        Self::parse(&std::fs::read(path)?)
    }
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        let v: serde_json::Value = serde_json::from_slice(bytes)?;
        if v["formatVersion"] != 1 {
            return Err("unsupported Orient palette formatVersion".into());
        }
        let mut colors = HashMap::new();
        for (role, value) in v["colours"].as_object().ok_or("missing colours")? {
            let s = value.as_str().ok_or("color must be a string")?;
            if s.len() != 6 || !s.bytes().all(|b| b.is_ascii_hexdigit()) {
                return Err(format!("invalid color role {role}").into());
            }
            let n = u32::from_str_radix(s, 16)?;
            colors.insert(
                role.clone(),
                [(n >> 16) as u8, (n >> 8) as u8, n as u8, 255],
            );
        }
        Ok(Self(colors))
    }
    pub fn get(&self, name: &str) -> Result<[u8; 4]> {
        self.0
            .get(name)
            .copied()
            .ok_or_else(|| format!("missing palette role {name}").into())
    }
}
/// Measure a UTF-8 boundary with the same shaper/font selection as painted text.
pub fn text_boundary_x(
    scene: &Scene,
    text: &str,
    boundary: usize,
    family: &str,
    size: f32,
    width: f32,
) -> Result<f32> {
    let shared = scene_fonts(scene)?;
    let mut guard = shared.lock().map_err(|_| "font system poisoned")?;
    let fonts: &mut FontSystem = &mut guard;
    boundary_x(fonts, text, boundary, family, size, width)
}
fn boundary_x(
    fonts: &mut FontSystem,
    text: &str,
    boundary: usize,
    family: &str,
    size: f32,
    width: f32,
) -> Result<f32> {
    let mut buffer = Buffer::new(fonts, Metrics::new(size, size * 1.4));
    buffer.set_size(Some(width), Some(size * 2.));
    buffer.set_text(
        text,
        &Attrs::new().family(Family::Name(family)),
        Shaping::Advanced,
        None,
    );
    buffer.shape_until_scroll(fonts, false);
    let Some(run) = buffer.layout_runs().next() else {
        return Ok(0.);
    };
    for glyph in run.glyphs {
        if boundary <= glyph.start {
            return Ok(glyph.x);
        }
        if boundary < glyph.end {
            return Ok(glyph.x + glyph.w);
        }
    }
    Ok(run.line_w)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn font_pool_scene() -> Scene {
        let font = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../bead-test/fonts/IBMPlexSans.ttf");
        Scene {
            opacity: 1.,
            motion: None,
            resource_revision: 1,
            asset_data: Default::default(),
            clips: Default::default(),
            layers: vec![],
            prepared: Default::default(),
            font_files: vec![font],
            width: 240.,
            slide: None,
            offset: (0., 0.),
            bounds: Rect::new(0., 0., 240., 40.),
            primitives: vec![Primitive::Text {
                rect: Rect::new(0., 0., 240., 32.),
                text: "office café".into(),
                family: "IBM Plex Sans".into(),
                size: 17.,
                color: [13, 27, 41, 255],
                align: None,
            }],
        }
    }
    #[test]
    fn caret_uses_existing_font_pool_with_exact_advances_and_no_strong_index_owner() {
        let scene = font_pool_scene();
        let mut renderer = CpuRenderer::default();
        renderer.prepare(&scene, 1.).unwrap();
        let held = renderer.fonts.as_ref().unwrap().clone();
        let weak = std::sync::Arc::downgrade(&held);
        assert!(std::sync::Arc::ptr_eq(&held, &scene_fonts(&scene).unwrap()));
        let mut fresh = load_scene_fonts(&scene).unwrap();
        let text = "office café";
        for boundary in text
            .char_indices()
            .map(|(i, _)| i)
            .chain(std::iter::once(text.len()))
        {
            assert_eq!(
                text_boundary_x(&scene, text, boundary, "IBM Plex Sans", 17., 240.).unwrap(),
                boundary_x(&mut fresh, text, boundary, "IBM Plex Sans", 17., 240.).unwrap()
            );
        }
        FONT_POOL_INDEX
            .with(|slot| assert!(slot.borrow().as_ref().unwrap()._charge.bytes() <= 512 * 1024));
        drop(held);
        drop(renderer);
        assert!(
            weak.upgrade().is_none(),
            "the index must not retain the font system after renderer teardown"
        );
    }
    #[test]
    fn font_pool_index_invalidates_changed_file_metadata() {
        struct Temp(std::path::PathBuf);
        impl Drop for Temp {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir =
            std::env::temp_dir().join(format!("bead-font-index-{}-{nonce}", std::process::id()));
        std::fs::create_dir(&dir).unwrap();
        let guard = Temp(dir);
        let mut scene = font_pool_scene();
        let path = guard.0.join("fixture.ttf");
        std::fs::copy(&scene.font_files[0], &path).unwrap();
        scene.font_files = vec![path.clone()];
        let first = scene_fonts(&scene).unwrap();
        assert!(std::sync::Arc::ptr_eq(
            &first,
            &scene_fonts(&scene).unwrap()
        ));
        std::fs::File::options()
            .write(true)
            .open(&path)
            .unwrap()
            .set_modified(std::time::SystemTime::now() + std::time::Duration::from_secs(5))
            .unwrap();
        let second = scene_fonts(&scene).unwrap();
        assert!(!std::sync::Arc::ptr_eq(&first, &second));
    }
    #[test]
    fn clipped_primitives_leave_pixels_outside_the_scroll_viewport_intact() {
        let scene = Scene {
            opacity: 1.,
            motion: None,
            resource_revision: 1,
            asset_data: Default::default(),
            clips: std::collections::BTreeMap::from([(1, Rect::new(0., 0., 2., 2.))]),
            layers: Vec::new(),
            prepared: Default::default(),
            font_files: vec![],
            width: 4.,
            slide: None,
            offset: (0., 0.),
            bounds: Rect::new(0., 0., 4., 4.),
            primitives: vec![
                Primitive::Rounded {
                    rect: Rect::new(0., 0., 4., 4.),
                    radius: 0.,
                    fill: [0, 255, 0, 255],
                    border: None,
                },
                Primitive::Rounded {
                    rect: Rect::new(0., 0., 4., 4.),
                    radius: 0.,
                    fill: [255, 0, 0, 255],
                    border: None,
                },
            ],
        };
        let mut renderer = CpuRenderer::default();
        let mut bytes = vec![0; 64];
        renderer.prepare(&scene, 1.).unwrap();
        renderer
            .render(
                &scene,
                ShmTarget {
                    bytes: &mut bytes,
                    width: 4,
                    height: 4,
                    scale: 1.,
                    format: PixelFormat::Rgba,
                },
                Damage::full(4, 4),
            )
            .unwrap();
        assert_eq!(&bytes[0..4], &[255, 0, 0, 255]);
        assert_eq!(&bytes[12..16], &[0, 255, 0, 255]);
    }
    #[test]
    fn rejects_invalid_palette_versions_and_colors() {
        assert!(Palette::parse(br#"{"formatVersion":2,"colours":{}}"#).is_err());
        assert!(
            Palette::parse(br##"{"formatVersion":1,"colours":{"surface":"#ff0000"}}"##).is_err()
        );
        assert_eq!(
            Palette::parse(br#"{"formatVersion":1,"colours":{"surface":"ff0000"},"future":42}"#)
                .unwrap()
                .get("surface")
                .unwrap(),
            [255, 0, 0, 255]
        );
    }
    #[test]
    fn aged_partial_buffers_match_full_redraw_in_both_formats() {
        for format in [PixelFormat::Rgba, PixelFormat::Bgra] {
            let mut renderer = CpuRenderer::default();
            let mut reference = CpuRenderer::default();
            let mut buffers = [
                vec![0; 120 * 90 * 4],
                vec![0; 120 * 90 * 4],
                vec![0; 120 * 90 * 4],
            ];
            let mut pending = [Damage::full(120, 90); 3];
            let mut full = vec![0; 120 * 90 * 4];
            let mut previous = Rect::new(0., 0., 0., 0.);
            for frame in 0..30 {
                let mut scene = Scene {
                    opacity: 1.,
                    motion: None,
                    resource_revision: 0,
                    asset_data: Default::default(),
                    clips: Default::default(),
                    layers: Vec::new(),
                    prepared: Default::default(),
                    font_files: Vec::new(),
                    width: 40.,
                    slide: None,
                    offset: ((frame * 7 % 60) as f32, (frame * 3 % 30) as f32),
                    bounds: Rect::new(0., 0., 40., 30.),
                    primitives: vec![
                        Primitive::Rounded {
                            rect: Rect::new(1., 1., 38., 28.),
                            radius: 7.,
                            fill: [210, 20, 40, 180],
                            border: Some(([20, 190, 230, 255], 1.)),
                        },
                        Primitive::Text {
                            rect: Rect::new(3., 3., 34., 20.),
                            text: "A 😀".into(),
                            family: "sans-serif".into(),
                            align: None,
                            size: 10.,
                            color: [20, 120, 210, 255],
                        },
                        Primitive::Image {
                            blur: 0.,
                            rect: Rect::new(14., 8., 15., 15.),
                            path: Path::new(env!("CARGO_MANIFEST_DIR"))
                                .join("../../shell/sample.png"),
                        },
                        Primitive::Shadow {
                            rect: Rect::new(0., 0., 40., 30.),
                            radius: 4.,
                            blur: 2.,
                        },
                    ],
                };
                // Include fractional scaling and abrupt movement / erasure.
                if frame % 7 == 0 {
                    scene.offset = (-20., -10.);
                }
                let damage = previous.union(scene.bounds()).pixels(1.5, 120, 90);
                previous = scene.bounds();
                for p in &mut pending {
                    *p = p.union(damage);
                }
                let index = frame as usize % 3;
                renderer.prepare(&scene, 1.5).unwrap();
                reference.prepare(&scene, 1.5).unwrap();
                renderer
                    .render(
                        &scene,
                        ShmTarget {
                            bytes: &mut buffers[index],
                            width: 120,
                            height: 90,
                            scale: 1.5,
                            format,
                        },
                        pending[index],
                    )
                    .unwrap();
                pending[index] = Damage::full(0, 0);
                reference
                    .render(
                        &scene,
                        ShmTarget {
                            bytes: &mut full,
                            width: 120,
                            height: 90,
                            scale: 1.5,
                            format,
                        },
                        Damage::full(120, 90),
                    )
                    .unwrap();
                assert_eq!(buffers[index], full, "frame {frame}, {format:?}");
            }
        }
    }
    #[test]
    fn nine_patch_shadow_preserves_gaussian_appearance() {
        let rect = Rect::new(0., 0., 500., 300.);
        let scale = 1.5;
        let (w, h) = dimensions(rect, scale);
        let exact = svg_pixmap(shadow_svg(rect.w, rect.h, 12., 10.).as_bytes(), w, h).unwrap();
        let fast = shadow_pixmap(rect, 12., 10., scale).unwrap();
        let max = exact
            .data()
            .iter()
            .zip(fast.data())
            .map(|(a, b)| a.abs_diff(*b))
            .max()
            .unwrap();
        assert!(max <= 2, "shadow channel difference {max}/255");
    }
}
