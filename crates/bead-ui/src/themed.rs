// SPDX-License-Identifier: MIT OR Apache-2.0
//! Palette-templated SVG images (B0056). A file named `*.themed.svg` carries `@role@` tokens (a palette
//! role) or `@role+@` (the role lifted to readable contrast on the surface); the live palette fills them
//! when a scene is materialized, so an icon follows the wallpaper palette like any palette fill.
use bead_render::Palette;
use std::path::{Path, PathBuf};

fn luminance([r, g, b, _]: [u8; 4]) -> f64 {
    let lin = |v: u8| {
        let v = f64::from(v) / 255.;
        if v <= 0.04045 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    };
    lin(r) * 0.2126 + lin(g) * 0.7152 + lin(b) * 0.0722
}
fn contrast(a: [u8; 4], b: [u8; 4]) -> f64 {
    let (a, b) = (luminance(a), luminance(b));
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}
/// `NacreLogo`'s lift: blend toward the readable ink in 20 steps until the colour reaches `minimum` contrast on `bg`.
fn lift(value: [u8; 4], bg: [u8; 4], fg: [u8; 4], minimum: f64) -> [u8; 4] {
    if contrast(value, bg) >= minimum {
        return value;
    }
    let target = if contrast(fg, bg) >= minimum {
        fg
    } else if contrast([255; 4], bg) >= contrast([0, 0, 0, 255], bg) {
        [255; 4]
    } else {
        [0, 0, 0, 255]
    };
    for step in 1..=20 {
        let mix = |i: usize| {
            (f64::from(value[i])
                + (f64::from(target[i]) - f64::from(value[i])) * f64::from(step) / 20.)
                .round() as u8
        };
        let mixed = [mix(0), mix(1), mix(2), 255];
        if contrast(mixed, bg) >= minimum {
            return mixed;
        }
    }
    target
}
/// Fill a themed SVG template from the live palette and register it as an in-memory asset.
pub(crate) fn themed_asset(
    path: &Path,
    palette: &Palette,
    assets: &mut std::collections::BTreeMap<PathBuf, std::sync::Arc<[u8]>>,
) -> Result<PathBuf, String> {
    use std::hash::{Hash, Hasher};
    let bg = palette.get("surface").map_err(|e| e.to_string())?;
    let fg = palette.get("onSurface").unwrap_or(bg);
    let bytes = assets
        .get(path)
        .ok_or_else(|| format!("themed image {} is not in the snapshot", path.display()))?;
    let text = std::str::from_utf8(bytes)
        .map_err(|e| format!("themed image {}: {e}", path.display()))?
        .to_owned();
    let mut out = String::with_capacity(text.len());
    let mut rest = text.as_str();
    while let Some(start) = rest.find('@') {
        out.push_str(&rest[..start]);
        let tail = &rest[start + 1..];
        let end = tail.find('@').ok_or("themed image: unterminated @token@")?;
        let (name, lifted) = tail[..end]
            .strip_suffix('+')
            .map_or((&tail[..end], false), |n| (n, true));
        let color = palette.get(name).map_err(|e| e.to_string())?;
        let color = if lifted {
            lift(color, bg, fg, 3.4)
        } else {
            color
        };
        out.push_str(&format!(
            "#{:02x}{:02x}{:02x}",
            color[0], color[1], color[2]
        ));
        rest = &tail[end + 1..];
    }
    out.push_str(rest);
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    out.hash(&mut hash);
    let name = PathBuf::from(format!("@themed-{:x}.svg", hash.finish()));
    assets
        .entry(name.clone())
        .or_insert_with(|| std::sync::Arc::from(out.into_bytes()));
    Ok(name)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn palette(roles: &[(&str, [u8; 4])]) -> Palette {
        Palette(std::sync::Arc::new(
            roles.iter().map(|(k, v)| (k.to_string(), *v)).collect(),
        ))
    }
    fn fill(svg: &str, palette: &Palette) -> String {
        let path = PathBuf::from("a.themed.svg");
        let mut assets = std::collections::BTreeMap::from([(
            path.clone(),
            std::sync::Arc::from(svg.as_bytes()),
        )]);
        let name = themed_asset(&path, palette, &mut assets).unwrap();
        String::from_utf8(assets[&name].to_vec()).unwrap()
    }
    #[test]
    fn tokens_take_palette_colours_and_lift_only_when_marked() {
        let dark = palette(&[
            ("surface", [9, 19, 18, 255]),
            ("onSurface", [224, 239, 236, 255]),
            ("primary", [20, 60, 55, 255]),
        ]);
        assert_eq!(fill("<a fill='@primary@'/>", &dark), "<a fill='#143c37'/>");
        let lifted = fill("<a fill='@primary+@'/>", &dark);
        assert_ne!(lifted, "<a fill='#143c37'/>");
        // The lift reaches the contrast it promises.
        let hex = u32::from_str_radix(&lifted[10..16], 16).unwrap();
        let color = [(hex >> 16) as u8, (hex >> 8) as u8, hex as u8, 255];
        assert!(contrast(color, [9, 19, 18, 255]) >= 3.4);
        assert_eq!(fill("<a fill='@primary+@'/>", &dark), lifted);
    }
    #[test]
    fn an_unknown_role_or_open_token_is_an_error() {
        let p = palette(&[("surface", [0, 0, 0, 255])]);
        let path = PathBuf::from("a.themed.svg");
        for svg in ["@nope@", "@primary"] {
            let mut assets = std::collections::BTreeMap::from([(
                path.clone(),
                std::sync::Arc::from(svg.as_bytes()),
            )]);
            assert!(themed_asset(&path, &p, &mut assets).is_err());
        }
    }
}
