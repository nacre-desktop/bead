// SPDX-License-Identifier: GPL-3.0-only
//! Prepare bounded CPU panel content after startup without creating surfaces.
use bead::wayland::{PanelSpec, UiEvent};
use std::thread;

pub(crate) fn start(specs: Vec<(String, PanelSpec)>, viewport: (u32, u32), scale: f32) {
    if specs.is_empty() {
        return;
    }
    let _ = thread::Builder::new()
        .name("bead-panel-prewarm".into())
        .spawn(move || {
            // The compositor calls this callback only after the first primary
            // frame has settled; keep the worker below foreground priority.
            #[cfg(target_os = "linux")]
            // SAFETY: PRIO_PROCESS with id zero adjusts only this calling thread.
            unsafe {
                libc::setpriority(libc::PRIO_PROCESS, 0, 10);
            }
            let mut warmed = 0;
            let mut bytes = 0usize;
            let requested = specs.len();
            for (name, spec) in specs {
                match warm_spec(&spec, viewport, scale) {
                    Ok(size) => {
                        warmed += usize::from(size > 0);
                        bytes = bytes.saturating_add(size);
                        eprintln!("PANEL_STARTUP_PREWARM name={name} result=ready bytes={size} scale={scale} viewport={}x{}", viewport.0, viewport.1);
                    }
                    Err(error) => {
                        eprintln!("PANEL_STARTUP_PREWARM name={name} result=failed reason={error}");
                    }
                }
            }
            let (_, _, retained) = bead::render::tree::cpu_content_warm_cache_stats();
            eprintln!("PANEL_STARTUP_PREWARM_SUMMARY requested={requested} warmed={warmed} bytes_added={bytes} cache_bytes={retained}");
        });
}

fn warm_spec(
    spec: &PanelSpec,
    viewport: (u32, u32),
    scale: f32,
) -> Result<usize, Box<dyn std::error::Error>> {
    let mut source = (spec.factory)(true).map_err(std::io::Error::other)?;
    source.services(Box::new(|_| {}));
    source.viewport(viewport.0, viewport.1);
    source.scales(vec![scale]);
    let mut scene = source
        .stage(UiEvent::Reload)?
        .ok_or("initial shell view produced no scene")?;
    source.validate()?;
    scene.reveal(0.);

    let mut cache = bead::render::tree::TreeCache::default();
    cache.prepare(&scene, scale)?;
    let retained = bead::render::tree::TreeCpuRenderer::prewarm_content(&scene, scale)?;
    source.commit();
    Ok(retained)
}
