// SPDX-License-Identifier: GPL-3.0-only
//! Bound large transient rasters without mapping ordinary paint masks per frame.

#[cfg(all(target_os = "linux", target_env = "gnu"))]
const MMAP_THRESHOLD: i32 = 4 * 1024 * 1024;

pub fn configure() {
    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    {
        // Respect explicit mmap/trim configuration and allocator tunables. This application
        // policy changes neither the framework nor any child process's policy.
        let explicit = [
            "MALLOC_MMAP_THRESHOLD_",
            "MALLOC_TRIM_THRESHOLD_",
            "MALLOC_MMAP_MAX_",
        ]
        .iter()
        .any(|key| std::env::var_os(key).is_some())
            || std::env::var_os("GLIBC_TUNABLES").is_some_and(|value| {
                value
                    .to_string_lossy()
                    .split(':')
                    .any(|part| part.starts_with("glibc.malloc."))
            });
        if !explicit {
            // SAFETY: called once before application threads start. The value
            // is a supported glibc mallopt threshold, not an allocator hook.
            if unsafe { nix::libc::mallopt(nix::libc::M_MMAP_THRESHOLD, MMAP_THRESHOLD) } == 0 {
                eprintln!("MEMORY_POLICY_ERROR code=mmap_threshold");
            }
        }
    }
}

#[cfg(all(test, feature = "trace", target_os = "linux", target_env = "gnu"))]
mod tests {
    #[tokio::test(flavor = "current_thread")]
    async fn extras_retirement_releases_topic_data_and_subscriptions() {
        crate::extras::tests::retirement().await;
    }

    #[test]
    fn weather_hero_image_bytes_return_to_zero_after_close() -> bead_test::Result<()> {
        use bead::render::{Primitive, Renderer, tree::TreeCpuRenderer};
        use std::{collections::BTreeMap, path::Path, sync::Arc};

        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../shell");
        let panel = bead_test::Harness::load(
            &root,
            Path::new("dashboard/weather/main.luau"),
            &root.join("palette.json"),
            BTreeMap::from([
                (
                    "dashboard.weather.code".into(),
                    bead::core::Value::Number(3.),
                ),
                (
                    "dashboard.weather.day".into(),
                    bead::core::Value::Bool(true),
                ),
            ]),
            Default::default(),
        )?;
        let mut renderer = TreeCpuRenderer::default();
        renderer.prepare(panel.scene(), 1.5)?;
        let frame = renderer
            .prepared_frame()
            .ok_or("unprepared Weather frame")?;
        let images: Vec<_> = frame
            .items
            .iter()
            .filter(|item| matches!(item.primitive, Primitive::Image { .. }))
            .filter_map(|item| item.raster.as_ref())
            .map(|raster| {
                // Existing SVG decoding is display-sized, including fractional output scale.
                assert_eq!((raster.pixels.width(), raster.pixels.height()), (846, 447));
                Arc::downgrade(&raster.pixels)
            })
            .collect();
        let live_bytes = || {
            images
                .iter()
                .filter_map(std::sync::Weak::upgrade)
                .map(|pixels| pixels.data().len())
                .sum::<usize>()
        };
        assert_eq!(images.len(), 2, "both hero layers must have decoded pixels");
        assert_eq!(live_bytes(), 2 * 846 * 447 * 4);
        // release_panel drops the source/scene and the companion renderer; prepared
        // frames share the pixels, so neither cache eviction nor hiding is enough.
        drop(panel);
        drop(renderer);
        assert!(
            live_bytes() > 0,
            "the held presented frame must own its images"
        );
        drop(frame);
        assert_eq!(
            live_bytes(),
            0,
            "image bytes retained after panel retirement"
        );
        println!("MEMORY_POLICY_WEATHER_IMAGE_RELEASE open_bytes=3025296 closed_bytes=0");
        Ok(())
    }

    #[test]
    fn large_transients_return_their_mapping_after_repeated_release() {
        // Isolate the process-wide policy and accounting from parallel tests.
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "memory_policy::tests::allocation_child",
                "--ignored",
            ])
            .env("BEAD_ALLOCATOR_TEST", "1")
            .env_remove("GLIBC_TUNABLES")
            .env_remove("MALLOC_MMAP_THRESHOLD_")
            .env_remove("MALLOC_TRIM_THRESHOLD_")
            .env_remove("MALLOC_MMAP_MAX_")
            .status()
            .unwrap();
        assert!(status.success());
    }

    #[test]
    #[ignore = "private child of the isolated allocator test"]
    fn allocation_child() {
        assert_eq!(std::env::var("BEAD_ALLOCATOR_TEST").as_deref(), Ok("1"));
        // A freed large allocation can raise glibc's adaptive mmap threshold.
        let warmup = vec![1_u8; 8 * 1024 * 1024];
        std::hint::black_box(&warmup);
        drop(warmup);
        super::configure();
        // A content-sized launcher's byte mask is below4MiB: keep ordinary
        // per-frame scratch on the reusable heap, unlike the rejected1MiB policy.
        let before = unsafe { nix::libc::mallinfo2() };
        let mask = vec![1_u8; 1536 * 1024];
        std::hint::black_box(&mask);
        let during = unsafe { nix::libc::mallinfo2() };
        assert_eq!(during.hblkhd, before.hblkhd);
        drop(mask);
        for _ in 0..3 {
            // SAFETY: read-only by-value glibc accounting, trace tests only.
            let before = unsafe { nix::libc::mallinfo2() };
            let bytes = vec![1_u8; 5 * 1024 * 1024];
            std::hint::black_box(&bytes);
            let during = unsafe { nix::libc::mallinfo2() };
            assert!(during.hblkhd >= before.hblkhd + bytes.len());
            drop(bytes);
            let after = unsafe { nix::libc::mallinfo2() };
            assert_eq!(after.hblkhd, before.hblkhd);
        }
    }
}
