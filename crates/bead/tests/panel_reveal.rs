// SPDX-License-Identifier: MIT
//! A revealing folder-app popup draws its first line at a folder view's 0 px margin and at Luau
//! layout's 32 px shadow margin: worker bake, reveal pose and damage-only repaints, as presented.
use bead::host::{FolderServices, FolderView};
use bead::render::{self, PixelFormat, Rect, RendererKind, ShmTarget, damage::FrameDamage};
use bead::wayland::{SceneSource, UiEvent};
use std::path::PathBuf;

#[test]
fn status_peek_draws_its_first_line_while_revealing_and_at_rest() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/bar-with-panels");
    let services = FolderServices {
        desktop: ["clock".to_string(), "audio".to_string()].into(),
        commands: Default::default(),
    };
    let mut view = FolderView::load_with_services(
        root,
        "view/status.luau".into(),
        "palette.json".into(),
        "Status",
        (488, 332),
        services,
    )
    .unwrap()
    .with_panel_actions(None);
    view.stage(UiEvent::Reload).unwrap().unwrap();
    view.commit();
    let clock = [(
        "desktop.clock".to_string(),
        bead::core::Value::Text("12:34".into()),
    )];
    let scene = view.stage(UiEvent::Model(clock.into())).unwrap().unwrap();
    view.commit();
    // The first line: 16 px into the 300x120 body that hangs at x 94 from the top, 26 px tall.
    let body = scene.motion.as_ref().and_then(|m| m.body).unwrap();
    assert_eq!(body, Rect::new(94., 0., 300., 120.));
    let scale = 1.5;
    for margin in [0., 32.] {
        let mut scene = scene.clone();
        scene.bounds = Rect::new(-margin, -margin, 488. + 2. * margin, 332. + 2. * margin);
        let mut cache = render::tree::TreeCache::default();
        cache.prepare(&scene, scale).unwrap();
        scene.prepared.insert(scale.to_bits(), cache.frame.unwrap());
        let (w, h) = (
            (scene.bounds.w * scale) as u32,
            (scene.bounds.h * scale) as u32,
        );
        let mut pixels = vec![0; (w * h * 4) as usize];
        let (mut renderer, mut damage) = (
            render::renderer(RendererKind::Cpu).unwrap(),
            FrameDamage::default(),
        );
        for progress in [0., 0.2, 0.4, 0.7, 1., 1.] {
            let (dx, dy) = scene.panel_offset(progress, true);
            // The presenter's opening offset: the scene's own margin plus the motion pose.
            scene.offset = (margin + dx, margin + dy);
            renderer.prepare_on_ui(&scene, scale).unwrap();
            let frame = renderer.prepared_frame().unwrap();
            for region in damage.update(&frame, scene.offset, 1., w, h).unwrap() {
                let target = ShmTarget {
                    bytes: &mut pixels,
                    width: w,
                    height: h,
                    scale,
                    format: PixelFormat::Rgba,
                };
                renderer.render(&scene, target, region).unwrap();
            }
            // Primary-coloured (81d4bd) text pixels inside the first line's box.
            let at = |v: f32| ((v + margin) * scale) as u32;
            let ink = (at(16.)..at(42.))
                .flat_map(|y| (at(110.)..at(300.)).map(move |x| ((y * w + x) * 4) as usize))
                .filter(|&i| pixels[i + 1] > 150 && pixels[i + 2] > 120 && pixels[i] < 160)
                .count();
            // Closed nothing shows; once the reveal passes the line's bottom (42 of 120 px) and
            // at rest, the whole line does (about 340 pixels of "12:34").
            if progress == 0. || progress >= 0.4 {
                let shown = ink > 150;
                assert_eq!(
                    shown,
                    progress > 0.,
                    "margin {margin} progress {progress}: {ink}"
                );
            }
        }
    }
}
