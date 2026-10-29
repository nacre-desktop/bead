// SPDX-License-Identifier: GPL-3.0-only
//! On-demand app mode (B0052, ADR 0005). The same binary runs as `nacre-shell --app NAME [ARG]`:
//! one window or one edge panel, its own daemon connection as a plain client (never a second UI
//! provider), no bar and no primary surface. The process ends when its window or panel closes.
//!
//! Two halves share this file: [`Host`] runs in the always-on shell (find the running app and
//! forward the request, or start it) and [`run`] is the app process. The shell holds no app
//! state beyond one path; everything an app allocates dies with its process.
use crate::{
    chat, colors as colors_app, nacre_source, placement, popouts, prefs, prompt, settings, welcome,
    widgets,
};
use bead::{
    render::Palette,
    services::ipc::{self, Call, Reply},
    wayland::{Options, Panels},
};
use serde_json::{Value, json};
use std::os::unix::fs::FileTypeExt;
use std::{
    collections::BTreeMap,
    error::Error,
    path::{Path, PathBuf},
    process::Stdio,
    sync::{Arc, OnceLock},
    time::Duration,
};

/// Apps the shell can start. A third-party app adds a branch to [`run`] and a route in
/// `commands::map_views`; the launch, single-instance and exit rules are shared.
pub const APPS: [&str; 6] = ["settings", "chat", "colors", "welcome", "prompt", "widgets"];

/// Topics every app follows: Nacre's text catalog and the settings that shape the look.
const TOPICS: [&str; 6] = [
    "daemon",
    "i18n",
    prefs::DESKTOP,
    prefs::DASHBOARD,
    prefs::PROFILE,
    prefs::LAUNCHER_LAYOUT,
];

pub struct Host {
    exe: PathBuf,
    /// Effective shell command socket (`nacre clipboard` and other routed commands).
    command: PathBuf,
    /// Parent shell command socket for app-originated events, when this host is itself an app.
    parent: Option<PathBuf>,
    app_name: Option<String>,
    dir: PathBuf,
    daemon: Option<PathBuf>,
    palette: PathBuf,
}
static HOST: OnceLock<Host> = OnceLock::new();
/// The presentation palette the shell last committed (B0056): an app starts with it, so it wears the wallpaper's colours
/// and not the bundle's static ones.
static LIVE: std::sync::Mutex<Option<BTreeMap<String, [u8; 4]>>> = std::sync::Mutex::new(None);
pub fn configured() -> bool {
    HOST.get().is_some()
}
pub fn set_live_palette(palette: &BTreeMap<String, [u8; 4]>) {
    *LIVE.lock().unwrap_or_else(|p| p.into_inner()) = Some(palette.clone());
}
/// Orient format 1 file of the live palette beside the app sockets (owner-only directory), if one has arrived.
fn live_palette_file(dir: &Path) -> Option<PathBuf> {
    let live = LIVE.lock().unwrap_or_else(|p| p.into_inner()).clone()?;
    let colours: serde_json::Map<String, Value> = live
        .iter()
        .map(|(role, c)| {
            (
                role.clone(),
                json!(format!("{:02x}{:02x}{:02x}", c[0], c[1], c[2])),
            )
        })
        .collect();
    let path = dir.join("palette-live.json");
    std::fs::write(
        &path,
        json!({"formatVersion": 1, "colours": colours}).to_string(),
    )
    .ok()?;
    Some(path)
}

/// Called once by the shell: app sockets live beside its command socket (owner-only directory).
pub fn configure(command_socket: &Path, daemon: Option<PathBuf>, palette: PathBuf) {
    configure_host(command_socket, None, None, daemon, palette);
}

/// Called by an app child with its own panel socket and the parent shell socket.
fn configure_host(
    command_socket: &Path,
    parent: Option<PathBuf>,
    app_name: Option<String>,
    daemon: Option<PathBuf>,
    palette: PathBuf,
) {
    let (Some(dir), Ok(exe)) = (command_socket.parent(), std::env::current_exe()) else {
        return;
    };
    let _ = HOST.set(Host {
        exe,
        command: parent.clone().unwrap_or_else(|| command_socket.into()),
        parent,
        app_name,
        dir: dir.into(),
        daemon,
        palette,
    });
}
fn configure_child(
    command_socket: &Path,
    parent: Option<PathBuf>,
    app_name: String,
    daemon: Option<PathBuf>,
    palette: PathBuf,
) {
    configure_host(command_socket, parent, Some(app_name), daemon, palette);
}
#[derive(Debug, PartialEq, Eq)]
struct WidgetRuntimePaths {
    directory: PathBuf,
    daemon: PathBuf,
    command: PathBuf,
    parent: PathBuf,
    palette: PathBuf,
}
fn widget_runtime_paths(
    runtime_dir: Option<&Path>,
    daemon_env: Option<&Path>,
) -> Option<WidgetRuntimePaths> {
    let runtime_dir = runtime_dir.filter(|path| path.is_absolute());
    let daemon_env = daemon_env.filter(|path| path.is_absolute());
    let directory = runtime_dir
        .map(|path| path.join("nacre"))
        .or_else(|| daemon_env.and_then(Path::parent).map(Path::to_path_buf))?;
    Some(WidgetRuntimePaths {
        daemon: daemon_env
            .map(Path::to_path_buf)
            .unwrap_or_else(|| directory.join("daemon.sock")),
        command: directory.join("app-widgets.sock"),
        parent: directory.join("bead-command.sock"),
        palette: directory.join("palette-live.json"),
        directory,
    })
}
fn is_socket(path: &Path) -> bool {
    std::fs::symlink_metadata(path).is_ok_and(|metadata| metadata.file_type().is_socket())
}
/// Fill only absent paths for the no-argument Widgets entry. Other app modes and explicit/private
/// app invocations keep their caller-owned sockets, palette, output and daemon settings.
fn apply_widget_runtime_defaults(
    name: &str,
    daemon: &mut Option<PathBuf>,
    command: &mut Option<PathBuf>,
    parent: &mut Option<PathBuf>,
    palette: &mut PathBuf,
    palette_explicit: bool,
    paths: Option<WidgetRuntimePaths>,
) -> bool {
    if name != "widgets" {
        return false;
    }
    let Some(paths) = paths else { return false };
    if daemon.is_none() && is_socket(&paths.daemon) {
        *daemon = Some(paths.daemon.clone());
    }
    if paths.directory.is_dir() {
        if command.is_none() {
            *command = Some(paths.command.clone());
        }
        if parent.is_none() && is_socket(&paths.parent) {
            *parent = Some(paths.parent.clone());
        }
    }
    if !palette_explicit && paths.palette.is_file() {
        *palette = paths.palette;
    }
    true
}
fn open_existing_widget(socket: &Path) -> Result<bool, String> {
    if !is_socket(socket) {
        return Ok(false);
    }
    let call = Call {
        version: 1,
        id: 1,
        target: "bead.window".into(),
        method: "open".into(),
        args: Vec::new(),
        output: None,
    };
    match ipc::call(socket, &call, Duration::from_millis(300)) {
        Ok(reply) => match reply.error {
            Some(error) => Err(error.message),
            None => Ok(true),
        },
        Err(_) => Ok(false),
    }
}
/// Let the running shell supply its current palette and single-instance launch arguments.
fn open_parent_widget(socket: &Path) -> Result<bool, String> {
    if !is_socket(socket) {
        return Ok(false);
    }
    let call = Call {
        version: 1,
        id: 1,
        target: "widgets".into(),
        method: "open".into(),
        args: Vec::new(),
        output: None,
    };
    match ipc::call(socket, &call, Duration::from_millis(300)) {
        Ok(reply) => match reply.error {
            Some(error) => Err(error.message),
            None => Ok(true),
        },
        Err(_) => Ok(false),
    }
}
/// Runs a no-argument `nacre <method>` command through the shell's own socket (a tile that opens the launcher).
pub fn command(method: &str) -> bool {
    let Some(host) = HOST.get() else {
        return false;
    };
    let call = Call {
        version: 1,
        id: 1,
        target: "nacre".into(),
        method: method.into(),
        args: Vec::new(),
        output: None,
    };
    ipc::call(&host.command, &call, Duration::from_millis(800))
        .is_ok_and(|reply| reply.error.is_none())
}
/// Widgets app children use the owner-only parent command socket for the private drag bridge.
pub(crate) fn widget_private_call_timeout(
    method: &str,
    payload: Value,
    timeout: Duration,
) -> Result<(), String> {
    let host = HOST.get().ok_or("app host unavailable")?;
    if !is_widgets_child(host.app_name.as_deref()) {
        return Err("private Widgets relay is available only to the Widgets app".into());
    }
    let parent = host
        .parent
        .as_ref()
        .ok_or("Widgets app has no parent host")?;
    let call = Call {
        version: 1,
        id: 1,
        target: "bead.widgets".into(),
        method: method.into(),
        args: vec![payload],
        output: None,
    };
    let reply = ipc::call(parent, &call, timeout)?;
    if let Some(error) = reply.error {
        return Err(error.message);
    }
    Ok(())
}
fn is_widgets_child(app_name: Option<&str>) -> bool {
    app_name == Some("widgets")
}
/// Send a terminal Widgets drag outcome to the already-running app's private window socket.
fn widget_feedback_at(
    socket: &Path,
    pid: u32,
    generation: u64,
    epoch: u64,
    success: bool,
    error: &str,
) -> Result<(), String> {
    let call = Call {
        version: 1,
        id: 1,
        target: "bead.widgets".into(),
        method: "drag-feedback".into(),
        args: vec![
            json!({"app":"nacre-widgets","pid":pid,"generation":generation,"epoch":epoch,
            "success":success,"error":error}),
        ],
        output: None,
    };
    let reply = ipc::call(socket, &call, Duration::from_millis(150))?;
    if let Some(error) = reply.error {
        return Err(error.message);
    }
    Ok(())
}
pub(crate) fn widget_feedback(
    pid: u32,
    generation: u64,
    epoch: u64,
    success: bool,
    error: &str,
) -> Result<(), String> {
    let host = HOST.get().ok_or("app host unavailable")?;
    if host.app_name.is_some() {
        return Err("Widgets feedback is parent-only".into());
    }
    widget_feedback_at(
        &host.socket("widgets"),
        pid,
        generation,
        epoch,
        success,
        error,
    )
}
pub(crate) fn widget_history_feedback(
    pid: u32,
    generation: u64,
    epoch: Option<u64>,
    previous_document: Value,
    previous_revision: u64,
    document: Value,
    revision: u64,
) -> Result<(), String> {
    let host = HOST.get().ok_or("app host unavailable")?;
    if host.app_name.is_some() {
        return Err("Widgets feedback is parent-only".into());
    }
    let mut payload = json!({"app":"nacre-widgets","pid":pid,"generation":generation,
        "kind":"history","success":true,"error":"","previous_document":previous_document,
        "previous_revision":previous_revision,"document":document,"revision":revision});
    if let Some(epoch) = epoch {
        payload["epoch"] = json!(epoch)
    }
    let call = Call {
        version: 1,
        id: 1,
        target: "bead.widgets".into(),
        method: "drag-feedback".into(),
        args: vec![payload],
        output: None,
    };
    let reply = ipc::call(&host.socket("widgets"), &call, Duration::from_millis(150))?;
    if let Some(error) = reply.error {
        return Err(error.message);
    }
    Ok(())
}
/// Open, deep-link or toggle an app for a routed command. Never blocks on the app's startup.
pub fn route(id: u64, name: &str, method: &str, args: &[Value]) -> Reply {
    route_on(id, name, method, args, None)
}
/// B0194: the compositor-focused output, set once by the shell.
static FOCUSED: OnceLock<bead::wayland::FocusedOutput> = OnceLock::new();
pub fn set_focused_output(probe: bead::wayland::FocusedOutput) {
    let _ = FOCUSED.set(probe);
}
/// As [`route`], for an app panel that opens on `output`; none means the focused output
/// (keyboard and IPC), while pointer-driven callers pass the output under the pointer.
pub fn route_on(id: u64, name: &str, method: &str, args: &[Value], output: Option<&str>) -> Reply {
    let focused = output
        .is_none()
        .then(|| FOCUSED.get().and_then(|probe| probe()))
        .flatten();
    let output = output.or(focused.as_deref());
    match HOST.get() {
        Some(host) => host.route_on(id, name, method, args, output),
        None => Reply::error(id, "unavailable", "apps are not configured in this shell"),
    }
}

/// Edge observations and opens share one ordered lane. In particular a leave
/// cannot overtake startup or an earlier enter while the external app is cold.
pub fn panel_request(name: &'static str, method: &'static str, output: &str) {
    enqueue_panel_request(PanelRequest::Panel(name, method, output.to_owned()));
}
enum PanelRequest {
    Panel(&'static str, &'static str, String),
    Visibility(&'static str, bool),
    Describe(&'static str),
}
/// Queue a child panel visibility observation for its parent shell. The UI event path never waits
/// for private IPC; visibility and geometry queries share the panel worker's ordered lane.
pub fn notify_panel_visibility(open: bool) {
    let Some(host) = HOST.get() else { return };
    if host.parent.is_none() {
        return;
    }
    let Some(name) = host.app_name.as_deref() else {
        return;
    };
    if name != "chat" {
        return;
    }
    enqueue_panel_request(PanelRequest::Visibility(name, open));
}
/// Ask the shell worker to refresh geometry without delaying its private IPC reply.
pub fn refresh_panel_description(name: &'static str) {
    if name == "chat" {
        enqueue_panel_request(PanelRequest::Describe(name));
    }
}
fn enqueue_panel_request(request: PanelRequest) {
    let Some(host) = HOST.get() else { return };
    static REQUESTS: OnceLock<std::sync::mpsc::Sender<PanelRequest>> = OnceLock::new();
    let sender = REQUESTS.get_or_init(|| {
        let (sender, receiver) = std::sync::mpsc::channel();
        let _ = std::thread::Builder::new()
            .name("panel-requests".into())
            .stack_size(128 * 1024)
            .spawn(move || panel_requests(host, receiver));
        sender
    });
    if sender.send(request).is_err() {
        eprintln!("EXTERNAL_PANEL_ERROR request lane unavailable");
    }
}
fn panel_requests(host: &Host, receiver: std::sync::mpsc::Receiver<PanelRequest>) {
    for request in receiver {
        match request {
            PanelRequest::Panel(name, method, output) => {
                let reply = host.route_on(0, name, method, &[], Some(&output));
                if method == "hover-open" && reply.error.is_none() {
                    // The socket can precede the panel worker. Keep later observations
                    // queued until that worker can actually receive pointer events.
                    let _ = wait_panel_ready(&host.socket(name));
                }
            }
            PanelRequest::Visibility(name, open) => {
                let Some(parent) = &host.parent else { continue };
                if let Err(error) = send_panel_visibility(parent, name, open) {
                    eprintln!("EXTERNAL_PANEL_ERROR visibility IPC failed: {error}");
                }
            }
            PanelRequest::Describe(name) => describe_panel_state(name, &host.socket(name)),
        }
    }
}
fn send_panel_visibility(parent: &Path, name: &str, open: bool) -> Result<Reply, String> {
    let call = Call {
        version: 1,
        id: 1,
        target: "bead.apps".into(),
        method: "panel-visibility".into(),
        args: vec![json!(name), json!(open)],
        output: None,
    };
    ipc::call(parent, &call, Duration::from_millis(800))
}
fn wait_panel_ready(socket: &Path) -> bool {
    let call = Call {
        version: 1,
        id: 1,
        target: "bead.panel".into(),
        method: "state".into(),
        args: vec![],
        output: None,
    };
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while std::time::Instant::now() < deadline {
        if let Ok(reply) = ipc::call(socket, &call, Duration::from_millis(300))
            && let Some(state) = reply.result
            && state["target_open"] == true
            && state["prepared"] == true
        {
            return true;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    eprintln!("EXTERNAL_PANEL_ERROR panel startup did not become ready");
    false
}

/// The selector prompt route (`menuPrompt open|cancel ID`).
pub fn prompt(id: u64, method: &str, ident: &str) -> Reply {
    match HOST.get() {
        Some(host) => host.prompt(id, method, ident),
        None => Reply::error(id, "unavailable", "apps are not configured in this shell"),
    }
}

impl Host {
    pub fn socket(&self, name: &str) -> PathBuf {
        self.dir.join(format!("app-{name}.sock"))
    }
    pub fn route(&self, id: u64, name: &str, method: &str, args: &[Value]) -> Reply {
        self.route_on(id, name, method, args, None)
    }
    pub fn route_on(
        &self,
        id: u64,
        name: &str,
        method: &str,
        args: &[Value],
        output: Option<&str>,
    ) -> Reply {
        if !APPS.contains(&name) {
            return Reply::error(id, "unknown_app", "no such app");
        }
        // Windows take the open call (a second open deep-links the running one); panels the
        // given panel method (`toggle` closes the running sidebar).
        let (target, method) = if matches!(name, "settings" | "colors" | "welcome" | "widgets") {
            ("bead.window", "open")
        } else {
            ("bead.panel", method)
        };
        let call = Call {
            version: 1,
            id: 1,
            target: target.into(),
            method: method.into(),
            args: args.to_vec(),
            output: output.map(str::to_owned),
        };
        match ipc::call(&self.socket(name), &call, Duration::from_millis(800)) {
            Ok(reply) if reply.error.is_none() => return Reply::ok(id, Value::Null),
            Ok(reply) => {
                let error = reply.error.expect("checked");
                return Reply::error(id, &error.code, error.message);
            }
            Err(_) => {}
        }
        // A lip observation never starts an app (the dwell owns startup).
        if matches!(method, "hover-enter" | "hover-leave") {
            return Reply::ok(id, Value::Null);
        }
        let mut spawn_args = args.to_vec();
        if method == "hover-open" {
            spawn_args.push(json!("--hover-open"));
        }
        if name == "chat" && method == "pearl-open" {
            spawn_args.push(json!("--pearl"));
        }
        match self.spawn(name, &spawn_args, output) {
            Ok(()) => Reply::ok(id, Value::Null),
            Err(error) => Reply::error(id, "spawn", error),
        }
    }
    /// B0093 selector prompt. `open` starts the prompt app for a request. One that is still up is the previous
    /// prompt closing (a chained `pick` then `input`) or a stale one of a killed tool (the tool's own lock makes
    /// two live prompts impossible): it is closed and given a second to leave before the new one starts.
    /// `cancel` closes the prompt that is up and never starts one.
    pub fn prompt(&self, id: u64, method: &str, ident: &str) -> Reply {
        let call = |method: &str| Call {
            version: 1,
            id: 1,
            target: "bead.panel".into(),
            method: method.into(),
            args: vec![],
            output: None,
        };
        let socket = self.socket("prompt");
        let up = || ipc::call(&socket, &call("state"), Duration::from_millis(300)).is_ok();
        if method == "cancel" {
            if up() {
                let _ = ipc::call(&socket, &call("close"), Duration::from_millis(800));
            }
            return Reply::ok(id, Value::Null);
        }
        if up() {
            let _ = ipc::call(&socket, &call("close"), Duration::from_millis(800));
            for _ in 0..20 {
                std::thread::sleep(Duration::from_millis(50));
                if !up() {
                    break;
                }
            }
            if up() {
                return Reply::error(id, "busy", "another selector is still open");
            }
        }
        match self.spawn("prompt", &[json!(ident)], None) {
            Ok(()) => Reply::ok(id, Value::Null),
            Err(error) => Reply::error(id, "spawn", error),
        }
    }
    /// Starts the app detached from the shell's stdio. Single-instance is the socket bind: a
    /// second process that loses the race exits at once.
    fn spawn(&self, name: &str, args: &[Value], output: Option<&str>) -> Result<(), String> {
        use std::os::unix::process::CommandExt;
        let mut command = std::process::Command::new(&self.exe);
        command.arg("--app").arg(name);
        for arg in args.iter().filter_map(Value::as_str) {
            command.arg(arg);
        }
        if let Some(output) = output.filter(|_| name == "chat") {
            command.arg("--output").arg(output);
        }
        command
            .arg("--command-socket")
            .arg(self.socket(name))
            .arg("--host-socket")
            .arg(&self.command)
            .arg("--palette")
            .arg(live_palette_file(&self.dir).unwrap_or_else(|| self.palette.clone()));
        if let Some(daemon) = &self.daemon {
            command.arg("--daemon-socket").arg(daemon);
        }
        // Harness only: keep an app's profile output (BEAD_APP_LOG_DIR) instead of discarding it.
        let stdout = std::env::var_os("BEAD_APP_LOG_DIR")
            .and_then(|dir| {
                std::fs::File::create(std::path::Path::new(&dir).join(format!("app-{name}.log")))
                    .ok()
            })
            .map_or_else(Stdio::null, Stdio::from);
        let mut child = command
            .stdin(Stdio::null())
            .stdout(stdout)
            .process_group(0)
            .spawn()
            .map_err(|e| format!("{name}: {e}"))?;
        // Reap it when it exits; the closing app leaves no zombie in the shell. A panel app also
        // describes its body to the frame (B0056) from its own `bead.panel state` reply, and the
        // description goes with the process.
        let socket = (name == "chat").then(|| self.socket(name));
        let app = name.to_owned();
        std::thread::Builder::new()
            .name("bead-app-reap".into())
            .stack_size(256 * 1024)
            .spawn(move || {
                if let Some(socket) = socket {
                    describe_panel(&app, &socket, &mut child);
                }
                let _ = child.wait();
                placement::set_external(&app, None);
            })
            .map(|_| ())
            .map_err(|e| e.to_string())
    }
}

/// Wait (a few seconds at most) for a panel app to open, then tell the frame where its body is.
/// `bead.panel state` reports the panel surface (body plus the 32 px shadow margin on each side).
fn describe_panel(app: &str, socket: &Path, child: &mut std::process::Child) {
    let call = Call {
        version: 1,
        id: 1,
        target: "bead.panel".into(),
        method: "state".into(),
        args: vec![],
        output: None,
    };
    for _ in 0..60 {
        if !matches!(child.try_wait(), Ok(None)) {
            return;
        }
        std::thread::sleep(Duration::from_millis(50));
        let Ok(reply) = ipc::call(socket, &call, Duration::from_millis(300)) else {
            continue;
        };
        let state = reply
            .result
            .as_ref()
            .and_then(Value::as_str)
            .and_then(|t| serde_json::from_str::<Value>(t).ok())
            .or(reply.result);
        let Some(state) = state else { continue };
        if apply_panel_geometry(app, &state) {
            return;
        }
    }
}

/// Bounded state refresh used after a warm panel reopens without new geometry.
fn describe_panel_state(app: &str, socket: &Path) {
    let call = Call {
        version: 1,
        id: 1,
        target: "bead.panel".into(),
        method: "state".into(),
        args: vec![],
        output: None,
    };
    for _ in 0..10 {
        if let Ok(reply) = ipc::call(socket, &call, Duration::from_millis(300)) {
            let state = reply
                .result
                .as_ref()
                .and_then(Value::as_str)
                .and_then(|t| serde_json::from_str::<Value>(t).ok())
                .or(reply.result);
            if let Some(state) = state.as_ref()
                && apply_panel_geometry(app, state)
            {
                return;
            }
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn apply_panel_geometry(app: &str, state: &Value) -> bool {
    let size = state["size"]
        .as_array()
        .and_then(|s| Some((s.first()?.as_f64()?, s.get(1)?.as_f64()?)));
    if let (Some(true), Some((w, h))) = (state["target_open"].as_bool(), size)
        && w > 64.
        && h > 64.
        && w <= f32::MAX as f64
        && h <= f32::MAX as f64
    {
        placement::set_external(
            app,
            Some(placement::External {
                attachment: placement::Attachment::Attached(bead::core::Edge::Left),
                size: ((w - 64.).max(0.) as f32, (h - 64.).max(0.) as f32),
            }),
        );
        true
    } else {
        false
    }
}

/// The app process: `cli` is everything after `--app`.
pub fn run(cli: &[String]) -> Result<(), Box<dyn Error>> {
    let name = cli.first().ok_or("--app needs a name")?.as_str();
    if !APPS.contains(&name) {
        return Err(format!("unknown app: {name}").into());
    }
    let mut args = Vec::new();
    let mut pearl = false;
    let mut daemon = None;
    let mut command_socket = None;
    let mut parent_socket = None;
    let mut palette = PathBuf::from("shell/palette.json");
    let mut palette_explicit = false;
    let mut output = None;
    let mut hover_open = false;
    let mut rest = cli[1..].iter();
    while let Some(arg) = rest.next() {
        let mut value = |what: &str| rest.next().cloned().ok_or(format!("{arg} needs {what}"));
        match arg.as_str() {
            "--pearl" if name == "chat" => pearl = true,
            "--daemon-socket" => daemon = Some(PathBuf::from(value("a path")?)),
            "--command-socket" => command_socket = Some(PathBuf::from(value("a path")?)),
            "--host-socket" => parent_socket = Some(PathBuf::from(value("a path")?)),
            "--palette" => {
                palette = value("a path")?.into();
                palette_explicit = true;
            }
            "--output" => output = Some(value("a name")?),
            "--hover-open" => hover_open = true,
            flag if flag.starts_with("--") => {
                return Err(format!("unknown argument: {flag}").into());
            }
            positional => args.push(json!(positional)),
        }
    }
    if name == "widgets" && cli.len() == 1 {
        let runtime_dir = std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from);
        let daemon_env = std::env::var_os("NACRE_DAEMON_SOCKET").map(PathBuf::from);
        apply_widget_runtime_defaults(
            name,
            &mut daemon,
            &mut command_socket,
            &mut parent_socket,
            &mut palette,
            palette_explicit,
            widget_runtime_paths(runtime_dir.as_deref(), daemon_env.as_deref()),
        );
        if let Some(socket) = command_socket.as_deref() {
            match open_existing_widget(socket) {
                Ok(true) => return Ok(()),
                Ok(false) => {}
                Err(error) => return Err(error.into()),
            }
        }
        if let Some(socket) = parent_socket.as_deref() {
            match open_parent_widget(socket) {
                Ok(true) => return Ok(()),
                Ok(false) => {}
                Err(error) => return Err(error.into()),
            }
        }
    }
    let colors = Palette::read(&palette)
        .or_else(|_| Palette::parse(include_bytes!("../../../shell/palette.json")))?;
    crate::i18n::init();
    // An app that opens another (Welcome's tiles) calls it the way the shell does.
    if let Some(socket) = &command_socket {
        configure_child(
            socket,
            parent_socket.clone(),
            name.to_owned(),
            daemon.clone(),
            palette.clone(),
        );
    }
    // A plain client of the daemon: it follows topics and calls methods, registers nothing.
    let client = daemon
        .map(|socket| {
            let mut topics: Vec<String> = TOPICS.iter().map(|t| t.to_string()).collect();
            if name == "welcome" {
                topics.push(welcome::SHORTCUTS.into());
            }
            if name == "colors" {
                topics.extend(colors_app::TOPICS.iter().map(|t| t.to_string()));
            } else {
                // Every app wears the live palette (B0068): the wallpaper topic carries it.
                topics.push(colors_app::ACTIVE.to_string());
            }
            if name == "widgets" {
                topics.push(crate::widgets::TOPIC.to_owned());
                topics.push("displays".to_owned());
                topics.push("compositor".to_owned());
            }
            let mut config = nacre_source::Config::new(socket, topics);
            config.client = format!("nacre-app-{name}");
            if name == "chat" {
                config.event = Some(Arc::new(chat::route_event));
            }
            nacre_source::Client::start(config)
        })
        .transpose()
        .map_err(|e| e.to_string())?;
    let topic = |client: &nacre_source::Client, topic: &str| {
        client
            .topic(topic)
            .ok()
            .map(|t| Arc::new(t) as Arc<dyn bead::core::source::Source>)
    };
    let _i18n = client.as_ref().and_then(|client| {
        let sink: crate::i18n_source::Sink = Arc::new(bead::ui::i18n::install);
        crate::i18n_source::Service::start(topic(client, "i18n")?, sink).ok()
    });
    // The look follows the owner's settings like the shell's: motion, text style, frame size.
    let panel_gate = crate::shell_control::Gate::new();
    let prefs_gate = panel_gate.clone();
    let (first, first_seen) = std::sync::mpsc::sync_channel::<()>(1);
    let _prefs = client.as_ref().and_then(|client| {
        prefs::Service::start_with(
            topic(client, prefs::DESKTOP)?,
            topic(client, prefs::DASHBOARD)?,
            topic(client, prefs::PROFILE),
            topic(client, prefs::LAUNCHER_LAYOUT),
            Arc::new(move |p: &prefs::Prefs| {
                prefs_gate.apply(p);
                bead::core::animation::set_reduced_motion(p.reduce_motion);
                bead::ui::text_style::set(p.text_style());
                placement::set_sheen(p.frame_sheen);
                placement::Frame::set_current(placement::Frame::from_settings(&p.frame_settings()));
                let _ = first.try_send(());
            }),
        )
        .ok()
    });
    let mut options = Options {
        namespace: format!("nacre-app-{name}"),
        command_socket,
        // A panel app keeps every output eligible and opens its first panel on the one it was
        // started for (B0194); a window app is restricted to it.
        output: output.clone().filter(|_| name != "chat"),
        panel_output: output.filter(|_| name == "chat"),
        interactive: true,
        keyboard_focus: true,
        trace_state: cfg!(feature = "trace")
            && std::env::var("BEAD_TRACE_STATE").is_ok_and(|value| value == "1"),
        app: Some(args.clone()),
        app_as_pearl: pearl,
        ..Options::default()
    };
    let fill = colors.get("surface")?;
    // B0068: the live palette at start and on every change (the file above only covers a start
    // after the shell saw the first palette).
    options.palette_feed = client
        .as_ref()
        .and_then(|client| client.topic(colors_app::ACTIVE).ok())
        .map(|t| crate::wallpaper::Feed::new(t) as Arc<dyn bead::wayland::PaletteFeed>);
    let root = PathBuf::from("shell");
    match name {
        "welcome" => {
            options.window = Some(welcome::window_spec(
                root,
                palette,
                colors,
                client.clone(),
                false,
            )?);
        }
        "settings" => {
            options.window = Some(settings::window_spec(
                root,
                palette,
                colors,
                client.clone(),
                false,
            )?);
        }
        "colors" => {
            options.window = Some(colors_app::window_spec(
                root,
                palette,
                colors,
                client.clone(),
                false,
            )?);
        }
        "widgets" => {
            options.window = Some(widgets::window_spec(
                root,
                palette,
                colors,
                client.clone(),
                false,
            )?);
        }
        "prompt" => {
            let ident = args
                .first()
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned();
            let helper = prompt::helper_binary();
            let request = prompt::Request::read(&helper, &ident).inspect_err(|_| {
                // Answer the waiting tool at once instead of leaving it to its timeout.
                let _ = prompt::respond(
                    &helper,
                    &ident,
                    "",
                    true,
                    &crate::i18n::tr("bead.prompt.unavailable", &[]),
                );
            })?;
            let _ = first_seen.recv_timeout(Duration::from_millis(150));
            let spec = prompt::panel(palette, colors, helper, ident, request, false)?;
            options.panels = Some(Panels::new(
                "prompt".into(),
                BTreeMap::from([("prompt".into(), spec)]),
            )?);
        }
        _ => {
            // The panel's position reads the live frame: give the first settings a moment.
            let _ = first_seen.recv_timeout(Duration::from_millis(150));
            let spec = popouts::chat(
                client.clone().ok_or("the sidebar needs the daemon")?,
                palette,
                colors.clone(),
                Arc::new(std::sync::Mutex::new(BTreeMap::from([
                    (
                        "panel.hover_opened".into(),
                        bead::core::Value::Bool(hover_open),
                    ),
                    (
                        "panel.anchor_hovered".into(),
                        bead::core::Value::Bool(hover_open),
                    ),
                ]))),
                false,
            )?;
            options.pearls = Some(crate::pearls::options(
                crate::state::File::path()
                    .ok()
                    .and_then(|path| path.parent().map(|parent| parent.join("chat-pearls.json"))),
                client.clone(),
            ));
            if parent_socket.is_some() {
                panel_gate.parent_lips(crate::frame_config::load(Path::new("shell")));
            }
            options.control = Some(panel_gate);
            options.panels = Some(Panels::new(
                "chat".into(),
                BTreeMap::from([
                    ("chat".into(), spec),
                    (
                        crate::pearls::GUIDE_VIEW.into(),
                        crate::pearls::guide_spec(&colors)?,
                    ),
                ]),
            )?);
        }
    }
    bead::wayland::run(options, crate::sound::fallback(fill))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::{fs::PermissionsExt, net::UnixListener};

    #[test]
    fn widgets_private_relay_uses_the_app_mode_name() {
        assert!(is_widgets_child(Some("widgets")));
        assert!(!is_widgets_child(Some("nacre-widgets")));
        assert!(!is_widgets_child(Some("settings")));
    }

    fn host(dir: &Path, exe: &Path) -> Host {
        Host {
            exe: exe.into(),
            command: dir.join("command.sock"),
            parent: None,
            app_name: None,
            dir: dir.into(),
            daemon: Some("/run/fake/daemon.sock".into()),
            palette: "shell/palette.json".into(),
        }
    }
    fn private_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("bead-app-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();
        dir
    }
    #[test]
    fn bare_widgets_uses_the_parent_launch_route() {
        let dir = private_dir("widget-parent-route");
        let socket = dir.join("bead-command.sock");
        let _server = ipc::Server::bind(socket.clone(), move |call| {
            assert_eq!(call.target, "widgets");
            assert_eq!(call.method, "open");
            Reply::ok(call.id, Value::Null)
        })
        .unwrap();
        assert!(open_parent_widget(&socket).unwrap());
        drop(_server);
        let _ = std::fs::remove_dir_all(dir);
    }
    #[test]
    fn bare_widgets_defaults_follow_packaged_runtime_sockets() {
        let dir = private_dir("widget-defaults");
        let runtime = dir.join("runtime");
        let socket_dir = runtime.join("nacre");
        std::fs::create_dir_all(&socket_dir).unwrap();
        let parent = UnixListener::bind(socket_dir.join("bead-command.sock")).unwrap();
        let daemon = UnixListener::bind(socket_dir.join("daemon.sock")).unwrap();
        let palette = socket_dir.join("palette-live.json");
        std::fs::write(&palette, "{}").unwrap();
        let mut daemon_socket = None;
        let mut command_socket = None;
        let mut parent_socket = None;
        let mut selected_palette = PathBuf::from("shell/palette.json");
        assert!(apply_widget_runtime_defaults(
            "widgets",
            &mut daemon_socket,
            &mut command_socket,
            &mut parent_socket,
            &mut selected_palette,
            false,
            widget_runtime_paths(Some(&runtime), None)
        ));
        assert_eq!(command_socket, Some(socket_dir.join("app-widgets.sock")));
        assert_eq!(parent_socket, Some(socket_dir.join("bead-command.sock")));
        assert_eq!(daemon_socket, Some(socket_dir.join("daemon.sock")));
        assert_eq!(selected_palette, palette);

        let external_daemon = dir.join("custom-daemon.sock");
        let _custom = UnixListener::bind(&external_daemon).unwrap();
        let paths = widget_runtime_paths(None, Some(&external_daemon)).unwrap();
        assert_eq!(paths.directory, dir);
        assert_eq!(paths.daemon, external_daemon);
        drop((parent, daemon));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn explicit_widget_paths_and_non_widget_modes_are_preserved() {
        let dir = private_dir("widget-explicit");
        let mut daemon = Some(dir.join("explicit-daemon.sock"));
        let mut command = Some(dir.join("explicit-app.sock"));
        let mut parent = Some(dir.join("explicit-parent.sock"));
        let mut palette = PathBuf::from("private-palette.json");
        assert!(apply_widget_runtime_defaults(
            "widgets",
            &mut daemon,
            &mut command,
            &mut parent,
            &mut palette,
            true,
            widget_runtime_paths(Some(&dir), None)
        ));
        assert_eq!(daemon, Some(dir.join("explicit-daemon.sock")));
        assert_eq!(command, Some(dir.join("explicit-app.sock")));
        assert_eq!(parent, Some(dir.join("explicit-parent.sock")));
        assert_eq!(palette, PathBuf::from("private-palette.json"));
        let before = (
            daemon.clone(),
            command.clone(),
            parent.clone(),
            palette.clone(),
        );
        assert!(!apply_widget_runtime_defaults(
            "settings",
            &mut daemon,
            &mut command,
            &mut parent,
            &mut palette,
            false,
            widget_runtime_paths(Some(&dir), None)
        ));
        assert_eq!((daemon, command, parent, palette), before);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn bare_widgets_opens_an_existing_window_socket() {
        let dir = private_dir("widget-existing");
        let socket = dir.join("app-widgets.sock");
        let (tx, rx) = std::sync::mpsc::channel();
        let server = ipc::Server::bind(socket.clone(), move |call| {
            tx.send(call.clone()).unwrap();
            Reply::ok(call.id, Value::Null)
        })
        .unwrap();
        assert!(open_existing_widget(&socket).unwrap());
        let call = rx.recv_timeout(Duration::from_secs(1)).unwrap();
        assert_eq!(call.target, "bead.window");
        assert_eq!(call.method, "open");
        drop(server);
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn unknown_apps_are_refused_and_an_unconfigured_shell_says_so() {
        let dir = private_dir("unknown");
        let reply = host(&dir, Path::new("/bin/true")).route(7, "nope", "toggle", &[]);
        assert_eq!(reply.error.unwrap().code, "unknown_app");
        let reply = route(8, "settings", "open", &[]);
        assert_eq!(reply.id, 8);
        assert_eq!(reply.error.unwrap().code, "unavailable");
        let _ = std::fs::remove_dir_all(dir);
    }
    #[test]
    fn widget_feedback_uses_the_private_window_socket_and_keeps_gesture_identity() {
        let dir = private_dir("widgets-feedback");
        let socket = dir.join("app-widgets.sock");
        let (tx, rx) = std::sync::mpsc::channel();
        let _server = ipc::Server::bind(socket.clone(), move |call| {
            tx.send(call.clone()).unwrap();
            Reply::ok(call.id, serde_json::Value::Null)
        })
        .unwrap();
        widget_feedback_at(&socket, 23, 7, 11, false, "validation rejected").unwrap();
        let call = rx.recv_timeout(Duration::from_secs(1)).unwrap();
        assert_eq!(call.target, "bead.widgets");
        assert_eq!(call.method, "drag-feedback");
        assert_eq!(call.args[0]["pid"], json!(23));
        assert_eq!(call.args[0]["generation"], json!(7));
        assert_eq!(call.args[0]["epoch"], json!(11));
        assert_eq!(call.args[0]["success"], json!(false));
        assert_eq!(call.args[0]["error"], json!("validation rejected"));
        drop(_server);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_missing_app_is_started_with_its_page_socket_and_daemon() {
        let dir = private_dir("spawn");
        let out = dir.join("argv.txt");
        let script = dir.join("fake-app.sh");
        std::fs::write(
            &script,
            format!("#!/bin/sh\nprintf '%s\\n' \"$@\" > {}\n", out.display()),
        )
        .unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o700)).unwrap();
        let reply = host(&dir, &script).route(3, "settings", "open", &[json!("appearance")]);
        assert!(reply.error.is_none());
        let started = std::time::Instant::now();
        while !out.exists() && started.elapsed() < Duration::from_secs(5) {
            std::thread::sleep(Duration::from_millis(20));
        }
        std::thread::sleep(Duration::from_millis(100));
        let argv = std::fs::read_to_string(&out).unwrap();
        let lines: Vec<_> = argv.lines().collect();
        let socket = dir.join("app-settings.sock");
        let host_socket = dir.join("command.sock");
        assert_eq!(
            lines,
            [
                "--app",
                "settings",
                "appearance",
                "--command-socket",
                socket.to_str().unwrap(),
                "--host-socket",
                host_socket.to_str().unwrap(),
                "--palette",
                "shell/palette.json",
                "--daemon-socket",
                "/run/fake/daemon.sock"
            ]
        );
        std::fs::remove_file(&out).unwrap();
        let reply = host(&dir, &script).route(4, "chat", "hover-open", &[]);
        assert!(reply.error.is_none());
        let started = std::time::Instant::now();
        while !out.exists() && started.elapsed() < Duration::from_secs(5) {
            std::thread::sleep(Duration::from_millis(20));
        }
        let argv = std::fs::read_to_string(&out).unwrap();
        assert!(argv.lines().any(|arg| arg == "--hover-open"));
        let _ = std::fs::remove_dir_all(dir);
    }
    #[test]
    fn widgets_app_receives_its_initial_search_argument() {
        assert!(APPS.contains(&"widgets"));
        let dir = private_dir("widgets-window");
        let out = dir.join("argv.txt");
        let script = dir.join("fake-app.sh");
        std::fs::write(
            &script,
            format!("#!/bin/sh\nprintf '%s\\n' \"$@\" > {}\n", out.display()),
        )
        .unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o700)).unwrap();
        let reply = host(&dir, &script).route(3, "widgets", "open", &[json!("clock")]);
        assert!(reply.error.is_none());
        let started = std::time::Instant::now();
        while !out.exists() && started.elapsed() < Duration::from_secs(5) {
            std::thread::sleep(Duration::from_millis(20));
        }
        let argv = std::fs::read_to_string(&out).unwrap();
        let lines: Vec<_> = argv.lines().collect();
        assert_eq!(&lines[..3], ["--app", "widgets", "clock"]);
        assert!(lines.windows(2).any(|pair| pair
            == [
                "--command-socket",
                dir.join("app-widgets.sock").to_str().unwrap()
            ]));
        let _ = std::fs::remove_dir_all(dir);
    }
    #[test]
    fn widgets_routes_to_the_running_window_instead_of_a_panel() {
        let dir = private_dir("widgets-route");
        let (tx, rx) = std::sync::mpsc::channel();
        let _server = ipc::Server::bind(dir.join("app-widgets.sock"), move |call| {
            let _ = tx.send((call.target.clone(), call.method.clone(), call.args.clone()));
            Reply::ok(call.id, Value::Null)
        })
        .unwrap();
        let reply = host(&dir, Path::new("/unused")).route(8, "widgets", "open", &[]);
        assert!(reply.error.is_none());
        assert_eq!(
            rx.recv_timeout(Duration::from_secs(1)).unwrap(),
            ("bead.window".into(), "open".into(), Vec::new())
        );
        drop(_server);
        let _ = std::fs::remove_dir_all(dir);
    }
    #[test]
    fn lip_observations_never_start_a_closed_app() {
        let dir = private_dir("hover-observe");
        for method in ["hover-enter", "hover-leave"] {
            let reply = host(&dir, Path::new("/nonexistent/app")).route(5, "chat", method, &[]);
            assert!(reply.error.is_none());
        }
        let _ = std::fs::remove_dir_all(dir);
    }
    #[test]
    fn startup_queues_lip_leave_and_preserves_observation_order() {
        use std::sync::atomic::{AtomicBool, Ordering};
        let dir = private_dir("hover-order");
        let ready = Arc::new(AtomicBool::new(false));
        let prepared = ready.clone();
        let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
        let log = seen.clone();
        let (polled, polling) = std::sync::mpsc::channel();
        let _server = ipc::Server::bind(dir.join("app-chat.sock"), move |call| {
            if call.method == "state" {
                let _ = polled.send(());
                Reply::ok(
                    call.id,
                    json!({"target_open":true,"prepared":prepared.load(Ordering::SeqCst)}),
                )
            } else {
                log.lock().unwrap().push(call.method.clone());
                Reply::ok(call.id, Value::Null)
            }
        })
        .unwrap();
        let host = host(&dir, Path::new("/nonexistent/app"));
        let (tx, rx) = std::sync::mpsc::channel();
        let worker = std::thread::spawn(move || panel_requests(&host, rx));
        for method in ["hover-open", "hover-leave", "hover-enter"] {
            tx.send(PanelRequest::Panel("chat", method)).unwrap();
        }
        polling.recv_timeout(Duration::from_secs(2)).unwrap();
        assert_eq!(*seen.lock().unwrap(), ["hover-open"]);
        ready.store(true, Ordering::SeqCst);
        drop(tx);
        worker.join().unwrap();
        assert_eq!(
            *seen.lock().unwrap(),
            ["hover-open", "hover-leave", "hover-enter"]
        );
        let _ = std::fs::remove_dir_all(dir);
    }
    #[test]
    fn private_parent_visibility_ipc_preserves_open_close_reopen_order() {
        let dir = private_dir("visibility-order");
        let socket = dir.join("parent.sock");
        let received = Arc::new(std::sync::Mutex::new(Vec::new()));
        let log = received.clone();
        let _server = ipc::Server::bind(socket.clone(), move |call| {
            log.lock()
                .unwrap()
                .push((call.target.clone(), call.method.clone(), call.args.clone()));
            Reply::ok(call.id, Value::Null)
        })
        .unwrap();
        let mut host = host(&dir, Path::new("/nonexistent/app"));
        host.parent = Some(socket);
        let (tx, rx) = std::sync::mpsc::channel();
        let worker = std::thread::spawn(move || panel_requests(&host, rx));
        for open in [true, false, true] {
            tx.send(PanelRequest::Visibility("chat", open)).unwrap();
        }
        drop(tx);
        worker.join().unwrap();
        assert_eq!(
            *received.lock().unwrap(),
            [
                (
                    "bead.apps".into(),
                    "panel-visibility".into(),
                    vec![json!("chat"), json!(true)]
                ),
                (
                    "bead.apps".into(),
                    "panel-visibility".into(),
                    vec![json!("chat"), json!(false)]
                ),
                (
                    "bead.apps".into(),
                    "panel-visibility".into(),
                    vec![json!("chat"), json!(true)]
                ),
            ]
        );
        drop(_server);
        let _ = std::fs::remove_dir_all(dir);
    }
    #[test]
    fn a_running_app_gets_the_call_and_nothing_is_started() {
        let dir = private_dir("running");
        let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
        let log = seen.clone();
        let _server = ipc::Server::bind(dir.join("app-chat.sock"), move |call| {
            log.lock()
                .unwrap()
                .push((call.target.clone(), call.method.clone()));
            Reply::ok(call.id, Value::Null)
        })
        .unwrap();
        // The executable does not exist: a spawn attempt would fail the route.
        let reply = host(&dir, Path::new("/nonexistent/app")).route(5, "chat", "toggle", &[]);
        assert!(reply.error.is_none(), "{:?}", reply.error);
        assert_eq!(
            *seen.lock().unwrap(),
            [("bead.panel".to_string(), "toggle".to_string())]
        );
        let _ = std::fs::remove_dir_all(dir);
    }
}
