// SPDX-License-Identifier: GPL-3.0-only
//! Daemon UI calls enter the same audited local command path as the legacy CLI.
use bead::core::source::{self, Error};
use serde::Deserialize;
use serde_json::Value;
use std::{
    collections::BTreeMap,
    path::PathBuf,
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
/// `notifications` is the command socket of a separate private notice process (the
/// `--notification-bus` compatibility fixture); without it `notifs` calls go to this
/// shell. `extra` topics are subscribed besides the shell's own for services sharing this link.
pub fn start(
    daemon: PathBuf,
    shell: PathBuf,
    notifications: Option<PathBuf>,
    extra: &[&str],
) -> source::Result<crate::nacre_source::Client> {
    if !shell.is_absolute() || notifications.as_ref().is_some_and(|p| !p.is_absolute()) {
        return Err(Error::new(
            "validation",
            "UI sockets must be explicit absolute paths",
            false,
        ));
    }
    let topics = [
        "daemon",
        "menu",
        "tray",
        "i18n",
        crate::prefs::DESKTOP,
        crate::prefs::DASHBOARD,
        crate::prefs::PROFILE,
        crate::prefs::LAUNCHER_LAYOUT,
    ];
    let mut config = crate::nacre_source::Config::new(
        daemon,
        topics
            .iter()
            .copied()
            .chain(extra.iter().copied())
            .map(String::from)
            .collect(),
    );
    // `ai.chat` `send_failed` events restore the sidebar composer (B0043); `osd`
    // events drive the on-screen display (B0051).
    config.event = Some(Arc::new(|message| {
        crate::chat::route_event(message);
        crate::notice_service::route_event(message);
    }));
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
                    let (act, reply) = crate::launcher_actions::plan(params)?;
                    let client = slot.get().and_then(|c| c.upgrade());
                    tokio::spawn(crate::launcher_actions::execute(act, client, shell));
                    return serde_json::to_value(reply)
                        .map_err(|_| Error::new("internal", "Invalid launcher reply", false));
                }
                if method != "ui.dispatch" {
                    return Err(Error::new("unknown_method", "UI method unavailable", false));
                }
                let call = request(params)?;
                let socket = match (call.target.as_str(), notifications) {
                    ("notifs", Some(socket)) => socket,
                    _ => shell,
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
