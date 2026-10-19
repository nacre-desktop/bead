// SPDX-License-Identifier: GPL-3.0-only
use bead::backends::Backend;
use bead::render::Scene;
use bead::wayland::{SceneSource, UiEvent};
use std::collections::BTreeMap;
use std::error::Error;
pub struct Runtime {
    pub session: bead::ui::Session,
    pending: Option<Scene>,
    chrome: Option<crate::placement::Chrome>,
    launcher: Option<std::rc::Rc<crate::launcher::Menu>>,
    launcher_subscription: Option<bead::core::source::Subscription>,
    calendar: Option<std::rc::Rc<crate::calendar::Calendar>>,
    calendar_subscription: Option<bead::core::source::Subscription>,
    status: Option<std::rc::Rc<crate::status_views::Status>>,
    status_subscription: Option<bead::core::source::Subscription>,
    status_control_subscription: Option<bead::core::source::Subscription>,
    dashboard: Option<std::rc::Rc<crate::dashboard::Dashboard>>,
    dashboard_subscription: Vec<bead::core::source::Subscription>,
    launcher_memory: Option<crate::launcher::Memory>,
    connection_status: Option<std::sync::Arc<dyn bead::core::source::Source>>,
    connection_subscription: Option<bead::core::source::Subscription>,
    launcher_trigger: bool,
    backend: Option<std::sync::Arc<crate::backends::Native>>,
    subscription: Option<bead::backends::Subscription>,
    desktop: Option<std::sync::Arc<bead::services::Desktop>>,
    desktop_subscription: Option<bead::services::Subscription>,
    notification_hover: Option<u32>,
    notification_primary: bool,
    fonts: crate::font_sources::Selector,
    trace: bool,
    notifications: Option<std::sync::Arc<crate::notifications::Owner>>,
    notification_subscription: Option<crate::notifications::Subscription>,
    menu_item: String,
    pending_focus: Option<String>,
    bar: bool,
    state_file: Option<crate::state::File>,
    actions: bead::core::capabilities::Registry,
    wakeup: Option<std::sync::Arc<dyn Fn(UiEvent) + Send + Sync>>,
}
impl Drop for Runtime {
    fn drop(&mut self) {
        // The bar mirrors legacy state while the launcher is alive. Release it
        // after the panel's 2200 ms close grace or a named-view retirement.
        // Worker drop joins the actor before another factory can replace it;
        // an old epoch cannot clear a new launcher's state. Quick reopen keeps
        // the live worker and its view state.
        if self.launcher.is_some()
            && let Some(memory) = &self.launcher_memory
            && let Ok(mut saved) = memory.lock()
        {
            saved.clear();
            if self.trace {
                println!("PANEL_VIEW_STATE_RELEASED view=launcher entries=0");
            }
        }
    }
}
impl Runtime {
    pub fn new(mut session: bead::ui::Session) -> Self {
        if let Ok(path) = crate::state::File::path()
            && let Some(parent) = path.parent()
        {
            session.enable_recovery(parent.join("ui").join(session.recovery_key()));
        }
        let state_file = match crate::state::File::path()
            .map_err(|e| -> Box<dyn Error> { e.into() })
            .and_then(crate::state::File::open)
        {
            Ok((file, saved)) => {
                session.restore_on_start(saved);
                Some(file)
            }
            Err(error) => {
                eprintln!("STATE_ERROR restore disabled: {error}");
                None
            }
        };
        Self {
            session,
            pending: None,
            chrome: None,
            launcher: None,
            launcher_subscription: None,
            calendar: None,
            calendar_subscription: None,
            status: None,
            status_subscription: None,
            status_control_subscription: None,
            dashboard: None,
            dashboard_subscription: Vec::new(),
            launcher_memory: None,
            connection_status: None,
            connection_subscription: None,
            launcher_trigger: false,
            backend: None,
            subscription: None,
            desktop: None,
            desktop_subscription: None,
            notification_hover: None,
            notification_primary: false,
            fonts: Default::default(),
            trace: false,
            notifications: None,
            notification_subscription: None,
            menu_item: String::new(),
            pending_focus: None,
            bar: false,
            state_file,
            actions: Default::default(),
            wakeup: None,
        }
    }
    pub fn with_connection_status(
        mut self,
        source: std::sync::Arc<dyn bead::core::source::Source>,
    ) -> Result<Self, Box<dyn Error>> {
        // Subscriptions deliver immediately, before the first UI generation.
        // Declare this host-owned view state before any callback can update it.
        self.session.stores = self
            .session
            .stores
            .reconcile(&std::collections::BTreeMap::from([(
                "bar.reconnecting".into(),
                bead::core::Value::Bool(source.snapshot().data["reconnecting"] == true),
            )]))?;
        self.connection_status = Some(source);
        Ok(self)
    }
    pub fn panel_session(session: bead::ui::Session, open: bool) -> Self {
        let mut runtime = Self::new(session);
        if let Ok((_, mut saved)) = crate::state::File::path()
            .map_err(|e| -> Box<dyn Error> { e.into() })
            .and_then(crate::state::File::open)
        {
            saved.insert("panel.open".into(), bead::core::Value::Bool(open));
            runtime.session.restore_on_start(saved);
        }
        runtime
    }
    pub fn with_launcher(
        mut self,
        source: Option<std::sync::Arc<dyn bead::core::source::Source>>,
    ) -> Result<Self, Box<dyn Error>> {
        if self.launcher.is_some()
            || self.calendar.is_some()
            || self.status.is_some()
            || self.dashboard.is_some()
        {
            return Err("one data projector per panel".into());
        }
        let menu = std::rc::Rc::new(crate::launcher::Menu::new(source)?);
        let projector = menu.clone();
        self.session
            .set_projector(Box::new(move |stores| projector.project(stores)));
        self.session.stores = self
            .session
            .stores
            .reconcile(&std::collections::BTreeMap::from([(
                "bead.focused".into(),
                bead::core::Value::Text(String::new()),
            )]))?;
        self.session
            .stores
            .commit(std::collections::BTreeMap::from([(
                "bead.focused".into(),
                bead::core::Value::Text("launcher.search".into()),
            )]))?;
        self.actions.register("select_app", "launcher")?;
        self.actions.grant("launcher");
        self.launcher = Some(menu);
        Ok(self)
    }
    pub fn with_calendar(
        mut self,
        source: std::sync::Arc<dyn bead::core::source::Source>,
    ) -> Result<Self, Box<dyn Error>> {
        if self.launcher.is_some()
            || self.calendar.is_some()
            || self.status.is_some()
            || self.dashboard.is_some()
        {
            return Err("one data projector per panel".into());
        }
        let calendar = std::rc::Rc::new(crate::calendar::Calendar::new(source));
        let projector = calendar.clone();
        self.session
            .set_projector(Box::new(move |stores| Ok(projector.project(stores))));
        self.calendar = Some(calendar);
        Ok(self)
    }
    pub fn with_status(
        self,
        kind: crate::status_views::Kind,
        source: std::sync::Arc<dyn bead::core::source::Source>,
    ) -> Result<Self, Box<dyn Error>> {
        self.with_status_projector(crate::status_views::Status::new(kind, source))
    }
    pub fn with_core_network_status(
        self,
        source: std::sync::Arc<dyn bead::core::source::Source>,
    ) -> Result<Self, Box<dyn Error>> {
        self.with_status_projector(crate::status_views::Status::core_network(source))
    }
    pub fn with_core_network_control(
        self,
        source: std::sync::Arc<dyn bead::core::source::Source>,
        client: crate::nacre_source::Client,
    ) -> Result<Self, Box<dyn Error>> {
        let mut runtime = self.with_status_projector(
            crate::status_views::Status::core_network_control(source, client),
        )?;
        runtime.session.stores = runtime.session.stores.reconcile(&BTreeMap::from([
            (
                "network.wifi_checked".into(),
                bead::core::Value::Bool(false),
            ),
            (
                "network.wifi_enabled".into(),
                bead::core::Value::Bool(false),
            ),
            ("network.wifi_busy".into(), bead::core::Value::Bool(false)),
            (
                "network.wifi_message".into(),
                bead::core::Value::Text(String::new()),
            ),
        ]))?;
        runtime
            .actions
            .register("network.toggle_wifi", "network.control")?;
        runtime.actions.grant("network.control");
        Ok(runtime)
    }
    pub fn with_core_bluetooth_status(
        self,
        source: std::sync::Arc<dyn bead::core::source::Source>,
    ) -> Result<Self, Box<dyn Error>> {
        self.with_status_projector(crate::status_views::Status::core_bluetooth(source))
    }
    pub fn with_core_bluetooth_control(
        self,
        source: std::sync::Arc<dyn bead::core::source::Source>,
        client: crate::nacre_source::Client,
    ) -> Result<Self, Box<dyn Error>> {
        let mut runtime = self.with_status_projector(
            crate::status_views::Status::core_bluetooth_control(source, client),
        )?;
        runtime.session.stores = runtime.session.stores.reconcile(&BTreeMap::from([
            (
                "bluetooth.power_visible".into(),
                bead::core::Value::Bool(false),
            ),
            (
                "bluetooth.power_checked".into(),
                bead::core::Value::Bool(false),
            ),
            (
                "bluetooth.power_enabled".into(),
                bead::core::Value::Bool(false),
            ),
            (
                "bluetooth.power_busy".into(),
                bead::core::Value::Bool(false),
            ),
            (
                "bluetooth.power_message".into(),
                bead::core::Value::Text(String::new()),
            ),
        ]))?;
        runtime
            .actions
            .register("bluetooth.toggle_power", "bluetooth.control")?;
        runtime.actions.grant("bluetooth.control");
        Ok(runtime)
    }
    pub fn with_core_battery_status(
        self,
        source: std::sync::Arc<dyn bead::core::source::Source>,
    ) -> Result<Self, Box<dyn Error>> {
        self.with_status_projector(crate::status_views::Status::core_battery(source))
    }
    fn with_status_projector(
        mut self,
        status: crate::status_views::Status,
    ) -> Result<Self, Box<dyn Error>> {
        if self.launcher.is_some()
            || self.calendar.is_some()
            || self.status.is_some()
            || self.dashboard.is_some()
        {
            return Err("one data projector per panel".into());
        }
        let status = std::rc::Rc::new(status);
        let projector = status.clone();
        self.session
            .set_projector(Box::new(move |_| Ok(projector.project())));
        self.status = Some(status);
        Ok(self)
    }
    pub fn with_dashboard(
        mut self,
        page: crate::dashboard::Page,
        source: std::sync::Arc<dyn bead::core::source::Source>,
        updates: Option<std::sync::Arc<dyn bead::core::source::Source>>,
        clock: Option<std::sync::Arc<dyn bead::core::source::Source>>,
        preview: bool,
    ) -> Result<Self, Box<dyn Error>> {
        if self.launcher.is_some()
            || self.calendar.is_some()
            || self.status.is_some()
            || self.dashboard.is_some()
        {
            return Err("one data projector per panel".into());
        }
        let dashboard = std::rc::Rc::new(
            crate::dashboard::Dashboard::new(page, source, updates, preview).with_clock(clock),
        );
        let projector = dashboard.clone();
        self.session.set_projector(Box::new(move |stores| {
            Ok(projector.project_with_stores(stores))
        }));
        self.actions.register("view.open", "ui.popouts")?;
        self.actions.grant("ui.popouts");
        if let Some(chrome) = &mut self.chrome {
            chrome.set_engaged(true);
        }
        self.dashboard = Some(dashboard);
        Ok(self)
    }
    pub fn with_launcher_memory(
        mut self,
        memory: crate::launcher::Memory,
    ) -> Result<Self, Box<dyn Error>> {
        let saved = memory
            .lock()
            .map_err(|_| "launcher view state unavailable")?
            .clone();
        self.session.stores = self.session.stores.reconcile(&saved)?;
        self.launcher_memory = Some(memory);
        Ok(self)
    }
    pub fn with_launcher_trigger(
        mut self,
        memory: crate::launcher::Memory,
    ) -> Result<Self, Box<dyn Error>> {
        self.launcher_trigger = true;
        self.launcher_memory = Some(memory);
        self.actions.register("launcher.open", "launcher.view")?;
        self.actions.grant("launcher.view");
        self.session.stores = self
            .session
            .stores
            .reconcile(&std::collections::BTreeMap::from([(
                "bar.launcher".into(),
                bead::core::Value::Bool(true),
            )]))?;
        Ok(self)
    }
    pub fn with_popout_trigger(mut self) -> Result<Self, Box<dyn Error>> {
        self.actions.register("view.open", "ui.popouts")?;
        self.actions.grant("ui.popouts");
        self.session.stores = self
            .session
            .stores
            .reconcile(&std::collections::BTreeMap::from([(
                "bar.popouts".into(),
                bead::core::Value::Bool(true),
            )]))?;
        Ok(self)
    }
    fn popout_center(&self, id: &str, view: &str) -> Option<f32> {
        // Sound remains a side panel. Top-bar popouts center on their own
        // invoking control, including keyboard activation.
        (self.bar && matches!(view, "calendar" | "network" | "bluetooth" | "battery"))
            .then(|| {
                self.session
                    .view_nodes()
                    .iter()
                    .find(|n| n.id == id)
                    .map(|node| {
                        let rect = bead::ui::rect(&node.draw);
                        rect.x + rect.w / 2.
                    })
            })
            .flatten()
    }
    pub fn with_trace(mut self, trace: bool) -> Self {
        self.trace = trace;
        self
    }
    pub fn with_notifications(
        mut self,
        socket: std::path::PathBuf,
        history: std::path::PathBuf,
    ) -> Result<Self, Box<dyn Error>> {
        self.session.stores = self
            .session
            .stores
            .reconcile(&crate::notifications::values(&serde_json::Value::Null))?;
        for name in [
            "notifications.details",
            "notifications.dismiss",
            "notifications.action",
            "notifications.dnd",
            "notifications.feedback",
            "notifications.clear",
        ] {
            self.actions.register(name, "notifications")?;
        }
        self.actions.grant("notifications");
        self.notification_primary = true;
        self.state_file = None;
        self.notifications = Some(std::sync::Arc::new(crate::notifications::Owner::start(
            socket, history,
        )?));
        Ok(self)
    }
    pub fn notification_owner(&self) -> Option<std::sync::Arc<crate::notifications::Owner>> {
        self.notifications.clone()
    }
    pub fn with_notification_owner(
        mut self,
        owner: std::sync::Arc<crate::notifications::Owner>,
        open: bool,
    ) -> Result<Self, Box<dyn Error>> {
        self.session.stores = self
            .session
            .stores
            .reconcile(&crate::notifications::values(&serde_json::Value::Null))?;
        for name in [
            "notifications.details",
            "notifications.dismiss",
            "notifications.action",
            "notifications.dnd",
            "notifications.feedback",
            "notifications.clear",
        ] {
            self.actions.register(name, "notifications")?;
        }
        self.actions.grant("notifications");
        self.state_file = None;
        self.session
            .restore_on_start(std::collections::BTreeMap::from([(
                "panel.open".into(),
                bead::core::Value::Bool(open),
            )]));
        self.notifications = Some(owner);
        Ok(self)
    }
    pub fn with_desktop(self) -> Result<Self, Box<dyn Error>> {
        self.with_shared_desktop(bead::services::Desktop::start(), true)
    }
    pub fn desktop_owner(&self) -> Option<std::sync::Arc<bead::services::Desktop>> {
        self.desktop.clone()
    }
    pub fn with_shared_desktop(
        mut self,
        desktop: std::sync::Arc<bead::services::Desktop>,
        bar: bool,
    ) -> Result<Self, Box<dyn Error>> {
        self.session.stores = self
            .session
            .stores
            .reconcile(&crate::desktop::values(&desktop.snapshot(), ""))?;
        for name in [
            "services.audio",
            "services.audio.volume",
            "services.tray",
            "services.menu",
        ] {
            self.actions.register(name, "desktop.services")?;
        }
        self.actions.grant("desktop.services");
        self.desktop = Some(desktop);
        self.bar = bar;
        if bar {
            // Durable panel pages are loaded by their own worker on first open.
            // The bar must not retain either startup copy after configuration.
            self.state_file = None;
            self.session.restore_on_start(Default::default());
        }
        Ok(self)
    }
    fn desktop_action(
        &mut self,
        name: &str,
        arg: bead::core::Value,
        delta: Option<f64>,
        context_menu: bool,
    ) -> Result<(), Box<dyn Error>> {
        self.actions.authorize(name)?;
        use bead::services::Action;
        let desktop = self.desktop.as_ref().ok_or("desktop unavailable")?;
        let snapshot = desktop.snapshot();
        let action = match name {
            "services.audio.volume" => {
                let bead::core::Value::Number(value) = arg else {
                    return Err("volume requires a number".into());
                };
                Action::Volume((value / 100.).clamp(0., 1.))
            }
            "services.audio" => {
                if let Some(delta) = delta {
                    Action::Volume(
                        (snapshot.audio.volume.ok_or("audio unavailable")? + delta.signum() * 0.05)
                            .clamp(0., 1.),
                    )
                } else {
                    Action::Mute(!snapshot.audio.muted.ok_or("audio unavailable")?)
                }
            }
            "services.tray" => {
                let bead::core::Value::Text(item) = arg else {
                    return Err("tray ID required".into());
                };
                let tray = snapshot
                    .tray
                    .iter()
                    .find(|t| t.id == item)
                    .ok_or("stale tray ID")?;
                if tray.item_is_menu || context_menu {
                    self.menu_item = if self.menu_item == item {
                        String::new()
                    } else {
                        item.clone()
                    };
                    Action::TrayPrepare { item }
                } else {
                    Action::TrayActivate { item, x: 0, y: 0 }
                }
            }
            "services.menu" => {
                let bead::core::Value::Text(value) = arg else {
                    return Err("menu ID required".into());
                };
                let (item, entry) = value.rsplit_once('|').ok_or("menu ID required")?;
                self.menu_item.clear();
                Action::TrayMenu {
                    item: item.into(),
                    entry: entry.parse()?,
                }
            }
            _ => return Err("unknown desktop action".into()),
        };
        desktop
            .action(action)
            .map_err(|e| -> Box<dyn Error> { e.into() })
    }
    pub fn with_backend(
        mut self,
        backend: std::sync::Arc<crate::backends::Native>,
    ) -> Result<Self, Box<dyn Error>> {
        let values = backend_values(&backend.snapshot())?;
        self.session.stores = self.session.stores.reconcile(&values)?;
        for name in [
            "backend.workspace.switch",
            "backend.window.focus",
            "backend.window.close",
            "backend.windowTrash",
            "backend.scratchpad",
            "backend.windowCapture",
        ] {
            self.actions.register(name, "desktop.compositor")?;
        }
        self.actions.grant("desktop.compositor");
        self.backend = Some(backend);
        Ok(self)
    }
    fn backend_action(&self, name: &str, arg: bead::core::Value) -> Result<(), Box<dyn Error>> {
        self.actions.authorize(name)?;
        let backend = self.backend.as_ref().ok_or("backend unavailable")?;
        let bead::core::Value::Text(id) = arg else {
            return Err("backend action requires a stable ID".into());
        };
        let id = bead::backends::Id(id);
        let action = match name {
            "backend.workspace.switch" => bead::backends::Action::SwitchWorkspace { workspace: id },
            "backend.window.focus" => bead::backends::Action::FocusWindow { window: id },
            "backend.window.close" => bead::backends::Action::CloseWindow { window: id },
            "backend.windowTrash" => bead::backends::Action::WindowTrash { window: id },
            "backend.scratchpad" => bead::backends::Action::Scratchpad { window: Some(id) },
            "backend.windowCapture" => bead::backends::Action::WindowCapture { window: id },
            _ => {
                return Err(bead::backends::Error::Unsupported {
                    operation: name.into(),
                    backend: backend.snapshot().backend.clone(),
                }
                .into());
            }
        };
        let started = std::time::Instant::now();
        backend.action(action)?;
        println!(
            "BACKEND_ACTION name={name} request_ms={:.3}",
            started.elapsed().as_secs_f64() * 1000.
        );
        Ok(())
    }
}
impl Runtime {
    pub fn with_frame(
        mut self,
        attachment: crate::placement::Attachment,
        colors: &bead::render::Palette,
    ) -> Result<Self, Box<dyn Error>> {
        self.chrome = Some(crate::placement::Chrome::new(attachment, colors)?);
        Ok(self)
    }
    fn finish_scene(
        &mut self,
        mut scene: Option<Scene>,
        reload: bool,
    ) -> Result<Option<Scene>, Box<dyn Error>> {
        if let Some(scene) = &mut scene {
            if let Some(desktop) = &self.desktop {
                crate::desktop::icons(scene, &desktop.snapshot())?;
            }
            let asset_profile = bead::core::profile::Span::new("assets");
            for draw in &mut scene.primitives {
                if let bead::render::Primitive::Image { path, .. } = draw
                    && let Some(icon) = path.to_str().and_then(|s| s.strip_prefix("@icon:"))
                {
                    *path = if std::env::var_os("BEAD_MATCHED_ICONS").is_some() {
                        std::env::var_os("BEAD_ICON_PATH")
                            .map(std::path::PathBuf::from)
                            .unwrap_or_else(|| "shell/sample.svg".into())
                    } else {
                        freedesktop_icons::lookup(icon)
                            .with_size(48)
                            .find()
                            .unwrap_or_else(|| "shell/sample.svg".into())
                    };
                    let data = if !reload {
                        if let Some(data) = self.session.asset_snapshot(path) {
                            data
                        } else {
                            std::sync::Arc::from(std::fs::read(&*path)?)
                        }
                    } else {
                        std::sync::Arc::from(std::fs::read(&*path)?)
                    };
                    scene.asset_data.insert(path.clone(), data);
                }
            }
            drop(asset_profile);
            // Service text can introduce a family/codepoint after an initially empty title.
            scene.font_files = self.fonts.select(scene)?;
        }
        if let Some(scene) = &mut scene {
            if self.bar {
                scene.offset = (0., 0.);
                scene.slide = None;
                scene.bounds = bead::render::Rect::new(0., 0., scene.width, 50.);
            }
            let editor_profile = bead::core::profile::Span::new("editor_shape");
            self.session.decorate_inputs(scene)?;
            drop(editor_profile);
            if let Some(chrome) = &self.chrome {
                chrome.decorate(scene)?;
            }
        }
        self.session.check_sources()?;
        self.pending = scene.clone();
        Ok(scene)
    }
}
impl SceneSource for Runtime {
    fn panel_center(&mut self, center: Option<f32>) {
        if let Some(chrome) = &mut self.chrome {
            chrome.center = center;
        }
    }
    fn services(&mut self, wakeup: Box<dyn Fn(UiEvent) + Send + Sync>) {
        let wakeup: std::sync::Arc<dyn Fn(UiEvent) + Send + Sync> = std::sync::Arc::from(wakeup);
        self.wakeup = Some(wakeup.clone());
        if let Some(source) = &self.connection_status {
            let wakeup = wakeup.clone();
            match source.subscribe(std::sync::Arc::new(move |snapshot| {
                let mark = snapshot.data["reconnecting"] == true;
                wakeup(UiEvent::Model(std::collections::BTreeMap::from([(
                    "bar.reconnecting".into(),
                    bead::core::Value::Bool(mark),
                )])));
            })) {
                Ok(subscription) => self.connection_subscription = Some(subscription),
                Err(_) => eprintln!("SOURCE_ERROR code=connection_subscription"),
            }
        }
        if let Some(calendar) = &self.calendar {
            match calendar.subscribe(wakeup.clone()) {
                Ok(subscription) => self.calendar_subscription = Some(subscription),
                Err(_) => eprintln!("SOURCE_ERROR code=calendar_subscription"),
            }
        }
        if let Some(status) = &self.status {
            match status.subscribe(wakeup.clone()) {
                Ok(subscription) => self.status_subscription = Some(subscription),
                Err(_) => eprintln!("SOURCE_ERROR code=status_subscription"),
            }
            let wakeup = wakeup.clone();
            let notify: crate::nacre_source::CallWake = std::sync::Arc::new(move || {
                wakeup(bead::wayland::UiEvent::Model(BTreeMap::new()));
            });
            match status.subscribe_control_link(notify) {
                Ok(subscription) => self.status_control_subscription = Some(subscription),
                Err(error) if error.code == "unavailable" => {}
                Err(_) => eprintln!("SOURCE_ERROR code=status_control_subscription"),
            }
        }
        if let Some(dashboard) = &self.dashboard {
            match dashboard.subscribe(wakeup.clone()) {
                Ok(subscription) => self.dashboard_subscription = subscription,
                Err(_) => eprintln!("SOURCE_ERROR code=dashboard_subscription"),
            }
        }
        if let Some(menu) = &self.launcher {
            let wakeup = wakeup.clone();
            match menu.subscribe(std::sync::Arc::new(move |_| {
                wakeup(UiEvent::Model(Default::default()))
            })) {
                Ok(subscription) => self.launcher_subscription = subscription,
                Err(_) => eprintln!("SOURCE_ERROR code=launcher_subscription"),
            }
        }
        if let Some(owner) = &self.notifications {
            self.notification_subscription =
                Some(owner.subscribe(wakeup.clone(), self.notification_primary));
        }
        if let Some(desktop) = &self.desktop {
            let wakeup = wakeup.clone();
            self.desktop_subscription =
                Some(desktop.subscribe(move |snapshot| wakeup(UiEvent::Desktop(snapshot))));
        }
        if let Some(backend) = &self.backend {
            self.subscription = Some(backend.subscribe(Box::new(move |event| {
                if let bead::backends::Event::Snapshot(snapshot) = event {
                    wakeup(UiEvent::Backend(snapshot));
                }
            })));
        }
    }
    fn input_regions(&self) -> Vec<bead::wayland::InputRegion> {
        self.session
            .view_nodes()
            .iter()
            .filter(|node| node.input.is_some() || node.action.is_some())
            .map(|node| {
                let rect = match &node.draw {
                    bead::render::Primitive::Rounded { rect, .. }
                    | bead::render::Primitive::Text { rect, .. }
                    | bead::render::Primitive::Image { rect, .. }
                    | bead::render::Primitive::Shadow { rect, .. } => *rect,
                };
                bead::wayland::InputRegion {
                    id: node.id.clone(),
                    prepare_panel: self.bar
                        && (node.action.as_deref() == Some("view.open")
                            || node.action.as_deref()
                                == Some(if self.launcher_trigger {
                                    "launcher.open"
                                } else {
                                    "services.audio"
                                })),
                    prepare_view: if self.bar && node.action.as_deref() == Some("view.open") {
                        if let Some(bead::core::Value::Text(view)) = &node.action_arg {
                            Some(view.clone())
                        } else {
                            None
                        }
                    } else {
                        None
                    },
                    prepare_center: if node.action.as_deref() == Some("view.open") {
                        if let Some(bead::core::Value::Text(view)) = &node.action_arg {
                            self.popout_center(&node.id, view)
                        } else {
                            None
                        }
                    } else {
                        None
                    },
                    dismiss: node.action.as_deref() == Some("close")
                        && self.session.stores.get("panel.open").is_some(),
                    rect,
                    clip: node.clip,
                    cursor: if node.input.is_some() {
                        "text"
                    } else {
                        "pointer"
                    },
                }
            })
            .collect()
    }
    fn pointer_cursor(&self, x: f32, y: f32) -> &str {
        self.session.pointer_cursor(x, y)
    }
    fn accessibility_tree(&self) -> Option<accesskit::TreeUpdate> {
        Some(self.session.accessibility_tree())
    }
    fn surrounding_text(&self) -> Option<(String, i32, i32)> {
        self.session.surrounding_text()
    }
    fn viewport(&mut self, width: u32, height: u32) -> bool {
        if let Some(chrome) = &mut self.chrome {
            chrome.output = (width, height);
        }
        self.session.set_viewport(width, height)
    }
    fn watch(
        &mut self,
        wakeup: Box<dyn Fn(Result<(), String>) + Send>,
    ) -> Result<(), Box<dyn Error>> {
        self.session.watch(wakeup)
    }
    fn stage(&mut self, mut event: UiEvent) -> Result<Option<Scene>, Box<dyn Error>> {
        self.pending = None;
        if let UiEvent::Hover { x, y } = &event
            && let Some(owner) = &self.notifications
        {
            let next = self.session.action_at(*x, *y).and_then(|(_, name, arg)| {
                if !name.starts_with("notifications.") {
                    return None;
                }
                if let bead::core::Value::Text(id) = arg {
                    id.split('|').next().and_then(|id| id.parse::<u32>().ok())
                } else {
                    None
                }
            });
            if next != self.notification_hover {
                if let Some(id) = self.notification_hover.take() {
                    owner.hover(id, false);
                }
                if let Some(id) = next {
                    owner.hover(id, true);
                    self.notification_hover = Some(id);
                }
            }
        }
        let mut host_action = match &event {
            UiEvent::Click { x, y }
            | UiEvent::ContextMenu { x, y }
            | UiEvent::Scroll { x, y, .. } => self.session.action_at(*x, *y),
            UiEvent::Command { command, .. } if command == "enter" => self.session.focused_action(),
            UiEvent::Command { command, .. } if command == "left" || command == "right" => {
                self.session.range_action(command == "left")
            }
            UiEvent::Drag { x, y } => self.session.drag_action(*x, *y),
            _ => None,
        };
        if let Some((_, name, arg)) = &mut host_action
            && name == "view.open"
            && *arg == bead::core::Value::Text("sound".into())
            && matches!(&event, UiEvent::Scroll { .. })
        {
            *name = "services.audio".into();
        }
        if let Some((id, name, bead::core::Value::Text(view))) = &host_action
            && name == "view.open"
            && (matches!(&event, UiEvent::Click { .. })
                || matches!(&event, UiEvent::Command {command,..} if command == "enter"))
        {
            self.actions.authorize(name)?;
            self.wakeup
                .as_ref()
                .ok_or("view event channel unavailable")?(UiEvent::CompanionSelect {
                view: view.clone(),
                output: None,
                toggle: true,
                center: self.popout_center(id, view),
            });
            return Ok(None);
        }
        if let Some((_, name, _)) = &host_action
            && name == "network.toggle_wifi"
            && (matches!(&event, UiEvent::Click { .. })
                || matches!(&event, UiEvent::Command { command, .. } if command == "enter"))
        {
            self.actions.authorize(name)?;
            let wakeup = self
                .wakeup
                .clone()
                .ok_or("Wi-Fi completion event channel unavailable")?;
            let callback: crate::nacre_source::CallWake = std::sync::Arc::new(move || {
                wakeup(UiEvent::Model(BTreeMap::new()));
            });
            if let Some(status) = &self.status {
                status.wifi_action(callback);
            }
            let scene = self.session.update(BTreeMap::new())?;
            return self.finish_scene(scene, false);
        }
        if let Some((_, name, _)) = &host_action
            && name == "bluetooth.toggle_power"
            && (matches!(&event, UiEvent::Click { .. })
                || matches!(&event, UiEvent::Command { command, .. } if command == "enter"))
        {
            self.actions.authorize(name)?;
            let wakeup = self
                .wakeup
                .clone()
                .ok_or("Bluetooth completion event channel unavailable")?;
            let callback: crate::nacre_source::CallWake = std::sync::Arc::new(move || {
                wakeup(UiEvent::Model(BTreeMap::new()));
            });
            if let Some(status) = &self.status {
                status.bluetooth_action(callback);
            }
            let scene = self.session.update(BTreeMap::new())?;
            return self.finish_scene(scene, false);
        }
        if let Some((_, name, _)) = &host_action
            && name == "launcher.open"
            && self.launcher_trigger
        {
            self.actions.authorize(name)?;
            self.wakeup
                .as_ref()
                .ok_or("launcher event channel unavailable")?(
                UiEvent::CompanionVisibility(true)
            );
            return Ok(None);
        }
        if let (Some(menu), UiEvent::Command { command, .. }) = (&self.launcher, &event)
            && (self.session.surrounding_text().is_some()
                || host_action
                    .as_ref()
                    .is_some_and(|(_, name, _)| name == "select_app"))
            && matches!(command.as_str(), "up" | "down" | "left" | "right")
        {
            let buttons: Vec<_> = self
                .session
                .view_nodes()
                .iter()
                .filter(|node| node.action.as_deref() == Some("select_app"))
                .collect();
            if let Some(first) = buttons.first() {
                let first_rect = bead::ui::rect(&first.draw);
                let columns = buttons
                    .iter()
                    .filter(|node| (bead::ui::rect(&node.draw).y - first_rect.y).abs() < 0.5)
                    .count()
                    .max(1);
                let pitch = buttons
                    .iter()
                    .map(|node| f64::from(bead::ui::rect(&node.draw).y - first_rect.y))
                    .filter(|distance| *distance > 0.5)
                    .min_by(f64::total_cmp)
                    .unwrap_or(f64::from(first_rect.h));
                let viewport_height = self
                    .session
                    .view_nodes()
                    .iter()
                    .find(|node| node.scroll_store.as_deref() == Some("launcher.scroll.y"))
                    .map(|node| bead::ui::rect(&node.draw).h)
                    .unwrap_or_else(|| first.clip.map_or(first_rect.h, |clip| clip.h));
                let geometry = crate::launcher::NavigationGeometry {
                    columns,
                    pitch,
                    row_height: f64::from(first_rect.h),
                    viewport_height: f64::from(viewport_height),
                };
                if let Some(changes) = menu.navigate(&self.session.stores, command, geometry) {
                    let scene = self.session.update(changes)?;
                    let scene = self.finish_scene(scene, false)?;
                    if bead::core::profile::enabled() {
                        println!("MENU_NAV_DONE changed={}", scene.is_some());
                    }
                    return Ok(scene);
                }
            }
        }
        let launcher_id = if let Some((_, name, bead::core::Value::Text(id))) = &host_action {
            (name == "select_app").then(|| id.clone())
        } else if matches!(&event,UiEvent::Command{command,..} if command=="enter")
            && self.session.surrounding_text().is_some()
        {
            match self.session.stores.get("launcher.selection") {
                Some(bead::core::Value::Text(id)) => Some(id.clone()),
                _ => None,
            }
        } else {
            None
        };
        if let (Some(menu), Some(id)) = (&self.launcher, launcher_id) {
            self.actions.authorize("select_app")?;
            let changes = match menu.activate(&id) {
                Ok(result) => crate::launcher::activation_changes(result),
                Err(error) => std::collections::BTreeMap::from([(
                    "launcher.error".into(),
                    bead::core::Value::Text(error.message),
                )]),
            };
            let scene = self.session.update(changes)?;
            return self.finish_scene(scene, false);
        }
        if let Some((_, name, arg)) = host_action.as_ref()
            && name.starts_with("notifications.")
        {
            self.actions.authorize(name)?;
            self.notifications
                .as_ref()
                .ok_or("notification server unavailable")?
                .action(name, arg)?;
            return Ok(None);
        }
        if let Some((id, name, arg)) = host_action.as_ref()
            && name.starts_with("services.")
        {
            let delta = if let UiEvent::Scroll { delta, .. } = &event {
                Some(*delta)
            } else {
                None
            };
            if delta.is_none() || name == "services.audio" {
                self.desktop_action(
                    name,
                    arg.clone(),
                    delta,
                    matches!(event, UiEvent::ContextMenu { .. }),
                )?;
            }
            if !self.bar {
                self.pending_focus = Some(id.clone());
            }
            event = UiEvent::Desktop(
                self.desktop
                    .as_ref()
                    .ok_or("desktop unavailable after action")?
                    .snapshot(),
            );
        }
        if let Some((id, name, arg)) = host_action
            && name.starts_with("backend.")
            && !matches!(event, UiEvent::Scroll { .. } | UiEvent::ContextMenu { .. })
        {
            self.backend_action(&name, arg)?;
            if !self.bar {
                let scene = self.session.focus_id(&id)?;
                return self.finish_scene(scene, false);
            }
            event = UiEvent::Relayout;
        }
        let reload = matches!(event, UiEvent::Reload);
        if reload {
            self.fonts.clear();
        }
        let scene = match event {
            UiEvent::Model(values) => {
                if self.launcher.is_some() {
                    self.session.update(values)?
                } else {
                    self.session.update_model(values)?
                }
            }
            UiEvent::Desktop(snapshot) => {
                if self.notification_primary
                    && let Some(owner) = &self.notifications
                {
                    owner.audio_changed(&snapshot.audio);
                }
                let mut values = crate::desktop::values(&snapshot, &self.menu_item);
                if let Some(id) = self.pending_focus.take() {
                    values.insert("bead.focused".into(), bead::core::Value::Text(id));
                }
                if self.notification_primary {
                    self.session.update_model(values)?
                } else {
                    self.session.update(values)?
                }
            }
            UiEvent::Backend(snapshot) => {
                println!(
                    "BACKEND_UPDATE backend={} connected={} workspaces={} windows={}",
                    snapshot.backend,
                    snapshot.connected,
                    snapshot.workspaces.len(),
                    snapshot.windows.len()
                );
                self.session.update_model(backend_values(&snapshot)?)?
            }
            UiEvent::CompanionModel(_) => {
                return Err("companion model reached primary script".into());
            }
            UiEvent::CompanionVisibility(_) | UiEvent::CompanionSelect { .. } => {
                return Err("companion visibility reached script".into());
            }
            UiEvent::Panel(_, _) => return Err("panel envelope reached script".into()),
            UiEvent::PanelVisibility(open) => {
                self.session.update(std::collections::BTreeMap::from([(
                    "panel.open".into(),
                    bead::core::Value::Bool(open),
                )]))?
            }
            UiEvent::Fatal(error) => return Err(error.into()),
            UiEvent::Worker(_) => return Err("worker packet cannot reach script executor".into()),
            UiEvent::DebugScriptStall(ms) => {
                let clock = || crate::backends::clock_ns(nix::libc::CLOCK_MONOTONIC);
                println!("DEBUG_SCRIPT_STALL begin_ns={} duration_ms={ms}", clock());
                std::thread::sleep(std::time::Duration::from_millis(ms));
                println!("DEBUG_SCRIPT_STALL end_ns={}", clock());
                None
            }
            UiEvent::ClipboardText(text) => self.session.key(&text, false)?,
            UiEvent::ClipboardSet(_) | UiEvent::ClipboardRead => {
                return Err("clipboard transport event reached script".into());
            }
            UiEvent::Remote(request) => {
                let call = &request.call;
                let validation = if call.target == "bead.osd" {
                    call.validate()
                        .map_err(|e| bead::services::ipc::Reply::error(call.id, "invalid-call", e))
                } else {
                    crate::commands::validate(call)
                };
                let reply = match validation {
                    Err(reply) => reply,
                    Ok(()) => match (call.target.as_str(), call.method.as_str()) {
                        ("bead.osd", "show") => match self
                            .actions
                            .authorize("notifications.feedback")
                            .map_err(|_| "notification feedback capability denied")
                            .and_then(|_| {
                                self.notifications
                                    .as_ref()
                                    .ok_or("notification server unavailable")
                                    .and_then(|owner| owner.feedback(&call.args))
                            }) {
                            Ok(value) => bead::services::ipc::Reply::ok(call.id, value),
                            Err(e) => bead::services::ipc::Reply::error(call.id, "osd", e),
                        },
                        ("notifs", _) => match self
                            .notifications
                            .as_ref()
                            .ok_or("notification server unavailable")
                            .and_then(|owner| owner.command(call))
                        {
                            Ok(value) => bead::services::ipc::Reply::ok(call.id, value),
                            Err(e) => {
                                bead::services::ipc::Reply::error(call.id, "notifications", e)
                            }
                        },
                        ("drawers", "toggle")
                            if call.args.first().and_then(serde_json::Value::as_str)
                                == Some("dashboard")
                                && self.session.stores.get("bar.popouts")
                                    == Some(&bead::core::Value::Bool(true)) =>
                        {
                            self.actions.authorize("view.open")?;
                            let wakeup = self
                                .wakeup
                                .as_ref()
                                .ok_or("dashboard event channel unavailable")?;
                            if request.context.panel_open
                                && request
                                    .context
                                    .panel_view
                                    .as_deref()
                                    .and_then(crate::dashboard::Page::from_view)
                                    .is_some()
                            {
                                wakeup(UiEvent::CompanionVisibility(false));
                            } else {
                                wakeup(UiEvent::CompanionSelect {
                                    view: "dashboard-today".into(),
                                    output: call.output.clone(),
                                    toggle: false,
                                    center: None,
                                });
                            }
                            bead::services::ipc::Reply::ok(call.id, serde_json::Value::Null)
                        }
                        ("nacre", "launcher") if self.launcher_trigger => {
                            let values = std::collections::BTreeMap::from([
                                (
                                    "launcher.search.text".into(),
                                    bead::core::Value::Text(
                                        call.args
                                            .first()
                                            .and_then(serde_json::Value::as_str)
                                            .unwrap_or("")
                                            .into(),
                                    ),
                                ),
                                (
                                    "launcher.mode".into(),
                                    bead::core::Value::Text("apps".into()),
                                ),
                                (
                                    "launcher.route".into(),
                                    bead::core::Value::Text("apps".into()),
                                ),
                                (
                                    "launcher.selection".into(),
                                    bead::core::Value::Text(String::new()),
                                ),
                                ("launcher.scroll.y".into(), bead::core::Value::Number(0.)),
                            ]);
                            self.actions.authorize("launcher.open")?;
                            *self
                                .launcher_memory
                                .as_ref()
                                .ok_or("launcher view state unavailable")?
                                .lock()
                                .map_err(|_| "launcher view state unavailable")? = values.clone();
                            let wakeup = self
                                .wakeup
                                .as_ref()
                                .ok_or("launcher event channel unavailable")?;
                            wakeup(UiEvent::CompanionVisibility(true));
                            wakeup(UiEvent::CompanionModel(values));
                            bead::services::ipc::Reply::ok(call.id, serde_json::Value::Null)
                        }
                        ("nacre", "state") => {
                            let snapshot = self.backend.as_ref().map(|b| b.snapshot());
                            let view = self
                                .launcher_memory
                                .as_ref()
                                .and_then(|m| m.lock().ok().map(|m| m.clone()))
                                .unwrap_or_default();
                            let view_text = |key: &str| match view.get(key) {
                                Some(bead::core::Value::Text(s)) => s.as_str(),
                                _ => "",
                            };
                            let active_view = request.context.panel_view.as_deref().unwrap_or(
                                if self.launcher_trigger {
                                    "launcher"
                                } else {
                                    "sound"
                                },
                            );
                            let dashboard_page = crate::dashboard::Page::from_view(active_view);
                            let launcher_open =
                                request.context.panel_open && active_view == "launcher";
                            let edge_menu = if request.context.panel_open
                                && active_view != "launcher"
                                && dashboard_page.is_none()
                            {
                                active_view
                            } else {
                                ""
                            };
                            let core_views = self.session.stores.get("bar.popouts")
                                == Some(&bead::core::Value::Bool(true));
                            let state = serde_json::json!({"active":snapshot.as_ref().and_then(|s|s.workspaces.iter().find(|w|w.focused||w.active)).map(|w|w.index).unwrap_or(0),"workspaces":snapshot.as_ref().map(|s|s.workspaces.len()).unwrap_or(0),"dashboard":request.context.panel_open && dashboard_page.is_some(),"edgeMenu":edge_menu,"clickEdgeMenus":false,"tab":dashboard_page.map(|page| page.tab()).unwrap_or(0),"launcher":launcher_open,"session":false,"osd":false,"controlSection":"home","feedback":false,"hidden":false,"left":false,"leftPinned":false,"query":view_text("launcher.search.text"),"launcherMode":if self.launcher_trigger {view_text("launcher.mode")}else{"apps"},"launcherRoute":if self.launcher_trigger {view_text("launcher.route")}else{"home"},"reveal":false,"notificationsSuppressed":false,"bead":{"protocol":1,"available":if request.context.panel_configured {if core_views {vec!["bar","launcher","sound-output","calendar","network-status","bluetooth-status","battery-status","dashboard"]}else if self.launcher_trigger {vec!["bar","launcher"]}else{vec!["bar","sound-output"]}}else{vec!["bar"]},"panelOpen":request.context.panel_open}});
                            #[cfg(all(feature = "trace", target_os = "linux", target_env = "gnu"))]
                            let state = {
                                let mut state = state;
                                // SAFETY: read-only by-value allocator accounting.
                                let heap = unsafe { nix::libc::mallinfo2() };
                                state["bead"]["heap"] = serde_json::json!({"live":heap.uordblks,"free":heap.fordblks,"mapped":heap.hblkhd});
                                state
                            };
                            bead::services::ipc::Reply::ok(
                                call.id,
                                serde_json::Value::String(state.to_string()),
                            )
                        }
                        ("compositorState", "state") => {
                            if let Some(backend) = &self.backend {
                                bead::services::ipc::Reply::ok(
                                    call.id,
                                    serde_json::Value::String(serde_json::to_string(
                                        &*backend.snapshot(),
                                    )?),
                                )
                            } else {
                                bead::services::ipc::Reply::error(
                                    call.id,
                                    "unavailable",
                                    "compositor not configured",
                                )
                            }
                        }
                        (_, "state") if call.target.starts_with("windowTitle-") => {
                            bead::services::ipc::Reply::ok(
                                call.id,
                                serde_json::Value::String(serde_json::to_string(
                                    &serde_json::json!({"title":self.backend.as_ref().map(|b|b.snapshot()).and_then(|s|s.focused_window().map(|w|w.title.clone())).unwrap_or_default(),"retained":false}),
                                )?),
                            )
                        }
                        _ => bead::services::ipc::Reply::error(
                            call.id,
                            "unavailable",
                            "compatibility route belongs to a later native feature",
                        ),
                    },
                };
                let _ = request.reply.send(reply);
                None
            }
            UiEvent::Reload => Some(self.session.reload()?),
            UiEvent::Relayout => self.session.relayout()?,
            UiEvent::Accessibility {
                target,
                action,
                value,
            } => self.session.accessibility_action(target, &action, value)?,
            UiEvent::Hover { x, y } => self.session.hover(x, y)?,
            UiEvent::Drag { x, y } => self.session.drag(x, y)?,
            UiEvent::Ime {
                preedit,
                commit,
                before,
                after,
            } => self.session.ime(preedit, commit, before, after)?,
            UiEvent::Command { command, shift } => match command.as_str() {
                "clipboard-copy" | "clipboard-cut" => {
                    if let Some(text) = self.session.selected_text() {
                        self.wakeup
                            .as_ref()
                            .ok_or("clipboard event channel unavailable")?(
                            UiEvent::ClipboardSet(text),
                        );
                    }
                    if command == "clipboard-cut" {
                        self.session.key("", false)?
                    } else {
                        None
                    }
                }
                "clipboard-paste" => {
                    self.wakeup
                        .as_ref()
                        .ok_or("clipboard event channel unavailable")?(
                        UiEvent::ClipboardRead
                    );
                    None
                }
                "escape" if self.session.stores.get("panel.open").is_some() => {
                    self.session.update(std::collections::BTreeMap::from([(
                        "panel.open".into(),
                        bead::core::Value::Bool(false),
                    )]))?
                }
                "tab" => self.session.focus_next(shift)?,
                "enter" => self.session.activate_focused()?,
                "up" | "down" => self.session.focus_next(command == "up")?,
                "left" | "right" if self.session.focused_range() => {
                    self.session.step_range(command == "left")?
                }
                "left" | "right" if self.session.surrounding_text().is_none() => {
                    self.session.focus_next(command == "left")?
                }
                key => {
                    let key = match key {
                        "left" => bead::ui::input::EditKey::Left,
                        "right" => bead::ui::input::EditKey::Right,
                        "home" => bead::ui::input::EditKey::Home,
                        "end" => bead::ui::input::EditKey::End,
                        "delete" => bead::ui::input::EditKey::Delete,
                        "select-all" => bead::ui::input::EditKey::SelectAll,
                        _ => return Err("unsupported input command".into()),
                    };
                    self.session.edit_key(key, shift)?
                }
            },
            UiEvent::WatchError(error) => return Err(error.into()),
            UiEvent::ContextMenu { .. } => None,
            UiEvent::Click { x, y } => self.session.click(x, y)?,
            UiEvent::Scroll { x, y, delta } => self.session.scroll(x, y, delta)?,
            UiEvent::Key { text, backspace } => self.session.key(&text, backspace)?,
        };
        self.finish_scene(scene, reload)
    }
    fn validate(&self) -> Result<(), Box<dyn Error>> {
        self.session.check_sources()?;
        Ok(())
    }
    fn resource_counts(&self) -> String {
        let mut counts = self.session.resource_counts();
        if std::env::var_os("BEAD_SERVICE_DIAGNOSTICS").is_some() {
            if let Some(desktop) = &self.desktop {
                let (tasks, retained) = desktop.bus_task_counts();
                counts.push_str(&format!(
                    " desktop_subscriptions={} desktop_tasks={} desktop_retained_tasks={}",
                    desktop.subscription_count(),
                    tasks,
                    retained
                ));
            }
            #[cfg(all(feature = "trace", target_os = "linux", target_env = "gnu"))]
            {
                // SAFETY: glibc returns a by-value read-only snapshot of its allocator accounting.
                let heap = unsafe { nix::libc::mallinfo2() };
                counts.push_str(&format!(
                    " heap_live={} heap_free={} heap_mapped={}",
                    heap.uordblks, heap.fordblks, heap.hblkhd
                ));
            }
        }
        counts
    }
    fn state_summary(&self) -> String {
        if !self.trace {
            return String::new();
        }
        self.session.state_summary()
    }
    fn focused_dismiss(&self) -> bool {
        self.session.stores.get("panel.open").is_some()
            && self
                .session
                .focused_action()
                .is_some_and(|(_, name, _)| name == "close")
    }
    fn presented(&mut self, revision: u64) {
        if self.session.presented(revision).is_err() {
            eprintln!("RECOVERY_ERROR code=save_bundle");
        }
    }
    fn commit(&mut self) -> bool {
        if let Some(scene) = self.pending.take() {
            self.session.commit(scene);
            if self.launcher.is_some()
                && let Some(memory) = &self.launcher_memory
                && let Ok(mut saved) = memory.lock()
            {
                *saved = crate::launcher::remember(&self.session.stores);
            }
            if let Some(file) = &mut self.state_file
                && let Err(error) = file.save(&self.session.stores)
            {
                eprintln!("STATE_ERROR save: {error}");
            }
        }
        true
    }
    fn error_scene(&mut self, error: String) -> Scene {
        self.pending = None;
        let mut scene = self.session.reject(error);
        if scene.font_files.is_empty()
            && let Ok(files) = crate::font_sources::select(&scene)
        {
            scene.font_files = files;
        }
        scene
    }
}

fn backend_values(
    snapshot: &bead::backends::Snapshot,
) -> Result<std::collections::BTreeMap<String, bead::core::Value>, Box<dyn Error>> {
    use bead::core::Value;
    let mut values = std::collections::BTreeMap::from([
        ("bead.connected".into(), Value::Bool(snapshot.connected)),
        (
            "bead.focused_title".into(),
            Value::Text(
                snapshot
                    .focused_window()
                    .map_or("", |w| w.title.as_str())
                    .into(),
            ),
        ),
        (
            "bead.outputs".into(),
            Value::list(
                snapshot
                    .outputs
                    .iter()
                    .map(serde_json::to_value)
                    .collect::<Result<Vec<_>, _>>()?,
            ),
        ),
        (
            "bead.workspaces".into(),
            Value::list(
                snapshot
                    .workspaces
                    .iter()
                    .map(serde_json::to_value)
                    .collect::<Result<Vec<_>, _>>()?,
            ),
        ),
        (
            "bead.windows".into(),
            Value::list(
                snapshot
                    .windows
                    .iter()
                    .map(serde_json::to_value)
                    .collect::<Result<Vec<_>, _>>()?,
            ),
        ),
    ]);
    for (name, supported) in &snapshot.capabilities.features {
        values.insert(format!("bead.cap.{name}"), Value::Bool(*supported));
    }
    Ok(values)
}

/// Exercises actual bar hit testing and native service routes on explicit proof buses.
pub fn private_service_proof() -> Result<(), Box<dyn Error>> {
    if std::env::var("BEAD_PRIVATE_SERVICES_TEST").as_deref() != Ok("1") {
        return Err("requires private proof environment".into());
    }
    let palette = bead::render::Palette::read(std::path::Path::new("shell/palette.json"))?;
    let fallback = Scene {
        opacity: 1.,
        motion: None,
        resource_revision: 0,
        asset_data: Default::default(),
        clips: Default::default(),
        layers: Vec::new(),
        prepared: Default::default(),
        font_files: vec![],
        width: 1920.,
        slide: None,
        primitives: vec![],
        offset: (0., 0.),
        bounds: bead::render::Rect::new(0., 0., 1920., 50.),
    };
    let mut session = bead::ui::Session::new(
        "shell".into(),
        "bar/main.luau".into(),
        "shell/palette.json".into(),
        palette,
        fallback,
    );
    session.set_viewport(1920, 1200);
    session.stores = session
        .stores
        .reconcile(&std::collections::BTreeMap::from([
            (
                "bead.focused_title".into(),
                bead::core::Value::Text("Fixture title".into()),
            ),
            ("bead.workspaces".into(), bead::core::Value::list(vec![])),
        ]))?;
    let mut runtime = Runtime::new(session).with_desktop()?;
    let desktop = runtime
        .desktop
        .as_ref()
        .ok_or("proof desktop unavailable")?
        .clone();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while desktop.snapshot().audio.volume.is_none()
        || desktop.snapshot().tray.iter().all(|t| t.menu.is_empty())
    {
        if std::time::Instant::now() > deadline {
            return Err("proof services timeout".into());
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    runtime
        .session
        .update(crate::desktop::values(&desktop.snapshot(), ""))?;
    runtime.stage(UiEvent::Reload)?;
    runtime.commit();
    fn position(runtime: &Runtime, action: &str) -> Result<(f32, f32), Box<dyn Error>> {
        let node = runtime
            .session
            .view_nodes()
            .iter()
            .find(|n| n.action.as_deref() == Some(action))
            .ok_or("action node missing")?;
        let bead::render::Primitive::Rounded { rect, .. } = node.draw else {
            return Err("action bounds missing".into());
        };
        Ok((rect.x + rect.w / 2., rect.y + rect.h / 2.))
    }
    let (x, y) = position(&runtime, "services.audio")?;
    let previous = desktop
        .snapshot()
        .audio
        .muted
        .ok_or("proof mute unavailable")?;
    runtime.stage(UiEvent::Click { x, y })?;
    runtime.commit();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
    while desktop.snapshot().audio.muted == Some(previous) {
        if std::time::Instant::now() > deadline {
            return Err("click mute not observed".into());
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let before = desktop
        .snapshot()
        .audio
        .volume
        .ok_or("proof volume unavailable")?;
    runtime.stage(UiEvent::Scroll { x, y, delta: -1. })?;
    runtime.commit();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
    while (desktop
        .snapshot()
        .audio
        .volume
        .ok_or("proof volume unavailable")?
        - before)
        .abs()
        < 0.01
    {
        if std::time::Instant::now() > deadline {
            return Err("scroll volume not observed".into());
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    runtime.stage(UiEvent::Desktop(desktop.snapshot()))?;
    runtime.commit();
    let (x, y) = position(&runtime, "services.tray")?;
    runtime.stage(UiEvent::Click { x, y })?;
    runtime.commit();
    let item_menu = desktop.snapshot().tray[0].item_is_menu;
    if !item_menu {
        runtime.stage(UiEvent::ContextMenu { x, y })?;
        runtime.commit();
    }
    let (x, y) = position(&runtime, "services.menu")?;
    runtime.stage(UiEvent::Click { x, y })?;
    runtime.commit();
    assert!(runtime.menu_item.is_empty());
    println!(
        "BAR_SERVICE_ROUTES click_mute=true scroll_volume=true tray_activate_or_item_menu=true right_click_prepare=true menu_event=true item_is_menu={item_menu}"
    );
    Ok(())
}

#[cfg(test)]
mod bluetooth_power_tests;
#[cfg(test)]
mod wifi_runtime_tests;
