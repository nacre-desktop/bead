// SPDX-License-Identifier: GPL-3.0-only
//! Native model preparation; views still own layout and appearance.
use bead::core::{Stores, Value};
use std::collections::BTreeMap;
pub fn project(stores: &Stores) -> Result<BTreeMap<String, Value>, bead::ui::Diagnostic> {
    let _profile = bead::core::profile::Span::new("native_filter");
    let query = if let Some(Value::Text(q)) = stores.get("launcher.search.text") {
        q.to_lowercase()
    } else {
        String::new()
    };
    let selection = if let Some(Value::Text(s)) = stores.get("launcher.selection") {
        s.as_str()
    } else {
        ""
    };
    let rows = if let Some(Value::List(rows)) = stores.get("launcher.catalog") {
        rows.iter()
            .filter(|row| {
                row["name"]
                    .as_str()
                    .unwrap_or("")
                    .to_lowercase()
                    .contains(&query)
            })
            .map(|row| {
                let mut row = row.clone();
                row["selected"] = serde_json::Value::Bool(row["id"].as_str() == Some(selection));
                row
            })
            .collect()
    } else {
        vec![]
    };
    Ok(BTreeMap::from([(
        "launcher.apps".into(),
        Value::List(rows),
    )]))
}
