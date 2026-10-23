// SPDX-License-Identifier: MIT
//! Deterministic headless tests through Bead's actual frontend, stores and CPU tree.
use bead_core::Value;
use bead_render::{
    Damage, Palette, PixelFormat, Primitive, Rect, Renderer, Scene, ShmTarget,
    tree::TreeCpuRenderer,
};
use bead_ui::Session;
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};
pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
#[derive(Clone, Copy)]
pub struct Config {
    pub width: u32,
    pub height: u32,
    pub scale: f32,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            width: 800,
            height: 600,
            scale: 1.,
        }
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Counters {
    pub layouts: usize,
    /// Retained content/group builds; pixel compositing itself is separate.
    pub rasters: usize,
    pub direct_composites: usize,
}
pub enum Input {
    Pointer {
        x: f32,
        y: f32,
    },
    Click {
        x: f32,
        y: f32,
    },
    /// Pointer motion with the primary button held (after a `Click`).
    Drag {
        x: f32,
        y: f32,
    },
    /// The primary button was released.
    Release,
    Text(String),
    Backspace,
    Tab {
        backwards: bool,
    },
    Edit {
        key: bead_ui::input::EditKey,
        shift: bool,
    },
    Scroll {
        x: f32,
        y: f32,
        delta: f64,
    },
}
pub struct Harness {
    pub session: Session,
    scene: Scene,
    renderer: TreeCpuRenderer,
    config: Config,
    layout_baseline: usize,
    icons: BTreeMap<String, PathBuf>,
}
impl Harness {
    pub fn load(
        root: &Path,
        entry: &Path,
        palette: &Path,
        fixture: BTreeMap<String, Value>,
        config: Config,
    ) -> Result<Self> {
        Self::load_with_icons(root, entry, palette, fixture, config, BTreeMap::new())
    }
    pub fn load_with_icons(
        root: &Path,
        entry: &Path,
        palette: &Path,
        fixture: BTreeMap<String, Value>,
        config: Config,
        icons: BTreeMap<String, PathBuf>,
    ) -> Result<Self> {
        Self::load_resources(root, entry, palette, fixture, config, icons, None)
    }
    pub fn load_with_recovery(
        root: &Path,
        entry: &Path,
        palette: &Path,
        fixture: BTreeMap<String, Value>,
        config: Config,
        recovery: PathBuf,
    ) -> Result<Self> {
        Self::load_resources(
            root,
            entry,
            palette,
            fixture,
            config,
            BTreeMap::new(),
            Some(recovery),
        )
    }
    fn load_resources(
        root: &Path,
        entry: &Path,
        palette: &Path,
        fixture: BTreeMap<String, Value>,
        config: Config,
        icons: BTreeMap<String, PathBuf>,
        recovery: Option<PathBuf>,
    ) -> Result<Self> {
        let layout_baseline = bead_ui::layout::layout_passes();
        if config.width == 0
            || config.height == 0
            || !config.scale.is_finite()
            || config.scale <= 0.
            || config.scale > 4.
            || (f64::from(config.width) * f64::from(config.scale)).ceil()
                * (f64::from(config.height) * f64::from(config.scale)).ceil()
                > 16_777_216.
        {
            return Err("invalid headless dimensions/scale".into());
        }
        let fallback = Scene {
            opacity: 1.,
            motion: None,
            resource_revision: 0,
            asset_data: Default::default(),
            clips: Default::default(),
            layers: vec![],
            prepared: Default::default(),
            font_files: vec![],
            width: config.width as f32,
            slide: None,
            primitives: vec![],
            offset: (0., 0.),
            bounds: Rect::new(0., 0., config.width as f32, config.height as f32),
        };
        let mut session = Session::new(
            root.canonicalize()?,
            entry.into(),
            std::path::absolute(palette)?,
            Palette::read(palette).or_else(|error| {
                if recovery.is_some() {
                    Ok(Palette(Default::default()))
                } else {
                    Err(error)
                }
            })?,
            fallback.clone(),
        );
        if let Some(path) = recovery {
            session.enable_recovery(path);
        }
        session.stores = session.stores.reconcile(&fixture)?;
        session.set_viewport(config.width, config.height);
        let scene = session.reload()?;
        session.check_sources()?;
        let mut test = Self {
            session,
            scene: fallback,
            renderer: Default::default(),
            config,
            layout_baseline,
            icons,
        };
        test.commit(scene)?;
        Ok(test)
    }
    fn commit(&mut self, mut scene: Scene) -> Result<()> {
        // Font paths are injected before baking; no fontconfig or system fallback.
        let fonts = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fonts");
        scene.font_files = vec![
            fonts.join("IBMPlexSans.ttf"),
            fonts.join("FixtureSymbols.ttf"),
        ];
        for primitive in &mut scene.primitives {
            if let Primitive::Text { family, .. } = primitive {
                *family = if family.contains("Symbols") {
                    "Bead Test Symbols"
                } else {
                    "IBM Plex Sans"
                }
                .into();
            }
        }
        for primitive in &mut scene.primitives {
            if let Primitive::Image { path, .. } = primitive {
                if let Some(icon) = path.to_str().and_then(|p| p.strip_prefix("@icon:")) {
                    *path = self
                        .icons
                        .get(icon)
                        .ok_or_else(|| format!("missing deterministic icon fixture: {icon}"))?
                        .clone();
                }
                scene
                    .asset_data
                    .insert(path.clone(), std::sync::Arc::from(std::fs::read(&*path)?));
            }
        }
        scene.prepared.clear();
        self.renderer.prepare(&scene, self.config.scale)?;
        self.session.commit(scene.clone());
        self.scene = scene;

        Ok(())
    }
    pub fn reload(&mut self) -> Result<()> {
        let scene = self.session.reload()?;
        self.session.check_sources()?;
        self.commit(scene)
    }
    /// Simulates the host presentation acknowledgement after a successful CPU frame.
    pub fn presented(&mut self) -> Result<()> {
        self.render()?;
        self.session.presented(self.scene.resource_revision)?;
        Ok(())
    }
    pub fn update(&mut self, values: BTreeMap<String, Value>) -> Result<()> {
        if let Some(scene) = self.session.update(values)? {
            self.commit(scene)?;
        }
        Ok(())
    }
    pub fn script(&mut self, inputs: impl IntoIterator<Item = Input>) -> Result<()> {
        for input in inputs {
            let scene = match input {
                Input::Pointer { x, y } => self.session.hover(x, y)?,
                Input::Click { x, y } => self.session.click(x, y)?,
                Input::Drag { x, y } => self.session.drag(x, y)?,
                Input::Release => self.session.release()?,
                Input::Text(text) => self.session.key(&text, false)?,
                Input::Backspace => self.session.key("", true)?,
                Input::Tab { backwards } => self.session.focus_next(backwards)?,
                Input::Edit { key, shift } => self.session.edit_key(key, shift)?,
                Input::Scroll { x, y, delta } => self.session.scroll(x, y, delta)?,
            };
            if let Some(scene) = scene {
                self.commit(scene)?;
            }
        }
        Ok(())
    }
    pub fn counters(&self) -> Counters {
        Counters {
            layouts: bead_ui::layout::layout_passes().wrapping_sub(self.layout_baseline),
            rasters: self.renderer.cache.builds + self.renderer.opacity_builds,
            direct_composites: self.renderer.direct_composites,
        }
    }
    pub fn scene(&self) -> &Scene {
        &self.scene
    }
    pub fn accessibility(&self) -> accesskit::TreeUpdate {
        self.session.accessibility_tree()
    }
    pub fn assert_accessible(&self, label: &str, role: accesskit::Role) -> Result<()> {
        if self
            .accessibility()
            .nodes
            .iter()
            .any(|(_, n)| n.role() == role && n.label().or_else(|| n.value()) == Some(label))
        {
            Ok(())
        } else {
            Err(format!("missing accessible {role:?}: {label}").into())
        }
    }
    pub fn presentation_motion(&mut self, active: bool) {
        self.renderer.presentation_motion(active);
    }
    pub fn opaque_composition(&mut self, mode: bead_render::tree::OpaqueComposition) {
        self.renderer.opaque_composition = mode;
    }
    pub fn render(&mut self) -> Result<Frame> {
        self.pose(self.scene.offset, self.scene.opacity)
    }
    /// Presentation-only transform/opacity, without invoking the frontend/layout.
    pub fn pose(&mut self, offset: (f32, f32), opacity: f32) -> Result<Frame> {
        if !offset.0.is_finite()
            || !offset.1.is_finite()
            || !opacity.is_finite()
            || !(0. ..=1.).contains(&opacity)
        {
            return Err("invalid pose".into());
        }
        let mut scene = self.scene.clone();
        scene.offset = offset;
        scene.opacity = opacity;
        let (width, height) = (
            (self.config.width as f32 * self.config.scale).ceil() as u32,
            (self.config.height as f32 * self.config.scale).ceil() as u32,
        );
        let mut pixels = vec![0; width as usize * height as usize * 4];
        self.renderer.render(
            &scene,
            ShmTarget {
                bytes: &mut pixels,
                width,
                height,
                scale: self.config.scale,
                format: PixelFormat::Rgba,
            },
            Damage::full(width, height),
        )?;
        // Export straight-alpha PNG, while the real CPU target stays premultiplied.
        for p in pixels.as_chunks_mut::<4>().0 {
            let alpha = u32::from(p[3]);
            if alpha > 0 && alpha < 255 {
                for c in &mut p[..3] {
                    *c = (u32::from(*c) * 255 / alpha).min(255) as u8;
                }
            }
        }
        Ok(Frame {
            width,
            height,
            pixels,
        })
    }
}
pub struct Frame {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}
#[derive(Clone, Copy)]
pub struct Tolerance {
    pub channel: u8,
    pub changed_fraction: f64,
}
impl Default for Tolerance {
    fn default() -> Self {
        Self {
            channel: 2,
            changed_fraction: 0.001,
        }
    }
}
impl Frame {
    pub fn load(path: &Path) -> Result<Self> {
        let image = image::open(path)?.into_rgba8();
        Ok(Self {
            width: image.width(),
            height: image.height(),
            pixels: image.into_raw(),
        })
    }
    pub fn save(&self, path: &Path) -> Result<()> {
        image::save_buffer_with_format(
            path,
            &self.pixels,
            self.width,
            self.height,
            image::ColorType::Rgba8,
            image::ImageFormat::Png,
        )?;
        Ok(())
    }
    pub fn compare(&self, golden: &Path, tolerance: Tolerance, artifacts: &Path) -> Result<()> {
        if !tolerance.changed_fraction.is_finite()
            || !(0. ..=1.).contains(&tolerance.changed_fraction)
        {
            return Err("invalid golden tolerance".into());
        }
        let expected = image::open(golden)?.into_rgba8();
        std::fs::create_dir_all(artifacts)?;
        if expected.dimensions() != (self.width, self.height) {
            self.save(&artifacts.join("actual.png"))?;
            return Err("golden dimensions changed".into());
        }
        let mut diff = vec![0; self.pixels.len()];
        let mut changed = 0usize;
        for ((actual, expected), difference) in self
            .pixels
            .as_chunks::<4>()
            .0
            .iter()
            .zip(expected.as_raw().as_chunks::<4>().0.iter())
            .zip(diff.as_chunks_mut::<4>().0)
        {
            let maximum = actual
                .iter()
                .zip(expected)
                .map(|(a, b)| a.abs_diff(*b))
                .max()
                .unwrap_or(0);
            if maximum > tolerance.channel {
                changed += 1;
                difference.copy_from_slice(&[255, 0, maximum, 255]);
            }
        }
        let fraction = changed as f64 / (self.width as f64 * self.height as f64);
        if fraction > tolerance.changed_fraction {
            self.save(&artifacts.join("actual.png"))?;
            Frame {
                width: self.width,
                height: self.height,
                pixels: diff,
            }
            .save(&artifacts.join("diff.png"))?;
            return Err(format!(
                "golden mismatch: {changed} pixels ({:.4}%)",
                fraction * 100.
            )
            .into());
        }
        Ok(())
    }
}
#[derive(Clone, Copy, serde::Serialize, serde::Deserialize)]
pub struct FrameTime {
    pub input_ns: u64,
    pub presented_ns: u64,
    pub cpu_ns: u64,
}
#[derive(Clone, Copy)]
pub struct Budget {
    pub presented_p99_ms: f64,
    pub cpu_per_frame_ms: f64,
}
/// Replay recorded numeric boundaries. No wall-clock performance claims from CI.
pub fn replay(samples: &[FrameTime], budget: Budget) -> Result<()> {
    if samples.is_empty()
        || !budget.presented_p99_ms.is_finite()
        || !budget.cpu_per_frame_ms.is_finite()
        || budget.presented_p99_ms <= 0.
        || budget.cpu_per_frame_ms <= 0.
    {
        return Err("invalid frame replay/budget".into());
    }
    let mut latency = Vec::new();
    let mut cpu_max = 0u64;
    for sample in samples {
        latency.push(
            sample
                .presented_ns
                .checked_sub(sample.input_ns)
                .ok_or("presentation precedes input")?,
        );
        cpu_max = cpu_max.max(sample.cpu_ns);
    }
    latency.sort_unstable();
    let p99 = latency[(latency.len() * 99).div_ceil(100).saturating_sub(1)] as f64 / 1e6;
    let cpu = cpu_max as f64 / 1e6;
    if p99 > budget.presented_p99_ms || cpu > budget.cpu_per_frame_ms {
        return Err(
            format!("frame budget exceeded: presented p99={p99:.3}ms, CPU max={cpu:.3}ms").into(),
        );
    }
    Ok(())
}

pub mod source;
