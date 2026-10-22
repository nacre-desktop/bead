// SPDX-License-Identifier: GPL-3.0-only
//! Daemon UI calls enter the same audited local command path as the legacy CLI.
use bead::{
    core::source::{self, Error},
    services::ipc::Call,
};
use nacre_protocol::menu::{MenuActionValue, MenuUiActivate};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    io::Write,
    os::unix::process::CommandExt,
    path::PathBuf,
    process::Stdio,
    sync::{Arc, OnceLock},
    time::Duration,
};

#[derive(Deserialize)]
struct Dispatch {
    target: String,
    method: String,
    #[serde(default)]
    args: Vec<String>,
}
fn request(params: Value) -> source::Result<bead::services::ipc::Call> {
    let p: Dispatch = serde_json::from_value(params)
        .map_err(|_| Error::new("validation", "Invalid UI dispatch", false))?;
    if p.target.len() > 256
        || p.method.len() > 128
        || p.args.len() > 16
        || p.args.iter().any(|s| s.len() > 65536 || s.contains('\0'))
    {
        return Err(Error::new(
            "validation",
            "UI dispatch exceeds bounds",
            false,
        ));
    }
    let call = crate::commands::from_strings(p.target, p.method, &p.args)
        .map_err(|_| Error::new("validation", "Invalid UI dispatch arguments", false))?;
    crate::commands::validate(&call).map_err(|_| {
        Error::new(
            "unknown_method",
            "UI command is not in the route catalog",
            false,
        )
    })?;
    Ok(call)
}
/// What an accepted launcher row does once the provider has acknowledged it.
enum Act {
    Copy(String),
    CopyClip(String),
    Lock,
    /// A call to Bead's own command socket (view changes).
    Shell(Call),
    Run {
        argv: Vec<String>,
        dir: Option<PathBuf>,
    },
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
/// The row is daemon-selected after the ticket check (`ui.menu.activate` is not a
/// public method). Only operations Bead can do safely are accepted; everything
/// else is refused before anything runs. The second value is the reply's `keep_open`.
fn plan(params: Value) -> source::Result<(Act, bool)> {
    let invalid = || Error::new("validation", "Invalid launcher action", false);
    let request: MenuUiActivate = serde_json::from_value(params).map_err(|_| invalid())?;
    request.validate().map_err(|_| invalid())?;
    let row = request.row;
    if row.disabled {
        return Err(invalid());
    }
    let text = match row.value {
        Some(MenuActionValue::Text(text)) => Some(text),
        _ => None,
    };
    Ok(match (row.operation.as_str(), text.as_deref()) {
        ("copy", Some(text)) => (Act::Copy(text.into()), false),
        ("copy-clip", Some(id)) if !id.is_empty() && id.len() <= 256 => {
            (Act::CopyClip(id.into()), false)
        }
        ("command", _) => (
            Act::Run {
                argv: row.arguments.unwrap_or_default(),
                dir: row.directory.map(PathBuf::from).filter(|p| p.is_absolute()),
            },
            false,
        ),
        ("power", Some("lock")) => (Act::Lock, false),
        // Confirmation belongs to this UI: the session view asks before it acts.
        ("power", Some("logout" | "suspend" | "reboot" | "poweroff")) => (
            Act::Shell(shell_call("bead.panel", "open", &["session"])),
            false,
        ),
        ("route", Some(route)) => {
            let call = shell_call("nacre", "menu", &[route, ""]);
            match crate::key_routes::launcher(&call) {
                Some(Ok(_)) => (Act::Shell(call), true),
                _ => return Err(invalid()),
            }
        }
        _ => {
            return Err(Error::new(
                "unavailable",
                "Launcher action is not supported by this UI",
                false,
            ));
        }
    })
}
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
fn run(argv: &[String], dir: Option<PathBuf>) -> Result<(), String> {
    let mut command = bead::services::process::command(&argv[0]);
    command
        .args(&argv[1..])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .process_group(0);
    if let Some(dir) = dir {
        command.current_dir(dir);
    }
    let mut child = command.spawn().map_err(|e| format!("{}: {e}", argv[0]))?;
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}
async fn daemon(
    client: &Option<crate::nacre_source::Client>,
    method: &str,
    params: Value,
) -> Result<Value, String> {
    client
        .as_ref()
        .ok_or("daemon client unavailable")?
        .call(method.into(), params, Duration::from_secs(5))
        .await
        .map_err(|e| e.message)
}
/// Runs after the acknowledgement: the daemon waits for that reply before it can
/// finish the pending `menu.activate`, so nothing here may wait on the launcher.
async fn execute(act: Act, client: Option<crate::nacre_source::Client>, shell: PathBuf) {
    let result = match act {
        Act::Copy(text) => tokio::task::spawn_blocking(move || copy(&text)).await,
        Act::Run { argv, dir } => tokio::task::spawn_blocking(move || run(&argv, dir)).await,
        Act::Shell(call) => {
            tokio::task::spawn_blocking(move || {
                match bead::services::ipc::call(&shell, &call, Duration::from_secs(2)) {
                    Ok(reply) => reply.error.map_or(Ok(()), |e| Err(e.message)),
                    Err(e) => Err(e.to_string()),
                }
            })
            .await
        }
        Act::CopyClip(id) => Ok(daemon(&client, "clipboard.copy", json!({"id": id}))
            .await
            .map(drop)),
        Act::Lock => Ok(daemon(&client, "session.request", json!({"action":"lock"}))
            .await
            .and_then(|v| {
                (v.get("locked") == Some(&Value::Bool(true)))
                    .then_some(())
                    .ok_or("lock was not confirmed".into())
            })),
    };
    if let Err(error) = result.map_err(|e| e.to_string()).and_then(|r| r) {
        eprintln!("UI_MENU_ACTION_FAILED {error}");
    }
}
pub fn start(
    daemon: PathBuf,
    shell: PathBuf,
    notifications: Option<PathBuf>,
) -> source::Result<crate::nacre_source::Client> {
    if !shell.is_absolute() || notifications.as_ref().is_some_and(|p| !p.is_absolute()) {
        return Err(Error::new(
            "validation",
            "UI sockets must be explicit absolute paths",
            false,
        ));
    }
    let topics = [
        "menu",
        "tray",
        "i18n",
        crate::prefs::DESKTOP,
        crate::prefs::DASHBOARD,
    ];
    let mut config = crate::nacre_source::Config::new(daemon, topics.map(String::from).to_vec());
    let slot: Arc<OnceLock<crate::nacre_source::WeakClient>> = Default::default();
    let own = slot.clone();
    config.provider = Some((
        "ui".into(),
        BTreeMap::from([("ui.dispatch".into(), 1), ("ui.menu.activate".into(), 1)]),
        Arc::new(move |method, params| {
            let shell = shell.clone();
            let notifications = notifications.clone();
            let slot = slot.clone();
            Box::pin(async move {
                if method == "ui.menu.activate" {
                    let (act, keep_open) = plan(params)?;
                    let client = slot.get().and_then(|c| c.upgrade());
                    tokio::spawn(execute(act, client, shell));
                    return Ok(json!({"keep_open": keep_open}));
                }
                if method != "ui.dispatch" {
                    return Err(Error::new("unknown_method", "UI method unavailable", false));
                }
                let call = request(params)?;
                let socket = if call.target == "notifs" {
                    notifications.ok_or_else(|| {
                        Error::new("unavailable", "Notification UI is not registered", true)
                    })?
                } else {
                    shell
                };
                // Existing peer-checked IPC and bounded blocking pool; no shell command,
                // catalog/ranking logic, or extra always-on forwarding process.
                tokio::task::spawn_blocking(move || {
                    let reply = bead::services::ipc::call(&socket, &call, Duration::from_secs(2))
                        .map_err(|_| {
                        Error::new("unavailable", "UI command transport unavailable", true)
                    })?;
                    if let Some(error) = reply.error {
                        Err(Error::new(&error.code, &error.message, false))
                    } else {
                        Ok(reply.result.unwrap_or(Value::Null))
                    }
                })
                .await
                .map_err(|_| Error::new("unavailable", "UI command task unavailable", true))?
            })
        }),
    ));
    let client = crate::nacre_source::Client::start(config)?;
    let _ = own.set(client.downgrade());
    Ok(client)
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    /// Published daemon-to-provider rows, in transcript order (provider-1 ... provider-5).
    fn golden_rows() -> Vec<Value> {
        include_str!("../tests/protocol-v1/menu-ui-actions.jsonl")
            .lines()
            .map(|l| serde_json::from_str::<Value>(l).unwrap())
            .filter(|f| f["m"] == "ui.menu.activate" && f["id"].as_str().is_some())
            .map(|f| f["p"].clone())
            .collect()
    }
    fn row(operation: &str, value: Value) -> Value {
        json!({"row":{"id":"r","kind":"action","name":"R","operation":operation,"value":value,"disabled":false},"catalog_rev":3})
    }
    #[test]
    fn published_rows_map_to_the_operations_bead_can_do_and_refuse_the_rest() {
        let rows = golden_rows();
        assert_eq!(rows.len(), 5);
        let (act, keep) = plan(rows[0].clone()).unwrap();
        assert!(matches!(act, Act::Copy(ref t) if t == "2") && !keep);
        let (act, keep) = plan(rows[1].clone()).unwrap();
        // Restart is confirmed in Bead's own session view, never executed here.
        assert!(
            matches!(act, Act::Shell(ref c) if c.target == "bead.panel" && c.method == "open" && c.args == [json!("session")])
                && !keep
        );
        let (act, _) = plan(rows[2].clone()).unwrap();
        assert!(matches!(act, Act::CopyClip(ref id) if id == "fixture-image-1"));
        let (act, keep) = plan(rows[3].clone()).unwrap();
        assert!(
            matches!(act, Act::Run { ref argv, ref dir } if argv == &["fixture-app", "space & literal", "$(literal)"] && dir.as_deref() == Some(std::path::Path::new("/fixture")))
                && !keep
        );
        // The published refusal for an operation this UI does not know.
        let error = plan(rows[4].clone()).err().unwrap();
        assert_eq!(
            (error.code.as_str(), error.message.as_str(), error.retry),
            (
                "unavailable",
                "Launcher action is not supported by this UI",
                false
            )
        );
    }
    #[test]
    fn lock_route_and_malformed_rows_are_handled_fail_closed() {
        assert!(matches!(
            plan(row("power", json!("lock"))).unwrap(),
            (Act::Lock, false)
        ));
        // The key route resolves aliases (wallpaper -> style.wallpapers) when it runs.
        let (act, keep) = plan(row("route", json!("wallpaper"))).unwrap();
        assert!(
            matches!(act, Act::Shell(ref c) if c.method == "menu" && c.args == [json!("wallpaper"), json!("")])
                && keep
        );
        for bad in [
            row("route", json!("Bad Route!")),
            row("copy", json!(7)),
            row("copy-clip", json!("")),
            row("power", json!("reboot-now")),
            row("command", Value::Null),
            json!({"row":{"id":"r","kind":"a","name":"R","operation":"copy","value":"x","disabled":true},"catalog_rev":3}),
            json!({"row":{"id":"r"},"catalog_rev":3}),
            json!({"row":{"id":"r","kind":"a","name":"R","operation":"copy","value":"x\u{0}"},"catalog_rev":3}),
        ] {
            assert!(plan(bad.clone()).is_err(), "{bad}");
        }
    }
    #[test]
    fn dispatch_reuses_typed_legacy_routes_and_rejects_private_envelopes() {
        let call = request(json!({"target":"nacre","method":"launcher","args":["a"]})).unwrap();
        assert_eq!(call.target, "nacre");
        assert_eq!(call.args, vec![json!("a")]);
        assert!(request(json!({"target":"bead.panel","method":"open","args":[]})).is_err());
        assert!(request(json!({"target":"nacre","method":"launcher","args":[{}]})).is_err());
        assert!(request(json!({"target":"nacre","method":"missing","args":[]})).is_err());
        assert!(
            request(json!({"target":"nacre","method":"launcher","args":["x".repeat(65537)]}))
                .is_err()
        );
    }
}
