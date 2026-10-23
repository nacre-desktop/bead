// SPDX-License-Identifier: GPL-3.0-only
//! Nacre's drawers/layout.js, e0a81ce7. Keep application policy out of bead-wayland.
use bead::{
    core::Edge,
    render::{Rect, Scene},
    wayland::PanelPositioner,
};
use std::sync::Arc;

#[derive(Clone, Copy, Debug)]
pub enum Attachment {
    Attached(Edge),
    /// Compact Nacre control panel: joins the top frame at the usable top-right.
    ControlCenter,
    Popout,
}

#[derive(Clone, Copy, Debug)]
pub struct Frame {
    pub left: f32,
    pub right: f32,
    pub top: f32,
    pub bottom: f32,
    pub rounding: f32,
}
impl Default for Frame {
    fn default() -> Self {
        // NacreFrame defaults: 40px header + 10px frame, 25px contour rounding.
        Self {
            left: 10.,
            right: 10.,
            top: 50.,
            bottom: 10.,
            rounding: 25.,
        }
    }
}

pub fn attached(edge: Edge, viewport: (f32, f32), size: (f32, f32), offset: f32) -> (f32, f32) {
    let centered_x = ((viewport.0 - size.0) / 2.).max(0.);
    let centered_y = ((viewport.1 - size.1) / 2.).max(0.);
    match edge {
        Edge::Left => (0., centered_y),
        Edge::Right => ((viewport.0 - size.0 - offset).max(0.), centered_y),
        Edge::Bottom => (centered_x, (viewport.1 - size.1).max(0.)),
        Edge::Top => (centered_x, 0.),
    }
}

pub fn popout(
    viewport_x: f32,
    viewport_width: f32,
    center: f32,
    width: f32,
    target_width: f32,
    corner: f32,
) -> (f32, bool) {
    let relative_center = center - viewport_x;
    let safe_corner = corner.min(viewport_width / 2.).max(0.);
    let dock_right = relative_center + target_width / 2. >= viewport_width - safe_corner;
    let right_limit = (viewport_width - width).max(0.);
    let centered = relative_center - width / 2.;
    (
        if dock_right {
            right_limit
        } else {
            right_limit.min(safe_corner.max(centered))
        },
        dock_right,
    )
}

pub fn body(scene: &Scene) -> Rect {
    scene
        .primitives
        .iter()
        .find_map(|p| match p {
            bead::render::Primitive::Rounded { rect, .. } if rect.w > 0. && rect.h > 0. => {
                Some(*rect)
            }
            bead::render::Primitive::Image { rect, path, .. }
                if path.to_string_lossy().starts_with("@joined-frame-") =>
            {
                let top = if path.to_string_lossy().starts_with("@joined-frame-32-") {
                    32.
                } else {
                    2.
                };
                Some(Rect::new(
                    rect.x + 32.,
                    rect.y + top,
                    rect.w - 64.,
                    rect.h - 32. - top,
                ))
            }
            _ => None,
        })
        .unwrap_or(Rect::new(
            0.,
            0.,
            scene.width,
            (scene.bounds.h - 64.).max(0.),
        ))
}

static SHEEN: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);
/// `frameSheen` (B0036): the 1 px gradient rim along the frame. Off draws no rim.
pub fn set_sheen(on: bool) {
    SHEEN.store(on, std::sync::atomic::Ordering::SeqCst);
}
fn sheen() -> bool {
    SHEEN.load(std::sync::atomic::Ordering::SeqCst)
}

/// Worker-owned and transient. No module, raster or generated asset survives the
/// existing panel worker's close/idle retirement. Renderer caches remain capped.
pub struct Chrome {
    pub center: Option<f32>,
    pub output: (u32, u32),
    attachment: Attachment,
    engaged: bool,
    fill: [u8; 4],
    primary: [u8; 4],
    secondary: [u8; 4],
}
impl Chrome {
    pub fn set_engaged(&mut self, engaged: bool) {
        self.engaged = engaged;
    }
    pub fn new(
        attachment: Attachment,
        colors: &bead::render::Palette,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        Ok(Self {
            center: None,
            output: (1920, 1200),
            attachment,
            engaged: matches!(
                attachment,
                Attachment::Popout
                    | Attachment::ControlCenter
                    | Attachment::Attached(Edge::Left | Edge::Right)
            ),
            fill: colors.get("surface")?,
            primary: colors.get("primary")?,
            secondary: colors.get("secondary")?,
        })
    }
    fn svg(
        &self,
        output: (f32, f32),
        rectangles: &[Rect],
        crop: Rect,
        engaged: bool,
    ) -> Result<String, Box<dyn std::error::Error>> {
        let radius = 25_f64
            .min(((output.0 - 20.).max(0.) / 2.) as f64)
            .min(((output.1 - 60.).max(0.) / 2.) as f64);
        let contour =
            crate::contour::geometry(output.0 as f64, output.1 as f64, rectangles, radius)?;
        let rgb = |c: [u8; 4]| format!("rgb({},{},{})", c[0], c[1], c[2]);
        let highlight = std::array::from_fn::<_, 4, _>(|i| {
            if i == 3 {
                255
            } else {
                (self.primary[i] as f32 * 0.78 + [255., 249., 241.][i] * 0.22).round() as u8
            }
        });
        let rim = if sheen() {
            format!(
                r#"<path d="{}" fill="none" stroke="url(#rim)" stroke-width="1" opacity="{}" stroke-linecap="butt"/>"#,
                contour.rim,
                if engaged { 0.25 } else { 0.16 }
            )
        } else {
            String::new()
        };
        Ok(format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="{}" height="{}" viewBox="{} {} {} {}"><defs><linearGradient id="rim" gradientUnits="userSpaceOnUse" x1="0" y1="0" x2="{}" y2="{}"><stop stop-color="{}"/><stop offset=".38" stop-color="{}"/><stop offset="1" stop-color="{}"/></linearGradient></defs><path d="{}" fill="{}" fill-rule="evenodd"/>{}</svg>"#,
            crop.w,
            crop.h,
            crop.x,
            crop.y,
            crop.w,
            crop.h,
            output.0,
            output.1,
            rgb(highlight),
            rgb(self.primary),
            rgb(self.secondary),
            contour.body,
            rgb(self.fill),
            rim
        ))
    }
    pub fn decorations(self) -> bead::wayland::DecorationFactory {
        Arc::new(move |(w, h), engaged, panel| {
            let (w, h) = (w as f32, h as f32);
            let radius = 25_f32
                .min((w - 20.).max(0.) / 2.)
                .min((h - 60.).max(0.) / 2.);
            let mut material = vec![
                Rect::new(0., 0., w, 50.),
                Rect::new(0., 0., 10., h),
                Rect::new(w - 10., 0., 10., h),
                Rect::new(0., h - 10., w, 10.),
            ];
            if let Some(panel) = panel {
                material.push(body(panel.scene).translated(
                    panel.origin.0 as f32 - panel.scene.bounds.x,
                    panel.origin.1 as f32 - panel.scene.bounds.y,
                ));
            }
            let crops = [
                // Disjoint crops include the rim's half-pixel on BOTH sides.
                // Even logical coordinates map exactly at1x and1.5x.
                Rect::new(0., 52., 12., (h - 64.).max(0.)),
                Rect::new(w - 12., 52., 12., (h - 64.).max(0.)),
                Rect::new(0., h - 12., w, 12.),
                Rect::new(0., 48., w, 4.),
                Rect::new(12., 52., radius - 1., radius - 1.),
                Rect::new(w - 11. - radius, 52., radius - 1., radius - 1.),
                Rect::new(12., h - 11. - radius, radius - 1., radius - 1.),
                Rect::new(w - 11. - radius, h - 11. - radius, radius - 1., radius - 1.),
            ];
            crops
                .into_iter()
                .filter(|r| r.w > 0. && r.h > 0.)
                .map(|rect| {
                    let svg = self
                        .svg((w, h), &material, rect, engaged)
                        .map_err(|e| e.to_string())?;
                    Ok(bead::wayland::Decoration {
                        rect,
                        svg: Arc::from(svg.into_bytes()),
                    })
                })
                .collect()
        })
    }
    pub fn decorate(&self, scene: &mut Scene) -> Result<(), Box<dyn std::error::Error>> {
        use bead::render::Primitive;
        use std::hash::{Hash, Hasher};
        let body = body(scene);
        let output = (self.output.0 as f32, self.output.1 as f32);
        let frame = Frame::default();
        let top_inset = if matches!(
            self.attachment,
            Attachment::Popout | Attachment::ControlCenter | Attachment::Attached(Edge::Top)
        ) {
            2.
        } else {
            32.
        };
        let panel = frame.panel(self.attachment, output, (body.w, body.h), self.center);
        let rectangles = [
            Rect::new(0., 0., output.0, frame.top),
            Rect::new(0., 0., frame.left, output.1),
            Rect::new(output.0 - frame.right, 0., frame.right, output.1),
            Rect::new(0., output.1 - frame.bottom, output.0, frame.bottom),
            panel,
        ];
        let svg = self.svg(
            output,
            &rectangles,
            Rect::new(
                panel.x - 32.,
                panel.y - top_inset,
                body.w + 64.,
                body.h + 32. + top_inset,
            ),
            self.engaged,
        )?;
        let mut hash = std::collections::hash_map::DefaultHasher::new();
        svg.hash(&mut hash);
        let path =
            std::path::PathBuf::from(format!("@joined-frame-{top_inset}-{:x}.svg", hash.finish()));
        scene
            .asset_data
            .retain(|path, _| !path.to_string_lossy().starts_with("@joined-frame-"));
        scene
            .asset_data
            .insert(path.clone(), Arc::from(svg.into_bytes()));
        let mut replaced = false;
        for primitive in &mut scene.primitives {
            if !replaced && matches!(primitive, Primitive::Shadow { .. }) {
                *primitive = Primitive::Rounded {
                    rect: Rect::new(0., 0., 0., 0.),
                    radius: 0.,
                    fill: [0; 4],
                    border: None,
                };
                continue;
            }
            let is_root = match primitive {
                Primitive::Rounded { rect, .. } => {
                    rect.w == body.w && rect.h == body.h && rect.x == body.x && rect.y == body.y
                }
                Primitive::Image { path, .. } => {
                    path.to_string_lossy().starts_with("@joined-frame-")
                }
                _ => false,
            };
            if !replaced && is_root {
                *primitive = Primitive::Image {
                    blur: 0.,
                    rect: Rect::new(
                        body.x - 32.,
                        body.y - top_inset,
                        body.w + 64.,
                        body.h + 32. + top_inset,
                    ),
                    path: path.clone(),
                };
                replaced = true;
            }
        }
        if !replaced {
            return Err("Panel root background missing".into());
        }
        scene.prepared.clear();
        Ok(())
    }
}

impl Frame {
    pub fn panel(
        self,
        attachment: Attachment,
        output: (f32, f32),
        size: (f32, f32),
        center: Option<f32>,
    ) -> Rect {
        let viewport = (
            (output.0 - self.left - self.right).max(0.),
            (output.1 - self.top - self.bottom).max(0.),
        );
        let (x, y) = match attachment {
            Attachment::Attached(edge) => attached(edge, viewport, size, 0.),
            Attachment::ControlCenter => {
                let (x, _) = attached(Edge::Right, viewport, size, 0.);
                (x, 0.)
            }
            Attachment::Popout => (
                popout(
                    self.left,
                    viewport.0,
                    center.unwrap_or(output.0 / 2.),
                    size.0,
                    size.0,
                    self.rounding,
                )
                .0,
                0.,
            ),
        };
        Rect::new(self.left + x, self.top + y, size.0, size.1)
    }

    pub fn positioner(self, attachment: Attachment) -> PanelPositioner {
        Arc::new(move |scene, center, output| {
            let body = body(scene);
            let panel = self.panel(
                attachment,
                (output.0 as f32, output.1 as f32),
                (body.w, body.h),
                center,
            );
            // Surface pixels include the scene's shadow/contour inset. Align the
            // visible body, never that padded surface rectangle.
            (
                (panel.x - body.x + scene.bounds.x).round() as i32,
                (panel.y - body.y + scene.bounds.y).round() as i32,
            )
        })
    }
}

#[cfg(test)]
#[path = "placement_pixels_tests.rs"]
mod pixels;
#[cfg(test)]
mod tests {
    use super::*;
    fn colors() -> bead::render::Palette {
        bead::render::Palette::read(
            &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../shell/palette.json"),
        )
        .unwrap()
    }
    #[test]
    fn frame_tiles_stay_small_and_panel_redecoration_keeps_one_asset() {
        let colors = colors();
        let chrome = Chrome::new(Attachment::Popout, &colors).unwrap();
        let mut scene = crate::sound::fallback(colors.get("surface").unwrap());
        chrome.decorate(&mut scene).unwrap();
        let first = body(&scene);
        for _ in 0..20 {
            chrome.decorate(&mut scene).unwrap();
        }
        assert_eq!((body(&scene).w, body(&scene).h), (first.w, first.h));
        assert_eq!(
            scene.asset_data.len(),
            1,
            "No historical contour payloads retained"
        );
        let mut cache = bead::render::tree::TreeCache::default();
        cache.prepare(&scene, 1.5).unwrap();
        let builds = cache.builds;
        let position = Frame::default().positioner(Attachment::Popout);
        let origin = position(&scene, None, (1920, 1200));
        let factory = Chrome::new(Attachment::Popout, &colors)
            .unwrap()
            .decorations();
        let static_tiles = factory(
            (1920, 1200),
            true,
            Some(bead::wayland::DecorationPanel {
                scene: &scene,
                origin,
            }),
        )
        .unwrap();
        for offset in [1., 4., 8., 16.] {
            let mut moving = scene.clone();
            moving.offset.1 += offset;
            moving.opacity = 0.75;
            cache.prepare(&moving, 1.5).unwrap();
            assert_eq!(
                cache.builds, builds,
                "Transform/opacity must reuse the baked contour"
            );
            assert_eq!(
                position(&moving, None, (1920, 1200)),
                origin,
                "Animation must not move the layer's attachment"
            );
            let tiles = factory(
                (1920, 1200),
                true,
                Some(bead::wayland::DecorationPanel {
                    scene: &moving,
                    origin,
                }),
            )
            .unwrap();
            assert!(
                tiles.iter().zip(&static_tiles).all(|(a, b)| a.svg == b.svg),
                "Transform/opacity must not invalidate static frame geometry"
            );
        }
        let factory = Chrome::new(Attachment::Attached(Edge::Top), &colors)
            .unwrap()
            .decorations();
        let tiles = factory((1920, 1200), false, None).unwrap();
        assert_eq!(tiles.len(), 8);
        let bytes: usize = tiles
            .iter()
            .map(|t| (t.rect.w * 1.5).ceil() as usize * (t.rect.h * 1.5).ceil() as usize * 4)
            .sum();
        assert_eq!(bytes, 542592);
        assert!(
            bytes < 600 * 1024,
            "Never allocate an output-sized frame raster"
        );
        assert!(tiles.iter().all(|t| t.svg.len() < 16384));
        assert!(
            tiles
                .iter()
                .all(|t| String::from_utf8_lossy(&t.svg).contains("opacity=\"0.16\""))
        );
        let active = factory((1920, 1200), true, None).unwrap();
        assert!(
            active
                .iter()
                .all(|t| String::from_utf8_lossy(&t.svg).contains("opacity=\"0.25\""))
        );
    }
    #[test]
    fn nacre_attachment_and_corner_threshold_match_the_published_rules() {
        assert_eq!(
            attached(Edge::Top, (1900., 1140.), (1100., 590.), 0.),
            (400., 0.)
        );
        assert_eq!(
            attached(Edge::Right, (1900., 1140.), (400., 286.), 120.),
            (1380., 427.)
        );
        assert_eq!(
            attached(Edge::Bottom, (1900., 1140.), (1100., 590.), 0.),
            (400., 550.)
        );
        assert_eq!(
            attached(Edge::Left, (200., 100.), (400., 286.), 0.),
            (0., 0.)
        );
        assert_eq!(popout(10., 1900., 960., 310., 310., 25.), (795., false));
        assert_eq!(popout(10., 1900., 1729., 310., 310., 25.), (1564., false));
        // Already docked at the rounding threshold, before ordinary clamping.
        assert_eq!(popout(10., 1900., 1730., 310., 310., 25.), (1590., true));
        assert_eq!(popout(10., 1900., 0., 310., 310., 25.), (25., false));
    }
    #[test]
    fn visible_body_is_flush_even_with_a_thirty_two_pixel_surface_inset() {
        let scene = crate::sound::fallback([20, 20, 20, 255]);
        let position = Frame::default().positioner(Attachment::Attached(Edge::Right));
        assert_eq!(position(&scene, None, (1920, 1200)), (1478, 445));
        let mut scene = scene;
        scene.primitives = vec![bead::render::Primitive::Rounded {
            rect: Rect::new(0., 0., 310., 222.),
            radius: 16.,
            fill: [0; 4],
            border: None,
        }];
        let position = Frame::default().positioner(Attachment::Popout);
        assert_eq!(position(&scene, Some(1890.), (1920, 1200)), (1568, 18));
        // Body is y=18+32=50, touching the header rather than floating at102.
    }

    #[test]
    fn control_center_body_joins_top_frame_at_right_inset() {
        let frame = Frame::default();
        assert_eq!(
            frame.panel(
                Attachment::ControlCenter,
                (1920., 1200.),
                (400., 286.),
                None,
            ),
            Rect::new(1510., 50., 400., 286.)
        );
        let scene = crate::sound::fallback([20, 20, 20, 255]);
        let position = frame.positioner(Attachment::ControlCenter);
        assert_eq!(position(&scene, None, (1920, 1200)), (1478, 18));
    }
}
