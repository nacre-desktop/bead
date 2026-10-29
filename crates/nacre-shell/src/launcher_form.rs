// SPDX-License-Identifier: GPL-3.0-only
//! B0199: the launcher's inline forms. QML drew these inside the launcher (`NacreMenuForm`);
//! Bead draws them from `shell/ui/components/form.luau` and keeps only the part that has to
//! be Rust here: which rows open a form, the values a form starts with, validation, and the
//! backend each form submits to (the same helper commands and daemon methods QML used).
//!
//! The provider (`launcher_actions::act`) answers a form row with a view whose route is
//! `form:<kind>` and whose query is the confirmation text; `launcher::activation_changes`
//! turns that into the `launcher.form.*` stores. Nothing else ever sees the route.
use crate::i18n::tr;
use bead::core::Value;
use serde_json::{Value as Json, json};
use std::{collections::BTreeMap, io::Write, process::Stdio, sync::Mutex};

/// The route prefix that marks a form view in an activation reply.
pub(crate) const ROUTE: &str = "form:";
/// The first text field's node id (`form.luau`): where focus goes when a form with text opens.
pub(crate) const FIRST_FIELD: &str = "launcher.form.form-fields.field-1.f1";
/// Text slots a form can fill (`launcher.form.v1` ... `v4`).
pub(crate) const SLOTS: usize = 4;

/// The form a row opens, if Bead draws one for it.
pub(crate) fn kind_for(operation: &str, value: &str) -> Option<&'static str> {
    Some(match (operation, value) {
        ("form", "reminder") => "reminder",
        ("form", "tui") => "tui",
        ("form", "webapp") => "webapp",
        ("form", "software-theme-import") => "theme-import",
        ("form", "software-extension-import") => "extension-import",
        ("tool", "record-start") => "recording-area",
        ("tool", "record-screen") => "recording-screen",
        ("tool", "install-mise") => "confirm-mise",
        ("maintenance", "restart") => "confirm-restart",
        _ => return None,
    })
}
fn has_text_field(kind: &str) -> bool {
    matches!(
        kind,
        "reminder" | "webapp" | "tui" | "theme-import" | "extension-import"
    )
}
fn store(key: &str, value: Value) -> (String, Value) {
    (format!("launcher.form.{key}"), value)
}
fn text(value: &str) -> Value {
    Value::Text(value.into())
}
/// The stores for a freshly opened form: its starting values (what QML pre-filled).
pub(crate) fn open(kind: &str, message: &str) -> BTreeMap<String, Value> {
    let initial: [&str; SLOTS] = match kind {
        "reminder" => ["25", "", "", ""],
        "tui" => ["", "htop", "utilities-terminal", "kitty"],
        "recording-area" | "recording-screen" => ["none", "", "", ""],
        _ => ["", "", "", ""],
    };
    let mut changes = BTreeMap::from([
        store("kind", text(kind)),
        store("message", text(message)),
        store("error", text("")),
        store("busy", Value::Bool(false)),
        ("launcher.selection".into(), text("")),
        (
            "bead.focused".into(),
            // The first field; the confirmations and the sound choice have none.
            text(if has_text_field(kind) {
                FIRST_FIELD
            } else {
                ""
            }),
        ),
    ]);
    for (index, value) in initial.iter().enumerate() {
        changes.extend([store(&format!("v{}", index + 1), text(value))]);
    }
    changes
}
/// Back to the launcher's results.
pub(crate) fn close() -> BTreeMap<String, Value> {
    BTreeMap::from([
        store("kind", text("")),
        store("error", text("")),
        store("busy", Value::Bool(false)),
        ("bead.focused".into(), text("launcher.search")),
    ])
}

/// What a valid form runs. All of it happens off the UI thread.
#[derive(Debug, PartialEq)]
pub(crate) enum Work {
    /// A helper command with its JSON document on stdin; its `message` or `error` is the answer.
    Helper { argv: Vec<String>, input: Json },
    /// A published daemon method, after the launcher has closed (QML `captureDelay`).
    Daemon { method: &'static str, params: Json },
    /// A visible terminal; fire and forget.
    Terminal(Vec<String>),
}
impl Work {
    /// A recording and a confirmed command close the launcher with the form (QML did); a
    /// helper answer is shown in it.
    pub(crate) fn closes_launcher(&self) -> bool {
        !matches!(self, Work::Helper { .. })
    }
}

/// Validates the slots of a form and says what to run. The `Err` text is shown in the form.
pub(crate) fn work(kind: &str, v: &[String; SLOTS]) -> Result<Work, String> {
    let trimmed = |i: usize| v[i].trim().to_owned();
    let helper = |args: &[&str], input: Json| Work::Helper {
        argv: crate::launcher_actions::helper_argv(args),
        input,
    };
    match kind {
        "reminder" => {
            let minutes = trimmed(0).parse::<f64>().ok();
            if !minutes.is_some_and(|m| (0.1..=1440.).contains(&m)) || trimmed(1).is_empty() {
                return Err(tr("bead.launcher.form-error-reminder", &[]));
            }
            Ok(helper(
                &["menu-backend", "reminder", ""],
                json!({"minutes": trimmed(0), "message": v[1]}),
            ))
        }
        "webapp" => {
            let url = trimmed(1);
            if trimmed(0).is_empty() || !(url.starts_with("https://") || url.starts_with("http://"))
            {
                return Err(tr("bead.launcher.form-error-webapp", &[]));
            }
            Ok(helper(
                &["menu-backend", "webapp-create", ""],
                json!({"label": v[0], "url": url}),
            ))
        }
        "tui" => {
            let command: Vec<&str> = v[1].split('\n').filter(|line| !line.is_empty()).collect();
            if trimmed(0).is_empty()
                || command.is_empty()
                || !matches!(v[3].as_str(), "kitty" | "foot" | "alacritty" | "wezterm")
            {
                return Err(tr("bead.launcher.form-error-tui", &[]));
            }
            Ok(helper(
                &["software-library", "tui-create", ""],
                json!({"name": v[0], "command": command, "icon": v[2], "terminal": v[3]}),
            ))
        }
        "theme-import" | "extension-import" => {
            let (action, field, error) = if kind == "theme-import" {
                ("theme-import", "path", "bead.launcher.form-error-path")
            } else {
                ("import", "url", "bead.launcher.form-error-url")
            };
            if trimmed(0).is_empty() {
                return Err(tr(error, &[]));
            }
            Ok(helper(
                &["software-library", action, ""],
                json!({ field: trimmed(0) }),
            ))
        }
        "recording-area" | "recording-screen" => {
            let audio = v[0].as_str();
            if !matches!(audio, "none" | "system" | "mixed") {
                return Err(tr("bead.launcher.form-error-audio", &[]));
            }
            let mode = if kind == "recording-screen" {
                "screen"
            } else {
                "region"
            };
            let mut params = json!({ "mode": mode });
            // Absent means no sound: a daemon without the option still records.
            if audio != "none" {
                params["audio"] = json!(audio);
            }
            Ok(Work::Daemon {
                method: "recorder.start",
                params,
            })
        }
        "confirm-restart" | "confirm-mise" => Ok(Work::Terminal(
            crate::launcher_actions::confirmed_argv(kind).ok_or("unknown form")?,
        )),
        _ => Err("unknown form".into()),
    }
}

/// `stdout` of a helper that answers one JSON document: `message` on success, `error` on failure.
pub(crate) fn helper_answer(stdout: &str, success: bool) -> Result<String, String> {
    let value: Json = serde_json::from_str(stdout.trim()).unwrap_or(Json::Null);
    if let Some(error) = value["error"].as_str().filter(|e| !e.is_empty()) {
        return Err(error.to_owned());
    }
    if !success {
        return Err(tr("bead.launcher.form-error-failed", &[]));
    }
    Ok(value["message"].as_str().unwrap_or_default().to_owned())
}
/// Runs a helper to completion with its document on stdin.
pub(crate) fn run_helper(argv: &[String], input: &Json) -> Result<String, String> {
    let mut child = bead::services::process::command(&argv[0])
        .args(&argv[1..])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("{}: {e}", argv[0]))?;
    if let Some(mut stdin) = child.stdin.take() {
        let _ = writeln!(stdin, "{input}");
    }
    let output = child.wait_with_output().map_err(|e| e.to_string())?;
    helper_answer(
        &String::from_utf8_lossy(&output.stdout),
        output.status.success(),
    )
}

/// The one helper submit in flight and its answer (`message` or `error`).
#[derive(Default)]
pub(crate) struct Run {
    done: Mutex<Option<Result<String, String>>>,
}
impl Run {
    pub(crate) fn finish(&self, result: Result<String, String>) {
        if let Ok(mut slot) = self.done.lock() {
            *slot = Some(result);
        }
    }
    pub(crate) fn take(&self) -> Option<Result<String, String>> {
        self.done.lock().ok().and_then(|mut slot| slot.take())
    }
}
/// The stores a finished helper submit changes, and the message the launcher then shows.
pub(crate) fn finished(
    result: Result<String, String>,
) -> (BTreeMap<String, Value>, Option<String>) {
    match result {
        Err(error) => (
            BTreeMap::from([
                store("error", text(&error)),
                store("busy", Value::Bool(false)),
            ]),
            None,
        ),
        Ok(message) => (close(), Some(message)),
    }
}

#[cfg(test)]
#[path = "launcher_form_tests.rs"]
mod tests;
