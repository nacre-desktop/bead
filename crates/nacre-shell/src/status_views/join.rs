// SPDX-License-Identifier: GPL-3.0-only
//! B0050: password prompt, hidden-network join and saved-network forget for the network popout.
//! The view is Luau (`shell/status/network.luau` over kit components); this module is the Nacre
//! edge: it knows the daemon contract (N0042), keeps the join flow, and publishes it as stores.
//! The secret lives only in the `network.secret` input store until the call is built; it is
//! cleared by the host right after, is never put in a message, store projection or log, and the
//! `.secret` suffix keeps it out of traces and saved state.
use super::{
    LinkOp, LinkTarget, Status, WifiControl, line, link_error_id, resolve_link, sole_device, tr,
};
use crate::nacre_source::{CallGuard, CallWake, Client};
use bead::core::{Stores, Value};
use nacre_protocol::core_services::{
    EntityId, NetworkConnect, NetworkState, NetworkTarget, WifiSecurity,
};
use std::{
    collections::BTreeMap,
    time::{Duration, Instant},
};

pub(super) const CONFIRM_TIMEOUT: Duration = Duration::from_secs(4);
const MAX_SAVED_ROWS: usize = 6;
/// The single store that holds a typed password.
pub(crate) const SECRET_STORE: &str = "network.secret";

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Page {
    List,
    Prompt,
    Hidden,
    Saved,
}
impl Page {
    fn as_str(self) -> &'static str {
        match self {
            Self::List => "list",
            Self::Prompt => "prompt",
            Self::Hidden => "hidden",
            Self::Saved => "saved",
        }
    }
}
#[derive(Clone)]
pub(super) struct Prompt {
    pub key: String,
    pub label: String,
}
/// What to show again when a join is rejected asynchronously (wrong password).
pub(super) enum Reprompt {
    Prompt(Prompt),
    Hidden,
}
impl WifiControl {
    pub(super) fn reset_flow(&mut self) {
        self.page = Page::List;
        self.prompt = None;
        self.form_error = None;
        self.forget = None;
    }
    pub(super) fn reopen(&mut self, again: Reprompt) {
        match again {
            Reprompt::Prompt(prompt) => {
                self.prompt = Some(prompt);
                self.page = Page::Prompt;
            }
            Reprompt::Hidden => self.page = Page::Hidden,
        }
        self.form_error = Some("bead.panel.network-wrong-password");
    }
}

/// A connect to a secured access point without a saved profile needs a password first.
pub(super) fn secret_prompt(
    state: &NetworkState,
    key: &str,
    target: &LinkTarget,
) -> Option<Prompt> {
    if target.method != "network.connect" || target.params["target"]["kind"] != "access_point" {
        return None;
    }
    prompt_for(state, key).filter(|_| {
        key.strip_prefix("ap:")
            .and_then(|id| state.access_points.items.get(&EntityId(id.into())))
            .is_some_and(|ap| ap.secured)
    })
}
/// The password prompt for access point row `key` (`ap:<id>`).
pub(super) fn prompt_for(state: &NetworkState, key: &str) -> Option<Prompt> {
    let ap = state
        .access_points
        .items
        .get(&EntityId(key.strip_prefix("ap:")?.into()))?;
    Some(Prompt {
        key: key.into(),
        label: if ap.name.is_empty() {
            tr("bead.panel.network-hidden", &[])
        } else {
            line(&ap.name, 28)
        },
    })
}
/// The stable N0042 pre-flight refusal for a secured network without a secret.
pub(super) fn needs_secret(code: &str, message: &str) -> bool {
    code == "validation" && message == "Network secret required"
}

/// Message id for a rejected join. A needs-secret validation re-asks for the password.
pub(super) fn join_error_id(code: &str, message: &str) -> &'static str {
    if needs_secret(code, message) {
        return "bead.panel.network-secret-required";
    }
    match code {
        "validation" | "invalid_request" => "bead.panel.network-join-invalid",
        _ => link_error_id(code),
    }
}

/// Whether an admitted operation still waits for its readback.
pub(super) fn still_pending(state: &NetworkState, op: &LinkOp) -> bool {
    if let Some(ssid) = op.key.strip_prefix("hidden:") {
        let named = |name: &str| name == ssid;
        return !(state
            .access_points
            .items
            .values()
            .any(|ap| ap.active && named(&ap.name))
            || state
                .connections
                .items
                .values()
                .any(|c| c.active && c.ssid.as_deref().is_some_and(named)));
    }
    resolve_link(state, &op.key).is_some_and(|target| target.method == op.method)
}

fn advertised(client: &Client, name: &str) -> Option<u32> {
    client
        .status()
        .methods
        .get(name)
        .filter(|method| method.v == 1)
        .map(|method| method.timeout)
}

/// Send `target` as the next link operation; a rejected join keeps its form open with the reason.
#[allow(clippy::too_many_arguments)]
pub(super) fn dispatch(
    control: &mut WifiControl,
    client: &Client,
    boot: String,
    (method, timeout): (&str, u32),
    target: LinkTarget,
    key: String,
    reprompt: Option<Reprompt>,
    wake: CallWake,
) {
    match client.begin_call(
        method.into(),
        target.params,
        Duration::from_millis(u64::from(timeout.min(10_000))),
        CallGuard {
            boot: boot.clone(),
            version: 1,
        },
        Some(wake),
    ) {
        Ok(ticket) => {
            control.link = Some(LinkOp {
                boot,
                key,
                method: target.method,
                ticket: Some(ticket),
                deadline: None,
                reprompt,
            })
        }
        Err(error) if reprompt.is_some() => {
            control.form_error = Some(join_error_id(&error.code, &error.message));
        }
        Err(error) => control.link_message = Some(super::link_error_message(&error.code)),
    }
}

fn text(stores: &Stores, name: &str) -> String {
    match stores.get(name) {
        Some(Value::Text(text)) => text.clone(),
        _ => String::new(),
    }
}

impl Status {
    /// Fresh state plus the live control, or nothing when no flow action is possible.
    fn flow<R>(
        &self,
        f: impl FnOnce(&mut WifiControl, &Client, &str, &NetworkState) -> R,
    ) -> Option<R> {
        let control = self.wifi_control.as_ref()?;
        let snapshot = self.source.snapshot();
        let client = control.borrow().client.clone();
        let candidate = super::wifi_candidate(&snapshot, &client);
        let (boot, state) = (candidate.topic_boot.as_ref()?, candidate.state.as_ref()?);
        if !candidate.fresh {
            return None;
        }
        let mut control = control.borrow_mut();
        Some(f(&mut control, &client, boot, state))
    }
    /// Show the hidden-network form (`hidden`), the saved list (`saved`) or the list.
    pub fn network_open(&self, view: &str) {
        self.flow(|control, client, _, state| {
            control.reset_flow();
            match view {
                "hidden"
                    if advertised(client, "network.connect").is_some()
                        && sole_device(state, "wifi", false).is_some() =>
                {
                    control.page = Page::Hidden;
                }
                "saved" if advertised(client, "network.forget").is_some() => {
                    control.page = Page::Saved;
                }
                _ => {}
            }
        });
    }
    /// Close the prompt or form without sending anything.
    pub fn network_cancel(&self) {
        if let Some(control) = &self.wifi_control {
            control.borrow_mut().reset_flow();
        }
    }
    /// Send the join the open prompt or hidden form describes. The caller clears
    /// [`SECRET_STORE`] afterwards whatever the outcome.
    pub fn network_join(&self, stores: &Stores, wake: CallWake) {
        self.flow(|control, client, boot, state| {
            if control.pending.is_some() || control.link.is_some() {
                return;
            }
            let Some(timeout) = advertised(client, "network.connect") else {
                return;
            };
            let secret = text(stores, SECRET_STORE);
            control.form_error = None;
            let (params, key, reprompt) = match control.page {
                Page::Prompt => {
                    let Some(prompt) = control.prompt.clone() else {
                        return;
                    };
                    let ap = prompt
                        .key
                        .strip_prefix("ap:")
                        .and_then(|id| {
                            let id = EntityId(id.into());
                            state.access_points.items.get(&id).map(|ap| (id, ap))
                        })
                        .filter(|(_, ap)| ap.secured && !ap.active);
                    let Some((id, ap)) = ap else {
                        control.form_error = Some("bead.panel.network-list-changed");
                        return;
                    };
                    if secret.is_empty() {
                        control.form_error = Some("bead.panel.network-secret-required");
                        return;
                    }
                    let connect = NetworkConnect {
                        device: ap.device.clone(),
                        target: NetworkTarget::AccessPoint(id),
                        secret: Some(secret),
                        security: None,
                    };
                    (connect, prompt.key.clone(), Reprompt::Prompt(prompt))
                }
                Page::Hidden => {
                    let ssid = text(stores, "network.hidden_ssid");
                    let security = match text(stores, "network.hidden_security").as_str() {
                        "open" => WifiSecurity::Open,
                        "sae" => WifiSecurity::Sae,
                        _ => WifiSecurity::WpaPsk,
                    };
                    if ssid.is_empty() || ssid.len() > 32 || ssid.contains('\0') {
                        control.form_error = Some("bead.panel.network-ssid-invalid");
                        return;
                    }
                    let open = security == WifiSecurity::Open;
                    if !open && secret.is_empty() {
                        control.form_error = Some("bead.panel.network-secret-required");
                        return;
                    }
                    let Some(device) = sole_device(state, "wifi", false) else {
                        control.form_error = Some("bead.panel.network-no-wifi");
                        return;
                    };
                    let connect = NetworkConnect {
                        device: device.clone(),
                        target: NetworkTarget::HiddenSsid(ssid.clone()),
                        secret: (!open).then_some(secret),
                        security: Some(security),
                    };
                    (connect, format!("hidden:{ssid}"), Reprompt::Hidden)
                }
                _ => return,
            };
            let params = serde_json::to_value(params).unwrap_or_default();
            dispatch(
                control,
                client,
                boot.into(),
                ("network.connect", timeout),
                LinkTarget {
                    method: "network.connect",
                    params,
                },
                key,
                Some(reprompt),
                wake,
            );
        });
    }
    /// Forget a saved profile: the first call (`confirm == false`) arms an inline confirmation
    /// for `CONFIRM_TIMEOUT`; only the second call for the same row sends `network.forget`.
    pub fn network_forget(&self, key: &str, confirm: bool, wake: CallWake) {
        self.flow(|control, client, boot, state| {
            if control.link.is_some() || control.pending.is_some() {
                return;
            }
            let Some(timeout) = advertised(client, "network.forget") else {
                return;
            };
            let Some(target) = resolve_link(state, key).filter(|t| t.method == "network.forget")
            else {
                control.forget = None;
                control.link_message = Some(tr("bead.panel.network-list-changed", &[]));
                return;
            };
            if !confirm {
                control.forget = Some((key.into(), Instant::now() + control.confirm_timeout));
                // Wake the panel at the deadline so the row reverts on its own.
                let wait = control.confirm_timeout + Duration::from_millis(20);
                std::thread::spawn(move || {
                    std::thread::sleep(wait);
                    wake();
                });
                return;
            }
            let armed = control
                .forget
                .take()
                .is_some_and(|(armed, deadline)| armed == key && Instant::now() < deadline);
            if !armed {
                return;
            }
            control.link_message = None;
            dispatch(
                control,
                client,
                boot.into(),
                ("network.forget", timeout),
                target,
                key.into(),
                None,
                wake,
            );
        });
    }
    #[cfg(test)]
    pub(crate) fn set_confirm_timeout_for_test(&self, timeout: Duration) {
        if let Some(control) = &self.wifi_control {
            control.borrow_mut().confirm_timeout = timeout;
        }
    }
}

/// Stores for the join/forget views.
pub(super) fn project(
    control: &mut WifiControl,
    state: Option<&NetworkState>,
    client: &Client,
) -> BTreeMap<String, Value> {
    if control
        .forget
        .as_ref()
        .is_some_and(|(_, deadline)| Instant::now() >= *deadline)
    {
        control.forget = None;
    }
    let busy = control
        .link
        .as_ref()
        .is_some_and(|op| op.reprompt.is_some() && op.ticket.is_some());
    let link_busy = control.link.is_some();
    let can_join = state.is_some_and(|state| {
        advertised(client, "network.connect").is_some()
            && sole_device(state, "wifi", false).is_some()
    });
    let can_forget = state.is_some() && advertised(client, "network.forget").is_some();
    let mut saved: Vec<_> = state
        .map(|state| {
            state
                .connections
                .order
                .iter()
                .filter_map(|id| state.connections.items.get(id).map(|c| (id, c)))
                .filter(|(_, c)| c.ssid.is_some())
                .collect()
        })
        .unwrap_or_default();
    saved.sort_by_key(|(_, c)| std::cmp::Reverse(c.last_used.unwrap_or(0)));
    saved.truncate(MAX_SAVED_ROWS);
    let rows: Vec<_> = saved
        .into_iter()
        .map(|(id, c)| {
            let key = format!("forget:{}", id.0);
            let name = c
                .ssid
                .as_deref()
                .filter(|s| !s.is_empty())
                .unwrap_or(&c.name);
            serde_json::json!({
                "id": key,
                "label": line(name, 28),
                "caption": tr(
                    if c.security.as_deref().is_some_and(|s| s != "open") {
                        "bead.panel.network-saved-secured"
                    } else {
                        "bead.panel.network-saved-open"
                    },
                    &[],
                ),
                "confirming": control.forget.as_ref().is_some_and(|(armed, _)| *armed == key),
                "enabled": can_forget && !link_busy,
            })
        })
        .collect();
    BTreeMap::from([
        (
            "network.view".into(),
            Value::Text(control.page.as_str().into()),
        ),
        (
            "network.prompt_label".into(),
            Value::Text(
                control
                    .prompt
                    .as_ref()
                    .map_or(String::new(), |p| p.label.clone()),
            ),
        ),
        (
            "network.form_error".into(),
            Value::Text(control.form_error.map_or(String::new(), |id| tr(id, &[]))),
        ),
        ("network.form_busy".into(), Value::Bool(busy)),
        (
            "network.can_hidden".into(),
            Value::Bool(can_join && !link_busy),
        ),
        ("network.can_forget".into(), Value::Bool(can_forget)),
        ("network.saved".into(), Value::list(rows)),
    ])
}
