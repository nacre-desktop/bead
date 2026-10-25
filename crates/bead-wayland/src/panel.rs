// SPDX-License-Identifier: MIT OR Apache-2.0
//! Dynamic view ownership: only small durable state survives a closed/idle view.
use super::*;
/// A closed panel stays built for this long, so reopening it shows prepared content on its first frame.
const PANEL_KEEP_ALIVE: Duration = Duration::from_secs(120);
impl App {
    pub(super) fn notify_panel_owner(&self, event: UiEvent) {
        let Some(sender) = &self.event_sender else {
            return;
        };
        if let Some(epoch) = self.panel_canvas_owner {
            let notification = match &event {
                UiEvent::PanelSelected(view) => Some(UiEvent::PanelSelected(view.clone())),
                UiEvent::PanelDismissed => Some(UiEvent::PanelDismissed),
                UiEvent::PointerRegion(inside) => Some(UiEvent::PointerRegion(*inside)),
                _ => None,
            };
            if let Some(notification) = notification {
                let _ = sender.send(UiEvent::Canvas(epoch, Box::new(notification)));
            }
        }
        let event = if self.options.primary_factory.is_some() {
            let Some(output) = self
                .panel_output
                .as_ref()
                .map(|o| self.output_id(o))
                .or_else(|| self.active_output.clone())
            else {
                return;
            };
            let epoch = if self.active_output.as_ref() == Some(&output) {
                self.active_output_epoch
            } else {
                self.output_sessions.get(&output).map(|s| s.epoch)
            };
            let Some(epoch) = epoch else { return };
            UiEvent::Output(epoch, output, Box::new(event))
        } else {
            event
        };
        let _ = sender.send(event);
    }

    pub(super) fn primary_idle(&mut self, output: &wl_output::WlOutput, idle: bool) {
        let id = self.output_id(output);
        let source =
            if self.options.primary_factory.is_none() || self.active_output.as_ref() == Some(&id) {
                self.runtime.as_mut()
            } else {
                self.output_sessions
                    .get_mut(&id)
                    .and_then(|s| s.source.as_mut())
            };
        if let Some(source) = source {
            source.set_idle(idle);
        }
    }
    pub(super) fn view_event(&mut self, panel: bool, event: UiEvent) {
        if panel && self.panel_runtime.is_some() {
            self.panel_ui(event);
        } else if !panel && self.options.primary_factory.is_some() {
            let output = self.active_input_output.clone().or_else(|| {
                self.keyboard_surface.as_ref().and_then(|surface| {
                    self.surfaces
                        .iter()
                        .find(|s| s.wl_surface() == surface)
                        .map(|s| s.output_name.clone())
                })
            });
            if let Some(output) = output
                && let Some(session) = self.output_sessions.get(&output)
            {
                self.ui(UiEvent::Output(session.epoch, output, Box::new(event)));
            }
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
                let mut value = serde_json::json!({"size":self.surfaces.iter().find(|s|s.companion && s.configured && s.width > 0 && s.height > 0).map(|s|[s.width,s.height]),"target_open":self.panel_target,"resident":self.panel_runtime.is_some(),"prepared":self.panel_ui_serial != 0 && self.companion.as_ref().is_some_and(|c| !c.0.prepared.is_empty()),"surfaces":self.surfaces.iter().filter(|s|s.companion).count(),"keyboard_focus":self.options.panel_spec.as_ref().is_some_and(|spec| spec.keyboard_focus),"dismiss_active":self.dismiss.as_ref().is_some_and(|d| d.enabled),"hidden":self.hidden});
                if std::env::var_os("BEAD_PRIVATE_INPUT_PROOF").is_some() {
                    value["stores"] = self
                        .panel_runtime
                        .as_ref()
                        .and_then(|r| {
                            serde_json::from_str::<serde_json::Value>(&r.state_summary()).ok()
                        })
                        .unwrap_or_default();
                }
                if let Some(name) = &self.panel_name {
                    value["view"] = serde_json::Value::String(name.clone());
                }
                if self.options.trace_state {
                    value["cache_bytes"] =
                        serde_json::to_value(bead_core::cache::report()).unwrap_or_default();
                    #[cfg(all(target_os = "linux", target_env = "gnu"))]
                    {
                        // SAFETY: read-only, by-value glibc allocator accounting.
                        let heap = unsafe { libc::mallinfo2() };
                        value["heap_bytes"] = serde_json::json!({
                            "live": heap.uordblks, "free": heap.fordblks,
                            "mapped": heap.hblkhd, "arena": heap.arena
                        });
                    }
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
                            .find(|s| s.companion && s.configured && s.width > 0 && s.height > 0)
                            .map(|s| [s.width, s.height])
                    );
                    if let Some(output) = self.panel_output.as_ref().or_else(|| {
                        self.surfaces
                            .iter()
                            .find(|surface| !surface.companion)
                            .map(|surface| &surface.output)
                    }) && let Some(size) =
                        self.output.info(output).and_then(|info| info.logical_size)
                    {
                        value["output_size"] = serde_json::json!([size.0, size.1]);
                        value["output"] = serde_json::Value::String(
                            self.output
                                .info(output)
                                .and_then(|info| info.name)
                                .unwrap_or_else(|| format!("output-{}", output.id().protocol_id())),
                        );
                        if let (Some((scene, _, _)), Some(positioner)) = (
                            self.companion.as_ref(),
                            self.options
                                .panel_spec
                                .as_ref()
                                .and_then(|spec| spec.position.as_ref()),
                        ) {
                            let (x, y) = positioner(scene, self.panel_center, size);
                            value["origin"] = serde_json::json!([x, y]);
                        }
                    }
                    if let Some((scene, _, _)) = &self.companion
                        && let Some(motion) = &scene.motion
                    {
                        value["edge"] =
                            serde_json::json!(format!("{:?}", motion.edge).to_lowercase());
                    }
                    if let Some((scene, _, _)) = &self.companion {
                        value["scene_bounds"] = serde_json::json!([
                            scene.bounds.x,
                            scene.bounds.y,
                            scene.bounds.w,
                            scene.bounds.h
                        ]);
                        if let Some(rect) = scene
                            .layers
                            .iter()
                            .find(|layer| layer.parent.is_none())
                            .map(|layer| {
                                [
                                    layer.bounds.x,
                                    layer.bounds.y,
                                    layer.bounds.w,
                                    layer.bounds.h,
                                ]
                            })
                        {
                            value["content_frame"] = serde_json::json!(rect);
                        }
                    }
                }
                Ok(value)
            }
            "hide" => {
                self.set_hidden(!self.hidden);
                Ok(serde_json::Value::Null)
            }
            "prepare" | "open" | "hover-open" | "toggle" => {
                if call.method != "prepare" {
                    self.set_hidden(false);
                }
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
                                self.request_panel_at(name, output, true, center)?;
                                self.panel_ui(UiEvent::PanelHover {
                                    opened: Some(call.method == "hover-open"),
                                    anchor: call.method == "hover-open",
                                });
                                Ok(())
                            }
                        })
                        .map(|()| serde_json::Value::Null)
                }
            }
            "hover-enter" | "hover-leave" => {
                self.panel_ui(UiEvent::PanelHover {
                    opened: None,
                    anchor: call.method == "hover-enter",
                });
                Ok(serde_json::Value::Null)
            }
            "close" => {
                self.close_panel();
                Ok(serde_json::Value::Null)
            }
            // B0048: detach the open docked panel; list, open, dock or close pearls.
            "detach" => self
                .resident_body_rect()
                .ok_or_else(|| "no open docked panel".to_string())
                .and_then(|rect| self.detach_resident(rect))
                .map(|id| serde_json::json!(id)),
            "pearls" => Ok(self.pearl_listing()),
            "pearls-reset" => {
                self.reset_pearls();
                Ok(serde_json::Value::Null)
            }
            "pearl-open" => {
                let view = call.args.first().and_then(serde_json::Value::as_str);
                let output = call.output.as_deref();
                view.ok_or_else(|| "pearl view needs a string".to_string())
                    .and_then(|view| self.open_pearl(view, output, None))
                    .map(|id| serde_json::json!(id))
            }
            "dock" | "pearl-close" => {
                let id = call
                    .args
                    .first()
                    .and_then(serde_json::Value::as_u64)
                    .map(|id| id as u32)
                    .or_else(|| self.last_pearl());
                match (id, call.method.as_str()) {
                    (Some(id), "dock") => self
                        .dock_pearl(id, call.output.as_deref())
                        .map(|()| serde_json::Value::Null),
                    (Some(id), _) => self.close_pearl(id).map(|()| serde_json::Value::Null),
                    (None, _) => Err("no pearl".into()),
                }
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
                    self.options.app.is_some()
                        || self
                            .surfaces
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
        self.notify_panel_owner(UiEvent::PanelSelected(name.to_owned()));
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

    pub(super) fn panel_navigated(&mut self, name: String) {
        let Some(panels) = &self.options.panels else {
            return;
        };
        let Ok((resolved, spec)) = panels.resolve(Some(&name)) else {
            eprintln!("PANEL_NAVIGATION_ERROR view={name}");
            return;
        };
        let resolved = resolved.to_owned();
        let mut spec = spec.clone();
        spec.initial_open = false;
        self.panel_name = Some(resolved.clone());
        self.options.panel_spec = Some(spec);
        self.notify_panel_owner(UiEvent::PanelNavigated(resolved));
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
        self.request_panel_anchored(name, output, open, center, None)
    }
    pub(super) fn request_panel_anchored(
        &mut self,
        name: Option<&str>,
        output: Option<&str>,
        open: bool,
        center: Option<f32>,
        anchor: Option<PanelAnchor>,
    ) -> Result<(), String> {
        if anchor.is_some_and(|anchor| {
            ![anchor.rect.x, anchor.rect.y, anchor.rect.w, anchor.rect.h]
                .iter()
                .all(|n| n.is_finite() && n.abs() <= 1_000_000.)
                || anchor.rect.w <= 0.
                || anchor.rect.h <= 0.
        }) {
            return Err("invalid panel anchor".into());
        }
        panels::validate_center(center)?;
        if open
            && let Some((name, control)) = name.zip(self.options.control.as_ref())
            && !control.panel_allowed(name)
        {
            return Err(format!("panel {name} is disabled"));
        }
        if open {
            self.panel_ambient = false;
        }
        // IPC and keybindings use the same live trigger as pointer activation.
        // View names remain application-defined; no shell geometry lives here.
        let center = center.or_else(|| {
            let name = name?;
            let source = output
                .and_then(|id| {
                    self.output_sessions
                        .get(id)
                        .and_then(|session| session.source.as_ref())
                })
                .or(self.runtime.as_ref())?;
            source
                .input_regions()
                .into_iter()
                .find(|region| region.prepare_view.as_deref() == Some(name))?
                .prepare_center
        });
        let changed = self.requested_panel_differs(name)?;
        // Preparation never replaces a visible view or restarts an already warm one.
        if !open
            && !panels::preparation_allowed(
                self.panel_target,
                self.surfaces.iter().any(|surface| surface.companion),
                self.panel_runtime.is_some(),
                changed,
            )
        {
            return Ok(());
        }
        // A missing output must fail before retiring the old worker/surface.
        self.validate_panel_output(output)?;
        self.select_panel(name)?;
        self.panel_center = center;
        self.panel_anchor = anchor;
        if let Some(panels) = &self.options.panels
            && let Ok((_, original)) = panels.resolve(name)
            && let Some(spec) = &mut self.options.panel_spec
        {
            spec.engages_decorations = anchor.is_none() && original.engages_decorations;
        }
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
    fn notify_panel_origin(&self, scene: &Scene, x: i32, y: i32) {
        if self
            .panel_runtime
            .as_ref()
            .is_some_and(|source| source.tracks_origin())
            && let Some(sender) = &self.event_sender
        {
            let _ = sender.send(UiEvent::CompanionModel(std::collections::BTreeMap::from([
                (
                    "bead.surface.x".into(),
                    bead_core::Value::Number(x as f64 + scene.offset.0 as f64),
                ),
                (
                    "bead.surface.y".into(),
                    bead_core::Value::Number(y as f64 + scene.offset.1 as f64),
                ),
            ])));
        }
    }
    pub(super) fn position_panel_surface(
        &self,
        layer: &LayerSurface,
        output: &wl_output::WlOutput,
        scene: &Scene,
    ) {
        if let Some(info) = self.pearl.filter(|i| i.kind == pearls::Kind::Pearl) {
            let (x, y) = self.pearl_margins(output, scene, info.rect);
            layer.set_anchor(Anchor::TOP | Anchor::LEFT);
            layer.set_margin(y, 0, 0, x);
            self.notify_panel_origin(scene, x, y);
            return;
        }
        if let Some(anchor) = self.panel_anchor
            && let Some(size) = self.output.info(output).and_then(|info| info.logical_size)
        {
            let (x, y) = panels::anchored_position(scene, anchor, size);
            layer.set_anchor(Anchor::TOP | Anchor::LEFT);
            layer.set_margin(y, 0, 0, x);
            self.notify_panel_origin(scene, x, y);
            return;
        }
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
            self.notify_panel_origin(scene, x, y);
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
    pub(super) fn start_panel(&mut self, output: Option<&str>, open: bool) -> Result<(), String> {
        bead_core::profile::mark("panel_admission");
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
                    || {
                        self.options.app.is_some()
                            || self.surfaces.iter().any(|s| !s.companion && s.output == *o)
                    },
                    |name| self.output.info(o).and_then(|i| i.name).as_deref() == Some(name),
                )
            })
            .ok_or("panel output not available")?;
        self.panel_close_epoch = self.panel_close_epoch.wrapping_add(1);
        self.panel_target = open;
        if let Some(m) = spec.fallback.motion.as_ref().filter(|m| m.reveal) {
            let now = self.started.elapsed().as_secs_f64();
            let curve = m.curve.unwrap_or(bead_core::animation::Curve::EaseOut);
            if self.panel_animations.value(&m.id, now).is_none() {
                self.panel_animations
                    .target(&m.id, 0., now, m.duration, curve);
            }
            // Visibility starts the reveal clock, including prepared/reopened views.
            // Worker preparation and configure advance it; draw() keeps the target.
            self.panel_animations
                .target(&m.id, if open { 1. } else { 0. }, now, m.duration, curve);
            bead_core::profile::mark("reveal_admitted");
        }
        self.edges_panel_changed(false);
        if let Some(source) = &mut self.panel_runtime {
            source.panel_center(self.panel_center);
            source.panel_anchor(self.panel_anchor);
        }
        if open {
            self.primary_idle(&output, false);
        }
        if self.panel_runtime.is_some() {
            if self.panel_output.as_ref() != Some(&output)
                || ((spec.keyboard_focus && self.pearl.is_none()) != self.dismiss.is_some())
                || (open && !self.surfaces.iter().any(|surface| surface.companion))
            {
                drop(self.dismiss.take());
                self.surfaces.retain(|s| !s.companion);
                self.panel_output = Some(output.clone());
                if spec.keyboard_focus && self.pearl.is_none() {
                    self.create_dismiss(&output)?;
                }
                let qh = self.qh.clone();
                self.add_output(&qh, output);
            }
            if let Some((scene, _, _)) = &self.companion {
                for surface in self.surfaces.iter().filter(|s| s.companion) {
                    self.position_panel_surface(&surface.layer, &surface.output, scene);
                    surface.commit();
                }
            }
            self.sync_all_decorations()?;
            if spec.retain_when_closed && open && self.panel_ready {
                self.set_dismiss_enabled(
                    self.panel_runtime
                        .as_ref()
                        .is_some_and(|source| source.outside_dismiss()),
                )?;
                for surface in self.surfaces.iter_mut().filter(|surface| surface.companion) {
                    surface.callbacks.invalidate();
                }
            }
            // PanelVisibility forces a draw of the last prepared scene even if
            // its worker is still refreshing service data in the background.
            self.panel_ui(UiEvent::PanelVisibility(true));
            return Ok(());
        }
        if let Some(m) = &spec.fallback.motion
            && !m.reveal
        {
            self.animations.target(
                &m.id,
                0.,
                self.started.elapsed().as_secs_f64(),
                m.duration,
                bead_core::animation::Curve::EaseOut,
            );
        }
        self.panel_epoch = self.next_epoch();
        let epoch = self.panel_epoch;
        self.panel_reload_pending = false;
        self.panel_ready = false;
        self.panel_output = Some(output.clone());
        if open && spec.keyboard_focus && self.pearl.is_none() {
            self.create_dismiss(&output)?;
        }
        self.companion_policy = Default::default();
        self.options.companion = Some((spec.fallback.clone(), spec.renderer));
        let gpu_first = cfg!(feature = "native-gl")
            && spec.retain_when_closed
            && spec.renderer == RendererKind::FemtovgGl;
        self.options.lazy_gl = !gpu_first;
        let renderer_kind = if gpu_first {
            spec.renderer
        } else {
            RendererKind::Cpu
        };
        self.companion = Some((
            spec.fallback.clone(),
            bead_render::renderer(renderer_kind).map_err(|e| e.to_string())?,
            renderer_kind,
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
        source.panel_anchor(self.panel_anchor);
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
        let palette = if let Some(waiting) = &mut self.wallpaper_palette_outputs {
            // A panel opened/replaced during preparation joins the current fence.
            waiting.join_panel(epoch);
            Some(waiting.palette.clone())
        } else {
            self.wallpaper_palette.clone()
        };
        if let Some(palette) = palette {
            self.panel_ui(UiEvent::WallpaperPalette(palette.clone()));
            if !self
                .panel_runtime
                .as_ref()
                .is_some_and(|source| source.wallpaper_palette_pending())
            {
                self.wallpaper_panel_palette_settled(epoch, &palette, true);
            }
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
        self.options.keyboard_focus = self
            .options
            .panel_spec
            .as_ref()
            .is_some_and(|s| s.keyboard_focus);
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
        if changed && self.sync_native_pearl_scene() {
            self.place_native_pearl(true);
            self.schedule_pearl_save();
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
    /// The first real scene of a panel has arrived: its reveal starts now, not while a blank fallback was all there was.
    pub(super) fn panel_content_ready(&mut self) {
        self.panel_ready = true;
        self.panel_animations = Default::default();
        let reveal = self
            .options
            .panel_spec
            .as_ref()
            .and_then(|spec| spec.fallback.motion.as_ref())
            .filter(|m| m.reveal)
            .map(|m| (m.id.clone(), m.duration, m.curve));
        if let Some((id, duration, curve)) = reveal {
            let now = self.started.elapsed().as_secs_f64();
            let curve = curve.unwrap_or(bead_core::animation::Curve::EaseOut);
            self.panel_animations.target(&id, 0., now, duration, curve);
            self.panel_animations.target(
                &id,
                if self.panel_target { 1. } else { 0. },
                now,
                duration,
                curve,
            );
        }
    }
    pub(super) fn close_panel(&mut self) {
        if self.panel_runtime.is_none() {
            return;
        }
        // A pearl has no edge to slide back to: it simply goes.
        if self.pearl.is_some() {
            self.release_panel();
            return;
        }
        self.notify_panel_owner(UiEvent::PanelDismissed);
        self.panel_target = false;
        self.edges_panel_changed(true);
        if self
            .options
            .panel_spec
            .as_ref()
            .is_some_and(|spec| spec.retain_when_closed)
        {
            // Keep its stacking order below the prepared panel. Recreating the
            // catcher on reopen would also recreate the panel surface/buffers.
            if let Err(error) = self.set_dismiss_enabled(false) {
                self.fail(error);
            }
        } else {
            drop(self.dismiss.take());
        }
        // Release the seat before waiting for a worker/animation frame. Closed
        // workers remain alive and may publish new models during keep-alive.
        for surface in self.surfaces.iter_mut().filter(|s| s.companion) {
            surface
                .layer
                .set_keyboard_interactivity(KeyboardInteractivity::None);
            surface.keyboard_visible = Some(KeyboardInteractivity::None);
            match Region::new(&self.compositor) {
                Ok(empty) => {
                    surface
                        .wl_surface()
                        .set_input_region(Some(empty.wl_region()));
                    surface.input_visible = Some(false);
                }
                Err(error) => {
                    eprintln!("PANEL_INPUT_ERROR {error}");
                    surface.input_visible = None;
                }
            }
            surface.input_rect = None;
            surface.commit();
        }
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
        if self
            .options
            .panel_spec
            .as_ref()
            .is_some_and(|spec| spec.retain_when_closed)
        {
            return;
        }
        let qh = self.qh.clone();
        let epoch = self.panel_epoch;
        if let Err(e) = self.loop_handle.insert_source(
            Timer::from_duration(PANEL_KEEP_ALIVE),
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
        // Trace probes are weak: observing retirement must not extend raster lifetime.
        #[cfg(all(target_os = "linux", target_env = "gnu"))]
        let retired_images = self.options.trace_state.then(|| {
            self.companion
                .as_ref()
                .into_iter()
                .flat_map(|(scene, _, _)| scene.prepared.values())
                .flat_map(|frame| &frame.items)
                .filter(|item| matches!(item.primitive, bead_render::Primitive::Image { .. }))
                .filter_map(|item| item.raster.as_ref())
                .map(|raster| std::sync::Arc::downgrade(&raster.pixels))
                .collect::<Vec<_>>()
        });
        if self.pearl.is_none()
            && let Some(output) = self.panel_output.clone()
        {
            self.primary_idle(&output, true);
        }
        let retired_panel_epoch = self.panel_epoch;
        self.panel_target = false;
        // An app process lives exactly as long as its panel.
        if self.options.app.is_some() && self.app_pending.is_none() {
            self.signal.stop();
        }
        self.panel_ambient = false;
        self.panel_center = None;
        self.panel_anchor = None;
        self.panel_epoch = self.next_epoch();
        drop(self.dismiss.take());
        drop(self.panel_runtime.take());
        drop(self.companion.take());
        self.panel_animations = Default::default();
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
        // The last companion update must not remain the active accessibility
        // tree after its worker and surface retire. Restore this output's
        // primary tree even when the bar has no pending visual update.
        let primary_tree = self.panel_output.as_ref().and_then(|output| {
            let id = self.output_id(output);
            let source = if self.options.primary_factory.is_none()
                || self.active_output.as_ref() == Some(&id)
            {
                self.runtime.as_ref()
            } else {
                self.output_sessions
                    .get(&id)
                    .and_then(|session| session.source.as_ref())
            };
            source.and_then(|source| source.accessibility_tree())
        });
        if let (Some(bridge), Some(tree)) = (&mut self.accessibility, primary_tree) {
            if self.options.trace_state {
                println!("PANEL_ACCESSIBILITY_RESTORED nodes={}", tree.nodes.len());
            }
            bridge.update(tree);
        }
        self.panel_output = None;
        self.companion_policy = Default::default();
        self.release_timer = false;
        self.wallpaper_panel_released(retired_panel_epoch);
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
        if self.options.primary_factory.is_none()
            && let Some(source) = &mut self.runtime
        {
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
                        if let Some(images) = &retired_images {
                            let retained_bytes: usize = images
                                .iter()
                                .filter_map(std::sync::Weak::upgrade)
                                .map(|pixels| pixels.data().len())
                                .sum();
                            println!(
                                "PANEL_IMAGE_RELEASE image_rasters={} image_bytes={retained_bytes}",
                                images.len()
                            );
                            assert_eq!(retained_bytes, 0, "closed panel retained image pixels");
                        }
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
    pub(super) enabled: bool,
    pub layer: LayerSurface,
    pub viewport: WpViewport,
    pub buffer: smithay_client_toolkit::shm::slot::Buffer,
    pub attached: bool,
    pub _pool: SlotPool,
}
impl Drop for Dismiss {
    fn drop(&mut self) {
        self.viewport.destroy();
    }
}
impl App {
    pub(super) fn set_dismiss_enabled(&mut self, enabled: bool) -> Result<(), String> {
        let enabled = enabled && !self.options.test_passive;
        let Some(dismiss) = self.dismiss.as_mut() else {
            return Ok(());
        };
        if dismiss.enabled == enabled {
            return Ok(());
        }
        let empty = if enabled {
            None
        } else {
            Some(Region::new(&self.compositor).map_err(|e| e.to_string())?)
        };
        dismiss
            .layer
            .wl_surface()
            .set_input_region(empty.as_ref().map(|r| r.wl_region()));
        dismiss.layer.commit();
        dismiss.enabled = enabled;
        Ok(())
    }
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
            if self
                .options
                .panel_spec
                .as_ref()
                .is_some_and(|spec| spec.overlay)
            {
                Layer::Overlay
            } else {
                Layer::Top
            },
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
            enabled: !self.options.test_passive,
            layer,
            viewport,
            buffer,
            attached: false,
            _pool: pool,
        });
        Ok(())
    }
}
