// SPDX-License-Identifier: MIT OR Apache-2.0
//! Dynamic view ownership: only small durable state survives a closed/idle view.
use super::*;
impl App {
    pub(super) fn view_event(&mut self, panel: bool, event: UiEvent) {
        if panel && self.panel_runtime.is_some() {
            self.panel_ui(event);
        } else {
            self.ui(event);
        }
    }
    pub(super) fn panel_command(&mut self, request: std::sync::Arc<Remote>) -> bool {
        let call = &request.call;
        if call.target != "bead.panel" {
            return false;
        }
        let result = match call.method.as_str() {
            "state" => {
                let mut value = serde_json::json!({"target_open":self.panel_target,"resident":self.panel_runtime.is_some(),"surfaces":self.surfaces.iter().filter(|s|s.companion).count()});
                if self.options.trace_state {
                    value["regions"] = serde_json::json!(
                        self.panel_runtime
                            .as_ref()
                            .map(|r| r
                                .input_regions()
                                .iter()
                                .map(|r| [r.rect.x, r.rect.y, r.rect.w, r.rect.h])
                                .collect::<Vec<_>>())
                            .unwrap_or_default()
                    );
                    value["size"] = serde_json::json!(
                        self.surfaces
                            .iter()
                            .find(|s| s.companion)
                            .map(|s| [s.width, s.height])
                    );
                }
                Ok(value)
            }
            "open" | "toggle" => {
                if call.method == "toggle" && self.panel_target {
                    self.close_panel();
                    Ok(serde_json::Value::Null)
                } else {
                    self.open_panel(
                        call.output
                            .as_deref()
                            .or_else(|| call.args.get(1).and_then(|v| v.as_str())),
                    )
                    .map(|_| serde_json::Value::Null)
                }
            }
            "close" => {
                self.close_panel();
                Ok(serde_json::Value::Null)
            }
            _ => Err("unknown panel method".into()),
        };
        let reply = match result {
            Ok(value) => bead_services::ipc::Reply::ok(call.id, value),
            Err(e) => bead_services::ipc::Reply::error(call.id, "panel", e),
        };
        let _ = request.reply.send(reply);
        true
    }
    pub(super) fn open_panel(&mut self, output: Option<&str>) -> Result<(), String> {
        let spec = self
            .options
            .panel_spec
            .clone()
            .ok_or("dynamic panel not configured")?;
        let output = self
            .output
            .outputs()
            .find(|o| {
                output.map_or_else(
                    || self.surfaces.iter().any(|s| !s.companion && s.output == *o),
                    |name| self.output.info(o).and_then(|i| i.name).as_deref() == Some(name),
                )
            })
            .ok_or("panel output not available")?;
        self.panel_close_epoch = self.panel_close_epoch.wrapping_add(1);
        self.panel_target = true;
        if self.panel_runtime.is_some() {
            if self.panel_output.as_ref() != Some(&output) || self.dismiss.is_none() {
                drop(self.dismiss.take());
                self.surfaces.retain(|s| !s.companion);
                self.panel_output = Some(output.clone());
                self.create_dismiss(&output)?;
                let qh = self.qh.clone();
                self.add_output(&qh, output);
            }
            self.panel_ui(UiEvent::PanelVisibility(true));
            return Ok(());
        }
        if let Some(m) = &spec.fallback.motion {
            self.animations.target(
                &m.id,
                0.,
                self.started.elapsed().as_secs_f64(),
                m.duration,
                bead_core::animation::Curve::EaseOut,
            );
        }
        self.panel_epoch = self.panel_epoch.wrapping_add(1);
        let epoch = self.panel_epoch;
        self.panel_output = Some(output.clone());
        self.create_dismiss(&output)?;
        self.companion_policy = Default::default();
        self.options.companion = Some((spec.fallback.clone(), spec.renderer));
        self.options.lazy_gl = true;
        self.companion = Some((
            spec.fallback.clone(),
            bead_render::renderer(RendererKind::Cpu).map_err(|e| e.to_string())?,
            RendererKind::Cpu,
        ));
        let factory = spec.factory.clone();
        let mut source: Box<dyn SceneSource> = Box::new(worker::Source::new(
            Box::new(move || factory()),
            spec.fallback,
        ));
        let sender = self
            .event_sender
            .clone()
            .ok_or("UI event channel unavailable")?;
        let notify = sender.clone();
        source.services(Box::new(move |event| {
            let _ = notify.send(UiEvent::Panel(epoch, Box::new(event)));
        }));
        source
            .watch(Box::new(move |result| {
                let event = match result {
                    Ok(()) => UiEvent::Reload,
                    Err(e) => UiEvent::WatchError(e),
                };
                let _ = sender.send(UiEvent::Panel(epoch, Box::new(event)));
            }))
            .map_err(|e| e.to_string())?;
        self.panel_runtime = Some(source);
        let qh = self.qh.clone();
        self.add_output(&qh, output);
        self.panel_ui(UiEvent::Reload);
        let frame_epoch = self.panel_epoch;
        let kick = self.qh.clone();
        self.loop_handle
            .insert_source(
                Timer::from_duration(Duration::from_millis(16)),
                move |_, _, app| {
                    if app.panel_epoch == frame_epoch {
                        let indices: Vec<_> = app
                            .surfaces
                            .iter()
                            .enumerate()
                            .filter(|(_, s)| s.companion)
                            .map(|(i, _)| i)
                            .collect();
                        for i in indices {
                            app.surfaces[i].callbacks.invalidate();
                            app.draw(i, &kick);
                        }
                    }
                    TimeoutAction::Drop
                },
            )
            .map_err(|e| e.to_string())?;
        println!("PANEL_OPEN epoch={epoch}");
        Ok(())
    }
    pub(super) fn panel_ui(&mut self, event: UiEvent) {
        if self.panel_runtime.is_none() || self.companion.is_none() {
            return;
        }
        let (bar, panel, content, kind, keyboard) = (
            self.options.bar,
            self.options.panel,
            self.options.content_size,
            self.options.renderer,
            self.options.keyboard_focus,
        );
        std::mem::swap(&mut self.runtime, &mut self.panel_runtime);
        {
            let (scene, renderer, effective) = self.companion.as_mut().unwrap();
            std::mem::swap(&mut self.scene, scene);
            std::mem::swap(&mut self.renderer, renderer);
            self.options.renderer = *effective;
        }
        self.processing_panel = true;
        self.options.bar = false;
        self.options.panel = true;
        self.options.content_size = true;
        self.options.keyboard_focus = true;
        self.ui_inner(event);
        self.options.bar = bar;
        self.options.panel = panel;
        self.options.content_size = content;
        self.options.renderer = kind;
        self.options.keyboard_focus = keyboard;
        self.processing_panel = false;
        std::mem::swap(&mut self.runtime, &mut self.panel_runtime);
        {
            let (scene, renderer, _) = self.companion.as_mut().unwrap();
            std::mem::swap(&mut self.scene, scene);
            std::mem::swap(&mut self.renderer, renderer);
        }
        if self.panel_target
            && self
                .companion
                .as_ref()
                .and_then(|c| c.0.motion.as_ref())
                .is_some_and(|m| !m.open)
        {
            self.close_panel();
        }
        let qh = self.qh.clone();
        let indices: Vec<_> = self
            .surfaces
            .iter()
            .enumerate()
            .filter(|(_, s)| s.companion)
            .map(|(i, _)| i)
            .collect();
        for i in indices {
            self.draw(i, &qh);
        }
    }
    pub(super) fn close_panel(&mut self) {
        if self.panel_runtime.is_none() {
            return;
        }
        self.panel_target = false;
        drop(self.dismiss.take());
        self.panel_close_epoch = self.panel_close_epoch.wrapping_add(1);
        let token = self.panel_close_epoch;
        if let Some((scene, _, _)) = &mut self.companion
            && let Some(motion) = &mut scene.motion
        {
            motion.open = false;
        }
        self.panel_ui(UiEvent::PanelVisibility(false));
        let qh = self.qh.clone();
        let epoch = self.panel_epoch;
        if let Err(e) = self.loop_handle.insert_source(
            Timer::from_duration(Duration::from_millis(2200)),
            move |_, _, app| {
                if !app.panel_target && app.panel_epoch == epoch && app.panel_close_epoch == token {
                    app.release_panel();
                    for i in 0..app.surfaces.len() {
                        app.draw(i, &qh);
                    }
                }
                TimeoutAction::Drop
            },
        ) {
            self.fail(e);
        }
    }
    pub(super) fn release_panel(&mut self) {
        self.panel_target = false;
        self.panel_epoch = self.panel_epoch.wrapping_add(1);
        drop(self.dismiss.take());
        drop(self.panel_runtime.take());
        drop(self.companion.take());
        self.options.companion = None;
        if self.options.report.is_some() {
            self.report_surfaces.extend(
                self.surfaces
                    .iter()
                    .filter(|s| s.companion)
                    .map(report::surface),
            );
        }
        self.surfaces.retain(|s| !s.companion);
        self.panel_output = None;
        self.companion_policy = Default::default();
        self.release_timer = false;
        #[cfg(feature = "native-gl")]
        if self.surfaces.iter().all(|s| s.gl.is_none())
            && let Some(gl) = self.gl.take()
            && let Err(e) = unsafe { gl.shutdown_exclusive() }
        {
            self.fail(e);
        }
        #[cfg(feature = "wgpu")]
        if self.surfaces.iter().all(|s| s.gpu.is_none()) {
            self.gpu = None;
        }
        // Functional closed-idle reclamation, explicitly requested for Gate C; not a diagnostic.
        #[cfg(all(target_os = "linux", target_env = "gnu"))]
        unsafe {
            libc::malloc_trim(0);
        }
        println!("PANEL_IDLE_RELEASED workers=0 rasters=0 shm=0");
    }
}

pub(super) struct Dismiss {
    pub layer: LayerSurface,
    pub viewport: WpViewport,
    pub buffer: smithay_client_toolkit::shm::slot::Buffer,
    pub _pool: SlotPool,
}
impl Drop for Dismiss {
    fn drop(&mut self) {
        self.viewport.destroy();
    }
}
impl App {
    pub(super) fn create_dismiss(&mut self, output: &wl_output::WlOutput) -> Result<(), String> {
        let manager = self
            .viewporter
            .as_ref()
            .ok_or("dismissal requires viewporter")?;
        let surface = self.compositor.create_surface(&self.qh);
        let viewport = manager.get_viewport(&surface, &self.qh, ());
        let layer = self.layers.create_layer_surface(
            &self.qh,
            surface,
            Layer::Top,
            Some("bead-panel-dismiss"),
            Some(output),
        );
        layer.set_anchor(Anchor::TOP | Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT);
        layer.set_size(0, 0);
        layer.set_exclusive_zone(-1);
        layer.set_keyboard_interactivity(KeyboardInteractivity::None);
        let mut pool = SlotPool::new(4096, &self.shm).map_err(|e| e.to_string())?;
        let (buffer, bytes) = pool
            .create_buffer(1, 1, 4, wl_shm::Format::Argb8888)
            .map_err(|e| e.to_string())?;
        bytes.fill(0);
        layer.commit();
        self.dismiss = Some(Dismiss {
            layer,
            viewport,
            buffer,
            _pool: pool,
        });
        Ok(())
    }
}
