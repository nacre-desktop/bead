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
                let mut value = serde_json::json!({"target_open":self.panel_target,"resident":self.panel_runtime.is_some(),"prepared":self.panel_ui_serial != 0 && self.companion.as_ref().is_some_and(|c| !c.0.prepared.is_empty()),"surfaces":self.surfaces.iter().filter(|s|s.companion).count()});
                if let Some(name) = &self.panel_name {
                    value["view"] = serde_json::Value::String(name.clone());
                }
                if self.options.trace_state {
                    value["cache_bytes"] =
                        serde_json::to_value(bead_core::cache::report()).unwrap_or_default();
                    value["prepare_regions"] = serde_json::json!(
                        self.runtime
                            .as_ref()
                            .map(|r| r
                                .input_regions()
                                .into_iter()
                                .filter(|r| r.prepare_panel)
                                .map(|r| [r.rect.x, r.rect.y, r.rect.w, r.rect.h])
                                .collect::<Vec<_>>())
                            .unwrap_or_default()
                    );
                    value["prepare_views"] = serde_json::json!(self.runtime.as_ref().map(|r| r.input_regions().into_iter().filter(|r| r.prepare_panel).map(|r| serde_json::json!({"id":r.id,"view":r.prepare_view,"center":r.prepare_center,"rect":[r.rect.x,r.rect.y,r.rect.w,r.rect.h]})).collect::<Vec<_>>()).unwrap_or_default());
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
            "prepare" | "open" | "toggle" => {
                let name = call.args.first().and_then(serde_json::Value::as_str);
                let output = call
                    .output
                    .as_deref()
                    .or_else(|| call.args.get(1).and_then(|v| v.as_str()));
                let center = call.args.get(1).and_then(|v| v.as_f64()).map(|v| v as f32);
                if self.options.panels.is_some() && !call.args.is_empty() && name.is_none() {
                    Err("panel view needs a string".into())
                } else if call.method == "prepare" {
                    self.request_panel_at(name, output, false, center)
                        .map(|()| serde_json::Value::Null)
                } else {
                    self.requested_panel_differs(name)
                        .and_then(|changed| {
                            panels::validate_center(center)?;
                            if call.method == "toggle" && self.panel_target && !changed {
                                self.validate_panel_output(output)?;
                                self.close_panel();
                                Ok(())
                            } else {
                                self.request_panel_at(name, output, true, center)
                            }
                        })
                        .map(|()| serde_json::Value::Null)
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
    pub(super) fn requested_panel_differs(&self, name: Option<&str>) -> Result<bool, String> {
        match &self.options.panels {
            Some(panels) => panels
                .resolve(name)
                .map(|(name, _)| self.panel_name.as_deref() != Some(name)),
            None => Ok(false),
        }
    }
    fn validate_panel_output(&self, name: Option<&str>) -> Result<(), String> {
        if self.output.outputs().any(|output| {
            name.map_or_else(
                || {
                    self.surfaces
                        .iter()
                        .any(|s| !s.companion && s.output == output)
                },
                |name| {
                    self.output
                        .info(&output)
                        .and_then(|info| info.name)
                        .as_deref()
                        == Some(name)
                },
            )
        }) {
            Ok(())
        } else {
            Err("panel output not available".into())
        }
    }
    // Resolve before retirement: an unknown view must leave the current one intact.
    fn select_panel(&mut self, name: Option<&str>) -> Result<(), String> {
        let Some(panels) = &self.options.panels else {
            return Ok(());
        };
        let (name, spec) = panels.resolve(name)?;
        if self.panel_name.as_deref() == Some(name) {
            return Ok(());
        }
        let name = name.to_owned();
        let mut spec = spec.clone();
        spec.initial_open = false;
        if self.panel_runtime.is_some() {
            self.release_panel();
        }
        self.options.panel_spec = Some(spec);
        self.panel_name = Some(name);
        Ok(())
    }
    pub(super) fn request_panel(
        &mut self,
        name: Option<&str>,
        output: Option<&str>,
        open: bool,
    ) -> Result<(), String> {
        self.request_panel_at(name, output, open, None)
    }
    pub(super) fn request_panel_at(
        &mut self,
        name: Option<&str>,
        output: Option<&str>,
        open: bool,
        center: Option<f32>,
    ) -> Result<(), String> {
        panels::validate_center(center)?;
        // IPC and keybindings use the same live trigger as pointer activation.
        // View names remain application-defined; no shell geometry lives here.
        let center = center.or_else(|| {
            let name = name?;
            self.runtime
                .as_ref()?
                .input_regions()
                .into_iter()
                .find(|region| region.prepare_view.as_deref() == Some(name))?
                .prepare_center
        });
        let changed = self.requested_panel_differs(name)?;
        // Preparation never replaces a visible view or restarts an already warm one.
        if !open && (self.panel_target || (self.panel_runtime.is_some() && !changed)) {
            return Ok(());
        }
        // A missing output must fail before retiring the old worker/surface.
        self.validate_panel_output(output)?;
        self.select_panel(name)?;
        self.panel_center = center;
        self.start_panel(output, open)
    }
    pub(super) fn open_panel(&mut self, output: Option<&str>) -> Result<(), String> {
        self.request_panel(None, output, true)
    }
    pub(super) fn prepare_panel(&mut self, output: Option<&str>) {
        self.prepare_named_panel(None, output, None);
    }
    pub(super) fn prepare_named_panel(
        &mut self,
        name: Option<&str>,
        output: Option<&str>,
        center: Option<f32>,
    ) {
        if self.options.panel_spec.is_none() {
            return;
        }
        if let Err(error) = self.request_panel_at(name, output, false, center) {
            eprintln!("PANEL_PREPARE_ERROR {error}");
        }
    }
    pub(super) fn position_panel_surface(
        &self,
        layer: &LayerSurface,
        output: &wl_output::WlOutput,
        scene: &Scene,
    ) {
        if let Some(position) = self
            .options
            .panel_spec
            .as_ref()
            .and_then(|s| s.position.as_ref())
            && let Some(size) = self.output.info(output).and_then(|i| i.logical_size)
        {
            let (x, y) = position(scene, self.panel_center, size);
            layer.set_anchor(Anchor::TOP | Anchor::LEFT);
            layer.set_margin(y, 0, 0, x);
            return;
        }
        let width = scene.bounds.w.ceil().max(1.) as u32;
        let height =
            (scene.bounds.y + scene.bounds.h + scene.slide.map_or(scene.offset.1, |s| s.open_y))
                .ceil()
                .max(1.) as u32;
        let (anchor, [top, right, bottom, left]) = panels::placement(
            scene.motion.as_ref().map_or(self.options.edge, |m| m.edge),
            self.panel_center,
            (width, height),
            self.output.info(output).and_then(|i| i.logical_size),
        );
        layer.set_anchor(anchor);
        layer.set_margin(top, right, bottom, left);
    }
    fn start_panel(&mut self, output: Option<&str>, open: bool) -> Result<(), String> {
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
        self.panel_target = open;
        if let Some(source) = &mut self.panel_runtime {
            source.panel_center(self.panel_center);
        }
        if open && let Some(source) = &mut self.runtime {
            source.set_idle(false);
        }
        if self.panel_runtime.is_some() {
            if self.panel_output.as_ref() != Some(&output)
                || (!spec.passive && self.dismiss.is_none())
            {
                drop(self.dismiss.take());
                self.surfaces.retain(|s| !s.companion);
                self.panel_output = Some(output.clone());
                if !spec.passive {
                    self.create_dismiss(&output)?;
                }
                let qh = self.qh.clone();
                self.add_output(&qh, output);
            }
            if let Some((scene, _, _)) = &self.companion {
                for surface in self.surfaces.iter().filter(|s| s.companion) {
                    self.position_panel_surface(&surface.layer, &surface.output, scene);
                    surface.layer.commit();
                }
            }
            self.sync_all_decorations()?;
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
        self.panel_reload_pending = false;
        self.panel_output = Some(output.clone());
        if open && !spec.passive {
            self.create_dismiss(&output)?;
        }
        self.companion_policy = Default::default();
        self.options.companion = Some((spec.fallback.clone(), spec.renderer));
        self.options.lazy_gl = true;
        self.companion = Some((
            spec.fallback.clone(),
            bead_render::renderer(RendererKind::Cpu).map_err(|e| e.to_string())?,
            RendererKind::Cpu,
        ));
        self.sync_all_decorations()?;
        let factory = spec.factory.clone();
        let mut source: Box<dyn SceneSource> = Box::new(worker::Source::new(
            Box::new(move || factory(open)),
            spec.fallback.clone(),
        ));
        let sender = self
            .event_sender
            .clone()
            .ok_or("UI event channel unavailable")?;
        let notify = sender.clone();
        source.services(Box::new(move |event| {
            let _ = notify.send(UiEvent::Panel(epoch, Box::new(event)));
        }));
        source.panel_center(self.panel_center);
        source
            .watch(Box::new(move |result| {
                let event = match result {
                    Ok(()) => UiEvent::Reload,
                    Err(e) => UiEvent::WatchError(e),
                };
                let _ = sender.send(UiEvent::Panel(epoch, Box::new(event)));
            }))
            .map_err(|e| e.to_string())?;
        // Match the eventual content-sized surface before any Wayland allocation.
        let height = spec.fallback.bounds.y
            + spec.fallback.bounds.h
            + spec
                .fallback
                .slide
                .map_or(spec.fallback.offset.1, |s| s.open_y);
        // Content size is an output of layout, not its available-space input.
        // Reusing the surface size here feeds percentage limits back into layout.
        let viewport = self
            .output
            .info(&output)
            .and_then(|i| i.logical_size)
            .filter(|(w, h)| *w > 0 && *h > 0)
            .map(|(w, h)| (w as u32, h as u32))
            .unwrap_or((
                spec.fallback.bounds.w.ceil() as u32,
                height.ceil().max(1.) as u32,
            ));
        source.viewport(viewport.0, viewport.1);
        self.panel_ui_serial = 0;
        self.panel_runtime = Some(source);
        let qh = self.qh.clone();
        if open {
            self.add_output(&qh, output);
        }
        self.panel_ui(UiEvent::Reload);
        if !open {
            self.loop_handle
                .insert_source(
                    Timer::from_duration(Duration::from_secs(2)),
                    move |_, _, app| {
                        if app.panel_epoch == epoch && !app.panel_target {
                            app.release_panel();
                        }
                        TimeoutAction::Drop
                    },
                )
                .map_err(|e| e.to_string())?;
            println!("PANEL_PREPARE epoch={epoch} timeout_ms=2000");
            return Ok(());
        }
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
        let force_draw = matches!(&event, UiEvent::Relayout | UiEvent::PanelVisibility(_));
        self.processing_panel = true;
        self.options.bar = false;
        self.options.panel = true;
        self.options.content_size = true;
        self.options.keyboard_focus = !self.options.panel_spec.as_ref().is_some_and(|s| s.passive);
        let changed = self.ui_inner(event);
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
        if !changed && !force_draw {
            return;
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
        if let Err(error) = self.sync_all_decorations() {
            self.fail(error);
        }
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
        self.panel_center = None;
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
        if let Some(source) = &mut self.runtime {
            source.set_idle(true);
        }
        // Functional closed-idle reclamation, explicitly requested for Gate C; not a diagnostic.
        #[cfg(all(target_os = "linux", target_env = "gnu"))]
        unsafe {
            libc::malloc_trim(0);
        }
        // Accepted worker packets already queued on the UI channel may still
        // own retired rasters. Let that queue drain before the final trim.
        #[cfg(all(target_os = "linux", target_env = "gnu"))]
        {
            let epoch = self.panel_epoch;
            if let Err(error) = self.loop_handle.insert_source(
                Timer::from_duration(Duration::from_millis(100)),
                move |_, _, app| {
                    if app.panel_epoch == epoch && app.panel_runtime.is_none() && !app.panel_target
                    {
                        unsafe {
                            libc::malloc_trim(0);
                        }
                        println!("PANEL_IDLE_TRIM scheduled_delay_ms=100");
                    }
                    TimeoutAction::Drop
                },
            ) {
                self.fail(error);
            }
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
        if self.options.test_passive {
            let empty = Region::new(&self.compositor).map_err(|e| e.to_string())?;
            layer.wl_surface().set_input_region(Some(empty.wl_region()));
        }
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
