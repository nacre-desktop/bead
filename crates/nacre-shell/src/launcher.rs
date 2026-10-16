// SPDX-License-Identifier: GPL-3.0-only
//! View projection of ordered engine results. No catalog, providers or ranking.
use bead::core::{
    Stores, Value,
    source::{self, Source},
};
use nacre_protocol::menu::{MenuActivate, MenuActivated, MenuQuery, MenuQueryResult};
use serde_json::{Value as Json, json};
use std::{cell::RefCell, collections::BTreeMap, path::Path, sync::Arc, time::Duration};

pub type Memory = Arc<std::sync::Mutex<BTreeMap<String, Value>>>;
/// Retain only bounded view state; never catalogs, results, rasters or font pools.
pub fn remember(stores: &Stores) -> BTreeMap<String, Value> {
    let mut result = BTreeMap::new();
    for (key, limit) in [
        ("launcher.search.text", 65536),
        ("launcher.selection", 1024),
        ("launcher.mode", 128),
        ("launcher.route", 128),
        ("launcher.engine_route", 256),
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

/// Only display data crosses into Luau; opaque activation stays in the host.
struct Prepared {
    rows: Vec<Json>,
    actions: BTreeMap<String, String>,
    catalog_rev: u64,
}
fn invalid_menu() -> source::Error {
    source::Error::new("validation", "Invalid menu method payload", false)
}
fn prepare(reply: Json) -> source::Result<Prepared> {
    let mut result: MenuQueryResult = serde_json::from_value(reply).map_err(|_| invalid_menu())?;
    result.validate().map_err(|_| invalid_menu())?;
    let mut rows = Vec::with_capacity(result.order.len());
    let mut actions = BTreeMap::new();
    for id in result.order {
        let item = result.items.remove(&id).ok_or_else(invalid_menu)?;
        if let Some(token) = item.activation {
            actions.insert(id.clone(), token);
        }
        let description = item
            .breadcrumb
            .filter(|s| !s.is_empty())
            .or(item.description)
            .unwrap_or_default();
        let mut row = json!({"id":id,"name":item.name,"kind":item.kind,
            "description":description,"icon":item.icon.filter(|s|!s.is_empty()).unwrap_or_else(||"application-x-executable".into()),
            "disabled":item.disabled});
        for (key, value) in [
            ("emoji", item.emoji),
            ("brand", item.brand),
            ("status", item.status),
            ("reason", item.reason),
        ] {
            if let Some(value) = value {
                row[key] = Json::String(value);
            }
        }
        if let Some(checked) = item.checked {
            row["checked"] = Json::Bool(checked);
        }
        rows.push(row);
    }
    Ok(Prepared {
        rows,
        actions,
        catalog_rev: result.catalog_rev,
    })
}
pub fn rows(reply: Json) -> source::Result<Vec<Json>> {
    Ok(prepare(reply)?.rows)
}

pub fn activation_changes(result: MenuActivated) -> BTreeMap<String, Value> {
    let mut changes = BTreeMap::from([("panel.open".into(), Value::Bool(result.keep_open))]);
    if let Some(view) = result.view {
        if let Some(mode) = view.mode {
            if matches!(mode.as_str(), "apps" | "search" | "actions") {
                changes.insert("launcher.route".into(), Value::Text(mode.clone()));
            }
            changes.insert("launcher.mode".into(), Value::Text(mode));
        }
        for (key, value) in [
            ("launcher.search.text", view.query),
            ("launcher.engine_route", view.route),
        ] {
            if let Some(value) = value {
                changes.insert(key.into(), Value::Text(value));
            }
        }
        changes.insert("launcher.selection".into(), Value::Text(String::new()));
        changes.insert("launcher.scroll.y".into(), Value::Number(0.));
        changes.insert("bead.focused".into(), Value::Text("launcher.search".into()));
    }
    changes
}

#[cfg(test)]
pub(crate) fn fixture_reply(query: &str, rows: Vec<Json>) -> Json {
    let mut items = serde_json::Map::new();
    let mut order = Vec::new();
    for mut row in rows {
        let id = row["id"].as_str().expect("fixture id").to_owned();
        if row.get("name").is_none() {
            row["name"] = row["label"].clone();
        }
        if row.get("kind").is_none() {
            row["kind"] = json!("app");
        }
        row["activation"] = json!(format!("fixture:{id}"));
        order.push(id.clone());
        items.insert(id, row);
    }
    json!({"query":query,"mode":"apps","route":"apps","catalog_rev":1,"total":order.len(),"order":order,"items":items})
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
            let request: MenuQuery = serde_json::from_value(params).map_err(|_| invalid_menu())?;
            request.validate().map_err(|_| invalid_menu())?;
            let query = &request.query;
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
    request: Option<(String, String, String)>,
    actions: BTreeMap<String, String>,
    catalog_rev: u64,
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
    pub fn activate(&self, id: &str) -> source::Result<MenuActivated> {
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
        let request = MenuActivate {
            id: id.into(),
            activation: view.actions.get(id).cloned().ok_or_else(|| {
                source::Error::new("unavailable", "Result has no activation descriptor", false)
            })?,
            catalog_rev: view.catalog_rev,
        };
        request.validate().map_err(|_| invalid_menu())?;
        drop(view);
        let source = self.source.as_ref().ok_or_else(|| {
            source::Error::new("unavailable", "Launcher service unavailable", true)
        })?;
        match self.runtime.block_on(source.call(
            "menu.activate".into(),
            serde_json::to_value(request).map_err(|_| invalid_menu())?,
            Duration::from_secs(3),
        )) {
            Ok(reply) => {
                let result: MenuActivated =
                    serde_json::from_value(reply).map_err(|_| invalid_menu())?;
                result.validate().map_err(|_| invalid_menu())?;
                if result.keep_open {
                    self.view.borrow_mut().request = None;
                }
                Ok(result)
            }
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
        let mode = text("launcher.mode");
        let request = (
            text("launcher.search.text"),
            if mode.is_empty() { "apps".into() } else { mode },
            text("launcher.engine_route"),
        );
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
                        json!({"query":request.0,"mode":request.1,"route":if request.2.is_empty(){Json::Null}else{json!(request.2)}}),
                        Duration::from_secs(3),
                    ))
                    .and_then(prepare)
            } else {
                Err(source::Error::new(
                    "unavailable",
                    "Launcher service unavailable",
                    true,
                ))
            };
            match result {
                Ok(prepared) => {
                    view.rows = prepared.rows;
                    view.actions = prepared.actions;
                    view.catalog_rev = prepared.catalog_rev;
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
    fn published_menu_payloads_preserve_order_and_keep_activation_out_of_luau() {
        use nacre_protocol::{Message, Reply, framing};
        let mut populated = false;
        for trace in nacre_protocol::artifacts::menu_transcripts().values() {
            for line in trace.lines() {
                if let Message::Ret {
                    reply: Reply::Ok { ok },
                    ..
                } = framing::decode(line.as_bytes()).unwrap()
                    && ok.get("items").is_some()
                {
                    let expected: MenuQueryResult = serde_json::from_value(ok.clone()).unwrap();
                    let prepared = prepare(ok).unwrap();
                    assert_eq!(
                        prepared
                            .rows
                            .iter()
                            .map(|r| r["id"].as_str().unwrap())
                            .collect::<Vec<_>>(),
                        expected
                            .order
                            .iter()
                            .map(String::as_str)
                            .collect::<Vec<_>>()
                    );
                    assert_eq!(prepared.catalog_rev, expected.catalog_rev);
                    assert!(prepared.rows.iter().all(|r| r.get("activation").is_none()));
                    populated |= prepared.rows.len() == 320;
                }
            }
        }
        assert!(populated);
        assert!(
            rows(json!({"rows":[]})).is_err(),
            "obsolete fixture format must not masquerade as a protocol result"
        );
    }
    #[test]
    fn opaque_activation_revision_and_navigation_are_engine_owned() {
        struct Engine {
            publisher: source::Publisher,
            called: std::sync::Mutex<Vec<Json>>,
        }
        impl Source for Engine {
            fn snapshot(&self) -> Arc<source::Snapshot> {
                self.publisher.snapshot()
            }
            fn subscribe(&self, cb: source::Callback) -> source::Result<source::Subscription> {
                self.publisher.subscribe(cb)
            }
            fn invoke(&self, method: String, params: Json) -> source::Reply<'_> {
                Box::pin(async move {
                    if method == "menu.query" {
                        return Ok(fixture_reply(
                            params["query"].as_str().unwrap_or(""),
                            vec![json!({"id":"chosen","name":"Fixture"})],
                        ));
                    }
                    assert_eq!(method, "menu.activate");
                    self.called.lock().unwrap().push(params);
                    Ok(
                        json!({"keep_open":true,"view":{"mode":"actions","route":"opaque-route","query":"> "}}),
                    )
                })
            }
            fn reconnect(&self) -> source::Reply<'_> {
                Box::pin(async { Ok(Json::Null) })
            }
            fn stop(&self) -> source::Reply<'_> {
                Box::pin(async { Ok(Json::Null) })
            }
        }
        let engine = Arc::new(Engine {
            publisher: source::Publisher::new("test".into(), json!({})).unwrap(),
            called: Default::default(),
        });
        let menu = Menu::new(Some(engine.clone())).unwrap();
        menu.project(&Stores::default()).unwrap();
        let result = menu.activate("chosen").unwrap();
        assert_eq!(
            engine.called.lock().unwrap().as_slice(),
            &[json!({"id":"chosen","activation":"fixture:chosen","catalog_rev":1})]
        );
        let changes = activation_changes(result);
        assert_eq!(changes["panel.open"], Value::Bool(true));
        assert_eq!(
            changes["launcher.engine_route"],
            Value::Text("opaque-route".into())
        );
        assert_eq!(changes["launcher.mode"], Value::Text("actions".into()));
        assert_eq!(
            changes["bead.focused"],
            Value::Text("launcher.search".into())
        );
        assert!(menu.activate("not-in-result").is_err());
        assert_eq!(engine.called.lock().unwrap().len(), 1);
    }
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
        let rows = rows(fixture_reply(
            "",
            vec![
                json!({"id":"a","name":"A","breadcrumb":"All apps","description":"Other"}),
                json!({"id":"b","name":"B","description":"Fallback"}),
            ],
        ))
        .unwrap();
        assert_eq!(rows[0]["description"], "All apps");
        assert_eq!(rows[1]["description"], "Fallback");
    }
    #[test]
    fn engine_order_survives_and_invalid_replies_fail() {
        let reply = fixture_reply(
            "",
            vec![
                json!({"id":"z","name":"Zulu"}),
                json!({"id":"a","name":"Alpha"}),
            ],
        );
        let r = rows(reply.clone()).unwrap();
        assert_eq!(r[0]["id"], "z");
        assert_eq!(r[1]["name"], "Alpha");
        assert!(
            rows(fixture_reply(
                "",
                vec![json!({"id":"x","name":"X"}), json!({"id":"x","name":"X"})]
            ))
            .is_err()
        );
    }
    #[test]
    fn a_source_revision_refreshes_the_same_query() {
        let source = Arc::new(
            Fixture::new(
                json!({"queries":{"":fixture_reply("",vec![json!({"id":"x","name":"Old"})])}}),
            )
            .unwrap(),
        );
        let menu = Menu::new(Some(source.clone())).unwrap();
        let stores = Stores::default();
        assert!(
            matches!(&menu.project(&stores).unwrap()["launcher.apps"],Value::List(r) if r[0]["name"]=="Old")
        );
        source
            .publisher
            .publish(json!({"queries":{"":fixture_reply("",vec![json!({"id":"x","name":"New"})])}}))
            .unwrap();
        assert!(
            matches!(&menu.project(&stores).unwrap()["launcher.apps"],Value::List(r) if r[0]["name"]=="New")
        );
    }
    #[test]
    fn exact_fixture_queries_do_not_filter_or_rank() {
        let source=Arc::new(Fixture::new(json!({"queries":{"":fixture_reply("",vec![json!({"id":"z","name":"Zulu"}),json!({"id":"a","name":"Alpha"})]),"z":fixture_reply("z",vec![json!({"id":"a","name":"Alpha"})])}})).unwrap());
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
