// SPDX-License-Identifier: MIT
//! Manual baseline harness. Run with `BEAD_PROFILE_INPUT=1 cargo test -p bead-ui --features test-counters scroll_measure -- --ignored --nocapture`.
use super::*;
use bead_render::{Damage, RendererKind, ShmTarget, damage::FrameDamage};
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::sync::atomic::{AtomicUsize, Ordering};

thread_local! { static COUNTING: Cell<bool> = const { Cell::new(false) }; }
static ALLOCS: AtomicUsize = AtomicUsize::new(0);
struct CountAlloc;
unsafe impl GlobalAlloc for CountAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        COUNTING.with(|on| {
            if on.get() {
                ALLOCS.fetch_add(1, Ordering::Relaxed);
            }
        });
        // SAFETY: delegated unchanged to the system allocator.
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        COUNTING.with(|on| {
            if on.get() {
                ALLOCS.fetch_add(1, Ordering::Relaxed);
            }
        });
        unsafe { System.realloc(ptr, layout, size) }
    }
}
#[global_allocator]
static ALLOCATOR: CountAlloc = CountAlloc;

fn write_case(root: &std::path::Path, virtualized: bool) {
    std::fs::create_dir_all(root.join("ui")).unwrap();
    std::fs::write(
        root.join("palette.json"),
        r#"{"formatVersion":1,"colours":{"surface":"101010"}}"#,
    )
    .unwrap();
    let rows = if virtualized { 1000 } else { 300 };
    let mut behavior = String::from("return {stores={scroll=0,rows={");
    for i in 0..rows {
        behavior.push_str(&format!(
            "{{id='r{i}',name='Row {i} mixed text with sample payload'}},"
        ));
    }
    behavior.push_str("}},actions={},computed={}}\n");
    std::fs::write(root.join("ui/behavior.luau"), behavior).unwrap();
    let view = if virtualized {
        r#"local ui=require('bead.ui');local virtual_list=require('bead.widgets.kit.virtual-list')
return ui.Box{id='root',width=640,height=480,virtual_list{id='list',width=640,height=480,row_height=32,gap=2,scroll=ui.bind('scroll'),model=ui.bind('rows'),key='id',render=function(item)return ui.Text{id='row',text=item.name,width=640,height=32}end}}"#.to_owned()
    } else {
        let mut s = String::from(
            "local ui=require('bead.ui')\nreturn ui.Scroll{id='list',scroll=ui.bind('scroll'),width=640,height=480,ui.Column{id='rows',width=640,",
        );
        for i in 0..rows {
            s.push_str(&format!("ui.Text{{id='r{i}',text='Row {i} mixed text payload with varying length {}',width=640,height={}}},", "x".repeat(i % 47), 20 + (i % 13)));
        }
        s.push_str("}}\n");
        s
    };
    std::fs::write(root.join("ui/main.luau"), view).unwrap();
}

#[test]
#[ignore = "manual synthetic CPU scroll baseline; run explicitly with --ignored --nocapture"]
fn scroll_measure_baseline_300_rows_and_virtual_1000() {
    let base = std::env::temp_dir().join(format!("bead-scroll-measure-{}", std::process::id()));
    for virtualized in [false, true] {
        let root = base.join(if virtualized { "virtual" } else { "rows" });
        write_case(&root, virtualized);
        let fallback = bead_render::Scene {
            clips: Default::default(),
            opacity: 1.,
            motion: None,
            resource_revision: 0,
            asset_data: Default::default(),
            layers: vec![],
            prepared: Default::default(),
            font_files: vec![],
            width: 640.,
            slide: None,
            primitives: vec![],
            offset: (0., 0.),
            bounds: bead_render::Rect::new(0., 0., 640., 480.),
        };
        let mut session = Session::new(
            root.clone(),
            "ui/main.luau".into(),
            root.join("palette.json"),
            Palette::read(&root.join("palette.json")).unwrap(),
            fallback,
        );
        session.set_viewport(640, 480);
        let scene = session.reload().unwrap();
        session.commit(scene.clone());
        let mut renderer = bead_render::renderer(RendererKind::Cpu).unwrap();
        renderer.prepare(&scene, 1.).unwrap();
        let mut damage_tracker = FrameDamage::default();
        let prepared = renderer.prepared_frame().unwrap();
        damage_tracker
            .update(&prepared, scene.offset, scene.opacity, 640, 480)
            .unwrap();
        let mut pixels = vec![0u8; 640 * 480 * 4];
        let mut samples = Vec::new();
        let mut last_layout = layout::layout_passes();
        let mut last_materialize = generation::materialize_calls();
        let mut last_build = frontend::build_calls();
        let mut layouts = Vec::new();
        for hz in [60u64, 120] {
            let frames = (125 * hz).div_ceil(1000) as usize;
            let mut delivered = 0.;
            for frame in 0..frames {
                let t = (((frame + 1) as f64 * 1_000_000_000. / hz as f64) / 125_000_000.).min(1.);
                let eased = 100. * (1. - (1. - t).powi(3));
                let delta = eased - delivered;
                delivered = eased;
                let before = std::time::Instant::now();
                ALLOCS.store(0, Ordering::Relaxed);
                COUNTING.with(|on| on.set(true));
                let next = session
                    .scroll(320., 240., delta)
                    .unwrap()
                    .unwrap_or_else(|| session.current.clone());
                let scroll_done = std::time::Instant::now();
                let stage_allocs = ALLOCS.load(Ordering::Relaxed);
                let lp = layout::layout_passes();
                let layout_delta = lp - last_layout;
                last_layout = lp;
                let materialize_delta = generation::materialize_calls() - last_materialize;
                last_materialize = generation::materialize_calls();
                let build_delta = frontend::build_calls() - last_build;
                last_build = frontend::build_calls();
                renderer.prepare(&next, 1.).unwrap();
                let prep_done = std::time::Instant::now();
                let prepare_allocs = ALLOCS.load(Ordering::Relaxed) - stage_allocs;
                let prepared = renderer.prepared_frame().unwrap();
                let damage_start = std::time::Instant::now();
                let dirty = damage_tracker
                    .update(&prepared, next.offset, next.opacity, 640, 480)
                    .unwrap();
                let damage_done = std::time::Instant::now();
                let diff_allocs = ALLOCS.load(Ordering::Relaxed) - stage_allocs - prepare_allocs;
                let damage_area: u64 = dirty.iter().map(|d| u64::from(d.w) * u64::from(d.h)).sum();
                renderer
                    .render(
                        &next,
                        ShmTarget {
                            bytes: &mut pixels,
                            width: 640,
                            height: 480,
                            scale: 1.,
                            format: bead_render::PixelFormat::Bgra,
                        },
                        Damage::full(640, 480),
                    )
                    .unwrap();
                let end = std::time::Instant::now();
                COUNTING.with(|on| on.set(false));
                session.commit(next);
                samples.push((
                    hz,
                    frame,
                    delta,
                    scroll_done.duration_since(before).as_micros(),
                    prep_done.duration_since(scroll_done).as_micros(),
                    damage_done.duration_since(damage_start).as_micros(),
                    end.duration_since(damage_done).as_micros(),
                    damage_area,
                    ALLOCS.load(Ordering::Relaxed),
                    layout_delta,
                    materialize_delta,
                    build_delta,
                    stage_allocs,
                    prepare_allocs,
                    diff_allocs,
                ));
                layouts.push(layout_delta);
            }
        }
        println!(
            "SCROLL_BASELINE virtualized={virtualized} primitives={} layout_passes={:?}",
            session.current.primitives.len(),
            layouts
        );
        for (
            hz,
            frame,
            delta,
            stage,
            prepare,
            damage_us,
            raster,
            damage_area,
            allocs,
            layout_delta,
            materialize_delta,
            build_delta,
            stage_allocs,
            prepare_allocs,
            diff_allocs,
        ) in samples
        {
            println!(
                "FRAME hz={hz} i={frame} eased_wheel_delta={delta:.3} session_scroll_us={stage} cpu_prepare_us={prepare} damage_us={damage_us} cpu_raster_us={raster} damage_area_px={damage_area} allocations={allocs} stage_allocations={stage_allocs} prepare_allocations={prepare_allocs} diff_allocations={diff_allocs} layout_passes={layout_delta} materialize_calls={materialize_delta} frontend_build_calls={build_delta}"
            );
        }
        let _ = std::fs::remove_dir_all(&root);
    }
    let _ = std::fs::remove_dir_all(base);
}
