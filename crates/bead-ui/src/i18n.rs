// SPDX-License-Identifier: MIT OR Apache-2.0
//! Message lookup for shells: `tr(id, args)` over an injected catalog.
//!
//! The host supplies two things and nothing here knows where they come from:
//! a built-in English table (`set_builtin`, flat JSON `{id: text | {one, other}}`)
//! and, optionally, a fetched catalog (`install`) that replaces the previous one
//! whole, so an old revision never stays resident. An id missing from the
//! installed catalog falls back to the built-in text, then to the id itself.
//! Texts use `{name}` placeholders; plural entries use `{one, other}` forms
//! selected by `count`.
use serde::Deserialize;
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex, OnceLock},
};

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(untagged)]
pub enum Message {
    Text(String),
    Plural { one: String, other: String },
}
impl Message {
    fn bytes(&self) -> usize {
        match self {
            Self::Text(t) => t.len(),
            Self::Plural { one, other } => one.len() + other.len(),
        }
    }
}
pub type Entries = BTreeMap<String, Message>;
type Listener = Arc<dyn Fn() + Send + Sync>;
#[derive(Default)]
struct State {
    revision: u64,
    entries: Arc<Entries>,
    listeners: BTreeMap<u64, Listener>,
    next: u64,
}
static STATE: Mutex<Option<State>> = Mutex::new(None);
static BUILTIN: OnceLock<Entries> = OnceLock::new();
fn with<T>(f: impl FnOnce(&mut State) -> T) -> T {
    let mut guard = STATE.lock().unwrap_or_else(|p| p.into_inner());
    f(guard.get_or_insert_with(State::default))
}

/// Register the built-in English table once; later calls are ignored.
pub fn set_builtin(json: &str) {
    BUILTIN.get_or_init(|| {
        serde_json::from_str(json).unwrap_or_else(|e| {
            eprintln!("I18N_BUILTIN_INVALID {e}");
            Entries::new()
        })
    });
}
/// Replace the fetched catalog. Listeners run only when the content changed.
pub fn install(revision: u64, entries: Entries) {
    let listeners: Vec<Listener> = with(|s| {
        if s.revision == revision && *s.entries == entries {
            return Vec::new();
        }
        s.revision = revision;
        s.entries = Arc::new(entries);
        s.listeners.values().cloned().collect()
    });
    for listener in listeners {
        listener();
    }
}
/// Drop the fetched catalog (daemon gone for good); built-in English remains.
pub fn clear() {
    install(0, Entries::new());
}
pub fn revision() -> u64 {
    with(|s| s.revision)
}
/// Bytes held by the fetched catalog (ids and texts, without map overhead).
pub fn resident_bytes() -> usize {
    with(|s| s.entries.iter().map(|(k, v)| k.len() + v.bytes()).sum())
}
pub struct Subscription(u64);
impl Drop for Subscription {
    fn drop(&mut self) {
        with(|s| s.listeners.remove(&self.0));
    }
}
/// Call `listener` after every catalog change; dropping the handle unsubscribes.
pub fn on_change(listener: impl Fn() + Send + Sync + 'static) -> Subscription {
    with(|s| {
        s.next += 1;
        s.listeners.insert(s.next, Arc::new(listener));
        Subscription(s.next)
    })
}

fn lookup(id: &str) -> Option<Message> {
    with(|s| s.entries.get(id).cloned()).or_else(|| BUILTIN.get().and_then(|b| b.get(id).cloned()))
}
fn substitute(template: &str, args: &[(&str, &str)]) -> String {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(start) = rest.find('{') {
        out.push_str(&rest[..start]);
        let tail = &rest[start + 1..];
        match tail.find('}') {
            Some(end) => match args.iter().find(|(k, _)| *k == &tail[..end]) {
                Some((_, value)) => {
                    out.push_str(value);
                    rest = &tail[end + 1..];
                }
                None => {
                    out.push('{');
                    rest = tail;
                }
            },
            None => {
                out.push('{');
                rest = tail;
            }
        }
    }
    out.push_str(rest);
    out
}
/// Translate `id`. A plural entry without a count uses its `other` form.
pub fn tr(id: &str, args: &[(&str, &str)]) -> String {
    translate(id, None, args)
}
/// Translate a plural `id` for `count`; `{count}` is filled in automatically.
pub fn tr_n(id: &str, count: u64, args: &[(&str, &str)]) -> String {
    translate(id, Some(count), args)
}
pub(crate) fn translate(id: &str, count: Option<u64>, args: &[(&str, &str)]) -> String {
    let template = match lookup(id) {
        Some(Message::Text(text)) => text,
        Some(Message::Plural { one, other }) => {
            if count == Some(1) {
                one
            } else {
                other
            }
        }
        None => return id.to_owned(),
    };
    match count {
        Some(n) => {
            let n = n.to_string();
            let mut all = args.to_vec();
            all.push(("count", &n));
            substitute(&template, &all)
        }
        None => substitute(&template, args),
    }
}

/// The Luau entry point shared by markup (`ui.tr`) and behaviour (`tr`) scripts:
/// `tr(id, args?)`. A numeric `count` argument selects the plural form.
pub(crate) fn lua_function(lua: &mlua::Lua) -> mlua::Result<mlua::Function> {
    lua.create_function(|lua, (id, args): (String, Option<mlua::Table>)| {
        if id.is_empty() || id.len() > 128 {
            return Err(mlua::Error::runtime("tr needs a message id of 1-128 bytes"));
        }
        let (mut count, mut pairs) = (None, Vec::new());
        let table = match args {
            Some(table) => table,
            None => lua.create_table()?,
        };
        for pair in table.pairs::<String, mlua::Value>() {
            let (key, value) = pair?;
            let text = match value {
                mlua::Value::String(s) => s.to_str()?.to_string(),
                mlua::Value::Integer(i) => i.to_string(),
                mlua::Value::Number(n) if n.fract() == 0. && n.abs() < 1e15 => {
                    (n as i64).to_string()
                }
                mlua::Value::Number(n) => n.to_string(),
                _ => {
                    return Err(mlua::Error::runtime(
                        "tr arguments must be strings or numbers",
                    ));
                }
            };
            if key == "count" {
                count = text.parse::<i64>().ok().map(|n| n.max(0) as u64);
            }
            if pairs.len() >= 16 || text.len() > 4096 {
                return Err(mlua::Error::runtime("tr arguments exceed their bounds"));
            }
            pairs.push((key, text));
        }
        let refs: Vec<(&str, &str)> = pairs
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();
        Ok(translate(&id, count, &refs))
    })
}

#[cfg(test)]
mod tests {
    use super::substitute;
    #[test]
    fn placeholders_are_replaced_and_unknown_braces_survive() {
        assert_eq!(
            substitute("a {x} b {y}", &[("x", "1"), ("y", "2")]),
            "a 1 b 2"
        );
        assert_eq!(substitute("{missing} {", &[]), "{missing} {");
        assert_eq!(substitute("{x}{x}", &[("x", "é")]), "éé");
    }
}
