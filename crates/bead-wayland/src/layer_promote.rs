// SPDX-License-Identifier: MIT
//! Optional compositor ownership of a layer surface's floating placement.

use crate::{
    App,
    layer_promote_protocol::{
        bead_layer_promote_manager_v1::BeadLayerPromoteManagerV1,
        bead_layer_promote_v1::{self, BeadLayerPromoteV1},
    },
};
use wayland_client::WEnum;
use wayland_client::protocol::{wl_seat, wl_surface};
use wayland_client::{Connection, Dispatch, QueueHandle, delegate_noop, globals::GlobalList};

delegate_noop!(App: ignore BeadLayerPromoteManagerV1);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Phase {
    Docked,
    PromotePending {
        serial: u32,
    },
    PromotePendingDock {
        promote_serial: u32,
        dock_serial: u32,
    },
    Promoted {
        serial: u32,
    },
    DockPending {
        serial: u32,
        promotion_serial: u32,
    },
    CancelPending {
        serial: u32,
        promotion_serial: u32,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PromotionState(Phase);

impl Default for PromotionState {
    fn default() -> Self {
        Self(Phase::Docked)
    }
}

impl PromotionState {
    #[cfg(test)]
    pub(crate) fn phase(self) -> Phase {
        self.0
    }
    pub(crate) fn is_promoted(self) -> bool {
        matches!(
            self.0,
            Phase::Promoted { .. } | Phase::DockPending { .. } | Phase::CancelPending { .. }
        )
    }
    pub(crate) fn renders_natural_extent(self) -> bool {
        matches!(self.0, Phase::Promoted { .. })
    }
    pub(crate) fn stage_promote(&mut self, serial: u32) {
        self.0 = Phase::PromotePending { serial };
    }
    pub(crate) fn stage_dock(&mut self, serial: u32) -> DockStage {
        match self.0 {
            Phase::PromotePending {
                serial: promote_serial,
            } => {
                self.0 = Phase::PromotePendingDock {
                    promote_serial,
                    dock_serial: serial,
                };
                DockStage::AfterPromotionAck
            }
            Phase::Promoted {
                serial: promotion_serial,
            } => {
                self.0 = Phase::DockPending {
                    serial,
                    promotion_serial,
                };
                DockStage::SendNow
            }
            _ => DockStage::Rejected,
        }
    }
    pub(crate) fn stage_cancel(&mut self, serial: u32) {
        if let Phase::Promoted {
            serial: promotion_serial,
        } = self.0
        {
            self.0 = Phase::CancelPending {
                serial,
                promotion_serial,
            };
        }
    }
    pub(crate) fn invalidate(&mut self) {
        self.0 = Phase::Docked;
    }
    pub(crate) fn matches(&self, serial: u32) -> bool {
        match self.0 {
            Phase::Docked => false,
            Phase::PromotePending { serial: current } | Phase::Promoted { serial: current } => {
                current == serial
            }
            Phase::PromotePendingDock { promote_serial, .. } => promote_serial == serial,
            Phase::DockPending {
                serial: current, ..
            }
            | Phase::CancelPending {
                serial: current, ..
            } => current == serial,
        }
    }
    pub(crate) fn configure(
        &mut self,
        serial: u32,
        mode: bead_layer_promote_v1::State,
    ) -> ConfigureAction {
        if let Phase::DockPending {
            promotion_serial, ..
        }
        | Phase::CancelPending {
            promotion_serial, ..
        } = self.0
            && serial == promotion_serial
            && mode == bead_layer_promote_v1::State::Moving
        {
            return ConfigureAction::Updated;
        }
        if !self.matches(serial) {
            return ConfigureAction::Ignore;
        }
        match (self.0, mode) {
            (
                Phase::PromotePending { .. },
                bead_layer_promote_v1::State::Promoted | bead_layer_promote_v1::State::Moving,
            ) => {
                self.0 = Phase::Promoted { serial };
                ConfigureAction::Updated
            }
            (
                Phase::PromotePendingDock { dock_serial, .. },
                bead_layer_promote_v1::State::Promoted | bead_layer_promote_v1::State::Moving,
            ) => {
                self.0 = Phase::DockPending {
                    serial: dock_serial,
                    promotion_serial: serial,
                };
                ConfigureAction::SendDock(dock_serial)
            }
            (
                Phase::DockPending { .. } | Phase::CancelPending { .. },
                bead_layer_promote_v1::State::Docked,
            ) => {
                self.0 = Phase::Docked;
                ConfigureAction::Docked
            }
            (Phase::Promoted { .. }, bead_layer_promote_v1::State::Docked) => {
                self.0 = Phase::Docked;
                ConfigureAction::Docked
            }
            (Phase::Promoted { .. }, _) => ConfigureAction::Updated,
            _ => ConfigureAction::Updated,
        }
    }
    pub(crate) fn rejected(&mut self, serial: u32) -> RejectAction {
        if !self.matches(serial) {
            return RejectAction::Ignore;
        }
        match self.0 {
            Phase::PromotePending { .. } | Phase::PromotePendingDock { .. } => {
                self.0 = Phase::Docked;
                RejectAction::RollbackPromotion
            }
            Phase::DockPending {
                promotion_serial, ..
            } => {
                self.0 = Phase::Promoted {
                    serial: promotion_serial,
                };
                RejectAction::RejectedDock
            }
            Phase::CancelPending {
                promotion_serial, ..
            } => {
                self.0 = Phase::Promoted {
                    serial: promotion_serial,
                };
                RejectAction::KeepPromotion
            }
            Phase::Promoted { .. } => RejectAction::KeepPromotion,
            Phase::Docked => RejectAction::Ignore,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DockStage {
    SendNow,
    AfterPromotionAck,
    Rejected,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ConfigureAction {
    Ignore,
    Updated,
    SendDock(u32),
    Docked,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RejectAction {
    Ignore,
    RollbackPromotion,
    KeepPromotion,
    RejectedDock,
}

fn configure_requires_redraw(was_promoted: bool, mode: bead_layer_promote_v1::State) -> bool {
    mode == bead_layer_promote_v1::State::Docked || !was_promoted
}

pub(crate) struct LayerPromotionManager(BeadLayerPromoteManagerV1);

impl LayerPromotionManager {
    pub(crate) fn bind(globals: &GlobalList, qh: &QueueHandle<App>) -> Option<Self> {
        globals.bind(qh, 1..=1, ()).ok().map(Self)
    }

    pub(crate) fn create(
        &self,
        surface: &wl_surface::WlSurface,
        qh: &QueueHandle<App>,
    ) -> LayerPromotionController {
        LayerPromotionController {
            proxy: self.0.get_promotion(surface, qh, surface.clone()),
        }
    }
}

impl Drop for LayerPromotionManager {
    fn drop(&mut self) {
        self.0.destroy();
    }
}

pub(crate) struct LayerPromotionController {
    proxy: BeadLayerPromoteV1,
}

impl LayerPromotionController {
    pub(crate) fn promote(
        &self,
        serial: u32,
        seat: Option<&wl_seat::WlSeat>,
        button_serial: u32,
        rect: [f64; 4],
    ) {
        self.proxy.promote(
            serial,
            seat,
            button_serial,
            rect[0],
            rect[1],
            rect[2],
            rect[3],
        );
    }
    pub(crate) fn move_with_grab(&self, seat: &wl_seat::WlSeat, button_serial: u32) {
        self.proxy._move(seat, button_serial);
    }
    pub(crate) fn set_body_size(&self, width: u32, height: u32) {
        self.proxy.set_body_size(width as f64, height as f64);
    }
    pub(crate) fn dock(&self, serial: u32) {
        self.proxy.dock(serial);
    }
    pub(crate) fn cancel(&self, serial: u32) {
        self.proxy.cancel(serial);
    }
}

impl Drop for LayerPromotionController {
    fn drop(&mut self) {
        self.proxy.destroy();
    }
}

impl Dispatch<BeadLayerPromoteV1, wl_surface::WlSurface> for App {
    fn event(
        state: &mut Self,
        _proxy: &BeadLayerPromoteV1,
        event: bead_layer_promote_v1::Event,
        surface: &wl_surface::WlSurface,
        _conn: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        match event {
            bead_layer_promote_v1::Event::Configure {
                serial,
                state: mode,
                x,
                y,
                width,
                height,
                output,
            } => {
                let configured = match mode {
                    WEnum::Value(mode) => mode,
                    WEnum::Unknown(_) => return,
                };
                let output_for_dock = output.clone();
                let (action, redraw) = state
                    .on_surface(surface, |app| {
                        let Some(layer) = app
                            .surfaces
                            .iter_mut()
                            .find(|item| item.layer.wl_surface() == surface)
                        else {
                            return (ConfigureAction::Ignore, false);
                        };
                        let was_promoted = layer.promotion_state.is_promoted();
                        let action = layer.promotion_state.configure(serial, configured);
                        if action == ConfigureAction::Ignore {
                            return (action, false);
                        }
                        let redraw = configure_requires_redraw(was_promoted, configured);
                        if let Some(info) = &mut app.pearl {
                            info.rect = bead_core::pearl::Rect::new(x, y, width, height);
                        }
                        layer.layer_promoted = layer.promotion_state.is_promoted();
                        if configured == bead_layer_promote_v1::State::Docked {
                            layer.promoted_size = None;
                            layer.promoted_body_size = None;
                            layer.promotion_reset_motion = true;
                            layer.redraw = true;
                        } else if !was_promoted && layer.layer_promoted {
                            layer.promoted_size = Some((layer.width, layer.height));
                            layer.promotion_reset_motion = true;
                            layer.redraw = true;
                        }
                        layer.output_name = output;
                        if let Some(target) = app.output.outputs().find(|candidate| {
                            app.output
                                .info(candidate)
                                .and_then(|info| info.name)
                                .as_deref()
                                == Some(layer.output_name.as_str())
                        }) {
                            layer.output = target.clone();
                            if app.pearl.is_some() {
                                app.panel_output = Some(target);
                            }
                        }
                        (action, redraw)
                    })
                    .unwrap_or((ConfigureAction::Ignore, false));
                if let ConfigureAction::SendDock(dock_serial) = action {
                    state.on_surface(surface, |app| {
                        if let Some(layer) = app
                            .surfaces
                            .iter_mut()
                            .find(|item| item.layer.wl_surface() == surface)
                        {
                            if let Some(controller) = &layer.layer_promotion {
                                controller.dock(dock_serial);
                            }
                            layer.redraw = true;
                        }
                    });
                }
                let docked_id = state
                    .on_surface(surface, |app| app.pearl.map(|info| info.id))
                    .flatten();
                if action == ConfigureAction::Docked
                    && let Some(id) = docked_id.filter(|id| {
                        state
                            .pending_promotion_dock
                            .as_ref()
                            .is_some_and(|(pending, _)| pending == id)
                    })
                {
                    state.pending_promotion_dock = None;
                    if let Err(error) = state.dock_pearl_local(id, Some(&output_for_dock), true) {
                        eprintln!("PEARL_DOCK_ERROR {error}");
                    }
                }
                if redraw {
                    state.on_surface(surface, |app| {
                        if let Some(index) = app
                            .surfaces
                            .iter()
                            .position(|item| item.layer.wl_surface() == surface)
                        {
                            app.draw(index, qh);
                        }
                    });
                }
            }
            bead_layer_promote_v1::Event::Rejected { serial, .. } => {
                let (action, pearl_id, redraw) = state
                    .on_surface(surface, |app| {
                        let Some(layer) = app
                            .surfaces
                            .iter_mut()
                            .find(|item| item.layer.wl_surface() == surface)
                        else {
                            return (RejectAction::Ignore, None, false);
                        };
                        let action = layer.promotion_state.rejected(serial);
                        let (pearl_id, redraw) = match action {
                            RejectAction::RollbackPromotion => {
                                layer.layer_promoted = false;
                                layer.promoted_body_origin = None;
                                layer.promoted_size = None;
                                layer.promoted_body_size = None;
                                (app.pearl.map(|info| info.id), true)
                            }
                            RejectAction::KeepPromotion => {
                                layer.layer_promoted = true;
                                (None, false)
                            }
                            RejectAction::RejectedDock => {
                                layer.layer_promoted = true;
                                layer.redraw = true;
                                (app.pearl.map(|info| info.id), true)
                            }
                            RejectAction::Ignore => (None, false),
                        };
                        (action, pearl_id, redraw)
                    })
                    .unwrap_or((RejectAction::Ignore, None, false));
                if action == RejectAction::RejectedDock
                    && state
                        .pending_promotion_dock
                        .as_ref()
                        .is_some_and(|(id, _)| pearl_id.is_some_and(|pearl_id| *id == pearl_id))
                {
                    state.pending_promotion_dock = None;
                }
                if action == RejectAction::RollbackPromotion
                    && let Some(id) = pearl_id
                    && let Err(error) = state.dock_pearl_local(id, None, true)
                {
                    eprintln!("PEARL_PROMOTION_ROLLBACK_ERROR {error}");
                }
                if redraw {
                    state.on_surface(surface, |app| {
                        if let Some(index) = app
                            .surfaces
                            .iter()
                            .position(|item| item.layer.wl_surface() == surface)
                        {
                            app.draw(index, qh);
                        }
                    });
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::bead_layer_promote_v1::State;
    use super::{
        ConfigureAction, DockStage, Phase, PromotionState, RejectAction, configure_requires_redraw,
    };

    #[test]
    fn accepted_promotion_move_and_dock_follow_compositor_state() {
        let mut state = PromotionState::default();
        state.stage_promote(1);
        assert_eq!(state.phase(), Phase::PromotePending { serial: 1 });
        assert_eq!(state.configure(0, State::Promoted), ConfigureAction::Ignore);
        assert_eq!(state.configure(1, State::Moving), ConfigureAction::Updated);
        assert!(state.is_promoted());
        assert_eq!(state.stage_dock(2), DockStage::SendNow);
        assert!(
            state.is_promoted(),
            "docking remains pending until acknowledged"
        );
        assert_eq!(state.configure(2, State::Docked), ConfigureAction::Docked);
        assert_eq!(state.phase(), Phase::Docked);
    }

    #[test]
    fn rejection_rolls_back_only_unaccepted_promotion() {
        let mut state = PromotionState::default();
        state.stage_promote(5);
        assert_eq!(state.rejected(4), RejectAction::Ignore);
        assert_eq!(state.rejected(5), RejectAction::RollbackPromotion);
        assert_eq!(state.phase(), Phase::Docked);
        state.stage_promote(6);
        assert_eq!(
            state.configure(6, State::Promoted),
            ConfigureAction::Updated
        );
        assert_eq!(state.stage_dock(7), DockStage::SendNow);
        assert_eq!(
            state.stage_dock(8),
            DockStage::Rejected,
            "one dock may be pending per surface"
        );
        assert_eq!(state.rejected(7), RejectAction::RejectedDock);
        assert!(state.is_promoted());
    }

    #[test]
    fn rejected_move_and_cancel_keep_current_promoted_surface() {
        let mut state = PromotionState::default();
        state.stage_promote(8);
        assert_eq!(
            state.configure(8, State::Promoted),
            ConfigureAction::Updated
        );
        assert_eq!(state.rejected(8), RejectAction::KeepPromotion);
        state.stage_cancel(9);
        assert_eq!(state.rejected(9), RejectAction::KeepPromotion);
        assert!(state.is_promoted());
        assert_eq!(state.configure(8, State::Moving), ConfigureAction::Updated);
    }

    #[test]
    fn dock_requested_before_promotion_ack_waits_and_keeps_original_serial() {
        let mut state = PromotionState::default();
        state.stage_promote(1);
        assert_eq!(state.stage_dock(2), DockStage::AfterPromotionAck);
        assert_eq!(
            state.configure(1, State::Moving),
            ConfigureAction::SendDock(2)
        );
        assert_eq!(state.rejected(2), RejectAction::RejectedDock);
        assert_eq!(state.phase(), Phase::Promoted { serial: 1 });
        assert_eq!(state.configure(1, State::Moving), ConfigureAction::Updated);
    }

    #[test]
    fn rejected_pending_promote_dock_and_unmap_ignore_late_events() {
        let mut state = PromotionState::default();
        state.stage_promote(4);
        assert_eq!(state.stage_dock(5), DockStage::AfterPromotionAck);
        assert_eq!(state.rejected(4), RejectAction::RollbackPromotion);
        assert!(!state.is_promoted());
        state.stage_promote(6);
        assert_eq!(
            state.configure(6, State::Promoted),
            ConfigureAction::Updated
        );
        state.stage_cancel(7);
        state.invalidate();
        assert_eq!(state.configure(7, State::Docked), ConfigureAction::Ignore);
        assert_eq!(state.rejected(7), RejectAction::Ignore);
        assert_eq!(state.phase(), Phase::Docked);
    }

    #[test]
    fn first_ack_and_dock_ack_redraw_but_movement_updates_do_not() {
        assert!(configure_requires_redraw(false, State::Promoted));
        assert!(!configure_requires_redraw(true, State::Moving));
        assert!(configure_requires_redraw(true, State::Docked));
        let mut state = PromotionState::default();
        state.stage_promote(1);
        assert_eq!(
            state.configure(1, State::Promoted),
            ConfigureAction::Updated
        );
        assert!(state.renders_natural_extent());
        assert_eq!(state.stage_dock(2), DockStage::SendNow);
        assert!(
            !state.renders_natural_extent(),
            "pending dock keeps configured root size"
        );
        assert_eq!(state.rejected(2), RejectAction::RejectedDock);
        assert!(
            state.renders_natural_extent(),
            "rejected dock restores natural root size"
        );
    }
}
