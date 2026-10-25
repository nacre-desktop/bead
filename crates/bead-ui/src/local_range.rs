// SPDX-License-Identifier: MIT OR Apache-2.0
//! Local continuous range feedback that avoids re-running view materialization.

use super::*;

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
    let object = format.as_object()?;
    let prefix = object.get("prefix")?.as_str()?;
    let suffix = match object.get("suffix") {
        None => "",
        Some(value) => value.as_str()?,
    };
    let precision = match object.get("precision") {
        None => 3,
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
    if len > 65_536 {
        return None;
    }
    let mut text = String::with_capacity(len);
    text.push_str(prefix);
    text.push_str(&number);
    text.push_str(suffix);
    Some(Value::Text(text))
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

        let mut changes = self.preview_changes(key.clone(), value);
        changes.insert("bead.focused".into(), Value::Text(id.to_owned()));
        self.stores
            .commit(changes)
            .map_err(|error| Diagnostic::new(&self.entry, 1, 1, error))?;
        Ok(Some(argument))
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
}
