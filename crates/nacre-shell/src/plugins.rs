// SPDX-License-Identifier: GPL-3.0-only
//! Nacre's edge of the Bead plugin loader (spec 0203): default paths and the
//! Plugins page's `plugins.do` actions. Slow work (git, copies) runs off the UI
//! thread; its outcome lands in the plugin state file, whose change reloads views.
use bead::core::Value;
use bead::ui::plugins;
use std::{collections::BTreeMap, path::Path};

/// `$XDG_DATA_HOME/nacre/plugins` for user plugins, `$XDG_CONFIG_HOME/nacre/plugins.json` for state.
pub fn configure(root: &Path) {
    plugins::configure(root, plugins::Config::for_app(root, "nacre"));
}

fn decode(value: &str) -> Option<serde_json::Value> {
    let (kind, raw) = value.split_once(':')?;
    match kind {
        "b" => raw.parse::<bool>().ok().map(Into::into),
        "n" => raw
            .parse::<f64>()
            .ok()
            .and_then(serde_json::Number::from_f64)
            .map(serde_json::Value::Number),
        "s" => Some(raw.into()),
        _ => None,
    }
}

fn background(
    config: plugins::Config,
    work: impl FnOnce(&plugins::Config) -> Result<String, String> + Send + 'static,
) {
    let spawned = std::thread::Builder::new()
        .name("nacre-plugins".into())
        .spawn(move || {
            let notice =
                work(&config).unwrap_or_else(|error| format!("Plugin action failed: {error}"));
            if let Err(error) = plugins::update_state(&config, |state| state.notice = notice) {
                eprintln!("PLUGIN_ERROR state {error}");
            }
        });
    if let Err(error) = spawned {
        eprintln!("PLUGIN_ERROR worker {error}");
    }
}

fn describe(capability: &str) -> String {
    match capability {
        "network" => "Network access".into(),
        "files" => "Read files".into(),
        "exec" => "Run programs".into(),
        _ => match capability.split_once(':') {
            Some(("topic", prefix)) => format!("Reads {prefix}*"),
            Some(("action", prefix)) => format!("Triggers {prefix}*"),
            _ => capability.into(),
        },
    }
}

/// B0208: the launcher's `>plugin [git URL or folder]` mode. Outside the mode only the
/// inactive flag is projected, so ordinary typing never scans the plugin folders.
pub fn launcher_values(root: &Path, query: &str) -> BTreeMap<String, Value> {
    let source = query
        .strip_prefix(">plugin")
        .filter(|rest| rest.is_empty() || rest.starts_with(char::is_whitespace))
        .map(str::trim);
    let Some(source) = source else {
        return BTreeMap::from([("launcher.plugin.active".into(), Value::Bool(false))]);
    };
    let config = plugins::config(root);
    let state = plugins::State::load(config.state.as_deref());
    let rows = plugins::discover(&config, &state)
        .into_iter()
        .map(|plugin| {
            let (name, access, error) = match &plugin.manifest {
                Ok(manifest) => {
                    let access: Vec<_> =
                        manifest.capabilities.iter().map(|c| describe(c)).collect();
                    let access = if access.is_empty() {
                        "none beyond drawing".to_owned()
                    } else {
                        access.join(", ")
                    };
                    (manifest.name.clone(), access, String::new())
                }
                Err(error) => (plugin.id.clone(), String::new(), error.clone()),
            };
            serde_json::json!({"id":plugin.id,"name":name,"enabled":plugin.enabled,"access":access,
                "error":error,"update":state.updates.contains(&plugin.id)})
        })
        .collect();
    BTreeMap::from([
        ("launcher.plugin.active".into(), Value::Bool(true)),
        ("launcher.plugin.source".into(), Value::Text(source.into())),
        ("launcher.plugin.rows".into(), Value::list(rows)),
        (
            "launcher.plugin.notice".into(),
            Value::Text(state.notice.clone()),
        ),
    ])
}

/// Apply one plugin action (Plugins page or launcher); the returned stores carry the message.
pub fn act(root: &Path, argument: &str) -> BTreeMap<String, Value> {
    let config = plugins::config(root);
    let mut parts = argument.splitn(4, '|');
    let result = match (parts.next(), parts.next(), parts.next(), parts.next()) {
        (Some(op @ ("enable" | "disable")), Some(id), None, None) => {
            plugins::set_enabled(&config, id, op == "enable").map(|()| format!("{id} {op}d"))
        }
        (Some("set"), Some(id), Some(key), Some(value)) => decode(value)
            .ok_or_else(|| "invalid setting value".to_owned())
            .and_then(|value| plugins::set_setting(&config, id, key, value))
            .map(|()| format!("{id}: {key} saved")),
        (Some("install"), Some(source), None, None) => {
            let source = source.trim().to_owned();
            if source.is_empty() {
                Err("enter a git URL or an absolute folder path".into())
            } else {
                background(config, move |config| {
                    let id = if source.starts_with('/') {
                        plugins::install_local(config, Path::new(&source))?
                    } else {
                        plugins::install_git(config, &source)?
                    };
                    Ok(format!("Installed {id}; enable it to load it"))
                });
                Ok("Installing…".into())
            }
        }
        (Some("check"), None, ..) => {
            background(config, |config| {
                plugins::check_updates(config).map(|n| format!("{n} plugin update(s) available"))
            });
            Ok("Checking for updates…".into())
        }
        (Some("update"), Some(id), None, None) => {
            let id = id.to_owned();
            background(config, move |config| {
                plugins::apply_update(config, &id).map(|()| format!("Updated {id}"))
            });
            Ok("Updating…".into())
        }
        _ => Err(format!("unknown plugin action {argument}")),
    };
    let message = result.unwrap_or_else(|error| format!("Plugin action failed: {error}"));
    BTreeMap::from([("plugins.message".into(), Value::Text(message))])
}

#[cfg(test)]
#[path = "plugins_tests.rs"]
mod tests;
