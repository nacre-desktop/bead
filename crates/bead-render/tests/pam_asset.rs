//! A raw PAM frame (no PNG encode or decode) rasters to its exact pixels in a Dense scene.
use bead_render::tree::{CpuResourcesPending, TreeCpuRenderer};
use bead_render::{Damage, PixelFormat, Primitive, Rect, Renderer, Scene, ShmTarget};
use std::sync::Arc;

#[test]
fn a_pam_rgba_asset_rasters_to_its_pixels() {
    let mut pam =
        b"P7\nWIDTH 2\nHEIGHT 1\nDEPTH 4\nMAXVAL 255\nTUPLTYPE RGB_ALPHA\nENDHDR\n".to_vec();
    pam.extend([255, 0, 0, 255, 0, 0, 255, 255]);
    let mut scene = Scene::empty(2., 1.);
    scene
        .primitives
        .push(Primitive::image(Rect::new(0., 0., 2., 1.), "frame.pam"));
    scene.asset_data.insert("frame.pam".into(), Arc::from(pam));
    let mut renderer = TreeCpuRenderer::default();
    for _ in 0..2 {
        if let Err(pending) = renderer.prepare_on_ui(&scene, 1.) {
            let request = pending
                .downcast_ref::<CpuResourcesPending>()
                .unwrap()
                .request
                .clone();
            assert!(request.mark_requested());
            request.prepare().unwrap();
        }
    }
    renderer.prepare_on_ui(&scene, 1.).unwrap();
    let mut bytes = vec![7u8; 8];
    let target = ShmTarget {
        bytes: &mut bytes,
        width: 2,
        height: 1,
        scale: 1.,
        format: PixelFormat::Rgba,
    };
    renderer.render(&scene, target, Damage::full(2, 1)).unwrap();
    assert_eq!(bytes, [255, 0, 0, 255, 0, 0, 255, 255]);
}
