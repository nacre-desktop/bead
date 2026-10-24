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
};
use bead::{
    render::Palette,
    services::ipc::{self, Call, Reply},
    wayland::{Options, Panels},
};
use serde_json::{Value, json};
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
pub const APPS: [&str; 5] = ["settings", "chat", "colors", "welcome", "prompt"];

/// Topics every app follows: Nacre's text catalog and the settings that shape the look.
const TOPICS: [&str; 5] = [
    "i18n",
    prefs::DESKTOP,
    prefs::DASHBOARD,
    prefs::PROFILE,
    prefs::LAUNCHER_LAYOUT,
];

pub struct Host {
    exe: PathBuf,
    /// The shell's own command socket (`nacre clipboard` and the other routed commands).
    command: PathBuf,
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
    let (Some(dir), Ok(exe)) = (command_socket.parent(), std::env::current_exe()) else {
        return;
    };
    let _ = HOST.set(Host {
        exe,
        command: command_socket.into(),
        dir: dir.into(),
        daemon,
        palette,
    });
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
/// Open, deep-link or toggle an app for a routed command. Never blocks on the app's startup.
pub fn route(id: u64, name: &str, method: &str, args: &[Value]) -> Reply {
    match HOST.get() {
        Some(host) => host.route(id, name, method, args),
        None => Reply::error(id, "unavailable", "apps are not configured in this shell"),
    }
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
        if !APPS.contains(&name) {
            return Reply::error(id, "unknown_app", "no such app");
        }
        // Windows take the open call (a second open deep-links the running one); panels the
        // given panel method (`toggle` closes the running sidebar).
        let (target, method) = if matches!(name, "settings" | "colors" | "welcome") {
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
            output: None,
        };
        match ipc::call(&self.socket(name), &call, Duration::from_millis(800)) {
            Ok(reply) if reply.error.is_none() => return Reply::ok(id, Value::Null),
            Ok(reply) => {
                let error = reply.error.expect("checked");
                return Reply::error(id, &error.code, error.message);
            }
            Err(_) => {}
        }
        match self.spawn(name, args) {
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
        match self.spawn("prompt", &[json!(ident)]) {
            Ok(()) => Reply::ok(id, Value::Null),
            Err(error) => Reply::error(id, "spawn", error),
        }
    }
    /// Starts the app detached from the shell's stdio. Single-instance is the socket bind: a
    /// second process that loses the race exits at once.
    fn spawn(&self, name: &str, args: &[Value]) -> Result<(), String> {
        use std::os::unix::process::CommandExt;
        let mut command = std::process::Command::new(&self.exe);
        command.arg("--app").arg(name);
        for arg in args.iter().filter_map(Value::as_str) {
            command.arg(arg);
        }
        command
            .arg("--command-socket")
            .arg(self.socket(name))
            .arg("--palette")
            .arg(live_palette_file(&self.dir).unwrap_or_else(|| self.palette.clone()));
        if let Some(daemon) = &self.daemon {
            command.arg("--daemon-socket").arg(daemon);
        }
        let mut child = command
            .stdin(Stdio::null())
            .stdout(Stdio::null())
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
        let size = state["size"]
            .as_array()
            .and_then(|s| Some((s.first()?.as_f64()?, s.get(1)?.as_f64()?)));
        if let (Some(true), Some((w, h))) = (state["target_open"].as_bool(), size)
            && w > 64.
            && h > 64.
        {
            placement::set_external(
                app,
                Some(placement::External {
                    attachment: placement::Attachment::Attached(bead::core::Edge::Left),
                    size: ((w - 64.).max(0.) as f32, (h - 64.).max(0.) as f32),
                }),
            );
            return;
        }
    }
}

/// The app process: `cli` is everything after `--app`.
pub fn run(cli: &[String]) -> Result<(), Box<dyn Error>> {
    let name = cli.first().ok_or("--app needs a name")?.as_str();
    if !APPS.contains(&name) {
        return Err(format!("unknown app: {name}").into());
    }
    let mut args = Vec::new();
    let mut daemon = None;
    let mut command_socket = None;
    let mut palette = PathBuf::from("shell/palette.json");
    let mut output = None;
    let mut rest = cli[1..].iter();
    while let Some(arg) = rest.next() {
        let mut value = |what: &str| rest.next().cloned().ok_or(format!("{arg} needs {what}"));
        match arg.as_str() {
            "--daemon-socket" => daemon = Some(PathBuf::from(value("a path")?)),
            "--command-socket" => command_socket = Some(PathBuf::from(value("a path")?)),
            "--palette" => palette = value("a path")?.into(),
            "--output" => output = Some(value("a name")?),
            flag if flag.starts_with("--") => {
                return Err(format!("unknown argument: {flag}").into());
            }
            positional => args.push(json!(positional)),
        }
    }
    let colors = Palette::read(&palette)
        .or_else(|_| Palette::parse(include_bytes!("../../../shell/palette.json")))?;
    crate::i18n::init();
    // An app that opens another (Welcome's tiles) calls it the way the shell does.
    if let Some(socket) = &command_socket {
        configure(socket, daemon.clone(), palette.clone());
    }
    // A plain client of the daemon: it follows topics and calls methods, registers nothing.
    let client = daemon
        .map(|socket| {
            let mut topics: Vec<String> = TOPICS.iter().map(|t| t.to_string()).collect();
            if name == "colors" || name == "welcome" {
                topics.extend(colors_app::TOPICS.iter().map(|t| t.to_string()));
            } else {
                // Every app wears the live palette (B0068): the wallpaper topic carries it.
                topics.push(colors_app::ACTIVE.to_string());
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
    let (first, first_seen) = std::sync::mpsc::sync_channel::<()>(1);
    let _prefs = client.as_ref().and_then(|client| {
        prefs::Service::start_with(
            topic(client, prefs::DESKTOP)?,
            topic(client, prefs::DASHBOARD)?,
            topic(client, prefs::PROFILE),
            topic(client, prefs::LAUNCHER_LAYOUT),
            Arc::new(move |p: &prefs::Prefs| {
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
        output,
        interactive: true,
        keyboard_focus: true,
        app: Some(args.clone()),
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
                colors,
                Default::default(),
                false,
            )?;
            options.panels = Some(Panels::new(
                "chat".into(),
                BTreeMap::from([("chat".into(), spec)]),
            )?);
        }
    }
    bead::wayland::run(options, crate::sound::fallback(fill))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn host(dir: &Path, exe: &Path) -> Host {
        Host {
            exe: exe.into(),
            command: dir.join("command.sock"),
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
        assert_eq!(
            lines,
            [
                "--app",
                "settings",
                "appearance",
                "--command-socket",
                socket.to_str().unwrap(),
                "--palette",
                "shell/palette.json",
                "--daemon-socket",
                "/run/fake/daemon.sock"
            ]
        );
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
