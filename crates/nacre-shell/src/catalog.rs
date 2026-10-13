// SPDX-License-Identifier: GPL-3.0-only
//! XDG desktop entries supply data, not UI rows. Execution/ranking stay later work.
use serde_json::{Value, json};
use std::{collections::BTreeSet, path::Path};
pub fn load(fixture: Option<&Path>) -> Result<Vec<Value>, Box<dyn std::error::Error>> {
    if let Some(path) = fixture {
        let rows: Vec<Value> = serde_json::from_slice(&std::fs::read(path)?)?;
        if rows.len() > 2048 {
            return Err("catalog fixture exceeds2048entries".into());
        }
        return Ok(rows);
    }
    let mut seen = BTreeSet::new();
    let locales = vec!["en".to_owned()];
    let mut rows = Vec::new();
    for entry in freedesktop_desktop_entry::desktop_entries(&locales) {
        if !seen.insert(entry.appid.clone())
            || entry.hidden()
            || entry.no_display()
            || entry.type_() != Some("Application")
        {
            continue;
        }
        let Some(name) = entry.name(&locales) else {
            continue;
        };
        rows.push(json!({"id":entry.appid,"name":name,"icon":entry.icon().unwrap_or("application-x-executable")}));
    }
    rows.sort_by_key(|row| row["name"].as_str().unwrap_or("").to_lowercase());
    rows.truncate(2048);
    Ok(rows)
}
