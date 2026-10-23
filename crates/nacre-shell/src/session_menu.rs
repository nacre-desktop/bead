// SPDX-License-Identifier: GPL-3.0-only
//! B0023 session menu: lock, log out, suspend, restart, power off.
//!
//! Bead never locks or powers anything itself. Each action is one daemon call,
//! `session.request` v1 `{action, confirmed}` (N0031), enabled only while the
//! connected daemon advertises that exact method. Success is accepted only on
//! the published reply `{changed:true}`; every other outcome is an error and
//! nothing is shown as locked or done. Every action except lock is sent with
//! `confirmed:true`; log out, restart and power off only after the inline
//! confirmation (Enter/click again within `CONFIRM_TIMEOUT`; Esc, timeout or
//! another entry reverts). Lock and suspend act on the first activation.
use crate::i18n::tr;
use crate::nacre_source::{CallGuard, CallTicket, CallWake, Client};
use bead::core::{Value, source::Source};
use std::{
    cell::RefCell,
    collections::BTreeMap,
    time::{Duration, Instant},
};

pub const METHOD: &str = "session.request";
/// How long an inline confirmation waits before the item reverts.
const CONFIRM_TIMEOUT: Duration = Duration::from_secs(4);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Lock,
    Logout,
    Suspend,
    Reboot,
    Shutdown,
}
impl Action {
    fn parse(key: &str) -> Option<Self> {
        Some(match key {
            "lock" => Self::Lock,
            "logout" => Self::Logout,
            "suspend" => Self::Suspend,
            "reboot" => Self::Reboot,
            "shutdown" => Self::Shutdown,
            _ => return None,
        })
    }
    fn key(self) -> &'static str {
        match self {
            Self::Lock => "lock",
            Self::Logout => "logout",
            Self::Suspend => "suspend",
            Self::Reboot => "reboot",
            Self::Shutdown => "shutdown",
        }
    }
    /// Inline confirmation label (translated when projected) for the destructive actions only. Suspend is
    /// reversible and asks nothing, but N0031 still requires `confirmed:true` from the caller, so
    /// `needs_flag` is true for everything except lock.
    fn confirm(self) -> Option<&'static str> {
        match self {
            Self::Logout => Some("bead.panel.session-confirm-logout"),
            Self::Reboot => Some("bead.panel.session-confirm-reboot"),
            Self::Shutdown => Some("bead.panel.session-confirm-shutdown"),
            Self::Lock | Self::Suspend => None,
        }
    }
    fn needs_flag(self) -> bool {
        self != Self::Lock
    }
    fn failure(self) -> &'static str {
        match self {
            Self::Lock => "bead.panel.session-lock-failed",
            _ => "bead.panel.session-failed",
        }
    }
}

/// Fail-closed reading of a daemon reply. Anything but an explicit positive
/// answer for this action is an error; the daemon message is never shown.
fn verdict(
    action: Action,
    reply: &Result<serde_json::Value, bead::core::source::Error>,
) -> Result<(), &'static str> {
    match reply {
        Ok(value) if value.get("changed") == Some(&serde_json::Value::Bool(true)) => Ok(()),
        Ok(_) | Err(_) => Err(action.failure()),
    }
}

enum Phase {
    Idle,
    Confirm(Action, Instant),
    Pending(Action, CallTicket),
}
struct State {
    phase: Phase,
    message: String,
    done: bool,
}
pub struct SessionMenu {
    client: Client,
    confirm_timeout: Duration,
    state: RefCell<State>,
}
impl SessionMenu {
    pub fn new(client: Client) -> Self {
        Self {
            client,
            confirm_timeout: CONFIRM_TIMEOUT,
            state: RefCell::new(State {
                phase: Phase::Idle,
                message: String::new(),
                done: false,
            }),
        }
    }
    /// Connected daemon, known boot and the exact method at version 1.
    fn ready(&self) -> Option<(String, Duration)> {
        let status = self.client.status();
        let method = status.methods.get(METHOD).filter(|m| m.v == 1)?;
        if !self.client.snapshot().connected {
            return None;
        }
        Some((
            status.boot?,
            Duration::from_millis(u64::from(method.timeout).clamp(1, 10_000)),
        ))
    }
    pub fn subscribe(
        &self,
        notify: CallWake,
    ) -> bead::core::source::Result<bead::core::source::Subscription> {
        self.client
            .subscribe(std::sync::Arc::new(move |_| notify()))
    }
    #[cfg(test)]
    pub(crate) fn with_confirm_timeout(mut self, timeout: Duration) -> Self {
        self.confirm_timeout = timeout;
        self
    }
    #[cfg(test)]
    pub(crate) fn client(&self) -> Client {
        self.client.clone()
    }
    /// The action whose inline confirmation is showing, if any.
    pub fn confirming(&self) -> Option<&'static str> {
        match self.state.borrow().phase {
            Phase::Confirm(action, _) => Some(action.key()),
            _ => None,
        }
    }
    pub fn select(&self, key: &str, wake: CallWake) {
        let Some(action) = Action::parse(key) else {
            return;
        };
        if matches!(self.state.borrow().phase, Phase::Pending(..)) {
            return;
        }
        if self.ready().is_none() {
            let mut state = self.state.borrow_mut();
            state.phase = Phase::Idle;
            state.message = "bead.panel.session-unavailable".into();
            return;
        }
        if action.confirm().is_some() {
            let mut state = self.state.borrow_mut();
            state.phase = Phase::Confirm(action, Instant::now() + self.confirm_timeout);
            state.message.clear();
            // Wake the panel at the deadline so `project` can revert the item.
            let timeout = self.confirm_timeout;
            std::thread::spawn(move || {
                std::thread::sleep(timeout + Duration::from_millis(20));
                wake();
            });
        } else {
            self.request(action, wake);
        }
    }
    pub fn confirm(&self, wake: CallWake) {
        let Phase::Confirm(action, deadline) = self.state.borrow().phase else {
            return;
        };
        if Instant::now() >= deadline {
            self.state.borrow_mut().phase = Phase::Idle;
            return;
        }
        self.request(action, wake);
    }
    pub fn cancel(&self) {
        let mut state = self.state.borrow_mut();
        if matches!(state.phase, Phase::Confirm(..)) {
            state.phase = Phase::Idle;
        }
    }
    fn request(&self, action: Action, wake: CallWake) {
        let mut state = self.state.borrow_mut();
        state.message.clear();
        let Some((boot, timeout)) = self.ready() else {
            state.phase = Phase::Idle;
            state.message = "bead.panel.session-unavailable".into();
            return;
        };
        match self.client.begin_call(
            METHOD.into(),
            if action.needs_flag() {
                serde_json::json!({"action": action.key(), "confirmed": true})
            } else {
                serde_json::json!({"action": action.key()})
            },
            timeout,
            CallGuard { boot, version: 1 },
            Some(wake),
        ) {
            Ok(ticket) => state.phase = Phase::Pending(action, ticket),
            Err(_) => {
                state.phase = Phase::Idle;
                state.message = action.failure().into();
            }
        }
    }
    /// Poll the pending call, revalidate the capability, and project stores.
    /// A pending call is never retried; losing the capability or the link
    /// drops an unconfirmed prompt and surfaces a stale request as unconfirmed.
    pub fn project(&self) -> BTreeMap<String, Value> {
        let ready = self.ready().is_some();
        let mut state = self.state.borrow_mut();
        let state = &mut *state;
        if let Phase::Pending(action, ticket) = &mut state.phase
            && let Some(reply) = ticket.try_take()
        {
            match verdict(*action, &reply) {
                Ok(()) => state.done = true,
                Err(message) => state.message = message.into(),
            }
            state.phase = Phase::Idle;
        }
        if let Phase::Confirm(_, deadline) = state.phase
            && (!ready || Instant::now() >= deadline)
        {
            state.phase = Phase::Idle;
        }
        let (confirm, action) = match &state.phase {
            Phase::Confirm(action, _) => (
                action.confirm().map_or(String::new(), |id| tr(id, &[])),
                action.key(),
            ),
            _ => (String::new(), ""),
        };
        let message = if state.message.is_empty() {
            String::new()
        } else {
            tr(&state.message, &[])
        };
        let mut stores = BTreeMap::from([
            ("session.enabled".into(), Value::Bool(ready)),
            (
                "session.busy".into(),
                Value::Bool(matches!(state.phase, Phase::Pending(..))),
            ),
            ("session.confirm".into(), Value::Text(confirm)),
            ("session.confirm_action".into(), Value::Text(action.into())),
            ("session.message".into(), Value::Text(message)),
        ]);
        if std::mem::take(&mut state.done) {
            stores.insert("panel.open".into(), Value::Bool(false));
        }
        stores
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::nacre_source::Config;
    use nacre_protocol::{Capability, ErrorCode, Failure, Message, Reply, framing};
    use std::os::unix::fs::PermissionsExt;
    use std::sync::Arc;
    use tokio::{io::BufReader, net::UnixListener, sync::mpsc};

    type Calls = mpsc::UnboundedReceiver<serde_json::Value>;
    fn welcome(advertise: bool) -> Message {
        let mut methods = nacre_protocol::core_methods();
        if advertise {
            methods.insert(
                METHOD.into(),
                nacre_protocol::Method {
                    v: 1,
                    class: Capability::System,
                    timeout: 10_000,
                    params: serde_json::json!({"type":"object"}),
                    result: serde_json::json!({}),
                },
            );
        }
        Message::Welcome {
            proto: 1,
            min: 1,
            boot: "session-boot".into(),
            topics: Default::default(),
            methods,
            frame_ms: None,
        }
    }
    /// Fake daemon: records each call's params and answers from `replies`.
    pub(crate) async fn lab(
        advertise: bool,
        replies: Vec<Reply>,
    ) -> (std::path::PathBuf, SessionMenu, Calls) {
        let dir =
            std::env::temp_dir().join(format!("session-menu-{}-{}", std::process::id(), unique()));
        std::fs::create_dir(&dir).unwrap();
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();
        let socket = dir.join("daemon.sock");
        let listener = UnixListener::bind(&socket).unwrap();
        std::fs::set_permissions(&socket, std::fs::Permissions::from_mode(0o600)).unwrap();
        let (calls, received) = mpsc::unbounded_channel();
        tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let (read, mut writer) = stream.into_split();
            let mut reader = framing::Reader::new(BufReader::new(read));
            assert!(matches!(
                reader.next().await.unwrap().unwrap(),
                Message::Hello { .. }
            ));
            framing::write(&mut writer, &welcome(advertise))
                .await
                .unwrap();
            let mut replies = replies.into_iter();
            while let Ok(Some(message)) = reader.next().await {
                if let Message::Call { id, m, p, .. } = message {
                    assert_eq!(m, METHOD);
                    calls.send(p).unwrap();
                    let reply = replies.next().expect("unexpected extra call");
                    framing::write(&mut writer, &Message::Ret { id, reply })
                        .await
                        .unwrap();
                }
            }
        });
        let client = Client::start(Config::new(socket, vec![])).unwrap();
        let menu = SessionMenu::new(client.clone());
        for _ in 0..500 {
            if client.snapshot().connected && client.status().boot.is_some() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        (dir, menu, received)
    }
    fn unique() -> u64 {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    }
    pub(crate) fn ok(value: serde_json::Value) -> Reply {
        Reply::Ok { ok: value }
    }
    fn wake() -> CallWake {
        Arc::new(|| {})
    }
    fn text(stores: &BTreeMap<String, Value>, key: &str) -> String {
        match stores.get(key) {
            Some(Value::Text(text)) => text.clone(),
            other => panic!("{key}: {other:?}"),
        }
    }
    async fn settle(menu: &SessionMenu) -> BTreeMap<String, Value> {
        for _ in 0..500 {
            let stores = menu.project();
            if stores["session.busy"] == Value::Bool(false) {
                return stores;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        panic!("session request never completed");
    }

    #[test]
    fn verdict_accepts_only_the_explicit_positive_field() {
        let err = || Err(bead::core::source::Error::new("unavailable", "x", true));
        assert!(verdict(Action::Lock, &Ok(serde_json::json!({"changed":true}))).is_ok());
        assert!(verdict(Action::Reboot, &Ok(serde_json::json!({"changed":true}))).is_ok());
        for reply in [
            Ok(serde_json::json!({"changed":false})),
            Ok(serde_json::json!({"accepted":true})),
            Ok(serde_json::json!({"locked":true})),
            Ok(serde_json::json!({"changed":"true"})),
            Ok(serde_json::json!({})),
            Ok(serde_json::Value::Null),
            err(),
        ] {
            assert!(verdict(Action::Lock, &reply).is_err(), "{reply:?}");
        }
        assert!(verdict(Action::Suspend, &Ok(serde_json::json!({"locked":true}))).is_err());
        assert!(verdict(Action::Shutdown, &err()).is_err());
    }

    #[test]
    fn only_logout_restart_and_power_off_ask_first() {
        let asks: Vec<_> = ["lock", "logout", "suspend", "reboot", "shutdown"]
            .into_iter()
            .map(|key| Action::parse(key).unwrap().confirm().is_some())
            .collect();
        assert_eq!(asks, [false, true, false, true, true]);
        assert!(Action::parse("hibernate").is_none());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn disabled_until_the_method_is_advertised_and_never_calls() {
        let (dir, menu, mut calls) = lab(false, vec![]).await;
        let stores = menu.project();
        assert_eq!(stores["session.enabled"], Value::Bool(false));
        menu.select("lock", wake());
        menu.select("reboot", wake());
        let stores = menu.project();
        assert_eq!(
            text(&stores, "session.message"),
            "Session control unavailable"
        );
        assert_eq!(text(&stores, "session.confirm"), "");
        assert!(!stores.contains_key("panel.open"));
        assert!(calls.try_recv().is_err());
        std::fs::remove_dir_all(dir).ok();
    }

    #[tokio::test(flavor = "current_thread")]
    async fn lock_closes_only_on_confirmed_lock() {
        let (dir, menu, mut calls) = lab(true, vec![ok(serde_json::json!({"changed":true}))]).await;
        assert_eq!(menu.project()["session.enabled"], Value::Bool(true));
        menu.select("lock", wake());
        assert_eq!(
            calls.recv().await.unwrap(),
            serde_json::json!({"action":"lock"})
        );
        let stores = settle(&menu).await;
        assert_eq!(stores["panel.open"], Value::Bool(false));
        assert_eq!(text(&stores, "session.message"), "");
        std::fs::remove_dir_all(dir).ok();
    }

    #[tokio::test(flavor = "current_thread")]
    async fn unconfirmed_lock_fails_closed_and_is_never_retried() {
        let failure = Reply::Err {
            err: Failure::new(ErrorCode::Unavailable, "no hyprlock", false),
        };
        let (dir, menu, mut calls) = lab(
            true,
            vec![
                ok(serde_json::json!({"accepted":true})),
                ok(serde_json::json!({"changed":false})),
                failure,
            ],
        )
        .await;
        for _ in 0..3 {
            menu.select("lock", wake());
            calls.recv().await.unwrap();
            let stores = settle(&menu).await;
            assert!(
                !stores.contains_key("panel.open"),
                "must not close as locked"
            );
            assert!(text(&stores, "session.message").contains("not confirmed"));
            assert!(!text(&stores, "session.message").contains("hyprlock"));
        }
        assert!(calls.try_recv().is_err(), "no automatic retry");
        std::fs::remove_dir_all(dir).ok();
    }

    #[tokio::test(flavor = "current_thread")]
    async fn restart_needs_confirmation_cancel_sends_nothing_and_busy_blocks() {
        let (dir, menu, mut calls) = lab(true, vec![ok(serde_json::json!({"changed":true}))]).await;
        menu.select("reboot", wake());
        let stores = menu.project();
        assert_eq!(text(&stores, "session.confirm"), "Confirm restart");
        assert!(menu.confirming().is_some() && calls.try_recv().is_err());
        menu.cancel();
        assert!(menu.confirming().is_none());
        menu.confirm(wake());
        assert!(
            calls.try_recv().is_err(),
            "confirm without a prompt is inert"
        );
        menu.select("reboot", wake());
        assert_eq!(menu.confirming(), Some("reboot"));
        menu.confirm(wake());
        menu.select("shutdown", wake());
        menu.confirm(wake());
        assert_eq!(
            calls.recv().await.unwrap(),
            serde_json::json!({"action":"reboot","confirmed":true})
        );
        assert!(calls.try_recv().is_err(), "only one request in flight");
        let stores = settle(&menu).await;
        assert_eq!(stores["panel.open"], Value::Bool(false));
        std::fs::remove_dir_all(dir).ok();
    }

    #[tokio::test(flavor = "current_thread")]
    async fn nothing_is_sent_before_confirmation_and_only_confirm_sends() {
        let replies = vec![ok(serde_json::json!({"changed":true})); 2];
        let (dir, menu, mut calls) = lab(true, replies).await;
        for key in ["logout", "reboot", "shutdown"] {
            menu.select(key, wake());
            assert_eq!(menu.confirming(), Some(key));
            assert!(text(&menu.project(), "session.confirm").starts_with("Confirm "));
            tokio::time::sleep(Duration::from_millis(30)).await;
            assert!(calls.try_recv().is_err(), "{key} sent before confirmation");
            menu.cancel();
        }
        assert!(calls.try_recv().is_err());
        menu.select("logout", wake());
        menu.select("shutdown", wake()); // switching item replaces the prompt
        assert_eq!(menu.confirming(), Some("shutdown"));
        menu.confirm(wake());
        assert_eq!(
            calls.recv().await.unwrap(),
            serde_json::json!({"action":"shutdown","confirmed":true})
        );
        settle(&menu).await;
        // Suspend needs no prompt but still carries the flag N0031 requires.
        menu.select("suspend", wake());
        assert!(menu.confirming().is_none());
        assert_eq!(
            calls.recv().await.unwrap(),
            serde_json::json!({"action":"suspend","confirmed":true})
        );
        settle(&menu).await;
        assert!(calls.try_recv().is_err());
        std::fs::remove_dir_all(dir).ok();
    }

    #[tokio::test(flavor = "current_thread")]
    async fn an_unanswered_confirmation_times_out_and_then_sends_nothing() {
        let (dir, menu, mut calls) = lab(true, vec![]).await;
        let menu = menu.with_confirm_timeout(Duration::from_millis(80));
        let woke = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let flag = woke.clone();
        menu.select(
            "logout",
            Arc::new(move || flag.store(true, std::sync::atomic::Ordering::SeqCst)),
        );
        assert_eq!(text(&menu.project(), "session.confirm"), "Confirm log out");
        tokio::time::sleep(Duration::from_millis(200)).await;
        assert!(
            woke.load(std::sync::atomic::Ordering::SeqCst),
            "deadline wake"
        );
        let stores = menu.project();
        assert_eq!(text(&stores, "session.confirm"), "");
        assert_eq!(text(&stores, "session.confirm_action"), "");
        menu.confirm(wake());
        assert!(calls.try_recv().is_err(), "late confirm is inert");
        std::fs::remove_dir_all(dir).ok();
    }
}
