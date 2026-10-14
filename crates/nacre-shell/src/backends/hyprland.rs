// SPDX-License-Identifier: GPL-3.0-only
use super::{Shared, epoch, protocol};
use bead_backends::{
    Action, Capabilities, Direction, Error, Event, Id, Output, Snapshot, Window, Workspace,
    stable_id,
};
use serde_json::Value;
use std::{
    io::{BufRead, BufReader, Read, Write},
    os::unix::net::UnixStream,
    path::PathBuf,
    sync::{Arc, atomic::Ordering},
    thread::{self, JoinHandle},
    time::Duration,
};
#[derive(Clone)]
pub struct Transport {
    directory: PathBuf,
}
impl Transport {
    pub fn new(directory: PathBuf) -> Self {
        Self { directory }
    }
    pub fn request(&self, command: &str) -> Result<String, Error> {
        super::trace("request_begin");
        let mut socket = UnixStream::connect(self.directory.join(".socket.sock"))
            .map_err(|e| Error::Disconnected(e.to_string()))?;
        socket
            .set_read_timeout(Some(Duration::from_secs(2)))
            .map_err(protocol)?;
        socket
            .set_write_timeout(Some(Duration::from_secs(2)))
            .map_err(protocol)?;
        socket.write_all(command.as_bytes()).map_err(protocol)?;
        let mut result = String::new();
        socket
            .take(4 * 1024 * 1024)
            .read_to_string(&mut result)
            .map_err(protocol)?;
        super::trace("request_reply");
        super::record(
            "hyprland-reply",
            serde_json::json!({"command":command,"reply":result}),
        );
        super::trace("request_recorded");
        Ok(result)
    }
    pub fn snapshot(&self) -> Result<Snapshot, Error> {
        let epoch = epoch(&self.directory.join(".socket.sock"));
        // Socket1 batches preserve command order. Decode a stream of JSON
        // documents, rather than splitting on whitespace inside pretty JSON.
        // The event worker owns this round trip; drawing never waits for it.
        let reply = self.request("[[BATCH]]j/monitors;j/workspaces;j/clients;j/activewindow")?;
        normalize_batch(epoch, &reply)
    }
    pub fn action(&self, snapshot: &Snapshot, action: Action) -> Result<(), Error> {
        let expression = encode_action(snapshot, &action)?;
        let reply = self.request(&format!("/eval hl.dispatch({expression})"))?;
        if reply.trim() == "ok" {
            Ok(())
        } else {
            Err(protocol(format!("unexpected action reply: {reply}")))
        }
    }
}
pub(super) fn normalize_batch(epoch: u64, reply: &str) -> Result<Snapshot, Error> {
    let documents = serde_json::Deserializer::from_str(reply)
        .into_iter::<Value>()
        .take(5) // Bound reply count too; a fifth document is always invalid.
        .collect::<Result<Vec<_>, _>>()
        .map_err(protocol)?;
    let [monitors, workspaces, windows, active]: [Value; 4] = documents
        .try_into()
        .map_err(|_| protocol("snapshot batch must contain exactly four JSON replies"))?;
    if !active.is_object() {
        return Err(protocol("activewindow reply must be an object"));
    }
    normalize(epoch, &monitors, &workspaces, &windows, &active)
}
fn string<'a>(v: &'a Value, key: &str) -> &'a str {
    v[key].as_str().unwrap_or("")
}
pub fn normalize(
    epoch: u64,
    monitors: &Value,
    workspaces: &Value,
    windows: &Value,
    active: &Value,
) -> Result<Snapshot, Error> {
    let mut snapshot = Snapshot {
        backend: "hyprland".into(),
        epoch,
        connected: true,
        capabilities: Capabilities::empty(),
        ..Snapshot::default()
    };
    for name in [
        "stableWorkspaceIndices",
        "globalWorkspaces",
        "floatingPlacement",
        "fullscreenState",
    ] {
        snapshot.capabilities.features.insert(name.into(), true);
    }
    for m in monitors
        .as_array()
        .ok_or_else(|| protocol("monitors not array"))?
    {
        let name = string(m, "name");
        snapshot.outputs.push(Output {
            id: stable_id("hyprland", epoch, "output", name),
            name: name.into(),
            width: m["width"].as_u64().unwrap_or(0) as u32,
            height: m["height"].as_u64().unwrap_or(0) as u32,
            scale: m["scale"].as_f64().unwrap_or(1.),
            refresh_mhz: (m["refreshRate"].as_f64().unwrap_or(0.) * 1000.) as u32,
            x: m["x"].as_i64().unwrap_or(0) as i32,
            y: m["y"].as_i64().unwrap_or(0) as i32,
            focused: m["focused"].as_bool().unwrap_or(false),
        });
    }
    for w in workspaces
        .as_array()
        .ok_or_else(|| protocol("workspaces not array"))?
    {
        let index = w["id"]
            .as_i64()
            .ok_or_else(|| protocol("workspace id missing"))?;
        let output_name = string(w, "monitor");
        let monitor = monitors
            .as_array()
            .ok_or_else(|| protocol("monitors not array"))?
            .iter()
            .find(|m| string(m, "name") == output_name);
        let active_ws = monitor.is_some_and(|m| {
            m["activeWorkspace"]["id"].as_i64() == Some(index)
                || m["specialWorkspace"]["id"].as_i64() == Some(index)
        });
        snapshot.workspaces.push(Workspace {
            id: stable_id("hyprland", epoch, "workspace", index),
            name: string(w, "name").into(),
            index,
            output: snapshot
                .outputs
                .iter()
                .find(|o| o.name == output_name)
                .map(|o| o.id.clone()),
            active: active_ws,
            focused: active_ws && monitor.is_some_and(|m| m["focused"] == true),
            urgent: false,
        });
    }
    for w in windows
        .as_array()
        .ok_or_else(|| protocol("clients not array"))?
    {
        let address = string(w, "address");
        if address.is_empty() {
            continue;
        }
        let workspace = w["workspace"]["id"]
            .as_i64()
            .map(|id| stable_id("hyprland", epoch, "workspace", id));
        let output = workspace
            .as_ref()
            .and_then(|id| snapshot.workspaces.iter().find(|ws| &ws.id == id))
            .and_then(|ws| ws.output.clone());
        snapshot.windows.push(Window {
            id: stable_id("hyprland", epoch, "window", address),
            title: string(w, "title").into(),
            app_id: string(w, "class").into(),
            workspace,
            output,
            focused: address == string(active, "address"),
            floating: w["floating"].as_bool().unwrap_or(false),
            fullscreen: w["fullscreen"].as_u64().map(|state| state != 0),
            urgent: w["urgent"].as_bool().unwrap_or(false),
        });
    }
    snapshot.workspaces.sort_by_key(|w| w.index);
    Ok(snapshot)
}
fn native(snapshot: &Snapshot, id: &Id, kind: &str) -> Result<String, Error> {
    let prefix = format!("hyprland:{}:{kind}:", snapshot.epoch);
    id.0.strip_prefix(&prefix)
        .map(str::to_owned)
        .ok_or_else(|| Error::UnknownId(id.0.clone()))
}
fn quote(value: &str) -> String {
    let mut output = String::from("\"");
    for ch in value.chars() {
        match ch {
            '\\' => output.push_str("\\\\"),
            '"' => output.push_str("\\\""),
            c if c.is_control() => output.push_str(&format!("\\{:03}", u32::from(c))),
            c => output.push(c),
        }
    }
    output.push('"');
    output
}
pub fn encode_action(snapshot: &Snapshot, action: &Action) -> Result<String, Error> {
    let window = |id: &Id| -> Result<String, Error> {
        if !snapshot.windows.iter().any(|w| &w.id == id) {
            return Err(Error::UnknownId(id.0.clone()));
        }
        let value = native(snapshot, id, "window")?;
        if !value.starts_with("0x") || !value[2..].chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(protocol("invalid native address"));
        }
        Ok(quote(&format!("address:{value}")))
    };
    let workspace = |id: &Id| -> Result<String, Error> {
        if !snapshot.workspaces.iter().any(|w| &w.id == id) {
            return Err(Error::UnknownId(id.0.clone()));
        }
        let value = native(snapshot, id, "workspace")?;
        value.parse::<i64>().map_err(protocol)?;
        Ok(value)
    };
    let output = |id: &Id| -> Result<String, Error> {
        snapshot
            .outputs
            .iter()
            .find(|o| &o.id == id)
            .map(|o| quote(&o.name))
            .ok_or_else(|| Error::UnknownId(id.0.clone()))
    };
    Ok(match action {
        Action::FocusWindow { window: id } => format!("hl.dsp.focus({{window={}}})", window(id)?),
        Action::CloseWindow { window: id } => {
            format!("hl.dsp.window.close({{window={}}})", window(id)?)
        }
        Action::SwitchWorkspace { workspace: id } => {
            format!("hl.dsp.focus({{workspace={}}})", workspace(id)?)
        }
        Action::MoveWindow {
            window: id,
            workspace: ws,
            follow,
        } => format!(
            "hl.dsp.window.move({{window={},workspace={},follow={follow}}})",
            window(id)?,
            workspace(ws)?
        ),
        Action::MoveWindowToOutput {
            window: id,
            output: id2,
            follow,
        } => format!(
            "hl.dsp.window.move({{window={},monitor={},follow={follow}}})",
            window(id)?,
            output(id2)?
        ),
        Action::FocusOutput { output: id } => format!("hl.dsp.focus({{monitor={}}})", output(id)?),
        Action::FocusDirection { direction } => format!(
            "hl.dsp.focus({{direction={}}})",
            quote(match direction {
                Direction::Left => "left",
                Direction::Right => "right",
                Direction::Up => "up",
                Direction::Down => "down",
            })
        ),
        Action::SetFloating {
            window: id,
            floating,
        } => format!(
            "hl.dsp.window.float({{window={},action={}}})",
            window(id)?,
            quote(if *floating { "set" } else { "unset" })
        ),
        Action::SetFullscreen {
            window: id,
            fullscreen,
        } => format!(
            "hl.dsp.window.fullscreen_state({{window={},internal={},client={},action=\"set\"}})",
            window(id)?,
            if *fullscreen { 2 } else { 0 },
            if *fullscreen { 2 } else { 0 }
        ),
        other => {
            return Err(Error::Unsupported {
                operation: match other {
                    Action::WindowTrash { .. } => "windowTrash",
                    Action::Scratchpad { .. } => "scratchpad",
                    Action::WindowCapture { .. } => "windowCapture",
                    Action::MonitorMirroring { .. } => "monitorMirroring",
                    _ => "action",
                }
                .into(),
                backend: "hyprland".into(),
            });
        }
    })
}
pub fn spawn(transport: Transport, shared: Arc<Shared>) -> JoinHandle<()> {
    thread::spawn(move || {
        super::worker_signals();
        let mut attempt = 0;
        while !shared.stop.load(Ordering::SeqCst) {
            let result = (|| -> Result<(), Error> {
                let stream = UnixStream::connect(transport.directory.join(".socket2.sock"))
                    .map_err(|e| Error::Disconnected(e.to_string()))?;
                *shared
                    .event_socket
                    .lock()
                    .unwrap_or_else(crate::backends::poisoned) =
                    Some(stream.try_clone().map_err(protocol)?);
                shared.publish(transport.snapshot()?);
                attempt = 0;
                let mut reader = BufReader::new(stream);
                let mut line = String::new();
                while !shared.stop.load(Ordering::SeqCst) {
                    line.clear();
                    let n = reader.read_line(&mut line).map_err(protocol)?;
                    if n == 0 {
                        return Err(Error::Disconnected("event socket EOF".into()));
                    }
                    if line.len() > 65536 {
                        return Err(protocol("event line too large"));
                    }
                    super::trace("event_received");
                    super::record("hyprland-event", serde_json::json!(line.trim_end()));
                    let name = line.split_once(">>").map(|v| v.0).unwrap_or("");
                    if relevant(name) {
                        shared.publish(transport.snapshot()?);
                    }
                }
                Ok(())
            })();
            if shared.stop.load(Ordering::SeqCst) {
                break;
            }
            if let Err(error) = result {
                shared.disconnected(error.to_string());
            }
            attempt += 1;
            shared.emit(Event::Reconnecting { attempt });
            thread::sleep(Duration::from_millis((100u64 << attempt.min(4)).min(2000)));
        }
    })
}
pub fn relevant(name: &str) -> bool {
    [
        "workspace",
        "workspacev2",
        "focusedmon",
        "focusedmonv2",
        "activewindow",
        "activewindowv2",
        "openwindow",
        "closewindow",
        "movewindow",
        "movewindowv2",
        "windowtitle",
        "windowtitlev2",
        "monitoradded",
        "monitoraddedv2",
        "monitorremoved",
        "createworkspace",
        "createworkspacev2",
        "destroyworkspace",
        "destroyworkspacev2",
        "renameworkspace",
        "changefloatingmode",
        "fullscreen",
        "urgent",
        "configreloaded",
    ]
    .contains(&name)
}
