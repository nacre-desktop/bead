// SPDX-License-Identifier: GPL-3.0-only
//! Official niri-ipc types and persistent command socket; event stream is separate.
use super::{Shared, epoch, protocol};
use bead::backends::{
    Action, Capabilities, Direction, Error, Event as BackendEvent, Id, Output, Snapshot, Window,
    Workspace, stable_id,
};
use niri_ipc::{
    Action as NativeAction, Event, Request, Response, WorkspaceReferenceArg, socket::Socket,
};
use std::{
    collections::{BTreeMap, HashMap},
    io::{BufRead, BufReader, Write},
    os::unix::net::UnixStream,
    path::PathBuf,
    sync::{Arc, Mutex, atomic::Ordering},
    thread::{self, JoinHandle},
    time::Duration,
};
#[derive(Clone)]
pub struct Transport {
    path: PathBuf,
    command: Arc<Mutex<Option<Socket>>>,
}
impl Transport {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            command: Arc::new(Mutex::new(None)),
        }
    }
    pub fn request(&self, request: Request) -> Result<Response, Error> {
        let mut socket = self.command.lock().map_err(protocol)?;
        if socket.is_none() {
            *socket = Some(
                Socket::connect_to(&self.path).map_err(|e| Error::Disconnected(e.to_string()))?,
            );
        }
        match socket
            .as_mut()
            .ok_or_else(|| protocol("command socket missing"))?
            .send(request)
        {
            Ok(reply) => {
                super::record(
                    "niri-reply",
                    serde_json::to_value(&reply).map_err(protocol)?,
                );
                reply.map_err(protocol)
            }
            Err(error) => {
                *socket = None;
                Err(Error::Disconnected(error.to_string()))
            }
        }
    }
    fn outputs(&self) -> Result<HashMap<String, niri_ipc::Output>, Error> {
        match self.request(Request::Outputs)? {
            Response::Outputs(outputs) => Ok(outputs),
            _ => Err(protocol("unexpected Outputs reply")),
        }
    }
    pub fn action(&self, snapshot: &Snapshot, action: Action) -> Result<(), Error> {
        let native = encode_action(snapshot, &action)?;
        match self.request(Request::Action(native))? {
            Response::Handled => Ok(()),
            other => Err(protocol(format!("unexpected action reply {other:?}"))),
        }
    }
}
#[derive(Default)]
pub struct Reducer {
    pub epoch: u64,
    pub outputs: HashMap<String, niri_ipc::Output>,
    pub workspaces: BTreeMap<u64, niri_ipc::Workspace>,
    pub windows: BTreeMap<u64, niri_ipc::Window>,
    pub(crate) workspace_ready: bool,
    pub(crate) window_ready: bool,
}
impl Reducer {
    pub fn apply(&mut self, event: Event) -> bool {
        match event {
            Event::WorkspacesChanged { workspaces } => {
                self.workspaces = workspaces.into_iter().map(|w| (w.id, w)).collect();
                self.workspace_ready = true;
                true
            }
            Event::WorkspaceActivated { id, focused } => {
                let output = self.workspaces.get(&id).and_then(|w| w.output.clone());
                for w in self.workspaces.values_mut() {
                    if w.output == output {
                        w.is_active = w.id == id;
                    }
                    if focused {
                        w.is_focused = w.id == id;
                    }
                }
                true
            }
            Event::WorkspaceUrgencyChanged { id, urgent } => {
                if let Some(w) = self.workspaces.get_mut(&id) {
                    w.is_urgent = urgent;
                }
                true
            }
            Event::WindowsChanged { windows } => {
                self.windows = windows.into_iter().map(|w| (w.id, w)).collect();
                self.window_ready = true;
                true
            }
            Event::WindowOpenedOrChanged { window } => {
                if window.is_focused {
                    for w in self.windows.values_mut() {
                        w.is_focused = false;
                    }
                }
                self.windows.insert(window.id, window);
                true
            }
            Event::WindowClosed { id } => {
                self.windows.remove(&id);
                true
            }
            Event::WindowFocusChanged { id } => {
                for w in self.windows.values_mut() {
                    w.is_focused = Some(w.id) == id;
                }
                true
            }
            Event::WindowUrgencyChanged { id, urgent } => {
                if let Some(w) = self.windows.get_mut(&id) {
                    w.is_urgent = urgent;
                }
                true
            }
            _ => false,
        }
    }
    pub fn snapshot(&self) -> Snapshot {
        let mut snapshot = Snapshot {
            backend: "niri".into(),
            epoch: self.epoch,
            connected: self.workspace_ready && self.window_ready,
            capabilities: Capabilities::empty(),
            ..Snapshot::default()
        };
        for name in [
            "perOutputWorkspaces",
            "scrollingLayout",
            "floatingPlacement",
        ] {
            snapshot.capabilities.features.insert(name.into(), true);
        }
        let focused_output = self
            .workspaces
            .values()
            .find(|w| w.is_focused)
            .and_then(|w| w.output.as_deref());
        for output in self.outputs.values() {
            let mode = output.current_mode.and_then(|i| output.modes.get(i));
            let logical = output.logical.as_ref();
            snapshot.outputs.push(Output {
                id: stable_id("niri", self.epoch, "output", &output.name),
                name: output.name.clone(),
                width: mode.map_or(0, |m| u32::from(m.width)),
                height: mode.map_or(0, |m| u32::from(m.height)),
                scale: logical.map_or(1., |l| l.scale),
                refresh_mhz: mode.map_or(0, |m| m.refresh_rate),
                x: logical.map_or(0, |l| l.x),
                y: logical.map_or(0, |l| l.y),
                focused: focused_output == Some(output.name.as_str()),
            });
        }
        snapshot.outputs.sort_by(|a, b| a.name.cmp(&b.name));
        for w in self.workspaces.values() {
            snapshot.workspaces.push(Workspace {
                id: stable_id("niri", self.epoch, "workspace", w.id),
                name: w.name.clone().unwrap_or_else(|| w.idx.to_string()),
                index: i64::from(w.idx),
                output: w
                    .output
                    .as_ref()
                    .map(|name| stable_id("niri", self.epoch, "output", name)),
                active: w.is_active,
                focused: w.is_focused,
                urgent: w.is_urgent,
                fixed_slot: w.name.is_some(),
                visiting: false,
            });
        }
        snapshot
            .workspaces
            .sort_by(|a, b| a.output.cmp(&b.output).then(a.index.cmp(&b.index)));
        for w in self.windows.values() {
            let workspace = w
                .workspace_id
                .map(|id| stable_id("niri", self.epoch, "workspace", id));
            let output = w
                .workspace_id
                .and_then(|id| self.workspaces.get(&id))
                .and_then(|w| w.output.as_ref())
                .map(|name| stable_id("niri", self.epoch, "output", name));
            snapshot.windows.push(Window {
                id: stable_id("niri", self.epoch, "window", w.id),
                title: w.title.clone().unwrap_or_default(),
                app_id: w.app_id.clone().unwrap_or_default(),
                workspace,
                output,
                focused: w.is_focused,
                floating: w.is_floating,
                fullscreen: None,
                urgent: w.is_urgent,
            });
        }
        snapshot
    }
}
fn native(snapshot: &Snapshot, id: &Id, kind: &str) -> Result<u64, Error> {
    id.0.strip_prefix(&format!("niri:{}:{kind}:", snapshot.epoch))
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| Error::UnknownId(id.0.clone()))
}
pub fn encode_action(snapshot: &Snapshot, action: &Action) -> Result<NativeAction, Error> {
    let window = |id: &Id| {
        if snapshot.windows.iter().any(|w| &w.id == id) {
            native(snapshot, id, "window")
        } else {
            Err(Error::UnknownId(id.0.clone()))
        }
    };
    let workspace = |id: &Id| {
        if snapshot.workspaces.iter().any(|w| &w.id == id) {
            native(snapshot, id, "workspace")
        } else {
            Err(Error::UnknownId(id.0.clone()))
        }
    };
    let output = |id: &Id| {
        snapshot
            .outputs
            .iter()
            .find(|o| &o.id == id)
            .map(|o| o.name.clone())
            .ok_or_else(|| Error::UnknownId(id.0.clone()))
    };
    Ok(match action {
        Action::FocusWindow { window: id } => NativeAction::FocusWindow { id: window(id)? },
        Action::CloseWindow { window: id } => NativeAction::CloseWindow {
            id: Some(window(id)?),
        },
        Action::SwitchWorkspace { workspace: id } => NativeAction::FocusWorkspace {
            reference: WorkspaceReferenceArg::Id(workspace(id)?),
        },
        Action::MoveWindow {
            window: id,
            workspace: ws,
            follow,
        } => NativeAction::MoveWindowToWorkspace {
            window_id: Some(window(id)?),
            reference: WorkspaceReferenceArg::Id(workspace(ws)?),
            focus: *follow,
        },
        Action::MoveWindowToOutput {
            window: id,
            output: id2,
            follow: true,
        } => NativeAction::MoveWindowToMonitor {
            id: Some(window(id)?),
            output: output(id2)?,
        },
        Action::MoveWindowToOutput { follow: false, .. } => {
            return Err(Error::Unsupported {
                operation: "move-window-to-output without follow".into(),
                backend: "niri".into(),
            });
        }
        Action::FocusOutput { output: id } => NativeAction::FocusMonitor {
            output: output(id)?,
        },
        Action::FocusDirection { direction } => match direction {
            Direction::Left => NativeAction::FocusColumnLeft {},
            Direction::Right => NativeAction::FocusColumnRight {},
            Direction::Up => NativeAction::FocusWindowUp {},
            Direction::Down => NativeAction::FocusWindowDown {},
        },
        Action::SetFloating {
            window: id,
            floating: true,
        } => NativeAction::MoveWindowToFloating {
            id: Some(window(id)?),
        },
        Action::SetFloating {
            window: id,
            floating: false,
        } => NativeAction::MoveWindowToTiling {
            id: Some(window(id)?),
        },
        other => {
            return Err(Error::Unsupported {
                operation: match other {
                    Action::WindowTrash { .. } => "windowTrash",
                    Action::Scratchpad { .. } => "scratchpad",
                    Action::WindowCapture { .. } => "windowCapture",
                    Action::MonitorMirroring { .. } => "monitorMirroring",
                    Action::SetFullscreen { .. } => "fullscreenState",
                    _ => "action",
                }
                .into(),
                backend: "niri".into(),
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
                let mut stream = UnixStream::connect(&transport.path)
                    .map_err(|e| Error::Disconnected(e.to_string()))?;
                *shared
                    .event_socket
                    .lock()
                    .unwrap_or_else(crate::backends::poisoned) =
                    Some(stream.try_clone().map_err(protocol)?);
                // Wire types/serialization are provided by niri-ipc; the cancellable reader uses std I/O.
                stream
                    .write_all(
                        format!(
                            "{}\n",
                            serde_json::to_string(&Request::EventStream).map_err(protocol)?
                        )
                        .as_bytes(),
                    )
                    .map_err(protocol)?;
                let mut reader = BufReader::new(stream);
                let mut line = String::new();
                reader.read_line(&mut line).map_err(protocol)?;
                let reply: niri_ipc::Reply = serde_json::from_str(&line).map_err(protocol)?;
                reply.map_err(protocol)?;
                let mut reducer = Reducer {
                    epoch: epoch(&transport.path),
                    outputs: transport.outputs()?,
                    ..Reducer::default()
                };
                attempt = 0;
                while !shared.stop.load(Ordering::SeqCst) {
                    line.clear();
                    let n = reader.read_line(&mut line).map_err(protocol)?;
                    if n == 0 {
                        return Err(Error::Disconnected("niri event stream EOF".into()));
                    }
                    if line.len() > 4 * 1024 * 1024 {
                        return Err(protocol("niri event too large"));
                    }
                    let event: Event = serde_json::from_str(&line).map_err(protocol)?;
                    super::record(
                        "niri-event",
                        serde_json::to_value(&event).map_err(protocol)?,
                    );
                    let topology = matches!(
                        event,
                        Event::WorkspacesChanged { .. } | Event::ConfigLoaded { .. }
                    );
                    if reducer.apply(event) || topology {
                        if topology {
                            reducer.outputs = transport.outputs()?;
                        }
                        shared.publish(reducer.snapshot());
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
            *transport
                .command
                .lock()
                .unwrap_or_else(crate::backends::poisoned) = None;
            attempt += 1;
            shared.emit(BackendEvent::Reconnecting { attempt });
            thread::sleep(Duration::from_millis((100u64 << attempt.min(4)).min(2000)));
        }
    })
}
