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

        // Avoid implicit transparent huge pages for future anonymous mappings while
        // preserving callers' explicit MADV_HUGEPAGE/VM_HUGEPAGE requests. Do not
        // overwrite a parent policy that already disabled THP.
        // libc 0.2.190 does not export these THP prctl constants for GNU/Linux;
        // values are from the kernel UAPI in include/uapi/linux/prctl.h and
        // /usr/include/linux/prctl.h. See https://docs.kernel.org/admin-guide/mm/transhuge.html.
        const PR_SET_THP_DISABLE: nix::libc::c_int = 41;
        const PR_GET_THP_DISABLE: nix::libc::c_int = 42;
        // PR_THP_DISABLE_EXCEPT_ADVISED preserves MADV_HUGEPAGE / VM_HUGEPAGE.
        const PR_THP_DISABLE_EXCEPT_ADVISED: nix::libc::c_ulong = 1 << 1;
        // SAFETY: `prctl` receives the Linux UAPI integer option and unsigned-long
        // arguments. A zero result means no inherited THP-disable mode is active.
        let thp_mode = unsafe {
            nix::libc::prctl(
                PR_GET_THP_DISABLE,
                0 as nix::libc::c_ulong,
                0 as nix::libc::c_ulong,
                0 as nix::libc::c_ulong,
                0 as nix::libc::c_ulong,
            )
        };
        if thp_mode == 0 {
            // Keep explicit huge-page advice (MADV_HUGEPAGE / VM_HUGEPAGE) available.
            // SAFETY: option and arguments follow Linux <linux/prctl.h>; failure is
            // best-effort only and must not prevent the shell from starting.
            let _ = unsafe {
                nix::libc::prctl(
                    PR_SET_THP_DISABLE,
                    1 as nix::libc::c_ulong,
                    PR_THP_DISABLE_EXCEPT_ADVISED,
                    0 as nix::libc::c_ulong,
                    0 as nix::libc::c_ulong,
                )
            };
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
        assert_eq!(images.len(), 1, "the shaded illustration is one raster");
        assert_eq!(live_bytes(), 846 * 447 * 4);
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
        println!("MEMORY_POLICY_WEATHER_IMAGE_RELEASE open_bytes=1512648 closed_bytes=0");
        Ok(())
    }

    /// N0056: the leased city photo's decoded raster follows the same rule as the local hero.
    #[test]
    fn weather_daemon_photo_pixels_return_to_zero_after_close() -> bead_test::Result<()> {
        use bead::{
            core::Value,
            render::{Primitive, Renderer, tree::TreeCpuRenderer},
        };
        use std::{
            collections::BTreeMap,
            path::{Path, PathBuf},
            sync::Arc,
        };

        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../shell");
        let dir = std::env::temp_dir().join(format!("bead-photo-memory-{}", std::process::id()));
        std::fs::create_dir_all(&dir)?;
        let photo = dir.join("city.image");
        image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(
            960,
            720,
            image::Rgb([20, 90, 160]),
        ))
        .save_with_format(&photo, image::ImageFormat::Jpeg)?;
        let panel = bead_test::Harness::load_with_icons(
            &root,
            Path::new("dashboard/weather/main.luau"),
            &root.join("palette.json"),
            BTreeMap::from([
                ("dashboard.weather.photo".into(), Value::Bool(true)),
                ("dashboard.weather.photo_w".into(), Value::Number(960.)),
                ("dashboard.weather.photo_h".into(), Value::Number(720.)),
                (
                    "dashboard.weather.photo_author".into(),
                    Value::Text("Fixture".into()),
                ),
                (
                    "dashboard.weather.photo_license".into(),
                    Value::Text("CC0".into()),
                ),
            ]),
            Default::default(),
            BTreeMap::from([("weather-photo".to_owned(), PathBuf::from(&photo))]),
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
            .map(|raster| Arc::downgrade(&raster.pixels))
            .collect();
        let live_bytes = || {
            images
                .iter()
                .filter_map(std::sync::Weak::upgrade)
                .map(|pixels| pixels.data().len())
                .sum::<usize>()
        };
        assert_eq!(images.len(), 2, "photo and shade must have decoded pixels");
        let open = live_bytes();
        assert!(open > 0);
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
            "photo bytes retained after panel retirement"
        );
        println!("MEMORY_POLICY_WEATHER_PHOTO_RELEASE open_bytes={open} closed_bytes=0");
        std::fs::remove_dir_all(dir)?;
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
