// SPDX-License-Identifier: GPL-3.0-only
//! Keep large transient UI rasters out of glibc's reusable thread arenas.

#[cfg(all(target_os = "linux", target_env = "gnu"))]
const MMAP_THRESHOLD: i32 = 1024 * 1024;

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
        for _ in 0..3 {
            // SAFETY: read-only by-value glibc accounting, trace tests only.
            let before = unsafe { nix::libc::mallinfo2() };
            let bytes = vec![1_u8; 1536 * 1024];
            std::hint::black_box(&bytes);
            let during = unsafe { nix::libc::mallinfo2() };
            assert!(during.hblkhd >= before.hblkhd + bytes.len());
            drop(bytes);
            let after = unsafe { nix::libc::mallinfo2() };
            assert_eq!(after.hblkhd, before.hblkhd);
        }
    }
}
