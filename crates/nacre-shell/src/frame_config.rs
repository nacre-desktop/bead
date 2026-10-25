// SPDX-License-Identifier: GPL-3.0-only
//! B0047: which edge lips the desktop frame has and which panel each opens. This is
//! configuration, not code: `shell/frame/behavior.luau` (read from the shell bundle when
//! present, else the copy built in) declares them, so a different layout needs no rebuild.
use bead::core::{Edge, Value};
use std::path::Path;

const BUILT_IN: &str = include_str!("../../../shell/frame/behavior.luau");
const MAX_LIPS: usize = 8;

/// One declared lip. Thickness is not configured: it is the frame width of its edge.
#[derive(Clone, Debug, PartialEq)]
pub struct LipConfig {
    pub id: String,
    pub edge: Edge,
    pub length: f32,
    pub opens: String,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FrameConfig {
    pub lips: Vec<LipConfig>,
    /// Open panels that stop hover from opening a lip's panel (QML: launcher, session).
    pub blocked_by: Vec<String>,
    /// Companion views that share chrome's Top layer rather than Overlay.
    pub fullscreen_covered_overlays: Vec<String>,
}
fn name(value: &serde_json::Value, key: &str) -> Option<String> {
    value
        .get(key)?
        .as_str()
        .filter(|s| {
            !s.is_empty() && s.len() <= 64 && !s.contains('/') && !s.chars().any(char::is_control)
        })
        .map(str::to_owned)
}
fn list<'a>(
    stores: &'a std::collections::BTreeMap<String, Value>,
    id: &str,
) -> &'a [serde_json::Value] {
    match stores.get(id) {
        Some(Value::List(items)) => items,
        _ => &[],
    }
}
pub fn parse(source: &[u8], path: &Path) -> Result<FrameConfig, String> {
    let script = bead::ui::script::Script::load(source, path).map_err(|e| e.to_string())?;
    let stores = script.schema().map_err(|e| e.to_string())?;
    let mut config = FrameConfig::default();
    for item in list(&stores, "frame.lips").iter().take(MAX_LIPS) {
        let edge = match item.get("edge").and_then(|e| e.as_str()) {
            Some("top") => Edge::Top,
            Some("bottom") => Edge::Bottom,
            Some("left") => Edge::Left,
            Some("right") => Edge::Right,
            _ => return Err("frame.lips: edge must be top, bottom, left or right".into()),
        };
        let length = item
            .get("length")
            .and_then(|l| l.as_f64())
            .filter(|l| l.is_finite() && (1. ..=4000.).contains(l))
            .ok_or("frame.lips: length must be 1..4000")? as f32;
        let (Some(id), Some(opens)) = (name(item, "id"), name(item, "opens")) else {
            return Err("frame.lips: id and opens are required names".into());
        };
        if config.lips.iter().any(|l| l.id == id) {
            return Err(format!("frame.lips: duplicate id {id}"));
        }
        config.lips.push(LipConfig {
            id,
            edge,
            length,
            opens,
        });
    }
    config.blocked_by = list(&stores, "frame.lips.blocked_by")
        .iter()
        .take(MAX_LIPS)
        .filter_map(|v| name(v, "id"))
        .collect();
    config.fullscreen_covered_overlays = list(&stores, "frame.fullscreen_covered_overlays")
        .iter()
        .take(MAX_LIPS)
        .filter_map(|v| name(v, "id"))
        .collect();
    Ok(config)
}
/// The bundle's `frame/behavior.luau` under `root`, else the built-in copy. A broken file
/// keeps the built-in lips and reports why.
pub fn load(root: &Path) -> FrameConfig {
    let path = root.join("frame/behavior.luau");
    if let Ok(bytes) = std::fs::read(&path) {
        match parse(&bytes, &path) {
            Ok(config) => return config,
            Err(error) => eprintln!("FRAME_CONFIG_ERROR {error}"),
        }
    }
    parse(BUILT_IN.as_bytes(), Path::new("frame/behavior.luau")).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    fn parse_str(text: &str) -> Result<FrameConfig, String> {
        parse(text.as_bytes(), Path::new("t.luau"))
    }
    #[test]
    fn the_shipped_file_declares_the_three_qml_lips() {
        let config = parse(BUILT_IN.as_bytes(), Path::new("frame/behavior.luau")).unwrap();
        let ids: Vec<_> = config.lips.iter().map(|l| l.id.as_str()).collect();
        assert_eq!(ids, ["dashboard", "assistant", "controls"]);
        assert_eq!(config.lips[0].edge, Edge::Top);
        assert_eq!(config.lips[0].length, 208.);
        assert_eq!(config.lips[1].edge, Edge::Left);
        assert_eq!(config.lips[2].edge, Edge::Right);
        assert_eq!(config.lips[2].opens, "control");
        assert!(config.lips[1..].iter().all(|l| l.length == 144.));
        assert_eq!(config.blocked_by, ["launcher", "session"]);
        assert_eq!(config.fullscreen_covered_overlays, ["notices"]);
    }
    #[test]
    fn a_different_layout_is_pure_configuration() {
        let config = parse_str(
            r#"return {stores={["frame.lips"]={{id="dock",edge="bottom",length=300,opens="launcher"}}}}"#,
        )
        .unwrap();
        assert_eq!(
            config.lips,
            [LipConfig {
                id: "dock".into(),
                edge: Edge::Bottom,
                length: 300.,
                opens: "launcher".into()
            }]
        );
        assert!(config.blocked_by.is_empty());
        // No lips at all is a valid frame.
        assert!(parse_str("return {stores={}}").unwrap().lips.is_empty());
    }
    #[test]
    fn bad_entries_are_refused_with_a_reason() {
        for (text, why) in [
            (r#"{id="a",edge="up",length=10,opens="x"}"#, "edge"),
            (r#"{id="a",edge="top",length=0,opens="x"}"#, "length"),
            (r#"{id="a",edge="top",length=99999,opens="x"}"#, "length"),
            (r#"{id="",edge="top",length=10,opens="x"}"#, "required"),
            (r#"{id="a/b",edge="top",length=10,opens="x"}"#, "required"),
            (r#"{id="a",edge="top",length=10}"#, "required"),
        ] {
            let source = format!(r#"return {{stores={{["frame.lips"]={{{text}}}}}}}"#);
            let error = parse_str(&source).unwrap_err();
            assert!(error.contains(why), "{text}: {error}");
        }
        let twice = r#"return {stores={["frame.lips"]={{id="a",edge="top",length=10,opens="x"},{id="a",edge="left",length=10,opens="y"}}}}"#;
        assert!(parse_str(twice).unwrap_err().contains("duplicate"));
    }
}
