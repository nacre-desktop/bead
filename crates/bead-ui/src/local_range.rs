// SPDX-License-Identifier: MIT OR Apache-2.0
//! Local continuous range feedback that avoids re-running view materialization.

use super::*;
use bead_render::{Primitive, Scene};

/// The accepted action argument and, when authored, its local text-only scene patch.
pub struct LocalRangeFeedback {
    pub argument: Value,
    pub scene: Option<Scene>,
}

fn formatted_argument(
    value: f32,
    numeric_action_argument: bool,
    format: Option<&serde_json::Value>,
) -> Option<Value> {
    if !value.is_finite() {
        return None;
    }
    let Some(format) = format else {
        return numeric_action_argument.then_some(Value::Number(f64::from(value)));
    };
    formatted_text(value, format, 3, 65_536).map(Value::Text)
}

fn formatted_label(value: f32, format: &serde_json::Value) -> Option<String> {
    formatted_text(value, format, 0, 256)
}

fn formatted_text(
    value: f32,
    format: &serde_json::Value,
    default_precision: usize,
    limit: usize,
) -> Option<String> {
    if !value.is_finite() {
        return None;
    }
    let object = format.as_object()?;
    let prefix = object.get("prefix").and_then(serde_json::Value::as_str)?;
    let suffix = match object.get("suffix") {
        None => "",
        Some(value) => value.as_str()?,
    };
    let precision = match object.get("precision") {
        None => default_precision,
        Some(value) => usize::try_from(value.as_u64()?).ok()?,
    };
    if precision > 6 {
        return None;
    }
    let number = format!("{value:.precision$}");
    let len = prefix
        .len()
        .checked_add(number.len())?
        .checked_add(suffix.len())?;
    if len > limit {
        return None;
    }
    let mut text = String::with_capacity(len);
    text.push_str(prefix);
    text.push_str(&number);
    text.push_str(suffix);
    Some(text)
}

fn patch_text_layer(scene: &mut Scene, id: &str, text: &str) -> bool {
    let Some(layer) = scene.layers.iter().find(|layer| layer.id == id) else {
        return false;
    };
    let Some(index) = layer
        .own
        .clone()
        .find(|index| matches!(scene.primitives.get(*index), Some(Primitive::Text { .. })))
    else {
        return false;
    };
    let Some(Primitive::Text { text: current, .. }) = scene.primitives.get_mut(index) else {
        return false;
    };
    if current == text {
        return false;
    }
    *current = text.to_owned();
    true
}

impl Session {
    pub fn supports_local_range(&self, id: &str, format: Option<&serde_json::Value>) -> bool {
        self.current_nodes
            .iter()
            .find(|node| node.id == id && node.drag && !node.preview_key.is_empty())
            .is_some_and(|node| {
                formatted_argument(
                    node.preview_value,
                    matches!(node.action_arg.as_ref(), Some(Value::Number(_))),
                    format,
                )
                .is_some()
            })
    }

    /// Update one authored range's accepted local feedback without evaluating its view.
    pub fn local_range_feedback(
        &mut self,
        id: &str,
        x: f32,
        format: Option<&serde_json::Value>,
    ) -> Result<Option<Value>, Diagnostic> {
        Ok(self
            .local_range_feedback_with_label(id, x, format, None)?
            .map(|feedback| feedback.argument))
    }

    /// Update one range's preview stores and optionally patch a fixed text layer in the
    /// accepted scene. This deliberately does not evaluate Luau or relayout the view.
    pub fn local_range_feedback_with_label(
        &mut self,
        id: &str,
        x: f32,
        format: Option<&serde_json::Value>,
        label: Option<&serde_json::Value>,
    ) -> Result<Option<LocalRangeFeedback>, Diagnostic> {
        let _profile = bead_core::profile::Span::new("slider_label");
        if self.pending.is_some() || !x.is_finite() {
            return Ok(None);
        }
        let Some((key, value, numeric_action_argument)) = self
            .current_nodes
            .iter()
            .find(|node| {
                node.id == id && node.focusable && node.drag && !node.preview_key.is_empty()
            })
            .and_then(|node| {
                let numeric = node
                    .action_arg
                    .as_ref()
                    .is_some_and(|argument| matches!(argument, Value::Number(_)));
                let (key, value) = self.preview_position(node, Some(x));
                (value.is_finite() && !key.is_empty()).then_some((key, value, numeric))
            })
        else {
            return Ok(None);
        };
        let Some(argument) = formatted_argument(value, numeric_action_argument, format) else {
            return Ok(None);
        };

        let mut scene = None;
        if let Some(label) = label
            && let Some(object) = label.as_object()
            && let Some(label_id) = object.get("id").and_then(serde_json::Value::as_str)
            && !label_id.is_empty()
            && let Some(label_format) = object.get("format")
            && let Some(text) = formatted_label(value, label_format)
        {
            let mut patched = self.current.clone();
            if patch_text_layer(&mut patched, label_id, &text) {
                // The prepared frame contains the previous glyph texture for this layer.
                patched.prepared.clear();
                scene = Some(patched);
            }
        }

        let mut changes = self.preview_changes(key.clone(), value);
        changes.insert("bead.focused".into(), Value::Text(id.to_owned()));
        self.stores
            .commit(changes)
            .map_err(|error| Diagnostic::new(&self.entry, 1, 1, error))?;
        if let Some(patched) = &scene {
            self.current = patched.clone();
        }
        Ok(Some(LocalRangeFeedback { argument, scene }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numeric_arguments_require_a_numeric_action_argument() {
        assert_eq!(
            formatted_argument(17.25, true, None),
            Some(Value::Number(17.25))
        );
        assert_eq!(formatted_argument(17.25, false, None), None);
    }

    #[test]
    fn authored_text_format_uses_default_and_explicit_precision() {
        let default = serde_json::json!({"prefix":"volume:"});
        assert_eq!(
            formatted_argument(17.25, false, Some(&default)),
            Some(Value::Text("volume:17.250".into()))
        );
        let explicit = serde_json::json!({"prefix":"set:","suffix":"%","precision":1});
        assert_eq!(
            formatted_argument(17.25, false, Some(&explicit)),
            Some(Value::Text("set:17.2%".into()))
        );
    }

    #[test]
    fn unsupported_formats_are_rejected() {
        for format in [
            serde_json::json!([]),
            serde_json::json!({"suffix":"%"}),
            serde_json::json!({"prefix":"x","precision":7}),
            serde_json::json!({"prefix":"x","precision":1.5}),
        ] {
            assert_eq!(formatted_argument(4., false, Some(&format)), None);
        }
        assert_eq!(formatted_argument(f32::NAN, true, None), None);
    }

    #[test]
    fn labels_use_independent_display_formatting() {
        assert_eq!(
            formatted_label(53.6, &serde_json::json!({"prefix":"", "suffix":"%"})),
            Some("54%".into())
        );
        assert_eq!(
            formatted_label(
                53.626,
                &serde_json::json!({"prefix":"", "suffix":" dB", "precision":2})
            ),
            Some("53.63 dB".into())
        );
        assert_eq!(
            formatted_label(1.0, &serde_json::json!({"suffix":"%"})),
            None
        );
    }

    #[test]
    fn text_patch_changes_only_the_named_layer_primitive() {
        let mut scene = Scene {
            opacity: 1.,
            motion: None,
            resource_revision: 4,
            asset_data: Default::default(),
            clips: Default::default(),
            layers: vec![bead_render::SceneLayer {
                id: "tile.value".into(),
                parent: Some("tile".into()),
                own: 0..1,
                subtree_end: 1,
                bounds: Rect::new(20., 10., 40., 16.),
                clip_bounds: None,
            }],
            prepared: Default::default(),
            font_files: vec![],
            width: 100.,
            slide: None,
            primitives: vec![Primitive::Text {
                rect: Rect::new(20., 10., 40., 16.),
                text: "Muted".into(),
                family: String::new(),
                weight: 400,
                align: Some(bead_render::TextAlign::Right),
                size: 12.,
                color: [255, 255, 255, 255],
            }],
            offset: (0., 0.),
            bounds: Rect::new(0., 0., 100., 100.),
        };
        assert!(patch_text_layer(&mut scene, "tile.value", "54%"));
        assert!(matches!(
            &scene.primitives[0],
            Primitive::Text { text, rect, .. } if text == "54%" && *rect == Rect::new(20., 10., 40., 16.)
        ));
        assert_eq!(scene.layers[0].bounds, Rect::new(20., 10., 40., 16.));
        assert_eq!(scene.resource_revision, 4);
        assert!(!patch_text_layer(&mut scene, "missing", "55%"));
    }
}
