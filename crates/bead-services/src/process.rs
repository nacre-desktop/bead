// SPDX-License-Identifier: MIT OR Apache-2.0
//! Bounded argv execution. Only the process group created here is terminated.
use std::{
    io::{Read, Write},
    os::{fd::AsRawFd, unix::process::CommandExt},
    process::{Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
pub struct Limits {
    pub timeout: Duration,
    pub input: usize,
    pub output: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(5),
            input: 4 * 1024 * 1024,
            output: 8 * 1024 * 1024,
        }
    }
}
#[derive(Debug)]
pub struct Output {
    pub code: i32,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}
fn capture(
    mut stream: impl Read + AsRawFd + Send + 'static,
    limit: usize,
    overflow: Arc<AtomicBool>,
    stopped: Arc<AtomicBool>,
) -> std::thread::JoinHandle<std::io::Result<Vec<u8>>> {
    std::thread::spawn(move || {
        let fd = stream.as_raw_fd();
        unsafe {
            libc::fcntl(
                fd,
                libc::F_SETFL,
                libc::fcntl(fd, libc::F_GETFL) | libc::O_NONBLOCK,
            );
        }
        let mut bytes = Vec::new();
        let mut buf = [0; 8192];
        loop {
            let n = match stream.read(&mut buf) {
                Ok(n) => n,
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    if stopped.load(Ordering::Acquire) {
                        break;
                    }
                    let mut p = libc::pollfd {
                        fd,
                        events: libc::POLLIN,
                        revents: 0,
                    };
                    unsafe {
                        libc::poll(&mut p, 1, 20);
                    }
                    continue;
                }
                Err(e) => return Err(e),
            };
            if n == 0 {
                break;
            }
            let space = limit.saturating_sub(bytes.len());
            bytes.extend_from_slice(&buf[..n.min(space)]);
            if n > space {
                overflow.store(true, Ordering::Release);
            }
        }
        Ok(bytes)
    })
}
/// No shell is inserted. The caller selects the program, arguments and environment.
/// Spawn configuration for event-loop actors. Clear inherited blocked signals
/// in the child only; the parent's signal handling and mask stay unchanged.
pub fn command(program: impl AsRef<std::ffi::OsStr>) -> Command {
    let mut command = Command::new(program);
    // SAFETY: only async-signal-safe signal operations execute after fork.
    unsafe {
        command.pre_exec(|| {
            let mut empty = std::mem::MaybeUninit::<libc::sigset_t>::uninit();
            if libc::sigemptyset(empty.as_mut_ptr()) != 0
                || libc::sigprocmask(libc::SIG_SETMASK, empty.as_ptr(), std::ptr::null_mut()) != 0
            {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    command
}
pub fn run(command: &mut Command, input: &[u8], limits: Limits) -> Result<Output, String> {
    if limits.input > 4 * 1024 * 1024
        || input.len() > limits.input
        || limits.timeout > Duration::from_secs(60)
        || limits.output > 8 * 1024 * 1024
    {
        return Err("invalid process limits/input".into());
    }
    let mut child = command
        .process_group(0)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| e.to_string())?;
    let pid = child.id();
    let overflow = Arc::new(AtomicBool::new(false));
    let stopped = Arc::new(AtomicBool::new(false));
    let out = capture(
        child.stdout.take().ok_or("stdout missing")?,
        limits.output,
        overflow.clone(),
        stopped.clone(),
    );
    let err = capture(
        child.stderr.take().ok_or("stderr missing")?,
        limits.output,
        overflow.clone(),
        stopped.clone(),
    );
    let mut stdin = child.stdin.take().ok_or("stdin missing")?;
    let data = input.to_vec();
    let writing = stopped.clone();
    let writer = std::thread::spawn(move || {
        let fd = stdin.as_raw_fd();
        unsafe {
            libc::fcntl(
                fd,
                libc::F_SETFL,
                libc::fcntl(fd, libc::F_GETFL) | libc::O_NONBLOCK,
            );
        }
        let mut offset = 0;
        while offset < data.len() {
            match stdin.write(&data[offset..]) {
                Ok(0) => return Err(std::io::Error::from(std::io::ErrorKind::WriteZero)),
                Ok(n) => offset += n,
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    if writing.load(Ordering::Acquire) {
                        return Err(std::io::Error::from(std::io::ErrorKind::TimedOut));
                    }
                    let mut p = libc::pollfd {
                        fd,
                        events: libc::POLLOUT,
                        revents: 0,
                    };
                    unsafe {
                        libc::poll(&mut p, 1, 20);
                    }
                }
                Err(e) => return Err(e),
            }
        }
        Ok(())
    });
    let started = Instant::now();
    let fault = loop {
        if overflow.load(Ordering::Acquire) {
            break Some("process output limit exceeded".to_string());
        }
        if started.elapsed() >= limits.timeout {
            break Some("process deadline exceeded".to_string());
        }
        // WNOWAIT retains ownership of the PID until the whole group is stopped;
        // a reaped/reused PID can never become an unrelated termination target.
        let mut info = std::mem::MaybeUninit::<libc::siginfo_t>::zeroed();
        let result = unsafe {
            libc::waitid(
                libc::P_PID,
                pid,
                info.as_mut_ptr(),
                libc::WEXITED | libc::WNOWAIT | libc::WNOHANG,
            )
        };
        if result < 0 {
            break Some(std::io::Error::last_os_error().to_string());
        }
        if unsafe { info.assume_init().si_pid() } != 0 {
            break None;
        }
        std::thread::sleep(Duration::from_millis(5));
    };
    // The unreaped child reserves this group ID, including on normal completion.
    unsafe {
        libc::kill(-(pid as i32), libc::SIGKILL);
    }
    let status = child.wait().map_err(|e| e.to_string())?;
    stopped.store(true, Ordering::Release);
    let write = writer.join().map_err(|_| "stdin worker panicked")?;
    let stdout = out
        .join()
        .map_err(|_| "stdout worker panicked")?
        .map_err(|e| e.to_string())?;
    let stderr = err
        .join()
        .map_err(|_| "stderr worker panicked")?
        .map_err(|e| e.to_string())?;
    if let Some(error) = fault {
        return Err(error);
    }
    if overflow.load(Ordering::Acquire) {
        return Err("process output limit exceeded".into());
    }
    write.map_err(|e| e.to_string())?;
    Ok(Output {
        code: status.code().unwrap_or(128),
        stdout,
        stderr,
    })
}
#[derive(serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct HelperRequest {
    pub id: u64,
    pub tool: String,
    pub arguments: Vec<String>,
    pub input: Option<serde_json::Value>,
}
#[derive(serde::Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct HelperReply {
    pub id: u64,
    pub code: i32,
    pub output: String,
}
pub fn helper(
    command: &mut Command,
    request: &HelperRequest,
    limits: Limits,
) -> Result<HelperReply, String> {
    if request.id == 0
        || request.id > 9007199254740991
        || ![
            "menu-backend",
            "dashboard-data",
            "dashboard-media",
            "launcher-preferences",
            "notification-history",
            "network-state",
        ]
        .contains(&request.tool.as_str())
        || request.arguments.len() > 8
        || request
            .arguments
            .iter()
            .any(|a| a.len() > 65536 || a.contains('\0'))
    {
        return Err("invalid helper request".into());
    }
    let mut input = serde_json::to_vec(request).map_err(|e| e.to_string())?;
    input.push(b'\n');
    let out = run(command, &input, limits)?;
    if out.code != 0 {
        return Err(format!("helper exited {}", out.code));
    }
    let reply: HelperReply = serde_json::from_slice(&out.stdout).map_err(|e| e.to_string())?;
    if reply.id != request.id {
        return Err("helper response ID mismatch".into());
    }
    Ok(reply)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn argv_streams_and_bounded_descendants() {
        let out = run(
            Command::new("/bin/sh").args(["-c", "cat; printf err >&2"]),
            b"hello",
            Limits::default(),
        )
        .unwrap();
        assert_eq!(out.stdout, b"hello");
        assert_eq!(out.stderr, b"err");
        assert_eq!(out.code, 0);
        let start = Instant::now();
        assert!(
            run(
                Command::new("/bin/sh").args(["-c", "sleep 30 & wait"]),
                b"",
                Limits {
                    timeout: Duration::from_millis(80),
                    ..Limits::default()
                }
            )
            .is_err()
        );
        assert!(start.elapsed() < Duration::from_secs(2));
        assert!(
            run(
                Command::new("/bin/sh").args(["-c", "yes x"]),
                b"",
                Limits {
                    output: 4096,
                    ..Limits::default()
                }
            )
            .is_err()
        );
    }
    #[test]
    fn existing_helper_wire_format_and_id_validation() {
        let request = HelperRequest {
            id: 7,
            tool: "menu-backend".into(),
            arguments: vec!["convert".into(), "1m to cm".into()],
            input: None,
        };
        let script = "import sys,json; r=json.loads(sys.stdin.readline()); print(json.dumps({'id':r['id'],'code':0,'output':'100 cm\\n'}))";
        let out = helper(
            Command::new("python3").args(["-c", script]),
            &request,
            Limits::default(),
        )
        .unwrap();
        assert_eq!(out.output, "100 cm\n");
        assert!(
            helper(
                Command::new("python3")
                    .args(["-c", "print('{\"id\":8,\"code\":0,\"output\":\"\"}')"]),
                &request,
                Limits::default()
            )
            .is_err()
        );
    }
    #[test]
    fn child_clears_event_loop_mask_without_changing_parent() {
        struct Restore(libc::sigset_t);
        impl Drop for Restore {
            fn drop(&mut self) {
                unsafe {
                    libc::pthread_sigmask(libc::SIG_SETMASK, &self.0, std::ptr::null_mut());
                }
            }
        }
        let mut blocked = unsafe { std::mem::zeroed::<libc::sigset_t>() };
        let mut old = unsafe { std::mem::zeroed::<libc::sigset_t>() };
        unsafe {
            libc::sigemptyset(&mut blocked);
            libc::sigaddset(&mut blocked, libc::SIGTERM);
            assert_eq!(
                libc::pthread_sigmask(libc::SIG_BLOCK, &blocked, &mut old),
                0
            );
        }
        let _restore = Restore(old);
        let output = run(
            command("cat").arg("/proc/self/status"),
            b"",
            Limits::default(),
        )
        .unwrap();
        let text = String::from_utf8(output.stdout).unwrap();
        let mask = text
            .lines()
            .find_map(|l| l.strip_prefix("SigBlk:\t"))
            .unwrap();
        assert_eq!(
            u64::from_str_radix(mask.trim(), 16).unwrap() & (1 << (libc::SIGTERM - 1)),
            0
        );
        let mut current = unsafe { std::mem::zeroed::<libc::sigset_t>() };
        unsafe {
            libc::pthread_sigmask(libc::SIG_SETMASK, std::ptr::null(), &mut current);
            assert_eq!(libc::sigismember(&current, libc::SIGTERM), 1);
        }
    }
}
