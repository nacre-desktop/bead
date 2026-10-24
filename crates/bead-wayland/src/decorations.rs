// SPDX-License-Identifier: MIT OR Apache-2.0
//! Static, bounded vector tiles; no timer, GL context or full-output bitmap.
use super::*;
struct Child {
    surface: wl_surface::WlSurface,
    role: wl_subsurface::WlSubsurface,
    viewport: WpViewport,
    _buffer: Buffer,
}
impl Drop for Child {
    fn drop(&mut self) {
        self.viewport.destroy();
        self.role.destroy();
        self.surface.destroy();
    }
}
#[derive(Clone, Copy, PartialEq, Eq)]
struct PanelKey {
    origin: (i32, i32),
    bounds: (u32, u32),
}
#[derive(Clone, Copy, PartialEq, Eq)]
struct Key {
    viewport: (u32, u32),
    scale120: u32,
    engaged: bool,
    panel: Option<PanelKey>,
    variant: u64,
}
pub(super) struct Tiles {
    key: Key,
    pub bytes: usize,
    _children: Vec<Child>,
    _pool: SlotPool,
}
impl App {
    pub(super) fn sync_all_decorations(&mut self) -> Result<(), String> {
        for index in 0..self.surfaces.len() {
            self.sync_decorations(index)?;
        }
        Ok(())
    }
    pub(super) fn sync_decorations(&mut self, index: usize) -> Result<(), String> {
        // A pearl is not part of the frame; the docked panel's tiles stay as they are.
        if self.pearl.is_some() {
            return Ok(());
        }
        let surface = &self.surfaces[index];
        if surface.companion || !surface.configured {
            return Ok(());
        }
        let Some(factory) = self.options.decorations.clone() else {
            return Ok(());
        };
        let Some((width, height)) = self
            .output
            .info(&surface.output)
            .and_then(|o| o.logical_size)
            .filter(|(w, h)| *w > 0 && *h > 0)
        else {
            return Ok(());
        };
        let on_output = self.panel_output.as_ref() == Some(&surface.output);
        let engaged = self.panel_target
            && on_output
            && self
                .options
                .panel_spec
                .as_ref()
                .is_some_and(|s| s.engages_decorations);
        let panel = if self.panel_target && on_output {
            self.companion
                .as_ref()
                .zip(self.options.panel_spec.as_ref())
                .and_then(|((scene, _, _), spec)| {
                    spec.position.as_ref().map(|position| DecorationPanel {
                        scene,
                        origin: position(scene, self.panel_center, (width, height)),
                    })
                })
        } else {
            None
        };
        let shape = panel.map(|p| PanelKey {
            origin: p.origin,
            bounds: (p.scene.bounds.w.to_bits(), p.scene.bounds.h.to_bits()),
        });
        let key = Key {
            viewport: (width as u32, height as u32),
            scale120: surface.scale120,
            engaged,
            panel: shape,
            variant: self.decoration_variant,
        };
        if surface.decorations.as_ref().is_some_and(|d| d.key == key) {
            return Ok(());
        }
        let subcompositor = self
            .subcompositor
            .as_ref()
            .ok_or("Static decorations need wl_subcompositor")?;
        let viewporter = self
            .viewporter
            .as_ref()
            .ok_or("Static decorations need wp_viewporter")?;
        let specs = factory((width as u32, height as u32), engaged, panel)?;
        if specs.len() > 16 {
            return Err("Too many static decorations".into());
        }
        let scale = key.scale120 as f32 / 120.;
        let mut sizes = Vec::new();
        let mut total = 0usize;
        for spec in &specs {
            if ![spec.rect.x, spec.rect.y, spec.rect.w, spec.rect.h]
                .iter()
                .all(|n| n.is_finite())
                || spec.rect.w <= 0.
                || spec.rect.h <= 0.
                || spec.svg.len() > 1_048_576
            {
                return Err("Invalid static decoration".into());
            }
            let w = (spec.rect.w * scale).ceil() as u32;
            let h = (spec.rect.h * scale).ceil() as u32;
            let bytes = (w as usize)
                .checked_mul(h as usize)
                .and_then(|n| n.checked_mul(4))
                .ok_or("Static decoration overflow")?;
            // SlotPool rounds each slot to64 bytes. Reserve that amount now;
            // a tiny final-slot shortfall otherwise doubles the entire pool.
            let reserved = bytes
                .checked_add(63)
                .map(|n| n & !63)
                .ok_or("Static decoration overflow")?;
            total = total
                .checked_add(reserved)
                .ok_or("Static decoration overflow")?;
            if total > 2 * 1024 * 1024 {
                return Err("Static decoration SHM budget exceeded".into());
            }
            sizes.push((w, h));
        }
        let mut pool = SlotPool::new(total.max(4096), &self.shm).map_err(|e| e.to_string())?;
        let empty = Region::new(&self.compositor).map_err(|e| e.to_string())?;
        let parent = surface.wl_surface();
        let mut children = Vec::new();
        for (spec, (w, h)) in specs.into_iter().zip(sizes) {
            let child = self.compositor.create_surface(&self.qh);
            let role = subcompositor.get_subsurface(&child, parent, &self.qh, ());
            let viewport = viewporter.get_viewport(&child, &self.qh, ());
            let (buffer, bytes) = pool
                .create_buffer(w as i32, h as i32, (w * 4) as i32, wl_shm::Format::Argb8888)
                .map_err(|e| e.to_string())?;
            bead_render::svg_into(
                &spec.svg,
                ShmTarget {
                    bytes,
                    width: w,
                    height: h,
                    scale,
                    format: PixelFormat::Bgra,
                },
            )
            .map_err(|e| e.to_string())?;
            role.set_position(spec.rect.x.round() as i32, spec.rect.y.round() as i32);
            role.set_desync();
            // The half-stroke inside the bar must remain visible too. Tiles
            // never cover controls and have an empty input region.
            role.place_above(parent);
            viewport.set_destination(spec.rect.w.round() as i32, spec.rect.h.round() as i32);
            child.set_input_region(Some(empty.wl_region()));
            buffer.attach_to(&child).map_err(|e| e.to_string())?;
            child.damage_buffer(0, 0, w as i32, h as i32);
            child.commit();
            children.push(Child {
                surface: child,
                role,
                viewport,
                _buffer: buffer,
            });
        }
        self.surfaces[index].decorations = Some(Tiles {
            key,
            bytes: total,
            _children: children,
            _pool: pool,
        });
        self.surfaces[index].commit();
        Ok(())
    }
}
delegate_noop!(App: ignore wl_subcompositor::WlSubcompositor);
delegate_noop!(App: ignore wl_subsurface::WlSubsurface);
