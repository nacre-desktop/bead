// SPDX-License-Identifier: MIT
use std::process::Command;

#[test]
fn errors_print_readable_multiline_text_and_exit_one() {
    let folder = std::env::temp_dir().join(format!("bead-cli-errors-{}", std::process::id()));
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(folder.join("bead.toml"), "id = 'x.y'\ntitle = \n").unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_bead"))
        .arg("check")
        .arg(&folder)
        .output()
        .unwrap();
    let _ = std::fs::remove_dir_all(&folder);
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert_eq!(out.status.code(), Some(1), "{stderr}");
    assert!(stderr.starts_with("Error: "), "{stderr}");
    assert!(stderr.trim_end().lines().count() > 1, "{stderr}");
    assert!(
        !stderr.contains("\\n") && !stderr.contains("Error: \""),
        "{stderr}"
    );
}
