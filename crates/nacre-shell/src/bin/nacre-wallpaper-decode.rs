// SPDX-License-Identifier: GPL-3.0-only
//! Private companion process: one bounded request, raw RGBA response, then exit.
#[path = "../wallpaper_decode.rs"]
mod wallpaper_decode;
use std::io::{Read, Write};
fn main() {
    if decode().is_err() {
        std::process::exit(1);
    }
}
fn decode() -> Result<(), Box<dyn std::error::Error>> {
    let mut request = Vec::new();
    std::io::stdin().take(131_073).read_to_end(&mut request)?;
    if request.len() > 131_072 {
        return Err("wallpaper request exceeds bound".into());
    }
    let (pair, width, height) = serde_json::from_slice(&request)?;
    let rgba = wallpaper_decode::decode_pair(pair, width, height)?;
    std::io::stdout().write_all(&rgba)?;
    Ok(())
}
