// SPDX-License-Identifier: GPL-3.0-only
//! View projection of ordered engine results. No catalog, providers or ranking.
use bead::core::{
    Stores, Value,
    source::{self, Source},
};
use serde_json::{Value as Json, json};
use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet},
    path::Path,
    sync::Arc,
    time::Duration,
};

pub type Memory = Arc<std::sync::Mutex<BTreeMap<String, Value>>>;
/// Retain only bounded view state; never catalogs, results, rasters or font pools.
pub fn remember(stores: &Stores) -> BTreeMap<String, Value> {
    let mut result = BTreeMap::new();
    for (key, limit) in [
        ("launcher.search.text", 65536),
        ("launcher.selection", 1024),
        ("launcher.mode", 128),
        ("launcher.route", 128),
    ] {
        if let Some(Value::Text(value)) = stores.get(key)
            && value.len() <= limit
        {
            result.insert(key.into(), Value::Text(value.clone()));
        }
    }
    if let Some(Value::Number(value)) = stores.get("launcher.scroll.y")
        && value.is_finite()
        && (0. ..=20000.).contains(value)
    {
        result.insert("launcher.scroll.y".into(), Value::Number(*value));
    }
    result
}
pub fn source(
    fixture: Option<&Path>,
    socket: Option<&Path>,
) -> Result<Option<Arc<dyn Source>>, Box<dyn std::error::Error>> {
    if fixture.is_some() && socket.is_some() {
        return Err("select one launcher Source".into());
    }
    if let Some(path) = fixture {
        Ok(Some(Arc::new(Fixture::read(path)?)))
    } else if let Some(socket) = socket {
        Ok(Some(Arc::new(crate::nacre_source::Client::start(
            crate::nacre_source::Config::new(socket.into(), vec![]),
        )?)))
    } else {
        Ok(None)
    }
}

/// Interim QML row contract. Replace only this projection when menu.query's
/// canonical typed result lands; ordering and activation remain engine-owned.
pub fn rows(mut reply: Json) -> source::Result<Vec<Json>> {
    let Some(Json::Array(rows)) = reply.as_object_mut().and_then(|r| r.remove("rows")) else {
        return Err(source::Error::new(
            "validation",
            "menu result rows missing",
            false,
        ));
    };
    if rows.len() > 2048 {
        return Err(source::Error::new("validation", "menu result limit", false));
    }
    let mut seen = BTreeSet::new();
    let mut normalized = Vec::with_capacity(rows.len());
    for mut row in rows {
        let id = row
            .get("id")
            .and_then(Json::as_str)
            .filter(|s| !s.is_empty() && s.len() <= 1024)
            .ok_or_else(|| source::Error::new("validation", "menu result id missing", false))?;
        if !seen.insert(id.to_owned()) {
            return Err(source::Error::new(
                "validation",
                "duplicate menu result id",
                false,
            ));
        }
        let name = row
            .get("label")
            .or_else(|| row.get("name"))
            .and_then(Json::as_str)
            .ok_or_else(|| source::Error::new("validation", "menu result label missing", false))?;
        let name = name.to_owned();
        if let Some(breadcrumb) = row
            .get("breadcrumb")
            .and_then(Json::as_str)
            .filter(|s| !s.is_empty())
        {
            let breadcrumb = breadcrumb.to_owned();
            row["description"] = Json::String(breadcrumb);
        } else if !row.get("description").is_some_and(Json::is_string) {
            row["description"] = json!("");
        }
        row["name"] = json!(name);
        if !row.get("icon").is_some_and(Json::is_string) {
            row["icon"] = json!("application-x-executable");
        }
        normalized.push(row);
    }
    Ok(normalized)
}

/// Explicit deterministic query replies, never a searchable application catalog.
/// Fixtures are keyed by the *exact* request string; unknown queries fail.
pub struct Fixture {
    publisher: source::Publisher,
}
impl Fixture {
    pub fn read(path: &Path) -> source::Result<Self> {
        use std::io::Read;
        let file = std::fs::File::open(path)
            .map_err(|_| source::Error::new("validation", "menu fixture unavailable", false))?;
        let mut bytes = Vec::new();
        file.take(1_048_577)
            .read_to_end(&mut bytes)
            .map_err(|_| source::Error::new("validation", "menu fixture read failed", false))?;
        if bytes.len() > 1_048_576 {
            return Err(source::Error::new(
                "validation",
                "menu fixture exceeds bound",
                false,
            ));
        }
        let data: Json = serde_json::from_slice(&bytes)
            .map_err(|_| source::Error::new("validation", "invalid menu fixture", false))?;
        Self::new(data)
    }
    pub fn new(data: Json) -> source::Result<Self> {
        let queries = data
            .get("queries")
            .and_then(Json::as_object)
            .ok_or_else(|| {
                source::Error::new("validation", "menu fixture queries missing", false)
            })?;
        for reply in queries.values() {
            rows(reply.clone())?;
        }
        Ok(Self {
            publisher: source::Publisher::new("menu-fixture".into(), data)?,
        })
    }
}
impl Source for Fixture {
    fn snapshot(&self) -> Arc<source::Snapshot> {
        self.publisher.snapshot()
    }
    fn subscribe(&self, callback: source::Callback) -> source::Result<source::Subscription> {
        self.publisher.subscribe(callback)
    }
    fn invoke(&self, method: String, params: Json) -> source::Reply<'_> {
        Box::pin(async move {
            if method != "menu.query" {
                return Err(source::Error::new(
                    "unavailable",
                    "fixture never activates external actions",
                    false,
                ));
            }
            let snapshot = self.snapshot();
            let query = params
                .get("query")
                .and_then(Json::as_str)
                .ok_or_else(|| source::Error::new("validation", "query missing", false))?;
            snapshot
                .data
                .get("queries")
                .and_then(|q| q.get(query))
                .cloned()
                .ok_or_else(|| {
                    source::Error::new("unavailable", "query absent from fixture", false)
                })
        })
    }
    fn reconnect(&self) -> source::Reply<'_> {
        Box::pin(async { Ok(Json::Null) })
    }
    fn stop(&self) -> source::Reply<'_> {
        Box::pin(async {
            self.publisher.disconnect()?;
            Ok(Json::Null)
        })
    }
}

#[derive(Default)]
struct View {
    request: Option<(String, String)>,
    stamp: Option<(String, u64, bool)>,
    rows: Vec<Json>,
    error: String,
}
pub struct Menu {
    source: Option<Arc<dyn Source>>,
    runtime: tokio::runtime::Runtime,
    view: RefCell<View>,
}
impl Menu {
    pub fn new(source: Option<Arc<dyn Source>>) -> Result<Self, String> {
        Ok(Self {
            source,
            runtime: tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .max_blocking_threads(2)
                .build()
                .map_err(|e| e.to_string())?,
            view: RefCell::new(View::default()),
        })
    }
    pub fn subscribe(
        &self,
        callback: source::Callback,
    ) -> source::Result<Option<source::Subscription>> {
        self.source
            .as_ref()
            .map(|s| s.subscribe(callback))
            .transpose()
    }
    pub fn activate(&self, id: &str) -> source::Result<()> {
        let view = self.view.borrow();
        if !view.error.is_empty()
            || !view
                .rows
                .iter()
                .any(|r| r["id"].as_str() == Some(id) && r["disabled"] != true)
        {
            return Err(source::Error::new(
                "unavailable",
                "Result is not available",
                true,
            ));
        }
        drop(view);
        let source = self.source.as_ref().ok_or_else(|| {
            source::Error::new("unavailable", "Launcher service unavailable", true)
        })?;
        match self.runtime.block_on(source.call(
            "menu.activate".into(),
            json!({"id":id}),
            Duration::from_secs(3),
        )) {
            Ok(_) => Ok(()),
            Err(e) => {
                self.view.borrow_mut().error = e.message.clone();
                Err(e)
            }
        }
    }
    pub fn project(
        &self,
        stores: &Stores,
    ) -> Result<BTreeMap<String, Value>, bead::ui::Diagnostic> {
        let _profile = bead::core::profile::Span::new("source_projection");
        let text = |key: &str| match stores.get(key) {
            Some(Value::Text(s)) => s.clone(),
            _ => String::new(),
        };
        let request = (text("launcher.search.text"), text("launcher.mode"));
        let selection = text("launcher.selection");
        let stamp = self.source.as_ref().map(|s| {
            let snap = s.snapshot();
            (snap.epoch.clone(), snap.revision, snap.connected)
        });
        let mut view = self.view.borrow_mut();
        if view.request.as_ref() != Some(&request) || view.stamp != stamp {
            let result = if let Some(source) = &self.source {
                let _query = bead::core::profile::Span::new("menu_query");
                self.runtime
                    .block_on(source.call(
                        "menu.query".into(),
                        json!({"query":request.0,"mode":request.1}),
                        Duration::from_secs(3),
                    ))
                    .and_then(rows)
            } else {
                Err(source::Error::new(
                    "unavailable",
                    "Launcher service unavailable",
                    true,
                ))
            };
            match result {
                Ok(rows) => {
                    view.rows = rows;
                    view.error.clear();
                }
                Err(e) => {
                    view.error = e.message;
                }
            }
            view.request = Some(request);
            view.stamp = stamp;
        }
        let selection = if view
            .rows
            .iter()
            .any(|r| r["id"].as_str() == Some(selection.as_str()))
        {
            selection
        } else {
            view.rows
                .first()
                .and_then(|r| r["id"].as_str())
                .unwrap_or("")
                .to_string()
        };
        for row in &mut view.rows {
            let selected = row["id"].as_str() == Some(selection.as_str());
            row["selected"] = Json::Bool(selected);
        }
        let mut delta = BTreeMap::from([
            (
                "launcher.result_count".into(),
                Value::Number(view.rows.len() as f64),
            ),
            ("launcher.selection".into(), Value::Text(selection)),
            ("launcher.error".into(), Value::Text(view.error.clone())),
        ]);
        // A query may change while the ordered engine result stays identical.
        // Retain the store's immutable model instead of cloning it just to compare.
        if !matches!(stores.get("launcher.apps"),Some(Value::List(current))if current.as_ref()==&view.rows)
        {
            delta.insert("launcher.apps".into(), Value::list(view.rows.clone()));
        }
        Ok(delta)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn idle_memory_retains_only_bounded_scalar_view_state() {
        let stores = Stores::default()
            .reconcile(&BTreeMap::from([
                (
                    "launcher.search.text".into(),
                    Value::Text("x".repeat(65536)),
                ),
                ("launcher.selection".into(), Value::Text("id".into())),
                ("launcher.route".into(), Value::Text("apps".into())),
                ("launcher.mode".into(), Value::Text("apps".into())),
                ("launcher.scroll.y".into(), Value::Number(1200.)),
                (
                    "launcher.apps".into(),
                    Value::list(vec![
                        json!({"id":"large-result","payload":"x".repeat(65536)}),
                    ]),
                ),
            ]))
            .unwrap();
        let remembered = remember(&stores);
        assert_eq!(remembered.len(), 5);
        assert!(!remembered.contains_key("launcher.apps"));
        assert!(serde_json::to_vec(&remembered).unwrap().len() < 70 * 1024);
    }
    #[test]
    fn breadcrumb_and_description_follow_qml_result_labels() {
        let rows=rows(json!({"rows":[{"id":"a","label":"A","breadcrumb":"All apps","description":"Other"},{"id":"b","label":"B","description":"Fallback"}]})).unwrap();
        assert_eq!(rows[0]["description"], "All apps");
        assert_eq!(rows[1]["description"], "Fallback");
    }
    #[test]
    fn engine_order_survives_and_invalid_replies_fail() {
        let reply = json!({"rows":[{"id":"z","label":"Zulu"},{"id":"a","label":"Alpha"}]});
        let r = rows(reply.clone()).unwrap();
        assert_eq!(r[0]["id"], "z");
        assert_eq!(r[1]["name"], "Alpha");
        assert!(rows(json!({"rows":[{"id":"x","label":"X"},{"id":"x","label":"X"}]})).is_err());
    }
    #[test]
    fn a_source_revision_refreshes_the_same_query() {
        let source = Arc::new(
            Fixture::new(json!({"queries":{"":{"rows":[{"id":"x","label":"Old"}]}}})).unwrap(),
        );
        let menu = Menu::new(Some(source.clone())).unwrap();
        let stores = Stores::default();
        assert!(
            matches!(&menu.project(&stores).unwrap()["launcher.apps"],Value::List(r) if r[0]["name"]=="Old")
        );
        source
            .publisher
            .publish(json!({"queries":{"":{"rows":[{"id":"x","label":"New"}]}}}))
            .unwrap();
        assert!(
            matches!(&menu.project(&stores).unwrap()["launcher.apps"],Value::List(r) if r[0]["name"]=="New")
        );
    }
    #[test]
    fn exact_fixture_queries_do_not_filter_or_rank() {
        let source=Arc::new(Fixture::new(json!({"queries":{"":{"rows":[{"id":"z","label":"Zulu"},{"id":"a","label":"Alpha"}]},"z":{"rows":[{"id":"a","label":"Alpha"}]}}})).unwrap());
        let menu = Menu::new(Some(source.clone())).unwrap();
        let mut stores = Stores::default()
            .reconcile(&BTreeMap::from([(
                "launcher.search.text".into(),
                Value::Text("z".into()),
            )]))
            .unwrap();
        assert!(
            matches!(&menu.project(&stores).unwrap()["launcher.apps"],Value::List(r) if r[0]["id"]=="a")
        );
        stores
            .commit(BTreeMap::from([(
                "launcher.search.text".into(),
                Value::Text("missing".into()),
            )]))
            .unwrap();
        let v = menu.project(&stores).unwrap();
        assert!(matches!(&v["launcher.error"],Value::Text(s) if !s.is_empty()));
        assert!(
            menu.runtime
                .block_on(source.invoke("menu.activate".into(), json!({})))
                .is_err()
        );
    }
}
