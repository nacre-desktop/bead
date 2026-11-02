// SPDX-License-Identifier: MIT
//! Host-provided design tokens used by the embedded Luau kit.
use serde_json::{Value, json};
use std::sync::Mutex;

#[cfg(test)]
thread_local! {
    static TEST_TOKENS: std::cell::RefCell<Option<Value>> = const { std::cell::RefCell::new(None) };
}

static TOKENS: Mutex<Option<Value>> = Mutex::new(None);

/// Generic scale and family names for a host that does not install a theme.
pub fn defaults() -> Value {
    let mut value: Value =
        serde_json::from_str(bead_kit::DEFAULT_SCALE).expect("Bead default scale is valid JSON");
    value["fonts"] = json!({"sans":"sans-serif", "mono":"monospace", "icon":"icons"});
    value["gradients"] = true.into();
    value
}

/// Set the tokens supplied by the current host, rejecting partial or malformed inputs.
/// Invalid inputs leave the active tokens untouched.
pub fn set(tokens: Value) -> Result<bool, String> {
    validate(&tokens)?;
    let changed = {
        let mut held = TOKENS.lock().unwrap_or_else(|p| p.into_inner());
        let changed = held.as_ref().is_none_or(|old| old != &tokens);
        *held = Some(tokens);
        changed
    };
    if changed {
        crate::i18n::refresh();
    }
    Ok(changed)
}

fn validate(tokens: &Value) -> Result<(), String> {
    fn group(tokens: &Value, name: &str, required: &[&str]) -> Result<(), String> {
        let values = tokens
            .get(name)
            .and_then(Value::as_object)
            .ok_or_else(|| format!("theme {name} must be an object"))?;
        for (key, value) in values {
            if !value
                .as_f64()
                .is_some_and(|value| value.is_finite() && value > 0. && value <= 10_000.)
            {
                return Err(format!("theme {name}.{key} must be in 0..10000"));
            }
        }
        for key in required {
            if !values.contains_key(*key) {
                return Err(format!("theme {name}.{key} is required"));
            }
        }
        Ok(())
    }
    if tokens.get("version").and_then(Value::as_u64) != Some(1) {
        return Err("theme version must be 1".into());
    }
    group(
        tokens,
        "spacing",
        &[
            "hairline", "xs", "sm", "compact", "md", "lg", "panel", "xl", "xxl", "huge",
        ],
    )?;
    group(tokens, "radii", &["small", "medium", "large", "full"])?;
    group(tokens, "text", &["caption", "body", "title", "headline"])?;
    group(
        tokens,
        "icons",
        &["small", "medium", "large", "xlarge", "hero"],
    )?;
    let fonts = tokens
        .get("fonts")
        .and_then(Value::as_object)
        .ok_or_else(|| "theme fonts must be an object".to_owned())?;
    for (key, value) in fonts {
        if !value.as_str().is_some_and(|font| {
            !font.trim().is_empty() && font.len() <= 256 && !font.chars().any(char::is_control)
        }) {
            return Err(format!("theme fonts.{key} must be a nonempty family name"));
        }
    }
    for key in ["sans", "mono", "icon"] {
        if !fonts.contains_key(key) {
            return Err(format!("theme fonts.{key} is required"));
        }
    }
    if !tokens.get("gradients").is_some_and(Value::is_boolean) {
        return Err("theme gradients must be boolean".into());
    }
    Ok(())
}

/// Snapshot the current host tokens, including generic defaults when unset.
pub fn current() -> Value {
    #[cfg(test)]
    if let Some(tokens) = TEST_TOKENS.with(|held| held.borrow().clone()) {
        return tokens;
    }
    TOKENS
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .clone()
        .unwrap_or_else(defaults)
}

/// The host's sans family, used for text with no family authored by a view.
pub fn sans_family() -> String {
    with_sans_family(str::to_owned)
}

/// Call with the cached host sans family without cloning the token tree.
pub(crate) fn with_sans_family<T>(f: impl FnOnce(&str) -> T) -> T {
    #[cfg(test)]
    if let Some(value) = TEST_TOKENS.with(|held| {
        held.borrow()
            .as_ref()
            .and_then(|tokens| tokens["fonts"]["sans"].as_str().map(str::to_owned))
    }) {
        return f(&value);
    }
    let held = TOKENS.lock().unwrap_or_else(|p| p.into_inner());
    if let Some(value) = held
        .as_ref()
        .and_then(|tokens| tokens["fonts"]["sans"].as_str())
    {
        f(value)
    } else {
        f("sans-serif")
    }
}

#[cfg(test)]
pub(crate) fn with_test_tokens<T>(tokens: Value, run: impl FnOnce() -> T) -> T {
    struct Restore(Option<Value>);
    impl Drop for Restore {
        fn drop(&mut self) {
            TEST_TOKENS.with(|held| {
                held.replace(self.0.take());
            });
        }
    }
    TEST_TOKENS.with(|held| {
        let previous = held.replace(Some(tokens));
        let _restore = Restore(previous);
        run()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_tokens_replace_scale_and_family_defaults() {
        let mut alternate = defaults();
        alternate["spacing"]["sm"] = 13.into();
        alternate["fonts"]["sans"] = "Alternate Sans".into();
        alternate["fonts"]["icon"] = "Alternate Icons".into();
        with_test_tokens(alternate.clone(), || assert_eq!(current(), alternate));
    }

    #[test]
    fn invalid_or_partial_tokens_are_rejected_before_replacement() {
        let before = current();
        let mut partial = defaults();
        partial["spacing"].as_object_mut().unwrap().remove("md");
        assert!(set(partial).is_err());
        assert_eq!(current(), before);
        let mut malformed = defaults();
        malformed["fonts"]["icon"] = 12.into();
        assert!(validate(&malformed).is_err());
        malformed = defaults();
        malformed["gradients"] = "yes".into();
        assert!(validate(&malformed).is_err());
    }
}
