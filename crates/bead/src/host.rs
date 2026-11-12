// SPDX-License-Identifier: MIT
//! Small reusable host pieces for applications authored as folders of Luau files.
use crate::{
    render::{Damage, Palette, PixelFormat, Rect, RendererKind, Scene, ShmTarget},
    ui::Session,
    wayland::{SceneSource, UiEvent},
};
use bead_core::Value;
use bead_services::{
    Action as DesktopAction, ClockSecondsLease, Config as DesktopConfig, Desktop, Snapshot,
    Subscription,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Duration,
};

#[derive(Clone, Debug, Default)]
pub struct FolderServices {
    pub desktop: BTreeSet<String>,
    pub commands: BTreeMap<String, CommandSpec>,
}
#[derive(Clone, Debug)]
pub struct CommandSpec {
    pub argv: Vec<String>,
    pub interval: Option<Duration>,
    pub timeout: Duration,
}
#[derive(Default)]
struct CommandState {
    results: BTreeMap<String, (String, bool, String)>,
    active: BTreeSet<String>,
}
type ServiceWakeup = Arc<Mutex<Option<Box<dyn Fn(UiEvent) + Send + Sync>>>>;

/// Project the raw desktop snapshot into stable, app-facing stores.
pub fn desktop_values(snapshot: &Snapshot) -> BTreeMap<String, Value> {
    let mut values = BTreeMap::new();
    let mut put = |key: &str, value| {
        values.insert(format!("desktop.{key}"), value);
    };
    put("clock", Value::Text(snapshot.clock.clone()));
    put("clock.seconds", Value::Text(snapshot.clock_seconds.clone()));
    put(
        "date",
        Value::Text(
            snapshot
                .date
                .filter(|d| d.is_valid())
                .map_or_else(String::new, |d| {
                    format!("{:04}-{:02}-{:02}", d.year, d.month, d.day)
                }),
        ),
    );
    put("audio.available", Value::Bool(snapshot.audio.connected));
    put(
        "audio.volume",
        Value::Number(
            snapshot
                .audio
                .volume
                .filter(|v| v.is_finite())
                .map_or(-1., |v| v.clamp(0., 1.)),
        ),
    );
    put(
        "audio.muted",
        Value::Bool(snapshot.audio.muted.unwrap_or(false)),
    );
    put(
        "audio.sink",
        Value::Text(snapshot.audio.default_sink.clone().unwrap_or_default()),
    );
    put("battery.available", Value::Bool(snapshot.battery.available));
    put(
        "battery.percent",
        Value::Number(snapshot.battery.percentage.unwrap_or(-1.)),
    );
    put("battery.charging", Value::Bool(snapshot.battery.charging));
    put(
        "battery.on_battery",
        Value::Bool(snapshot.battery.on_battery),
    );
    put("network.available", Value::Bool(snapshot.network.available));
    put("network.connected", Value::Bool(snapshot.network.connected));
    put(
        "network.wireless",
        Value::Bool(snapshot.network.wireless_enabled),
    );
    put("network.name", Value::Text(snapshot.network.name.clone()));
    put(
        "network.strength",
        Value::Number(snapshot.network.strength.map_or(-1., f64::from)),
    );
    put(
        "bluetooth.available",
        Value::Bool(snapshot.bluetooth.available),
    );
    put("bluetooth.powered", Value::Bool(snapshot.bluetooth.powered));
    put(
        "bluetooth.connected",
        Value::list(
            snapshot
                .bluetooth
                .connected_devices
                .iter()
                .map(|name| serde_json::json!({"name": name}))
                .collect(),
        ),
    );
    put("tray", Value::list(snapshot.tray.iter().map(|item| serde_json::json!({"id": item.id, "title": item.title, "icon_name": item.icon_name, "attention": item.attention, "has_menu": item.has_menu})).collect()));
    put("session.locked", Value::Bool(snapshot.session.locked));
    values
}
fn declared_desktop_values(
    snapshot: &Snapshot,
    declared: &BTreeSet<String>,
) -> BTreeMap<String, Value> {
    desktop_values(snapshot)
        .into_iter()
        .filter(|(key, _)| {
            key.strip_prefix("desktop.")
                .and_then(|rest| rest.split('.').next())
                .is_some_and(|service| declared.contains(service))
        })
        .collect()
}

#[derive(Clone, Debug)]
enum DesktopRequest {
    Action(DesktopAction),
    Command(String),
}
fn route_folder_action(
    name: &str,
    argument: &Value,
    declared: &BTreeSet<String>,
    pointer: Option<(f32, f32)>,
) -> Result<Option<DesktopRequest>, String> {
    if let Some(command) = name.strip_prefix("command.") {
        return if !command.is_empty() {
            Ok(Some(DesktopRequest::Command(command.into())))
        } else {
            Err("command action needs a name".into())
        };
    }
    if !name.starts_with("desktop.") {
        return Ok(None);
    }
    let need = match name {
        "desktop.volume" | "desktop.mute" => "audio",
        "desktop.tray.activate" | "desktop.tray.secondary" | "desktop.tray.menu" => "tray",
        _ => return Err(format!("unknown desktop action '{name}'")),
    };
    if !declared.contains(need) {
        return Err(format!(
            "desktop action '{name}' requires undeclared service '{need}'"
        ));
    }
    let point = pointer
        .map(|(x, y)| (x.round() as i32, y.round() as i32))
        .unwrap_or((0, 0));
    let action = match name {
        "desktop.volume" => match argument {
            Value::Number(v) if v.is_finite() => DesktopAction::Volume(v.clamp(0., 1.)),
            _ => return Err("desktop.volume expects a number".into()),
        },
        "desktop.mute" => match argument {
            Value::Bool(v) => DesktopAction::Mute(*v),
            _ => return Err("desktop.mute expects a boolean".into()),
        },
        "desktop.tray.activate" | "desktop.tray.secondary" | "desktop.tray.menu" => {
            let Value::Text(item) = argument else {
                return Err(format!("{name} expects a tray item id"));
            };
            match name {
                "desktop.tray.activate" => DesktopAction::TrayActivate {
                    item: item.clone(),
                    x: point.0,
                    y: point.1,
                },
                "desktop.tray.secondary" => DesktopAction::TraySecondaryActivate {
                    item: item.clone(),
                    x: point.0,
                    y: point.1,
                },
                _ => DesktopAction::TrayContextMenu {
                    item: item.clone(),
                    x: point.0,
                    y: point.1,
                },
            }
        }
        _ => unreachable!(),
    };
    Ok(Some(DesktopRequest::Action(action)))
}
fn launch_command(
    name: &str,
    specs: &BTreeMap<String, CommandSpec>,
    state: &Arc<Mutex<CommandState>>,
    wake: &ServiceWakeup,
) {
    let Some(spec) = specs.get(name).cloned() else {
        return;
    };
    if let Ok(mut state) = state.lock()
        && !state.active.insert(name.to_owned())
    {
        return;
    }
    let name = name.to_owned();
    let state = state.clone();
    let wake = wake.clone();
    std::thread::spawn(move || {
        let (stdout, ok, error) = execute_command(&spec);
        if let Ok(mut shared) = state.lock() {
            shared.active.remove(&name);
            shared.results.insert(name, (stdout, ok, error));
        }
        if let Ok(slot) = wake.lock()
            && let Some(cb) = slot.as_ref()
        {
            cb(UiEvent::Model(BTreeMap::new()));
        }
    });
}
fn execute_command(spec: &CommandSpec) -> (String, bool, String) {
    let mut command = bead_services::process::command(&spec.argv[0]);
    command.args(&spec.argv[1..]);
    match bead_services::process::run(
        &mut command,
        &[],
        bead_services::process::Limits {
            timeout: spec.timeout,
            output: 8 * 1024 * 1024,
            ..Default::default()
        },
    ) {
        Ok(output) => {
            let raw = String::from_utf8_lossy(&output.stdout);
            let trimmed = raw.trim();
            let capped = if trimmed.len() > 4096 {
                let end = trimmed
                    .char_indices()
                    .take_while(|(i, c)| *i + c.len_utf8() <= 4096)
                    .last()
                    .map_or(0, |(i, c)| i + c.len_utf8());
                &trimmed[..end]
            } else {
                trimmed
            };
            (
                capped.to_owned(),
                output.code == 0,
                if output.code == 0 {
                    String::new()
                } else {
                    format!("command exited {}", output.code)
                },
            )
        }
        Err(e) => (
            String::new(),
            false,
            e.replace("process deadline exceeded", "command timeout exceeded"),
        ),
    }
}

/// An empty, surface-sized fallback for a folder host.
pub fn empty_scene(size: (u32, u32)) -> Scene {
    Scene {
        opacity: 1.,
        motion: None,
        resource_revision: 0,
        asset_data: Default::default(),
        clips: Default::default(),
        layers: vec![],
        prepared: Default::default(),
        font_files: vec![],
        width: size.0 as f32,
        slide: None,
        offset: (0., 0.),
        bounds: Rect::new(0., 0., size.0 as f32, size.1 as f32),
        primitives: vec![],
    }
}

/// A Luau-backed scene source rooted in an application's folder.
pub struct FolderView {
    pub session: Session,
    pending: Option<Scene>,
    size: (u32, u32),
    services: FolderServices,
    desktop: Option<Arc<Desktop>>,
    _subscription: Option<Subscription>,
    _clock_lease: Option<ClockSecondsLease>,
    last_revision: Option<u64>,
    pointer: Option<(f32, f32)>,
    commands: Arc<Mutex<CommandState>>,
    services_wakeup: ServiceWakeup,
    stopped: Arc<std::sync::atomic::AtomicBool>,
}
impl FolderView {
    pub fn load(
        root: PathBuf,
        view: PathBuf,
        palette: PathBuf,
        title: &str,
        size: (u32, u32),
    ) -> Result<Self, Box<dyn Error>> {
        Self::load_with_services(root, view, palette, title, size, FolderServices::default())
    }
    pub fn load_with_services(
        root: PathBuf,
        view: PathBuf,
        palette: PathBuf,
        title: &str,
        size: (u32, u32),
        services: FolderServices,
    ) -> Result<Self, Box<dyn Error>> {
        crate::ui::plugins::configure(
            &root,
            crate::ui::plugins::Config {
                system: vec![root.join("plugins")],
                user: None,
                state: None,
            },
        );
        let palette = if palette.is_absolute() {
            palette
        } else {
            root.join(palette)
        };
        let colors = Palette::read(&palette)?;
        let fallback = empty_scene(size);
        let mut session = Session::new(root, view, palette, colors, fallback);
        session.set_accessibility_label(title);
        session.set_viewport(size.0, size.1);
        let desktop = if services.desktop.is_empty() {
            None
        } else {
            Some(Desktop::start_config(DesktopConfig {
                audio: services.desktop.contains("audio"),
                bus: services.desktop.iter().any(|s| {
                    matches!(
                        s.as_str(),
                        "battery" | "network" | "bluetooth" | "tray" | "session"
                    )
                }),
            }))
        };
        let clock_lease = if services.desktop.contains("clock") {
            desktop.as_ref().map(|d| d.clock_seconds_lease())
        } else {
            None
        };
        let services_wakeup: ServiceWakeup = Arc::new(Mutex::new(None));
        let subscription = desktop.as_ref().map(|d| {
            let signal = services_wakeup.clone();
            d.subscribe(move |_| {
                if let Ok(slot) = signal.lock()
                    && let Some(wake) = slot.as_ref()
                {
                    wake(UiEvent::Model(BTreeMap::new()));
                }
            })
        });
        Ok(Self {
            session,
            pending: None,
            size,
            services,
            desktop,
            _subscription: subscription,
            _clock_lease: clock_lease,
            last_revision: None,
            pointer: None,
            commands: Arc::new(Mutex::new(CommandState::default())),
            services_wakeup,
            stopped: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        })
        .map(|host| {
            for (name, spec) in host.services.commands.clone() {
                if spec.interval.is_some() {
                    host.start_command(&name);
                }
                if let Some(interval) = spec.interval {
                    let stopped = host.stopped.clone();
                    let commands = host.commands.clone();
                    let wake = host.services_wakeup.clone();
                    let specs = host.services.commands.clone();
                    std::thread::spawn(move || {
                        while !stopped.load(std::sync::atomic::Ordering::Acquire) {
                            std::thread::sleep(interval);
                            if stopped.load(std::sync::atomic::Ordering::Acquire) {
                                break;
                            }
                            launch_command(&name, &specs, &commands, &wake);
                        }
                    });
                }
            }
            host
        })
    }
    fn start_command(&self, name: &str) {
        launch_command(
            name,
            &self.services.commands,
            &self.commands,
            &self.services_wakeup,
        );
    }
    fn run_command(&self, name: &str) {
        if !self.services.commands.contains_key(name) {
            eprintln!("unknown command '{name}'");
            return;
        }
        self.start_command(name);
    }
    fn apply_external(&mut self) -> Result<Option<Scene>, Box<dyn Error>> {
        let mut changes = BTreeMap::new();
        if let Some(desktop) = &self.desktop {
            let snapshot = desktop.snapshot();
            if self
                .last_revision
                .is_none_or(|revision| snapshot.revision >= revision)
                && self.last_revision != Some(snapshot.revision)
            {
                self.last_revision = Some(snapshot.revision);
                changes.extend(declared_desktop_values(&snapshot, &self.services.desktop));
            }
        }
        if let Ok(mut state) = self.commands.lock() {
            for (name, (stdout, ok, error)) in std::mem::take(&mut state.results) {
                changes.insert(format!("commands.{name}.stdout"), Value::Text(stdout));
                changes.insert(format!("commands.{name}.ok"), Value::Bool(ok));
                changes.insert(format!("commands.{name}.error"), Value::Text(error));
            }
        }
        changes.retain(|key, _| self.session.has_store(key));
        if changes.is_empty() {
            Ok(None)
        } else {
            Ok(self
                .session
                .update_model(changes)?
                .map(|s| self.surface_scene(s)))
        }
    }
    fn surface_scene(&self, mut scene: Scene) -> Scene {
        // Folder views occupy their surface; Luau layout reserves a panel shadow
        // margin by default, which would clip thin bars and offset their input.
        scene.offset = (0., 0.);
        scene.bounds = Rect::new(0., 0., self.size.0 as f32, self.size.1 as f32);
        scene
    }
    /// Stage, validate, and render one update on the CPU without a display connection.
    pub fn check_render(&mut self, event: UiEvent) -> Result<Scene, Box<dyn Error>> {
        let scene = self.stage(event)?.ok_or("expected a scene update")?;
        self.validate()?;
        let mut renderer = crate::render::renderer(RendererKind::Cpu)?;
        renderer.prepare(&scene, 1.)?;
        let mut pixels = vec![0; (self.size.0 * self.size.1 * 4) as usize];
        renderer.render(
            &scene,
            ShmTarget {
                bytes: &mut pixels,
                width: self.size.0,
                height: self.size.1,
                scale: 1.,
                format: PixelFormat::Rgba,
            },
            Damage::full(self.size.0, self.size.1),
        )?;
        self.commit();
        Ok(scene)
    }
}
impl SceneSource for FolderView {
    fn watch(
        &mut self,
        wakeup: Box<dyn Fn(Result<(), String>) + Send>,
    ) -> Result<(), Box<dyn Error>> {
        self.session.watch(wakeup)
    }
    fn services(&mut self, wakeup: Box<dyn Fn(UiEvent) + Send + Sync>) {
        if let Ok(mut slot) = self.services_wakeup.lock() {
            *slot = Some(wakeup);
        }
    }
    fn viewport(&mut self, w: u32, h: u32) -> bool {
        self.size = (w, h);
        self.session.set_viewport(w, h)
    }
    fn stage(&mut self, event: UiEvent) -> Result<Option<Scene>, Box<dyn Error>> {
        match &event {
            UiEvent::Click { x, y }
            | UiEvent::Hover { x, y }
            | UiEvent::Drag { x, y }
            | UiEvent::LocalDrag { x, y }
            | UiEvent::Scroll { x, y, .. } => self.pointer = Some((*x, *y)),
            _ => {}
        }
        let reload = matches!(&event, UiEvent::Reload);
        let model_update = matches!(&event, UiEvent::Model(_));
        let scene = match event {
            UiEvent::Reload => Some(self.session.reload()?),
            UiEvent::Relayout => self.session.relayout()?,
            UiEvent::Click { x, y } => self.session.click(x, y)?,
            UiEvent::Release => self.session.release()?,
            UiEvent::Hover { x, y } => self.session.hover(x, y)?,
            UiEvent::Drag { x, y } => self.session.drag(x, y)?,
            UiEvent::Key { text, backspace } => self.session.key(&text, backspace)?,
            UiEvent::Scroll { x, y, delta } => self.session.scroll(x, y, delta)?,
            UiEvent::Ime {
                preedit,
                commit,
                before,
                after,
            } => self.session.ime(preedit, commit, before, after)?,
            UiEvent::Model(changes) => self.session.update_model(changes)?,
            UiEvent::Command { command, shift } => match command.as_str() {
                "tab" => self.session.focus_next(shift)?,
                "enter" | "space" => self.session.activate_focused()?,
                "left" => self
                    .session
                    .edit_key(crate::ui::input::EditKey::Left, shift)?,
                "right" => self
                    .session
                    .edit_key(crate::ui::input::EditKey::Right, shift)?,
                _ => None,
            },
            UiEvent::Action { name, argument } => {
                match route_folder_action(&name, &argument, &self.services.desktop, self.pointer) {
                    Ok(Some(DesktopRequest::Action(action))) => {
                        if let Some(desktop) = &self.desktop {
                            if let Err(e) = desktop.action(action) {
                                eprintln!("desktop action {name}: {e}");
                            }
                        } else {
                            eprintln!("desktop action {name}: desktop service is not enabled");
                        }
                        None
                    }
                    Ok(Some(DesktopRequest::Command(command))) => {
                        self.run_command(&command);
                        None
                    }
                    Err(e) => {
                        eprintln!("folder action: {e}");
                        None
                    }
                    Ok(None) => {
                        let patch = self.session.action_values(&name, argument)?;
                        self.session.update(patch)?
                    }
                }
            }
            UiEvent::Accessibility {
                target,
                action,
                value,
            } => self.session.accessibility_action(target, &action, value)?,
            UiEvent::WatchError(e) => return Err(e.into()),
            _ => None,
        };
        let mut scene = scene;
        if reload || model_update {
            scene = self.apply_external()?.or(scene);
        }
        let scene = scene.map(|scene| self.surface_scene(scene));
        self.pending = scene.clone();
        Ok(scene)
    }
    fn validate(&self) -> Result<(), Box<dyn Error>> {
        self.session.check_sources()?;
        Ok(())
    }
    fn state_summary(&self) -> String {
        self.session.state_summary()
    }
    fn pointer_cursor(&self, x: f32, y: f32) -> &str {
        self.session.pointer_cursor(x, y)
    }
    fn surrounding_text(&self) -> Option<(String, i32, i32)> {
        self.session.surrounding_text()
    }
    fn accessibility_tree(&self) -> Option<accesskit::TreeUpdate> {
        Some(self.session.accessibility_tree())
    }
    fn commit(&mut self) -> bool {
        if let Some(scene) = self.pending.take() {
            self.session.commit(scene)
        }
        true
    }
    fn error_scene(&mut self, error: String) -> Scene {
        self.pending = None;
        let scene = self.session.reject(error);
        self.surface_scene(scene)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folder_bar_removes_panel_margin_and_uses_surface_viewport() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/hello-bar");
        let mut view = FolderView::load(
            root,
            "view/main.luau".into(),
            "palette.json".into(),
            "Bar",
            (440, 36),
        )
        .unwrap();
        // Reproduce the old primary path: output-sized layout, then a 36px buffer.
        view.viewport(1280, 720);
        let raw = view.session.reload().unwrap();
        assert_eq!(raw.offset, (32., 32.));
        for primitive in &raw.primitives {
            if let crate::render::Primitive::Text { rect, .. } = primitive {
                assert!(
                    rect.h > 36.,
                    "old path lays out labels against the full output height"
                );
            }
        }
        assert!(view.viewport(1280, 36));
        let scene = view.check_render(UiEvent::Reload).unwrap();
        assert_eq!(scene.offset, (0., 0.));
        assert_eq!(scene.bounds, Rect::new(0., 0., 1280., 36.));
        let mut renderer = crate::render::renderer(RendererKind::Cpu).unwrap();
        for (scene, visible) in [(&raw, false), (&scene, true)] {
            let mut bytes = vec![0; 1280 * 36 * 4];
            renderer.prepare(scene, 1.).unwrap();
            renderer
                .render(
                    scene,
                    ShmTarget {
                        bytes: &mut bytes,
                        width: 1280,
                        height: 36,
                        scale: 1.,
                        format: PixelFormat::Rgba,
                    },
                    Damage::full(1280, 36),
                )
                .unwrap();
            assert_eq!(
                bytes[64 * 4 + 3] != 0,
                visible,
                "surface fill along the top edge, outside rounded corners"
            );
        }
        for label in ["Hello bar", "12:34"] {
            assert!(scene.primitives.iter().any(|p| matches!(p,
                crate::render::Primitive::Text { text, rect, .. }
                if text == label && rect.y < 36. && rect.x >= 0. && rect.x + rect.w <= 1280.
            )));
        }
    }

    #[test]
    fn desktop_projection_covers_live_and_unknown_values() {
        let empty = desktop_values(&Snapshot::default());
        assert_eq!(empty["desktop.battery.percent"], Value::Number(-1.));
        assert_eq!(empty["desktop.audio.volume"], Value::Number(-1.));
        assert_eq!(empty["desktop.network.strength"], Value::Number(-1.));
        let mut snapshot = Snapshot::default();
        snapshot.clock = "12:34".into();
        snapshot.clock_seconds = "12:34:56".into();
        snapshot.date = Some(bead_services::LocalDate {
            year: 2026,
            month: 11,
            day: 12,
        });
        snapshot.audio.connected = true;
        snapshot.audio.volume = Some(0.42);
        snapshot.audio.muted = Some(true);
        snapshot.audio.default_sink = Some("Speakers".into());
        snapshot.battery.available = true;
        snapshot.battery.percentage = Some(83.);
        snapshot.battery.charging = true;
        snapshot.battery.on_battery = false;
        snapshot.network.available = true;
        snapshot.network.connected = true;
        snapshot.network.wireless_enabled = true;
        snapshot.network.name = "wifi".into();
        snapshot.network.strength = Some(70);
        snapshot.bluetooth.available = true;
        snapshot.bluetooth.powered = true;
        snapshot.bluetooth.connected_devices = vec!["Headphones".into()];
        snapshot.tray.push(bead_services::TrayItem {
            id: "item".into(),
            title: "Tray".into(),
            icon_name: "icon".into(),
            attention: true,
            has_menu: true,
            ..Default::default()
        });
        snapshot.session.locked = true;
        let values = desktop_values(&snapshot);
        assert_eq!(
            values["desktop.clock.seconds"],
            Value::Text("12:34:56".into())
        );
        assert_eq!(values["desktop.date"], Value::Text("2026-11-12".into()));
        assert_eq!(values["desktop.audio.volume"], Value::Number(0.42));
        assert_eq!(values["desktop.battery.percent"], Value::Number(83.));
        assert_eq!(values["desktop.network.strength"], Value::Number(70.));
        assert_eq!(values["desktop.audio.available"], Value::Bool(true));
        assert_eq!(values["desktop.audio.muted"], Value::Bool(true));
        assert_eq!(values["desktop.audio.sink"], Value::Text("Speakers".into()));
        assert_eq!(values["desktop.battery.charging"], Value::Bool(true));
        assert_eq!(values["desktop.network.connected"], Value::Bool(true));
        assert_eq!(values["desktop.network.wireless"], Value::Bool(true));
        assert_eq!(values["desktop.network.name"], Value::Text("wifi".into()));
        assert_eq!(values["desktop.bluetooth.powered"], Value::Bool(true));
        assert!(
            matches!(&values["desktop.bluetooth.connected"],Value::List(devices) if devices[0]["name"]=="Headphones")
        );
        assert!(
            matches!(&values["desktop.tray"],Value::List(items) if items[0]["id"]=="item" && items[0]["title"]=="Tray" && items[0]["icon_name"]=="icon" && items[0]["attention"]==true && items[0]["has_menu"]==true)
        );
        assert_eq!(values["desktop.session.locked"], Value::Bool(true));
        let subset = declared_desktop_values(&snapshot, &BTreeSet::from(["clock".to_owned()]));
        assert_eq!(
            subset.keys().cloned().collect::<Vec<_>>(),
            vec!["desktop.clock", "desktop.clock.seconds", "desktop.date"]
        );
    }

    #[test]
    fn action_router_checks_declarations_and_clamps_volume() {
        let declared = BTreeSet::from(["audio".to_owned(), "tray".to_owned()]);
        let Ok(Some(DesktopRequest::Action(DesktopAction::Volume(v)))) =
            route_folder_action("desktop.volume", &Value::Number(2.), &declared, None)
        else {
            panic!("volume was not routed")
        };
        assert_eq!(v, 1.);
        let Ok(Some(DesktopRequest::Action(DesktopAction::Volume(low)))) =
            route_folder_action("desktop.volume", &Value::Number(-1.), &declared, None)
        else {
            panic!("low volume was not routed")
        };
        assert_eq!(low, 0.);
        assert!(
            route_folder_action("desktop.mute", &Value::Bool(true), &BTreeSet::new(), None)
                .unwrap_err()
                .contains("undeclared service")
        );
        assert!(
            route_folder_action("desktop.unknown", &Value::Bool(true), &declared, None)
                .unwrap_err()
                .contains("unknown desktop action")
        );
        let Ok(Some(DesktopRequest::Action(DesktopAction::TrayActivate { item, x, y }))) =
            route_folder_action(
                "desktop.tray.activate",
                &Value::Text("x".into()),
                &declared,
                None,
            )
        else {
            panic!("tray was not routed")
        };
        assert_eq!((item, x, y), ("x".into(), 0, 0));
        let Ok(Some(DesktopRequest::Action(DesktopAction::TraySecondaryActivate { x, y, .. }))) =
            route_folder_action(
                "desktop.tray.secondary",
                &Value::Text("x".into()),
                &declared,
                Some((12.7, 4.3)),
            )
        else {
            panic!("tray secondary action was not routed")
        };
        assert_eq!((x, y), (13, 4));
        assert!(
            route_folder_action("increment", &Value::Bool(true), &declared, None)
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn declared_commands_trim_bound_fail_and_time_out() {
        let spec = |argv: &[&str], timeout| CommandSpec {
            argv: argv.iter().map(|s| s.to_string()).collect(),
            interval: None,
            timeout: Duration::from_secs(timeout),
        };
        assert_eq!(
            execute_command(&spec(&["echo", "hi"], 5)),
            ("hi".into(), true, String::new())
        );
        let (_, ok, error) = execute_command(&spec(&["false"], 5));
        assert!(!ok);
        assert!(error.contains("exited"));
        let (_, ok, error) = execute_command(&spec(&["sleep", "10"], 1));
        assert!(!ok);
        assert!(error.contains("timeout"));
        let (stdout, ok, _) = execute_command(&spec(&["python3", "-c", "print('x'*5000)"], 5));
        assert!(ok);
        assert_eq!(stdout.len(), 4096);
    }

    #[test]
    fn folder_view_click_uses_the_drawn_counter_button_coordinates() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/hello-window");
        let mut view = FolderView::load(
            root,
            "view/main.luau".into(),
            "palette.json".into(),
            "Window",
            (440, 300),
        )
        .unwrap();
        let scene = view.check_render(UiEvent::Reload).unwrap();
        let label = scene
            .primitives
            .iter()
            .find_map(|p| {
                if let crate::render::Primitive::Text { text, rect, .. } = p {
                    (text == "Button presses: 0").then_some(*rect)
                } else {
                    None
                }
            })
            .expect("counter button label");
        let button = scene
            .primitives
            .iter()
            .filter_map(|p| {
                if let crate::render::Primitive::Rounded { rect, .. } = p {
                    (label.x >= rect.x
                        && label.y >= rect.y
                        && label.x + label.w <= rect.x + rect.w
                        && label.y + label.h <= rect.y + rect.h)
                        .then_some(*rect)
                } else {
                    None
                }
            })
            .min_by(|a, b| (a.w * a.h).total_cmp(&(b.w * b.h)))
            .expect("drawn counter button rectangle");
        let x = button.x + button.w / 2.;
        let y = button.y + button.h / 2.;
        let clicked = view.check_render(UiEvent::Click { x, y }).unwrap();
        let _ = view.check_render(UiEvent::Release).unwrap();
        assert!(clicked.primitives.iter().any(
            |p| matches!(p,crate::render::Primitive::Text{text,..} if text=="Button presses: 1")
        ));
        assert!(view.session.state_summary().contains("\"count\":1"));
    }
}

impl Drop for FolderView {
    fn drop(&mut self) {
        self.stopped
            .store(true, std::sync::atomic::Ordering::Release);
    }
}
