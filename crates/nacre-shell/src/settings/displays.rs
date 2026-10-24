// SPDX-License-Identifier: GPL-3.0-only
//! B0089: the Displays page over the Nacre N0063 contract (`displays` topic, `display.apply`,
//! `display.confirm`, `display.revert`). Same shape as QML's `NacreDisplaysPage`: a "Displays"
//! card (primary chips, Extend right/left, Mirror), one card per output (Scale, Resolution and
//! refresh rate) and, while a change is pending, "Keep this display change?" with Keep and
//! Revert. The one addition is the countdown the contract's `pending.deadline_ms` allows.
//!
//! Nothing here positions anything: it emits generic rows (`actions`, `picker`, `keep`) that
//! `shell/settings/main.luau` maps to kit components. Output names, scales and modes in a click
//! are re-resolved against the published topic and never trusted.
use super::pages::{actions, healthy, help, opt, short};
use super::*;
use nacre_protocol::displays::{DisplayOutput, DisplaysState};

const MODE_CHIPS: usize = 8;
const MODE_ROWS: usize = 8;

/// Stops the one-second countdown wake when dropped.
pub(super) struct Ticker {
    _stop: std::sync::mpsc::Sender<()>,
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}
fn percent(scale: f64) -> String {
    format!("{}%", (scale * 100.).round() as u32)
}
/// A name that survives the `do:op|name|value` argument split.
fn splittable(name: &str) -> bool {
    !name.contains('|')
}

impl Settings {
    /// The `displays` topic as the contract's type, when it is live, healthy and valid.
    fn displays(&self) -> Option<DisplaysState> {
        let topic = self.topic("displays").filter(healthy)?;
        let state: DisplaysState = serde_json::from_value(topic).ok()?;
        state.validate().ok()?;
        Some(state)
    }
    /// Changes are refused while one is pending, another call is in flight, or the compositor
    /// cannot configure outputs (QML `blocked`).
    fn display_blocked(&self, display: &DisplaysState) -> bool {
        !display.can_configure
            || display.pending.is_some()
            || self.state.borrow().pending.is_some()
            || !self.can("display.apply")
    }
    /// The chosen primary output, else the first (QML `primary`).
    fn display_primary<'a>(&self, display: &'a DisplaysState) -> Option<&'a DisplayOutput> {
        let chosen = self.state.borrow().display_primary.clone();
        display
            .outputs
            .iter()
            .find(|o| o.name == chosen)
            .or_else(|| display.outputs.first())
    }

    /// The cards of the Displays page: one per section the QML page drew.
    pub(super) fn display_sections(&self, state: &State) -> Vec<Json> {
        let title = tr("bead.settings.displays-title", &[]);
        let Some(display) = self.displays() else {
            return vec![card(
                "displays",
                &title,
                "",
                vec![help(
                    "displays.unavailable",
                    &tr("bead.settings.unavailable-now", &[]),
                )],
            )];
        };
        let blocked = self.display_blocked(&display);
        let primary = self.display_primary(&display).map(|o| o.name.clone());
        let description = if !display.can_configure {
            "bead.settings.displays-unavailable"
        } else if !display.can_mirror {
            "bead.settings.displays-no-mirror"
        } else {
            "bead.settings.displays-description"
        };
        let mut rows = Vec::new();
        if display.outputs.len() > 1 {
            rows.push(actions(
                "displays.primary",
                "",
                "",
                display
                    .outputs
                    .iter()
                    .map(|o| {
                        opt(
                            format!("do:disp-primary|{}", o.name),
                            &short(&o.name, 40),
                            primary.as_deref() == Some(o.name.as_str()),
                            splittable(&o.name),
                        )
                    })
                    .collect(),
                true,
            ));
            let can = !blocked && primary.as_deref().is_some_and(splittable);
            let mut arrange = vec![
                ("extend-right", "bead.settings.displays-extend-right"),
                ("extend-left", "bead.settings.displays-extend-left"),
            ];
            if display.can_mirror {
                arrange.push(("mirror", "bead.settings.displays-mirror"));
            }
            rows.push(actions(
                "displays.arrange",
                "",
                "",
                arrange
                    .into_iter()
                    .map(|(how, label)| {
                        opt(format!("do:disp-arrange|{how}"), &tr(label, &[]), false, can)
                    })
                    .collect(),
                can,
            ));
        }
        let mut cards = vec![card("displays", &title, &tr(description, &[]), rows)];
        for output in &display.outputs {
            cards.push(self.output_card(&display, output, blocked, state));
        }
        if let Some(pending) = display.pending {
            let seconds = pending.deadline_ms.saturating_sub(now_ms()).div_ceil(1000);
            cards.push(card(
                "displays.keep",
                &tr("bead.settings.displays-keep-title", &[]),
                "",
                vec![json!({"id": "displays.keep.bar", "kind": "keep", "enabled": true, "own": true,
                    "text": tr("bead.settings.displays-countdown", &[("seconds", &seconds.to_string())]),
                    "keep_label": tr("bead.settings.displays-keep", &[]),
                    "revert_label": tr("bead.settings.displays-revert", &[])})],
            ));
        }
        cards
    }

    /// "DP-1 · 2560×1440 · 60 Hz": Scale and Resolution and refresh rate.
    fn output_card(
        &self,
        display: &DisplaysState,
        output: &DisplayOutput,
        blocked: bool,
        state: &State,
    ) -> Json {
        let name = short(&output.name, 40);
        let id = format!("displays.out.{}", output.name);
        let can = !blocked && splittable(&output.name);
        let title = tr(
            "bead.settings.displays-monitor",
            &[
                ("name", &name),
                ("width", &output.width.to_string()),
                ("height", &output.height.to_string()),
                ("rate", &(output.refresh_hz.round() as u64).to_string()),
            ],
        );
        let note = tr(
            "bead.settings.displays-revert-note",
            &[("seconds", &nacre_protocol::displays::CONFIRM_SECONDS.to_string())],
        );
        let mut rows = vec![actions(
            &format!("{id}.scale"),
            &tr("bead.settings.displays-scale", &[]),
            "",
            display
                .scales
                .iter()
                .map(|scale| {
                    opt(
                        format!("do:disp-scale|{}|{scale}", output.name),
                        &percent(*scale),
                        (*scale - output.scale).abs() < 1e-9,
                        can,
                    )
                })
                .collect(),
            can,
        )];
        if !output.modes.is_empty() {
            let label = tr("bead.settings.displays-mode", &[]);
            let option = |index: usize, mode: &String| {
                (
                    format!("do:disp-mode|{}|{index}", output.name),
                    mode.clone(),
                    output.mode.as_ref() == Some(mode),
                )
            };
            if output.modes.len() <= MODE_CHIPS {
                rows.push(actions(
                    &format!("{id}.mode"),
                    &label,
                    "",
                    output
                        .modes
                        .iter()
                        .enumerate()
                        .map(|(i, m)| {
                            let (arg, text, selected) = option(i, m);
                            opt(arg, &text, selected, can)
                        })
                        .collect(),
                    can,
                ));
            } else {
                // QML searches above 8 entries: the same filter field the other pickers use.
                let tokens: Vec<String> = state
                    .pick
                    .split_whitespace()
                    .map(str::to_lowercase)
                    .collect();
                let matching: Vec<(usize, &String)> = output
                    .modes
                    .iter()
                    .enumerate()
                    .filter(|(_, m)| {
                        let hay = m.to_lowercase();
                        tokens.iter().all(|t| hay.contains(t))
                    })
                    .collect();
                let shown: Vec<Json> = matching
                    .iter()
                    .take(MODE_ROWS)
                    .map(|(i, m)| {
                        let (arg, text, selected) = option(*i, m);
                        json!({"id": arg, "label": text, "selected": selected})
                    })
                    .collect();
                let more = matching.len().saturating_sub(MODE_ROWS);
                rows.push(json!({"id": format!("{id}.mode"), "kind": "picker", "enabled": can, "own": true,
                    "label": label, "placeholder": tr("bead.settings.displays-mode-filter", &[]),
                    "items": shown, "empty": tr("bead.settings.displays-mode-none", &[]),
                    "more": if more > 0 { tr("bead.settings.displays-mode-more", &[("count", &more.to_string())]) } else { String::new() }}));
            }
        }
        card(&id, &title, &note, rows)
    }

    /// The buttons of the page: `do:disp-<op>|<a>|<b>`. Every name, scale and mode is looked up
    /// in the published topic; an unknown one sends nothing.
    pub(super) fn display_op(&self, op: &str, a: &str, b: &str, wake: CallWake) {
        let Some(display) = self.displays() else {
            return;
        };
        let output = |name: &str| display.outputs.iter().find(|o| o.name == name);
        match op {
            "disp-primary" if output(a).is_some() => {
                self.state.borrow_mut().display_primary = a.into();
            }
            "disp-arrange" if !self.display_blocked(&display) && display.outputs.len() > 1 => {
                if !(matches!(a, "extend-right" | "extend-left")
                    || a == "mirror" && display.can_mirror)
                {
                    return;
                }
                let Some(primary) = self.display_primary(&display) else {
                    return;
                };
                self.call(
                    "display.apply",
                    json!({"output": primary.name, "arrangement": a}),
                    wake,
                    "",
                );
            }
            "disp-scale" if !self.display_blocked(&display) => {
                let scale = b.parse::<f64>().ok().filter(|s| display.scales.contains(s));
                if let (Some(target), Some(scale)) = (output(a), scale) {
                    self.call(
                        "display.apply",
                        json!({"output": target.name, "scale": scale}),
                        wake,
                        "",
                    );
                }
            }
            "disp-mode" if !self.display_blocked(&display) => {
                let mode = output(a).and_then(|o| Some((o, o.modes.get(b.parse::<usize>().ok()?)?)));
                if let Some((target, mode)) = mode {
                    self.call(
                        "display.apply",
                        json!({"output": target.name, "scale": target.scale, "mode": mode}),
                        wake,
                        "",
                    );
                }
            }
            "disp-keep" if display.pending.is_some() => {
                self.call("display.confirm", json!({}), wake, "");
            }
            "disp-revert" if display.pending.is_some() => {
                self.call("display.revert", json!({}), wake, "");
            }
            _ => {}
        }
    }

    /// One wake per second while the Keep card is on screen; none otherwise.
    pub(super) fn display_tick(&self, page: &str) {
        let live = page == "displays"
            && self
                .displays()
                .and_then(|d| d.pending)
                .is_some_and(|p| p.deadline_ms + 2000 > now_ms());
        let mut state = self.state.borrow_mut();
        if !live {
            state.display_ticker = None;
            return;
        }
        let Some(wake) = self.wake.borrow().clone().filter(|_| state.display_ticker.is_none())
        else {
            return;
        };
        let (stop, wait) = std::sync::mpsc::channel::<()>();
        let spawned = std::thread::Builder::new()
            .name("display-tick".into())
            .stack_size(128 * 1024)
            .spawn(move || {
                while let Err(std::sync::mpsc::RecvTimeoutError::Timeout) =
                    wait.recv_timeout(Duration::from_secs(1))
                {
                    wake();
                }
            });
        if spawned.is_ok() {
            state.display_ticker = Some(Ticker { _stop: stop });
        }
    }
}

/// One raised card: what `main.luau` draws for a `section` row.
fn card(id: &str, label: &str, description: &str, rows: Vec<Json>) -> Json {
    json!({"id": format!("section:{id}"), "kind": "section", "label": label,
        "description": description, "rows": rows})
}
