// SPDX-License-Identifier: MIT OR Apache-2.0
//! Standard compositor blur only; no vendor protocol, shader or desktop rule changes.
use super::*;
use smithay_client_toolkit::background_effect::{BackgroundEffectHandler, BackgroundEffectState};
use wayland_protocols::ext::background_effect::v1::client::ext_background_effect_manager_v1::Capability as BackgroundCapability;
/// Region coordinates are logical surface units, independent of buffer/fractional scale.
fn region(
    requested: bool,
    advertised: bool,
    capabilities: u32,
    size: (u32, u32),
) -> Option<(i32, i32)> {
    if !requested
        || !advertised
        || capabilities & BackgroundCapability::Blur.bits() == 0
        || size.0 == 0
        || size.1 == 0
    {
        return None;
    }
    Some((i32::try_from(size.0).ok()?, i32::try_from(size.1).ok()?))
}
pub(super) fn update(
    state: &BackgroundEffectState,
    compositor: &CompositorState,
    surface: &mut Surface,
    qh: &QueueHandle<App>,
    requested: bool,
    size: (u32, u32),
) {
    surface.logical_size = size;
    let advertised = state.ext_background_effect_manager_v1().is_ok();
    let capabilities = state.capabilities().map_or(0, |c| c.bits());
    let wanted = region(requested, advertised, capabilities, size);
    let Some((width, height)) = wanted else {
        if let Some(effect) = surface.background.take() {
            println!("BACKGROUND_EFFECT destroyed={}", effect.id().protocol_id());
            effect.destroy();
            surface.background_size = None;
            surface.commit();
            println!("BACKGROUND_EFFECT enabled=false reason=capability_or_request");
        }
        return;
    };
    if surface.background.is_some() && surface.background_size == Some((width, height)) {
        return;
    }
    let area = match Region::new(compositor) {
        Ok(area) => area,
        Err(_) => {
            eprintln!("BACKGROUND_EFFECT code=region_unavailable");
            return;
        }
    };
    if surface.background.is_none() {
        match state.get_background_effect(surface.wl_surface(), qh) {
            Ok(effect) => {
                println!("BACKGROUND_EFFECT created={}", effect.id().protocol_id());
                surface.background = Some(effect);
            }
            Err(_) => {
                eprintln!("BACKGROUND_EFFECT code=unavailable");
                return;
            }
        }
    }
    area.add(0, 0, width, height);
    if let Some(effect) = &surface.background {
        effect.set_blur_region(Some(area.wl_region()));
    }
    surface.background_size = Some((width, height));
    // The protocol copies the region and applies it on commit. This state-only
    // commit does not bake/layout content or create a presentation timing sample.
    surface.commit();
    println!("BACKGROUND_EFFECT enabled=true logical_width={width} logical_height={height}");
}
impl BackgroundEffectHandler for App {
    fn background_effect_state(&mut self) -> &mut BackgroundEffectState {
        &mut self.background
    }
    fn update_capabilities(&mut self) {
        println!(
            "BACKGROUND_EFFECT advertised={} capabilities={}",
            self.background.ext_background_effect_manager_v1().is_ok(),
            self.background.capabilities().map_or(0, |c| c.bits())
        );
        for surface in &mut self.surfaces {
            let size = surface.logical_size;
            let requested = self.options.background_blur && surface.background_visible;
            update(
                &self.background,
                &self.compositor,
                surface,
                &self.qh,
                requested,
                size,
            );
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn opt_in_global_and_blur_capability_are_all_required() {
        assert_eq!(region(false, true, 1, (400, 200)), None);
        assert_eq!(region(true, false, 1, (400, 200)), None);
        assert_eq!(region(true, true, 0, (400, 200)), None);
        assert_eq!(region(true, true, 8, (400, 200)), None);
        assert_eq!(region(true, true, 9, (400, 200)), Some((400, 200)));
    }
    #[test]
    fn region_is_logical_and_invalid_sizes_do_not_allocate_an_effect() {
        assert_eq!(region(true, true, 1, (1920, 50)), Some((1920, 50)));
        assert_eq!(region(true, true, 1, (0, 50)), None);
        assert_eq!(region(true, true, 1, (400, 0)), None);
        assert_eq!(region(true, true, 1, (u32::MAX, 50)), None);
    }
}
