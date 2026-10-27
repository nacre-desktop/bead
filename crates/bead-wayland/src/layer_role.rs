//! The one owner of a shell surface's wlr layer-surface role state.
//!
//! Layer-shell state (anchor, size, margin, exclusive zone, keyboard
//! interactivity) is double-buffered: every `wl_surface.commit` applies the
//! whole pending set, and the compositor validates it. Two rules are fatal:
//!
//! * a zero size on an axis that is not anchored to both of its edges;
//! * a buffer attached before a configure has been acked. A null attach
//!   (unmap) returns the role to its initial state: the compositor drops the
//!   pending state to defaults (no anchor, zero size) and forgets the acked
//!   configure, so the client must resend its state, commit without a buffer
//!   and wait for a fresh configure.
//!
//! `LayerRole` keeps the compositor's pending state equal to a validated copy
//! of what callers asked for at all times, so any commit (including a raw
//! buffer commit through `wl_surface`) carries a valid state, and it tracks the
//! configure phase so buffer paths can ask [`LayerRole::can_attach`].
use smithay_client_toolkit::shell::{
    WaylandSurface,
    wlr_layer::{Anchor, KeyboardInteractivity, LayerSurface},
};
use std::cell::Cell;
use wayland_client::protocol::wl_surface::WlSurface;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct LayerState {
    pub anchor: Anchor,
    pub size: (u32, u32),
    /// top, right, bottom, left
    pub margin: [i32; 4],
    pub exclusive_zone: i32,
    pub keyboard: KeyboardInteractivity,
}

impl Default for LayerState {
    fn default() -> Self {
        Self {
            anchor: Anchor::empty(),
            size: (0, 0),
            margin: [0; 4],
            exclusive_zone: 0,
            keyboard: KeyboardInteractivity::None,
        }
    }
}

impl LayerState {
    /// The state the compositor may see: zero means "span the anchored axis",
    /// which is only valid when both edges of that axis are anchored.
    pub(crate) fn valid(mut self) -> Self {
        if self.size.0 == 0 && !self.anchor.contains(Anchor::LEFT | Anchor::RIGHT) {
            self.size.0 = 1;
        }
        if self.size.1 == 0 && !self.anchor.contains(Anchor::TOP | Anchor::BOTTOM) {
            self.size.1 = 1;
        }
        self
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Phase {
    /// Created or unmapped: the next commit must carry no buffer.
    Initial,
    /// The initial commit was sent; no configure has been acked since.
    AwaitingConfigure,
    /// A configure was acked; buffers may be attached.
    Configured,
}

impl Phase {
    fn after_commit(self) -> Self {
        if self == Self::Initial {
            Self::AwaitingConfigure
        } else {
            self
        }
    }
    fn after_configure(self) -> Self {
        if self == Self::AwaitingConfigure {
            Self::Configured
        } else {
            self
        }
    }
}

pub(crate) struct LayerRole {
    layer: LayerSurface,
    desired: Cell<LayerState>,
    sent: Cell<Option<LayerState>>,
    phase: Cell<Phase>,
}

impl PartialEq<LayerSurface> for LayerRole {
    fn eq(&self, other: &LayerSurface) -> bool {
        self.layer == *other
    }
}
impl PartialEq for LayerRole {
    fn eq(&self, other: &Self) -> bool {
        self.layer == other.layer
    }
}

impl LayerRole {
    /// Takes a freshly created role; nothing is committed until [`Self::commit`].
    pub(crate) fn new(layer: LayerSurface, state: LayerState) -> Self {
        let role = Self {
            layer,
            desired: Cell::new(state),
            sent: Cell::new(None),
            phase: Cell::new(Phase::Initial),
        };
        role.send();
        role
    }

    pub(crate) fn wl_surface(&self) -> &WlSurface {
        self.layer.wl_surface()
    }
    pub(crate) fn phase(&self) -> Phase {
        self.phase.get()
    }
    pub(crate) fn can_attach(&self) -> bool {
        self.phase.get() == Phase::Configured
    }
    pub(crate) fn state(&self) -> LayerState {
        self.desired.get()
    }

    /// Requests a new state; returns whether anything was sent.
    pub(crate) fn set(&self, state: LayerState) -> bool {
        self.desired.set(state);
        self.send()
    }
    pub(crate) fn set_size(&self, width: u32, height: u32) -> bool {
        self.set(LayerState {
            size: (width, height),
            ..self.desired.get()
        })
    }
    pub(crate) fn set_placement(&self, anchor: Anchor, margin: [i32; 4]) -> bool {
        self.set(LayerState {
            anchor,
            margin,
            ..self.desired.get()
        })
    }
    pub(crate) fn set_keyboard_interactivity(&self, keyboard: KeyboardInteractivity) -> bool {
        self.set(LayerState {
            keyboard,
            ..self.desired.get()
        })
    }

    /// Commits the surface. From the initial phase this is the bufferless
    /// commit that asks the compositor for a configure.
    pub(crate) fn commit(&self) {
        self.layer.commit();
        self.phase.set(self.phase.get().after_commit());
    }

    /// Called for every configure; SCTK has already acked it.
    pub(crate) fn configured(&self) {
        self.phase.set(self.phase.get().after_configure());
    }

    /// Unmaps with a null buffer and restores the full pending state the
    /// compositor just discarded. The next commit asks for a new configure.
    pub(crate) fn unmap(&self) {
        self.layer.wl_surface().attach(None, 0, 0);
        self.layer.commit();
        self.phase.set(Phase::Initial);
        self.sent.set(None);
        self.send();
    }

    fn send(&self) -> bool {
        let next = self.desired.get().valid();
        let sent = self.sent.replace(Some(next));
        let all = sent.is_none();
        let sent = sent.unwrap_or_default();
        // The anchor goes first so the size check never sees a mixed state
        // (the compositor validates only at commit, but keep the order obvious).
        if all || sent.anchor != next.anchor {
            self.layer.set_anchor(next.anchor);
        }
        if all || sent.size != next.size {
            self.layer.set_size(next.size.0, next.size.1);
        }
        if all || sent.margin != next.margin {
            let [top, right, bottom, left] = next.margin;
            self.layer.set_margin(top, right, bottom, left);
        }
        if all || sent.exclusive_zone != next.exclusive_zone {
            self.layer.set_exclusive_zone(next.exclusive_zone);
        }
        if all || sent.keyboard != next.keyboard {
            self.layer.set_keyboard_interactivity(next.keyboard);
        }
        all || sent != next
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_size_needs_both_edges_of_its_axis() {
        let full = LayerState {
            anchor: Anchor::all(),
            ..LayerState::default()
        };
        assert_eq!(full.valid().size, (0, 0));
        let top = LayerState {
            anchor: Anchor::TOP,
            ..LayerState::default()
        };
        assert_eq!(top.valid().size, (1, 1));
        let bar = LayerState {
            anchor: Anchor::TOP | Anchor::LEFT | Anchor::RIGHT,
            size: (0, 32),
            ..LayerState::default()
        };
        assert_eq!(bar.valid().size, (0, 32));
        let side = LayerState {
            anchor: Anchor::LEFT | Anchor::TOP | Anchor::BOTTOM,
            size: (0, 0),
            ..LayerState::default()
        };
        assert_eq!(side.valid().size, (1, 0));
        let sized = LayerState {
            anchor: Anchor::BOTTOM,
            size: (640, 480),
            ..LayerState::default()
        };
        assert_eq!(sized.valid(), sized);
    }

    #[test]
    fn buffers_wait_for_a_configure_after_every_initial_commit() {
        // Created or unmapped: a configure that races the unmap must not count.
        assert_eq!(Phase::Initial.after_configure(), Phase::Initial);
        let waiting = Phase::Initial.after_commit();
        assert_eq!(waiting, Phase::AwaitingConfigure);
        assert_eq!(waiting.after_commit(), Phase::AwaitingConfigure);
        assert_eq!(waiting.after_configure(), Phase::Configured);
        assert_eq!(Phase::Configured.after_commit(), Phase::Configured);
    }

    #[test]
    fn every_anchor_combination_is_valid_after_normalising() {
        for bits in 0..16u32 {
            let anchor = Anchor::from_bits_truncate(bits);
            for size in [(0, 0), (0, 5), (5, 0), (5, 5)] {
                let state = LayerState {
                    anchor,
                    size,
                    ..LayerState::default()
                }
                .valid();
                assert!(
                    state.size.0 > 0 || anchor.contains(Anchor::LEFT | Anchor::RIGHT),
                    "{anchor:?} {size:?}"
                );
                assert!(
                    state.size.1 > 0 || anchor.contains(Anchor::TOP | Anchor::BOTTOM),
                    "{anchor:?} {size:?}"
                );
                // Normalising never changes a size the caller set explicitly.
                assert!(size.0 == 0 || state.size.0 == size.0);
                assert!(size.1 == 0 || state.size.1 == size.1);
            }
        }
    }
}
