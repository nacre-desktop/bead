// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use bead::backends::{Id, stable_id};
use serde_json::Value;
fn records(name: &str) -> Vec<Value> {
    let text = match name {
        "niri" => include_str!("fixtures/niri-ipc.jsonl"),
        _ => include_str!("fixtures/hyprland-ipc.jsonl"),
    };
    text.lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect()
}
#[test]
fn recorded_niri_events_preserve_ids_when_indices_and_outputs_change() {
    let mut reducer = niri::Reducer {
        epoch: 1,
        ..Default::default()
    };
    let mut had_two = false;
    let mut saw_focus = false;
    let mut closed = false;
    for row in records("niri") {
        if row["kind"] != "niri-event" {
            continue;
        }
        let event: niri_ipc::Event = serde_json::from_value(row["data"].clone()).unwrap();
        if matches!(event, niri_ipc::Event::WindowClosed { .. }) {
            closed = true;
        }
        reducer.apply(event);
        let snapshot = reducer.snapshot();
        had_two |= snapshot.windows.len() >= 2;
        saw_focus |= snapshot.focused_window().is_some();
    }
    assert!(had_two && saw_focus && closed);
    let original = reducer.snapshot();
    let mut ws = reducer.workspaces.values().next().unwrap().clone();
    let id = stable_id("niri", 1, "workspace", ws.id);
    ws.idx = 8;
    ws.output = Some("other-output".into());
    reducer.apply(niri_ipc::Event::WorkspacesChanged {
        workspaces: vec![ws],
    });
    let changed = reducer.snapshot();
    assert_eq!(changed.workspaces[0].id, id);
    assert_ne!(changed.workspaces[0].index, original.workspaces[0].index);
    reducer.epoch = 2;
    assert_ne!(
        reducer.snapshot().workspaces[0].id,
        id,
        "restart must not reuse old-session ID"
    );
    let window = original.windows.first().unwrap();
    assert!(matches!(
        niri::encode_action(
            &original,
            &Action::WindowTrash {
                window: window.id.clone()
            }
        ),
        Err(Error::Unsupported { .. })
    ));
}
#[test]
fn recorded_hyprland_replies_and_event_vocabulary_normalize_and_validate_actions() {
    let rows = records("hyprland");
    let mut data = std::collections::BTreeMap::new();
    let mut snapshots = 0;
    let mut focus_events = 0;
    for row in rows {
        if row["kind"] == "hyprland-event" {
            if row["data"].as_str().unwrap().starts_with("activewindowv2") {
                focus_events += 1;
            }
            continue;
        }
        let Some(command) = row["data"]["command"].as_str() else {
            continue;
        };
        if let Some(name) = command.strip_prefix("j/") {
            data.insert(
                name.to_owned(),
                serde_json::from_str::<Value>(row["data"]["reply"].as_str().unwrap()).unwrap(),
            );
        }
        if ["monitors", "workspaces", "clients", "activewindow"]
            .iter()
            .all(|k| data.contains_key(*k))
        {
            let snapshot = hyprland::normalize(
                1,
                &data["monitors"],
                &data["workspaces"],
                &data["clients"],
                &data["activewindow"],
            )
            .unwrap();
            snapshots += 1;
            if let Some(window) = snapshot.windows.first() {
                let encoded = hyprland::encode_action(
                    &snapshot,
                    &Action::FocusWindow {
                        window: window.id.clone(),
                    },
                )
                .unwrap();
                assert!(encoded.contains("hl.dsp.focus") && encoded.contains("address:0x"));
                assert!(matches!(
                    hyprland::encode_action(
                        &snapshot,
                        &Action::WindowCapture {
                            window: window.id.clone()
                        }
                    ),
                    Err(Error::Unsupported { .. })
                ));
            }
        }
    }
    assert!(snapshots > 0 && focus_events > 0);
    assert!(hyprland::relevant("monitorremoved"));
    assert!(!hyprland::relevant("unrelated"));
    let snapshot = hyprland::normalize(
        1,
        &data["monitors"],
        &data["workspaces"],
        &data["clients"],
        &data["activewindow"],
    )
    .unwrap();
    assert!(matches!(
        hyprland::encode_action(
            &snapshot,
            &Action::FocusWindow {
                window: Id("hyprland:2:window:0x123".into())
            }
        ),
        Err(Error::UnknownId(_))
    ));
}
#[test]
fn both_transports_reconnect_after_recorded_socket_sessions_and_output_hotplug() {
    use std::{
        io::{BufRead, BufReader, Read, Write},
        os::unix::net::{UnixListener, UnixStream},
        sync::{
            Arc, Mutex,
            atomic::{AtomicBool, Ordering},
        },
        thread,
        time::{Duration, Instant},
    };
    fn wait(client: &Native, predicate: impl Fn(&Snapshot) -> bool) {
        let end = Instant::now() + Duration::from_secs(8);
        while Instant::now() < end {
            if predicate(&client.snapshot()) {
                return;
            }
            thread::sleep(Duration::from_millis(10));
        }
        panic!("reconnect snapshot timeout: {:?}", client.snapshot());
    }
    for name in ["hyprland", "niri"] {
        let directory =
            std::env::temp_dir().join(format!("bead-reconnect-{}-{name}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        let sockets = Arc::new(Mutex::new(Vec::<UnixStream>::new()));
        let (event_ready, event_started) = std::sync::mpsc::channel();
        let stop = Arc::new(AtomicBool::new(false));
        let (path, event_path) = if name == "hyprland" {
            (
                directory.join(".socket.sock"),
                directory.join(".socket2.sock"),
            )
        } else {
            let path = directory.join("niri.sock");
            (path.clone(), path)
        };
        let all = records(name);
        let shared_records = Arc::new(all);
        let start_server = |path: std::path::PathBuf, event: bool| {
            if path.exists() {
                std::fs::remove_file(&path).unwrap();
            }
            let listener = UnixListener::bind(&path).unwrap();
            listener.set_nonblocking(true).unwrap();
            let sockets = sockets.clone();
            let stop = stop.clone();
            let records = shared_records.clone();
            let name = name.to_owned();
            let event_ready = event_ready.clone();
            thread::spawn(move || {
                while !stop.load(Ordering::SeqCst) {
                    match listener.accept() {
                        Ok((stream, _)) => {
                            if name == "niri" || event {
                                sockets.lock().unwrap().push(stream.try_clone().unwrap());
                            }
                            let records = records.clone();
                            let name = name.clone();
                            let stop = stop.clone();
                            let event_ready = event_ready.clone();
                            thread::spawn(move || {
                                if name == "hyprland" && event {
                                    let _ = event_ready.send(());
                                    while !stop.load(Ordering::SeqCst) {
                                        thread::sleep(Duration::from_millis(10));
                                    }
                                    return;
                                }
                                if name == "hyprland" {
                                    let mut stream = stream;
                                    let mut bytes = [0; 8192];
                                    let n = stream.read(&mut bytes).unwrap_or(0);
                                    let command = String::from_utf8_lossy(&bytes[..n]);
                                    let reply_for = |command: &str| {
                                        records
                                            .iter()
                                            .find(|r| {
                                                r["kind"] == "hyprland-reply"
                                                    && r["data"]["command"].as_str()
                                                        == Some(command)
                                            })
                                            .and_then(|r| r["data"]["reply"].as_str())
                                            .unwrap_or("ok")
                                    };
                                    let reply =
                                        if let Some(batch) = command.strip_prefix("[[BATCH]]") {
                                            batch
                                                .split(';')
                                                .map(reply_for)
                                                .collect::<Vec<_>>()
                                                .join("\n\n\n")
                                        } else {
                                            reply_for(&command).to_owned()
                                        };
                                    let _ = stream.write_all(reply.as_bytes());
                                    return;
                                }
                                let mut reader = BufReader::new(stream);
                                let mut line = String::new();
                                while reader.read_line(&mut line).unwrap_or(0) > 0 {
                                    let request: niri_ipc::Request =
                                        serde_json::from_str(&line).unwrap();
                                    line.clear();
                                    if matches!(request, niri_ipc::Request::EventStream) {
                                        let _ = event_ready.send(());
                                        let _ =
                                            reader.get_mut().write_all(b"{\"Ok\":\"Handled\"}\n");
                                        for kind in ["WorkspacesChanged", "WindowsChanged"] {
                                            if let Some(record) = records.iter().find(|r| {
                                                r["kind"] == "niri-event"
                                                    && r["data"].get(kind).is_some()
                                            }) {
                                                let text = serde_json::to_string(&record["data"])
                                                    .unwrap()
                                                    + "\n";
                                                let _ = reader.get_mut().write_all(text.as_bytes());
                                            }
                                        }
                                        while !stop.load(Ordering::SeqCst) {
                                            thread::sleep(Duration::from_millis(10));
                                        }
                                        return;
                                    }
                                    let reply = records
                                        .iter()
                                        .find(|r| {
                                            r["kind"] == "niri-reply"
                                                && r["data"]["Ok"].get("Outputs").is_some()
                                        })
                                        .unwrap();
                                    let text =
                                        serde_json::to_string(&reply["data"]).unwrap() + "\n";
                                    if reader.get_mut().write_all(text.as_bytes()).is_err() {
                                        return;
                                    }
                                }
                            });
                        }
                        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                            thread::sleep(Duration::from_millis(5))
                        }
                        Err(_) => break,
                    }
                }
            })
        };
        let mut servers = vec![start_server(path.clone(), false)];
        if name == "hyprland" {
            servers.push(start_server(event_path.clone(), true));
        }
        let client = if name == "hyprland" {
            Native::hyprland(directory.clone())
        } else {
            Native::niri(path.clone())
        };
        wait(&client, |s| s.connected);
        let old = client.snapshot();
        let old_epoch = old.epoch;
        // A dropped connection is not a new compositor session: identities survive.
        let disconnects = Arc::new(std::sync::atomic::AtomicU64::new(0));
        let count = disconnects.clone();
        let _subscription = client.subscribe(Box::new(move |event| {
            if matches!(event, Event::Disconnected(_)) {
                count.fetch_add(1, Ordering::SeqCst);
            }
        }));
        // Initial snapshot readiness can precede event-stream connection.
        // Drop a stream that the fixture has actually accepted, not just a
        // completed command socket. The disconnect assertion stays strict.
        event_started.recv_timeout(Duration::from_secs(8)).unwrap();
        for socket in sockets.lock().unwrap().drain(..) {
            let _ = socket.shutdown(std::net::Shutdown::Both);
        }
        let deadline = Instant::now() + Duration::from_secs(8);
        while disconnects.load(Ordering::SeqCst) == 0 && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(10));
        }
        assert!(disconnects.load(Ordering::SeqCst) > 0);
        wait(&client, |s| s.connected);
        assert_eq!(client.snapshot().epoch, old_epoch);
        assert_eq!(client.snapshot().workspaces[0].id, old.workspaces[0].id);
        stop.store(true, Ordering::SeqCst);
        for socket in sockets.lock().unwrap().drain(..) {
            let _ = socket.shutdown(std::net::Shutdown::Both);
        }
        for server in servers.drain(..) {
            server.join().unwrap();
        }
        std::fs::remove_file(&path).unwrap();
        if name == "hyprland" {
            std::fs::remove_file(&event_path).unwrap();
        }
        wait(&client, |s| !s.connected);
        stop.store(false, Ordering::SeqCst);
        servers.push(start_server(path.clone(), false));
        if name == "hyprland" {
            servers.push(start_server(event_path.clone(), true));
        }
        wait(&client, |s| s.connected && s.epoch != old_epoch);
        assert_ne!(client.snapshot().workspaces[0].id, old.workspaces[0].id);
        drop(client);
        stop.store(true, Ordering::SeqCst);
        for socket in sockets.lock().unwrap().drain(..) {
            let _ = socket.shutdown(std::net::Shutdown::Both);
        }
        for server in servers {
            server.join().unwrap();
        }
        std::fs::remove_dir_all(directory).unwrap();
    }
}

#[test]
fn subscriptions_can_read_snapshots_and_unregister() {
    let shared = Shared::new("test");
    let reads = Arc::new(std::sync::atomic::AtomicU64::new(0));
    let count = reads.clone();
    let weak = Arc::downgrade(&shared);
    let subscription = shared.subscribe(Box::new(move |_| {
        let _snapshot = weak.upgrade().unwrap().snapshot.lock().unwrap().clone();
        count.fetch_add(1, Ordering::SeqCst);
    }));
    assert_eq!(reads.load(Ordering::SeqCst), 1);
    drop(subscription);
    shared.emit(Event::Reconnecting { attempt: 1 });
    assert_eq!(reads.load(Ordering::SeqCst), 1);
    assert!(shared.callbacks.lock().unwrap().is_empty());
}

#[test]
fn recorded_topology_can_hotplug_and_move_without_rekeying_workspaces() {
    let rows = records("hyprland");
    let reply = |command| {
        let row = rows
            .iter()
            .find(|r| r["data"]["command"] == command)
            .unwrap();
        serde_json::from_str::<Value>(row["data"]["reply"].as_str().unwrap()).unwrap()
    };
    let monitors = reply("j/monitors");
    let mut workspaces = reply("j/workspaces");
    let before = hyprland::normalize(
        1,
        &monitors,
        &workspaces,
        &reply("j/clients"),
        &reply("j/activewindow"),
    )
    .unwrap();
    let mut added = monitors.clone();
    let mut second = added[0].clone();
    second["name"] = "hotplug".into();
    added.as_array_mut().unwrap().push(second);
    workspaces[0]["monitor"] = "hotplug".into();
    workspaces[0]["name"] = "renamed".into();
    let moved = hyprland::normalize(
        1,
        &added,
        &workspaces,
        &reply("j/clients"),
        &reply("j/activewindow"),
    )
    .unwrap();
    assert_eq!(moved.outputs.len(), before.outputs.len() + 1);
    assert_eq!(moved.workspaces[0].id, before.workspaces[0].id);
    assert_ne!(moved.workspaces[0].output, before.workspaces[0].output);
    let removed = hyprland::normalize(
        1,
        &monitors,
        &workspaces,
        &reply("j/clients"),
        &reply("j/activewindow"),
    )
    .unwrap();
    assert_eq!(removed.outputs.len(), before.outputs.len());
    assert!(removed.workspaces[0].output.is_none());

    let mut reducer = niri::Reducer {
        epoch: 1,
        ..Default::default()
    };
    for row in records("niri") {
        if row["kind"] == "niri-event" {
            reducer.apply(serde_json::from_value(row["data"].clone()).unwrap());
        } else if let Ok(Ok(niri_ipc::Response::Outputs(outputs))) =
            serde_json::from_value::<niri_ipc::Reply>(row["data"].clone())
        {
            reducer.outputs = outputs;
        }
    }
    let before = reducer.snapshot();
    let mut output = reducer.outputs.values().next().unwrap().clone();
    output.name = "hotplug".into();
    reducer.outputs.insert(output.name.clone(), output);
    let mut ws = reducer.workspaces.values().next().unwrap().clone();
    let ws_id = stable_id("niri", 1, "workspace", ws.id);
    ws.output = Some("hotplug".into());
    ws.idx = 7;
    reducer.apply(niri_ipc::Event::WorkspacesChanged {
        workspaces: vec![ws.clone()],
    });
    let moved = reducer.snapshot();
    assert_eq!(moved.outputs.len(), before.outputs.len() + 1);
    assert_eq!(moved.workspaces[0].id, ws_id);
    assert!(
        moved.workspaces[0]
            .output
            .as_ref()
            .unwrap()
            .0
            .ends_with(":hotplug")
    );
    reducer.outputs.remove("hotplug");
    ws.output = None;
    reducer.apply(niri_ipc::Event::WorkspacesChanged {
        workspaces: vec![ws],
    });
    assert_eq!(reducer.snapshot().outputs.len(), before.outputs.len());
    assert_eq!(reducer.snapshot().workspaces[0].id, ws_id);
    assert!(reducer.snapshot().workspaces[0].output.is_none());
}

#[test]
fn hyprland_batch_decodes_ordered_documents_and_rejects_partial_or_extra_replies() {
    use super::hyprland::normalize_batch;
    let monitors = r#"[{"name":"proof","width":2880,"height":1800,"scale":1.5,"activeWorkspace":{"id":1},"focused":true}]"#;
    let workspaces = r#"[{"id":1,"name":"one","monitor":"proof"}]"#;
    let clients = r#"[{"address":"0xabc","title":"space; braces {} and \n text","class":"fixture","workspace":{"id":1}}]"#;
    let active = r#"{"address":"0xabc"}"#;
    let reply = [monitors, workspaces, clients, active].join("\n\n\n");
    let snapshot = normalize_batch(7, &reply).unwrap();
    assert_eq!(snapshot.outputs.len(), 1);
    assert!(snapshot.workspaces[0].focused);
    assert!(snapshot.windows[0].focused);
    assert!(snapshot.windows[0].title.contains("braces {}"));
    assert!(normalize_batch(7, &reply[..reply.len() - 2]).is_err());
    assert!(normalize_batch(7, &(reply.clone() + " {} ")).is_err());
    assert!(normalize_batch(7, "[] [] [] null").is_err());
    assert!(normalize_batch(7, "[] [] []").is_err());
    assert!(normalize_batch(7, "ok").is_err());
}
