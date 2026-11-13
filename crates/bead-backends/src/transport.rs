// SPDX-License-Identifier: MIT
use super::*;
use serde_json::{Value, json};
use std::{
    ffi::OsString,
    io::{BufRead, BufReader, Read, Write},
    os::unix::net::UnixStream,
    path::{Path, PathBuf},
    sync::{
        Condvar, Mutex, Weak,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    thread,
    time::Duration,
};

type Listener = Arc<dyn Fn(Event) + Send + Sync>;

struct Inner {
    backend: String,
    path: PathBuf,
    state: Mutex<Snapshot>,
    callbacks: Mutex<Vec<(u64, Listener)>>,
    next: AtomicU64,
    stopping: AtomicBool,
    stream: Mutex<Option<UnixStream>>,
    backoff: (Mutex<()>, Condvar),
}
struct Client(Arc<Inner>, Option<thread::JoinHandle<()>>);
pub fn connect(backend: &str, path: PathBuf) -> Arc<dyn Backend> {
    let s = Snapshot {
        backend: backend.into(),
        epoch: 0,
        connected: false,
        capabilities: capabilities(backend),
        ..Snapshot::default()
    };
    let inner = Arc::new(Inner {
        backend: backend.into(),
        path,
        state: Mutex::new(s),
        callbacks: Mutex::new(vec![]),
        next: AtomicU64::new(1),
        stopping: AtomicBool::new(false),
        stream: Mutex::new(None),
        backoff: (Mutex::new(()), Condvar::new()),
    });
    let weak = Arc::downgrade(&inner);
    let worker = thread::Builder::new()
        .name(format!("bead-{backend}-ipc"))
        .spawn(move || worker(weak))
        .expect("spawn compositor IPC worker");
    Arc::new(Client(inner, Some(worker)))
}
fn selection(
    niri: Option<OsString>,
    hypr: Option<OsString>,
    runtime: Option<OsString>,
) -> Option<(String, PathBuf)> {
    if let Some(path) = niri.filter(|s| !s.is_empty()) {
        Some(("niri".into(), PathBuf::from(path)))
    } else if let (Some(sig), Some(rt)) = (
        hypr.filter(|s| !s.is_empty()),
        runtime.filter(|s| !s.is_empty()),
    ) {
        Some(("hyprland".into(), PathBuf::from(rt).join("hypr").join(sig)))
    } else {
        None
    }
}
pub fn detect(
    niri: Option<OsString>,
    hypr: Option<OsString>,
    runtime: Option<OsString>,
) -> Option<Arc<dyn Backend>> {
    selection(niri, hypr, runtime).map(|(kind, path)| connect(&kind, path))
}

fn capabilities(b: &str) -> Capabilities {
    let mut c = Capabilities::empty();
    if b == "niri" {
        c.features.insert("perOutputWorkspaces".into(), true);
    } else {
        c.features.insert("globalWorkspaces".into(), true);
        c.features.insert("stableWorkspaceIndices".into(), true);
    }
    c
}
impl Backend for Client {
    fn snapshot(&self) -> Arc<Snapshot> {
        Arc::new(
            self.0
                .state
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .clone(),
        )
    }
    fn subscribe(&self, cb: Box<dyn Fn(Event) + Send + Sync>) -> Subscription {
        let id = self.0.next.fetch_add(1, Ordering::Relaxed);
        self.0
            .callbacks
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push((id, Arc::from(cb)));
        let w = Arc::downgrade(&self.0);
        Subscription::new(move || {
            if let Some(i) = w.upgrade() {
                i.callbacks
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .retain(|(n, _)| *n != id);
            }
        })
    }
    fn action(&self, a: Action) -> Result<(), Error> {
        send_action(&self.0, a)
    }
}
impl Drop for Client {
    fn drop(&mut self) {
        self.0.stopping.store(true, Ordering::Release);
        // Pair with the backoff lock so a stop cannot miss the worker's wait.
        {
            let _waiting = self.0.backoff.0.lock().unwrap();
            self.0.backoff.1.notify_all();
        }
        if let Some(stream) = self.0.stream.lock().unwrap().as_ref() {
            let _ = stream.shutdown(std::net::Shutdown::Both);
        }
        if let Some(worker) = self.1.take()
            && worker.thread().id() != thread::current().id()
        {
            let _ = worker.join();
        }
    }
}
fn emit(i: &Inner, e: Event) {
    let cbs: Vec<_> = i
        .callbacks
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .iter()
        .map(|(_, c)| c.clone())
        .collect();
    for cb in cbs {
        cb(e.clone());
    }
}
fn publish(i: &Inner, mut s: Snapshot) {
    let mut guard = i.state.lock().unwrap_or_else(|e| e.into_inner());
    s.backend = i.backend.clone();
    s.epoch = guard.epoch;
    s.connected = true;
    s.capabilities = capabilities(&i.backend);
    if *guard == s {
        return;
    }
    *guard = s;
    let snap = Arc::new(guard.clone());
    drop(guard);
    emit(i, Event::Snapshot(snap));
}
fn disconnected(i: &Inner, msg: String) {
    let mut s = i.state.lock().unwrap_or_else(|e| e.into_inner());
    s.connected = false;
    let e = s.epoch;
    *s = Snapshot {
        backend: i.backend.clone(),
        epoch: e,
        connected: false,
        capabilities: capabilities(&i.backend),
        ..Snapshot::default()
    };
    drop(s);
    emit(i, Event::Disconnected(msg));
}
fn worker(weak: Weak<Inner>) {
    let mut attempt = 0u32;
    loop {
        let Some(i) = weak.upgrade() else { return };
        if i.stopping.load(Ordering::Acquire) {
            return;
        }
        if attempt > 0 {
            emit(&i, Event::Reconnecting { attempt });
        }
        let r = if i.backend == "niri" {
            niri_stream(&i)
        } else {
            hypr_stream(&i)
        };
        if i.stopping.load(Ordering::Acquire) {
            return;
        }
        disconnected(&i, r.err().unwrap_or_else(|| "event stream ended".into()));
        attempt = attempt.saturating_add(1);
        let delay =
            Duration::from_millis((100u64.saturating_mul(1u64 << attempt.min(6))).min(5000));
        let waiting = i.backoff.0.lock().unwrap();
        let _wait = i
            .backoff
            .1
            .wait_timeout_while(waiting, delay, |_| !i.stopping.load(Ordering::Acquire))
            .unwrap();
    }
}
fn niri_stream(i: &Arc<Inner>) -> Result<(), String> {
    let mut stream = UnixStream::connect(&i.path).map_err(|e| e.to_string())?;
    stream
        .write_all(b"\"EventStream\"\n")
        .map_err(|e| e.to_string())?;
    stream.flush().map_err(|e| e.to_string())?;
    let shared = stream.try_clone().map_err(|e| e.to_string())?;
    *i.stream.lock().unwrap_or_else(|e| e.into_inner()) = Some(shared);
    if i.stopping.load(Ordering::Acquire) {
        let _ = stream.shutdown(std::net::Shutdown::Both);
        return Ok(());
    }
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    if reader.read_line(&mut line).map_err(|e| e.to_string())? == 0 {
        return Err("missing niri event-stream acknowledgement".into());
    }
    let ack: Value = serde_json::from_str(line.trim()).map_err(|e| e.to_string())?;
    if ack.get("Err").is_some() {
        return Err(format!("niri rejected event stream: {ack}"));
    }
    if ack.get("Ok").is_none() {
        return Err(format!("invalid niri acknowledgement: {ack}"));
    }
    let epoch = {
        let mut s = i.state.lock().unwrap_or_else(|e| e.into_inner());
        s.epoch = s.epoch.wrapping_add(1);
        s.epoch
    };
    let mut state = Snapshot {
        backend: "niri".into(),
        epoch,
        connected: true,
        capabilities: capabilities("niri"),
        ..Snapshot::default()
    };
    if let Err(error) = refresh_niri_outputs(&i.path, &mut state) {
        emit(i, Event::Error(Error::Protocol(error)));
    }
    loop {
        line.clear();
        let n = reader.read_line(&mut line).map_err(|e| e.to_string())?;
        if n == 0 {
            return Err("niri event socket closed".into());
        }
        let v: Value =
            serde_json::from_str(line.trim()).map_err(|e| format!("invalid niri event: {e}"))?;
        apply_niri_event(&mut state, &v);
        if (v.get("WorkspacesChanged").is_some() || v.get("ConfigLoaded").is_some())
            && let Err(error) = refresh_niri_outputs(&i.path, &mut state)
        {
            emit(i, Event::Error(Error::Protocol(error)));
        }
        let focused_output = state.focused_workspace().and_then(|w| w.output.clone());
        for screen in &mut state.outputs {
            screen.focused = Some(&screen.id) == focused_output.as_ref();
        }
        publish(i, state.clone());
        if i.stopping.load(Ordering::Acquire) {
            return Ok(());
        }
    }
}
fn niri_req(path: &Path, req: &Value) -> Result<Value, String> {
    let mut s = UnixStream::connect(path).map_err(|e| e.to_string())?;
    s.set_read_timeout(Some(Duration::from_secs(2)))
        .map_err(|e| e.to_string())?;
    s.set_write_timeout(Some(Duration::from_secs(2)))
        .map_err(|e| e.to_string())?;
    s.write_all(req.to_string().as_bytes())
        .and_then(|_| s.write_all(b"\n"))
        .map_err(|e| e.to_string())?;
    s.shutdown(std::net::Shutdown::Write)
        .map_err(|e| e.to_string())?;
    let mut b = String::new();
    BufReader::new(s)
        .read_line(&mut b)
        .map_err(|e| e.to_string())?;
    serde_json::from_str(b.trim()).map_err(|e| e.to_string())
}
fn unwrap_ok(v: Value) -> Result<Value, String> {
    if let Some(e) = v.get("Err") {
        Err(e.to_string())
    } else {
        v.get("Ok")
            .cloned()
            .ok_or_else(|| "missing niri Ok/Err reply".into())
    }
}
fn refresh_niri_outputs(path: &Path, s: &mut Snapshot) -> Result<(), String> {
    let v = unwrap_ok(niri_req(path, &json!("Outputs"))?)?;
    let outputs = &v["Outputs"];
    let Some(m) = outputs.as_object() else {
        return Err("niri outputs was not a map".into());
    };
    s.outputs = m
        .iter()
        .map(|(name, v)| {
            let l = &v["logical"];
            let mode = &v["modes"][v["current_mode"].as_u64().unwrap_or(0) as usize];
            let (w, h) = (
                l["width"].as_u64().unwrap_or(0) as u32,
                l["height"].as_u64().unwrap_or(0) as u32,
            );
            let mw = mode["width"].as_u64().unwrap_or(0) as u32;
            let mh = mode["height"].as_u64().unwrap_or(0) as u32;
            Output {
                id: stable_id("niri", s.epoch, "output", name),
                name: name.clone(),
                width: if w == 0 { mw } else { w },
                height: if h == 0 { mh } else { h },
                scale: l["scale"].as_f64().unwrap_or(1.0),
                refresh_mhz: mode["refresh_rate"].as_u64().unwrap_or(0) as u32,
                x: l["x"].as_i64().unwrap_or(0) as i32,
                y: l["y"].as_i64().unwrap_or(0) as i32,
                focused: false,
            }
        })
        .collect();
    Ok(())
}
fn apply_niri_event(s: &mut Snapshot, v: &Value) {
    if let Some(w) = v
        .get("WorkspacesChanged")
        .and_then(|x| x["workspaces"].as_array())
    {
        s.workspaces = w
            .iter()
            .map(|x| {
                let id = x["id"].as_u64().unwrap_or(0);
                let n = x["name"].as_str().unwrap_or("").to_owned();
                Workspace {
                    id: stable_id("niri", s.epoch, "workspace", id),
                    name: n,
                    index: x["idx"].as_i64().unwrap_or(0),
                    output: x["output"]
                        .as_str()
                        .map(|n| stable_id("niri", s.epoch, "output", n)),
                    active: x["is_active"].as_bool().unwrap_or(false),
                    focused: x["is_focused"].as_bool().unwrap_or(false),
                    urgent: x["is_urgent"].as_bool().unwrap_or(false),
                    fixed_slot: x["name"].is_string(),
                    visiting: false,
                }
            })
            .collect();
    }
    if v.get("WorkspacesChanged").is_some() {
        for window in &mut s.windows {
            window.output = s
                .workspaces
                .iter()
                .find(|w| Some(&w.id) == window.workspace.as_ref())
                .and_then(|w| w.output.clone());
        }
    }
    if let Some(w) = v
        .get("WindowsChanged")
        .and_then(|x| x["windows"].as_array())
    {
        s.windows = w.iter().filter_map(|x| niri_window(s, x)).collect();
    }
    if let Some(x) = v.get("WindowOpenedOrChanged").and_then(|x| x.get("window"))
        && let Some(w) = niri_window(s, x)
    {
        if w.focused {
            for old in &mut s.windows {
                old.focused = false;
            }
        }
        if let Some(old) = s.windows.iter_mut().find(|q| q.id == w.id) {
            *old = w
        } else {
            s.windows.push(w)
        }
    }
    if let Some(id) = v.get("WindowClosed").and_then(|x| x["id"].as_u64()) {
        s.windows
            .retain(|w| w.id != stable_id("niri", s.epoch, "window", id));
    }
    if let Some(x) = v.get("WindowFocusChanged") {
        let id = x["id"].as_u64();
        for w in &mut s.windows {
            w.focused = id.is_some_and(|id| w.id == stable_id("niri", s.epoch, "window", id));
        }
    }
    if let Some(x) = v.get("WorkspaceActivated") {
        let id = x["id"].as_u64().unwrap_or(0);
        let output = s
            .workspaces
            .iter()
            .find(|q| q.id == stable_id("niri", s.epoch, "workspace", id))
            .and_then(|q| q.output.clone());
        let is_focused = x["focused"].as_bool().unwrap_or(false);
        for w in &mut s.workspaces {
            if w.id == stable_id("niri", s.epoch, "workspace", id) {
                w.active = true;
                if is_focused {
                    w.focused = true;
                }
            } else if w.output == output {
                w.active = false;
            }
            if is_focused && w.id != stable_id("niri", s.epoch, "workspace", id) {
                w.focused = false;
            }
        }
    }
    if let Some(x) = v.get("WorkspaceUrgencyChanged") {
        let id = x["id"].as_u64().unwrap_or(0);
        if let Some(w) = s
            .workspaces
            .iter_mut()
            .find(|w| w.id == stable_id("niri", s.epoch, "workspace", id))
        {
            w.urgent = x["urgent"].as_bool().unwrap_or(false)
        }
    }
    if let Some(x) = v.get("WindowUrgencyChanged") {
        let id = x["id"].as_u64().unwrap_or(0);
        if let Some(w) = s
            .windows
            .iter_mut()
            .find(|w| w.id == stable_id("niri", s.epoch, "window", id))
        {
            w.urgent = x["urgent"].as_bool().unwrap_or(false)
        }
    }
}
fn niri_window(s: &Snapshot, x: &Value) -> Option<Window> {
    let id = x["id"].as_u64()?;
    let wid = x["workspace_id"].as_u64();
    Some(Window {
        id: stable_id("niri", s.epoch, "window", id),
        title: x["title"].as_str().unwrap_or("").into(),
        app_id: x["app_id"].as_str().unwrap_or("").into(),
        workspace: wid.map(|id| stable_id("niri", s.epoch, "workspace", id)),
        output: wid.and_then(|id| {
            s.workspaces
                .iter()
                .find(|w| w.id == stable_id("niri", s.epoch, "workspace", id))
                .and_then(|w| w.output.clone())
        }),
        focused: x["is_focused"].as_bool().unwrap_or(false),
        floating: x["is_floating"].as_bool().unwrap_or(false),
        fullscreen: None,
        urgent: x["is_urgent"].as_bool().unwrap_or(false),
    })
}
fn hypr_socket(dir: &Path, event: bool) -> PathBuf {
    dir.join(if event {
        ".socket2.sock"
    } else {
        ".socket.sock"
    })
}
fn hypr_request(dir: &Path, cmd: &str) -> Result<String, String> {
    let mut s = UnixStream::connect(hypr_socket(dir, false)).map_err(|e| e.to_string())?;
    s.set_read_timeout(Some(Duration::from_secs(2)))
        .map_err(|e| e.to_string())?;
    s.set_write_timeout(Some(Duration::from_secs(2)))
        .map_err(|e| e.to_string())?;
    s.write_all(cmd.as_bytes()).map_err(|e| e.to_string())?;
    s.shutdown(std::net::Shutdown::Write)
        .map_err(|e| e.to_string())?;
    let mut out = String::new();
    s.read_to_string(&mut out).map_err(|e| e.to_string())?;
    if out.starts_with("invalid command") || out.starts_with("error") {
        Err(out)
    } else {
        Ok(out)
    }
}
fn hypr_json(dir: &Path, cmd: &str) -> Result<Value, String> {
    serde_json::from_str(&hypr_request(dir, &format!("j/{cmd}"))?)
        .map_err(|e| format!("invalid Hyprland {cmd} JSON: {e}"))
}
fn hypr_stream(i: &Arc<Inner>) -> Result<(), String> {
    let stream = UnixStream::connect(hypr_socket(&i.path, true)).map_err(|e| e.to_string())?;
    let shared = stream.try_clone().map_err(|e| e.to_string())?;
    *i.stream.lock().unwrap_or_else(|e| e.into_inner()) = Some(shared);
    if i.stopping.load(Ordering::Acquire) {
        let _ = stream.shutdown(std::net::Shutdown::Both);
        return Ok(());
    }
    let mut state = Snapshot {
        backend: "hyprland".into(),
        connected: true,
        capabilities: capabilities("hyprland"),
        ..Snapshot::default()
    };
    {
        let mut old = i.state.lock().unwrap_or_else(|e| e.into_inner());
        old.epoch = old.epoch.wrapping_add(1);
        state.epoch = old.epoch;
    }
    refresh_hypr(&i.path, &mut state)?;
    publish(i, state.clone());
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    loop {
        line.clear();
        let n = reader.read_line(&mut line).map_err(|e| e.to_string())?;
        if n == 0 {
            return Err("Hyprland event socket closed".into());
        }
        if !line.contains(">>") {
            continue;
        }
        let kind = line.split_once(">>").map(|x| x.0).unwrap_or("");
        if matches!(
            kind,
            "workspace"
                | "workspacev2"
                | "focusedmon"
                | "focusedmonv2"
                | "createworkspace"
                | "createworkspacev2"
                | "destroyworkspace"
                | "destroyworkspacev2"
                | "moveworkspace"
                | "moveworkspacev2"
                | "activewindow"
                | "activewindowv2"
                | "openwindow"
                | "closewindow"
                | "movewindow"
                | "movewindowv2"
                | "windowtitle"
                | "windowtitlev2"
                | "urgent"
                | "monitoradded"
                | "monitoraddedv2"
                | "monitorremoved"
                | "monitorremovedv2"
                | "fullscreen"
                | "renameworkspace"
                | "configreloaded"
                | "activespecial"
                | "activespecialv2"
        ) {
            refresh_hypr(&i.path, &mut state)?;
            if kind == "urgent" {
                let address = line
                    .split_once(">>")
                    .map(|(_, data)| data.trim())
                    .unwrap_or("");
                if let Some(window) = state.windows.iter_mut().find(|w| {
                    w.id.0
                        .rsplit(':')
                        .next()
                        .unwrap_or("")
                        .trim_start_matches("0x")
                        == address.trim_start_matches("0x")
                }) {
                    window.urgent = true;
                }
                for ws in &mut state.workspaces {
                    ws.urgent = state
                        .windows
                        .iter()
                        .any(|w| w.urgent && w.workspace.as_ref() == Some(&ws.id));
                }
            }
            publish(i, state.clone());
        }
        if i.stopping.load(Ordering::Acquire) {
            return Ok(());
        }
    }
}
fn refresh_hypr(dir: &Path, s: &mut Snapshot) -> Result<(), String> {
    let monitors = hypr_json(dir, "monitors")?;
    let workspaces = hypr_json(dir, "workspaces")?;
    let clients = hypr_json(dir, "clients")?;
    let mons = monitors
        .as_array()
        .ok_or("Hyprland monitors was not an array")?;
    let wss = workspaces
        .as_array()
        .ok_or("Hyprland workspaces was not an array")?;
    let wins = clients
        .as_array()
        .ok_or("Hyprland clients was not an array")?;
    s.outputs = mons
        .iter()
        .map(|m| {
            let name = strf(&m["name"]);
            let pos = (
                m["x"].as_i64().unwrap_or(0) as i32,
                m["y"].as_i64().unwrap_or(0) as i32,
            );
            let size = (
                m["width"].as_i64().unwrap_or(0) as i32,
                m["height"].as_i64().unwrap_or(0) as i32,
            );
            Output {
                id: stable_id("hyprland", s.epoch, "output", &name),
                name,
                width: size.0.max(0) as u32,
                height: size.1.max(0) as u32,
                scale: m["scale"].as_f64().unwrap_or(1.),
                refresh_mhz: (m["refreshRate"].as_f64().unwrap_or(0.) * 1000.) as u32,
                x: pos.0,
                y: pos.1,
                focused: m["focused"].as_bool().unwrap_or(false),
            }
        })
        .collect();
    let output_id = |name: &str| {
        s.outputs
            .iter()
            .find(|o| o.name == name)
            .map(|o| o.id.clone())
    };
    s.workspaces = wss
        .iter()
        .map(|w| {
            let native = strf(&w["id"]);
            let name = strf(&w["name"]);
            let mon = strf(&w["monitor"]);
            let active = mons
                .iter()
                .any(|m| strf(&m["activeWorkspace"]["id"]) == native);
            Workspace {
                id: stable_id("hyprland", s.epoch, "workspace", &native),
                index: w["id"].as_i64().or_else(|| name.parse().ok()).unwrap_or(-1),
                name,
                output: output_id(&mon),
                active,
                focused: active
                    && mons.iter().any(|m| {
                        m["name"].as_str() == Some(mon.as_str())
                            && m["focused"].as_bool().unwrap_or(false)
                    }),
                urgent: false,
                fixed_slot: false,
                visiting: false,
            }
        })
        .collect();
    let active_address = strf(&hypr_json(dir, "activewindow")?["address"]);
    let old_urgent: std::collections::HashSet<String> = s
        .windows
        .iter()
        .filter(|w| w.urgent)
        .map(|w| w.id.0.rsplit(':').next().unwrap_or_default().to_owned())
        .collect();
    s.windows = wins
        .iter()
        .map(|w| {
            let native = strf(&w["address"]);
            let wid = strf(&w["workspace"]["id"]);
            let ws = s
                .workspaces
                .iter()
                .find(|q| q.id == stable_id("hyprland", s.epoch, "workspace", &wid));
            let mon = strf(&w["monitor"]);
            Window {
                id: stable_id("hyprland", s.epoch, "window", &native),
                title: strf(&w["title"]),
                app_id: w.get("class").and_then(Value::as_str).unwrap_or("").into(),
                workspace: Some(stable_id("hyprland", s.epoch, "workspace", &wid)),
                output: ws
                    .and_then(|q| q.output.clone())
                    .or_else(|| output_id(&mon)),
                focused: !active_address.is_empty() && native == active_address,
                floating: w["floating"].as_bool().unwrap_or(false),
                fullscreen: None,
                urgent: native != active_address
                    && (w["urgent"].as_bool().unwrap_or(false) || old_urgent.contains(&native)),
            }
        })
        .collect();
    for ws in &mut s.workspaces {
        ws.urgent = s
            .windows
            .iter()
            .any(|w| w.urgent && w.workspace.as_ref() == Some(&ws.id));
    }
    Ok(())
}
fn strf(v: &Value) -> String {
    v.as_str()
        .map(str::to_owned)
        .or_else(|| v.as_i64().map(|n| n.to_string()))
        .unwrap_or_default()
}
fn send_action(i: &Inner, a: Action) -> Result<(), Error> {
    let s = i.state.lock().unwrap_or_else(|e| e.into_inner()).clone();
    if !s.connected {
        return Err(Error::Disconnected(i.backend.clone()));
    }
    let request = match a {
        Action::SwitchWorkspace { workspace } => {
            let n = validate_id(&s, &workspace, "workspace")?;
            if i.backend == "niri" {
                json!({"Action":{"FocusWorkspace":{"reference":{"Id":n.parse::<u64>().map_err(|_|Error::UnknownId(workspace.0.clone()))?}}}}).to_string()
            } else {
                let target = s.workspaces.iter().find(|w| w.id == workspace).unwrap();
                let selector = if n.parse::<i64>().is_ok_and(|id| id > 0) {
                    n
                } else {
                    format!("name:{}", target.name)
                };
                if selector.bytes().any(|b| b.is_ascii_control() || b == b';') {
                    return Err(Error::Protocol(
                        "workspace name contains command syntax".into(),
                    ));
                }
                format!("/dispatch workspace {selector}")
            }
        }
        Action::SwitchWorkspaceIndex { index } => {
            if index <= 0 {
                return Err(Error::Protocol("workspace index must be positive".into()));
            }
            if i.backend == "niri" {
                let idx = u8::try_from(index)
                    .map_err(|_| Error::Protocol("niri workspace index exceeds u8".into()))?;
                json!({"Action":{"FocusWorkspace":{"reference":{"Index":idx}}}}).to_string()
            } else {
                format!("/dispatch workspace {index}")
            }
        }
        Action::FocusWindow { window } => {
            let n = validate_id(&s, &window, "window")?;
            if i.backend == "niri" {
                json!({"Action":{"FocusWindow":{"id":n.parse::<u64>().map_err(|_|Error::UnknownId(window.0.clone()))?}}}).to_string()
            } else if n.starts_with("0x") && n[2..].bytes().all(|b| b.is_ascii_hexdigit()) {
                format!("/dispatch focuswindow address:{n}")
            } else {
                return Err(Error::Protocol(
                    "Hyprland window address is malformed".into(),
                ));
            }
        }
        Action::FocusOutput { output } => {
            validate_id(&s, &output, "output")?;
            let name = s
                .outputs
                .iter()
                .find(|x| x.id == output)
                .map(|x| x.name.clone())
                .ok_or_else(|| Error::UnknownId(output.0.clone()))?;
            if name.bytes().any(|b| b.is_ascii_control() || b == b';') {
                return Err(Error::Protocol(
                    "output name contains command syntax".into(),
                ));
            }
            if i.backend == "niri" {
                json!({"Action":{"FocusMonitor":{"output":name}}}).to_string()
            } else {
                format!("/dispatch focusmonitor {name}")
            }
        }
        other => {
            return Err(Error::Unsupported {
                operation: format!("{other:?}"),
                backend: i.backend.clone(),
            });
        }
    };
    if i.backend == "niri" {
        let reply = niri_req(
            &i.path,
            &serde_json::from_str(&request).map_err(|e| Error::Protocol(e.to_string()))?,
        )
        .map_err(Error::Disconnected)?;
        unwrap_ok(reply).map(|_| ()).map_err(Error::Protocol)
    } else {
        hypr_request(&i.path, &request)
            .and_then(|reply| {
                if reply.trim() == "ok" {
                    Ok(())
                } else {
                    Err(reply)
                }
            })
            .map_err(Error::Protocol)
    }
}
fn validate_id(s: &Snapshot, id: &Id, kind: &str) -> Result<String, Error> {
    let prefix = format!("{}:{}:{kind}:", s.backend, s.epoch);
    id.0.strip_prefix(&prefix)
        .filter(|n| {
            !n.is_empty()
                && match kind {
                    "workspace" => s.workspaces.iter().any(|x| x.id == *id),
                    "window" => s.windows.iter().any(|x| x.id == *id),
                    "output" => s.outputs.iter().any(|x| x.id == *id),
                    _ => false,
                }
        })
        .map(str::to_owned)
        .ok_or_else(|| Error::UnknownId(id.0.clone()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detection_prefers_niri_and_requires_hypr_runtime() {
        let both = selection(
            Some("/run/niri.sock".into()),
            Some("instance".into()),
            Some("/run".into()),
        )
        .unwrap();
        assert_eq!(both, ("niri".into(), PathBuf::from("/run/niri.sock")));
        assert!(selection(None, Some("instance".into()), None).is_none());
        assert!(selection(None, None, None).is_none());
        assert_eq!(
            selection(None, Some("instance".into()), Some("/run".into())).unwrap(),
            ("hyprland".into(), PathBuf::from("/run/hypr/instance"))
        );
    }

    #[test]
    fn niri_recorded_event_sequence_updates_workspaces_and_window_title() {
        let mut s = Snapshot {
            backend: "niri".into(),
            epoch: 4,
            ..Snapshot::default()
        };
        apply_niri_event(
            &mut s,
            &json!({"WorkspacesChanged":{"workspaces":[
                {"id":12,"idx":1,"name":"one","output":"DP-1","is_active":true,"is_focused":true,"is_urgent":false},
                {"id":13,"idx":2,"name":"two","output":"DP-1","is_active":false,"is_focused":false,"is_urgent":false}
            ]}}),
        );
        apply_niri_event(
            &mut s,
            &json!({"WindowsChanged":{"windows":[{"id":7,"title":"old","app_id":"term","workspace_id":12,"is_focused":true,"is_floating":false,"is_urgent":false}]}}),
        );
        apply_niri_event(
            &mut s,
            &json!({"WindowOpenedOrChanged":{"window":{"id":7,"title":"new","app_id":"term","workspace_id":12,"is_focused":true,"is_floating":false,"is_urgent":false}}}),
        );
        apply_niri_event(
            &mut s,
            &json!({"WorkspaceActivated":{"id":13,"focused":true}}),
        );
        assert_eq!(s.windows[0].title, "new");
        assert_eq!(s.workspaces.iter().find(|w| w.focused).unwrap().name, "two");
        assert!(!s.workspaces[0].active);
    }

    #[test]
    fn ids_are_rejected_after_epoch_changes() {
        let stale = stable_id("niri", 2, "workspace", 12);
        let current = Snapshot {
            backend: "niri".into(),
            epoch: 3,
            connected: true,
            workspaces: vec![Workspace {
                id: stable_id("niri", 3, "workspace", 12),
                name: "one".into(),
                index: 1,
                output: None,
                active: true,
                focused: true,
                urgent: false,
                fixed_slot: false,
                visiting: false,
            }],
            ..Snapshot::default()
        };
        assert!(validate_id(&current, &stale, "workspace").is_err());
        assert_eq!(
            validate_id(&current, &current.workspaces[0].id, "workspace").unwrap(),
            "12"
        );
    }

    #[test]
    fn action_request_is_written_as_documented_niri_json() {
        use std::os::unix::net::UnixListener;
        let root = std::env::temp_dir().join(format!("bead-niri-{}", std::process::id()));
        let _ = std::fs::remove_file(&root);
        let listener = UnixListener::bind(&root).unwrap();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = String::new();
            BufReader::new(stream.try_clone().unwrap())
                .read_line(&mut request)
                .unwrap();
            stream.write_all(b"{\"Ok\":\"Handled\"}\n").unwrap();
            request
        });
        let workspace = stable_id("niri", 1, "workspace", 12);
        let i = Inner {
            backend: "niri".into(),
            path: root.clone(),
            state: Mutex::new(Snapshot {
                backend: "niri".into(),
                epoch: 1,
                connected: true,
                workspaces: vec![Workspace {
                    id: workspace.clone(),
                    name: "one".into(),
                    index: 1,
                    output: None,
                    active: true,
                    focused: true,
                    urgent: false,
                    fixed_slot: false,
                    visiting: false,
                }],
                ..Snapshot::default()
            }),
            callbacks: Mutex::new(vec![]),
            next: AtomicU64::new(1),
            stopping: AtomicBool::new(false),
            stream: Mutex::new(None),
            backoff: (Mutex::new(()), Condvar::new()),
        };
        send_action(&i, Action::SwitchWorkspace { workspace }).unwrap();
        assert_eq!(
            server.join().unwrap().trim(),
            r#"{"Action":{"FocusWorkspace":{"reference":{"Id":12}}}}"#
        );
        let _ = std::fs::remove_file(root);
    }
}
