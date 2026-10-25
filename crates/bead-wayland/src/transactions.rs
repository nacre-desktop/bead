// SPDX-License-Identifier: MIT OR Apache-2.0
//! Validated worker candidates and IME synchronization.
use super::*;
fn panel_event_belongs_to_host(event: &UiEvent) -> bool {
    matches!(
        event,
        UiEvent::CompanionSelect { .. }
            | UiEvent::CompanionSelectAt { .. }
            | UiEvent::PanelNavigated(_)
            | UiEvent::CompanionOffer { .. }
            | UiEvent::HostModel(_)
            | UiEvent::CompanionAction { .. }
            | UiEvent::CanvasOffer { .. }
            | UiEvent::CanvasModel { .. }
            | UiEvent::CompanionVisibility(true)
            | UiEvent::ShellHidden(_)
            | UiEvent::LivePalette(_)
            | UiEvent::ControlChanged
    )
}

impl App {
    pub(super) fn ui(&mut self, event: UiEvent) {
        if let UiEvent::NativePearlPlaced {
            id,
            epoch,
            rect,
            success,
        } = event
        {
            self.in_slot(id, |app| app.native_pearl_placed(epoch, rect, success));
            return;
        }
        if let UiEvent::NativePearlPrewarmed {
            id,
            epoch,
            generation,
            rect,
            anchor,
        } = event
        {
            if self.reserved_pearl_id == Some(id) {
                self.native_pearl_prewarmed(id, epoch, generation, rect, anchor);
            } else {
                self.in_slot(id, |app| {
                    app.native_pearl_prewarmed(id, epoch, generation, rect, anchor)
                });
            }
            return;
        }
        if let UiEvent::NativePearlMotion(motion) = event {
            self.native_pearl_motion(motion);
            return;
        }
        if self.options.primary_factory.is_some()
            && self.active_output.is_none()
            && matches!(&event, UiEvent::Remote(_) | UiEvent::Accessibility { .. })
        {
            let requested_output = if let UiEvent::Remote(request) = &event {
                request.call.output.as_deref()
            } else {
                None
            };
            let focused_output = self
                .keyboard_surface
                .as_ref()
                .and_then(|surface| {
                    self.surfaces
                        .iter()
                        .find(|s| s.wl_surface() == surface)
                        .map(|s| s.output_name.as_str())
                })
                .or(self.active_input_output.as_deref());
            if let Some(id) = requested_output
                && !self.output_sessions.contains_key(id)
                && let UiEvent::Remote(request) = &event
            {
                let _ = request.reply.send(bead_services::ipc::Reply::error(
                    request.call.id,
                    "output",
                    "unknown output session",
                ));
                return;
            }
            let output = requested_output
                .or_else(|| focused_output.filter(|id| self.output_sessions.contains_key(*id)))
                .or_else(|| self.output_sessions.keys().next().map(String::as_str));
            if let Some(output) = output
                && let Some(session) = self.output_sessions.get(output)
            {
                return self.ui(UiEvent::Output(
                    session.epoch,
                    output.to_owned(),
                    Box::new(event),
                ));
            }
        }
        let event = match event {
            UiEvent::Output(epoch, id, inner) => {
                // Callback closures retain the announcement-time name. The
                // lifetime epoch remains authoritative when a late name arrives.
                let id = if self
                    .output_sessions
                    .get(&id)
                    .is_some_and(|s| s.epoch == epoch)
                {
                    id
                } else if let Some((current, _)) =
                    self.output_sessions.iter().find(|(_, s)| s.epoch == epoch)
                {
                    current.clone()
                } else {
                    return;
                };
                let Some(mut session) = self.output_sessions.remove(&id) else {
                    return;
                };
                if !output_event_is_current(Some(session.epoch), epoch) {
                    self.output_sessions.insert(id, session);
                    return;
                }
                let previous_output = self.active_output.replace(id.clone());
                let previous_epoch = self.active_output_epoch.replace(epoch);
                std::mem::swap(&mut self.runtime, &mut session.source);
                std::mem::swap(&mut self.scene, &mut session.scene);
                std::mem::swap(&mut self.animations, &mut session.animations);
                let previous_serial = std::mem::replace(&mut self.ui_serial, session.ui_serial);
                let previous_primary_serial =
                    std::mem::replace(&mut self.primary_ui_serial, session.ui_serial);
                let previous_initialized =
                    std::mem::replace(&mut self.ui_initialized, session.initialized);
                let was_reload = matches!(&*inner, UiEvent::Reload);
                let wallpaper_palette = match &*inner {
                    UiEvent::WallpaperPalette(palette) => Some(palette.clone()),
                    _ => None,
                };
                self.ui(*inner);
                if let Some(palette) = wallpaper_palette
                    && !self
                        .runtime
                        .as_ref()
                        .is_some_and(|source| source.wallpaper_palette_pending())
                {
                    self.wallpaper_palette_settled(&palette, true);
                }
                session.initialized =
                    std::mem::replace(&mut self.ui_initialized, previous_initialized) || was_reload;
                session.ui_serial = std::mem::replace(&mut self.ui_serial, previous_serial);
                self.primary_ui_serial = previous_primary_serial;
                std::mem::swap(&mut self.animations, &mut session.animations);
                std::mem::swap(&mut self.scene, &mut session.scene);
                std::mem::swap(&mut self.runtime, &mut session.source);
                self.active_output = previous_output;
                self.active_output_epoch = previous_epoch;
                self.output_sessions.insert(id, session);
                return;
            }
            event => event,
        };
        let mut event = event;
        event = match event {
            UiEvent::WallpaperRefresh => {
                self.refresh_wallpapers();
                return;
            }
            UiEvent::WallpaperFrame(frame) => {
                if let Ok(frame) = std::sync::Arc::try_unwrap(frame) {
                    self.complete_wallpaper(frame);
                }
                return;
            }
            UiEvent::WallpaperFailed(output) => {
                self.wallpaper_failed(&output);
                return;
            }
            UiEvent::WallpaperTick(output) => {
                self.wallpaper_tick(&output);
                return;
            }
            other => other,
        };
        if let UiEvent::LivePalette(palette) = event {
            if self.wallpaper_palette.as_ref() != Some(&palette) {
                // An open panel follows through `set_wallpaper_palette`; a window gets it here.
                self.set_wallpaper_palette(palette.clone());
                self.window_ui(UiEvent::WallpaperPalette(palette));
            }
            return;
        }
        if matches!(event, UiEvent::ControlChanged) {
            self.apply_control();
            return;
        }
        if let UiEvent::ShellHidden(state) = event {
            self.set_hidden(state.unwrap_or(!self.hidden));
            return;
        }
        if let UiEvent::CompanionSelectAt {
            view,
            output,
            anchor,
        } = event
        {
            if self.hidden {
                return;
            }
            let output = output.or_else(|| self.active_output.clone());
            if let Err(error) = self.request_panel_anchored(
                Some(&view),
                output.as_deref(),
                true,
                None,
                Some(anchor),
            ) {
                eprintln!("COMPANION_ERROR {error}");
            }
            return;
        }
        if let UiEvent::CompanionSelect {
            view,
            output,
            toggle,
            center,
        } = event
        {
            if let Some(epoch) = self.panel_canvas_owner.take()
                && let Some(sender) = &self.event_sender
            {
                let _ = sender.send(UiEvent::Canvas(epoch, Box::new(UiEvent::PanelDismissed)));
            }
            // Automatic peeks stay suppressed while hidden; explicit opens clear it first.
            if self.hidden {
                return;
            }
            let output = output.or_else(|| self.active_output.clone());
            let result = if self.options.panels.is_none() {
                Err("named panels are not configured".into())
            } else {
                self.requested_panel_differs(Some(&view))
                    .and_then(|changed| {
                        let output_changed = output.as_deref().is_some_and(|target| {
                            self.panel_output
                                .as_ref()
                                .is_none_or(|current| self.output_id(current) != target)
                        });
                        if toggle && self.panel_target && !changed && !output_changed {
                            self.close_panel();
                            Ok(())
                        } else {
                            self.request_panel_at(Some(&view), output.as_deref(), true, center)
                        }
                    })
            };
            if let Err(error) = result {
                eprintln!("COMPANION_ERROR {error}");
            }
            return;
        }
        if let UiEvent::CanvasModel {
            view,
            output,
            values,
        } = event
        {
            let keys: Vec<_> = self
                .canvases
                .keys()
                .filter(|(name, id)| {
                    id == &view && output.as_ref().is_none_or(|output| output == name)
                })
                .cloned()
                .collect();
            for key in keys {
                self.canvas_ui(&key, UiEvent::Model(values.clone()));
            }
            return;
        }
        if let UiEvent::CanvasOffer {
            view,
            output,
            open,
            values,
        } = event
        {
            self.canvas_offer(&view, output.as_deref(), open, values);
            return;
        }
        if let UiEvent::CompanionOffer { view, output, open } = event {
            if self.is_overlay_view(&view) {
                self.overlay_offer(&view, output.as_deref(), open);
                return;
            }
            let shown = self.panel_target && self.panel_name.as_deref() == Some(view.as_str());
            if open {
                if self.hidden || (self.panel_target && !self.panel_ambient && !shown) {
                    return;
                }
                let output = output.or_else(|| self.active_output.clone());
                match self.request_panel_at(Some(&view), output.as_deref(), true, None) {
                    Ok(()) => self.panel_ambient = true,
                    Err(error) => eprintln!("COMPANION_ERROR {error}"),
                }
            } else if shown && self.panel_ambient {
                self.close_panel();
            }
            return;
        }
        if let UiEvent::HostModel(values) = event {
            self.notify_panel_owner(UiEvent::Model(values));
            return;
        }
        if let UiEvent::CompanionAction { name, argument } = event {
            self.panel_ui(UiEvent::Action { name, argument });
            return;
        }
        if let UiEvent::CompanionModel(values) = event {
            if self.panel_runtime.is_some() {
                self.panel_ui(UiEvent::Model(values));
            }
            return;
        }

        if let UiEvent::CompanionVisibility(open) = event {
            // Feedback surfaces stay suppressed while hidden; explicit opens clear it first.
            if open && self.hidden {
                return;
            }
            if open {
                let default_changed = self.options.panels.as_ref().is_some_and(|panels| {
                    self.panel_name.as_deref() != Some(panels.default_name())
                });
                if !self.panel_target || default_changed {
                    let result = if let Some(output) = self.active_output.clone() {
                        self.request_panel(None, Some(&output), true)
                    } else {
                        self.open_panel(None)
                    };
                    if let Err(e) = result {
                        eprintln!("COMPANION_ERROR {e}");
                    }
                }
            } else if self.panel_target {
                if let (Some(owner), Some(panel_output)) = (
                    self.active_output.as_deref(),
                    self.panel_output
                        .as_ref()
                        .map(|output| self.output_id(output)),
                ) && owner != panel_output.as_str()
                {
                    return;
                }
                self.close_panel();
            }
            return;
        }
        if let UiEvent::Overlay(epoch, event) = event {
            if self.overlay.as_ref().is_some_and(|o| o.epoch() == epoch) {
                // A view retires itself when its content vanishes.
                if matches!(&*event, UiEvent::CompanionVisibility(false)) {
                    self.overlay_close();
                } else {
                    self.overlay_ui(*event);
                }
            }
            return;
        }
        if let UiEvent::Canvas(epoch, event) = event {
            self.canvas_event(epoch, *event);
            return;
        }
        if let UiEvent::Window(epoch, event) = event {
            if self.window.as_ref().is_some_and(|w| w.epoch() == epoch) {
                // A window's view can retire itself (a Done button, Escape), as a panel's does.
                if matches!(&*event, UiEvent::CompanionVisibility(false)) {
                    self.close_window();
                } else {
                    self.window_ui(*event);
                }
            }
            return;
        }
        if let UiEvent::Remote(request) = &event
            && self.window_command(request.clone())
        {
            return;
        }
        if let UiEvent::Panel(epoch, event) = event {
            if epoch != self.panel_epoch || self.panel_runtime.is_none() {
                // A detached pearl's worker keeps its own epoch.
                self.with_epoch(epoch, |app| app.ui(UiEvent::Panel(epoch, event)));
                return;
            }
            if epoch == self.panel_epoch && self.panel_runtime.is_some() {
                // A service-backed view can retire itself when its item vanishes.
                // The epoch prevents an old worker from closing its replacement.
                if matches!(&*event, UiEvent::CompanionVisibility(false)) {
                    self.close_panel();
                    return;
                }
                if panel_event_belongs_to_host(&event) {
                    // Panel Runtime emits host commands through its wakeup sender.
                    // After the epoch check, dispatch those commands at the App
                    // level instead of feeding them back into the panel script.
                    if let UiEvent::PanelNavigated(view) = &*event {
                        self.panel_navigated(view.clone());
                        return;
                    }
                    self.ui(*event);
                    return;
                }
                if matches!(&*event, UiEvent::Reload) {
                    if !self.panel_reload_pending {
                        self.panel_reload_pending = true;
                        if let Err(error) = self.loop_handle.insert_source(
                            Timer::from_duration(Duration::from_millis(50)),
                            move |_, _, app| {
                                app.with_epoch(epoch, |app| {
                                    app.panel_reload_pending = false;
                                    app.panel_ui(UiEvent::Reload);
                                });
                                TimeoutAction::Drop
                            },
                        ) {
                            self.fail(error);
                        }
                    }
                } else {
                    let palette_settled = if let UiEvent::Worker(packet) = &*event {
                        packet
                            .wallpaper_palette()
                            .filter(|_| {
                                packet.is_accepted() || packet.is_settled() || packet.is_failed()
                            })
                            .map(|palette| {
                                (palette.clone(), !packet.is_failed() && !packet.has_error())
                            })
                    } else {
                        None
                    };
                    self.panel_ui(*event);
                    if let Some((palette, accepted)) = palette_settled {
                        self.wallpaper_panel_palette_settled(epoch, &palette, accepted);
                    }
                    if self
                        .pearl
                        .is_some_and(|info| info.kind == pearls::Kind::Pearl)
                    {
                        self.schedule_pearl_save();
                    }
                }
            }
            return;
        }
        if let UiEvent::Remote(request) = &event
            && self.panel_command(request.clone())
        {
            return;
        }
        if let UiEvent::Remote(request) = &event {
            event = UiEvent::Remote(std::sync::Arc::new(Remote {
                call: request.call.clone(),
                reply: request.reply.clone(),
                context: CommandContext {
                    panel_open: self.panel_target,
                    panel_configured: self.options.panel_spec.is_some(),
                    panel_view: self.panel_name.clone(),
                    hidden: self.hidden,
                },
            }));
        }
        let palette_packet = if let UiEvent::Worker(packet) = &event {
            packet.wallpaper_palette().map(|palette| {
                (
                    palette.clone(),
                    packet.is_accepted(),
                    packet.is_settled(),
                    packet.is_failed(),
                    packet.has_error(),
                )
            })
        } else {
            None
        };
        let accepted = self.ui_inner(event);
        if let Some((palette, is_accepted, is_settled, is_failed, has_error)) = palette_packet
            && (is_settled || is_failed || is_accepted)
        {
            self.wallpaper_palette_settled(
                &palette,
                is_settled || (is_accepted && accepted && !has_error),
            );
        }
    }
    pub(super) fn ui_inner(&mut self, event: UiEvent) -> bool {
        let palette_commit = match &event {
            UiEvent::WallpaperPalette(_) => true,
            UiEvent::Worker(packet) => packet.wallpaper_palette().is_some(),
            _ => false,
        };
        if matches!(&event, UiEvent::Remote(_)) {
            bead_core::profile::mark("dispatch_received");
        }
        match &event {
            UiEvent::ClipboardSet(text) => {
                if let Err(error) =
                    self.clipboard
                        .set(text.clone(), self.last_input_serial, &self.qh)
                {
                    eprintln!("CLIPBOARD_ERROR {error}");
                }
                return false;
            }
            UiEvent::ClipboardRead => {
                let panel = self.processing_panel;
                let epoch = self.panel_epoch;
                let output = self
                    .active_output
                    .as_ref()
                    .zip(self.active_output_epoch)
                    .map(|(id, epoch)| (epoch, id.clone()));
                if let Some(sender) = self.event_sender.clone()
                    && let Err(error) = self.clipboard.get(move |result| match result {
                        Ok(text) => {
                            let event = UiEvent::ClipboardText(text);
                            let _ = sender.send(if panel {
                                UiEvent::Panel(epoch, Box::new(event))
                            } else if let Some((epoch, id)) = output {
                                UiEvent::Output(epoch, id, Box::new(event))
                            } else {
                                event
                            });
                        }
                        Err(e) => eprintln!("CLIPBOARD_ERROR {e}"),
                    })
                {
                    eprintln!("CLIPBOARD_ERROR {error}");
                }
                return false;
            }
            _ => {}
        }

        if let UiEvent::Remote(request) = &event
            && request.call.target == "bead"
        {
            if request.call.method == "clipboard-set" {
                let result = request
                    .call
                    .args
                    .first()
                    .and_then(|v| v.as_str())
                    .ok_or("clipboard-set needs text")
                    .and_then(|text| {
                        self.clipboard
                            .set(text.into(), self.last_input_serial, &self.qh)
                            .map_err(|_| "native clipboard set unavailable")
                    });
                let reply = match result {
                    Ok(()) => {
                        bead_services::ipc::Reply::ok(request.call.id, serde_json::Value::Null)
                    }
                    Err(e) => bead_services::ipc::Reply::error(request.call.id, "clipboard", e),
                };
                let _ = request.reply.send(reply);
                return false;
            }
            if request.call.method == "clipboard-get" {
                let copy = request.clone();
                let result = self.clipboard.get(move |result| {
                    let reply = match result {
                        Ok(text) => bead_services::ipc::Reply::ok(
                            copy.call.id,
                            serde_json::Value::String(text),
                        ),
                        Err(e) => bead_services::ipc::Reply::error(copy.call.id, "clipboard", e),
                    };
                    let _ = copy.reply.send(reply);
                });
                if let Err(e) = result {
                    let _ = request.reply.send(bead_services::ipc::Reply::error(
                        request.call.id,
                        "clipboard",
                        e,
                    ));
                }
                return false;
            }
            let reply = match request.call.method.as_str() {
                "ping" => {
                    bead_services::ipc::Reply::ok(request.call.id, serde_json::json!({"version":1}))
                }
                "regions" if std::env::var_os("BEAD_PRIVATE_INPUT_PROOF").is_some() => {
                    bead_services::ipc::Reply::ok(
                        request.call.id,
                        serde_json::json!({
                            "regions":self.runtime.as_ref().map(|r|r.input_regions().into_iter().map(|r|serde_json::json!({"id":r.id,"rect":[r.rect.x,r.rect.y,r.rect.w,r.rect.h],"cursor":r.cursor})).collect::<Vec<_>>()).unwrap_or_default(),
                            "offset":[self.scene.offset.0,self.scene.offset.1],"size":[self.scene.bounds.w,self.scene.bounds.h]
                        }),
                    )
                }
                "panel-regions" if std::env::var_os("BEAD_PRIVATE_INPUT_PROOF").is_some() => {
                    bead_services::ipc::Reply::ok(
                        request.call.id,
                        serde_json::json!({
                            "regions":self.panel_runtime.as_ref().map(|r|r.input_regions().into_iter().map(|r|serde_json::json!({"id":r.id,"rect":[r.rect.x,r.rect.y,r.rect.w,r.rect.h],"cursor":r.cursor})).collect::<Vec<_>>()).unwrap_or_default(),
                            "offset":self.companion.as_ref().map(|c|[c.0.offset.0,c.0.offset.1]),
                            "size":self.companion.as_ref().map(|c|[c.0.bounds.w,c.0.bounds.h])
                        }),
                    )
                }
                "overlay" => bead_services::ipc::Reply::ok(request.call.id, self.overlay_state()),
                "overlay-regions" if std::env::var_os("BEAD_PRIVATE_INPUT_PROOF").is_some() => {
                    bead_services::ipc::Reply::ok(request.call.id, self.overlay_regions())
                }
                "state" => bead_services::ipc::Reply::ok(
                    request.call.id,
                    serde_json::json!({
                        "surfaces":self.surfaces.len(),"panel":self.companion.is_some(),
                        "output_sessions":self.output_sessions.values().map(|session| serde_json::json!({
                            "output":session.id,"epoch":session.epoch,"serial":session.ui_serial,
                            "initialized":session.initialized
                        })).collect::<Vec<_>>()
                    }),
                ),
                _ => bead_services::ipc::Reply::error(
                    request.call.id,
                    "unavailable",
                    "native command not implemented",
                ),
            };
            let _ = request.reply.send(reply);
            return false;
        }

        if let UiEvent::Fatal(error) = event {
            if self.active_output.is_some() {
                eprintln!(
                    "OUTPUT_SESSION_FATAL output={} error={error}",
                    self.active_output.as_deref().unwrap_or("")
                );
                if let Some(runtime) = &mut self.runtime {
                    self.scene = runtime.error_scene(error);
                }
            } else {
                self.fail(error);
            }
            return false;
        }
        if matches!(&event, UiEvent::PanelDismissed) && self.panel_target {
            return false;
        }
        if let UiEvent::PanelSelected(view) = &event
            && self.panel_name.as_deref() != Some(view.as_str())
        {
            return false;
        }
        let Some(mut runtime) = self.runtime.take() else {
            return false;
        };
        let active_output = self.active_output.clone();
        let mut scales: Vec<_> = self
            .surfaces
            .iter()
            .filter(|s| {
                s.configured
                    && s.companion == self.processing_panel
                    && output_scope_matches(active_output.as_deref(), &s.output_name)
            })
            .map(|s| s.scale120 as f32 / 120.)
            .collect();
        scales.sort_by(f32::total_cmp);
        scales.dedup();
        if scales.is_empty() && self.processing_panel {
            scales.extend(
                self.surfaces
                    .iter()
                    .filter(|s| !s.companion && self.panel_output.as_ref() == Some(&s.output))
                    .map(|s| s.scale120 as f32 / 120.),
            );
        }
        if scales.is_empty() {
            scales.push(1.);
        }
        runtime.scales(scales);
        let source_error = matches!(&event, UiEvent::Worker(packet) if packet.has_error());
        let reload = matches!(event, UiEvent::Reload)
            || matches!(&event,UiEvent::Worker(packet) if packet.is_reload());
        let started = Instant::now();
        if self.options.trace_state
            && let UiEvent::Backend(snapshot) = &event
        {
            bead_core::vprintln!(
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
                    .filter(|(_, s)| {
                        s.configured
                            && s.companion == self.processing_panel
                            && output_scope_matches(active_output.as_deref(), &s.output_name)
                    })
                    .map(|(i, _)| i)
                    .collect();
                for i in indices {
                    #[cfg(any(feature = "native-gl", feature = "wgpu"))]
                    if !self.processing_panel
                        && self.renderer.wants_gpu()
                        && let Err(error) = self.ensure_gpu(i)
                    {
                        if !self.options.auto_fallback {
                            return Err(error);
                        }
                        eprintln!("FALLBACK GPU unavailable during staging: {error}");
                        self.renderer = bead_render::renderer(RendererKind::Cpu)?;
                        for surface in &mut self.surfaces {
                            #[cfg(feature = "native-gl")]
                            {
                                drop(surface.gl.take());
                                surface.gl_prepared = false;
                                surface.gl_prepare_pending = false;
                            }
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
                    return false;
                }
                if self.processing_panel {
                    // Drawing owns interactivity, including its cached protocol
                    // state. A warm worker can commit while its panel is closed;
                    // a model publication must never reacquire keyboard focus.
                    // Keep the catcher below the panel when pinning, then restore its input.
                    // Recreating it on unpin would map a newer surface above the panel.
                    if runtime.outside_dismiss()
                        && self.dismiss.is_none()
                        && self.panel_target
                        && self.pearl.is_none()
                        && self.options.panel_spec.as_ref().is_some_and(|spec| {
                            spec.keyboard_focus || runtime.surrounding_text().is_some()
                        })
                        && let Some(output) = self.panel_output.clone()
                        && let Err(error) = self.create_dismiss(&output)
                    {
                        self.fail(error);
                    }
                    if let Err(error) =
                        self.set_dismiss_enabled(self.panel_target && runtime.outside_dismiss())
                    {
                        self.fail(error);
                    }
                }
                if let (Some(bridge), Some(tree)) =
                    (&mut self.accessibility, runtime.accessibility_tree())
                {
                    bridge.update(tree);
                }
                if self.options.trace_state {
                    bead_core::vprintln!("UI_STATE {}", runtime.state_summary());
                }
                self.ui_serial += 1;
                if self.processing_panel {
                    self.panel_ui_serial = self.ui_serial;
                } else {
                    self.primary_ui_serial = self.ui_serial;
                }
                for surface in self.surfaces.iter_mut().filter(|s| {
                    s.companion == self.processing_panel
                        && output_scope_matches(active_output.as_deref(), &s.output_name)
                }) {
                    surface.ui_serial = self.ui_serial;
                }
                let reopening = self.scene.motion.as_ref().is_some_and(|m| !m.open)
                    && scene.motion.as_ref().is_some_and(|m| m.open);
                if reopening {
                    for surface in self.surfaces.iter_mut().filter(|s| {
                        s.companion == self.processing_panel
                            && output_scope_matches(active_output.as_deref(), &s.output_name)
                    }) {
                        surface.callbacks.invalidate();
                    }
                }
                self.scene = scene;
                if self.processing_panel && !self.panel_ready {
                    self.panel_content_ready();
                }
                if self.options.panel && (self.processing_panel || self.scene.motion.is_some()) {
                    let anchor = self.scene.motion.as_ref().map(|motion| match motion.edge {
                        bead_core::Edge::Left => Anchor::LEFT | Anchor::TOP,
                        bead_core::Edge::Right => Anchor::RIGHT | Anchor::TOP,
                        bead_core::Edge::Top => Anchor::TOP,
                        bead_core::Edge::Bottom => Anchor::BOTTOM,
                    });
                    for surface in self.surfaces.iter().filter(|s| {
                        s.native.is_none()
                            && s.companion == self.processing_panel
                            && output_scope_matches(active_output.as_deref(), &s.output_name)
                    }) {
                        let placement_changed = if self.processing_panel {
                            self.position_panel_surface(
                                &surface.layer,
                                &surface.output,
                                &self.scene,
                            )
                        } else {
                            if let Some(anchor) = anchor {
                                surface.layer.set_anchor(anchor);
                            }
                            true
                        };
                        if placement_changed {
                            surface.layer.commit();
                        }
                    }
                }
                if self.options.content_size {
                    let width = self.scene.bounds.w.ceil().max(1.) as u32;
                    let height = (self.scene.bounds.y
                        + self.scene.bounds.h
                        + self.scene.slide.map_or(self.scene.offset.1, |s| s.open_y))
                    .ceil()
                    .max(1.) as u32;
                    for surface in self.surfaces.iter().filter(|s| {
                        s.native.is_none()
                            && s.companion == self.processing_panel
                            && output_scope_matches(active_output.as_deref(), &s.output_name)
                    }) {
                        if surface.width != width || surface.height != height {
                            surface.layer.set_size(width, height);
                            surface.layer.commit();
                        }
                    }
                }
                bead_core::vprintln!(
                    "UI_PLATFORM a11y_adapters={} ime_objects={} ui_channels={}",
                    usize::from(self.accessibility.is_some()),
                    usize::from(self.text_input.is_some()),
                    if self.accessibility.is_some() { 2 } else { 1 }
                );
                let (layers, images) = self.renderer.resource_counts();
                bead_core::vprintln!(
                    "UI_RESOURCES {} layers={} gpu_images={} surfaces={} output={}",
                    runtime.resource_counts(),
                    layers,
                    images,
                    self.surfaces.len(),
                    active_output.as_deref().unwrap_or("shared")
                );
                bead_core::vprintln!(
                    "UI_COMMIT generation={} reload={} prepare_ms={:.3} monotonic_ns={} update={} companion={} source_error={} output={} wallpaper_palette={}",
                    self.scene.resource_revision,
                    reload,
                    started.elapsed().as_secs_f64() * 1000.,
                    monotonic_ns(),
                    self.ui_serial,
                    self.processing_panel,
                    source_error,
                    self.active_output.as_deref().unwrap_or("shared"),
                    palette_commit
                );
            }
            Ok(None) => {
                self.runtime = Some(runtime);
                return false;
            }
            Err(error) => {
                eprintln!("UI_ERROR {error}");
                self.scene = runtime.error_scene(error.to_string());
            }
        }
        if let Some(motion) = &self.scene.motion {
            runtime.set_idle(!motion.open);
        }
        self.runtime = Some(runtime);
        if !self.processing_panel
            && self.scene.resource_revision > 0
            && self
                .options
                .panel_spec
                .as_ref()
                .is_some_and(|s| s.initial_open)
        {
            if let Some(spec) = &mut self.options.panel_spec {
                spec.initial_open = false;
            }
            if let Err(error) = self.open_panel(None) {
                eprintln!("PANEL_RESTORE_ERROR {error}");
            }
        }
        self.sync_ime();
        let qh = self.qh.clone();
        if !self.processing_panel {
            for i in 0..self.surfaces.len() {
                if output_scope_matches(active_output.as_deref(), &self.surfaces[i].output_name)
                    && (!self.surfaces[i].companion || self.options.panel_spec.is_none())
                {
                    self.draw(i, &qh);
                }
            }
        }
        true
    }

    pub(super) fn sync_ime(&mut self) {
        let Some(input) = &self.text_input else {
            return;
        };
        if self.ime_surface.is_none() {
            return;
        }
        let panel_focus = self.keyboard_surface.as_ref().is_some_and(|p| {
            self.surfaces
                .iter()
                .any(|s| s.companion && s.wl_surface() == p)
        });
        let source = if self.processing_panel {
            self.runtime.as_ref()
        } else if panel_focus {
            self.panel_runtime.as_ref()
        } else if self.options.primary_factory.is_some() {
            let output = self.keyboard_surface.as_ref().and_then(|surface| {
                self.surfaces
                    .iter()
                    .find(|s| s.wl_surface() == surface)
                    .map(|s| s.output_name.as_str())
            });
            if output == self.active_output.as_deref() {
                self.runtime.as_ref()
            } else {
                output
                    .and_then(|output| self.output_sessions.get(output))
                    .and_then(|session| session.source.as_ref())
            }
        } else {
            self.runtime.as_ref()
        };
        let current = source.and_then(|r| r.surrounding_text());
        let sensitive = source.is_some_and(|r| r.sensitive_input());
        if current == self.ime_last && sensitive == self.ime_sensitive {
            return;
        }
        if let Some((text, cursor, anchor)) = &current {
            input.enable();
            if sensitive {
                input.set_content_type(
                    zwp_text_input_v3::ContentHint::SensitiveData
                        | zwp_text_input_v3::ContentHint::HiddenText,
                    zwp_text_input_v3::ContentPurpose::Password,
                );
            } else {
                input.set_content_type(
                    zwp_text_input_v3::ContentHint::None,
                    zwp_text_input_v3::ContentPurpose::Normal,
                );
            }
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
        self.ime_sensitive = sensitive;
    }
}

#[cfg(test)]
mod panel_event_routing_tests {
    use super::*;
    use std::collections::BTreeMap;

    #[test]
    fn companion_selection_from_panel_is_dispatched_by_the_host() {
        let event = UiEvent::CompanionSelect {
            view: "dashboard-media".into(),
            output: Some("fixture-output".into()),
            toggle: false,
            center: None,
        };
        assert!(panel_event_belongs_to_host(&event));
        assert!(panel_event_belongs_to_host(&UiEvent::PanelNavigated(
            "dashboard-weather".into()
        )));
        assert!(panel_event_belongs_to_host(&UiEvent::CompanionVisibility(
            true
        )));
        assert!(!panel_event_belongs_to_host(&UiEvent::CompanionVisibility(
            false
        )));
        assert!(!panel_event_belongs_to_host(&UiEvent::Reload));
        assert!(!panel_event_belongs_to_host(&UiEvent::Model(
            BTreeMap::new()
        )));
    }
}
