// SPDX-License-Identifier: MIT OR Apache-2.0
//! Pause/release policy for a motion wallpaper, one machine per output. Pure and
//! clock-injected: the caller supplies inputs and a monotonic millisecond clock.
//!
//! Two kinds of pause exist. A *hold* keeps the picture visible but frozen (the user paused,
//! on battery, windows cover the output); a *hide* means nobody can see the output (fullscreen
//! cover, locked or sleeping session, output off). Both keep the decoder at first; each
//! releases it after its own optional delay, and the renderer keeps showing the last frame.
use crate::protocol::Decoder;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Config {
    /// Treat `Inputs::on_battery` as a hold reason.
    pub pause_on_battery: bool,
    /// Treat `Inputs::covered` as a hold reason.
    pub pause_when_covered: bool,
    /// Release the decoder after this long hidden. `None` never releases.
    pub release_hidden_ms: Option<u64>,
    /// Release the decoder after this long held. `None` never releases.
    pub release_held_ms: Option<u64>,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            pause_on_battery: false,
            pause_when_covered: true,
            release_hidden_ms: Some(5_000),
            release_held_ms: Some(30_000),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Inputs {
    /// Windows cover (part of) the output; a hold when `pause_when_covered`.
    pub covered: bool,
    /// Nothing is visible: fullscreen cover, locked or sleeping session, or the output is off.
    pub hidden: bool,
    pub on_battery: bool,
    /// The user (or a setting) froze the wallpaper.
    pub user_paused: bool,
    /// Motion is switched off: show the still image and keep no decoder.
    pub stopped: bool,
    /// Ignore `covered` and `hidden` for this output (for example while a picker previews it).
    pub force_visible: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Reason {
    Hold,
    Hide,
}

#[derive(Clone, Debug, Default)]
pub struct Machine {
    since: Option<u64>,
    limit: Option<u64>,
    last: Option<Decoder>,
}
impl Machine {
    pub fn step(&mut self, config: &Config, inputs: Inputs, now_ms: u64) -> Decoder {
        if inputs.stopped {
            return self.settle(Decoder::Release, None, None);
        }
        let hidden = inputs.hidden && !inputs.force_visible;
        let held = inputs.user_paused
            || (config.pause_on_battery && inputs.on_battery)
            || (config.pause_when_covered && inputs.covered && !inputs.force_visible);
        let reason = if hidden {
            Some(Reason::Hide)
        } else if held {
            Some(Reason::Hold)
        } else {
            None
        };
        let Some(reason) = reason else {
            return self.settle(Decoder::Play, None, None);
        };
        let limit = match reason {
            Reason::Hide => config.release_hidden_ms,
            Reason::Hold => config.release_held_ms,
        };
        let since = *self.since.get_or_insert(now_ms);
        let state = match limit {
            Some(limit) if now_ms.saturating_sub(since) >= limit => Decoder::Release,
            _ => Decoder::Pause,
        };
        self.last = Some(state);
        self.limit = limit;
        state
    }
    fn settle(&mut self, state: Decoder, since: Option<u64>, limit: Option<u64>) -> Decoder {
        self.since = since;
        self.limit = limit;
        self.last = Some(state);
        state
    }
    /// Clock value at which `step` with unchanged inputs would turn `Pause` into `Release`.
    pub fn deadline(&self) -> Option<u64> {
        match (self.last, self.since, self.limit) {
            (Some(Decoder::Pause), Some(since), Some(limit)) => Some(since + limit),
            _ => None,
        }
    }
}

/// One decoder serves every output: it plays while any output plays, holds while any is
/// merely paused, and is released only when every output allows it.
pub fn aggregate(states: impl IntoIterator<Item = Decoder>) -> Decoder {
    let mut result = Decoder::Release;
    for state in states {
        match state {
            Decoder::Play => return Decoder::Play,
            Decoder::Pause => result = Decoder::Pause,
            Decoder::Release => {}
        }
    }
    result
}

/// All outputs of one wallpaper: per-output machines plus the aggregate decision.
#[derive(Debug, Default)]
pub struct Group {
    outputs: BTreeMap<String, Machine>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Decision {
    pub decoder: Decoder,
    /// Outputs that should receive new frames (those that are playing).
    pub render: BTreeSet<String>,
    /// Next clock value at which the decision may change by itself.
    pub deadline: Option<u64>,
}
impl Group {
    /// `inputs` lists the currently connected outputs; others are forgotten.
    pub fn step(
        &mut self,
        config: &Config,
        inputs: &BTreeMap<String, Inputs>,
        now_ms: u64,
    ) -> Decision {
        self.outputs.retain(|name, _| inputs.contains_key(name));
        let mut render = BTreeSet::new();
        let mut states = Vec::new();
        for (name, input) in inputs {
            let machine = self.outputs.entry(name.clone()).or_default();
            let state = machine.step(config, *input, now_ms);
            if state == Decoder::Play {
                render.insert(name.clone());
            }
            states.push(state);
        }
        Decision {
            decoder: aggregate(states),
            render,
            deadline: self.outputs.values().filter_map(Machine::deadline).min(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn cfg() -> Config {
        Config {
            pause_on_battery: true,
            ..Config::default()
        }
    }
    #[test]
    fn plays_until_a_reason_appears_and_resumes_when_it_clears() {
        let mut m = Machine::default();
        let c = cfg();
        assert_eq!(m.step(&c, Inputs::default(), 0), Decoder::Play);
        let covered = Inputs {
            covered: true,
            ..Default::default()
        };
        assert_eq!(m.step(&c, covered, 10), Decoder::Pause);
        assert_eq!(m.step(&c, Inputs::default(), 20), Decoder::Play);
        assert_eq!(m.deadline(), None);
    }
    #[test]
    fn hide_releases_after_its_delay_and_hold_after_its_longer_delay() {
        let c = cfg();
        let mut hidden = Machine::default();
        let h = Inputs {
            hidden: true,
            ..Default::default()
        };
        assert_eq!(hidden.step(&c, h, 1_000), Decoder::Pause);
        assert_eq!(hidden.deadline(), Some(6_000));
        assert_eq!(hidden.step(&c, h, 5_999), Decoder::Pause);
        assert_eq!(hidden.step(&c, h, 6_000), Decoder::Release);
        assert_eq!(hidden.deadline(), None);
        let mut held = Machine::default();
        let b = Inputs {
            on_battery: true,
            ..Default::default()
        };
        assert_eq!(held.step(&c, b, 0), Decoder::Pause);
        assert_eq!(held.deadline(), Some(30_000));
        assert_eq!(held.step(&c, b, 29_999), Decoder::Pause);
        assert_eq!(held.step(&c, b, 30_000), Decoder::Release);
        assert_eq!(held.step(&c, Inputs::default(), 31_000), Decoder::Play);
    }
    #[test]
    fn escalating_from_hold_to_hide_keeps_the_original_clock() {
        let c = cfg();
        let mut m = Machine::default();
        m.step(
            &c,
            Inputs {
                covered: true,
                ..Default::default()
            },
            0,
        );
        let both = Inputs {
            covered: true,
            hidden: true,
            ..Default::default()
        };
        assert_eq!(m.step(&c, both, 4_000), Decoder::Pause);
        assert_eq!(m.step(&c, both, 5_000), Decoder::Release);
    }
    #[test]
    fn battery_and_cover_rules_are_configuration_not_policy() {
        let mut m = Machine::default();
        let off = Config {
            pause_on_battery: false,
            pause_when_covered: false,
            ..Config::default()
        };
        let busy = Inputs {
            on_battery: true,
            covered: true,
            ..Default::default()
        };
        assert_eq!(m.step(&off, busy, 0), Decoder::Play);
        let never = Config {
            release_held_ms: None,
            release_hidden_ms: None,
            ..cfg()
        };
        let user = Inputs {
            user_paused: true,
            ..Default::default()
        };
        assert_eq!(m.step(&never, user, 0), Decoder::Pause);
        assert_eq!(m.step(&never, user, u64::MAX / 2), Decoder::Pause);
        assert_eq!(m.deadline(), None);
    }
    #[test]
    fn stopped_releases_at_once_and_force_visible_overrides_cover_and_hide() {
        let c = cfg();
        let mut m = Machine::default();
        let stopped = Inputs {
            stopped: true,
            ..Default::default()
        };
        assert_eq!(m.step(&c, stopped, 0), Decoder::Release);
        let forced = Inputs {
            covered: true,
            hidden: true,
            force_visible: true,
            ..Default::default()
        };
        assert_eq!(m.step(&c, forced, 1), Decoder::Play);
        let forced_user = Inputs {
            user_paused: true,
            force_visible: true,
            ..Default::default()
        };
        assert_eq!(m.step(&c, forced_user, 2), Decoder::Pause);
    }
    #[test]
    fn group_plays_for_any_playing_output_and_forgets_removed_ones() {
        let c = cfg();
        let mut g = Group::default();
        let mut inputs = BTreeMap::from([
            (
                "A".to_string(),
                Inputs {
                    covered: true,
                    ..Default::default()
                },
            ),
            ("B".to_string(), Inputs::default()),
        ]);
        let d = g.step(&c, &inputs, 0);
        assert_eq!(d.decoder, Decoder::Play);
        assert_eq!(d.render, BTreeSet::from(["B".to_string()]));
        inputs.insert(
            "B".into(),
            Inputs {
                hidden: true,
                ..Default::default()
            },
        );
        let d = g.step(&c, &inputs, 100);
        assert_eq!((d.decoder, d.render.len()), (Decoder::Pause, 0));
        assert_eq!(d.deadline, Some(5_100));
        inputs.remove("B");
        let d = g.step(&c, &inputs, 200);
        assert_eq!(d.deadline, Some(30_000));
        inputs.clear();
        let d = g.step(&c, &inputs, 300);
        assert_eq!((d.decoder, d.deadline), (Decoder::Release, None));
    }
    #[test]
    fn aggregate_prefers_play_then_pause_then_release() {
        use Decoder::*;
        assert_eq!(aggregate([Release, Pause, Play]), Play);
        assert_eq!(aggregate([Release, Pause]), Pause);
        assert_eq!(aggregate([Release]), Release);
        assert_eq!(aggregate([]), Release);
    }
}
