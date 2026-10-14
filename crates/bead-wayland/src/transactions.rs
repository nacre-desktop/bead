// SPDX-License-Identifier: MIT OR Apache-2.0
//! Validated worker candidates and IME synchronization.
use super::*;
impl App {
    pub(super) fn ui(&mut self, event: UiEvent) {
        if let UiEvent::Fatal(error) = event {
            self.fail(error);
            return;
        }
        let Some(mut runtime) = self.runtime.take() else {
            return;
        };
        let mut scales: Vec<_> = self
            .surfaces
            .iter()
            .filter(|s| s.configured && !s.companion)
            .map(|s| s.scale120 as f32 / 120.)
            .collect();
        scales.sort_by(f32::total_cmp);
        scales.dedup();
        if scales.is_empty() {
            scales.push(1.);
        }
        runtime.scales(scales);
        let reload = matches!(event, UiEvent::Reload)
            || matches!(&event,UiEvent::Worker(packet) if packet.is_reload());
        let started = Instant::now();
        if self.options.trace_state
            && let UiEvent::Backend(snapshot) = &event
        {
            println!(
                "UI_EVENT monotonic_ns={} title={}",
                monotonic_ns(),
                snapshot.focused_window().map_or("", |w| w.title.as_str())
            );
        }
        let staged = runtime.stage(event).and_then(|scene| {
            if let Some(scene) = &scene {
                let indices: Vec<_> = self
                    .surfaces
                    .iter()
                    .enumerate()
                    .filter(|(_, s)| s.configured && !s.companion)
                    .map(|(i, _)| i)
                    .collect();
                for i in indices {
                    #[cfg(any(feature = "native-gl", feature = "wgpu"))]
                    if self.renderer.wants_gpu()
                        && let Err(error) = self.ensure_gpu(i)
                    {
                        if !self.options.auto_fallback {
                            return Err(error);
                        }
                        eprintln!("FALLBACK GPU unavailable during staging: {error}");
                        self.renderer = bead_render::renderer(RendererKind::Cpu)?;
                        for surface in &mut self.surfaces {
                            #[cfg(feature = "native-gl")]
                            drop(surface.gl.take());
                            #[cfg(feature = "wgpu")]
                            drop(surface.gpu.take());
                        }
                        #[cfg(feature = "native-gl")]
                        {
                            self.gl = None;
                        }
                        #[cfg(feature = "wgpu")]
                        {
                            self.gpu = None;
                        }
                        self.options.renderer = RendererKind::Cpu;
                    }
                    let surface = &self.surfaces[i];
                    self.renderer
                        .prepare(scene, surface.scale120 as f32 / 120.)?;
                    #[cfg(feature = "native-gl")]
                    if let Some(gl) = &surface.gl {
                        let transport = self.gl.as_ref().ok_or("missing shared GL context")?;
                        let (w, h, _, _) =
                            buffer_dimensions(surface.width, surface.height, surface.scale120)?;
                        transport.activate(gl, w as u32, h as u32)?;
                        self.renderer
                            .stage_resources(bead_render::ResourceTarget::Gl {
                                display: &transport.display,
                            })?;
                    }
                    #[cfg(feature = "wgpu")]
                    if let Some(gpu) = &surface.gpu {
                        let device = &self
                            .gpu
                            .as_ref()
                            .ok_or("missing shared GPU device")?
                            .context
                            .devices[gpu.dev_id];
                        self.renderer
                            .stage_resources(bead_render::ResourceTarget::Wgpu {
                                device: &device.device,
                                queue: &device.queue,
                            })?;
                    }
                }
            }
            runtime.validate()?;
            Ok(scene)
        });
        match staged {
            Ok(Some(scene)) => {
                if !runtime.commit() {
                    self.runtime = Some(runtime);
                    return;
                }
                if let (Some(bridge), Some(tree)) =
                    (&mut self.accessibility, runtime.accessibility_tree())
                {
                    bridge.update(tree);
                }
                if self.options.trace_state {
                    println!("UI_STATE {}", runtime.state_summary());
                }
                self.ui_serial += 1;
                let reopening = self.scene.motion.as_ref().is_some_and(|m| !m.open)
                    && scene.motion.as_ref().is_some_and(|m| m.open);
                if reopening {
                    for surface in self.surfaces.iter_mut().filter(|s| !s.companion) {
                        surface.callbacks.invalidate();
                    }
                }
                self.scene = scene;
                if self.options.panel
                    && let Some(motion) = &self.scene.motion
                {
                    let anchor = match motion.edge {
                        bead_core::Edge::Left => Anchor::LEFT | Anchor::TOP,
                        bead_core::Edge::Right => Anchor::RIGHT | Anchor::TOP,
                        bead_core::Edge::Top => Anchor::TOP,
                        bead_core::Edge::Bottom => Anchor::BOTTOM,
                    };
                    for surface in self.surfaces.iter().filter(|s| !s.companion) {
                        surface.layer.set_anchor(anchor);
                        surface.layer.commit();
                    }
                }
                if self.options.content_size {
                    let width = self.scene.bounds.w.ceil().max(1.) as u32;
                    let height = (self.scene.bounds.y
                        + self.scene.bounds.h
                        + self.scene.slide.map_or(self.scene.offset.1, |s| s.open_y))
                    .ceil()
                    .max(1.) as u32;
                    for surface in self.surfaces.iter().filter(|s| !s.companion) {
                        if surface.width != width || surface.height != height {
                            surface.layer.set_size(width, height);
                            surface.layer.commit();
                        }
                    }
                }
                println!(
                    "UI_PLATFORM a11y_adapters={} ime_objects={} ui_channels={}",
                    usize::from(self.accessibility.is_some()),
                    usize::from(self.text_input.is_some()),
                    if self.accessibility.is_some() { 2 } else { 1 }
                );
                let (layers, images) = self.renderer.resource_counts();
                println!(
                    "UI_RESOURCES {} layers={} gpu_images={} surfaces={}",
                    runtime.resource_counts(),
                    layers,
                    images,
                    self.surfaces.len()
                );
                println!(
                    "UI_COMMIT generation={} reload={} prepare_ms={:.3} monotonic_ns={} update={}",
                    self.scene.resource_revision,
                    reload,
                    started.elapsed().as_secs_f64() * 1000.,
                    monotonic_ns(),
                    self.ui_serial
                );
            }
            Ok(None) => {
                self.runtime = Some(runtime);
                return;
            }
            Err(error) => {
                eprintln!("UI_ERROR {error}");
                self.scene = runtime.error_scene(error.to_string());
            }
        }
        self.runtime = Some(runtime);
        self.sync_ime();
        let qh = self.qh.clone();
        for i in 0..self.surfaces.len() {
            self.draw(i, &qh);
        }
    }

    pub(super) fn sync_ime(&mut self) {
        let Some(input) = &self.text_input else {
            return;
        };
        if self.ime_surface.is_none() {
            return;
        }
        let current = self.runtime.as_ref().and_then(|r| r.surrounding_text());
        if current == self.ime_last {
            return;
        }
        if let Some((text, cursor, anchor)) = &current {
            input.enable();
            input.set_content_type(
                zwp_text_input_v3::ContentHint::None,
                zwp_text_input_v3::ContentPurpose::Normal,
            );
            let mut start = (*cursor as usize).saturating_sub(2000);
            while !text.is_char_boundary(start) {
                start += 1;
            }
            let mut end = (start + 4000).min(text.len());
            while !text.is_char_boundary(end) {
                end -= 1;
            }
            let cursor = (*cursor as usize).clamp(start, end) - start;
            let mut anchor = (*anchor as usize).clamp(start, end);
            while !text.is_char_boundary(anchor) {
                anchor -= 1;
            }
            input.set_surrounding_text(
                text[start..end].to_owned(),
                cursor as i32,
                (anchor - start) as i32,
            );
            input.set_cursor_rectangle(32, 32, 1, 24);
            input.commit();
        } else {
            input.disable();
            input.commit();
        }
        self.ime_last = current;
    }
}
