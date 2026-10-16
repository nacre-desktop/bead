// SPDX-License-Identifier: GPL-3.0-only
//! Daemon UI calls enter the same audited local command path as the legacy CLI.
use bead::core::source::{self, Error};
use serde::Deserialize;
use serde_json::Value;
use std::{collections::BTreeMap, path::PathBuf, sync::Arc, time::Duration};

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
    let mut config = crate::nacre_source::Config::new(daemon, vec!["menu".into()]);
    config.provider = Some((
        "ui".into(),
        BTreeMap::from([("ui.dispatch".into(), 1)]),
        Arc::new(move |method, params| {
            let shell = shell.clone();
            let notifications = notifications.clone();
            Box::pin(async move {
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
    crate::nacre_source::Client::start(config)
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
