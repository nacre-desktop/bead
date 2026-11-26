// SPDX-License-Identifier: MIT
//! Per-surface rendering, GPU staging and idle release.
use super::*;

fn layer_motion_renderer_supported(kind: RendererKind) -> bool {
    matches!(kind, RendererKind::Cpu | RendererKind::FemtovgGl)
}

fn accepted_frozen_snapshot(
    renderer: &dyn bead_render::Renderer,
    scene: &bead_render::Scene,
    scale: f32,
) -> Result<bool, Box<dyn std::error::Error>> {
    if scene.motion.as_ref().is_some_and(|motion| motion.reveal)
        || renderer.prepared_frame().is_some()
    {
        return Ok(false);
    }
    renderer.reusable_scene(scene, scale)
}

#[cfg(test)]
mod layer_motion_renderer_tests {
    use super::{accepted_frozen_snapshot, layer_motion_renderer_supported};
    use bead_render::{Renderer, RendererKind};

    #[test]
    fn cpu_and_native_gl_can_submit_compositor_motion_atlases() {
        assert!(layer_motion_renderer_supported(RendererKind::Cpu));
        assert!(layer_motion_renderer_supported(RendererKind::FemtovgGl));
        assert!(!layer_motion_renderer_supported(RendererKind::Femtovg));
        assert!(!layer_motion_renderer_supported(RendererKind::Vello));
    }

    #[test]
    fn gpu_reveal_requires_exact_opt_in_value() {
        assert!(!super::layer_reveal_enabled(None));
        assert!(!super::layer_reveal_enabled(Some("0")));
        assert!(!super::layer_reveal_enabled(Some("true")));
        assert!(super::layer_reveal_enabled(Some("1")));
    }

    #[test]
    fn supported_reveal_reversals_keep_the_compositor_controller() {
        assert!(!super::layer_motion_reveal_reversal_uses_cpu(
            true, true, true, true, true
        ));
        assert!(super::layer_motion_reveal_reversal_uses_cpu(
            true, true, true, true, false
        ));
        assert!(!super::layer_motion_completion_uses_cpu_reveal(
            true,
            Some(false),
            true,
            true,
            true
        ));
    }

    #[test]
    fn prepared_release_requires_a_current_frozen_non_reveal_snapshot() {
        let mut scene = bead_render::Scene {
            opacity: 1.,
            motion: None,
            resource_revision: 1,
            asset_data: Default::default(),
            clips: Default::default(),
            layers: vec![],
            prepared: Default::default(),
            font_files: vec![],
            width: 16.,
            slide: None,
            primitives: vec![bead_render::Primitive::Rounded {
                rect: bead_render::Rect::new(0., 0., 16., 16.),
                radius: 0.,
                fill: [20, 40, 80, 255],
                border: None,
                radial: None,
            }],
            offset: (0., 0.),
            bounds: bead_render::Rect::new(0., 0., 16., 16.),
            input_islands: None,
            key_repeat: None,
            palette_at_rest: false,
        };
        let frozen = bead_render::tree::TreeCpuRenderer::snapshot(&scene, 1.).unwrap();
        assert!(accepted_frozen_snapshot(&frozen, &scene, 1.).unwrap());

        let mut direct = bead_render::tree::TreeCpuRenderer::default();
        direct.prepare(&scene, 1.).unwrap();
        assert!(!accepted_frozen_snapshot(&direct, &scene, 1.).unwrap());

        scene.motion = Some(bead_render::Motion {
            id: "release-guard".into(),
            open: false,
            duration: 0.2,
            return_duration: None,
            return_curve: None,
            auto_open: false,
            spring: false,
            curve: None,
            edge: bead_core::Edge::Bottom,
            slide_distance: None,
            scale_from: None,
            fade: false,
            reveal: true,
            progress: 0.,
            parts: Vec::new(),
            art: None,
            body: None,
        });
        let reveal = bead_render::tree::TreeCpuRenderer::snapshot(&scene, 1.).unwrap();
        assert!(!accepted_frozen_snapshot(&reveal, &scene, 1.).unwrap());
    }
}

/// B0334: a retained GL panel closes without a CPU snapshot and reopens on GL.
#[cfg(test)]
mod retained_gl_open_tests {
    use super::keeps_gl_closed;
    use crate::wallpaper_fade::{gl_surfaces_present, prime_wanted, transport_releasable};
    use bead_render::RendererKind::{Cpu, FemtovgGl};

    /// The shutdown guard of suspend, release, warm-off and the release timers (`has_gl_surfaces`)
    /// for a closed panel: no EGL surface, no fade run, a CPU bar.
    fn closed_transport_releasable(companion: Option<bead_render::RendererKind>) -> bool {
        transport_releasable(
            false,
            gl_surfaces_present(keeps_gl_closed(companion), false),
            true,
        )
    }

    #[test]
    fn a_closed_gl_panel_opens_on_its_kept_gl_renderer_from_the_first_frame() {
        // The renderer is still GL at open, so update_companion_policy has nothing to promote
        // (promotion waited for the motion to end) and never takes the first_frame=cpu path.
        assert!(keeps_gl_closed(Some(FemtovgGl)));
        // B0339: after the grace the companion is CPU, so its next open takes the old path:
        // the CPU snapshot first, then promotion to GL.
        assert!(
            !keeps_gl_closed(Some(Cpu)),
            "a CPU companion still paints a CPU first frame"
        );
    }

    #[cfg(feature = "native-gl")]
    #[test]
    fn the_grace_ends_only_for_the_same_close_of_a_still_closed_kept_panel() {
        use super::kept_gl_grace_due;
        assert!(kept_gl_grace_due(false, true, true));
        assert!(!kept_gl_grace_due(true, true, true), "a reopen cancels it");
        assert!(
            !kept_gl_grace_due(false, false, true),
            "a later close (another close epoch) or another panel arms its own"
        );
        assert!(
            !kept_gl_grace_due(false, true, false),
            "a fallback already left GL"
        );
        assert_eq!(super::KEPT_GL_GRACE, std::time::Duration::from_secs(120));
    }

    #[test]
    fn after_the_grace_the_transport_goes_unless_warm_mode_or_another_owner_holds_it() {
        // The grace makes the companion CPU, then asks transport_releasable(warm, other GL, CPU host).
        let other = |runs| gl_surfaces_present(keeps_gl_closed(Some(Cpu)), runs);
        assert!(transport_releasable(false, other(false), true));
        assert!(
            !transport_releasable(true, other(false), true),
            "warm mode and its prime keep it"
        );
        assert!(
            !transport_releasable(false, other(true), true),
            "a resting wallpaper run keeps it"
        );
        assert!(
            !transport_releasable(false, other(false), false),
            "a GL bar keeps it"
        );
    }

    #[test]
    fn close_keeps_the_transport_and_the_gl_renderer_without_a_snapshot() {
        assert!(!closed_transport_releasable(Some(FemtovgGl)));
        // A CPU companion (the snapshot path) still gives the context back.
        assert!(closed_transport_releasable(Some(Cpu)));
        assert!(
            closed_transport_releasable(None),
            "a released panel gives it back"
        );
    }

    #[test]
    fn warm_off_with_the_panel_closed_keeps_the_transport_for_the_prime() {
        // Warm-off ends the resting runs; the kept panel renderer still holds the transport, and a
        // later warm-on primes on that same context (ensure_transport reuses it).
        assert!(!closed_transport_releasable(Some(FemtovgGl)));
        assert!(prime_wanted(true, false, false, false, true));
    }

    #[cfg(feature = "native-gl")]
    #[test]
    fn a_gl_failure_or_any_budget_refusal_of_a_gl_panel_falls_back_to_cpu() {
        use super::{GpuUnavailable, companion_falls_back};
        // What `Budget::reserve` returns when the GL image admission refuses (B0318).
        let refused: Box<dyn std::error::Error> =
            "retained cache byte budget exceeded: native-gl-images requested=64".into();
        // B0337: any draw of a GL companion (not only an open's first upload) takes the
        // gpu-to-cpu path, like GpuUnavailable from a GL failure, instead of ending the shell.
        assert!(companion_falls_back(true, refused.as_ref()));
        let gl_failure = GpuUnavailable("no EGL".into());
        assert!(companion_falls_back(false, &gl_failure));
        // A CPU companion's refusal and other GL errors keep today's handling, and pending
        // images still go to the worker.
        assert!(!companion_falls_back(false, refused.as_ref()));
        let missing: Box<dyn std::error::Error> = "GL canvas missing".into();
        assert!(!companion_falls_back(true, missing.as_ref()));
        let pending = bead_render::tree::ResourcesPending { images: Vec::new() };
        assert!(!companion_falls_back(true, &pending));
        // B0338: the renderer's own texture budget refusing also falls back, with one
        // RENDER_BUDGET line (owner, need, cap, action).
        let texture = bead_render::gl::TextureBudgetRefused {
            need: 64,
            cap: 64 * bead_core::cache::MIB,
        };
        assert!(companion_falls_back(true, &texture));
        assert!(super::report_texture_refusal(&texture));
        assert!(!super::report_texture_refusal(&gl_failure));
        // After the fallback the companion is CPU: its closes take the CPU snapshot again.
        assert!(!keeps_gl_closed(Some(Cpu)));
    }
}

fn draw_waits_for_callback(pending: bool, gpu: bool, compositor_owns_motion: bool) -> bool {
    pending && gpu && !compositor_owns_motion
}

/// B0420: how long a stand-in waits for the callback niri may never send a commit without damage.
fn missed_callback_delay(damage_px: u64, in_progress: bool, refresh: u32) -> Option<Duration> {
    let ns = if refresh > 0 { refresh } else { 16_000_000 };
    (damage_px == 0 && in_progress).then(|| Duration::from_nanos(ns.into()))
}

/// B0420b: niri ignores damage off its output, so per axis the surface centre clamped onto it.
fn visible_feedback_point(origin: i32, size: u32, span: i32, buffer: i32) -> u32 {
    let (lo, hi) = (origin.max(0), (origin + size as i32).min(span) - 1);
    let point = i64::from((origin + size as i32 / 2).min(hi).max(lo) - origin);
    (point * i64::from(buffer) / i64::from(size.max(1))).clamp(0, i64::from(buffer) - 1) as u32
}

#[cfg(test)]
mod missed_callback_tests {
    use super::{draw_waits_for_callback, missed_callback_delay, visible_feedback_point};
    use std::time::Duration;

    const REFRESH: u32 = 16_666_667;

    #[test]
    fn a_motion_frame_without_damage_gets_its_next_pose_within_a_refresh_without_a_callback() {
        // B0420: a pixel-identical pose on an already-presented generation commits with no damage,
        // which niri may never call back. A stand-in still lets the next pose draw a refresh later.
        let wait = missed_callback_delay(0, true, REFRESH).expect("no stand-in for the callback");
        assert!(wait <= Duration::from_nanos(REFRESH.into()));
        assert_eq!(
            missed_callback_delay(0, true, 0),
            Some(Duration::from_millis(16))
        );
        // It releases the owed callback (no compositor reply), so the waiting GPU draw goes ahead.
        let mut callbacks = crate::policy::Callbacks::default();
        callbacks.request();
        assert!(draw_waits_for_callback(callbacks.pending, true, false));
        callbacks.invalidate();
        assert!(!draw_waits_for_callback(callbacks.pending, true, false));
    }

    #[test]
    fn an_idle_or_damaged_frame_arms_no_stand_in() {
        // Idle (no motion or open in progress): no timer, so no wakeup. Damaged: niri repaints it.
        assert_eq!(missed_callback_delay(0, false, REFRESH), None);
        assert_eq!(missed_callback_delay(1, true, REFRESH), None);
        assert_eq!(missed_callback_delay(1, false, REFRESH), None);
    }

    #[test]
    fn feedback_damage_lands_on_the_output_part_of_a_surface_wider_than_its_output() {
        // B0420b: a centred 1942 px panel surface on a 1920 px output starts at x = -11, so its
        // buffer (0,0) is off the output and niri neither repaints nor calls that damage back.
        let on_output = |origin: i32, x: u32, size: u32, buffer: i32, span: i32| {
            let logical = origin + (i64::from(x) * i64::from(size) / i64::from(buffer)) as i32;
            (0..span).contains(&logical)
        };
        // Scale 1 and 1.5.
        for buffer in [1942, 2913] {
            let x = visible_feedback_point(-11, 1942, 1920, buffer);
            assert!(on_output(-11, x, 1942, buffer, 1920), "x={x} of {buffer}");
        }
        // Past the far edge it clamps onto the output; a surface fully on it gets its centre.
        let x = visible_feedback_point(1500, 1000, 1920, 1000);
        assert!(on_output(1500, x, 1000, 1000, 1920), "x={x}");
        assert_eq!(visible_feedback_point(100, 600, 1920, 900), 450);
        // No known output: still a point inside the buffer.
        assert!(visible_feedback_point(0, 600, 0, 900) < 900);
    }
}

fn layer_motion_handoff_eligible(
    opening: bool,
    sampled_progress: Option<f32>,
    compositor_owns_timeline: bool,
) -> bool {
    compositor_owns_timeline
        || sampled_progress.is_some_and(|progress| {
            if opening {
                progress == 0.
            } else {
                progress == 1.
            }
        })
}

fn layer_motion_reveal_reversal_uses_cpu(
    opening: bool,
    reveal: bool,
    target_changed: bool,
    compositor_owns_timeline: bool,
    compositor_has_contour: bool,
) -> bool {
    opening && reveal && target_changed && compositor_owns_timeline && !compositor_has_contour
}

pub(super) fn reveal_contour_capable(
    scene: &Scene,
    manager: Option<&crate::layer_motion::LayerMotionManager>,
) -> bool {
    let Some(motion) = scene.motion.as_ref().filter(|motion| motion.reveal) else {
        return true;
    };
    layer_reveal_enabled(std::env::var("BEAD_LAYER_REVEAL").ok().as_deref())
        && manager.is_some_and(crate::layer_motion::LayerMotionManager::supports_reveal)
        && (motion.art.as_ref().is_none_or(|art| art.contour.is_some()))
}

fn layer_reveal_enabled(value: Option<&str>) -> bool {
    value == Some("1")
}

fn layer_motion_cpu_replacement_pending(
    surface_still_owns_atlas: bool,
    controller_has_active_serial: bool,
    target_changed: bool,
    plan_can_retarget: bool,
) -> bool {
    surface_still_owns_atlas
        && !controller_has_active_serial
        && (!target_changed || !plan_can_retarget)
}

pub(super) fn layer_motion_completion_uses_cpu_reveal(
    completed: bool,
    completed_target_open: Option<bool>,
    current_target_open: bool,
    current_target_reveals: bool,
    compositor_has_contour: bool,
) -> bool {
    completed
        && completed_target_open == Some(false)
        && current_target_open
        && current_target_reveals
        && !compositor_has_contour
}

fn copy_layer_motion_pixels(target: &mut [u8], pixels: &[u8]) -> bool {
    let Some(target) = target.get_mut(..pixels.len()) else {
        return false;
    };
    target.copy_from_slice(pixels);
    true
}

fn is_cache_pressure(error: &str) -> bool {
    let error = error.to_ascii_lowercase();
    error.contains("budget") || error.contains("admission") || error.contains("cache pressure")
}

/// B0334: a retained panel's GL renderer survives its close. It keeps the shared context alive (no
/// CPU snapshot) and paints the next open from its first frame. A CPU renderer, also after a GL
/// failure or a budget fallback, closes to the CPU snapshot.
pub(super) fn keeps_gl_closed(companion: Option<RendererKind>) -> bool {
    companion == Some(RendererKind::FemtovgGl)
}
/// B0339: how long a closed panel keeps its GL renderer and context before the CPU snapshot
/// close path takes over (the idle rule: a kept context above ~10 MiB gets a grace).
#[cfg(feature = "native-gl")]
const KEPT_GL_GRACE: Duration = Duration::from_secs(120);
/// B0339: the grace ends only for the same close of a panel that is still closed and kept.
#[cfg(feature = "native-gl")]
fn kept_gl_grace_due(open: bool, same_close: bool, kept: bool) -> bool {
    !open && same_close && kept
}

/// B0318/B0334/B0337: a GL failure, or a budget refusal on any draw of a GL companion, takes the
/// gpu-to-cpu fallback (a CPU companion, CPU snapshots at close from then on) instead of App::fail.
#[cfg(any(feature = "native-gl", feature = "wgpu"))]
fn companion_falls_back(gl_companion: bool, error: &(dyn Error + 'static)) -> bool {
    error.is::<GpuUnavailable>() || (gl_companion && is_cache_pressure(&error.to_string()))
}

/// B0338: a fallback on the renderer's texture budget prints its RENDER_BUDGET line (action=cpu).
#[cfg(feature = "native-gl")]
fn report_texture_refusal(error: &(dyn Error + 'static)) -> bool {
    error
        .downcast_ref::<bead_render::gl::TextureBudgetRefused>()
        .is_some_and(|refused| {
            bead_core::cache::report_degrade("native-gl-images", refused.need, refused.cap, "cpu")
        })
}

fn report_cache_pressure(owner: &str, error: &str) {
    static LAST_REPORT: std::sync::OnceLock<std::sync::Mutex<Option<Instant>>> =
        std::sync::OnceLock::new();
    let now = Instant::now();
    let mut last = LAST_REPORT
        .get_or_init(|| std::sync::Mutex::new(None))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if last.is_none_or(|last| now.duration_since(last) >= Duration::from_secs(5)) {
        eprintln!("CACHE_PRESSURE owner={owner} action=render_without_snapshot detail={error}");
        *last = Some(now);
    }
}

pub(super) fn prepare_pending_images(
    error: &(dyn Error + 'static),
    source: &mut dyn SceneSource,
) -> Result<bool, Box<dyn Error>> {
    if let Some(pending) = error.downcast_ref::<bead_render::tree::CpuResourcesPending>() {
        source.prepare_cpu(pending.request.clone())?;
        return Ok(true);
    }
    let Some(pending) = error.downcast_ref::<bead_render::tree::ResourcesPending>() else {
        return Ok(false);
    };
    source.prepare_images(pending.images.clone())?;
    Ok(true)
}

/// Hidden content never requests a keyboard, including retained warm panels.
fn panel_keyboard_mode(visible: bool, click_focus: bool, exclusive: bool) -> KeyboardInteractivity {
    if !visible {
        KeyboardInteractivity::None
    } else if click_focus || !exclusive {
        KeyboardInteractivity::OnDemand
    } else {
        KeyboardInteractivity::Exclusive
    }
}

#[cfg(test)]
mod keyboard_policy_tests {
    use super::*;

    #[test]
    fn warm_updates_never_reacquire_keyboard_and_pinning_changes_mode() {
        for pinned in [false, true] {
            for typing in [false, true] {
                assert_eq!(
                    panel_keyboard_mode(false, pinned, typing),
                    KeyboardInteractivity::None
                );
            }
        }
        assert_eq!(
            panel_keyboard_mode(true, false, true),
            KeyboardInteractivity::Exclusive
        );
        assert_eq!(
            panel_keyboard_mode(true, true, true),
            KeyboardInteractivity::OnDemand
        );
    }

    #[test]
    #[cfg(feature = "native-gl")]
    fn cache_pressure_is_a_recoverable_render_condition() {
        assert!(is_cache_pressure("retained cache byte budget exceeded"));
        assert!(is_cache_pressure("CPU snapshot admission failed"));
        assert!(!is_cache_pressure("invalid wallpaper image header"));
    }
}

#[cfg(test)]
mod layer_motion_buffer_tests {
    use super::copy_layer_motion_pixels;

    #[test]
    fn atlas_upload_accepts_slot_pool_alignment_padding_only_after_valid_pixels() {
        let pixels = (0..127).map(|value| value as u8).collect::<Vec<_>>();
        let mut padded = vec![0xEE; pixels.len() + 16];
        assert!(copy_layer_motion_pixels(&mut padded, &pixels));
        assert_eq!(&padded[..pixels.len()], pixels);
        assert!(padded[pixels.len()..].iter().all(|byte| *byte == 0xEE));

        let mut short = vec![0; pixels.len() - 1];
        assert!(!copy_layer_motion_pixels(&mut short, &pixels));
    }
}

#[cfg(test)]
mod layer_motion_handoff_tests {
    use super::{draw_waits_for_callback, layer_motion_handoff_eligible};

    #[test]
    fn a_compositor_owned_motion_never_waits_for_a_surface_frame_callback() {
        assert!(draw_waits_for_callback(true, true, false));
        assert!(!draw_waits_for_callback(true, true, true));
        assert!(!draw_waits_for_callback(true, false, false));
        assert!(!draw_waits_for_callback(false, true, false));
    }
    use bead_core::animation::{Animations, Curve};

    #[test]
    fn cpu_motion_handoff_requires_the_matching_sampled_endpoint() {
        let mut animations = Animations::default();
        animations.open_to("panel", true, 0., 1., Curve::EaseOut);
        assert!(layer_motion_handoff_eligible(
            true,
            animations.value("panel", 0.).map(|(progress, _)| progress),
            false,
        ));
        animations.committed("panel", 0., 0.);

        let open_time = 1. - 0.6_f64.cbrt();
        let (opening_progress, active) = animations.value("panel", open_time).unwrap();
        assert!(active && (opening_progress - 0.4).abs() < 1e-6);
        assert!(!layer_motion_handoff_eligible(
            false,
            Some(opening_progress),
            false,
        ));
        animations.open_to("panel", false, open_time, 1., Curve::EaseOut);
        animations.committed("panel", open_time, open_time);
        let (closing_progress, active) = animations.value("panel", open_time).unwrap();
        assert!(active && (closing_progress - opening_progress).abs() < f32::EPSILON);
        assert!(!layer_motion_handoff_eligible(
            false,
            Some(closing_progress),
            false,
        ));

        let close_reverse_time = open_time + 0.1;
        let (closing_midflight, active) = animations.value("panel", close_reverse_time).unwrap();
        assert!(active && closing_midflight > 0. && closing_midflight < 1.);
        animations.open_to("panel", true, close_reverse_time, 1., Curve::EaseOut);
        animations.committed("panel", close_reverse_time, close_reverse_time);
        let (opening_after_close, active) = animations.value("panel", close_reverse_time).unwrap();
        assert!(active && (opening_after_close - closing_midflight).abs() < f32::EPSILON);
        assert!(!layer_motion_handoff_eligible(
            true,
            Some(opening_after_close),
            false,
        ));

        let settled_time = close_reverse_time + 2.;
        let (settled_open, active) = animations.value("panel", settled_time).unwrap();
        assert!(!active && settled_open == 1.);
        animations.open_to("panel", false, settled_time, 1., Curve::EaseOut);
        let (closing_endpoint, active) = animations.value("panel", settled_time).unwrap();
        assert!(active && closing_endpoint == 1.);
        assert!(layer_motion_handoff_eligible(
            false,
            Some(closing_endpoint),
            false
        ));

        // Once the server owns presentation, CPU clock progress is irrelevant:
        // the compositor retarget path samples its current pose.
        assert!(layer_motion_handoff_eligible(false, Some(0.4), true));
    }
}

fn content_body(scene: &Scene) -> Option<bead_render::Rect> {
    if let Some(body) = scene.motion.as_ref().and_then(|m| m.body) {
        return Some(body);
    }
    scene
        .layers
        .iter()
        .find(|layer| layer.parent.is_none())
        .map(|layer| layer.bounds)
        .or_else(|| {
            scene
                .primitives
                .iter()
                .find_map(|primitive| match primitive {
                    bead_render::Primitive::Rounded { rect, .. } if rect.w > 0. && rect.h > 0. => {
                        Some(*rect)
                    }
                    _ => None,
                })
        })
}

/// `margin`: the scene's shadow margin, `-bounds` origin (32 from Luau layout, 0 for a folder view
/// that fills its surface), so a panel's resting content sits where its host laid it out.
fn motion_scene_offset(
    prior_x: f32,
    edge: bead_core::Edge,
    opening: bool,
    (dx, dy): (f32, f32),
    margin: (f32, f32),
) -> (f32, f32) {
    let x = if !opening && matches!(edge, bead_core::Edge::Top | bead_core::Edge::Bottom) {
        prior_x
    } else {
        margin.0 + dx
    };
    (x, margin.1 + dy)
}

/// The docked layer may be clipped to the output while its detached window
/// must retain the complete panel content extent.
pub(super) fn natural_surface_size(scene: &Scene) -> Option<(u32, u32)> {
    let width = scene.bounds.w.ceil();
    let height = (scene.bounds.y + scene.bounds.h + scene.offset.1).ceil();
    if !width.is_finite()
        || !height.is_finite()
        || width <= 0.
        || height <= 0.
        || width > 32768.
        || height > 32768.
    {
        return None;
    }
    Some((width as u32, height as u32))
}

fn effective_render_size(
    configured: (u32, u32),
    promoted: bool,
    natural: Option<(u32, u32)>,
) -> (u32, u32) {
    if promoted {
        natural.unwrap_or(configured)
    } else {
        configured
    }
}

#[cfg(any(feature = "native-gl", test))]
fn native_gl_dimensions(scene: &Scene, scale120: u32) -> Result<(u32, u32), Box<dyn Error>> {
    let (width, height) = natural_surface_size(scene).ok_or("invalid natural scene extent")?;
    let (width, height, _, _) = buffer_dimensions(width, height, scale120)?;
    Ok((width as u32, height as u32))
}

/// Native window geometry describes the complete body, even during its reveal.
pub(super) fn content_window_rect(scene: &Scene) -> Option<[i32; 4]> {
    let body = content_body(scene).filter(|body| body.w > 0. && body.h > 0.)?;
    let body = if scene.motion.is_some() {
        body.translated(-scene.bounds.x, -scene.bounds.y)
    } else {
        body.translated(scene.offset.0, scene.offset.1)
    };
    let (x, y) = (body.x.floor(), body.y.floor());
    Some([
        x as i32,
        y as i32,
        ((body.x + body.w).ceil() - x).max(0.) as i32,
        ((body.y + body.h).ceil() - y).max(0.) as i32,
    ])
}

/// The integer rect that contains `r`, as `content_input_rect` rounds it.
pub(super) fn outer_rect(r: bead_render::Rect) -> [i32; 4] {
    let (x, y) = (r.x.floor(), r.y.floor());
    [
        x as i32,
        y as i32,
        ((r.x + r.w).ceil() - x).max(0.) as i32,
        ((r.y + r.h).ceil() - y).max(0.) as i32,
    ]
}

/// A bar's children-only input (B0321): each island moved by the scene offset, then the hosted
/// lips, which keep taking the bar's pointer. Empty rects are dropped.
fn island_input_rects(
    islands: &[bead_render::Rect],
    offset: (f32, f32),
    lips: &[[i32; 4]],
) -> Vec<[i32; 4]> {
    islands
        .iter()
        .map(|r| outer_rect(r.translated(offset.0, offset.1)))
        .chain(lips.iter().copied())
        .filter(|r| r[2] > 0 && r[3] > 0)
        .collect()
}

/// What a bar surface does with its input region this frame (B0321).
#[derive(Debug, PartialEq)]
enum IslandInput {
    Keep,
    Set(Vec<[i32; 4]>),
    Reset,
}

/// Compares the wanted islands with the list last applied: set on change, reset when the prop is gone.
fn island_input_action(
    islands: Option<&[bead_render::Rect]>,
    offset: (f32, f32),
    lips: &[[i32; 4]],
    applied: Option<&[[i32; 4]]>,
) -> IslandInput {
    match (islands, applied) {
        (None, None) => IslandInput::Keep,
        (None, Some(_)) => IslandInput::Reset,
        (Some(islands), applied) => {
            let rects = island_input_rects(islands, offset, lips);
            if applied == Some(rects.as_slice()) {
                IslandInput::Keep
            } else {
                IslandInput::Set(rects)
            }
        }
    }
}

/// Shadows reserve buffer space, but must not intercept adjacent controls.
/// Retained layout bounds survive a shell replacing its background primitive.
pub(super) fn content_input_rect(scene: &Scene) -> Option<[i32; 4]> {
    let body = content_body(scene)?;
    let painted = body.translated(scene.offset.0, scene.offset.1);
    // An entering panel may slide across adjacent surfaces. Keep hit testing
    // inside its final content footprint throughout the animation.
    let clip = if scene.motion.is_some() {
        body.translated(-scene.bounds.x, -scene.bounds.y)
    } else {
        painted
    };
    let x = painted.x.max(clip.x).floor();
    let y = painted.y.max(clip.y).floor();
    let right = (painted.x + painted.w).min(clip.x + clip.w).ceil();
    let bottom = (painted.y + painted.h).min(clip.y + clip.h).ceil();
    Some([
        x as i32,
        y as i32,
        (right - x).max(0.) as i32,
        (bottom - y).max(0.) as i32,
    ])
}

/// Limit compositor body geometry to the root's currently committed logical
/// extent. Promotion preserves the existing buffer and cannot enlarge it.
pub(super) fn clip_body_rect_to_surface(body: [i32; 4], size: (u32, u32)) -> Option<[i32; 4]> {
    if body[2] <= 0 || body[3] <= 0 {
        return None;
    }
    let width = i64::from(size.0.min(i32::MAX as u32));
    let height = i64::from(size.1.min(i32::MAX as u32));
    let left = i64::from(body[0]).clamp(0, width);
    let top = i64::from(body[1]).clamp(0, height);
    let right = (i64::from(body[0]) + i64::from(body[2])).clamp(left, width);
    let bottom = (i64::from(body[1]) + i64::from(body[3])).clamp(top, height);
    (right > left && bottom > top).then_some([
        left as i32,
        top as i32,
        (right - left) as i32,
        (bottom - top) as i32,
    ])
}

fn visible_promoted_body_size(scene: &Scene, root_size: (u32, u32)) -> Option<(u32, u32)> {
    content_input_rect(scene)
        .and_then(|body| clip_body_rect_to_surface(body, root_size))
        .map(|body| (body[2] as u32, body[3] as u32))
}

/// Keep the existing visible body at its original root-local origin after
/// detached UI updates change scene padding or position.
fn stabilize_body_origin(scene: &mut Scene, origin: (i32, i32)) {
    if let Some(current) = content_input_rect(scene)
        && current[2] > 0
        && current[3] > 0
    {
        scene.offset.0 += (origin.0 - current[0]) as f32;
        scene.offset.1 += (origin.1 - current[1]) as f32;
    }
}

fn stage_command_promotion(surface: &mut Surface, body: [i32; 4]) -> bool {
    if surface.layer_promoted {
        return true;
    }
    let Some(controller) = surface.layer_promotion.as_ref() else {
        return false;
    };
    if surface.layer_promoted {
        return false;
    }
    surface.promotion_serial = surface.promotion_serial.wrapping_add(1).max(1);
    surface
        .promotion_state
        .stage_promote(surface.promotion_serial);
    surface.layer_promoted = true;
    surface.promoted_body_origin = Some((body[0], body[1]));
    surface.promoted_body_size = Some((body[2] as u32, body[3] as u32));
    controller.promote(
        surface.promotion_serial,
        None,
        0,
        [
            body[0] as f64,
            body[1] as f64,
            body[2] as f64,
            body[3] as f64,
        ],
    );
    true
}

fn should_stage_command_promotion(
    pending: bool,
    companion: bool,
    configured: bool,
    controller: bool,
) -> bool {
    pending && companion && configured && controller
}

/// Adopt only pixels the following complete repairs will leave untouched.
pub(super) fn copy_outside_repairs(
    source: &[u8],
    target: &mut [u8],
    width: u32,
    height: u32,
    repairs: &[Damage],
    zero_target: bool,
) {
    for y in 0..height {
        let row = y as usize * width as usize * 4;
        let mut x = 0;
        loop {
            // Repair history is bounded and tiny. Find/merge the next horizontal
            // interval in place instead of allocating and sorting a Vec per frame.
            let next = repairs
                .iter()
                .filter(|r| y >= r.y && y < r.y.saturating_add(r.h) && r.w != 0)
                .map(|r| (r.x.min(width), r.x.saturating_add(r.w).min(width)))
                .filter(|(left, right)| *right > x && *left <= x)
                .map(|(_, right)| right)
                .max();
            if let Some(right) = next {
                x = right;
                continue;
            }
            let next_left = repairs
                .iter()
                .filter(|r| y >= r.y && y < r.y.saturating_add(r.h) && r.w != 0)
                .map(|r| r.x.min(width))
                .filter(|left| *left > x)
                .min()
                .unwrap_or(width);
            if next_left > x {
                let range = row + x as usize * 4..row + next_left as usize * 4;
                copy_adopted_span(&source[range.clone()], &mut target[range], zero_target);
            }
            if next_left == width {
                break;
            }
            x = next_left;
        }
    }
}

#[cfg(test)]
mod repair_copy_tests {
    use super::*;

    #[test]
    fn copies_only_the_complement_of_unsorted_overlapping_repairs() {
        let width = 8;
        let height = 3;
        let source: Vec<u8> = (0..width * height * 4).map(|n| n as u8).collect();
        let mut target = vec![0; source.len()];
        let repairs = [
            Damage {
                x: 4,
                y: 0,
                w: 2,
                h: 3,
            },
            Damage {
                x: 1,
                y: 1,
                w: 3,
                h: 1,
            },
            Damage {
                x: 2,
                y: 1,
                w: 4,
                h: 1,
            },
        ];

        copy_outside_repairs(&source, &mut target, width, height, &repairs, false);

        for y in 0..height {
            for x in 0..width {
                let at = ((y * width + x) * 4) as usize;
                let repaired =
                    (y == 0 || y == 2) && (4..6).contains(&x) || y == 1 && (1..6).contains(&x);
                assert_eq!(
                    &target[at..at + 4],
                    if repaired {
                        &[0; 4]
                    } else {
                        &source[at..at + 4]
                    },
                    "pixel ({x},{y})"
                );
            }
        }
    }
}

fn copy_adopted_span(source: &[u8], target: &mut [u8], zero_target: bool) {
    if !zero_target {
        target.copy_from_slice(source);
        return;
    }
    static ZERO: [u8; 4096] = [0; 4096];
    for (source, target) in source.chunks(ZERO.len()).zip(target.chunks_mut(ZERO.len())) {
        // Fresh slots in a new SHM pool are zero-filled. Avoid faulting their
        // transparent margins into residency just to copy zeroes.
        // SAFETY: both read ranges contain source.len() bytes (at most 4096).
        if unsafe { libc::memcmp(source.as_ptr().cast(), ZERO.as_ptr().cast(), source.len()) } != 0
        {
            target.copy_from_slice(source);
        }
    }
}

/// Best-effort huge-page advice on the mapped pages touched by an existing frame.
/// Adjacent slots must share advice on their boundary page, so the kernel can
/// merge their VMAs and use huge pages across slot boundaries.
fn advise_frame_pages(bytes: &mut [u8], include_edges: bool) {
    #[cfg(target_os = "linux")]
    {
        // SAFETY: sysconf has no pointer or process-state preconditions.
        let page = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
        if page <= 0 {
            return;
        }
        let page = page as usize;
        let start = bytes.as_mut_ptr() as usize;
        let end = start + bytes.len();
        let (begin, end) = if include_edges {
            (start / page * page, end.next_multiple_of(page))
        } else {
            (start.next_multiple_of(page), end / page * page)
        };
        if end > begin {
            // SAFETY: SlotPool maps whole OS pages. Rounding a live slot to
            // its containing pages stays in that mapping; advice does not
            // read or write other slots sharing the first/last page.
            // Failure merely retains ordinary small-page allocation behavior.
            unsafe {
                libc::madvise(begin as *mut _, end - begin, libc::MADV_HUGEPAGE);
            }
        }
    }
    #[cfg(not(target_os = "linux"))]
    let _ = (bytes, include_edges);
}

/// Quiet time before an idle SHM pool is compacted. A surface that holds keyboard
/// focus (an open launcher) waits longer: the retire-and-repaint costs the next
/// key ~2.4 ms of UI CPU (B0040), and hiding the panel destroys its pool anyway.
/// Set `SHM_COMPACT_FOCUSED_IDLE` to `SHM_COMPACT_IDLE` for the previous behaviour.
const SHM_COMPACT_IDLE: Duration = Duration::from_millis(250);
const SHM_COMPACT_FOCUSED_IDLE: Duration = Duration::from_secs(5);

#[cfg(feature = "native-gl")]
pub(super) struct GlPreparation {
    result: std::sync::mpsc::Receiver<Result<bead_render::gl::PreparedTransport, String>>,
    worker: Option<std::thread::JoinHandle<()>>,
    // Declared last: unused EGL objects are destroyed before the connection.
    _connection: Connection,
}
#[cfg(feature = "native-gl")]
impl Drop for GlPreparation {
    fn drop(&mut self) {
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
        if let Ok(Ok(prepared)) = self.result.try_recv() {
            // SAFETY: one preparation is allowed, and no UI GL transport was installed.
            if let Err(error) = unsafe { prepared.shutdown_exclusive() } {
                eprintln!("GL_RELEASE_ERROR {error}");
            }
        }
    }
}
/// The shared transport for a synchronous owner (a fade start, a bar or panel draw), made on
/// first use. B0398: it waits for the preparation in flight instead of initializing the display
/// a second time, and initializes only after the pending shutdown.
#[cfg(feature = "native-gl")]
pub(super) fn shared_transport<'a>(
    gl: &'a mut Option<bead_render::gl::Transport>,
    preparation: &mut Option<GlPreparation>,
    release: &mut crate::wallpaper_fade::PendingRelease,
    display: raw_window_handle::RawDisplayHandle,
) -> Result<&'a mut bead_render::gl::Transport, Box<dyn Error>> {
    if gl.is_none() {
        *gl = Some(match preparation.take() {
            Some(preparation) => preparation
                .result
                .recv()
                .unwrap_or_else(|_| Err("GL preparation worker disconnected".into()))?
                .into_transport(),
            None => {
                drop(std::mem::take(release));
                // SAFETY: the caller's retained connection outlives the EGL context and surfaces;
                // nothing else sets this display up or tears it down now.
                unsafe { bead_render::gl::Transport::new(display) }?
            }
        });
    }
    gl.as_mut().ok_or_else(|| "GL transport missing".into())
}
struct PanelSnapshotWorker(Option<std::thread::JoinHandle<()>>);
impl PanelSnapshotWorker {
    fn join(&mut self) -> Option<std::thread::Result<()>> {
        self.0.take().map(std::thread::JoinHandle::join)
    }
}
impl Drop for PanelSnapshotWorker {
    fn drop(&mut self) {
        if let Some(worker) = self.0.take() {
            let _ = worker.join();
        }
    }
}
fn panel_snapshot_worker_failure(
    worker: &mut PanelSnapshotWorker,
    completed: bool,
) -> Option<&'static str> {
    if completed {
        return None;
    }
    if worker.join().is_some_and(|result| result.is_err()) {
        Some("panel CPU snapshot worker panicked")
    } else {
        Some("panel CPU snapshot worker disconnected")
    }
}
#[cfg(test)]
mod panel_snapshot_worker_tests {
    use super::{PanelSnapshotWorker, panel_snapshot_worker_failure};

    #[test]
    fn channel_close_after_completion_is_ignored() {
        let mut worker = PanelSnapshotWorker(Some(std::thread::spawn(|| {})));
        assert!(worker.join().unwrap().is_ok());
        assert_eq!(panel_snapshot_worker_failure(&mut worker, true), None);
    }

    #[test]
    fn disconnected_panicking_worker_is_reported() {
        let mut worker = PanelSnapshotWorker(Some(std::thread::spawn(|| panic!("worker panic"))));
        assert_eq!(
            panel_snapshot_worker_failure(&mut worker, false),
            Some("panel CPU snapshot worker panicked")
        );
    }
}
fn shm_compact_idle(surface: &Surface) -> Duration {
    if surface
        .keyboard_visible
        .is_some_and(|mode| mode != KeyboardInteractivity::None)
    {
        SHM_COMPACT_FOCUSED_IDLE
    } else {
        SHM_COMPACT_IDLE
    }
}
impl App {
    /// RGBA pixels in ABGR8888 buffers when the compositor takes them, else BGRA in ARGB8888.
    pub(super) fn shm_format(&self) -> (PixelFormat, wl_shm::Format) {
        if cfg!(target_endian = "little")
            && !self.options.force_argb
            && self.shm.formats().contains(&wl_shm::Format::Abgr8888)
        {
            (PixelFormat::Rgba, wl_shm::Format::Abgr8888)
        } else {
            (PixelFormat::Bgra, wl_shm::Format::Argb8888)
        }
    }
    fn prepare_surface_images(&mut self, index: usize, error: &(dyn Error + 'static)) -> bool {
        if error.is::<bead_render::tree::ResourcePreparationFailed>() {
            eprintln!("UI_ERROR resource preparation: {error}");
            self.surfaces[index].redraw = false;
            return true;
        }
        let source = if self.surfaces[index].companion {
            self.panel_runtime.as_mut()
        } else {
            self.runtime.as_mut()
        };
        let Some(source) = source else { return false };
        match prepare_pending_images(error, source.as_mut()) {
            Ok(true) => {
                self.surfaces[index].redraw = true;
                if !self.surfaces[index].resources_retry_pending {
                    self.surfaces[index].resources_retry_pending = true;
                    let identity = self.surfaces[index].wl_surface().clone();
                    let qh = self.qh.clone();
                    if let Err(error) = self.loop_handle.insert_source(
                        Timer::from_duration(Duration::from_millis(8)),
                        move |_, _, app| {
                            if let Some(index) = app
                                .surfaces
                                .iter()
                                .position(|s| s.wl_surface() == &identity)
                            {
                                app.surfaces[index].resources_retry_pending = false;
                                app.draw(index, &qh);
                            }
                            TimeoutAction::Drop
                        },
                    ) {
                        self.surfaces[index].resources_retry_pending = false;
                        self.fail(error);
                    }
                }
                true
            }
            Ok(false) => false,
            Err(error) => {
                self.fail(error);
                true
            }
        }
    }
    #[cfg(feature = "native-gl")]
    pub(super) fn prepare_panel_gl(&mut self) -> Result<(), Box<dyn Error>> {
        if self.panel_gl_prewarm_error.is_some()
            || self.options.renderer != RendererKind::Cpu
            || !self.panel_target
            || !self
                .options
                .panel_spec
                .as_ref()
                .is_some_and(|s| s.retain_when_closed)
            || self.options.companion.as_ref().map(|(_, kind)| *kind)
                != Some(RendererKind::FemtovgGl)
        {
            return Ok(());
        }
        self.start_gl_preparation()
    }
    /// B0342: EGL display, config, context and the prewarmed canvas on the "bead-gl-prepare"
    /// thread, adopted in O(1) by a 2 ms poll. B0398: the one preparation of the panel and the
    /// wallpaper primes; it initializes after the pending shutdown, and a synchronous owner waits
    /// for it (`shared_transport`) instead of initializing the display again.
    #[cfg(feature = "native-gl")]
    pub(super) fn start_gl_preparation(&mut self) -> Result<(), Box<dyn Error>> {
        if self.gl.is_some() || self.gl_preparation.is_some() {
            return Ok(());
        }
        let upload = self.prime_target().is_some();
        // B0406: a canvas per wallpaper output to prime, so none compiles on the main thread.
        let canvases = if upload {
            self.wallpaper_surfaces.len()
        } else {
            1
        };
        let release = std::mem::take(&mut self.gl_release);
        let (sender, result) = std::sync::mpsc::channel();
        let connection = self.conn.clone();
        let worker = std::thread::Builder::new()
            .name("bead-gl-prepare".into())
            .spawn(move || {
                use raw_window_handle::{RawDisplayHandle, WaylandDisplayHandle};
                drop(release);
                let prepared = (|| -> Result<_, String> {
                    let _span = bead_core::profile::Span::new("panel_transport_prewarm");
                    let pointer = std::ptr::NonNull::new(connection.backend().display_ptr().cast())
                        .ok_or("native display missing")?;
                    let display = RawDisplayHandle::Wayland(WaylandDisplayHandle::new(pointer));
                    // SAFETY: the pending shutdown is joined, this is the one preparation and no
                    // transport exists (synchronous owners wait for it); the guard holds the
                    // connection.
                    unsafe { bead_render::gl::PreparedTransport::new(display, upload, canvases) }
                        .map_err(|e| e.to_string())
                })();
                if let Err(std::sync::mpsc::SendError(Ok(prepared))) = sender.send(prepared) {
                    // SAFETY: no receiver installed this transport, and setup is exclusive.
                    let _ = unsafe { prepared.shutdown_exclusive() };
                }
            })?;
        self.gl_preparation = Some(GlPreparation {
            result,
            worker: Some(worker),
            _connection: self.conn.clone(),
        });
        if let Err(error) = self.loop_handle.insert_source(
            Timer::from_duration(Duration::from_millis(2)),
            |_, _, app| {
                let began = std::time::Instant::now();
                // Gone: a synchronous owner adopted it (`shared_transport`); the waiters go on.
                if let Some(preparation) = &app.gl_preparation {
                    match preparation.result.try_recv() {
                        Err(std::sync::mpsc::TryRecvError::Empty) => {
                            return TimeoutAction::ToDuration(Duration::from_millis(2));
                        }
                        ready => {
                            let prepared = ready.unwrap_or_else(|_| {
                                Err("GL preparation worker disconnected".into())
                            });
                            app.adopt_gl_preparation(prepared);
                        }
                    }
                }
                let ms = began.elapsed().as_secs_f64() * 1e3;
                app.wallpaper_gl_prepared(ms);
                let qh = app.qh.clone();
                let indices: Vec<_> = app
                    .surfaces
                    .iter()
                    .enumerate()
                    .filter(|(_, s)| s.companion)
                    .map(|(i, _)| i)
                    .collect();
                for index in indices {
                    app.draw(index, &qh);
                }
                TimeoutAction::Drop
            },
        ) {
            self.gl_preparation.take();
            return Err(error.into());
        }
        Ok(())
    }
    /// Install a finished preparation if the panel or a wallpaper prime still wants it (B0398),
    /// else shut it down.
    #[cfg(feature = "native-gl")]
    fn adopt_gl_preparation(
        &mut self,
        prepared: Result<bead_render::gl::PreparedTransport, String>,
    ) {
        let _guard = self.gl_preparation.take();
        let active_gl_companion = self
            .companion
            .as_ref()
            .is_some_and(|(_, _, kind)| *kind == RendererKind::FemtovgGl);
        let panel = self.panel_runtime.is_some()
            && (!self.panel_gpu_suspended || active_gl_companion)
            && self.options.companion.as_ref().map(|(_, kind)| *kind)
                == Some(RendererKind::FemtovgGl);
        let prime = self.prime_target();
        let installed = self.gl.is_some();
        match prepared {
            Ok(prepared)
                if crate::wallpaper_fade::preparation_wanted(installed, panel, prime.is_some()) =>
            {
                self.gl = Some(prepared.into_transport());
            }
            // Another transport owns the display: only drop this context.
            Ok(prepared) if installed => drop(prepared),
            Ok(prepared) => {
                // SAFETY: no GPU renderer or surface was created during preparation.
                if let Err(error) = unsafe { prepared.shutdown_exclusive() } {
                    self.fail(error);
                }
            }
            Err(_) if installed => (),
            Err(error) => {
                if let Some(index) = prime {
                    self.wallpaper_gl_failed = true;
                    let name = &self.wallpaper_surfaces[index].name;
                    eprintln!("WALLPAPER_FADE_FALLBACK output={name} reason={error}");
                }
                if panel {
                    self.panel_gl_prewarm_error = Some(error);
                }
            }
        }
    }
    /// Cache a CPU first frame, then retire closed-panel GPU and SHM allocations.
    pub(super) fn start_panel_snapshot(&mut self) -> Result<(), Box<dyn Error>> {
        if self.panel_snapshot_pending
            || self.panel_target
            || self.panel_runtime.is_none()
            || !self
                .options
                .panel_spec
                .as_ref()
                .is_some_and(|spec| spec.retain_when_closed)
        {
            return Ok(());
        }
        let Some((scene, renderer, kind)) = &self.companion else {
            return Ok(());
        };
        let cpu_renderer = *kind == RendererKind::Cpu;
        let gl_renderer = *kind == RendererKind::FemtovgGl;
        if !cpu_renderer && !gl_renderer {
            return Ok(());
        }
        if !self.panel_snapshot_ready() {
            return Ok(());
        }
        #[cfg(feature = "native-gl")]
        if gl_renderer {
            // B0334: a GL panel closes without a CPU snapshot. It keeps its renderer and the
            // shared context, so its next open paints on GL from the first frame.
            return if self.panel_gpu_suspended {
                Ok(())
            } else {
                self.suspend_panel_gpu(None)
            };
        }
        let epoch = self.panel_epoch;
        let close_epoch = self.panel_close_epoch;
        let ui_serial = self.panel_ui_serial;
        let scale = self
            .surfaces
            .iter()
            .find(|surface| surface.companion)
            .map_or(1., |surface| surface.scale120 as f32 / 120.);
        if cpu_renderer && scene.motion.as_ref().is_some_and(|motion| motion.reveal) {
            return Ok(());
        }
        if cpu_renderer && accepted_frozen_snapshot(renderer.as_ref(), scene, scale)? {
            if !scene.prepared.is_empty() {
                self.release_panel_prepared();
            }
            return Ok(());
        }
        let scene = scene.clone();
        let (sender, receiver) = calloop::channel::channel();
        let snapshot_started_ns = monotonic_ns();
        bead_core::vprintln!(
            "PANEL_CPU_SNAPSHOT_BEGIN monotonic_ns={} epoch={} close_epoch={} ui_serial={}",
            snapshot_started_ns,
            epoch,
            close_epoch,
            ui_serial
        );
        let worker = std::thread::Builder::new()
            .name("bead-panel-cpu-snapshot".into())
            .spawn(move || {
                let _span = bead_core::profile::Span::new("panel_cpu_snapshot");
                let result = bead_render::tree::TreeCpuRenderer::prepare_snapshot(&scene, scale)
                    .map_err(|error| error.to_string());
                let _ = sender.send(result);
            })?;
        self.panel_snapshot_pending = true;
        let mut worker = PanelSnapshotWorker(Some(worker));
        let mut completed = false;
        if let Err(error) = self.loop_handle.insert_source(receiver, move |event, _, app| {
            match event {
                calloop::channel::Event::Msg(result) => {
                    completed = true;
                    let joined = worker.join().is_some_and(|result| result.is_ok());
                    let snapshot_completed_ns = monotonic_ns();
                    bead_core::vprintln!(
                        "PANEL_CPU_SNAPSHOT_COMPLETE monotonic_ns={} elapsed_ms={:.3} epoch={} close_epoch={}",
                        snapshot_completed_ns,
                        (snapshot_completed_ns.saturating_sub(snapshot_started_ns)) as f64
                            / 1_000_000.0,
                        epoch,
                        close_epoch
                    );
                    app.panel_snapshot_pending = false;
                    if !joined {
                        app.fail("panel CPU snapshot worker panicked");
                        return;
                    }
                    let current_scale = app
                        .surfaces
                        .iter()
                        .find(|surface| surface.companion)
                        .map_or(1., |surface| surface.scale120 as f32 / 120.);
                    let current = !app.panel_target
                        && app.panel_snapshot_ready()
                        && app.panel_runtime.is_some()
                        && app.panel_epoch == epoch
                        && app.panel_close_epoch == close_epoch
                        && app.panel_ui_serial == ui_serial
                        && current_scale.to_bits() == scale.to_bits();
                    if current {
                        match result {
                            Ok(snapshot) => {
                                #[cfg(feature = "native-gl")]
                                let accepted = if app.companion.as_ref().is_some_and(
                                    |(_, _, kind)| *kind == RendererKind::FemtovgGl,
                                ) {
                                    app.suspend_panel_gpu(Some((snapshot, scale)))
                                } else {
                                    app.accept_panel_cpu_snapshot(snapshot, scale)
                                };
                                #[cfg(not(feature = "native-gl"))]
                                let accepted = app.accept_panel_cpu_snapshot(snapshot, scale);
                                if let Err(error) = accepted {
                                    let detail = error.to_string();
                                    if is_cache_pressure(&detail) {
                                        report_cache_pressure("retained-panel", &detail);
                                        let qh = app.qh.clone();
                                        let indices = app
                                            .surfaces
                                            .iter()
                                            .enumerate()
                                            .filter(|(_, surface)| surface.companion)
                                            .map(|(index, _)| index)
                                            .collect::<Vec<_>>();
                                        for index in indices {
                                            app.draw(index, &qh);
                                        }
                                    } else {
                                        eprintln!("UI_ERROR CPU snapshot: {}", bead_core::cache::explain_refusal(&detail));
                                    }
                                }
                            }
                            Err(error) => {
                                if is_cache_pressure(&error) {
                                    report_cache_pressure("retained-panel", &error);
                                    let qh = app.qh.clone();
                                    let indices = app
                                        .surfaces
                                        .iter()
                                        .enumerate()
                                        .filter(|(_, surface)| surface.companion)
                                        .map(|(index, _)| index)
                                        .collect::<Vec<_>>();
                                    for index in indices {
                                        app.draw(index, &qh);
                                    }
                                } else {
                                    eprintln!("UI_ERROR CPU snapshot: {}", bead_core::cache::explain_refusal(&error));
                                }
                            }
                        }
                    } else if app.panel_target {
                        drop(result);
                        let qh = app.qh.clone();
                        let indices = app
                            .surfaces
                            .iter()
                            .enumerate()
                            .filter(|(_, surface)| surface.companion)
                            .map(|(index, _)| index)
                            .collect::<Vec<_>>();
                        for index in indices {
                            app.draw(index, &qh);
                        }
                    } else if !app.panel_target
                        && app.panel_runtime.is_some()
                        && let Err(error) = app.start_panel_snapshot()
                    {
                        app.fail(error);
                    }
                }
                calloop::channel::Event::Closed => {
                    if let Some(error) = panel_snapshot_worker_failure(&mut worker, completed) {
                        app.panel_snapshot_pending = false;
                        app.fail(error);
                    }
                }
            }
        }) {
            self.panel_snapshot_pending = false;
            return Err(error.into());
        }
        Ok(())
    }

    /// `keep_renderer` false: the caller replaces the renderer, so its canvas goes here while
    /// the context is current (B0334).
    #[cfg(feature = "native-gl")]
    fn release_companion_gpu_resources(
        &mut self,
        keep_renderer: bool,
    ) -> Result<(), Box<dyn Error>> {
        if !self
            .companion
            .as_ref()
            .is_some_and(|(_, _, kind)| *kind == RendererKind::FemtovgGl)
        {
            return Ok(());
        }
        if let Some(transport) = &self.gl
            && let Some(surface) = self.surfaces.iter().find(|s| s.companion && s.gl.is_some())
        {
            let scale = surface.scale120 as f32 / 120.;
            transport.activate(
                surface.gl.as_ref().unwrap(),
                (surface.width as f32 * scale).ceil() as u32,
                (surface.height as f32 * scale).ceil() as u32,
            )?;
            if let Some((_, renderer, _)) = &mut self.companion {
                renderer.release_gpu_resources(keep_renderer);
                if !keep_renderer {
                    *renderer = bead_render::renderer(RendererKind::FemtovgGl)?;
                }
            }
            transport.unbind()?;
        }
        Ok(())
    }

    /// B0334: a closed panel's kept GL renderer has no EGL surface left: `f` drops or replaces
    /// it with the context current surfaceless, so its canvas and textures are deleted.
    #[cfg(feature = "native-gl")]
    pub(super) fn with_kept_gl_context(
        &mut self,
        f: impl FnOnce(&mut Self),
    ) -> Result<(), Box<dyn Error>> {
        let bound = keeps_gl_closed(self.companion.as_ref().map(|(_, _, kind)| *kind))
            && !self.surfaces.iter().any(|s| s.companion && s.gl.is_some())
            && self
                .gl
                .as_ref()
                .is_some_and(|gl| gl.bind_surfaceless().is_ok());
        f(self);
        if bound && let Some(gl) = &self.gl {
            gl.unbind()?;
        }
        Ok(())
    }

    /// B0339: the grace ended. The kept renderer goes and the panel takes the CPU snapshot
    /// close path; the transport goes too unless another GL owner or warm mode (its resting
    /// runs and prime) still holds it.
    #[cfg(feature = "native-gl")]
    fn end_kept_gl(&mut self) -> Result<(), Box<dyn Error>> {
        let cpu = bead_render::renderer(RendererKind::Cpu)?;
        self.with_kept_gl_context(|app| {
            if let Some((_, renderer, kind)) = &mut app.companion {
                *renderer = cpu;
                *kind = RendererKind::Cpu;
            }
        })?;
        if crate::wallpaper_fade::transport_releasable(
            self.warm.on,
            self.has_gl_surfaces(),
            self.options.renderer == RendererKind::Cpu,
        ) && let Some(transport) = self.gl.take()
        {
            // SAFETY: no renderer, EGL surface or wallpaper run uses the display; warm mode is off.
            unsafe { transport.shutdown_exclusive() }?;
        }
        // Closed-idle reclamation, as suspend_panel_gpu does after its release.
        #[cfg(all(target_os = "linux", target_env = "gnu"))]
        unsafe {
            libc::malloc_trim(0);
        }
        self.start_panel_snapshot()?;
        println!(
            "PANEL_GPU_SUSPENDED shared_context_retained={} kept_renderer=false snapshot={} reason=grace monotonic_ns={}",
            self.gl.is_some(),
            self.panel_snapshot_pending,
            monotonic_ns()
        );
        Ok(())
    }

    /// Without a snapshot the companion keeps its GL renderer (B0334) for `KEPT_GL_GRACE`
    /// (B0339); either way its GL textures, EGL surface and SHM go.
    #[cfg(feature = "native-gl")]
    pub(super) fn suspend_panel_gpu(
        &mut self,
        snapshot: Option<(bead_render::tree::PreparedCpuSnapshot, f32)>,
    ) -> Result<(), Box<dyn Error>> {
        let _span = bead_core::profile::Span::new("panel_gpu_retire");
        let snapshot_taken = snapshot.is_some();
        // B0338: read before the release, which may replace the renderer.
        let gl_texture_peak = self
            .companion
            .as_mut()
            .map_or(0, |(_, renderer, _)| renderer.take_texture_peak());
        self.release_companion_gpu_resources(!snapshot_taken)?;
        if let Some((snapshot, scale)) = snapshot {
            let snapshot = snapshot.accept()?;
            let frozen = if let Some((scene, _, _)) = self.companion.as_ref() {
                accepted_frozen_snapshot(&snapshot, scene, scale)?
            } else {
                false
            };
            if let Some((_, renderer, kind)) = &mut self.companion {
                *renderer = Box::new(snapshot);
                *kind = RendererKind::Cpu;
                self.panel_cpu_cached = true;
            }
            if frozen {
                self.release_panel_prepared();
            } else if let Some((scene, _, _)) = &mut self.companion {
                // Preserve the legacy reveal/direct fallback handoff. Those frames
                // are not frozen snapshots and must not retire source ownership.
                scene.prepared.clear();
            }
        }
        for surface in self.surfaces.iter_mut().filter(|s| s.companion) {
            let wl_surface = surface.wl_surface().clone();
            drop(surface.gl.take());
            surface.gl_prepared = false;
            surface.gl_prepare_pending = false;
            surface.callbacks.invalidate();
            surface.visual_damage = Default::default();
            surface.buffers.clear();
            surface.pool = SlotPool::new(4096, &self.shm)?;
            surface.previous = None;
            surface.geometry = (0, 0);
            if surface.panel_unmapped || (surface.native.is_none() && !surface.layer.can_attach()) {
                // No buffer is attached, so the compositor holds no GL BO.
                continue;
            }
            // Replace the mapped transparent GL buffer too: otherwise the
            // compositor can retain its BO after our EGL display has been terminated.
            let scale = surface.scale120 as f32 / 120.;
            let (width, height) = if let Some(viewport) = &surface.viewport {
                wl_surface.set_buffer_scale(1);
                viewport.set_destination(surface.width.max(1) as i32, surface.height.max(1) as i32);
                (1, 1)
            } else {
                (
                    (surface.width as f32 * scale).ceil() as i32,
                    (surface.height as f32 * scale).ceil() as i32,
                )
            };
            // A fresh memfd is zero-filled; do not fault transparent pages in.
            let (buffer, _) =
                surface
                    .pool
                    .create_buffer(width, height, width * 4, wl_shm::Format::Argb8888)?;
            buffer.attach_to(&wl_surface)?;
            wl_surface.damage_buffer(0, 0, width, height);
            wl_surface.commit();
            surface.buffers.push(Reusable {
                buffer,
                pending: Vec::new(),
                painted: false,
            });
        }
        if self.options.renderer == RendererKind::Cpu
            && !self.has_gl_surfaces()
            && let Some(transport) = self.gl.take()
        {
            // SAFETY: renderers and all EGL surfaces are gone; the CPU bar owns no EGL objects.
            unsafe { transport.shutdown_exclusive() }?;
        }
        self.options.lazy_gl = true;
        self.companion_policy = Default::default();
        self.panel_gpu_suspended = true;
        #[cfg(all(target_os = "linux", target_env = "gnu"))]
        unsafe {
            libc::malloc_trim(0);
        }
        println!(
            "PANEL_GPU_SUSPENDED shared_context_retained={} kept_renderer={} snapshot={snapshot_taken} gl_texture_peak={gl_texture_peak} monotonic_ns={}",
            self.gl.is_some(),
            keeps_gl_closed(self.companion.as_ref().map(|(_, _, kind)| *kind)),
            monotonic_ns()
        );
        if !snapshot_taken {
            // Guarded like close_panel's snapshot timer: a reopen bumps the close epoch.
            let (epoch, close_epoch) = (self.panel_epoch, self.panel_close_epoch);
            self.loop_handle.insert_source(
                Timer::from_duration(KEPT_GL_GRACE),
                move |_, _, app| {
                    let kept = app.panel_gpu_suspended
                        && keeps_gl_closed(app.companion.as_ref().map(|(_, _, kind)| *kind));
                    let same_close =
                        app.panel_epoch == epoch && app.panel_close_epoch == close_epoch;
                    if kept_gl_grace_due(app.panel_target, same_close, kept)
                        && let Err(error) = app.end_kept_gl()
                    {
                        app.fail(error);
                    }
                    TimeoutAction::Drop
                },
            )?;
        }
        Ok(())
    }

    fn accept_panel_cpu_snapshot(
        &mut self,
        prepared: bead_render::tree::PreparedCpuSnapshot,
        scale: f32,
    ) -> Result<(), Box<dyn Error>> {
        let snapshot = prepared.accept()?;
        let frozen = if let Some((scene, _, kind)) = self.companion.as_ref() {
            *kind == RendererKind::Cpu && accepted_frozen_snapshot(&snapshot, scene, scale)?
        } else {
            false
        };
        if !frozen {
            return Ok(());
        }
        if let Some((_, renderer, _)) = &mut self.companion {
            *renderer = Box::new(snapshot);
        }
        self.release_panel_prepared();
        Ok(())
    }

    fn release_panel_prepared(&mut self) {
        let Some((scene, _, _)) = self.companion.as_mut() else {
            return;
        };
        if let Some(source) = self.panel_runtime.as_mut() {
            source.release_prepared(scene);
        }
        scene.prepared.clear();
    }

    pub(super) fn panel_snapshot_ready(&self) -> bool {
        !self.panel_target
            && self.panel_dock_transition.is_none()
            && !self
                .panel_animations
                .active(self.started.elapsed().as_secs_f64())
            && self
                .surfaces
                .iter()
                .filter(|surface| surface.companion)
                .all(|surface| !surface.layer_motion_active)
    }
    fn trim_idle_shm(&mut self, index: usize) {
        let surface = &mut self.surfaces[index];
        if surface.shm_trim_pending
            || surface.buffers.is_empty()
            || (surface.heap_trimmed_ui_serial == surface.ui_serial
                && surface
                    .buffers
                    .iter()
                    .filter(|buffer| buffer.painted)
                    .count()
                    <= 1)
        {
            return;
        }
        surface.shm_trim_pending = true;
        let identity = surface.wl_surface().clone();
        let qh = self.qh.clone();
        let expires = Instant::now() + SHM_COMPACT_FOCUSED_IDLE + Duration::from_secs(3);
        if let Err(error) = self.loop_handle.insert_source(
            Timer::from_duration(shm_compact_idle(surface)),
            move |_, _, app| {
                // The surface may belong to a pearl that is not swapped in right now.
                app.on_surface(&identity, |app| {
                    let Some(index) = app
                        .surfaces
                        .iter()
                        .position(|s| s.wl_surface() == &identity)
                    else {
                        return TimeoutAction::Drop;
                    };
                    let surface = &mut app.surfaces[index];
                    if surface.callbacks.pending
                        || surface
                            .last_callback
                            .is_none_or(|last| last.elapsed() < shm_compact_idle(surface))
                    {
                        if Instant::now() < expires {
                            let quiet = shm_compact_idle(surface);
                            let wait = surface
                                .last_callback
                                .map_or(quiet, |last| quiet.saturating_sub(last.elapsed()));
                            return TimeoutAction::ToDuration(wait.max(Duration::from_millis(100)));
                        }
                        surface.shm_trim_pending = false;
                        return TimeoutAction::Drop;
                    }
                    surface.shm_trim_pending = false;
                    surface.heap_trimmed_ui_serial = surface.ui_serial;
                    if surface
                        .buffers
                        .iter()
                        .filter(|buffer| buffer.painted)
                        .count()
                        <= 1
                    {
                        #[cfg(all(target_os = "linux", target_env = "gnu"))]
                        unsafe {
                            libc::malloc_trim(0);
                        }
                        return TimeoutAction::Drop;
                    }
                    // An idle burst can have touched several reusable slots. Retire
                    // that pool and repaint one current frame, instead of retaining
                    // spare frame pages indefinitely. Active old buffers remain
                    // compositor-owned until release, as during a geometry change.
                    let pool = match SlotPool::new(4096, &app.shm) {
                        Ok(pool) => pool,
                        Err(error) => {
                            app.fail(error);
                            return TimeoutAction::Drop;
                        }
                    };
                    surface.buffers.clear();
                    surface.pool = pool;
                    surface.previous = None;
                    surface.visual_damage = Default::default();
                    surface.redraw = true;
                    bead_core::vprintln!("SHM_POOL_COMPACT output={}", surface.output_name);
                    app.draw(index, &qh);
                    #[cfg(all(target_os = "linux", target_env = "gnu"))]
                    unsafe {
                        libc::malloc_trim(0);
                    }
                    TimeoutAction::Drop
                })
                .unwrap_or(TimeoutAction::Drop)
            },
        ) {
            self.surfaces[index].shm_trim_pending = false;
            self.fail(error);
        }
    }
    pub(super) fn update_companion_policy(
        &mut self,
        index: usize,
        qh: &QueueHandle<Self>,
    ) -> Result<(), Box<dyn Error>> {
        if !self.surfaces[index].companion {
            return Ok(());
        }
        let desired = self.options.companion.as_ref().map(|(_, kind)| *kind);
        if desired != Some(RendererKind::FemtovgGl) {
            return Ok(());
        }
        #[cfg(feature = "native-gl")]
        if self.panel_target
            && self
                .companion
                .as_ref()
                .is_some_and(|(_, _, kind)| *kind != RendererKind::FemtovgGl)
            && (!self.options.lazy_gl || self.gl.is_some())
            && !self.animations.active(self.started.elapsed().as_secs_f64())
            && !self
                .panel_animations
                .active(self.started.elapsed().as_secs_f64())
        {
            // The warm CPU snapshot is the fallback while a lazy GL context is
            // being prepared. Once GL is ready, promote before relayout/buffer
            // callback gates can defer the first visible panel frame.
            if let Some((_, renderer, kind)) = &mut self.companion {
                *renderer = bead_render::renderer(RendererKind::FemtovgGl)?;
                *kind = RendererKind::FemtovgGl;
            }
            if let Some(source) = self.panel_runtime.as_mut() {
                source.restore_image_policy();
            }
            self.surfaces[index].visual_damage = Default::default();
            println!("RENDER_POLICY companion=true transition=cpu-to-gl");
            return Ok(());
        }
        if !self.options.lazy_gl
            || !self.surfaces[index].configured
            || self.surfaces[index].callbacks.pending
            || self.surfaces[index].scale_relayout_pending
        {
            return Ok(());
        }
        #[cfg(feature = "native-gl")]
        if self.gl_preparation.is_some()
            || (self.options.renderer == RendererKind::Cpu
                && self.gl.is_none()
                && self
                    .options
                    .panel_spec
                    .as_ref()
                    .is_some_and(|s| s.retain_when_closed))
        {
            return Ok(());
        }
        let now = self.started.elapsed().as_secs_f64();
        // Moving a cached surface does not need a cold GPU upgrade. Keep an
        // existing GL renderer warm during prewarm, but let an idle CPU panel
        // hand off and return using its already rendered pixels.
        let animated = self.options.animate
            || (self.panel_dock_transition.is_none()
                && (self.animations.active(now) || self.panel_animations.active(now)));
        let promote = self.companion_policy.frame(now, animated);
        #[cfg(feature = "native-gl")]
        let promote = if self.panel_dock_transition.is_none()
            && self.gl.is_some()
            && self
                .options
                .panel_spec
                .as_ref()
                .is_some_and(|s| s.retain_when_closed)
        {
            // Cold content may use already-prepared GL immediately. A warm open
            // paints its CPU snapshot while the one replacement context starts.
            self.companion
                .as_ref()
                .is_some_and(|(_, _, kind)| *kind != RendererKind::FemtovgGl)
                && !animated
        } else {
            promote
        };
        if promote {
            if let Some((_, renderer, kind)) = &mut self.companion {
                *renderer = bead_render::renderer(RendererKind::FemtovgGl)?;
                *kind = RendererKind::FemtovgGl;
            }
            if let Some(source) = self.panel_runtime.as_mut() {
                source.restore_image_policy();
            }
            self.surfaces[index].visual_damage = Default::default();
            println!("RENDER_POLICY companion=true transition=cpu-to-gl");
            // A retained panel keeps its prepared renderer along with its
            // worker/layer. Replacing that single panel releases all three.
            if !self.release_timer
                && !self
                    .options
                    .panel_spec
                    .as_ref()
                    .is_some_and(|spec| spec.retain_when_closed)
            {
                let qh = qh.clone();
                self.loop_handle.insert_source(Timer::from_duration(Duration::from_secs(2)), move |_, _, app| {
                    if !app.release_timer||app.companion.is_none(){return TimeoutAction::Drop;}
                    let now = app.started.elapsed().as_secs_f64();
                    if app.panel_target
                        && app
                            .surfaces
                            .iter()
                            .any(|surface| surface.companion && surface.pointer_visual.is_some())
                    {
                        app.companion_policy.touch(now);
                        return TimeoutAction::ToDuration(Duration::from_secs_f64(
                            app.companion_policy.remaining(now),
                        ));
                    }
                    if app.surfaces.iter().any(|surface| surface.prepared_native.is_some()) {
                        app.companion_policy.frame(now, true);
                    }
                    if app.companion_policy.expire(now) {
                        #[cfg(feature="native-gl")]
                        if let Err(error) = app.release_companion_gpu_resources(false) {
                            app.fail(error); return TimeoutAction::Drop;
                        }
                        if let Some((_, renderer, kind)) = &mut app.companion {
                            match bead_render::renderer(RendererKind::Cpu) {
                                Ok(cpu) => { *renderer = cpu; *kind = RendererKind::Cpu; }
                                Err(error) => { app.fail(error); return TimeoutAction::Drop; }
                            }
                        }
                        #[cfg(any(feature="native-gl",feature="wgpu"))]
                        {
                            for surface in app.surfaces.iter_mut().filter(|s| s.companion) {
                                #[cfg(feature="native-gl")]
                                {
                                    drop(surface.gl.take());
                                    surface.gl_prepared = false;
                                    surface.gl_prepare_pending = false;
                                }
                                #[cfg(feature="wgpu")]
                            drop(surface.gpu.take());
                                surface.visual_damage = Default::default();
                                surface.callbacks.invalidate();
                            }
                            #[cfg(feature="native-gl")]
                            if !app.has_gl_surfaces() && app.options.renderer == RendererKind::Cpu
                                && let Some(transport) = app.gl.take() {
                                    // SAFETY: the CPU bar/companion own no GL resources;
                                    // this process owns one EGL display and all EGL surfaces are gone.
                                    if let Err(error) = unsafe { transport.shutdown_exclusive() } {
                                        app.fail(error); return TimeoutAction::Drop;
                                    }
                            }
                            #[cfg(feature="wgpu")]
                            if !app.has_gpu_surfaces() { app.gpu = None; }
                        }
                        println!("RENDER_POLICY companion=true transition=gl-to-cpu shared_context_released={}", app.options.renderer == RendererKind::Cpu);
                        app.release_timer = false;
                        let indices: Vec<_> = app.surfaces.iter().enumerate().filter(|(_, s)| s.companion).map(|(i, _)| i).collect();
                        for i in indices { app.draw(i, &qh); }
                        TimeoutAction::Drop
                    } else {
                        TimeoutAction::ToDuration(Duration::from_secs_f64(app.companion_policy.remaining(now)))
                    }
                })?;
                self.release_timer = true;
            }
        } else if self.surfaces[index].frames == 0
            // B0334: a retained panel that closed on GL is still GL: its first frame paints on GL.
            && !keeps_gl_closed(self.companion.as_ref().map(|(_, _, kind)| *kind))
        {
            self.surfaces[index].redraw = true;
            println!("RENDER_POLICY companion=true first_frame=cpu");
        }
        Ok(())
    }

    #[cfg(any(feature = "native-gl", feature = "wgpu"))]
    pub(super) fn ensure_gpu(&mut self, index: usize) -> Result<(), Box<dyn Error>> {
        use raw_window_handle::{
            RawDisplayHandle, RawWindowHandle, WaylandDisplayHandle, WaylandWindowHandle,
        };
        use std::ptr::NonNull;
        let surface = &mut self.surfaces[index];
        let (w, h, _, _) = buffer_dimensions(surface.width, surface.height, surface.scale120)?;
        let renderer_kind = if surface.companion {
            self.companion
                .as_ref()
                .map_or(RendererKind::Cpu, |(_, _, kind)| *kind)
        } else {
            self.options.renderer
        };
        let display = RawDisplayHandle::Wayland(WaylandDisplayHandle::new(
            NonNull::new(self.conn.backend().display_ptr().cast())
                .ok_or("native display missing")?,
        ));
        let window = RawWindowHandle::Wayland(WaylandWindowHandle::new(
            NonNull::new(surface.wl_surface().id().as_ptr().cast())
                .ok_or("native surface missing")?,
        ));
        #[cfg(feature = "native-gl")]
        if renderer_kind == RendererKind::FemtovgGl {
            // The retained connection outlives the EGL context/surfaces.
            let transport = shared_transport(
                &mut self.gl,
                &mut self.gl_preparation,
                &mut self.gl_release,
                display,
            )?;
            if surface.gl.is_none() {
                bead_core::vprintln!(
                    "BUFFER companion={} logical={}x{} physical={}x{} scale={} transport=GL",
                    surface.companion,
                    surface.width,
                    surface.height,
                    w,
                    h,
                    surface.scale120 as f32 / 120.
                );
                // SAFETY: Surface::drop drops EGL before its wl_surface.
                surface.gl = Some(unsafe { transport.create_surface(window, w as u32, h as u32) }?);
            }
            return Ok(());
        }
        #[cfg(feature = "wgpu")]
        {
            if self.gpu.is_none() {
                self.gpu = Some(bead_render::gpu::Transport::new(self.options.memory_usage));
            }
            if surface.gpu.is_none() {
                // SAFETY: Surface::drop drops swapchains before their protocol objects.
                surface.gpu = Some(unsafe {
                    self.gpu.as_mut().unwrap().create_surface(
                        display,
                        window,
                        w as u32,
                        h as u32,
                        renderer_kind,
                    )
                }?);
            }
        }
        Ok(())
    }
    pub(super) fn draw(&mut self, index: usize, qh: &QueueHandle<Self>) {
        let _busy = bead_core::profile::Busy::new(if self.surfaces[index].companion {
            "draw_panel"
        } else {
            "draw"
        });
        if self.hidden {
            return;
        }
        if self.surfaces[index].companion
            && self.surfaces[index].panel_unmapped
            && !self.panel_target
        {
            // Keep the resident scene and renderer warm, but don't remap a closed
            // layer when its provider publishes a background model update.
            return;
        }
        if self.surfaces[index].native.is_none()
            && self.surfaces[index].layer.phase() == layer_role::Phase::Initial
        {
            // An unmapped role commits without a buffer to ask for a fresh
            // configure; the configure redraws.
            self.surfaces[index].layer.commit();
            return;
        }
        if self.surfaces[index].companion && self.panel_gpu_suspended && !self.panel_target {
            return;
        }
        #[cfg(feature = "native-gl")]
        if self.surfaces[index].companion
            && let Err(error) = self.prepare_panel_gl()
        {
            self.fail(error);
            return;
        }
        if self.wallpaper_palette_outputs.is_some() {
            self.surfaces[index].redraw = true;
            return;
        }
        if self.surfaces[index].companion {
            // A worker failure publishes an error scene. A loading worker must never
            // expose the fallback buffer, even when hydration takes longer than a frame.
            if self.panel_resuming || (!self.panel_ready && !self.panel_fallback_ready) {
                self.surfaces[index].redraw = true;
                return;
            }
            #[cfg(feature = "native-gl")]
            if self.gl_preparation.is_some()
                && self.gl.is_none()
                && self
                    .companion
                    .as_ref()
                    .is_some_and(|(_, _, kind)| *kind == RendererKind::FemtovgGl)
            {
                // A cold open can draw its current CPU scene while the retained
                // GL transport warms; only an already-GL renderer must wait for it.
                self.surfaces[index].redraw = true;
                return;
            }
            #[cfg(feature = "native-gl")]
            if self.options.companion.as_ref().map(|(_, kind)| *kind)
                == Some(RendererKind::FemtovgGl)
                && self
                    .companion
                    .as_ref()
                    .is_some_and(|(_, _, kind)| *kind == RendererKind::Cpu)
                && let Err(error) = self.update_companion_policy(index, qh)
            {
                if !self.prepare_surface_images(index, error.as_ref()) {
                    self.fail(error);
                }
                return;
            }
            #[cfg(feature = "native-gl")]
            if self.options.companion.as_ref().map(|(_, kind)| *kind)
                == Some(RendererKind::FemtovgGl)
                && let Some((scene, renderer, kind)) = self.companion.as_ref()
                && *kind == RendererKind::Cpu
                && !self
                    .options
                    .panel_spec
                    .as_ref()
                    .is_some_and(|spec| spec.fallback_factory.is_some())
                && !renderer
                    .reusable_scene(scene, self.surfaces[index].scale120 as f32 / 120.)
                    .unwrap_or(false)
            {
                self.surfaces[index].redraw = true;
                return;
            }
            std::mem::swap(&mut self.animations, &mut self.panel_animations);
            let previous_frames = self.surfaces[index].frames;
            // CPU label repaints may replace a frame while its callback is still
            // pending. Every parent submission must omit the child-owned slider;
            // otherwise the retained pre-drag fill flashes underneath its pose.
            self.pointer_visual_position(index);
            let original = self
                .pointer_visual_scene(index)
                .map(|scene| std::mem::replace(&mut self.companion.as_mut().unwrap().0, scene));
            self.draw_current(index, qh);
            if let Some(scene) = original {
                self.companion.as_mut().unwrap().0 = scene;
            }
            let pending = (self.surfaces[index].frames > previous_frames)
                .then(|| self.pointer_visual_retire(index));
            std::mem::swap(&mut self.animations, &mut self.panel_animations);
            #[cfg(feature = "native-gl")]
            if self.surfaces[index].gl_prepare_pending {
                self.surfaces[index].gl_prepare_pending = false;
                let unprepared: Vec<_> = self
                    .surfaces
                    .iter()
                    .enumerate()
                    .filter(|(_, surface)| {
                        surface.companion && surface.configured && !surface.gl_prepared
                    })
                    .map(|(index, _)| index)
                    .collect();
                if let Some(index) = unprepared.first().copied() {
                    self.draw(index, qh);
                } else {
                    if self
                        .surfaces
                        .iter()
                        .filter(|surface| surface.companion)
                        .all(|surface| surface.frames == 0)
                    {
                        // GL uploads happened without a present; begin reveal only
                        // after the prepared frame can actually be shown.
                        self.panel_content_ready();
                    }
                    let prepared: Vec<_> = self
                        .surfaces
                        .iter()
                        .enumerate()
                        .filter(|(_, surface)| surface.companion && surface.configured)
                        .map(|(index, _)| index)
                        .collect();
                    for index in prepared {
                        self.draw(index, qh);
                    }
                }
            }
            if self.surfaces.get(index).is_some_and(|surface| {
                surface.frames > previous_frames
                    && surface
                        .pointer_visual
                        .as_ref()
                        .is_some_and(pointer_motion::Visual::has_painted_feedback)
            }) {
                // A label repaint can submit the latest child pose too. Flush
                // its matching local value just as the pose-only path does.
                self.pointer_visual_flush(index);
            }
            for event in pending.into_iter().flatten() {
                self.view_event(true, event);
            }
            return;
        }
        if self.options.primary_factory.is_some() && !self.surfaces[index].companion {
            let id = self.surfaces[index].output_name.clone();
            if self.active_output.as_deref() != Some(id.as_str()) {
                if let Some(mut session) = self.output_sessions.remove(&id) {
                    let previous_output = self.active_output.replace(id.clone());
                    let previous_epoch = self.active_output_epoch.replace(session.epoch);
                    std::mem::swap(&mut self.runtime, &mut session.source);
                    std::mem::swap(&mut self.scene, &mut session.scene);
                    std::mem::swap(&mut self.animations, &mut session.animations);
                    let previous_serial = std::mem::replace(&mut self.ui_serial, session.ui_serial);
                    let previous_primary_serial =
                        std::mem::replace(&mut self.primary_ui_serial, session.ui_serial);
                    self.draw(index, qh);
                    session.ui_serial = std::mem::replace(&mut self.ui_serial, previous_serial);
                    self.primary_ui_serial = previous_primary_serial;
                    std::mem::swap(&mut self.animations, &mut session.animations);
                    std::mem::swap(&mut self.scene, &mut session.scene);
                    std::mem::swap(&mut self.runtime, &mut session.source);
                    self.active_output = previous_output;
                    self.active_output_epoch = previous_epoch;
                    self.output_sessions.insert(id, session);
                }
                return;
            }
        }
        self.draw_current(index, qh);
    }
    fn draw_current(&mut self, index: usize, qh: &QueueHandle<Self>) {
        let draw_start = self.started.elapsed().as_secs_f64();
        if (if self.surfaces[index].companion {
            self.panel_runtime.as_ref()
        } else {
            self.runtime.as_ref()
        })
        .is_some_and(|source| source.resources_retired())
        {
            self.surfaces[index].redraw = true;
            return;
        }

        if self.surfaces[index].companion
            && self
                .panel_cover
                .as_ref()
                .is_some_and(|(serial, _)| self.panel_ui_serial < *serial)
        {
            return;
        }
        if self.surfaces[index].native.is_some() && self.surfaces[index].layer_motion_active {
            let layer_surface = self.surfaces[index].layer.wl_surface().clone();
            drop(self.surfaces[index].layer_motion.take());
            self.surfaces[index].layer_motion_plan = None;
            self.surfaces[index].layer_motion_active = false;
            self.surfaces[index].layer_motion_open = None;
            self.surfaces[index].layer_motion_resting_offset = None;
            self.surfaces[index].layer_motion = self
                .layer_motion_manager
                .as_ref()
                .map(|manager| manager.create(&layer_surface, qh));
        }
        if self.surfaces[index].layer_motion_active && self.surfaces[index].native.is_none() {
            let wanted_open = if self.surfaces[index].companion {
                self.panel_target
            } else {
                self.scene.motion.as_ref().is_some_and(|motion| motion.open)
            };
            if self.surfaces[index].layer_motion_open == Some(wanted_open)
                && self.surfaces[index].layer_motion_ui_serial == self.surfaces[index].ui_serial
            {
                // No frame callbacks or source work while the compositor owns the timeline.
                return;
            }
        }
        if self.surfaces[index]
            .handoff
            .as_ref()
            .is_some_and(|h| !h.gate.can_draw())
        {
            return;
        }
        if self.surfaces[index].companion
            && self.panel_dock_transition.is_some()
            && let Some((scene, _, _)) = self.companion.as_ref()
        {
            let output = self.surfaces[index].output.clone();
            self.position_panel_surface(&self.surfaces[index].layer, &output, scene);
        }
        let split_start = bead_core::profile::begin_splits();
        let traced_frame = bead_core::profile::frame_trace()
            .then(|| (monotonic_ns(), bead_core::profile::thread_cpu_ns()));
        let mut damage_px = 0u64;
        let decoration_result = if self.surfaces[index].companion {
            self.sync_all_decorations()
        } else {
            self.sync_decorations(index)
        };
        if let Err(error) = decoration_result {
            self.fail(error);
            return;
        }
        let frame_started = Instant::now();
        bead_core::profile::split("decorations");
        if let Err(error) = self.update_companion_policy(index, qh) {
            if !self.prepare_surface_images(index, error.as_ref()) {
                self.fail(error);
            }
            return;
        }
        if self.surfaces[index].companion {
            let parent = self.surfaces[index].wl_surface().clone();
            self.lip_tab_prepare(&parent);
        }
        let previous_frames = self.surfaces[index].frames;
        let mut busy = false;
        // The motion this frame shows and the clock of its pose; a committed frame starts a held clock.
        let mut motion_clock: Option<(String, f64, f64)> = None;
        let mut layer_motion_opening = None;
        let mut layer_motion_active = false;
        let mut layer_motion_source_scene = None;
        let mut layer_motion_pose = (0., 0.);
        let mut layer_motion_sampled_progress = None;
        let mut layer_motion_pending_input = None;
        let bar_lips = if self.options.bar && !self.surfaces[index].companion {
            self.bar_lip_input(&self.surfaces[index].output)
        } else {
            Vec::new()
        };
        let mut pending_promotion_staged = false;
        // B0388: rasters this frame's main-thread prepare built, for PANEL_SLIDE `main_builds`.
        let mut main_builds = 0;
        let result = (|| -> Result<(), Box<dyn Error>> {
            #[cfg(feature = "native-gl")]
            if self.surfaces[index].companion
                && let Some(error) = self.panel_gl_prewarm_error.take()
            {
                return Err(Box::new(GpuUnavailable(error)));
            }
            let s = &mut self.surfaces[index];
            let command_promotion_pending = should_stage_command_promotion(
                self.promotion_pending,
                s.companion,
                s.configured,
                s.layer_promotion.is_some(),
            );
            let wl_surface = s.wl_surface().clone();
            let compositor_motion_candidate = s.companion
                && !command_promotion_pending
                && s.native.is_none()
                && s.layer_motion.is_some()
                && self.edge.tab.is_none()
                && !bead_core::animation::reduced_motion()
                && !self.options.background_blur
                && self.companion.as_ref().is_some_and(|(scene, _, kind)| {
                    layer_motion_renderer_supported(*kind)
                        && scene.slide.is_none()
                        && scene.motion.as_ref().is_some_and(|motion| {
                            !bead_render::layer_motion::still_motion(motion)
                                && (!self.panel_target
                                    || !motion.reveal
                                    || reveal_contour_capable(
                                        scene,
                                        self.layer_motion_manager.as_ref(),
                                    ))
                        })
                });
            if s.configured
                && !s.callbacks.pending
                && !s.scale_relayout_pending
                && let Some(handoff) = s.handoff.as_ref().filter(|handoff| {
                    !(s.companion && compositor_motion_candidate) && handoff.has_prepared_frame()
                })
            {
                let generation = self
                    .companion
                    .as_ref()
                    .map_or(self.scene.resource_revision, |(scene, _, _)| {
                        scene.resource_revision
                    });
                let source_epoch = if s.companion { self.panel_epoch } else { 0 };
                let (w, h, _, _) = buffer_dimensions(s.width, s.height, s.scale120)?;
                wl_surface.damage_buffer(0, 0, w, h);
                wl_surface.frame(
                    qh,
                    FrameData {
                        surface: wl_surface.clone(),
                        epoch: s.callbacks.request(),
                        generation,
                        source_epoch,
                    },
                );
                s.callbacks.pending = true;
                s.redraw = false;
                s.presented_generation = None;
                if let Some(presentation) = &self.presentation {
                    presentation.feedback(
                        &wl_surface,
                        qh,
                        PresentationData {
                            content: s.companion
                                && self.panel_open.as_ref().is_some_and(|o| o.content),
                            source_epoch,
                            visibility: if s.companion {
                                self.panel_close_epoch
                            } else {
                                0
                            },
                            update: s.ui_serial,
                            surface: wl_surface.clone(),
                            generation,
                        },
                    );
                }
                bead_core::profile::mark("panel_commit");
                if let Some(first_frame) = handoff.first_frame.as_ref() {
                    first_frame.attach_to(&wl_surface)?;
                    wl_surface.commit();
                    if s.companion {
                        s.panel_unmapped = false;
                        // The held first pose (a reveal's zero-area frame) is not drawn by niri, so its
                        // callback only comes on the ~1 s fallback timer and the motion would not start
                        // until then. Draw the next pose a frame later unless a commit got there first (B0294).
                        let (epoch, frames) = (self.panel_epoch, s.frames + 1);
                        let (identity, kick) = (wl_surface.clone(), qh.clone());
                        let _ = self.loop_handle.insert_source(
                            Timer::from_duration(Duration::from_millis(16)),
                            move |_, _, app| {
                                if app.panel_epoch == epoch
                                    && let Some(i) = app
                                        .surfaces
                                        .iter()
                                        .position(|s| s.wl_surface() == &identity)
                                    && app.surfaces[i].frames == frames
                                {
                                    app.surfaces[i].callbacks.invalidate();
                                    app.draw(i, &kick);
                                }
                                TimeoutAction::Drop
                            },
                        );
                    }
                }
                s.last_submitted_ui_serial = s.ui_serial;
                #[cfg(feature = "native-gl")]
                if handoff.first_gl_frame {
                    let transport = self.gl.as_ref().ok_or("missing prepared GL transport")?;
                    let target = s.gl.as_ref().ok_or("missing prepared GL surface")?;
                    transport.activate(target, w as u32, h as u32)?;
                    transport.present(target, &[Damage::full(w as u32, h as u32)])?;
                }
                s.frames += 1;
                bead_core::profile::split("handoff");
                return Ok(());
            }
            let (scene, renderer, _kind, panel) = if s.companion {
                let (scene, renderer, kind) =
                    self.companion.as_mut().ok_or("missing companion scene")?;
                (scene, renderer, *kind, true)
            } else {
                (
                    &mut self.scene,
                    &mut self.renderer,
                    self.options.renderer,
                    self.options.panel,
                )
            };
            if !s.configured {
                return Ok(());
            }
            let configured_size = (s.width, s.height);
            let mut render_size = effective_render_size(
                configured_size,
                s.promotion_state.renders_natural_extent(),
                natural_surface_size(scene),
            );
            let promotion_reset_active = s.layer_motion.is_some()
                && ((s.promotion_reset_motion
                    && (s.layer_motion_active || s.layer_motion_plan.is_some()))
                    || (command_promotion_pending && s.layer_motion_active));
            if s.scale_relayout_pending {
                let deferred = if s.companion {
                    self.panel_runtime.as_ref()
                } else {
                    self.runtime.as_ref()
                }
                .is_some_and(|source| source.deferred_updates());
                let scale = s.scale120 as f32 / 120.;
                if deferred && !scene.prepared.contains_key(&scale.to_bits()) {
                    // Keep the last accepted buffer until the preparation actor
                    // supplies content validated for the new output scale.
                    s.redraw = true;
                    return Ok(());
                }
                s.scale_relayout_pending = false;
            }
            let scale = s.scale120 as f32 / 120.;
            let mut opaque_motion = self.options.animate;
            if let Some(motion) = scene.motion.as_ref() {
                let prior_x = scene.offset.0;
                let now = self.started.elapsed().as_secs_f64();
                let open = if self.options.animate {
                    ((now - 3.).max(0.) % 2.) >= 1. || now < 3.
                } else if s.companion && self.options.panel_spec.is_some() {
                    self.panel_target
                } else {
                    motion.open
                };
                // A panel opens from closed (B0067); the clock starts at the first committed frame (B0170).
                let (duration, curve) = motion.transition(open);
                self.animations.open_to(
                    &motion.id,
                    open,
                    now,
                    if self.options.animate && open {
                        1.
                    } else {
                        duration
                    },
                    curve,
                );
                let (progress, active) = self.animations.value(&motion.id, now).unwrap();
                layer_motion_opening = Some(open);
                layer_motion_active = active;
                layer_motion_sampled_progress = Some(progress);
                motion_clock = Some((
                    motion.id.clone(),
                    now,
                    if self.options.animate && open {
                        1.
                    } else {
                        duration
                    },
                ));
                scene.opacity = motion.opacity(progress);
                let (reveal, edge) = (motion.reveal, motion.edge);
                // shortcut: `motion.scale_from` is ignored on this client-frame path (fade and slide only), upgrade
                // when a panel needs scale without the layer-motion protocol.
                let (dx, dy) = scene.panel_offset(progress, open);
                layer_motion_pose = (dx, dy);
                opaque_motion = active && (dx != 0. || dy != 0. || reveal);
                // A vertical exit retains the open x origin instead of
                // re-centering from the live surface width every frame.
                scene.offset = motion_scene_offset(
                    prior_x,
                    edge,
                    open,
                    (dx, dy),
                    (-scene.bounds.x, -scene.bounds.y),
                );
                // The lip rides as the panel's tab: posed with this frame's body, applied by this commit.
                if panel
                    && self.edge.tab.is_some()
                    && let Some(body) = scene.posed_body()
                {
                    let state = s.layer.state();
                    let output = self
                        .output
                        .info(&s.output)
                        .and_then(|info| info.logical_size)
                        .unwrap_or((0, 0));
                    let origin = crate::edges::surface_origin(
                        state.anchor,
                        state.margin,
                        (s.width, s.height),
                        output,
                    );
                    self.edge.pose_tab(
                        body,
                        progress,
                        origin,
                        (dx, dy),
                        self.panel_target,
                        s.scale120,
                        self.options.edge_art.as_ref(),
                    );
                }
            }

            if !panel {
                scene.bounds.w = render_size.0 as f32;
                scene.bounds.h = render_size.1 as f32;
                scene.width = render_size.0 as f32;
            }
            if panel {
                if self
                    .options
                    .panel_spec
                    .as_ref()
                    .is_none_or(|spec| spec.position.is_none())
                    && scene.motion.as_ref().is_none_or(|m| {
                        m.open && matches!(m.edge, bead_core::Edge::Top | bead_core::Edge::Bottom)
                    })
                {
                    // Custom positioners already align the body through the padded
                    // surface origin, even when the compositor clips that surface.
                    scene.offset.0 = ((render_size.0 as f32 - scene.width) / 2.).max(0.);
                }
                if let Some(slide) = scene.slide {
                    let progress = if self.options.animate {
                        let elapsed = self.started.elapsed().as_secs_f32();
                        if self.options.start_open && (elapsed < 3. || !s.ready) {
                            1.
                        } else {
                            let t = ((elapsed - if self.options.start_open { 3. } else { 0. })
                                % slide.period_seconds)
                                / (slide.period_seconds / 2.);
                            let progress = if t <= 1. { t } else { 2. - t };
                            if self.options.start_open {
                                1. - progress
                            } else {
                                progress
                            }
                        }
                    } else {
                        1.
                    };
                    let eased = progress * progress * (3. - 2. * progress);
                    scene.opacity = eased;
                    let y = slide.closed_y + (slide.open_y - slide.closed_y) * eased;
                    scene.offset.1 = (y * scale).round() / scale;
                }
            }
            if let Some(origin) = s.promoted_body_origin {
                stabilize_body_origin(scene, origin);
            }
            let promotion_active = s.promotion_state.renders_natural_extent();
            render_size = effective_render_size(
                configured_size,
                promotion_active,
                natural_surface_size(scene),
            );
            let resize_promoted_root =
                promotion_active && s.promoted_size.unwrap_or(configured_size) != render_size;
            let promoted_body_size = promotion_active
                .then(|| visible_promoted_body_size(scene, render_size))
                .flatten();
            let promoted_body_resize =
                promoted_body_size.is_some_and(|size| s.promoted_body_size != Some(size));
            let pending_promotion_body = if command_promotion_pending {
                content_input_rect(scene)
                    .and_then(|body| clip_body_rect_to_surface(body, render_size))
            } else {
                None
            };
            if command_promotion_pending && pending_promotion_body.is_none() {
                s.redraw = true;
                if !s.callbacks.pending {
                    let epoch = s.callbacks.request();
                    wl_surface.frame(
                        qh,
                        FrameData {
                            surface: wl_surface.clone(),
                            epoch,
                            generation: scene.resource_revision,
                            source_epoch: self.panel_epoch,
                        },
                    );
                    s.callbacks.pending = true;
                    wl_surface.commit();
                }
                return Ok(());
            }
            s.background_visible = scene.opacity > 0.001;
            let background_requested = self.options.background_blur && s.background_visible;
            background::update(
                &self.background,
                &self.compositor,
                s,
                qh,
                background_requested,
                render_size,
            );
            let (w, h, stride, size) = buffer_dimensions(render_size.0, render_size.1, s.scale120)?;
            let format = if cfg!(target_endian = "little")
                && !self.options.force_argb
                && self.shm.formats().contains(&wl_shm::Format::Abgr8888)
            {
                PixelFormat::Rgba
            } else {
                PixelFormat::Bgra
            };
            let replace_pending_ui = _kind == RendererKind::Cpu
                && s.callbacks
                    .can_replace_pending_ui(s.ui_serial != s.last_submitted_ui_serial)
                && s.geometry == (w, h)
                && s.format == format
                && s.buffers
                    .iter()
                    .any(|b| !b.buffer.slot().has_active_buffers());
            // A compositor-owned motion (atlas) is drawn instead of this surface, so niri may not
            // send its frame callback for ~1 s: that callback must not gate the first pose or a
            // retarget (B0294).
            if draw_waits_for_callback(
                s.callbacks.pending,
                _kind != RendererKind::Cpu,
                compositor_motion_candidate && layer_motion_active,
            ) {
                s.redraw = true;
                return Ok(());
            }
            // A callback can arrive after the motion expires. Keep it armed
            // until we have drawn the terminal pose, rather than making the
            // next input finish the motion and invalidate the whole surface.
            s.redraw = layer_motion_active;
            if self.options.debug_progress
                && let Some(bead_render::Primitive::Rounded { rect, .. }) =
                    scene.primitives.last_mut()
            {
                let phase = self.started.elapsed().as_secs_f32() % 2.;
                rect.w = 1. + 299. * if phase < 1. { phase } else { 2. - phase };
            }
            let layer_motion_target_changed = s.layer_motion_active
                && layer_motion_opening.is_some_and(|opening| s.layer_motion_open != Some(opening));
            let layer_motion_cpu_reset_pending = layer_motion_cpu_replacement_pending(
                s.layer_motion_active,
                s.layer_motion
                    .as_ref()
                    .is_some_and(|motion| motion.has_active_serial()),
                layer_motion_target_changed,
                s.layer_motion_plan.is_some(),
            );
            let layer_motion_cpu_replacement = layer_motion_cpu_reset_pending
                || promotion_reset_active
                || scene.motion.as_ref().is_some_and(|motion| {
                    layer_motion_opening.is_some_and(|opening| {
                        layer_motion_reveal_reversal_uses_cpu(
                            opening,
                            motion.reveal,
                            layer_motion_target_changed,
                            s.layer_motion_active,
                            reveal_contour_capable(scene, self.layer_motion_manager.as_ref()),
                        )
                    })
                });
            let resting_offset = (
                scene.offset.0 - layer_motion_pose.0,
                scene.offset.1 - layer_motion_pose.1,
            );
            let layer_motion_transport_eligible = layer_motion_renderer_supported(_kind)
                && s.native.is_none()
                && !s.layer_promoted
                && !command_promotion_pending
                && s.layer_motion.is_some()
                && self.edge.tab.is_none()
                && layer_motion_active
                && !bead_core::animation::reduced_motion()
                && !self.options.background_blur
                && scene.slide.is_none()
                // B0334: a still motion (a client-frame storyboard) never takes the atlas path.
                && scene
                    .motion
                    .as_ref()
                    .is_some_and(|motion| !bead_render::layer_motion::still_motion(motion));
            let layer_motion_handoff_ok = !layer_motion_cpu_replacement
                && layer_motion_opening.is_some_and(|opening| {
                    layer_motion_handoff_eligible(
                        opening,
                        layer_motion_sampled_progress,
                        s.layer_motion_active,
                    )
                });
            let layer_motion_pose_eligible = layer_motion_handoff_ok
                && layer_motion_opening.is_some()
                && scene.motion.as_ref().is_some_and(|motion| {
                    !motion.reveal
                        || reveal_contour_capable(scene, self.layer_motion_manager.as_ref())
                });
            let layer_motion_reuses_atlas =
                layer_motion_target_changed && s.layer_motion_plan.is_some();
            let layer_motion_ink_safe = layer_motion_transport_eligible
                && layer_motion_pose_eligible
                && !layer_motion_reuses_atlas
                && layer_motion_opening.is_some()
                && bead_render::layer_motion::motion_atlas_ink_is_safe(
                    scene,
                    render_size.0,
                    render_size.1,
                    scale,
                    resting_offset,
                );
            let layer_motion_candidate = layer_motion_transport_eligible
                && layer_motion_pose_eligible
                && (layer_motion_reuses_atlas || layer_motion_ink_safe);
            let layer_motion_needs_atlas = !s.layer_motion_active
                || layer_motion_target_changed
                || s.layer_motion_ui_serial != s.ui_serial;
            if layer_motion_candidate
                && layer_motion_needs_atlas
                && let Some(opening) = layer_motion_opening
            {
                let mut source_scene = scene.clone();
                source_scene.offset = resting_offset;
                source_scene.opacity = 1.;
                source_scene.panel_offset(1., opening);
                layer_motion_source_scene = Some(source_scene);
            }
            if panel
                && self.options.interactive
                && (!s.companion || self.options.panel_spec.is_some())
                && !(s.layer_motion_active && !layer_motion_target_changed)
            {
                let spec = self.options.panel_spec.as_ref();
                let passive = s.companion && spec.is_some_and(|spec| spec.passive);
                let text_focus = s.companion
                    && self
                        .panel_runtime
                        .as_ref()
                        .is_some_and(|source| source.surrounding_text().is_some());
                let keyboard_focus = text_focus
                    || !s.companion
                    || spec.is_some_and(|spec| spec.keyboard_focus && !spec.text_focus_only);
                let visible = if layer_motion_candidate {
                    layer_motion_opening.unwrap_or(false)
                        && if s.companion && self.options.panel_spec.is_some() {
                            self.panel_target
                        } else {
                            true
                        }
                } else {
                    scene.opacity > 0.001
                        && if s.companion && self.options.panel_spec.is_some() {
                            self.panel_target
                        } else {
                            scene.motion.as_ref().is_none_or(|m| m.open)
                        }
                };
                let input_scene = layer_motion_source_scene.as_ref().unwrap_or(scene);
                let input_rect = if visible && s.companion && !passive && !self.options.test_passive
                {
                    content_input_rect(input_scene)
                } else {
                    None
                };
                let input_enabled = visible && !self.options.test_passive && !passive;
                if !layer_motion_cpu_replacement
                    && (s.input_visible != Some(visible) || s.input_rect != input_rect)
                {
                    if visible && !self.options.test_passive && !passive {
                        if let Some([x, y, w, h]) = input_rect {
                            let region = Region::new(&self.compositor)?;
                            if w > 0 && h > 0 {
                                region.add(x, y, w, h);
                            }
                            wl_surface.set_input_region(Some(region.wl_region()));
                        } else {
                            wl_surface.set_input_region(None);
                        }
                    } else {
                        let empty = Region::new(&self.compositor)?;
                        wl_surface.set_input_region(Some(empty.wl_region()));
                    }
                    s.input_visible = Some(visible);
                    s.input_rect = input_rect;
                    bead_core::vprintln!("PANEL_INPUT visible={visible}");
                }
                let focus_visible = visible
                    && !self.options.test_passive
                    && !passive
                    && if s.companion {
                        keyboard_focus
                    } else {
                        self.options.keyboard_focus || self.options.test_keyboard
                    };
                let interactivity = panel_keyboard_mode(
                    focus_visible,
                    s.companion
                        && (self.pearl.is_some()
                            || self
                                .panel_runtime
                                .as_ref()
                                .is_some_and(|source| !source.outside_dismiss())),
                    self.options.test_keyboard || s.companion,
                );
                if layer_motion_cpu_replacement {
                    layer_motion_pending_input =
                        Some((visible, input_rect, interactivity, input_enabled));
                } else if s.keyboard_visible != Some(interactivity) {
                    s.layer.set_keyboard_interactivity(interactivity);
                    s.keyboard_visible = Some(interactivity);
                }
            }
            // `input = "children"` (B0321): the bar takes input on its islands and hosted lips only.
            if !panel && !s.companion && self.options.interactive {
                match island_input_action(
                    scene.input_islands.as_deref(),
                    scene.offset,
                    &bar_lips,
                    s.input_islands.as_deref(),
                ) {
                    IslandInput::Keep => {}
                    IslandInput::Set(rects) => {
                        let region = Region::new(&self.compositor)?;
                        for [x, y, w, h] in &rects {
                            region.add(*x, *y, *w, *h);
                        }
                        wl_surface.set_input_region(Some(region.wl_region()));
                        s.input_islands = Some(rects);
                    }
                    // The prop went away (a live toggle): back to the default whole-surface input.
                    IslandInput::Reset => {
                        wl_surface.set_input_region(None);
                        s.input_islands = None;
                    }
                }
            }
            let input_content_offset = scene
                .motion
                .as_ref()
                .filter(|motion| motion.reveal)
                .and_then(|motion| {
                    motion.slide_distance.map(|distance| {
                        motion
                            .edge
                            .closed_offset(distance, distance, motion.progress)
                    })
                })
                .unwrap_or((0., 0.));
            if !layer_motion_cpu_replacement {
                s.offset = scene.offset;
                s.input_content_offset = input_content_offset;
            }
            if layer_motion_active
                && let Some(opening) = layer_motion_opening
                && s.layer_motion_active
                && s.layer_motion_open != Some(opening)
                && !scene.motion.as_ref().is_some_and(|motion| motion.reveal)
                && !layer_motion_cpu_replacement
                && let Some(plan) = s.layer_motion_plan.as_mut()
            {
                let (duration, curve) = scene
                    .motion
                    .as_ref()
                    .expect("active layer motion has a scene motion")
                    .transition(opening);
                for plane in &mut plan.planes {
                    std::mem::swap(&mut plane.from, &mut plane.to);
                }
                plan.duration = Duration::from_secs_f64(duration);
                plan.curve = curve;
                plan.retarget = true;
                if s.layer_motion.as_mut().unwrap().queue(plan).is_none() {
                    return Ok(());
                }
                let content_changed = s.layer_motion_ui_serial != s.ui_serial;
                if content_changed && let Some(epoch) = s.callbacks.request_if_idle() {
                    wl_surface.frame(
                        qh,
                        FrameData {
                            surface: wl_surface.clone(),
                            epoch,
                            generation: scene.resource_revision,
                            source_epoch: if s.companion { self.panel_epoch } else { 0 },
                        },
                    );
                }
                // The attached atlas remains valid, including for a reverse into
                // a rounded reveal route. The server samples the current group.
                wl_surface.commit();
                s.layer_motion_open = Some(opening);
                if let Some(offset) = s.layer_motion_resting_offset {
                    s.offset = offset;
                }
                s.input_content_offset = (0., 0.);
                if !content_changed {
                    s.layer_motion_ui_serial = s.ui_serial;
                    s.last_submitted_ui_serial = s.ui_serial;
                }
                s.redraw = false;
                s.frames += 1;
                bead_core::profile::split("layer_motion");
                return Ok(());
            }
            if layer_motion_active
                && let Some(opening) = layer_motion_opening
                && let Some(mut source_scene) = layer_motion_source_scene
                && s.native.is_none()
                && s.layer_motion.is_some()
                && self.layer_motion_manager.as_ref().is_some_and(|manager| {
                    scene
                        .motion
                        .as_ref()
                        .is_some_and(|motion| manager.supports_curves(motion))
                })
                && (!s.layer_motion_active
                    || s.layer_motion_open != Some(opening)
                    || s.layer_motion_ui_serial != s.ui_serial)
            {
                source_scene.offset = (
                    scene.offset.0 - layer_motion_pose.0,
                    scene.offset.1 - layer_motion_pose.1,
                );
                // Panel presentation opacity starts from its complete resting
                // pose; the compositor applies the configured opacity mode.
                source_scene.opacity = 1.;
                // Protocol atlases are SHM buffers even when the steady-state
                // renderer draws directly into an EGL surface. Use Bead's
                // deterministic CPU tree renderer for this one-time snapshot;
                // the active timeline itself remains compositor-driven.
                let mut atlas_renderer = bead_render::tree::TreeCpuRenderer::default();
                let atlas_renderer: &mut dyn bead_render::Renderer = if _kind == RendererKind::Cpu {
                    renderer.as_mut()
                } else {
                    &mut atlas_renderer
                };
                if let Some(mut plan) = bead_render::layer_motion::LayerMotionPlan::for_slide(
                    &source_scene,
                    opening,
                    render_size.0,
                    render_size.1,
                    scale,
                    format,
                    atlas_renderer,
                )? {
                    let changed_target =
                        s.layer_motion_active && s.layer_motion_open != Some(opening);
                    plan.retarget = changed_target;
                    let motion = s.layer_motion.as_mut().unwrap();
                    if motion.needs_recreation(&plan) {
                        if s.layer_motion_active {
                            // A source layout cannot change under an active animation.
                            // Let its current target finish, then recreate on the next draw.
                            if let Some(offset) = s.layer_motion_resting_offset {
                                s.offset = offset;
                            }
                            s.input_content_offset = (0., 0.);
                            s.redraw = false;
                            return Ok(());
                        }
                        drop(s.layer_motion.take());
                        s.layer_motion = self
                            .layer_motion_manager
                            .as_ref()
                            .map(|manager| manager.create(&wl_surface, qh));
                    }
                    let submit_animation = !s.layer_motion_active || changed_target;
                    if submit_animation && s.layer_motion.as_mut().unwrap().queue(&plan).is_none() {
                        return Ok(());
                    }
                    s.layer_motion_resting_offset = Some(source_scene.offset);
                    s.offset = source_scene.offset;
                    s.input_content_offset = (0., 0.);
                    let pixels = plan.atlas.take_pixels();
                    let buffer_format = if plan.atlas.format == PixelFormat::Rgba {
                        wl_shm::Format::Abgr8888
                    } else {
                        wl_shm::Format::Argb8888
                    };
                    let atlas_geometry = (plan.atlas.width as i32, plan.atlas.height as i32);
                    if s.geometry != atlas_geometry || s.format != plan.atlas.format {
                        s.buffers.clear();
                        let pool_bytes = (plan.atlas.stride as usize)
                            .checked_mul(plan.atlas.height as usize)
                            .ok_or("layer motion buffer size overflow")?;
                        s.pool = SlotPool::new(pool_bytes.next_multiple_of(64) * 3, &self.shm)?;
                        s.geometry = atlas_geometry;
                        s.format = plan.atlas.format;
                    }
                    let reusable = s.buffers.iter().position(|buffer| {
                        !buffer.buffer.slot().has_active_buffers()
                            && buffer.buffer.height() == plan.atlas.height as i32
                            && buffer.buffer.stride() == plan.atlas.stride as i32
                    });
                    let slot = if let Some(slot) = reusable {
                        let target = s.buffers[slot]
                            .buffer
                            .canvas(&mut s.pool)
                            .ok_or("layer motion buffer is still compositor-owned")?;
                        if !copy_layer_motion_pixels(target, &pixels) {
                            return Err(
                                "reusable layer motion slot is smaller than its atlas".into()
                            );
                        }
                        s.buffers[slot].pending.clear();
                        s.buffers[slot].painted = true;
                        slot
                    } else {
                        let (buffer, bytes) = s.pool.create_buffer(
                            plan.atlas.width as i32,
                            plan.atlas.height as i32,
                            plan.atlas.stride as i32,
                            buffer_format,
                        )?;
                        if !copy_layer_motion_pixels(bytes, &pixels) {
                            return Err("new layer motion slot is smaller than its atlas".into());
                        }
                        s.buffers.push(Reusable {
                            buffer,
                            pending: Vec::new(),
                            painted: true,
                        });
                        s.buffers.len() - 1
                    };
                    if let Some(viewport) = &s.viewport {
                        wl_surface.set_buffer_scale(1);
                        viewport.set_destination(render_size.0 as i32, render_size.1 as i32);
                    } else {
                        wl_surface.set_buffer_scale((s.scale120 / 120) as i32);
                    }
                    s.buffers[slot].buffer.attach_to(&wl_surface)?;
                    let terminal_buffer = if let Some(terminal) = plan.terminal.as_mut() {
                        let pixels = terminal.take_pixels();
                        let reusable = s.buffers.iter().enumerate().find_map(|(index, buffer)| {
                            (index != slot
                                && !buffer.buffer.slot().has_active_buffers()
                                && buffer.buffer.height() == terminal.height as i32
                                && buffer.buffer.stride() == terminal.stride as i32)
                                .then_some(index)
                        });
                        let terminal_slot = if let Some(terminal_slot) = reusable {
                            let target = s.buffers[terminal_slot]
                                .buffer
                                .canvas(&mut s.pool)
                                .ok_or("terminal layer motion buffer is still compositor-owned")?;
                            if !copy_layer_motion_pixels(target, &pixels) {
                                return Err(
                                    "reusable terminal layer motion slot is too small".into()
                                );
                            }
                            s.buffers[terminal_slot].buffer.activate()?;
                            terminal_slot
                        } else {
                            let buffer_format = if terminal.format == PixelFormat::Rgba {
                                wl_shm::Format::Abgr8888
                            } else {
                                wl_shm::Format::Argb8888
                            };
                            let (buffer, bytes) = s.pool.create_buffer(
                                terminal.width as i32,
                                terminal.height as i32,
                                terminal.stride as i32,
                                buffer_format,
                            )?;
                            if !copy_layer_motion_pixels(bytes, &pixels) {
                                return Err("new terminal layer motion slot is too small".into());
                            }
                            buffer.activate()?;
                            s.buffers.push(Reusable {
                                buffer,
                                pending: Vec::new(),
                                painted: true,
                            });
                            s.buffers.len() - 1
                        };
                        Some(s.buffers[terminal_slot].buffer.wl_buffer().clone())
                    } else {
                        None
                    };
                    s.layer_motion
                        .as_mut()
                        .unwrap()
                        .queue_terminal_buffer(terminal_buffer.as_ref(), render_size);
                    wl_surface.damage_buffer(
                        0,
                        0,
                        plan.atlas.width as i32,
                        plan.atlas.height as i32,
                    );
                    if let Some(epoch) = s.callbacks.request_if_idle() {
                        wl_surface.frame(
                            qh,
                            FrameData {
                                surface: wl_surface.clone(),
                                epoch,
                                generation: source_scene.resource_revision,
                                source_epoch: if s.companion { self.panel_epoch } else { 0 },
                            },
                        );
                    }
                    if let Some(visual) = &mut s.pointer_visual {
                        visual.stage_commit()?;
                    }
                    if let Some(presentation) = &self.presentation {
                        if let Some(visual) = &s.pointer_visual {
                            visual.feedback(presentation, &wl_surface, qh);
                        }
                        presentation.feedback(
                            &wl_surface,
                            qh,
                            PresentationData {
                                content: s.companion
                                    && self.panel_open.as_ref().is_some_and(|o| o.content),
                                source_epoch: if s.companion { self.panel_epoch } else { 0 },
                                visibility: if s.companion {
                                    self.panel_close_epoch
                                } else {
                                    0
                                },
                                update: s.ui_serial,
                                surface: wl_surface.clone(),
                                generation: source_scene.resource_revision,
                            },
                        );
                    }
                    wl_surface.commit();
                    s.layer_motion_active = true;
                    s.layer_motion_open = Some(opening);
                    s.layer_motion_ui_serial = s.ui_serial;
                    s.layer_motion_plan = Some(plan);
                    s.last_submitted_ui_serial = s.ui_serial;
                    s.panel_unmapped = false;
                    s.redraw = false;
                    s.frames += 1;
                    bead_core::profile::split("layer_motion");
                    return Ok(());
                }
            }
            if s.layer_motion_active && s.native.is_none() && !layer_motion_cpu_replacement {
                // The attached atlas remains the visible content until the
                // compositor retires its animation, including while a changed
                // or unsupported scene waits for the completion handoff.
                if let Some(offset) = s.layer_motion_resting_offset {
                    s.offset = offset;
                }
                s.input_content_offset = (0., 0.);
                s.redraw = false;
                return Ok(());
            }
            if layer_motion_opening == Some(true)
                && !layer_motion_active
                && layer_motion_sampled_progress == Some(1.)
                && !s.layer_motion_active
                && scene.opacity == 1.
                && scene.slide.is_none()
                && scene
                    .motion
                    .as_ref()
                    .is_some_and(|motion| motion.open && !motion.reveal)
            {
                renderer.resume_prepared(scene, scale)?;
            }
            renderer.presentation_motion(opaque_motion);
            bead_core::profile::split("pose");
            {
                let _prepare = bead_core::profile::Span::new("present_prepare");
                let builds = renderer.raster_builds();
                renderer.prepare_on_ui(scene, scale)?;
                main_builds += renderer.raster_builds().saturating_sub(builds);
            }
            bead_core::profile::split("prepare");
            let mut regions = {
                let _damage = bead_core::profile::Span::new("visual_damage");
                if self.options.damage {
                    if let Some(frame) = renderer.prepared_frame() {
                        s.visual_damage.update(
                            &frame,
                            scene.offset,
                            scene.opacity,
                            w as u32,
                            h as u32,
                        )?
                    } else if _kind == RendererKind::Cpu {
                        // A frozen CPU snapshot (a reopened panel): only its group moves.
                        s.visual_damage.update_group(
                            scene.bounds,
                            scene.offset,
                            scale,
                            w as u32,
                            h as u32,
                        )
                    } else {
                        vec![Damage::full(w as u32, h as u32)]
                    }
                } else {
                    vec![Damage::full(w as u32, h as u32)]
                }
            };
            if resize_promoted_root || command_promotion_pending {
                regions = vec![Damage::full(w as u32, h as u32)];
            }
            bead_core::profile::split("damage");
            damage_px = regions
                .iter()
                .map(|r| u64::from(r.w) * u64::from(r.h))
                .sum();
            // A replacement is only useful when newer UI content changes pixels.
            // In particular, do not turn animation ticks into extra commits while
            // the original frame callback is outstanding.
            if replace_pending_ui && regions.is_empty() {
                s.redraw = true;
                return Ok(());
            }
            // An accepted generation can be pixel-identical (including edits to
            // filtered rows). Request actual compositor feedback for that new
            // transaction too; a callback-only commit has no presentation event.
            if regions.is_empty() && s.presented_generation != Some(s.ui_serial) {
                let output = self.output.info(&s.output).and_then(|i| i.logical_size);
                let output = output.unwrap_or((0, 0));
                let origin = s.layer.state().origin(render_size, output);
                regions.push(Damage {
                    x: visible_feedback_point(origin.0, render_size.0, output.0, w),
                    y: visible_feedback_point(origin.1, render_size.1, output.1, h),
                    w: 1,
                    h: 1,
                });
            }
            if self.options.trace_state || self.options.debug_progress {
                let pixels: u64 = regions
                    .iter()
                    .map(|r| u64::from(r.w) * u64::from(r.h))
                    .sum();
                bead_core::vprintln!(
                    "DAMAGE rectangles={} pixels={} share={:.6}",
                    regions.len(),
                    pixels,
                    pixels as f64 / (f64::from(w) * f64::from(h))
                );
            }
            if let Some(viewport) = &s.viewport {
                wl_surface.set_buffer_scale(1);
                viewport.set_destination(render_size.0 as i32, render_size.1 as i32);
            } else {
                wl_surface.set_buffer_scale((s.scale120 / 120) as i32);
            }
            #[cfg(feature = "native-gl")]
            if _kind == RendererKind::FemtovgGl {
                use raw_window_handle::{
                    RawDisplayHandle, RawWindowHandle, WaylandDisplayHandle, WaylandWindowHandle,
                };
                use std::ptr::NonNull;
                let display = RawDisplayHandle::Wayland(WaylandDisplayHandle::new(
                    NonNull::new(self.conn.backend().display_ptr().cast())
                        .ok_or("native display missing")?,
                ));
                let window = RawWindowHandle::Wayland(WaylandWindowHandle::new(
                    NonNull::new(wl_surface.id().as_ptr().cast())
                        .ok_or("native surface missing")?,
                ));
                // The connection outlives Transport and surfaces.
                let transport = shared_transport(
                    &mut self.gl,
                    &mut self.gl_preparation,
                    &mut self.gl_release,
                    display,
                )
                .map_err(|e| GpuUnavailable(e.to_string()))?;
                if s.gl.is_none() {
                    // SAFETY: EGL surface is dropped before the retained wl_surface.
                    s.gl = Some(
                        unsafe { transport.create_surface(window, w as u32, h as u32) }
                            .map_err(|e| GpuUnavailable(e.to_string()))?,
                    );
                    bead_core::vprintln!(
                        "BUFFER output={} logical={}x{} physical={}x{} scale={scale} transport=GL",
                        s.output.id().protocol_id(),
                        render_size.0,
                        render_size.1,
                        w,
                        h
                    );
                }
                // Paint the target backbuffer without swapping it. It shares
                // the existing context, renderer and raster cache; only release
                // presents this frame together with its final surface offset.
                if s.native.is_none()
                    && let Some(prepared) = s.prepared_native.as_mut()
                    && prepared.configured
                    && prepared.scale120 == s.scale120
                {
                    let (native_w, native_h) = native_gl_dimensions(scene, s.scale120)?;
                    let key = policy::PreparedFrameKey {
                        generation: prepared.size_generation,
                        ui_serial: s.ui_serial,
                        size: (s.width, s.height),
                        scale120: s.scale120,
                    };
                    if !prepared.gl_ready
                        || prepared.snapshot_key() != Some(key)
                        || prepared.gl_size != Some((native_w, native_h))
                        || !regions.is_empty()
                    {
                        prepared.gl_ready = false;
                        if prepared.gl_size != Some((native_w, native_h)) {
                            // EGL retains its old backbuffer until a swap. Recreate
                            // on a genuine natural-extent change instead of resizing
                            // an unswapped surface and presenting stale dimensions.
                            drop(prepared.gl.take());
                            prepared.gl_size = None;
                        }
                        if prepared.gl.is_none() {
                            use raw_window_handle::{RawWindowHandle, WaylandWindowHandle};
                            let window =
                                prepared.window.as_ref().ok_or("missing prewarm window")?;
                            let handle = RawWindowHandle::Wayland(WaylandWindowHandle::new(
                                std::ptr::NonNull::new(window.wl_surface().id().as_ptr().cast())
                                    .ok_or("missing prewarm surface")?,
                            ));
                            // SAFETY: PreparedNative drops EGL before its window.
                            prepared.gl = Some(unsafe {
                                transport.create_surface(handle, native_w, native_h)
                            }?);
                            prepared.gl_size = Some((native_w, native_h));
                        }
                        transport.activate(prepared.gl.as_ref().unwrap(), native_w, native_h)?;
                        renderer.draw(
                            scene,
                            RenderTarget::Gl(bead_render::gl::GlTarget {
                                display: &transport.display,
                                width: native_w,
                                height: native_h,
                                context: transport.generation,
                                warm: &transport.warm,
                            }),
                            Damage::full(native_w, native_h),
                        )?;
                        prepared.snapshot = None;
                        prepared.snapshot_size_generation = key.generation;
                        prepared.snapshot_ui_serial = key.ui_serial;
                        prepared.snapshot_size = key.size;
                        prepared.snapshot_scale120 = key.scale120;
                        prepared.gl_ready = true;
                    }
                }
                let retained_gl_prepare = s.companion
                    && !s.gl_prepared
                    && self
                        .options
                        .panel_spec
                        .as_ref()
                        .is_some_and(|spec| spec.retain_when_closed);
                // B0334: a visible panel uploads and presents its first GL frame in one draw.
                let first_gl_upload = retained_gl_prepare && self.panel_target;
                let retained_gl_prepare = retained_gl_prepare && !self.panel_target;
                let activate_started = Instant::now();
                transport.activate(s.gl.as_ref().unwrap(), w as u32, h as u32)?;
                if self.started.elapsed().as_secs_f64() > 2. {
                    s.activate_ms
                        .push(activate_started.elapsed().as_secs_f64() * 1000.);
                }
                bead_core::profile::split("activate");
                if regions.is_empty() && !retained_gl_prepare {
                    wl_surface.frame(
                        qh,
                        FrameData {
                            surface: wl_surface.clone(),
                            epoch: s.callbacks.request(),
                            generation: scene.resource_revision,
                            source_epoch: if s.companion { self.panel_epoch } else { 0 },
                        },
                    );
                    s.callbacks.pending = true;
                    if let Some(visual) = &mut s.pointer_visual {
                        visual.stage_commit()?;
                    }
                    if let Some(presentation) = &self.presentation
                        && let Some(visual) = &s.pointer_visual
                    {
                        visual.feedback(presentation, &wl_surface, qh);
                    }
                    wl_surface.commit();
                    s.frames += 1;
                    return Ok(());
                }
                let start = Instant::now();
                let compose_profile = bead_core::profile::Span::new("compose");
                if retained_gl_prepare {
                    let opacity = scene.opacity;
                    // Warm a fade group only for motions that actually fade.
                    // Non-fading views need their image uploads, without a
                    // second full-window composite target.
                    scene.opacity = if scene.motion.as_ref().is_some_and(|motion| !motion.fade) {
                        1.
                    } else {
                        0.5
                    };
                    let prepared = renderer.draw(
                        scene,
                        RenderTarget::Gl(bead_render::gl::GlTarget {
                            display: &transport.display,
                            width: w as u32,
                            height: h as u32,
                            context: transport.generation,
                            warm: &transport.warm,
                        }),
                        Damage::full(w as u32, h as u32),
                    );
                    scene.opacity = opacity;
                    prepared?;
                    drop(compose_profile);
                    s.visual_damage = Default::default();
                    s.gl_prepared = true;
                    s.gl_prepare_pending = true;
                    if bead_core::profile::frame_trace() {
                        println!(
                            "PANEL_GPU_PREPARED monotonic_ns={} prepare_ms={:.3}",
                            monotonic_ns(),
                            frame_started.elapsed().as_secs_f64() * 1000.
                        );
                    }
                    return Ok(());
                }
                if command_promotion_pending {
                    let body = pending_promotion_body.ok_or("missing restored pearl body")?;
                    pending_promotion_staged = stage_command_promotion(s, body);
                    if !pending_promotion_staged {
                        return Err("could not stage restored pearl promotion".into());
                    }
                }
                if promoted_body_resize
                    && let Some(promotion) = &s.layer_promotion
                    && let Some((body_width, body_height)) = promoted_body_size
                {
                    promotion.set_body_size(body_width, body_height);
                }
                renderer.draw(
                    scene,
                    RenderTarget::Gl(bead_render::gl::GlTarget {
                        display: &transport.display,
                        width: w as u32,
                        height: h as u32,
                        context: transport.generation,
                        warm: &transport.warm,
                    }),
                    Damage::full(w as u32, h as u32),
                )?;
                drop(compose_profile);
                bead_core::profile::split("gl_flush");
                if self.started.elapsed().as_secs_f64() > 2. {
                    s.render_ms.push(start.elapsed().as_secs_f64() * 1000.);
                }
                for region in &regions {
                    wl_surface.damage_buffer(
                        region.x as i32,
                        region.y as i32,
                        region.w as i32,
                        region.h as i32,
                    );
                }
                wl_surface.frame(
                    qh,
                    FrameData {
                        surface: wl_surface.clone(),
                        epoch: s.callbacks.request(),
                        generation: scene.resource_revision,
                        source_epoch: if s.companion { self.panel_epoch } else { 0 },
                    },
                );
                s.callbacks.pending = true;
                if let Some(visual) = &mut s.pointer_visual {
                    visual.stage_commit()?;
                }
                if let Some(p) = &self.presentation {
                    if let Some(visual) = &s.pointer_visual {
                        visual.feedback(p, &wl_surface, qh);
                    }
                    p.feedback(
                        &wl_surface,
                        qh,
                        PresentationData {
                            content: s.companion
                                && self.panel_open.as_ref().is_some_and(|o| o.content),
                            source_epoch: if s.companion { self.panel_epoch } else { 0 },
                            visibility: if s.companion {
                                self.panel_close_epoch
                            } else {
                                0
                            },
                            update: s.ui_serial,
                            surface: wl_surface.clone(),
                            generation: scene.resource_revision,
                        },
                    );
                }
                let swap_started = Instant::now();
                if s.companion {
                    bead_core::profile::mark("panel_commit");
                }
                transport.present(s.gl.as_ref().unwrap(), &regions)?;
                s.gl_prepared |= first_gl_upload;
                bead_core::profile::split("swap");
                if pending_promotion_staged {
                    self.promotion_pending = false;
                }
                if s.promotion_state.renders_natural_extent() {
                    s.promoted_size = Some(render_size);
                    s.promoted_body_size = promoted_body_size;
                }
                s.promotion_reset_motion = false;
                s.last_submitted_ui_serial = s.ui_serial;
                if self.started.elapsed().as_secs_f64() > 2. {
                    s.swap_ms.push(swap_started.elapsed().as_secs_f64() * 1000.);
                }
                s.frames += 1;
                return Ok(());
            }
            #[cfg(feature = "wgpu")]
            if renderer.wants_gpu() {
                if command_promotion_pending {
                    let body = pending_promotion_body.ok_or("missing restored pearl body")?;
                    pending_promotion_staged = stage_command_promotion(s, body);
                    if !pending_promotion_staged {
                        return Err("could not stage restored pearl promotion".into());
                    }
                }
                if promoted_body_resize
                    && let Some(promotion) = &s.layer_promotion
                    && let Some((body_width, body_height)) = promoted_body_size
                {
                    promotion.set_body_size(body_width, body_height);
                }
                if self.gpu.is_none() {
                    self.gpu = Some(bead_render::gpu::Transport::new(self.options.memory_usage));
                }
                let transport = self.gpu.as_mut().unwrap();
                if s.gpu.is_none() {
                    use raw_window_handle::{
                        RawDisplayHandle, RawWindowHandle, WaylandDisplayHandle,
                        WaylandWindowHandle,
                    };
                    use std::ptr::NonNull;
                    let display = RawDisplayHandle::Wayland(WaylandDisplayHandle::new(
                        NonNull::new(self.conn.backend().display_ptr().cast())
                            .ok_or("native display missing")?,
                    ));
                    let surface = RawWindowHandle::Wayland(WaylandWindowHandle::new(
                        NonNull::new(wl_surface.id().as_ptr().cast())
                            .ok_or("native surface missing")?,
                    ));
                    // SAFETY: Surface::drop destroys GPU surface before protocol surface;
                    // app clears surfaces while the connection is still alive.
                    let created = unsafe {
                        transport.create_surface(display, surface, w as u32, h as u32, _kind)
                    };
                    match created {
                        Ok(gpu) => s.gpu = Some(gpu),
                        Err(e) => {
                            if !self.options.auto_fallback {
                                return Err(e);
                            }
                            eprintln!("FALLBACK GPU unavailable: {e}");
                            *renderer = bead_render::renderer(RendererKind::Cpu)?;
                            self.options.renderer = RendererKind::Cpu;
                            #[cfg(feature = "wgpu")]
                            {
                                self.gpu = None;
                            }
                        }
                    }
                    if s.gpu.is_some() {
                        bead_core::vprintln!(
                            "BUFFER output={} logical={}x{} physical={}x{} scale={scale} transport=GPU",
                            s.output.id().protocol_id(),
                            render_size.0,
                            render_size.1,
                            w,
                            h
                        );
                    }
                }
                if let Some(gpu) = s.gpu.as_mut() {
                    let transport = self.gpu.as_mut().unwrap();
                    transport.resize(gpu, w as u32, h as u32);
                    let dev = &transport.context.devices[gpu.dev_id];
                    use bead_render::gpu::{GpuTarget, wgpu};
                    let acquired = Instant::now();
                    let frame = match gpu.surface.get_current_texture() {
                        wgpu::CurrentSurfaceTexture::Success(f)
                        | wgpu::CurrentSurfaceTexture::Suboptimal(f) => f,
                        other => return Err(format!("GPU acquire failed: {other:?}").into()),
                    };
                    let acquire_ms = acquired.elapsed().as_secs_f64() * 1000.;
                    let start = Instant::now();
                    let compose_profile = bead_core::profile::Span::new("compose");
                    renderer.draw(
                        scene,
                        RenderTarget::Gpu(GpuTarget {
                            device: &dev.device,
                            queue: &dev.queue,
                            texture: &frame.texture,
                            target_view: gpu.target.as_ref().map(|(_, view, _)| view),
                            width: w as u32,
                            height: h as u32,
                        }),
                        Damage::full(w as u32, h as u32),
                    )?;
                    drop(compose_profile);
                    if let Some((_, view, blitter)) = &gpu.target {
                        let mut encoder = dev.device.create_command_encoder(&Default::default());
                        blitter.copy(
                            &dev.device,
                            &mut encoder,
                            view,
                            &frame.texture.create_view(&Default::default()),
                        );
                        dev.queue.submit([encoder.finish()]);
                    }
                    if self.started.elapsed().as_secs_f64() > 2. {
                        s.render_ms.push(start.elapsed().as_secs_f64() * 1000.);
                        s.acquire_ms.push(acquire_ms);
                    }
                    wl_surface.damage_buffer(0, 0, w, h);
                    wl_surface.frame(
                        qh,
                        FrameData {
                            surface: wl_surface.clone(),
                            epoch: s.callbacks.request(),
                            generation: scene.resource_revision,
                            source_epoch: if s.companion { self.panel_epoch } else { 0 },
                        },
                    );
                    s.callbacks.pending = true;
                    if let Some(visual) = &mut s.pointer_visual {
                        visual.stage_commit()?;
                    }
                    if let Some(p) = &self.presentation {
                        if let Some(visual) = &s.pointer_visual {
                            visual.feedback(p, &wl_surface, qh);
                        }
                        p.feedback(
                            &wl_surface,
                            qh,
                            PresentationData {
                                content: s.companion
                                    && self.panel_open.as_ref().is_some_and(|o| o.content),
                                source_epoch: if s.companion { self.panel_epoch } else { 0 },
                                visibility: if s.companion {
                                    self.panel_close_epoch
                                } else {
                                    0
                                },
                                update: s.ui_serial,
                                surface: wl_surface.clone(),
                                generation: scene.resource_revision,
                            },
                        );
                    }
                    dev.queue.present(frame);
                    if pending_promotion_staged {
                        self.promotion_pending = false;
                    }
                    if s.promotion_state.renders_natural_extent() {
                        s.promoted_size = Some(render_size);
                        s.promoted_body_size = promoted_body_size;
                    }
                    s.promotion_reset_motion = false;
                    s.last_submitted_ui_serial = s.ui_serial;
                    let _ = dev.device.poll(wgpu::PollType::Poll);
                    s.frames += 1;
                    return Ok(());
                }
                // An explicitly permitted automatic fallback uses the same retained Scene.
                renderer.prepare_on_ui(scene, scale)?;
            }
            bead_core::profile::split("gpu_fallback");
            let repair_profile = bead_core::profile::Span::new("repair_setup");
            if !cfg!(target_endian = "little") {
                return Err("CPU renderer currently requires little endian".into());
            }
            if s.geometry != (w, h) || s.format != format || s.buffers.is_empty() {
                s.buffers.clear();
                s.geometry = (w, h);
                s.format = format;
                s.previous = None;
                // Pools only grow, and an old geometry's active slots cannot be
                // reused yet. Start a bounded pool for this geometry instead of
                // retaining old-scale pages or growing around those slots.
                // SCTK defers active wl_buffer destruction until release; the
                // compositor retains its backing even after our mapping drops.
                // SlotPool rounds each slot to a 64-byte boundary.
                s.pool = SlotPool::new(size.next_multiple_of(64) * 3, &self.shm)?;
                for _ in 0..3 {
                    let (buffer, canvas) = s.pool.create_buffer(
                        w,
                        h,
                        stride,
                        if format == PixelFormat::Rgba {
                            wl_shm::Format::Abgr8888
                        } else {
                            wl_shm::Format::Argb8888
                        },
                    )?;
                    if scene.motion.is_some() {
                        advise_frame_pages(canvas, false);
                    }
                    s.buffers.push(Reusable {
                        buffer,
                        pending: vec![Damage::full(w as u32, h as u32)],
                        painted: false,
                    });
                    s.allocations += 1;
                }
                bead_core::vprintln!(
                    "BUFFER output={} logical={}x{} physical={}x{} scale={scale} format={format:?} allocation_count={} pool_bytes={} buffer_bytes={size}",
                    s.output.id().protocol_id(),
                    render_size.0,
                    render_size.1,
                    w,
                    h,
                    s.allocations,
                    s.pool.len()
                );
            }
            let current = scene.bounds();
            for b in &mut s.buffers {
                for region in &regions {
                    track_repair(&mut b.pending, *region);
                }
            }
            // Animation history needs the fresher-pixel policy. Ordinary surfaces
            // keep the cheap first-free path (and the existing unpainted adoption).
            let plan = if scene.motion.is_some() {
                repair_plan(
                    s.buffers.iter().map(|b| {
                        (
                            !b.buffer.slot().has_active_buffers(),
                            b.painted,
                            b.pending.as_slice(),
                        )
                    }),
                    w as u64 * h as u64,
                )
            } else {
                s.buffers
                    .iter()
                    .position(|b| !b.buffer.slot().has_active_buffers())
                    .map(|slot| {
                        let adopt = (!s.buffers[slot].painted)
                            .then(|| s.buffers.iter().position(|b| b.painted))
                            .flatten();
                        (slot, adopt)
                    })
            };
            let Some((slot, adopt)) = plan else {
                // The compositor still holds every buffer (a slow release right after a
                // resize): try again shortly rather than ending the whole shell.
                s.redraw = true;
                busy = true;
                return Ok(());
            };
            // An older painted slot can still owe an entire animation repaint.
            // Copy fresher pixels and their matching repairs together, just as
            // for an unpainted spare left by pool compaction.
            if let Some(from) = adopt {
                let pending = s.buffers[from].pending.clone();
                let zero_target = !s.buffers[slot].painted;
                // A held source is read-only; canvas() is only used for the free target.
                let (source, len) = {
                    let held = s.buffers[from].buffer.slot();
                    let data = s.pool.raw_data_mut(&held);
                    (data.as_ptr(), data.len())
                };
                if let Some(target) = s.buffers[slot].buffer.canvas(&mut s.pool) {
                    if zero_target && scene.motion.is_some() {
                        // Join boundary-page advice only when the spare is used,
                        // keeping untouched spare pages out of settled residency.
                        advise_frame_pages(target, true);
                    }
                    // SAFETY: distinct slots in the same live, unmoved pool mapping.
                    // Repairs fully replace their pixels, so only their complement
                    // needs the adopted contents. The source is never written.
                    copy_outside_repairs(
                        unsafe { std::slice::from_raw_parts(source, len) },
                        target,
                        w as u32,
                        h as u32,
                        &pending,
                        zero_target,
                    );
                    s.buffers[slot].pending = pending;
                }
            }
            if command_promotion_pending {
                let body = pending_promotion_body.ok_or("missing restored pearl body")?;
                pending_promotion_staged = stage_command_promotion(s, body);
                if !pending_promotion_staged {
                    return Err("could not stage restored pearl promotion".into());
                }
            }
            if promoted_body_resize
                && let Some(promotion) = &s.layer_promotion
                && let Some((body_width, body_height)) = promoted_body_size
            {
                promotion.set_body_size(body_width, body_height);
            }
            let b = &mut s.buffers[slot];
            b.painted = true;
            let bounding = b
                .pending
                .iter()
                .copied()
                .fold(Damage::full(0, 0), Damage::union);
            let repairs = if _kind == RendererKind::Cpu {
                b.pending.as_slice()
            } else {
                std::slice::from_ref(&bounding)
            };
            let start = Instant::now();
            drop(repair_profile);
            let compose_profile = bead_core::profile::Span::new("compose");
            let canvas = b
                .buffer
                .canvas(&mut s.pool)
                .ok_or("compositor owns buffer")?;
            for damage in repairs {
                renderer.draw(
                    scene,
                    RenderTarget::Shm(ShmTarget {
                        bytes: &mut *canvas,
                        width: w as u32,
                        height: h as u32,
                        scale,
                        format,
                    }),
                    *damage,
                )?;
            }
            drop(compose_profile);
            bead_core::profile::split("paint");
            if self.started.elapsed().as_secs_f64() > 2. {
                s.render_ms.push(start.elapsed().as_secs_f64() * 1000.);
            }
            let _commit_profile = bead_core::profile::Span::new("commit");
            for region in &regions {
                wl_surface.damage_buffer(
                    region.x as i32,
                    region.y as i32,
                    region.w as i32,
                    region.h as i32,
                );
            }
            if let Some(epoch) = s.callbacks.request_if_idle() {
                wl_surface.frame(
                    qh,
                    FrameData {
                        surface: wl_surface.clone(),
                        epoch,
                        generation: scene.resource_revision,
                        source_epoch: if s.companion { self.panel_epoch } else { 0 },
                    },
                );
                s.callbacks.pending = true;
            }
            if let Some(visual) = &mut s.pointer_visual {
                visual.stage_commit()?;
            }
            if let Some(p) = &self.presentation {
                if let Some(visual) = &s.pointer_visual {
                    visual.feedback(p, &wl_surface, qh);
                }
                p.feedback(
                    &wl_surface,
                    qh,
                    PresentationData {
                        content: s.companion && self.panel_open.as_ref().is_some_and(|o| o.content),
                        source_epoch: if s.companion { self.panel_epoch } else { 0 },
                        visibility: if s.companion {
                            self.panel_close_epoch
                        } else {
                            0
                        },
                        update: s.ui_serial,
                        surface: wl_surface.clone(),
                        generation: scene.resource_revision,
                    },
                );
            }
            b.buffer.attach_to(&wl_surface)?;
            if layer_motion_cpu_replacement {
                if let Some((visible, input_rect, interactivity, input_enabled)) =
                    layer_motion_pending_input
                {
                    if s.input_visible != Some(visible) || s.input_rect != input_rect {
                        if input_enabled {
                            if let Some([x, y, w, h]) = input_rect {
                                let region = Region::new(&self.compositor)?;
                                if w > 0 && h > 0 {
                                    region.add(x, y, w, h);
                                }
                                wl_surface.set_input_region(Some(region.wl_region()));
                            } else {
                                wl_surface.set_input_region(None);
                            }
                        } else {
                            let empty = Region::new(&self.compositor)?;
                            wl_surface.set_input_region(Some(empty.wl_region()));
                        }
                        s.input_visible = Some(visible);
                        s.input_rect = input_rect;
                    }
                    if s.keyboard_visible != Some(interactivity) {
                        s.layer.set_keyboard_interactivity(interactivity);
                        s.keyboard_visible = Some(interactivity);
                    }
                }
                // Reset takes effect with this replacement buffer commit. If
                // painting or buffer acquisition failed earlier, the old atlas
                // and its sampled presentation remain untouched for retry.
                s.layer_motion
                    .as_mut()
                    .ok_or("active compositor presentation has no controller")?
                    .reset_presentation();
            }
            if s.companion {
                bead_core::profile::mark("panel_commit");
                s.panel_unmapped = false;
            }
            wl_surface.commit();
            if pending_promotion_staged {
                self.promotion_pending = false;
            }
            if s.promotion_state.renders_natural_extent() {
                s.promoted_size = Some(render_size);
                s.promoted_body_size = promoted_body_size;
            }
            s.promotion_reset_motion = false;
            if layer_motion_cpu_replacement {
                s.offset = scene.offset;
                s.input_content_offset = input_content_offset;
                s.layer_motion_active = false;
                s.layer_motion_open = None;
                s.layer_motion_plan = None;
                s.layer_motion_resting_offset = None;
                s.layer_motion_ui_serial = 0;
                self.panel_frame_until = None;
            }
            if replace_pending_ui {
                let marked = s.callbacks.mark_replacement_used();
                debug_assert!(marked, "pending callback changed during synchronous draw");
            }
            s.last_submitted_ui_serial = s.ui_serial;
            if s.native.is_none() && s.prepared_native.is_some() {
                let slot = b.buffer.slot();
                let snapshot = s
                    .pool
                    .create_buffer_in(
                        &slot,
                        w,
                        h,
                        stride,
                        if format == PixelFormat::Rgba {
                            wl_shm::Format::Abgr8888
                        } else {
                            wl_shm::Format::Argb8888
                        },
                    )
                    .ok();
                if let Some(prepared) = s.prepared_native.as_mut() {
                    prepared.snapshot = snapshot;
                    #[cfg(feature = "native-gl")]
                    {
                        prepared.gl_ready = false;
                    }
                    prepared.snapshot_size_generation = prepared.size_generation;
                    prepared.snapshot_ui_serial = s.ui_serial;
                    prepared.snapshot_size = (s.width, s.height);
                    prepared.snapshot_scale120 = s.scale120;
                }
            }
            b.pending.clear();
            s.previous = Some(current);
            s.frames += 1;
            bead_core::profile::split("commit");
            Ok(())
        })();
        if result.is_ok()
            && self.surfaces[index].companion
            && self.surfaces[index].frames > previous_frames
        {
            self.lip_tab_handover();
        }
        if result.is_ok()
            && self.surfaces[index].companion
            && self.panel_target
            && self.surfaces[index].frames > previous_frames
        {
            self.surfaces[index].panel_unmapped = false;
        }
        if result.is_ok() && self.surfaces[index].frames > previous_frames {
            self.log_prepared_native_ready(index);
        }
        if result.is_ok()
            && self.surfaces[index].companion
            && self.surfaces[index].frames > previous_frames
        {
            if self
                .panel_dock_transition
                .as_ref()
                .is_some_and(|t| !t.started)
            {
                self.start_dock_transition_after_commit();
            } else if self
                .dock_transition_progress(self.started.elapsed().as_secs_f64())
                .is_some_and(|progress| progress >= 1.)
            {
                self.panel_dock_transition = None;
            }
        }
        self.retire_native_cover(
            index,
            result.is_ok() && self.surfaces[index].frames > previous_frames,
        );
        if result.is_ok()
            && self.surfaces[index].companion
            && self.surfaces[index].frames > previous_frames
        {
            // Reverse handoff: native resources retire only after the attached
            // destination has committed accepted content, never on dock request.
            self.panel_cover = None;
            // The panel's input rect or position may have changed: move the catcher's hole.
            if let Some(enabled) = self.dismiss.as_ref().map(|d| d.enabled)
                && let Err(error) = self.set_dismiss_enabled(enabled)
            {
                self.fail(error);
            }
        }
        let motion_label = traced_frame
            .and(motion_clock.as_ref())
            .map(|(id, _, _)| id.clone());
        if let Some((id, drawn, duration)) = motion_clock
            && self.surfaces[index].frames > previous_frames
        {
            // The first committed frame of a motion starts its clock: preparation, configure and
            // a slow first frame never spend the slide (B0170).
            let committed_at = self.started.elapsed().as_secs_f64();
            let sent_ago = self.surfaces[index].callback_sent_ago;
            let callback = self.surfaces[index].last_callback.map(|c| {
                (
                    c.saturating_duration_since(self.started).as_secs_f64(),
                    sent_ago,
                )
            });
            bead_core::profile::split("post");
            self.animations.add_builds(&id, main_builds);
            let slide = self.animations.committed_timed(
                &id,
                drawn,
                committed_at,
                callback,
                draw_start,
                &bead_core::profile::splits(split_start),
            );
            super::overlay::log_slide(&id, slide);
            let motion_active = self
                .animations
                .value(&id, committed_at)
                .is_some_and(|(_, active)| active);
            if self.surfaces[index].companion && self.panel_frame_until.is_none() && motion_active {
                self.panel_frame_until = Some(committed_at + duration);
            }
        }
        // A pending frame callback already supplies the retry wakeup.
        if busy && !self.surfaces[index].callbacks.pending {
            let qh = qh.clone();
            let retry = self.loop_handle.insert_source(
                Timer::from_duration(Duration::from_millis(8)),
                move |_, _, app| {
                    if index < app.surfaces.len() {
                        app.draw(index, &qh);
                    }
                    TimeoutAction::Drop
                },
            );
            if let Err(error) = retry {
                self.fail(error);
            }
        }
        if self.surfaces[index].frames > previous_frames
            && self.started.elapsed().as_secs_f64() > 2.
        {
            if let Some((wall, cpu)) = traced_frame {
                // B0205: the wall split of this frame, its thread CPU, and the gap since
                // the previous committed frame of the same kind of surface.
                thread_local!(static LAST: std::cell::Cell<[u64; 2]> = const { std::cell::Cell::new([0; 2]) });
                let now = monotonic_ns();
                let kind = usize::from(self.surfaces[index].companion);
                let gap = LAST.with(|last| {
                    let mut times = last.get();
                    let previous = std::mem::replace(&mut times[kind], now);
                    last.set(times);
                    previous
                });
                println!(
                    "UI_FRAME monotonic_ns={now} render_ms={:.3} companion={} wall_ms={:.3} cpu_ms={:.3} gap_ms={:.3} serial={} damage_px={damage_px} motion={} split={}",
                    frame_started.elapsed().as_secs_f64() * 1000.,
                    self.surfaces[index].companion,
                    now.saturating_sub(wall) as f64 / 1e6,
                    bead_core::profile::thread_cpu_ns().saturating_sub(cpu) as f64 / 1e6,
                    if gap == 0 {
                        0.
                    } else {
                        now.saturating_sub(gap) as f64 / 1e6
                    },
                    self.surfaces[index].ui_serial,
                    motion_label.as_deref().unwrap_or("-"),
                    bead_core::profile::take_splits(wall)
                );
            }
            if self.options.debug_luau_stall_ms.is_some() {
                bead_core::vprintln!(
                    "DEBUG_FRAME monotonic_ns={} companion={}",
                    monotonic_ns(),
                    self.surfaces[index].companion
                );
            }
            if self.options.trace_state && self.surfaces[index].companion {
                bead_core::vprintln!(
                    "PANEL_FRAME processing_ms={:.3}",
                    frame_started.elapsed().as_secs_f64() * 1000.
                );
            }
            self.surfaces[index]
                .frame_ms
                .push(frame_started.elapsed().as_secs_f64() * 1000.);
        }
        let companion = self.surfaces[index].companion;
        let s = &self.surfaces[index];
        let in_progress = layer_motion_active || (companion && self.panel_open.is_some());
        if result.is_ok()
            && s.frames > previous_frames
            && let Some(delay) = missed_callback_delay(damage_px, in_progress, s.refresh_ns)
        {
            let (identity, armed, kick) = (s.wl_surface().clone(), s.frames, qh.clone());
            let wait = Timer::from_duration(delay);
            let _ = self.loop_handle.insert_source(wait, move |_, _, app| {
                if let Some(s) = app.surfaces.get_mut(index)
                    && s.wl_surface() == &identity
                    && (s.frames, s.callbacks.pending, s.redraw) == (armed, true, true)
                {
                    s.callbacks.invalidate();
                    app.draw(index, &kick);
                }
                TimeoutAction::Drop
            });
        }
        let arm_transient = if companion {
            self.companion
                .as_mut()
                .is_some_and(|(_, r, _)| r.take_transient_timer())
        } else {
            self.renderer.take_transient_timer()
        };
        if arm_transient {
            let epoch = self.panel_epoch;
            if let Err(error) = self.loop_handle.insert_source(
                // B0205: the first check also starts quiet-time work (a close layer).
                Timer::from_duration(Duration::from_millis(150)),
                move |_, _, app| {
                    // A pearl's renderer is only in `app.companion` while its slot is swapped in.
                    let remaining = if companion {
                        app.with_epoch(epoch, |app| {
                            app.companion.as_mut().and_then(|(scene, r, _)| {
                                r.idle_work(scene);
                                r.trim_transient_idle()
                            })
                        })
                        .flatten()
                    } else {
                        app.renderer.idle_work(&app.scene);
                        app.renderer.trim_transient_idle()
                    };
                    if remaining.is_none() {
                        // The last repair can outlive the revision's heap trim.
                        // Reclaim its freed pages only after scratch becomes idle.
                        #[cfg(all(target_os = "linux", target_env = "gnu"))]
                        unsafe {
                            libc::malloc_trim(0);
                        }
                    }
                    remaining.map_or(TimeoutAction::Drop, TimeoutAction::ToDuration)
                },
            ) {
                self.fail(error);
            }
        }
        if result.is_ok() {
            self.trim_idle_shm(index);
            if self.surfaces[index].frames > previous_frames {
                let revision = if companion {
                    self.companion
                        .as_ref()
                        .map(|(scene, _, _)| scene.resource_revision)
                } else {
                    Some(self.scene.resource_revision)
                };
                let generation = revision
                    .map(|revision| (if companion { self.panel_epoch } else { 0 }, revision));
                if self.surfaces[index].heap_trim_generation != generation {
                    self.surfaces[index].heap_trim_generation = generation;
                    // Acceptance precedes presenting the replacement. Wait until
                    // old frame references and reveal scratch have been released;
                    // this heap-only trim does not retire or repaint SHM buffers.
                    if let Err(error) = self.loop_handle.insert_source(
                        Timer::from_duration(Duration::from_millis(500)),
                        |_, _, _| {
                            #[cfg(all(target_os = "linux", target_env = "gnu"))]
                            unsafe {
                                libc::malloc_trim(0);
                            }
                            TimeoutAction::Drop
                        },
                    ) {
                        self.fail(error);
                    }
                }
            }
        }
        if let Err(e) = result {
            if self.prepare_surface_images(index, e.as_ref()) {
                return;
            }
            #[cfg(any(feature = "native-gl", feature = "wgpu"))]
            if self.options.auto_fallback
                && self.surfaces[index].companion
                && companion_falls_back(
                    self.companion
                        .as_ref()
                        .is_some_and(|(_, _, kind)| *kind == RendererKind::FemtovgGl),
                    e.as_ref(),
                )
            {
                eprintln!("FALLBACK companion GPU unavailable: {e}");
                #[cfg(feature = "native-gl")]
                report_texture_refusal(e.as_ref());
                let renderer = match bead_render::renderer(RendererKind::Cpu) {
                    Ok(renderer) => renderer,
                    Err(error) => {
                        self.fail(error);
                        return;
                    }
                };
                if let Some((_, active, kind)) = &mut self.companion {
                    *active = renderer;
                    *kind = RendererKind::Cpu;
                } else {
                    self.fail("companion renderer unavailable during GPU fallback");
                    return;
                }
                if let Some((_, desired)) = &mut self.options.companion {
                    // Keep this slot on CPU after fallback instead of retrying GL
                    // on every frame. A replacement panel may choose its own policy.
                    *desired = RendererKind::Cpu;
                }
                for surface in self.surfaces.iter_mut().filter(|surface| surface.companion) {
                    #[cfg(feature = "native-gl")]
                    {
                        drop(surface.gl.take());
                        surface.gl_prepared = false;
                        surface.gl_prepare_pending = false;
                    }
                    #[cfg(feature = "wgpu")]
                    drop(surface.gpu.take());
                    surface.callbacks.invalidate();
                    surface.visual_damage = Default::default();
                }
                self.release_timer = false;
                self.companion_policy = Default::default();
                #[cfg(feature = "native-gl")]
                if self.options.renderer == RendererKind::Cpu
                    && !self.has_gl_surfaces()
                    && let Some(transport) = self.gl.take()
                    && let Err(error) = {
                        // SAFETY: all companion GL surfaces were just dropped, no primary
                        // GL surface remains, and the CPU renderer owns no EGL resources.
                        unsafe { transport.shutdown_exclusive() }
                    }
                {
                    self.fail(error);
                    return;
                }
                #[cfg(feature = "wgpu")]
                if !self.has_gpu_surfaces() {
                    self.gpu = None;
                }
                println!("RENDER_POLICY companion=true transition=gpu-to-cpu fallback=true");
                if self
                    .surfaces
                    .iter()
                    .filter(|surface| surface.companion)
                    .all(|surface| surface.frames == 0)
                {
                    self.animations = Default::default();
                }
                let indices: Vec<_> = self
                    .surfaces
                    .iter()
                    .enumerate()
                    .filter(|(_, surface)| surface.companion)
                    .map(|(index, _)| index)
                    .collect();
                for index in indices {
                    // draw() already selected the panel animation map for this call.
                    self.draw_current(index, qh);
                }
                return;
            }
            #[cfg(any(feature = "native-gl", feature = "wgpu"))]
            if self.options.auto_fallback
                && !self.surfaces[index].companion
                && e.downcast_ref::<GpuUnavailable>().is_some()
            {
                eprintln!("FALLBACK GPU unavailable: {e}");
                if let Ok(renderer) = bead_render::renderer(RendererKind::Cpu) {
                    self.renderer = renderer;
                }
                for surface in &mut self.surfaces {
                    #[cfg(feature = "native-gl")]
                    {
                        drop(surface.gl.take());
                        surface.gl_prepared = false;
                        surface.gl_prepare_pending = false;
                    }
                    #[cfg(feature = "wgpu")]
                    drop(surface.gpu.take());
                    surface.callbacks.pending = false;
                }
                #[cfg(feature = "native-gl")]
                {
                    self.release_all_wallpaper_runs();
                    self.gl = None;
                }
                #[cfg(feature = "wgpu")]
                {
                    self.gpu = None;
                }
                self.options.renderer = RendererKind::Cpu;
                for i in 0..self.surfaces.len() {
                    self.draw(i, qh);
                }
                return;
            }
            self.fail(bead_core::cache::explain_refusal(&e.to_string()));
        }
    }
}

/// Keep a bounded repair history for each compositor-owned SHM buffer.
fn track_repair(pending: &mut Vec<Damage>, mut region: Damage) {
    if region.w == 0 || region.h == 0 {
        return;
    }
    let mut index = 0;
    while index < pending.len() {
        let old = pending[index];
        if region.x < old.x + old.w
            && old.x < region.x + region.w
            && region.y < old.y + old.h
            && old.y < region.y + region.h
        {
            region = region.union(pending.swap_remove(index));
            index = 0;
        } else {
            index += 1;
        }
    }
    if pending.len() < 8 {
        pending.push(region);
    } else {
        let area = |d: Damage| u64::from(d.w) * u64::from(d.h);
        let index = pending
            .iter()
            .enumerate()
            .min_by_key(|(_, old)| {
                area(old.union(region))
                    .saturating_sub(area(**old))
                    .saturating_sub(area(region))
            })
            .map(|(i, _)| i)
            .unwrap_or(0);
        pending[index] = pending[index].union(region);
    }
}

/// Prefer current pixels over an older animation frame or an unpainted slot.
/// Rebase stale pixels only when the saved repair exceeds a quarter of a frame;
/// smaller edits do not justify a full-buffer copy. Held sources are read-only.
#[inline(never)]
fn repair_plan<'a>(
    buffers: impl Iterator<Item = (bool, bool, &'a [Damage])> + Clone,
    frame_pixels: u64,
) -> Option<(usize, Option<usize>)> {
    let pixels = |pending: &[Damage]| {
        pending
            .iter()
            .map(|d| u64::from(d.w) * u64::from(d.h))
            .sum::<u64>()
    };
    let (slot, (_, painted, pending)) = buffers
        .clone()
        .enumerate()
        .filter(|(_, (available, _, _))| *available)
        .min_by_key(|(_, (_, painted, pending))| (!*painted, pixels(pending)))?;
    let adopt = buffers
        .enumerate()
        .filter(|(_, (_, painted, _))| *painted)
        .min_by_key(|(_, (_, _, pending))| pixels(pending))
        .filter(|(from, (_, _, source))| {
            *from != slot && (!painted || pixels(pending) > pixels(source) + frame_pixels / 4)
        })
        .map(|(from, _)| from);
    Some((slot, adopt))
}

#[cfg(test)]
mod repair_tests {
    use super::*;
    use bead_render::{Primitive, Rect, Renderer, Scene, tree::TreeCpuRenderer};
    use std::collections::BTreeMap;

    #[test]
    fn settled_pixels_win_over_animation_debt_but_held_slots_are_never_written() {
        let animation = [Damage::full(1746, 981)];
        let edit = [Damage::full(200, 40)];
        let empty = [];
        let slots = [
            (true, true, animation.as_slice()),
            (true, true, edit.as_slice()),
            (true, false, empty.as_slice()),
        ];
        let area = 1746 * 981;
        assert_eq!(repair_plan(slots.into_iter(), area), Some((1, None)));
        let mut held = slots;
        held[1].0 = false;
        assert_eq!(repair_plan(held.into_iter(), area), Some((0, Some(1))));
        held[0].0 = false;
        assert_eq!(repair_plan(held.into_iter(), area), Some((2, Some(1))));
        held[2].0 = false;
        assert_eq!(repair_plan(held.into_iter(), area), None);
        let narrow = [Damage::full(100, 40)];
        assert_eq!(
            repair_plan(
                [
                    (true, true, edit.as_slice()),
                    (false, true, narrow.as_slice())
                ]
                .into_iter(),
                area,
            ),
            Some((0, None)),
            "small repair differences must not cause a full-buffer copy"
        );
    }

    #[test]
    fn repair_history_is_bounded_and_preserves_every_dirty_rectangle() {
        let changes: Vec<_> = (0..30)
            .map(|i| Damage {
                x: i * 7,
                y: (i % 3) * 9,
                w: 2,
                h: 2,
            })
            .collect();
        let mut pending = Vec::new();
        for change in &changes {
            track_repair(&mut pending, *change);
            assert!(pending.len() <= 8);
        }
        for change in changes {
            assert!(pending.iter().any(|d| d.x <= change.x
                && d.y <= change.y
                && d.x + d.w >= change.x + change.w
                && d.y + d.h >= change.y + change.h));
        }
        let mut separate = Vec::new();
        track_repair(
            &mut separate,
            Damage {
                x: 2,
                y: 2,
                w: 4,
                h: 4,
            },
        );
        track_repair(
            &mut separate,
            Damage {
                x: 80,
                y: 50,
                w: 4,
                h: 4,
            },
        );
        assert_eq!(
            separate.len(),
            2,
            "distant edits must not repaint their intervening area"
        );
    }

    #[test]
    fn three_buffer_disjoint_repair_matches_full_frames_with_age_and_alpha() {
        for scale in [1., 1.5] {
            let (width, height) = ((96. * scale) as u32, (72. * scale) as u32);
            for format in [PixelFormat::Rgba, PixelFormat::Bgra] {
                let mut buffers: Vec<_> = (0..3)
                    .map(|_| {
                        (
                            vec![0; (width * height * 4) as usize],
                            vec![Damage::full(width, height)],
                            false,
                        )
                    })
                    .collect();
                let mut renderer = TreeCpuRenderer::default();
                for frame in 0..12 {
                    let fill = |rect, color| Primitive::Rounded {
                        rect,
                        radius: 3.,
                        fill: color,
                        border: None,
                        radial: None,
                    };
                    let scene = Scene {
                        opacity: 1.,
                        motion: None,
                        resource_revision: 1,
                        asset_data: BTreeMap::new(),
                        clips: BTreeMap::new(),
                        layers: vec![],
                        prepared: BTreeMap::new(),
                        font_files: vec![],
                        width: 96.,
                        slide: None,
                        offset: (0., 0.),
                        bounds: Rect::new(0., 0., 96., 72.),
                        primitives: vec![
                            fill(Rect::new(0., 0., 96., 72.), [17, 31, 71, 210]),
                            fill(Rect::new(4., 4., 16., 10.), [frame * 13, 93, 25, 170]),
                            fill(Rect::new(74., 54., 16., 10.), [29, frame * 11, 217, 255]),
                        ],
                        input_islands: None,
                        key_repeat: None,
                        palette_at_rest: false,
                    };
                    for (_, pending, _) in &mut buffers {
                        track_repair(
                            pending,
                            Rect::new(2., 2., 20., 14.).pixels(scale, width, height),
                        );
                        track_repair(
                            pending,
                            Rect::new(72., 52., 20., 14.).pixels(scale, width, height),
                        );
                    }
                    renderer.prepare(&scene, scale).unwrap();
                    let frame_index = usize::from(frame);
                    let (slot, adopt) = repair_plan(
                        buffers
                            .iter()
                            .enumerate()
                            .map(|(index, (_, pending, painted))| {
                                let available = if frame_index % 4 == 0 {
                                    index == frame_index % 3
                                } else {
                                    index != (frame_index + 1) % 3
                                };
                                (available, *painted, pending.as_slice())
                            }),
                        u64::from(width) * u64::from(height),
                    )
                    .unwrap();
                    let adopted = adopt.map(|from| (from, buffers[from].clone()));
                    if let Some((_, (bytes, pending, _))) = &adopted {
                        let zero_target = !buffers[slot].2;
                        copy_outside_repairs(
                            bytes,
                            &mut buffers[slot].0,
                            width,
                            height,
                            pending,
                            zero_target,
                        );
                        buffers[slot].1 = pending.clone();
                    }
                    let (bytes, pending, painted) = &mut buffers[slot];
                    for damage in pending.iter().copied() {
                        renderer
                            .draw(
                                &scene,
                                RenderTarget::Shm(ShmTarget {
                                    bytes: &mut *bytes,
                                    width,
                                    height,
                                    scale,
                                    format,
                                }),
                                damage,
                            )
                            .unwrap();
                    }
                    pending.clear();
                    *painted = true;
                    let mut expected = vec![0; bytes.len()];
                    let mut reference = TreeCpuRenderer::default();
                    reference.prepare(&scene, scale).unwrap();
                    reference
                        .draw(
                            &scene,
                            RenderTarget::Shm(ShmTarget {
                                bytes: &mut expected,
                                width,
                                height,
                                scale,
                                format,
                            }),
                            Damage::full(width, height),
                        )
                        .unwrap();
                    assert!(
                        *bytes == expected,
                        "stale buffer at frame{frame},scale{scale},format{format:?}"
                    );
                    if let Some((from, before)) = adopted {
                        assert_eq!(buffers[from], before, "adoption wrote to its source");
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod input_tests {
    use super::*;
    #[test]
    fn vertical_close_keeps_x_fixed_while_side_close_keeps_y_fixed() {
        let close_duration = 0.21_f32;
        let mut previous_vertical_x = None;
        let mut previous_side_y = None;
        for frame in 0..=26 {
            let t = (frame as f32 / 120. / close_duration).clamp(0., 1.);
            let progress = 1. - t.powi(4);
            let (bottom_dx, bottom_dy) =
                bead_core::Edge::Bottom.closed_offset(1100., 590., progress);
            let bottom = motion_scene_offset(
                812.,
                bead_core::Edge::Bottom,
                false,
                (bottom_dx, bottom_dy),
                (32., 32.),
            );
            assert_eq!(bottom.0, 812.);
            if let Some(previous) = previous_vertical_x {
                assert_eq!(bottom.0, previous);
            }
            previous_vertical_x = Some(bottom.0);

            let (right_dx, right_dy) = bead_core::Edge::Right.closed_offset(1100., 590., progress);
            let right = motion_scene_offset(
                812.,
                bead_core::Edge::Right,
                false,
                (right_dx, right_dy),
                (32., 32.),
            );
            assert_eq!(right.1, 32.);
            if let Some(previous) = previous_side_y {
                assert_eq!(right.1, previous);
            }
            previous_side_y = Some(right.1);
        }
        // A folder view's scene has no shadow margin: its open panel rests at the surface origin.
        let open = motion_scene_offset(0., bead_core::Edge::Top, true, (0., 0.), (0., 0.));
        assert_eq!(open, (0., 0.));
    }

    fn scene() -> Scene {
        let body = Rect::new(0., 0., 200., 100.);
        Scene {
            opacity: 1.,
            motion: None,
            resource_revision: 0,
            asset_data: Default::default(),
            clips: Default::default(),
            layers: vec![bead_render::SceneLayer {
                id: "content".into(),
                parent: None,
                own: 0..2,
                subtree_end: 2,
                bounds: body,
            }],
            prepared: Default::default(),
            font_files: vec![],
            width: 200.,
            slide: None,
            primitives: vec![
                bead_render::Primitive::Rounded {
                    rect: Rect::new(0., 0., 0., 0.),
                    radius: 0.,
                    fill: [0; 4],
                    border: None,
                    radial: None,
                },
                bead_render::Primitive::Rounded {
                    rect: body,
                    radius: 8.,
                    fill: [20; 4],
                    border: None,
                    radial: None,
                },
            ],
            offset: (32., 32.),
            bounds: Rect::new(-32., -32., 264., 164.),
            input_islands: None,
            key_repeat: None,
            palette_at_rest: false,
        }
    }

    #[test]
    fn detached_gl_extent_uses_natural_scene_instead_of_clipped_source_buffer() {
        let mut scene = scene();
        scene.bounds = Rect::new(-32., -32., 1164., 654.);
        scene.offset = (32., 32.);

        let natural = native_gl_dimensions(&scene, 180).unwrap();
        let (clipped_width, clipped_height, _, _) = buffer_dimensions(1164, 632, 180).unwrap();

        assert_eq!(natural, (1746, 981));
        assert_eq!((clipped_width, clipped_height), (1746, 948));
    }

    #[test]
    fn painted_content_excludes_shadow_and_survives_background_replacement() {
        let mut scene = scene();
        assert_eq!(content_input_rect(&scene), Some([32, 32, 200, 100]));
        scene.primitives.clear();
        assert_eq!(content_input_rect(&scene), Some([32, 32, 200, 100]));
        scene.offset = (-10.5, 5.25);
        assert_eq!(content_input_rect(&scene), Some([-11, 5, 201, 101]));
    }
    #[test]
    fn unretained_fallback_skips_empty_shadow_placeholder() {
        let mut scene = scene();
        scene.layers.clear();
        assert_eq!(content_input_rect(&scene), Some([32, 32, 200, 100]));
        scene.primitives.clear();
        assert_eq!(content_input_rect(&scene), None);
    }
    #[test]
    fn entering_panel_never_captures_above_its_final_content_footprint() {
        let mut scene = scene();
        scene.motion = Some(bead_render::Motion {
            id: "content".into(),
            open: true,
            duration: 0.5,
            return_duration: None,
            return_curve: None,
            auto_open: false,
            spring: false,
            curve: None,
            edge: bead_core::Edge::Top,
            slide_distance: None,
            scale_from: None,
            fade: true,
            reveal: false,
            progress: 1.,
            parts: Vec::new(),
            art: None,
            body: None,
        });
        scene.offset = (32., -18.);
        assert_eq!(content_input_rect(&scene), Some([32, 32, 200, 50]));
        assert_eq!(content_window_rect(&scene), Some([32, 32, 200, 100]));
        scene.offset = (32., -100.);
        assert_eq!(content_input_rect(&scene), Some([32, 32, 200, 0]));
        assert_eq!(content_window_rect(&scene), Some([32, 32, 200, 100]));
        scene.offset = (32., 32.);
        assert_eq!(content_input_rect(&scene), Some([32, 32, 200, 100]));
    }

    #[test]
    fn promotion_body_is_clipped_to_the_committed_root_extent() {
        assert_eq!(
            clip_body_rect_to_surface([32, 32, 200, 1200], (1164, 632)),
            Some([32, 32, 200, 600]),
            "a tall body is promoted only through pixels already in the layer root"
        );
        assert_eq!(
            clip_body_rect_to_surface([-20, -10, 200, 100], (500, 300)),
            Some([0, 0, 180, 90])
        );
        assert_eq!(
            clip_body_rect_to_surface([600, 0, 100, 100], (500, 300)),
            None
        );
    }

    #[test]
    fn promotion_geometry_stays_logical_at_fractional_scale_1_5() {
        let configured = (1164, 632);
        let natural = (1164, 654);
        let promoted_extent = effective_render_size(configured, true, Some(natural));
        assert_eq!(promoted_extent, natural);
        assert_eq!(
            effective_render_size(configured, false, Some(natural)),
            configured
        );
        let physical = buffer_dimensions(promoted_extent.0, promoted_extent.1, 180).unwrap();
        let docked_physical = buffer_dimensions(configured.0, configured.1, 180).unwrap();
        assert_eq!((physical.0, physical.1), (1746, 981));
        assert_eq!((docked_physical.0, docked_physical.1), (1746, 948));
        assert_eq!(
            policy::panel_viewport_from_output(Some((720, 900)), configured),
            (720, 900)
        );
        assert_eq!(
            clip_body_rect_to_surface([32, 32, 200, 600], promoted_extent),
            Some([32, 32, 200, 600]),
            "body geometry stays in logical units at scale 1.5"
        );
    }

    #[test]
    fn promoted_body_size_uses_frozen_origin_not_root_height() {
        let mut scene = scene();
        scene.layers[0].bounds = Rect::new(0., 0., 200., 700.);
        scene.bounds = Rect::new(-32., -32., 1164., 654.);
        scene.offset = (32., 32.);
        stabilize_body_origin(&mut scene, (32, 32));
        let root_size = natural_surface_size(&scene).unwrap();
        assert_eq!(root_size, (1164, 654));
        let body = visible_promoted_body_size(&scene, root_size).unwrap();
        assert_eq!(body, (200, 622));
        assert!(body.1 + 32 <= root_size.1);
        assert_ne!(
            body.1, root_size.1,
            "protocol size is the body, not the root extent"
        );
    }

    #[test]
    fn promoted_scene_updates_keep_the_body_at_its_original_surface_origin() {
        let mut scene = scene();
        scene.offset = (57., 81.);
        stabilize_body_origin(&mut scene, (32, 32));
        assert_eq!(content_input_rect(&scene), Some([32, 32, 200, 100]));
    }

    #[test]
    fn restored_promotion_waits_for_a_configured_companion_and_cleared_intent_stays_cleared() {
        assert!(!should_stage_command_promotion(true, true, false, true));
        assert!(should_stage_command_promotion(true, true, true, true));
        assert!(!should_stage_command_promotion(false, true, true, true));
    }
}
#[cfg(test)]
mod island_input_tests {
    use super::*;
    use bead_render::Rect;
    #[test]
    fn islands_follow_the_scene_offset_round_outward_and_keep_hosted_lips() {
        let islands = [Rect::new(10.25, 0., 80., 40.5), Rect::new(0., 0., 0., 40.)];
        assert_eq!(
            island_input_rects(&islands, (32., 32.), &[[5, 6, 7, 8]]),
            [[42, 32, 81, 41], [5, 6, 7, 8]],
            "an empty island drops out; fractional edges round outward"
        );
        assert!(island_input_rects(&[], (32., 32.), &[]).is_empty());
    }
    #[test]
    fn same_layout_gives_the_same_list_and_a_moved_pill_a_new_one() {
        let at = |x| island_input_rects(&[Rect::new(x, 0., 80., 40.)], (32., 32.), &[]);
        assert_eq!(at(10.), at(10.), "unchanged layout: nothing to apply");
        assert_ne!(at(10.), at(11.));
    }
    #[test]
    fn the_region_action_keeps_sets_and_resets_when_the_prop_goes_away() {
        let islands = [Rect::new(0., 0., 10., 10.)];
        let rects = vec![[32, 32, 10, 10]];
        let act = |i: Option<&[Rect]>, applied: Option<&[[i32; 4]]>| {
            island_input_action(i, (32., 32.), &[], applied)
        };
        assert_eq!(act(None, None), IslandInput::Keep, "never opted in");
        assert_eq!(act(Some(&islands), None), IslandInput::Set(rects.clone()));
        assert_eq!(act(Some(&islands), Some(&rects)), IslandInput::Keep);
        assert_eq!(
            act(None, Some(&rects)),
            IslandInput::Reset,
            "live toggle back"
        );
    }
    #[test]
    fn fractional_scale_offsets_stay_in_logical_pixels() {
        // 1.25x: the offset is a logical 32.8; a 20.8-wide island ends at 53.6 and rounds out.
        assert_eq!(
            island_input_rects(&[Rect::new(0., 0., 20.8, 10.)], (32.8, 0.), &[]),
            [[32, 0, 22, 10]]
        );
    }
}
