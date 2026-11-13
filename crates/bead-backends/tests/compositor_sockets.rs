// SPDX-License-Identifier: MIT
#![cfg(all(feature = "compositors", unix))]

use bead_backends::{Action, Event};
use serde_json::{Value, json};
use std::{
    fs,
    io::{BufRead, BufReader, Read, Write},
    os::unix::net::{UnixListener, UnixStream},
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};

static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

struct TempDir(PathBuf);
impl TempDir {
    fn new(name: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "bead-{name}-{}-{}",
            std::process::id(),
            NEXT_TEMP.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }
}
impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn wait_for<T>(mut get: impl FnMut() -> Option<T>, message: &str) -> T {
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        if let Some(value) = get() {
            return value;
        }
        assert!(Instant::now() < deadline, "timed out waiting for {message}");
        thread::sleep(Duration::from_millis(10));
    }
}

fn read_request(stream: &mut UnixStream) -> String {
    let mut request = String::new();
    BufReader::new(stream.try_clone().unwrap())
        .read_line(&mut request)
        .unwrap();
    request
}

struct NiriServer {
    path: PathBuf,
    stop: Arc<AtomicBool>,
    acceptor: Option<thread::JoinHandle<()>>,
    event_writer: Arc<Mutex<Option<UnixStream>>>,
    requests: Arc<Mutex<Vec<Value>>>,
    event_connections: Arc<AtomicUsize>,
    workers: Arc<Mutex<Vec<thread::JoinHandle<()>>>>,
}
impl NiriServer {
    fn new(path: PathBuf) -> Self {
        let listener = UnixListener::bind(&path).unwrap();
        listener.set_nonblocking(true).unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let event_writer = Arc::new(Mutex::new(None));
        let requests = Arc::new(Mutex::new(Vec::new()));
        let event_connections = Arc::new(AtomicUsize::new(0));
        let workers = Arc::new(Mutex::new(Vec::new()));
        let thread_stop = stop.clone();
        let thread_writer = event_writer.clone();
        let thread_requests = requests.clone();
        let thread_connections = event_connections.clone();
        let thread_workers = workers.clone();
        let acceptor = thread::spawn(move || {
            while !thread_stop.load(Ordering::Acquire) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        let writer = thread_writer.clone();
                        let requests = thread_requests.clone();
                        let connections = thread_connections.clone();
                        let stop = thread_stop.clone();
                        let worker = thread::spawn(move || {
                            niri_connection(stream, writer, requests, connections, stop)
                        });
                        thread_workers.lock().unwrap().push(worker);
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(5));
                    }
                    Err(error) => panic!("niri fake listener failed: {error}"),
                }
            }
        });
        Self {
            path,
            stop,
            acceptor: Some(acceptor),
            event_writer,
            requests,
            event_connections,
            workers,
        }
    }

    fn event_count(&self) -> usize {
        self.event_connections.load(Ordering::Acquire)
    }

    fn event(&self, value: Value) {
        let line = format!("{}\n", value);
        let mut writer = self.event_writer.lock().unwrap();
        writer
            .as_mut()
            .expect("niri event stream is connected")
            .write_all(line.as_bytes())
            .unwrap();
        writer.as_mut().unwrap().flush().unwrap();
    }

    fn close_event(&self) {
        if let Some(stream) = self.event_writer.lock().unwrap().take() {
            let _ = stream.shutdown(std::net::Shutdown::Both);
        }
    }

    fn request_matching(&self, predicate: impl Fn(&Value) -> bool) -> Value {
        wait_for(
            || {
                self.requests
                    .lock()
                    .unwrap()
                    .iter()
                    .find(|request| predicate(request))
                    .cloned()
            },
            "niri request",
        )
    }
}
impl Drop for NiriServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        self.close_event();
        if let Some(acceptor) = self.acceptor.take() {
            acceptor.join().unwrap();
        }
        for worker in self.workers.lock().unwrap().drain(..) {
            worker.join().unwrap();
        }
        let _ = fs::remove_file(&self.path);
    }
}

fn niri_connection(
    mut stream: UnixStream,
    writer: Arc<Mutex<Option<UnixStream>>>,
    requests: Arc<Mutex<Vec<Value>>>,
    connections: Arc<AtomicUsize>,
    stop: Arc<AtomicBool>,
) {
    stream
        .set_read_timeout(Some(Duration::from_millis(100)))
        .unwrap();
    let request = read_request(&mut stream);
    if request.trim() == "\"EventStream\"" {
        connections.fetch_add(1, Ordering::AcqRel);
        stream.write_all(b"{\"Ok\":\"Handled\"}\n").unwrap();
        let mut events = vec![
            json!({"WorkspacesChanged":{"workspaces":[
                {"id":12,"idx":1,"name":"one","output":"DP-1","is_active":true,"is_focused":true,"is_urgent":false},
                {"id":13,"idx":2,"name":"two","output":"DP-1","is_active":false,"is_focused":false,"is_urgent":false}
            ]}}),
            json!({"WindowsChanged":{"windows":[
                {"id":7,"title":"Editor","app_id":"org.example.Editor","workspace_id":12,"is_focused":true,"is_floating":false,"is_urgent":false}
            ]}}),
        ];
        for event in events.drain(..) {
            writeln!(stream, "{event}").unwrap();
        }
        stream.flush().unwrap();
        *writer.lock().unwrap() = Some(stream.try_clone().unwrap());
        let mut reader = BufReader::new(stream);
        let mut ignored = String::new();
        while !stop.load(Ordering::Acquire) {
            ignored.clear();
            match reader.read_line(&mut ignored) {
                Ok(0) => break,
                Ok(_) => {}
                Err(error)
                    if matches!(
                        error.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                    ) => {}
                Err(_) => break,
            }
        }
        return;
    }
    let value: Value = serde_json::from_str(request.trim()).unwrap();
    requests.lock().unwrap().push(value.clone());
    let response = if value == json!("Outputs") {
        json!({"Ok":{"Outputs":{"DP-1":{
            "logical":{"width":1280,"height":720,"x":0,"y":0,"scale":1.0},
            "modes":[{"width":1280,"height":720,"refresh_rate":60000}],
            "current_mode":0
        }}}})
    } else if value == json!("FocusedOutput") {
        json!({"Ok":{"FocusedOutput":{"name":"DP-1"}}})
    } else {
        json!({"Ok":"Handled"})
    };
    writeln!(stream, "{response}").unwrap();
    let _ = stream.flush();
}

#[test]
fn niri_socket_events_actions_and_reconnect_update_public_snapshot() {
    let temp = TempDir::new("niri-ipc");
    let path = temp.0.join("niri.sock");
    let server = NiriServer::new(path.clone());
    let backend = bead_backends::niri::connect(&path);
    let initial = wait_for(
        || {
            let snapshot = backend.snapshot();
            (snapshot.connected && snapshot.windows.len() == 1).then(|| snapshot.clone())
        },
        "initial niri snapshot",
    );
    assert_eq!(initial.backend, "niri");
    assert_eq!(initial.outputs[0].name, "DP-1");
    assert!(initial.outputs[0].focused);
    assert_eq!(initial.workspaces.len(), 2);
    assert_eq!(initial.focused_workspace().unwrap().name, "one");
    assert_eq!(initial.focused_window().unwrap().title, "Editor");
    assert_eq!(
        initial.focused_window().unwrap().app_id,
        "org.example.Editor"
    );

    let (events_tx, events_rx) = mpsc::channel();
    let _subscription = backend.subscribe(Box::new(move |event| {
        let _ = events_tx.send(event);
    }));
    server.event(json!({"WindowOpenedOrChanged":{"window":{
        "id":7,"title":"Editor — updated","app_id":"org.example.Editor",
        "workspace_id":12,"is_focused":true,"is_floating":false,"is_urgent":false
    }}}));
    server.event(json!({"WorkspaceActivated":{"id":13,"focused":true}}));
    let changed = wait_for(
        || {
            let snapshot = backend.snapshot();
            (snapshot
                .focused_window()
                .is_some_and(|w| w.title == "Editor — updated")
                && snapshot
                    .focused_workspace()
                    .is_some_and(|w| w.name == "two"))
            .then(|| snapshot.clone())
        },
        "niri window and workspace change events",
    );
    assert!(!changed.workspaces[0].active);
    assert!(events_rx.recv_timeout(Duration::from_secs(1)).is_ok());
    server.event(json!({"WindowFocusChanged":{"id":null}}));
    wait_for(
        || {
            backend
                .snapshot()
                .windows
                .iter()
                .all(|window| !window.focused)
                .then_some(())
        },
        "niri cleared window focus",
    );
    server.event(json!({"WindowFocusChanged":{"id":7}}));
    wait_for(
        || {
            backend
                .snapshot()
                .focused_window()
                .is_some_and(|window| window.title == "Editor — updated")
                .then_some(())
        },
        "niri restored window focus",
    );

    let workspace_id = changed.workspaces[0].id.clone();
    let window_id = changed.windows[0].id.clone();
    let output_id = changed.outputs[0].id.clone();
    backend
        .action(Action::SwitchWorkspace {
            workspace: workspace_id.clone(),
        })
        .unwrap();
    backend
        .action(Action::SwitchWorkspaceIndex { index: 2 })
        .unwrap();
    backend
        .action(Action::FocusWindow {
            window: window_id.clone(),
        })
        .unwrap();
    backend
        .action(Action::FocusOutput { output: output_id })
        .unwrap();
    assert_eq!(
        server.request_matching(
            |request| request["Action"]["FocusWorkspace"]["reference"]["Id"] == 12
        )["Action"]["FocusWorkspace"]["reference"]["Id"],
        12
    );
    assert_eq!(
        server.request_matching(
            |request| request["Action"]["FocusWorkspace"]["reference"]["Index"] == 2
        )["Action"]["FocusWorkspace"]["reference"]["Index"],
        2
    );
    assert_eq!(
        server.request_matching(|request| request["Action"]["FocusWindow"]["id"] == 7)["Action"]["FocusWindow"]
            ["id"],
        7
    );
    assert_eq!(
        server.request_matching(|request| request["Action"]["FocusMonitor"]["output"] == "DP-1")["Action"]
            ["FocusMonitor"]["output"],
        "DP-1"
    );
    let request_count = server.requests.lock().unwrap().len();
    thread::sleep(Duration::from_millis(150));
    assert_eq!(server.requests.lock().unwrap().len(), request_count);

    let epoch = changed.epoch;
    server.close_event();
    wait_for(
        || (server.event_count() >= 2).then_some(()),
        "niri event socket reconnect",
    );
    let reconnected = wait_for(
        || {
            let snapshot = backend.snapshot();
            (snapshot.connected && snapshot.epoch > epoch).then(|| snapshot.clone())
        },
        "niri new connection epoch",
    );
    assert!(matches!(
        backend.action(Action::SwitchWorkspace {
            workspace: workspace_id
        }),
        Err(bead_backends::Error::UnknownId(_))
    ));
    assert_ne!(reconnected.epoch, epoch);
}

#[derive(Clone)]
struct HyprFixtures {
    monitors: Value,
    workspaces: Value,
    clients: Value,
    activewindow: Value,
}
struct HyprServer {
    directory: PathBuf,
    stop: Arc<AtomicBool>,
    acceptors: Vec<thread::JoinHandle<()>>,
    event_writer: Arc<Mutex<Option<UnixStream>>>,
    event_connections: Arc<AtomicUsize>,
    fixtures: Arc<Mutex<HyprFixtures>>,
    requests: Arc<Mutex<Vec<String>>>,
    workers: Arc<Mutex<Vec<thread::JoinHandle<()>>>>,
}
impl HyprServer {
    fn new(directory: PathBuf) -> Self {
        fs::create_dir_all(&directory).unwrap();
        let request_listener = UnixListener::bind(directory.join(".socket.sock")).unwrap();
        let event_listener = UnixListener::bind(directory.join(".socket2.sock")).unwrap();
        request_listener.set_nonblocking(true).unwrap();
        event_listener.set_nonblocking(true).unwrap();
        let fixtures = Arc::new(Mutex::new(hypr_initial_fixtures()));
        let requests = Arc::new(Mutex::new(Vec::new()));
        let event_writer = Arc::new(Mutex::new(None));
        let event_connections = Arc::new(AtomicUsize::new(0));
        let stop = Arc::new(AtomicBool::new(false));
        let workers = Arc::new(Mutex::new(Vec::new()));

        let request_stop = stop.clone();
        let request_fixtures = fixtures.clone();
        let request_log = requests.clone();
        let request_workers = workers.clone();
        let request_thread = thread::spawn(move || {
            while !request_stop.load(Ordering::Acquire) {
                match request_listener.accept() {
                    Ok((mut stream, _)) => {
                        let fixtures = request_fixtures.clone();
                        let requests = request_log.clone();
                        let worker = thread::spawn(move || {
                            let mut request = String::new();
                            stream.read_to_string(&mut request).unwrap();
                            requests.lock().unwrap().push(request.clone());
                            let state = fixtures.lock().unwrap().clone();
                            let response = match request.as_str() {
                                "j/monitors" => state.monitors.to_string(),
                                "j/workspaces" => state.workspaces.to_string(),
                                "j/clients" => state.clients.to_string(),
                                "j/activewindow" => state.activewindow.to_string(),
                                command if command.starts_with("/dispatch ") => "ok".into(),
                                other => panic!("unexpected Hyprland fake request: {other}"),
                            };
                            stream.write_all(response.as_bytes()).unwrap();
                        });
                        request_workers.lock().unwrap().push(worker);
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(5));
                    }
                    Err(error) => panic!("Hyprland fake request listener failed: {error}"),
                }
            }
        });
        let event_stop = stop.clone();
        let event_writer_thread = event_writer.clone();
        let event_connections_thread = event_connections.clone();
        let event_thread = thread::spawn(move || {
            while !event_stop.load(Ordering::Acquire) {
                match event_listener.accept() {
                    Ok((stream, _)) => {
                        event_connections_thread.fetch_add(1, Ordering::AcqRel);
                        *event_writer_thread.lock().unwrap() = Some(stream.try_clone().unwrap());
                        let mut stream = stream;
                        stream
                            .set_read_timeout(Some(Duration::from_millis(100)))
                            .unwrap();
                        let mut ignored = [0; 1];
                        while !event_stop.load(Ordering::Acquire) {
                            match stream.read(&mut ignored) {
                                Ok(0) => break,
                                Ok(_) => {}
                                Err(error)
                                    if matches!(
                                        error.kind(),
                                        std::io::ErrorKind::WouldBlock
                                            | std::io::ErrorKind::TimedOut
                                    ) => {}
                                Err(_) => break,
                            }
                        }
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(5));
                    }
                    Err(error) => panic!("Hyprland fake event listener failed: {error}"),
                }
            }
        });
        Self {
            directory,
            stop,
            acceptors: vec![request_thread, event_thread],
            event_writer,
            event_connections,
            fixtures,
            requests,
            workers,
        }
    }

    fn event_count(&self) -> usize {
        self.event_connections.load(Ordering::Acquire)
    }

    fn event(&self, event: &str) {
        let mut writer = self.event_writer.lock().unwrap();
        let stream = writer.as_mut().expect("Hyprland event socket connected");
        writeln!(stream, "{event}").unwrap();
        stream.flush().unwrap();
    }

    fn close_event(&self) {
        if let Some(stream) = self.event_writer.lock().unwrap().take() {
            let _ = stream.shutdown(std::net::Shutdown::Both);
        }
    }

    fn request_seen(&self, request: &str) -> bool {
        self.requests
            .lock()
            .unwrap()
            .iter()
            .any(|item| item == request)
    }
}
impl Drop for HyprServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        self.close_event();
        for acceptor in self.acceptors.drain(..) {
            acceptor.join().unwrap();
        }
        for worker in self.workers.lock().unwrap().drain(..) {
            worker.join().unwrap();
        }
        let _ = fs::remove_dir_all(&self.directory);
    }
}

fn hypr_initial_fixtures() -> HyprFixtures {
    HyprFixtures {
        monitors: json!([{
            "id":0,"name":"DP-1","description":"Display","make":"Example",
            "model":"Panel","serial":"1","width":1920,"height":1080,
            "refreshRate":60.0,"x":0,"y":0,"scale":1.0,"focused":true,
            "activeWorkspace":{"id":1,"name":"one"}
        }]),
        workspaces: json!([
            {"id":1,"name":"one","monitor":"DP-1"},
            {"id":2,"name":"two","monitor":"DP-1"}
        ]),
        clients: json!([{
            "address":"0xabc","class":"org.example.Editor","title":"Editor",
            "workspace":{"id":1,"name":"one"},"monitor":0,"floating":false,
            "urgent":false
        }]),
        activewindow: json!({"address":"0xabc","class":"org.example.Editor","title":"Editor"}),
    }
}

#[test]
fn hyprland_sockets_events_actions_and_reconnect_update_public_snapshot() {
    let temp = TempDir::new("hyprland-ipc");
    let directory = temp.0.join("hypr").join("instance-test");
    let server = HyprServer::new(directory.clone());
    let backend = bead_backends::hyprland::connect(&directory);
    let initial = wait_for(
        || {
            let snapshot = backend.snapshot();
            (snapshot.connected && snapshot.windows.len() == 1).then(|| snapshot.clone())
        },
        "initial Hyprland snapshot",
    );
    assert_eq!(initial.backend, "hyprland");
    assert_eq!(initial.outputs[0].name, "DP-1");
    assert_eq!(
        (initial.outputs[0].width, initial.outputs[0].height),
        (1920, 1080)
    );
    assert_eq!(initial.focused_workspace().unwrap().name, "one");
    assert_eq!(initial.focused_window().unwrap().title, "Editor");
    assert_eq!(
        initial.focused_window().unwrap().app_id,
        "org.example.Editor"
    );
    assert!(server.request_seen("j/monitors"));
    assert!(server.request_seen("j/workspaces"));
    assert!(server.request_seen("j/clients"));
    assert!(server.request_seen("j/activewindow"));
    // The initial snapshot acknowledges all initial queries, with no events queued.
    let request_count = server.requests.lock().unwrap().len();
    thread::sleep(Duration::from_millis(150));
    assert_eq!(server.requests.lock().unwrap().len(), request_count);

    let (events_tx, events_rx) = mpsc::channel();
    let _subscription = backend.subscribe(Box::new(move |event| {
        let _ = events_tx.send(event);
    }));
    {
        let mut fixtures = server.fixtures.lock().unwrap();
        fixtures.monitors[0]["activeWorkspace"] = json!({"id":2,"name":"two"});
        fixtures.clients[0]["title"] = json!("Editor — updated");
    }
    server.event("windowtitlev2>>0xabc,Editor — updated");
    server.event("workspacev2>>2,two");
    let changed = wait_for(
        || {
            let snapshot = backend.snapshot();
            (snapshot
                .focused_window()
                .is_some_and(|w| w.title == "Editor — updated")
                && snapshot
                    .focused_workspace()
                    .is_some_and(|w| w.name == "two"))
            .then(|| snapshot.clone())
        },
        "Hyprland title and workspace event refresh",
    );
    assert!(matches!(
        events_rx.recv_timeout(Duration::from_secs(1)),
        Ok(Event::Snapshot(_))
    ));

    {
        let mut fixtures = server.fixtures.lock().unwrap();
        fixtures.activewindow = json!({});
    }
    server.event("activewindowv2>>");
    wait_for(
        || {
            backend
                .snapshot()
                .windows
                .iter()
                .all(|window| !window.focused)
                .then_some(())
        },
        "Hyprland cleared active window",
    );
    {
        let mut fixtures = server.fixtures.lock().unwrap();
        fixtures.activewindow = json!({"address":"0xabc"});
    }
    server.event("activewindowv2>>0xabc");
    wait_for(
        || {
            backend
                .snapshot()
                .focused_window()
                .is_some_and(|window| window.title == "Editor — updated")
                .then_some(())
        },
        "Hyprland restored active window",
    );

    let old_workspace = changed.workspaces[0].id.clone();
    let window_id = changed.windows[0].id.clone();
    let output_id = changed.outputs[0].id.clone();
    backend
        .action(Action::SwitchWorkspace {
            workspace: old_workspace.clone(),
        })
        .unwrap();
    backend
        .action(Action::SwitchWorkspaceIndex { index: 2 })
        .unwrap();
    backend
        .action(Action::FocusWindow {
            window: window_id.clone(),
        })
        .unwrap();
    backend
        .action(Action::FocusOutput { output: output_id })
        .unwrap();
    wait_for(
        || {
            let requests = server.requests.lock().unwrap();
            [
                "/dispatch workspace 1",
                "/dispatch workspace 2",
                "/dispatch focuswindow address:0xabc",
                "/dispatch focusmonitor DP-1",
            ]
            .iter()
            .all(|request| requests.iter().any(|sent| sent == request))
            .then_some(())
        },
        "Hyprland action requests",
    );

    let epoch = changed.epoch;
    server.close_event();
    wait_for(
        || (server.event_count() >= 2).then_some(()),
        "Hyprland event socket reconnect",
    );
    wait_for(
        || {
            let snapshot = backend.snapshot();
            (snapshot.connected && snapshot.epoch > epoch).then_some(())
        },
        "Hyprland new connection epoch",
    );
    assert!(matches!(
        backend.action(Action::FocusWindow { window: window_id }),
        Err(bead_backends::Error::UnknownId(_))
    ));
}

#[test]
fn missing_niri_socket_retries_with_bounded_events_and_connects_when_available() {
    let temp = TempDir::new("niri-late-ipc");
    let path = temp.0.join("late.sock");
    let backend = bead_backends::niri::connect(&path);
    let (events_tx, events_rx) = mpsc::channel();
    let _subscription = backend.subscribe(Box::new(move |event| {
        let _ = events_tx.send(event);
    }));
    let start = Instant::now();
    let mut reconnects = 0;
    while start.elapsed() < Duration::from_millis(550) {
        if matches!(
            events_rx.recv_timeout(Duration::from_millis(25)),
            Ok(Event::Reconnecting { .. })
        ) {
            reconnects += 1;
        }
    }
    assert!(reconnects <= 4, "too many reconnect events: {reconnects}");
    assert!(!backend.snapshot().connected);

    let server = NiriServer::new(path);
    wait_for(
        || (server.event_count() > 0 && backend.snapshot().connected).then_some(()),
        "late niri socket availability",
    );
    drop(_subscription);
    drop(backend);
}
