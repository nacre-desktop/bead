// SPDX-License-Identifier: GPL-3.0-only
//! B0197: what an accepted launcher row does on Bead. The daemon selects the row after its
//! ticket check and forwards it here (`ui.menu.activate`); this file maps each operation the
//! menu engine emits to a published daemon method, a Bead route, or the same helper command
//! the QML shell ran. The table is the port of QML `NacreMenuContext.execute` and
//! `DesktopActions.execute`; an operation it does not know is refused before anything runs.
use crate::nacre_source::Client;
use bead::{
    core::source::{self, Error},
    services::ipc::Call,
};
use nacre_protocol::menu::{MenuActionValue, MenuActivated, MenuUiActivate, MenuUiRow, MenuView};
use serde_json::{Value, json};
use std::{
    io::Write,
    os::unix::process::CommandExt,
    path::PathBuf,
    process::Stdio,
    time::{Duration, Instant},
};

/// QML waited this long after closing the launcher before an interactive tool (capture, picker) ran.
const CLOSE_DELAY: Duration = Duration::from_millis(320);
/// How long a one-shot read waits for a leased topic's first snapshot.
const TOPIC_WAIT: Duration = Duration::from_secs(2);

#[derive(Debug)]
pub(crate) enum Act {
    /// The reply alone changes the launcher (scope prefixes, calculator, info rows).
    View,
    Copy(String),
    CopyClip(String),
    Lock,
    /// A call to Bead's own command socket (launcher routes, panels, apps).
    Shell(Call),
    /// A program, never through a shell; `input` is the one stdin line QML's worker wrote.
    Run {
        argv: Vec<String>,
        dir: Option<PathBuf>,
        input: Option<String>,
        delay: bool,
    },
    /// A published daemon method; `delay` lets the launcher close before an interactive tool.
    Daemon {
        method: &'static str,
        params: Value,
        delay: bool,
    },
    /// Flip a control-centre source; `ControlCenter` reads it and makes the published call.
    Toggle(&'static str),
    /// Flip a boolean in the desktop settings store (QML: `set(key, data[key] === false)`).
    Flip(String),
    /// Write one desktop settings key.
    Desktop(String, Value),
    /// `settings.desktop` arguments with the store revision (desktop presets).
    Preset(String),
    /// `player.*` on the active player of `dashboard.player`.
    Player(&'static str),
    /// `wallpaper.activate` on the catalogue item (its id is the path).
    Wallpaper(String),
    /// A new AI chat with this question, then the sidebar.
    Ask(String),
}

fn shell_call(target: &str, method: &str, args: &[&str]) -> Call {
    Call {
        version: 1,
        id: 1,
        target: target.into(),
        method: method.into(),
        args: args.iter().map(|a| json!(a)).collect(),
        output: None,
    }
}
fn strings(args: &[&str]) -> Vec<String> {
    args.iter().map(|a| (*a).to_owned()).collect()
}
/// The Nacre helper the QML views ran (`NacrePaths.helperBinary`).
fn helper(args: &[&str]) -> Vec<String> {
    let mut argv = vec![
        crate::prompt::helper_binary()
            .to_string_lossy()
            .into_owned(),
    ];
    argv.extend(strings(args));
    argv
}
fn run(argv: Vec<String>) -> Act {
    Act::Run {
        argv,
        dir: None,
        input: None,
        delay: false,
    }
}
/// QML `work(action, value)`: the helper with the JSON payload on stdin.
fn work(tool: &str, action: &str, value: &str, payload: Value) -> Act {
    Act::Run {
        argv: helper(&[tool, action, value]),
        dir: None,
        input: Some(payload.to_string()),
        delay: false,
    }
}
/// `NacreWallpapers.preference`: `wallpaper-media theme` with the palette keys on stdin.
fn theme(payload: Value) -> Act {
    Act::Run {
        argv: helper(&["wallpaper-media", "theme"]),
        dir: None,
        input: Some(payload.to_string()),
        delay: false,
    }
}
/// An interactive tool after the launcher has closed (QML `captureDelay`).
fn later(argv: Vec<String>, input: Option<Value>) -> Act {
    Act::Run {
        argv,
        dir: None,
        input: input.map(|v| v.to_string()),
        delay: true,
    }
}
/// QML `terminal()`: a visible kitty running a helper command.
fn terminal(title: &str, args: &[&str]) -> Act {
    let mut argv = strings(&["kitty", "--class", "nacre-control", "--title", title, "--"]);
    argv.extend(helper(args));
    run(argv)
}
fn daemon(method: &'static str, params: Value) -> Act {
    Act::Daemon {
        method,
        params,
        delay: false,
    }
}
fn nacre(method: &str, args: &[&str]) -> Act {
    Act::Shell(shell_call("nacre", method, args))
}
fn word(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && !value.contains("..")
        && value
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"-_./#".contains(&c))
}
fn web(url: &str) -> bool {
    url.starts_with("https://") || url.starts_with("http://")
}
fn absolute(path: &str) -> bool {
    path.starts_with('/') && !path.contains('\0')
}

/// `DesktopActions.execute(action, value)`: the legacy action ids (`legacy` rows and the
/// Actions search results).
fn legacy(action: &str, value: &str) -> Option<(Act, bool, Option<MenuView>)> {
    let settings_page = match action {
        "settings" => Some("appearance"),
        "sound" => Some("sound"),
        "wifi" => Some("network"),
        "bluetooth" => Some("bluetooth"),
        _ => None,
    };
    if let Some(page) = settings_page {
        return Some((nacre("settings", &[page]), false, None));
    }
    let act = match action {
        "apps" => return Some((nacre("menu", &["apps", ""]), true, None)),
        "clipboard" => return Some((nacre("menu", &["actions.clipboard", ""]), true, None)),
        "calculator" => return Some((Act::View, true, query("= "))),
        "wallpaper" => return Some((nacre("mode", &["wallpaper"]), true, None)),
        "palette" | "overview" | "keys" => nacre("mode", &[action]),
        "colors" => Act::Shell(shell_call("colorsApp", "open", &[""])),
        "welcome" => Act::Shell(shell_call("welcomeApp", "open", &["home"])),
        "left" | "brain-query" => nacre("left", &[]),
        "power" => Act::Shell(shell_call("bead.panel", "open", &["session"])),
        "preset" if word(value) => Act::Preset(value.into()),
        "toggle-setting" if matches!(value, "animations" | "blur" | "shadow") => {
            Act::Flip(value.into())
        }
        "dnd" => Act::Toggle("notifications.dnd"),
        "light" | "dark" => daemon("appearance.set_mode", json!({"mode": action})),
        "lock" => Act::Lock,
        "audio" => run(strings(&["pavucontrol"])),
        "network" => run(strings(&["nm-connection-editor"])),
        "load" => run(strings(&[
            "kitty",
            "--class",
            "nacre-ai-task",
            "--title",
            "Load chat",
            "--",
            "nacre-ai",
            "window",
            "--worker-command",
            "sessions",
        ])),
        "files" | "terminal" => run(strings(&["nacre-app", action])),
        "new" | "resume" | "tasks" | "brain" | "capture" => run(strings(&["nacre-shell", action])),
        _ => return None,
    };
    Some((act, false, None))
}
fn query(text: &str) -> Option<MenuView> {
    Some(MenuView {
        query: Some(text.into()),
        ..MenuView::default()
    })
}

/// Every operation the menu engine emits (`nacre-menu` rows and the catalogue), so a bad
/// operand is told apart from an operation this table does not know.
pub(crate) const KNOWN: &[&str] = &[
    "accent",
    "ai-terminal",
    "ask",
    "audio-output",
    "background",
    "bring-visitors",
    "calculator",
    "command",
    "config",
    "copy",
    "copy-clip",
    "environment",
    "extension-theme",
    "focus",
    "font",
    "form",
    "info",
    "install-package",
    "legacy",
    "legacy-action",
    "maintenance",
    "media",
    "mono-font",
    "next-wallpaper",
    "note",
    "open-file",
    "palette-mode",
    "personality",
    "power",
    "power-profile",
    "remove-package",
    "route",
    "scope",
    "search-web",
    "setting-toggle",
    "settings",
    "software-extension-install",
    "software-extension-open",
    "software-theme",
    "system-action",
    "timer-cancel",
    "toggle",
    "tool",
    "tui-launch",
    "tui-remove",
    "url",
    "wallpaper",
    "webapp",
    "webapp-offer",
    "workflow",
];
/// One row to its action, whether the launcher stays open, and a view change. `Err(None)` is
/// an operation this table does not know; `Err(Some(_))` a known one with a bad operand.
fn act(row: &MenuUiRow) -> Result<(Act, bool, Option<MenuView>), Option<&'static str>> {
    let bad = Err(Some("invalid operand"));
    let text = match &row.value {
        Some(MenuActionValue::Text(text)) => text.as_str(),
        _ => "",
    };
    let id = match &row.value {
        Some(MenuActionValue::Text(text)) => text.clone(),
        Some(MenuActionValue::Integer(n)) => n.to_string(),
        None => String::new(),
    };
    let done = |act: Act| Ok((act, false, None));
    let stay = |act: Act| Ok((act, true, None));
    match row.operation.as_str() {
        "copy" if matches!(row.value, Some(MenuActionValue::Text(_))) => {
            done(Act::Copy(text.into()))
        }
        "copy-clip" if !text.is_empty() && text.len() <= 256 => done(Act::CopyClip(text.into())),
        "command" => match &row.arguments {
            Some(argv) if !argv.is_empty() => done(Act::Run {
                argv: argv.clone(),
                dir: row
                    .directory
                    .as_deref()
                    .map(PathBuf::from)
                    .filter(|p| p.is_absolute()),
                input: None,
                delay: false,
            }),
            _ => bad,
        },
        "power" => match text {
            "lock" => done(Act::Lock),
            // Confirmation belongs to this UI: the session view asks before it acts.
            "logout" | "suspend" | "reboot" | "poweroff" => {
                done(Act::Shell(shell_call("bead.panel", "open", &["session"])))
            }
            _ => bad,
        },
        // QML treats the wallpaper route row as the visual picker, not a menu navigation.
        "route" if text == "wallpaper" => stay(nacre("mode", &["wallpaper"])),
        "route" => {
            let call = shell_call("nacre", "menu", &[text, ""]);
            match crate::key_routes::launcher(&call) {
                Some(Ok(_)) => stay(Act::Shell(call)),
                _ => bad,
            }
        }
        // View-only rows: the search field takes the prefix, the info row stays selected.
        "scope" if !text.is_empty() && text.len() <= 16 => Ok((Act::View, true, query(text))),
        "calculator" => Ok((Act::View, true, query("= "))),
        "info" => stay(Act::View),
        "settings" if word(text) => done(nacre("settings", &[text])),
        "legacy" => legacy(text, "").map_or(bad, Ok),
        "legacy-action" => legacy(row.action.as_deref().unwrap_or(""), text).map_or(bad, Ok),
        // The wallpaper-picker preferences QML wrote with `wallpaper-media theme`.
        "personality" if word(text) => stay(theme(json!({"palettePersonality": text}))),
        "background" if matches!(text, "accent" | "wallpaper") => stay(theme(
            json!({"paletteBackgroundFromWallpaper": text == "wallpaper"}),
        )),
        "accent" if !text.is_empty() && text.len() <= 64 => {
            stay(theme(json!({"paletteAccent": text})))
        }
        "software-theme" if word(text) && row.bundle != Some(true) => stay(theme(
            json!({"palettePreset": text, "paletteMode": row.theme_mode.as_deref().unwrap_or("dark")}),
        )),
        "software-theme" if word(text) => match row.bundle_scope.as_deref() {
            Some(scope) if word(scope) => done(work(
                "software-library",
                "bundle-apply",
                text,
                json!({"mode": row.theme_mode.as_deref().unwrap_or("dark"), "scope": scope}),
            )),
            _ => bad,
        },
        "palette-mode" if matches!(text, "light" | "dark" | "auto") => {
            stay(daemon("appearance.set_mode", json!({"mode": text})))
        }
        "next-wallpaper" => stay(daemon("wallpaper.next", json!({}))),
        "wallpaper" if absolute(text) => stay(Act::Wallpaper(text.into())),
        "setting-toggle" if matches!(text, "animations" | "blur" | "shadow") => {
            stay(Act::Flip(text.into()))
        }
        "font" => stay(Act::Desktop("interfaceFont".into(), json!(text))),
        "mono-font" => done(Act::Desktop("monoFont".into(), json!(text))),
        "toggle" => stay(Act::Toggle(match text {
            "dnd" => "notifications.dnd",
            "night-light" => "night-light.state",
            "stay-awake" => "stay-awake.state",
            "mute" => "audio.output",
            "mic-mute" => "audio.input",
            "wifi" => "network.wifi",
            "bluetooth" => "bluetooth.power",
            _ => return bad,
        })),
        "power-profile" if matches!(text, "power-saver" | "balanced" | "performance") => {
            stay(daemon("power.set_profile", json!({"profile": text})))
        }
        "media" => stay(Act::Player(match text {
            "playPause" => "player.play_pause",
            "next" => "player.next",
            "previous" => "player.previous",
            _ => return bad,
        })),
        "audio-output" if !id.is_empty() && id.len() <= 128 => {
            stay(daemon("audio.set_default", json!({"node": id})))
        }
        "focus" if !id.is_empty() && id.len() <= 128 => {
            done(daemon("window.focus", json!({"window": id})))
        }
        "url" if web(text) => done(run(strings(&["xdg-open", text]))),
        // Bead has no Brain explorer in the sidebar yet: a note opens like a file.
        "open-file" | "note" if absolute(text) => done(run(strings(&["xdg-open", text]))),
        "search-web" if !text.is_empty() => {
            let url = row
                .web_url
                .clone()
                .filter(|u| web(u))
                .unwrap_or_else(|| format!("https://duckduckgo.com/?q={}", encode(text)));
            done(run(strings(&["nacre-app", "browser", &url])))
        }
        "bring-visitors" => done(run(helper(&["monitor-workspaces", "bring"]))),
        "tool" => tool(text).map_or(bad, done),
        "maintenance" if matches!(text, "check" | "restart") => {
            done(terminal("Nacre maintenance", &["maintenance", text]))
        }
        "config" if word(text) => done(terminal(
            "Nacre configuration",
            &["menu-backend", "config", text],
        )),
        "timer-cancel" if word(text) => {
            stay(work("menu-backend", "cancel-reminder", text, json!({})))
        }
        "webapp" if word(text) => done(run(strings(&["nacre-menu", "webapp-launch", text]))),
        "webapp-offer" => match row.web_url.as_deref() {
            Some(url) if web(url) => done(work(
                "menu-backend",
                "webapp-create",
                "",
                json!({"label": row.name, "url": url}),
            )),
            _ => bad,
        },
        "install-package" if word(text) => done(terminal(
            &format!("Install {}", row.name),
            &["software-package", "install", text],
        )),
        "remove-package" if word(text) => done(terminal(
            &format!("Remove {}", row.name),
            &["menu-backend", "remove", text],
        )),
        "environment" if word(text) => match row.environment_operation.as_deref() {
            Some(op @ ("install" | "remove")) => {
                let mut argv = strings(&[
                    "kitty",
                    "--class",
                    "nacre-control",
                    "--title",
                    &row.name,
                    "--",
                ]);
                argv.extend(helper(&["software-environments", "run", op, text]));
                done(run(argv))
            }
            _ => bad,
        },
        "ai-terminal" => match &row.terminal_command {
            Some(command) if !command.is_empty() => {
                let mut argv = strings(&["kitty", "--hold", "--"]);
                argv.extend(command.iter().cloned());
                done(run(argv))
            }
            _ => bad,
        },
        "system-action" if word(text) => {
            done(work("software-library", "system-open", text, json!({})))
        }
        op @ ("tui-launch" | "tui-remove" | "extension-theme") if word(text) => {
            done(work("software-library", op, text, json!({})))
        }
        "software-extension-install" if word(text) => {
            done(work("software-library", "install-builtin", text, json!({})))
        }
        "software-extension-open" if word(text) => done(work(
            "software-library",
            "open",
            text,
            json!({"action": row.extension_action.as_deref().unwrap_or("")}),
        )),
        "workflow" if word(text) => done(work(
            "launcher-workflows",
            "activate",
            text,
            json!({"query": row.search_query.as_deref().unwrap_or("")}),
        )),
        "ask" if !text.trim().is_empty() => done(Act::Ask(text.into())),
        // Saved actions, snippets, searches, bookmarks, workspaces and roots are edited on
        // the Settings page that owns them (Bead has no inline launcher form).
        "form"
            if matches!(
                text,
                "action" | "snippet" | "search" | "bookmark" | "workspace" | "roots"
            ) =>
        {
            done(nacre("settings", &["windows-workspaces/workflows"]))
        }
        op if KNOWN.contains(&op) => bad,
        _ => Err(None),
    }
}
/// The `tool` rows of Actions > Capture, Reminders and Health.
fn tool(name: &str) -> Option<Act> {
    Some(match name {
        "capture-area" | "capture-screen" | "capture-window" => later(
            helper(&["launcher-workflows", "capture", name]),
            Some(json!({})),
        ),
        "capture-qr" => later(helper(&["launcher-workflows", "qr", ""]), Some(json!({}))),
        // The control-centre recorder path: the daemon refuses a second recording and
        // publishes `controls.recorder.recording` for the bar's stop button.
        "record-start" | "record-screen" => Act::Daemon {
            method: "recorder.start",
            params: json!({"mode": if name == "record-screen" { "screen" } else { "region" }}),
            delay: true,
        },
        "record-stop" => daemon("recorder.stop", json!({})),
        "color-picker" | "capture-text" => later(helper(&["menu-backend", "tool", name]), None),
        "updates" => terminal("System updates", &["menu-backend", "tool", name]),
        "logs" => terminal("Nacre logs", &["menu-backend", "tool", name]),
        "install-mise" => terminal("Install mise", &["software-package", "install", "mise"]),
        name if word(name) => work("menu-backend", "tool", name, json!({})),
        _ => return None,
    })
}
/// `encodeURIComponent` for the fallback web search URL.
fn encode(text: &str) -> String {
    text.bytes()
        .map(|b| match b {
            b'A'..=b'Z'
            | b'a'..=b'z'
            | b'0'..=b'9'
            | b'-'
            | b'_'
            | b'.'
            | b'!'
            | b'~'
            | b'*'
            | b'\''
            | b'('
            | b')' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

/// The row is daemon-selected after the ticket check (`ui.menu.activate` is not a public
/// method). Every operation the engine emits maps to an action; anything else is refused
/// before anything runs.
pub(crate) fn plan(params: Value) -> source::Result<(Act, MenuActivated)> {
    let invalid = || Error::new("validation", "Invalid launcher action", false);
    let request: MenuUiActivate = serde_json::from_value(params).map_err(|_| invalid())?;
    request.validate().map_err(|_| invalid())?;
    if request.row.disabled {
        return Err(invalid());
    }
    match act(&request.row) {
        Ok((act, keep_open, view)) => Ok((act, MenuActivated { keep_open, view })),
        Err(Some(_)) => Err(invalid()),
        Err(None) => Err(Error::new(
            "unavailable",
            "Launcher action is not supported by this UI",
            false,
        )),
    }
}

// ---------------------------------------------------------------- execution

fn copy(text: &str) -> Result<(), String> {
    if text.is_empty() {
        return Ok(());
    }
    let mut child = bead::services::process::command("wl-copy")
        .args(["--type", "text/plain;charset=utf-8"])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("wl-copy: {e}"))?;
    child
        .stdin
        .take()
        .ok_or("wl-copy stdin")?
        .write_all(text.as_bytes())
        .map_err(|e| format!("wl-copy: {e}"))?;
    match child.wait() {
        Ok(status) if status.success() => Ok(()),
        _ => Err("wl-copy failed".into()),
    }
}
fn spawn(argv: &[String], dir: Option<PathBuf>, input: Option<String>) -> Result<(), String> {
    let mut command = bead::services::process::command(&argv[0]);
    command
        .args(&argv[1..])
        .stdin(if input.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .process_group(0);
    if let Some(dir) = dir {
        command.current_dir(dir);
    }
    let mut child = command.spawn().map_err(|e| format!("{}: {e}", argv[0]))?;
    if let (Some(line), Some(mut stdin)) = (input, child.stdin.take()) {
        let _ = writeln!(stdin, "{line}");
    }
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}
async fn call(client: &Option<Client>, method: &str, params: Value) -> Result<Value, String> {
    client
        .as_ref()
        .ok_or("daemon client unavailable")?
        .call(method.into(), params, Duration::from_secs(5))
        .await
        .map_err(|e| e.message)
}
/// The first healthy snapshot of a topic, through a short view lease.
async fn topic(client: &Option<Client>, name: &str) -> Result<Value, String> {
    let source = client
        .as_ref()
        .ok_or("daemon client unavailable")?
        .view_topic(name)
        .map_err(|e| e.message)?;
    let deadline = Instant::now() + TOPIC_WAIT;
    loop {
        let snapshot = source.snapshot();
        if snapshot.connected && snapshot.data.as_object().is_some_and(|d| !d.is_empty()) {
            return Ok((*snapshot.data).clone());
        }
        if Instant::now() > deadline {
            return Err(format!("{name} is unavailable"));
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}
/// `settings.get` then one write at the read revision.
async fn desktop(client: &Option<Client>, key: &str, value: Option<Value>) -> Result<(), String> {
    let store = call(client, "settings.get", json!({"store": "desktop"})).await?;
    let value = value.unwrap_or_else(|| json!(store["data"][key] == false));
    call(
        client,
        "settings.set",
        json!({"store": "desktop", "key": [key], "value": value, "rev": store["rev"]}),
    )
    .await
    .map(drop)
}
/// The control-centre toggle, run on its own thread: `ControlCenter` keeps per-view state.
fn toggle(client: Client, source: &'static str) -> Result<(), String> {
    let topic = match source {
        "audio.output" | "audio.input" => "audio",
        "network.wifi" => "network",
        "bluetooth.power" => "bluetooth",
        "notifications.dnd" => "notifications",
        _ => "controls",
    };
    let lease = client.view_topic(topic).map_err(|e| e.message)?;
    let control = crate::control_center::ControlCenter::new(
        Some(client),
        vec![(topic, lease)],
        None,
        Vec::new(),
    );
    let deadline = Instant::now() + TOPIC_WAIT;
    loop {
        match control.quick_state(source) {
            Some((_, true, ..)) => break,
            Some((_, false, true, ..)) => return Err(format!("{source} is set elsewhere")),
            _ if Instant::now() > deadline => return Err(format!("{source} is unavailable")),
            _ => std::thread::sleep(Duration::from_millis(20)),
        }
    }
    control.act(&format!("toggle:{source}"));
    // The call is cancelled when its ticket drops: hold it until the daemon answered.
    let deadline = Instant::now() + Duration::from_secs(5);
    while control.quick_pending(source) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(20));
        control.poll_quick();
    }
    let message = control.quick_message();
    message.is_empty().then_some(()).ok_or(message)
}
fn blocking_shell(shell: PathBuf, call: Call) -> Result<(), String> {
    match bead::services::ipc::call(&shell, &call, Duration::from_secs(2)) {
        Ok(reply) => reply.error.map_or(Ok(()), |e| Err(e.message)),
        Err(e) => Err(e.to_string()),
    }
}
async fn blocking(
    work: impl FnOnce() -> Result<(), String> + Send + 'static,
) -> Result<(), String> {
    tokio::task::spawn_blocking(work)
        .await
        .map_err(|e| e.to_string())
        .and_then(|r| r)
}

/// Runs after the acknowledgement: the daemon waits for that reply before it can
/// finish the pending `menu.activate`, so nothing here may wait on the launcher.
pub(crate) async fn execute(act: Act, client: Option<Client>, shell: PathBuf) {
    let result = match act {
        Act::View => Ok(()),
        Act::Copy(text) => blocking(move || copy(&text)).await,
        Act::Run {
            argv,
            dir,
            input,
            delay,
        } => {
            if delay {
                tokio::time::sleep(CLOSE_DELAY).await;
            }
            blocking(move || spawn(&argv, dir, input)).await
        }
        Act::Shell(call) => blocking(move || blocking_shell(shell, call)).await,
        Act::CopyClip(id) => call(&client, "clipboard.copy", json!({"id": id}))
            .await
            .map(drop),
        Act::Lock => call(&client, "session.request", json!({"action":"lock"}))
            .await
            .and_then(|v| {
                (v.get("locked") == Some(&Value::Bool(true)))
                    .then_some(())
                    .ok_or("lock was not confirmed".into())
            }),
        Act::Daemon {
            method,
            params,
            delay,
        } => {
            if delay {
                tokio::time::sleep(CLOSE_DELAY).await;
            }
            call(&client, method, params).await.map(drop)
        }
        Act::Toggle(source) => match client {
            Some(client) => blocking(move || toggle(client, source)).await,
            None => Err("daemon client unavailable".into()),
        },
        Act::Flip(key) => desktop(&client, &key, None).await,
        Act::Desktop(key, value) => desktop(&client, &key, Some(value)).await,
        Act::Preset(name) => {
            async {
                let store = call(&client, "settings.get", json!({"store": "desktop"})).await?;
                call(
                    &client,
                    "settings.desktop",
                    json!({"arguments": ["preset", name], "rev": store["rev"]}),
                )
                .await
                .map(drop)
            }
            .await
        }
        Act::Player(method) => {
            async {
                let data = topic(&client, "dashboard.player").await?;
                let player = data["active"].clone();
                if player.is_null() {
                    return Err("no active player".to_owned());
                }
                call(&client, method, json!({"player": player}))
                    .await
                    .map(drop)
            }
            .await
        }
        Act::Wallpaper(path) => {
            async {
                let data = topic(&client, "wallpaper.active").await?;
                call(
                    &client,
                    "wallpaper.activate",
                    json!({"revision": data["revision"], "id": path}),
                )
                .await
                .map(drop)
            }
            .await
        }
        Act::Ask(text) => {
            async {
                call(&client, "ai.chat.new", json!({"channel": ""})).await?;
                call(
                    &client,
                    "ai.chat.send",
                    json!({"channel": "", "text": text, "attachments": []}),
                )
                .await?;
                blocking(|| {
                    let reply = crate::app::route(0, "chat", "open", &[]);
                    reply.error.map_or(Ok(()), |e| Err(e.message))
                })
                .await
            }
            .await
        }
    };
    if let Err(error) = result {
        eprintln!("UI_MENU_ACTION_FAILED {error}");
    }
}

#[cfg(test)]
#[path = "launcher_actions_tests.rs"]
mod tests;
