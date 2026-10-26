// SPDX-License-Identifier: GPL-3.0-only
use image::ImageEncoder;
use nacre_protocol::wallpaper::{ActiveWallpaper, WallpaperFileIdentity};
use std::{
    io::Write,
    os::unix::fs::PermissionsExt,
    process::{Command, Stdio},
};

#[test]
fn worker_returns_exact_pixels_and_rejects_changed_identity_and_oversized_output() {
    let root = std::env::temp_dir().join(format!("bead-wallpaper-worker-{}", std::process::id()));
    std::fs::create_dir(&root).unwrap();
    std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o700)).unwrap();
    let path = root.join("poster.png");
    let mut png = Vec::new();
    image::codecs::png::PngEncoder::new(&mut png)
        .write_image(&[30, 80, 120, 255], 1, 1, image::ExtendedColorType::Rgba8)
        .unwrap();
    std::fs::write(&path, png).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
    let pair: ActiveWallpaper = serde_json::from_value(serde_json::json!({
        "revision":1, "palette_revision":1, "path":path,
        "file_identity":WallpaperFileIdentity::from_metadata(&std::fs::metadata(&path).unwrap()),
        "presentation":"crop", "palette":{"mode":"dark", "colours":{"background":"#101820", "onSurface":"#ffffff", "primary":"#2255aa"}}
    })).unwrap();
    let run = |pair: &ActiveWallpaper, width, height, profile: bool| {
        let mut command = Command::new(env!("CARGO_BIN_EXE_nacre-wallpaper-decode"));
        command
            .env_remove("BEAD_PROFILE_INPUT")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if profile {
            command.env("BEAD_PROFILE_INPUT", "1");
        }
        let mut child = command.spawn().unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(&serde_json::to_vec(&(pair, width, height)).unwrap())
            .unwrap();
        child.wait_with_output().unwrap()
    };
    let output = run(&pair, 2, 1, true);
    assert!(output.status.success());
    assert_eq!(output.stdout, [30, 80, 120, 255, 30, 80, 120, 255]);
    let diagnostics = String::from_utf8(output.stderr).unwrap();
    assert!(diagnostics.contains("INPUT_PROFILE stage=wallpaper_png"));
    assert!(diagnostics.contains("INPUT_PROFILE stage=wallpaper_rgba_output"));
    assert!(!run(&pair, 16_385, 1, false).status.success());
    std::fs::write(&path, b"changed").unwrap();
    let output = run(&pair, 2, 1, false);
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    std::fs::remove_dir_all(root).unwrap();
}
