// SPDX-License-Identifier: MIT OR Apache-2.0
//! Keeps a wallpaper renderer process alive without ever depending on it. The renderer is
//! spawned only while a source exists, restarted with backoff after a crash (the desired
//! source and state are replayed), and abandoned after too many quick failures so a broken
//! decoder cannot spin. The controller process never blocks on or panics because of the child.
use crate::protocol::{self, Command, Event};
use std::{
    io::{BufRead, BufReader, Write},
    process::{Child, ChildStdin, Stdio},
    sync::{
        Arc, Mutex,
        mpsc::{self, RecvTimeoutError, Sender},
    },
    thread::JoinHandle,
    time::{Duration, Instant},
};

pub struct Config {
    /// Builds the renderer command. Stdin/stdout are set by the supervisor.
    pub command: Box<dyn Fn() -> std::process::Command + Send>,
    /// Restart delays; the last repeats.
    pub backoff: Vec<Duration>,
    /// More than `max_restarts` exits inside `window` abandon the renderer until a new source.
    pub max_restarts: usize,
    pub window: Duration,
    /// Grace period after `Quit` before the child is killed.
    pub quit_grace: Duration,
}
impl Config {
    pub fn new(command: impl Fn() -> std::process::Command + Send + 'static) -> Self {
        Self {
            command: Box::new(command),
            backoff: vec![
                Duration::from_millis(250),
                Duration::from_secs(1),
                Duration::from_secs(4),
            ],
            max_restarts: 4,
            window: Duration::from_secs(60),
            quit_grace: Duration::from_secs(2),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Status {
    pub pid: Option<u32>,
    /// Number of times a child has been started.
    pub starts: u64,
    /// The renderer kept failing; the static fallback stays until a new source arrives.
    pub abandoned: bool,
    pub last_error: Option<String>,
}

enum Msg {
    Command(Command),
    Event(u64, Event),
    Closed(u64),
    Stop,
}

pub struct Supervisor {
    sender: Sender<Msg>,
    status: Arc<Mutex<Status>>,
    thread: Option<JoinHandle<()>>,
}
impl Supervisor {
    /// `on_event` runs on the supervisor thread for each renderer event.
    pub fn start(config: Config, on_event: impl Fn(Event) + Send + 'static) -> Self {
        let (sender, receiver) = mpsc::channel();
        let status = Arc::new(Mutex::new(Status::default()));
        let worker = Worker {
            config,
            receiver,
            events: sender.clone(),
            status: status.clone(),
            on_event: Box::new(on_event),
            source: None,
            state: None,
            child: None,
            generation: 0,
            restarts: Vec::new(),
            retry_at: None,
        };
        let thread = std::thread::Builder::new()
            .name("wallpaper-supervisor".into())
            .spawn(move || worker.run())
            .ok();
        Self {
            sender,
            status,
            thread,
        }
    }
    pub fn send(&self, command: Command) {
        let _ = self.sender.send(Msg::Command(command));
    }
    pub fn status(&self) -> Status {
        self.status.lock().map(|s| s.clone()).unwrap_or_default()
    }
}
impl Drop for Supervisor {
    fn drop(&mut self) {
        let _ = self.sender.send(Msg::Stop);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

struct Live {
    child: Child,
    stdin: ChildStdin,
}
struct Worker {
    config: Config,
    receiver: mpsc::Receiver<Msg>,
    events: Sender<Msg>,
    status: Arc<Mutex<Status>>,
    on_event: Box<dyn Fn(Event) + Send>,
    /// Desired state, replayed to every new child.
    source: Option<Command>,
    state: Option<Command>,
    child: Option<Live>,
    generation: u64,
    restarts: Vec<Instant>,
    retry_at: Option<Instant>,
}
impl Worker {
    fn update(&self, change: impl FnOnce(&mut Status)) {
        if let Ok(mut status) = self.status.lock() {
            change(&mut status);
        }
    }
    fn run(mut self) {
        loop {
            let wait = self.retry_at.map_or(Duration::from_secs(3600), |t| {
                t.saturating_duration_since(Instant::now())
            });
            match self.receiver.recv_timeout(wait) {
                Ok(Msg::Stop) | Err(RecvTimeoutError::Disconnected) => {
                    self.stop_child();
                    return;
                }
                Ok(Msg::Command(command)) => self.command(command),
                Ok(Msg::Event(generation, event)) => {
                    if generation == self.generation {
                        (self.on_event)(event);
                    }
                }
                Ok(Msg::Closed(generation)) => {
                    if generation == self.generation {
                        self.exited();
                    }
                }
                Err(RecvTimeoutError::Timeout) => {
                    if self.retry_at.take().is_some() {
                        self.ensure_child();
                    }
                }
            }
        }
    }
    fn wanted(&self) -> bool {
        matches!(self.source, Some(Command::Source { source: Some(_) }))
    }
    fn command(&mut self, command: Command) {
        match &command {
            Command::Source { source } => {
                self.source = Some(command.clone());
                self.state = None;
                self.restarts.clear();
                self.update(|s| s.abandoned = false);
                if source.is_none() {
                    self.retry_at = None;
                    self.stop_child();
                    return;
                }
            }
            Command::State { .. } => self.state = Some(command.clone()),
            Command::Quit => {
                self.source = None;
                self.stop_child();
                return;
            }
        }
        let existed = self.child.is_some();
        self.ensure_child();
        if existed {
            self.write(&command);
        }
    }
    fn ensure_child(&mut self) {
        if self.child.is_some() || !self.wanted() || self.status.lock().is_ok_and(|s| s.abandoned) {
            return;
        }
        let mut command = (self.config.command)();
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit());
        let mut child = match command.spawn() {
            Ok(child) => child,
            Err(error) => {
                self.update(|s| s.last_error = Some(format!("spawn failed: {error}")));
                self.exited();
                return;
            }
        };
        let (Some(stdin), Some(stdout)) = (child.stdin.take(), child.stdout.take()) else {
            let _ = child.kill();
            let _ = child.wait();
            self.exited();
            return;
        };
        self.generation += 1;
        let generation = self.generation;
        let events = self.events.clone();
        let reader = std::thread::Builder::new()
            .name("wallpaper-events".into())
            .spawn(move || {
                for line in BufReader::new(stdout).lines() {
                    let Ok(line) = line else { break };
                    if let Ok(event) = protocol::decode::<Event>(&line) {
                        let _ = events.send(Msg::Event(generation, event));
                    }
                }
                let _ = events.send(Msg::Closed(generation));
            });
        if reader.is_err() {
            let _ = child.kill();
            let _ = child.wait();
            self.exited();
            return;
        }
        let pid = child.id();
        self.update(|s| {
            s.pid = Some(pid);
            s.starts += 1;
        });
        self.child = Some(Live { child, stdin });
        // Replay the desired state to the new child.
        for command in [self.source.clone(), self.state.clone()]
            .into_iter()
            .flatten()
        {
            self.write(&command);
        }
    }
    fn write(&mut self, command: &Command) {
        let Some(live) = self.child.as_mut() else {
            return;
        };
        let line = protocol::encode(command);
        if live.stdin.write_all(line.as_bytes()).is_err() || live.stdin.flush().is_err() {
            // The reader thread reports the exit; nothing else to do here.
        }
    }
    fn exited(&mut self) {
        let status = self.child.take().map(|mut live| {
            drop(live.stdin);
            live.child.wait().ok()
        });
        let now = Instant::now();
        self.restarts
            .retain(|t| now.duration_since(*t) < self.config.window);
        self.restarts.push(now);
        self.update(|s| {
            s.pid = None;
            if let Some(Some(code)) = status {
                s.last_error = Some(format!("renderer exited: {code}"));
            }
        });
        if !self.wanted() {
            return;
        }
        if self.restarts.len() > self.config.max_restarts {
            self.update(|s| s.abandoned = true);
            self.retry_at = None;
            return;
        }
        let index = (self.restarts.len() - 1).min(self.config.backoff.len().saturating_sub(1));
        let delay = self.config.backoff.get(index).copied().unwrap_or_default();
        self.retry_at = Some(now + delay);
    }
    fn stop_child(&mut self) {
        let Some(mut live) = self.child.take() else {
            return;
        };
        self.generation += 1;
        let _ = live
            .stdin
            .write_all(protocol::encode(&Command::Quit).as_bytes());
        drop(live.stdin);
        let deadline = Instant::now() + self.config.quit_grace;
        loop {
            match live.child.try_wait() {
                Ok(Some(_)) => break,
                Ok(None) if Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(20))
                }
                _ => {
                    let _ = live.child.kill();
                    let _ = live.child.wait();
                    break;
                }
            }
        }
        self.update(|s| s.pid = None);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{Decoder, Fit, Source};
    use std::process::Command as Process;

    fn video(path: &str) -> Command {
        Command::Source {
            source: Some(Source::Video {
                path: path.into(),
                fit: Fit::Crop,
                looped: true,
            }),
        }
    }
    fn fast(command: impl Fn() -> Process + Send + 'static) -> Config {
        Config {
            backoff: vec![Duration::from_millis(20)],
            window: Duration::from_secs(30),
            quit_grace: Duration::from_millis(500),
            ..Config::new(command)
        }
    }
    fn wait(mut done: impl FnMut() -> bool) -> bool {
        let end = Instant::now() + Duration::from_secs(10);
        while Instant::now() < end {
            if done() {
                return true;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        false
    }
    fn recorder(file: &std::path::Path) -> impl Fn() -> Process + Send + 'static + use<> {
        let file = file.to_owned();
        move || {
            let mut p = Process::new("sh");
            // Appends every command line, then the pid, so a restart replays visibly.
            p.arg("-c")
                .arg("echo '{\"event\":\"ready\",\"pid\":1}'; cat >> \"$1\"")
                .arg("sh")
                .arg(&file);
            p
        }
    }

    #[test]
    fn a_killed_renderer_is_restarted_and_given_the_desired_state_again() {
        let dir = std::env::temp_dir().join(format!("bead-wp-sup-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let log = dir.join("stdin.log");
        let ready = Arc::new(Mutex::new(0u32));
        let seen = ready.clone();
        let sup = Supervisor::start(fast(recorder(&log)), move |e| {
            if matches!(e, Event::Ready { .. }) {
                *seen.lock().unwrap() += 1;
            }
        });
        // Nothing runs before a source exists.
        assert_eq!(sup.status().starts, 0);
        sup.send(video("/clip.mp4"));
        sup.send(Command::State {
            decoder: Decoder::Play,
            render: vec!["DP-1".into()],
        });
        assert!(wait(
            || sup.status().pid.is_some() && *ready.lock().unwrap() == 1
        ));
        let first = sup.status().pid.unwrap();
        kill9(first);
        assert!(wait(
            || sup.status().starts == 2 && *ready.lock().unwrap() == 2
        ));
        assert_ne!(sup.status().pid, Some(first));
        assert!(!sup.status().abandoned);
        assert!(wait(|| {
            let log = std::fs::read_to_string(&log).unwrap_or_default();
            log.matches("\"cmd\":\"source\"").count() >= 2
                && log.matches("\"cmd\":\"state\"").count() >= 2
        }));
        drop(sup);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_renderer_that_keeps_crashing_is_abandoned_until_a_new_source() {
        let mut config = fast(|| {
            let mut p = Process::new("sh");
            p.arg("-c").arg("exit 3");
            p
        });
        config.max_restarts = 2;
        let sup = Supervisor::start(config, |_| {});
        sup.send(video("/clip.mp4"));
        assert!(wait(|| sup.status().abandoned));
        let starts = sup.status().starts;
        std::thread::sleep(Duration::from_millis(200));
        assert_eq!(sup.status().starts, starts, "no spin after giving up");
        assert!(sup.status().last_error.is_some());
        sup.send(video("/other.mp4"));
        assert!(wait(|| sup.status().starts > starts));
        sup.send(Command::Source { source: None });
        assert!(wait(|| sup.status().pid.is_none()));
    }

    #[test]
    fn clearing_the_source_stops_the_renderer_and_a_missing_binary_does_not_panic() {
        let dir = std::env::temp_dir().join(format!("bead-wp-sup2-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let log = dir.join("stdin.log");
        let sup = Supervisor::start(fast(recorder(&log)), |_| {});
        sup.send(video("/clip.mp4"));
        assert!(wait(|| sup.status().pid.is_some()));
        sup.send(Command::Source { source: None });
        assert!(wait(|| sup.status().pid.is_none()));
        assert_eq!(sup.status().starts, 1);
        drop(sup);
        let missing = Supervisor::start(
            Config {
                max_restarts: 1,
                ..fast(|| Process::new("/nonexistent/bead-wallpaper-video"))
            },
            |_| {},
        );
        missing.send(video("/clip.mp4"));
        assert!(wait(|| missing.status().abandoned));
        assert!(
            missing
                .status()
                .last_error
                .unwrap()
                .contains("spawn failed")
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Test-only SIGKILL of a pid the supervisor itself spawned.
    fn kill9(pid: u32) {
        let status = Process::new("kill").arg("-9").arg(pid.to_string()).status();
        assert!(status.unwrap().success());
    }
}
