// SPDX-License-Identifier: GPL-3.0-only
//! Measurement-only stdin driver, not the shell's future public command surface.
use super::Native;
use bead_backends::{Action, Backend, Event, Snapshot};
use std::{
    io::BufRead,
    sync::{Arc, Condvar, Mutex},
    time::{Duration, Instant},
};
pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    let backend = Native::from_env()?;
    let changed = Arc::new((Mutex::new(0u64), Condvar::new()));
    let signal = changed.clone();
    let _subscription = backend.subscribe(Box::new(move |event| match event {
        Event::Snapshot(snapshot) => {
            match serde_json::to_string(&*snapshot) {
                Ok(json) => println!("BACKEND_SNAPSHOT {json}"),
                Err(error) => eprintln!("BACKEND_ERROR snapshot encode: {error}"),
            }
            let mut n = signal.0.lock().unwrap_or_else(crate::backends::poisoned);
            *n += 1;
            signal.1.notify_all();
        }
        Event::Disconnected(reason) => println!("BACKEND_DISCONNECTED {reason}"),
        Event::Reconnecting { attempt } => println!("BACKEND_RECONNECT attempt={attempt}"),
        Event::Error(error) => println!("BACKEND_ERROR {error}"),
    }));
    for line in std::io::stdin().lock().lines() {
        let line = line?;
        if line == "stop" {
            break;
        }
        let action: Action = serde_json::from_str(&line)?;
        let start = Instant::now();
        let action_ns = super::clock_ns(nix::libc::CLOCK_MONOTONIC);
        #[cfg(feature = "trace")]
        let cpu_ns = super::clock_ns(nix::libc::CLOCK_THREAD_CPUTIME_ID);
        let result = backend.action(action.clone());
        let request_ms = start.elapsed().as_secs_f64() * 1000.;
        if let Err(error) = result {
            println!(
                "BACKEND_RESULT {}",
                serde_json::json!({"ok":false,"error":error.to_string(),"request_ms":request_ms})
            );
            continue;
        }
        let mut version = changed.0.lock().unwrap_or_else(crate::backends::poisoned);
        let deadline = Instant::now() + Duration::from_secs(2);
        while !observed(&backend.snapshot(), &action) && Instant::now() < deadline {
            let (next, _) = changed
                .1
                .wait_timeout(version, deadline.saturating_duration_since(Instant::now()))
                .map_err(|_| "backend probe state poisoned")?;
            version = next;
        }
        let observed = observed(&backend.snapshot(), &action);
        let result = serde_json::json!({"ok":true,"observed":observed,"request_ms":request_ms,"action_ns":action_ns,"state_update_ms":if observed{Some(start.elapsed().as_secs_f64()*1000.)}else{None}});
        #[cfg(feature = "trace")]
        let result = {
            let mut result = result;
            result["thread_cpu_ms"] = serde_json::json!(
                (super::clock_ns(nix::libc::CLOCK_THREAD_CPUTIME_ID) - cpu_ns) as f64 / 1_000_000.
            );
            result
        };
        println!("BACKEND_RESULT {result}");
    }
    drop(_subscription);
    drop(backend);
    Ok(())
}
fn observed(snapshot: &Snapshot, action: &Action) -> bool {
    match action {
        Action::FocusWindow { window } => {
            snapshot.focused_window().is_some_and(|w| &w.id == window)
        }
        Action::CloseWindow { window } => snapshot.windows.iter().all(|w| &w.id != window),
        Action::SwitchWorkspace { workspace } => snapshot
            .focused_workspace()
            .is_some_and(|w| &w.id == workspace),
        Action::MoveWindow {
            window, workspace, ..
        } => snapshot
            .windows
            .iter()
            .find(|w| &w.id == window)
            .is_some_and(|w| w.workspace.as_ref() == Some(workspace)),
        Action::MoveWindowToOutput { window, output, .. } => snapshot
            .windows
            .iter()
            .find(|w| &w.id == window)
            .is_some_and(|w| w.output.as_ref() == Some(output)),
        Action::FocusOutput { output } => snapshot
            .outputs
            .iter()
            .any(|o| &o.id == output && o.focused),
        Action::SetFloating { window, floating } => snapshot
            .windows
            .iter()
            .find(|w| &w.id == window)
            .is_some_and(|w| w.floating == *floating),
        Action::SetFullscreen { window, fullscreen } => snapshot
            .windows
            .iter()
            .find(|w| &w.id == window)
            .is_some_and(|w| w.fullscreen == Some(*fullscreen)),
        _ => false,
    }
}
