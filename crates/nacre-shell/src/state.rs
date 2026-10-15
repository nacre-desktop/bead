// SPDX-License-Identifier: GPL-3.0-only
//! Only panel open/page state is durable; transport snapshots and typed text are excluded.
use bead::core::{Stores, Value};
use std::{
    collections::BTreeMap,
    io::{Read, Write},
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::PathBuf,
};
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Saved {
    version: u32,
    values: BTreeMap<String, Value>,
}
pub struct File {
    path: PathBuf,
    last: BTreeMap<String, Value>,
}
fn durable(id: &str, value: &Value) -> bool {
    match value {
        Value::Bool(_) => id == "open" || id.ends_with(".open"),
        Value::Text(s) => s.len() <= 256 && (id.ends_with(".page") || id.ends_with(".route")),
        _ => false,
    }
}
impl File {
    pub fn path() -> Result<PathBuf, String> {
        let root = std::env::var_os("XDG_STATE_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/state")))
            .ok_or("state home unavailable")?;
        if !root.is_absolute() {
            return Err("state home must be absolute".into());
        }
        Ok(root.join("bead/panels.json"))
    }
    pub fn open(
        path: PathBuf,
    ) -> Result<(Self, BTreeMap<String, Value>), Box<dyn std::error::Error>> {
        let values = match std::fs::File::open(&path) {
            Ok(file) => {
                let mut bytes = Vec::new();
                file.take(32769).read_to_end(&mut bytes)?;
                if bytes.len() > 32768 {
                    return Err("panel state exceeds32KiB".into());
                }
                let saved: Saved = serde_json::from_slice(&bytes)?;
                if saved.version != 1 || saved.values.iter().any(|(id, v)| !durable(id, v)) {
                    return Err("invalid panel state schema".into());
                }
                saved.values
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => BTreeMap::new(),
            Err(error) => return Err(error.into()),
        };
        Ok((
            Self {
                path,
                last: values.clone(),
            },
            values,
        ))
    }
    pub fn save(&mut self, stores: &Stores) -> Result<(), Box<dyn std::error::Error>> {
        let mut values = self.last.clone();
        for (id, value) in stores.values().iter().filter(|(id, v)| durable(id, v)) {
            values.insert(id.clone(), value.clone());
        }
        if values == self.last {
            return Ok(());
        }
        let bytes = serde_json::to_vec(&Saved {
            version: 1,
            values: values.clone(),
        })?;
        if bytes.len() > 32768 {
            return Err("panel state exceeds32KiB".into());
        }
        let parent = self.path.parent().ok_or("invalid state path")?;
        std::fs::create_dir_all(parent)?;
        std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700))?;
        let temporary = self
            .path
            .with_extension(format!("tmp-{}", std::process::id()));
        let result = (|| -> Result<(), Box<dyn std::error::Error>> {
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&temporary)?;
            file.write_all(&bytes)?;
            file.sync_all()?;
            drop(file);
            std::fs::rename(&temporary, &self.path)?;
            std::fs::File::open(parent)?.sync_all()?;
            Ok(())
        })();
        if result.is_err() {
            let _ = std::fs::remove_file(&temporary);
        }
        result?;
        self.last = values;
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn atomic_panel_restore_excludes_private_transient_state_and_rejects_bad_schema() {
        let root = std::env::temp_dir().join(format!(
            "bead-state-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("panels.json");
        let (mut file, _) = File::open(path.clone()).unwrap();
        let stores = Stores::default()
            .reconcile(&BTreeMap::from([
                ("panel.open".into(), Value::Bool(false)),
                ("panel.page".into(), Value::Text("sound".into())),
                ("search.text".into(), Value::Text("private".into())),
            ]))
            .unwrap();
        file.save(&stores).unwrap();
        let (_, values) = File::open(path.clone()).unwrap();
        assert_eq!(values.len(), 2);
        assert_eq!(values["panel.open"], Value::Bool(false));
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        std::fs::write(&path, b"{\"version\":2,\"values\":{}}").unwrap();
        assert!(File::open(path).is_err());
        std::fs::remove_dir_all(root).unwrap();
    }
}
