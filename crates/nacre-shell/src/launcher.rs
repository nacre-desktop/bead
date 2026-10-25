// SPDX-License-Identifier: GPL-3.0-only
//! View projection of ordered engine results. No catalog, providers or ranking.
use crate::i18n::tr;
use bead::core::{
    Stores, Value,
    source::{self, Source},
};
use nacre_protocol::menu::{
    MAX_ITEMS, MenuActivate, MenuActivated, MenuConversion, MenuGroupsResult, MenuQuery,
    MenuQueryResult,
};
use serde_json::{Value as Json, json};
use std::{
    cell::RefCell,
    collections::{BTreeMap, VecDeque},
    path::Path,
    sync::Arc,
    time::Duration,
};

pub type Memory = Arc<std::sync::Mutex<BTreeMap<String, Value>>>;
type MenuRequest = (String, String, String);
type SourceStamp = (String, u64, bool);
type PendingQuery = (MenuRequest, Option<SourceStamp>);
/// Mirror only bounded live view state; Runtime drops it with the panel worker.
/// Never retain catalogs, results, rasters or font pools.
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
#[derive(Clone, Default)]
pub struct SourceChoice {
    pub fixture: Option<std::path::PathBuf>,
    pub socket: Option<std::path::PathBuf>,
    pub shared: Option<Arc<dyn Source>>,
    pub client: Option<crate::nacre_source::Client>,
}
impl SourceChoice {
    /// Acquired by the panel factory, never by the persistent bar configuration.
    pub fn quick(&self) -> Result<crate::launcher_quick::Quick, Box<dyn std::error::Error>> {
        let client = if let Some(client) = &self.client {
            Some(client.clone())
        } else if let Some(socket) = &self.socket {
            Some(crate::nacre_source::Client::start(
                crate::nacre_source::Config::new(socket.clone(), Vec::new()),
            )?)
        } else {
            None
        };
        Ok(match client {
            Some(client) => crate::launcher_quick::Quick::lease(client)?,
            None => crate::launcher_quick::Quick::new(None, Vec::new()),
        })
    }
    pub fn connect(&self) -> Result<Option<Arc<dyn Source>>, Box<dyn std::error::Error>> {
        if let Some(source) = &self.shared {
            Ok(Some(source.clone()))
        } else {
            source(self.fixture.as_deref(), self.socket.as_deref())
        }
    }
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
        let client = crate::nacre_source::Client::start(crate::nacre_source::Config::new(
            socket.into(),
            vec!["menu".into()],
        ))?;
        // Catalog generations must invalidate the view even without a query edit.
        Ok(Some(Arc::new(client.topic("menu")?)))
    } else {
        Ok(None)
    }
}

/// Only display data crosses into Luau; opaque activation stays in the host.
struct Prepared {
    mode: String,
    route: String,
    rows: Vec<Json>,
    total: usize,
    total_known: bool,
    actions: BTreeMap<String, String>,
    catalog_rev: u64,
    conversion: Option<MenuConversion>,
    message: String,
}
fn invalid_menu() -> source::Error {
    source::Error::new("validation", "Invalid menu method payload", false)
}
fn favorite_id(row: &Json) -> String {
    row.get("favoriteId")
        .and_then(Json::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .unwrap_or_else(|| {
            row.get("id")
                .and_then(Json::as_str)
                .unwrap_or_default()
                .to_owned()
        })
}
fn parse_favorites(value: &Json) -> source::Result<std::collections::BTreeSet<String>> {
    let strings = |key: &str| -> source::Result<()> {
        value
            .get(key)
            .and_then(Json::as_array)
            .filter(|rows| rows.iter().all(Json::is_string))
            .map(|_| ())
            .ok_or_else(invalid_menu)
    };
    strings("favorites")?;
    strings("hidden")?;
    strings("recent")?;
    if !value.get("usage").is_some_and(Json::is_object) {
        return Err(invalid_menu());
    }
    Ok(value["favorites"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(Json::as_str)
        .map(str::to_owned)
        .collect())
}
pub(crate) fn recent_ids(value: &Json) -> source::Result<Vec<String>> {
    parse_favorites(value)?;
    Ok(value["recent"]
        .as_array()
        .unwrap()
        .iter()
        .take(16)
        .filter_map(Json::as_str)
        .map(str::to_owned)
        .collect())
}
fn prepare(reply: Json, expected_query: Option<&str>) -> source::Result<Prepared> {
    let mut result: MenuQueryResult = serde_json::from_value(reply).map_err(|_| invalid_menu())?;
    result.validate().map_err(|_| invalid_menu())?;
    if expected_query.is_some_and(|query| result.query.as_str() != query) {
        return Err(invalid_menu());
    }
    // Preserve the engine's order and current2048-row response ceiling.
    let total = result.total.map(|n| (n as usize).min(MAX_ITEMS));
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
        mode: result.mode,
        route: result.route,
        total: total.unwrap_or(rows.len()).max(rows.len()),
        total_known: total.is_some(),
        rows,
        actions,
        catalog_rev: result.catalog_rev,
        conversion: result.conversion,
        message: result.message.unwrap_or_default(),
    })
}
pub fn rows(reply: Json) -> source::Result<Vec<Json>> {
    Ok(prepare(reply, None)?.rows)
}
/// `menu.groups` as the `launcher.groups` store rows. The caller owns hiding and order.
fn parse_groups(reply: Json) -> Option<Vec<Json>> {
    let result: MenuGroupsResult = serde_json::from_value(reply).ok()?;
    result.validate().ok()?;
    Some(
        result
            .groups
            .into_iter()
            .map(|g| json!({"id":g.id,"name":g.label,"icon":g.icon}))
            .collect(),
    )
}
fn prepare_for(reply: Json, request: &(String, String, String)) -> source::Result<Prepared> {
    let prepared = prepare(reply, Some(&request.0))?;
    let expected_route = if request.2.is_empty() {
        match request.1.as_str() {
            "apps" => "apps",
            "search" => "home",
            "actions" => "actions",
            _ => "",
        }
    } else {
        request.2.as_str()
    };
    // The daemon owns route validity and ranking; the shell only fences replies
    // whose echoed query scope does not match the request it issued.
    // Re-read the typed result fields without changing or sorting its rows.
    if prepared.mode != request.1 || prepared.route != expected_route {
        return Err(invalid_menu());
    }
    Ok(prepared)
}

async fn query(source: &dyn Source, params: Json, timeout: Duration) -> source::Result<Json> {
    let deadline = std::time::Instant::now() + timeout;
    let mut delay = Duration::from_millis(25);
    loop {
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        let result = source
            .call("menu.query".into(), params.clone(), remaining)
            .await;
        match result {
            Err(error) if error.code == "busy" && error.retry => {
                if deadline.saturating_duration_since(std::time::Instant::now()) <= delay {
                    return Err(error);
                }
                // Only retry this idempotent read, within its original deadline.
                // Activation is deliberately never routed through this helper.
                tokio::time::sleep(delay).await;
                delay = (delay * 2).min(Duration::from_millis(400));
            }
            result => return result,
        }
    }
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

/// The nine top-level groups `menu.groups` publishes (English labels), as store rows.
#[cfg(test)]
pub(crate) fn fixture_groups() -> Value {
    Value::list(
        [
            ("home", "Home", "home"),
            ("apps", "All apps", "apps"),
            ("actions", "Actions", "bolt"),
            ("style", "Style", "palette"),
            ("setup", "Setup", "tune"),
            ("software", "Software", "download"),
            ("health", "Health", "build"),
            ("learn", "Learn", "help"),
            ("power", "Power", "power_settings_new"),
        ]
        .map(|(id, name, icon)| json!({"id":id,"name":name,"icon":icon}))
        .to_vec(),
    )
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
    favorites: Arc<std::sync::Mutex<std::collections::BTreeSet<String>>>,
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
        let favorites = data
            .get("preferences")
            .map(parse_favorites)
            .transpose()?
            .unwrap_or_default();
        Ok(Self {
            publisher: source::Publisher::new("menu-fixture".into(), data)?,
            favorites: Arc::new(std::sync::Mutex::new(favorites)),
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
        let saved_favorites = self.favorites.clone();
        Box::pin(async move {
            if method == "menu.preferences" {
                let action = params
                    .get("action")
                    .and_then(Json::as_str)
                    .ok_or_else(invalid_menu)?;
                let mut favorites = saved_favorites.lock().map_err(|_| invalid_menu())?;
                if action == "favorite" {
                    let value = params.get("value").ok_or_else(invalid_menu)?;
                    let id = value
                        .get("id")
                        .and_then(Json::as_str)
                        .ok_or_else(invalid_menu)?;
                    let enabled = value
                        .get("enabled")
                        .and_then(Json::as_bool)
                        .ok_or_else(invalid_menu)?;
                    if enabled {
                        favorites.insert(id.into());
                    } else {
                        favorites.remove(id);
                    }
                } else if action != "load" {
                    return Err(invalid_menu());
                }
                return Ok(
                    json!({"favorites":favorites.iter().cloned().collect::<Vec<_>>(),"hidden":[],"recent":[],"usage":{}}),
                );
            }
            if method == "menu.groups" {
                let groups = self.snapshot().data.get("groups").cloned();
                return groups
                    .map(|groups| json!({"catalog_rev":1,"groups":groups}))
                    .ok_or_else(|| {
                        source::Error::new("unavailable", "no groups in fixture", false)
                    });
            }
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
            let reply = snapshot
                .data
                .get("queries")
                .and_then(|q| q.get(query))
                .ok_or_else(|| {
                    source::Error::new("unavailable", "query absent from fixture", false)
                })?;
            if let Some(limit) = request.limit {
                // Fixture results are already ordered: only mimic the wire prefix.
                // Never clone the full item map, filter, interpret or rank a query.
                let mut result = reply
                    .as_object()
                    .ok_or_else(invalid_menu)?
                    .iter()
                    .filter(|(key, _)| !matches!(key.as_str(), "items" | "order"))
                    .map(|(key, value)| (key.clone(), value.clone()))
                    .collect::<serde_json::Map<_, _>>();
                let order = reply["order"].as_array().ok_or_else(invalid_menu)?;
                let prefix = order
                    .iter()
                    .take(limit as usize)
                    .cloned()
                    .collect::<Vec<_>>();
                let items = prefix
                    .iter()
                    .map(|id| {
                        let key = id.as_str().ok_or_else(invalid_menu)?;
                        Ok((
                            key.to_owned(),
                            reply["items"].get(key).cloned().ok_or_else(invalid_menu)?,
                        ))
                    })
                    .collect::<source::Result<serde_json::Map<_, _>>>()?;
                result.insert("order".into(), Json::Array(prefix));
                result.insert("items".into(), Json::Object(items));
                Ok(Json::Object(result))
            } else {
                Ok(reply.clone())
            }
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

/// The last settled grid (empty query, apps) result, kept only while its panel worker lives.
/// Returning to the grid with an unchanged source stamp reuses it, so one key makes one commit.
struct GridResult {
    stamp: Option<SourceStamp>,
    rows: Arc<Vec<Json>>,
    total: usize,
    actions: BTreeMap<String, String>,
    catalog_rev: u64,
    conversion: Option<MenuConversion>,
    message: String,
}
#[derive(Default)]
struct View {
    grid: Option<GridResult>,
    request: Option<MenuRequest>,
    result_request: Option<MenuRequest>,
    pending: Option<PendingQuery>,
    pending_limited: bool,
    query_generation: u64,
    pending_navigation: VecDeque<(String, NavigationGeometry)>,
    actions: BTreeMap<String, String>,
    catalog_rev: u64,
    stamp: Option<SourceStamp>,
    result_stamp: Option<SourceStamp>,
    rows: Arc<Vec<Json>>,
    total: usize,
    force_full: bool,
    last_scroll: f64,
    error: String,
    conversion: Option<MenuConversion>,
    message: String,
}
#[derive(Clone)]
struct QueryTask {
    request: MenuRequest,
    stamp: Option<SourceStamp>,
    generation: u64,
    params: Json,
}
struct QueryCompletion {
    task: QueryTask,
    result: source::Result<Prepared>,
}
struct QueryActor {
    latest: tokio::sync::watch::Sender<Option<QueryTask>>,
    completion: Arc<std::sync::Mutex<Option<QueryCompletion>>>,
    wake: Arc<std::sync::Mutex<Option<source::Callback>>>,
    favorites: Arc<std::sync::Mutex<std::collections::BTreeSet<String>>>,
    preferences_error: Arc<std::sync::Mutex<Option<String>>>,
    preferences_generation: Arc<std::sync::atomic::AtomicU64>,
    preferences_loaded: Arc<std::sync::atomic::AtomicBool>,
    favorite_in_flight: Arc<std::sync::atomic::AtomicBool>,
    /// Engine rail groups (`menu.groups`) as `{id,name,icon}` store rows; empty until loaded.
    groups: Arc<std::sync::Mutex<Vec<Json>>>,
    runtime_handle: tokio::runtime::Handle,
    cancelled: Arc<std::sync::atomic::AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}
impl QueryActor {
    fn new(source: Arc<dyn Source>) -> Result<Self, String> {
        let (latest, mut receiver) = tokio::sync::watch::channel(None::<QueryTask>);
        let completion = Arc::new(std::sync::Mutex::new(None::<QueryCompletion>));
        let wake = Arc::new(std::sync::Mutex::new(None::<source::Callback>));
        let favorites = Arc::new(std::sync::Mutex::new(std::collections::BTreeSet::new()));
        let preferences_error = Arc::new(std::sync::Mutex::new(None::<String>));
        let preferences_generation = Arc::new(std::sync::atomic::AtomicU64::new(0));
        let preferences_loaded = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let favorite_in_flight = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let groups = Arc::new(std::sync::Mutex::new(Vec::<Json>::new()));
        let worker_groups = groups.clone();
        let done = completion.clone();
        let notify = wake.clone();
        let worker_favorites = favorites.clone();
        let worker_preferences_error = preferences_error.clone();
        let worker_preferences_generation = preferences_generation.clone();
        let worker_preferences_loaded = preferences_loaded.clone();
        let cancelled = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let thread_cancelled = cancelled.clone();
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|error| format!("cannot build menu query runtime: {error}"))?;
        let runtime_handle = runtime.handle().clone();
        let thread = std::thread::Builder::new()
            .name("bead-menu-query".into())
            .spawn(move || {
                runtime.block_on(async move {
                    let mut preference_task: Option<tokio::task::JoinHandle<()>> = None;
                    let mut groups_task: Option<tokio::task::JoinHandle<()>> = None;
                    let mut preference_stamp: Option<SourceStamp> = None;
                    let mut newest = None;
                    loop {
                        let task = if let Some(task) = newest.take() {
                            task
                        } else {
                            if receiver.changed().await.is_err() || thread_cancelled.load(std::sync::atomic::Ordering::Acquire) { break; }
                            let Some(task) = receiver.borrow_and_update().clone() else { continue; };
                            task
                        };
                        if preference_stamp.as_ref() != task.stamp.as_ref() {
                            preference_stamp = task.stamp.clone();
                            if let Some(previous) = preference_task.take() {
                                previous.abort();
                                let _ = previous.await;
                            }
                            if let Some(previous) = groups_task.take() {
                                previous.abort();
                                let _ = previous.await;
                            }
                            let groups_source = source.clone();
                            let groups_slot = worker_groups.clone();
                            let groups_notify = notify.clone();
                            groups_task = Some(tokio::spawn(async move {
                                // Older daemons do not publish the method: keep the fallback rail.
                                let Some(rows) = groups_source
                                    .call("menu.groups".into(), json!({}), Duration::from_secs(3))
                                    .await
                                    .ok()
                                    .and_then(parse_groups)
                                else {
                                    return;
                                };
                                let changed = groups_slot.lock().is_ok_and(|mut slot| {
                                    let changed = *slot != rows;
                                    *slot = rows;
                                    changed
                                });
                                if changed
                                    && let Some(callback) = groups_notify.lock().ok().and_then(|slot| slot.clone())
                                {
                                    callback(groups_source.snapshot());
                                }
                            }));
                            let prefs_source = source.clone();
                            let prefs = worker_favorites.clone();
                            let prefs_error = worker_preferences_error.clone();
                            let prefs_generation = worker_preferences_generation.clone();
                            let prefs_loaded = worker_preferences_loaded.clone();
                            let prefs_notify = notify.clone();
                            let expected_generation = prefs_generation.fetch_add(1, std::sync::atomic::Ordering::AcqRel) + 1;
                            preference_task = Some(tokio::spawn(async move {
                                let result = prefs_source
                                    .call("menu.preferences".into(), json!({"action":"load"}), Duration::from_secs(3))
                                    .await
                                    .and_then(|value| parse_favorites(&value));
                                if expected_generation == prefs_generation.load(std::sync::atomic::Ordering::Acquire) {
                                    match result {
                                        Ok(current) => {
                                            if let Ok(mut saved) = prefs.lock() { *saved = current; }
                                            if let Ok(mut error) = prefs_error.lock() { *error = None; }
                                        }
                                        Err(error) => if let Ok(mut saved_error) = prefs_error.lock() { *saved_error = Some(error.message); },
                                    }
                                    prefs_loaded.store(true, std::sync::atomic::Ordering::Release);
                                    if let Some(callback) = prefs_notify.lock().ok().and_then(|slot| slot.clone()) {
                                        callback(prefs_source.snapshot());
                                    }
                                }
                            }));
                        }
                        let expected = task.request.clone();
                        let limited = task.params.get("limit").is_some();
                        let mut full_params = task.params.clone();
                        if let Some(params) = full_params.as_object_mut() { params.remove("limit"); }
                        let source_ref = source.as_ref();
                        let params = task.params.clone();
                        let reply = async move {
                            let _query_span = bead::core::profile::Span::new("menu_query");
                            let first = query(source_ref, params, Duration::from_secs(3)).await?;
                            let prepared = prepare_for(first, &expected)?;
                            if limited && (!prepared.total_known || (prepared.total > prepared.rows.len()
                                && !prepared.rows.iter().any(|row| row["disabled"] != true))) {
                                let full = query(source_ref, full_params, Duration::from_secs(3)).await?;
                                prepare_for(full, &expected)
                            } else { Ok(prepared) }
                        };
                        tokio::pin!(reply);
                        let result = tokio::select! {
                            result = &mut reply => Some(result),
                            changed = receiver.changed() => {
                                if changed.is_err() || thread_cancelled.load(std::sync::atomic::Ordering::Acquire) { return; }
                                newest = receiver.borrow_and_update().clone();
                                None
                            }
                        };
                        if let Some(result) = result {
                            if let Ok(mut slot) = done.lock() {
                                *slot = Some(QueryCompletion { task, result });
                            }
                            if let Some(callback) = notify.lock().ok().and_then(|slot| slot.clone()) {
                                callback(source.snapshot());
                            }
                        }
                    }
                });
            })
            .map_err(|error| format!("cannot start menu query actor: {error}"))?;
        Ok(Self {
            latest,
            completion,
            wake,
            favorites,
            preferences_error,
            preferences_generation,
            preferences_loaded,
            favorite_in_flight,
            groups,
            runtime_handle,
            cancelled,
            thread: Some(thread),
        })
    }
    fn schedule(&self, task: QueryTask) {
        self.latest.send_replace(Some(task));
    }
}
impl Drop for QueryActor {
    fn drop(&mut self) {
        self.cancelled
            .store(true, std::sync::atomic::Ordering::Release);
        self.latest.send_replace(None);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
fn navigation_target(
    rows: &[Json],
    selection: &str,
    mut scroll: f64,
    key: &str,
    geometry: NavigationGeometry,
    grid: bool,
) -> (String, f64) {
    let direction = match key {
        "up" | "left" => -1,
        "down" | "right" => 1,
        _ => return (String::new(), 0.),
    };
    let current = rows
        .iter()
        .position(|row| row["id"].as_str() == Some(selection));
    let count = rows.len();
    if count == 0 {
        return (String::new(), 0.);
    }
    let mut next = current.map_or(if direction > 0 { 0 } else { count - 1 }, |index| {
        if grid && matches!(key, "up" | "down") {
            (index as i64 + direction * geometry.columns.max(1) as i64)
                .clamp(0, count.saturating_sub(1) as i64) as usize
        } else {
            (index as i64 + direction).rem_euclid(count as i64) as usize
        }
    });
    let mut selected = None;
    for _ in 0..count {
        if rows[next]["disabled"] != true {
            selected = Some(next);
            break;
        }
        let candidate = next as i64 + direction;
        if grid && matches!(key, "up" | "down") && !(0..count as i64).contains(&candidate) {
            selected = current.filter(|index| rows[*index]["disabled"] != true);
            break;
        }
        next = candidate.rem_euclid(count as i64) as usize;
    }
    if let Some(index) = selected {
        let columns = if grid { geometry.columns.max(1) } else { 1 };
        if geometry.pitch.is_finite()
            && geometry.pitch > 0.
            && geometry.row_height > 0.
            && geometry.viewport_height > 0.
        {
            let top = (index / columns) as f64 * geometry.pitch;
            let bottom = top + geometry.row_height;
            if top < scroll {
                scroll = top;
            } else if bottom > scroll + geometry.viewport_height {
                scroll = bottom - geometry.viewport_height;
            }
            let maximum = (count.div_ceil(columns).saturating_sub(1) as f64 * geometry.pitch
                + geometry.row_height
                - geometry.viewport_height)
                .max(0.);
            scroll = scroll.clamp(0., maximum);
        }
    } else {
        scroll = 0.;
    }
    (
        selected
            .and_then(|index| rows[index]["id"].as_str())
            .unwrap_or("")
            .into(),
        scroll,
    )
}
#[derive(Clone, Copy)]
pub struct NavigationGeometry {
    pub columns: usize,
    pub pitch: f64,
    pub row_height: f64,
    pub viewport_height: f64,
}
pub struct Menu {
    source: Option<Arc<dyn Source>>,
    runtime: tokio::runtime::Runtime,
    view: RefCell<View>,
    query_actor: Option<QueryActor>,
}
impl Menu {
    pub fn new(source: Option<Arc<dyn Source>>) -> Result<Self, String> {
        let query_actor = source.clone().map(QueryActor::new).transpose()?;
        Ok(Self {
            source,
            runtime: tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .max_blocking_threads(2)
                .build()
                .map_err(|e| e.to_string())?,
            view: RefCell::new(View::default()),
            query_actor,
        })
    }
    pub fn subscribe(
        &self,
        callback: source::Callback,
    ) -> source::Result<Option<source::Subscription>> {
        if let (Some(source), Some(actor)) = (&self.source, &self.query_actor)
            && let Ok(mut wake) = actor.wake.lock()
        {
            let source = source.clone();
            let callback_for_wake = callback.clone();
            *wake = Some(Arc::new(move |_| callback_for_wake(source.snapshot())));
        }
        self.source
            .as_ref()
            .map(|s| s.subscribe(callback))
            .transpose()
    }
    pub fn favorite(&self, row_id: &str) -> source::Result<()> {
        if self.query_actor.as_ref().is_some_and(|actor| {
            !actor
                .preferences_loaded
                .load(std::sync::atomic::Ordering::Acquire)
        }) {
            let error =
                source::Error::new("busy", &tr("bead.launcher.preferences-loading", &[]), true);
            if let Some(actor) = &self.query_actor
                && let Ok(mut saved_error) = actor.preferences_error.lock()
            {
                *saved_error = Some(error.message.clone());
            }
            return Err(error);
        }
        let (id, enabled) = {
            let view = self.view.borrow();
            let row = view
                .rows
                .iter()
                .find(|row| row["id"].as_str() == Some(row_id))
                .ok_or_else(|| {
                    source::Error::new(
                        "unavailable",
                        &tr("bead.launcher.result-unavailable", &[]),
                        true,
                    )
                })?;
            (favorite_id(row), row["favorite"] != true)
        };
        let source = self.source.as_ref().ok_or_else(|| {
            source::Error::new(
                "unavailable",
                &tr("bead.launcher.service-unavailable", &[]),
                true,
            )
        })?;
        let actor = self.query_actor.as_ref().ok_or_else(|| {
            source::Error::new(
                "unavailable",
                &tr("bead.launcher.service-unavailable", &[]),
                true,
            )
        })?;
        if actor
            .favorite_in_flight
            .compare_exchange(
                false,
                true,
                std::sync::atomic::Ordering::AcqRel,
                std::sync::atomic::Ordering::Acquire,
            )
            .is_err()
        {
            let error = source::Error::new("busy", &tr("bead.launcher.favorite-busy", &[]), true);
            if let Ok(mut saved_error) = actor.preferences_error.lock() {
                *saved_error = Some(error.message.clone());
            }
            return Err(error);
        }
        let source = source.clone();
        let favorites = actor.favorites.clone();
        let preferences_error = actor.preferences_error.clone();
        let preferences_generation = actor.preferences_generation.clone();
        let preferences_loaded = actor.preferences_loaded.clone();
        let favorite_in_flight = actor.favorite_in_flight.clone();
        let wake = actor.wake.clone();
        actor.runtime_handle.spawn(async move {
            let result = source
                .call(
                    "menu.preferences".into(),
                    json!({"action":"favorite","value":{"id":id,"enabled":enabled}}),
                    Duration::from_secs(3),
                )
                .await
                .and_then(|result| parse_favorites(&result));
            match result {
                Ok(current) => {
                    if let Ok(mut saved) = favorites.lock() {
                        preferences_generation.fetch_add(1, std::sync::atomic::Ordering::AcqRel);
                        *saved = current;
                        preferences_loaded.store(true, std::sync::atomic::Ordering::Release);
                    }
                    if let Ok(mut error) = preferences_error.lock() {
                        *error = None;
                    }
                }
                Err(error) => {
                    if let Ok(mut saved_error) = preferences_error.lock() {
                        *saved_error = Some(error.message);
                    }
                }
            }
            favorite_in_flight.store(false, std::sync::atomic::Ordering::Release);
            if let Some(callback) = wake.lock().ok().and_then(|slot| slot.clone()) {
                callback(source.snapshot());
            }
        });
        Ok(())
    }
    /// View-only navigation through the engine's existing order, never ranking.
    pub fn navigate(
        &self,
        stores: &Stores,
        key: &str,
        geometry: NavigationGeometry,
    ) -> Option<BTreeMap<String, Value>> {
        let query = matches!(stores.get("launcher.search.text"),Some(Value::Text(text))if !text.trim().is_empty());
        let grid = !query
            && stores
                .get("launcher.mode")
                .is_none_or(|value| matches!(value,Value::Text(mode)if mode=="apps"));
        let direction = match key {
            "up" => -1,
            "down" => 1,
            "left" if grid => -1,
            "right" if grid => 1,
            _ => return None,
        };
        let needs_full = {
            let view = self.view.borrow();
            let current = match stores.get("launcher.selection") {
                Some(Value::Text(id)) => view
                    .rows
                    .iter()
                    .position(|row| row["id"].as_str() == Some(id.as_str())),
                _ => None,
            };
            view.total > view.rows.len()
                && if direction > 0 {
                    !view
                        .rows
                        .iter()
                        .skip(current.map_or(0, |i| i + 1))
                        .any(|row| row["disabled"] != true)
                } else {
                    current
                        .is_none_or(|i| !view.rows[..i].iter().any(|row| row["disabled"] != true))
                }
        };
        let mut expanded = BTreeMap::new();
        if needs_full {
            // First visible selections stay local. Fetch before crossing an
            // unloaded boundary or wrapping, including disabled-row tails.
            self.view.borrow_mut().force_full = true;
            expanded = self.project(stores).ok()?;
            if expanded.get("launcher.pending") == Some(&Value::Bool(true)) {
                let mut view = self.view.borrow_mut();
                if view.pending_navigation.len() == MAX_ITEMS {
                    view.pending_navigation.pop_front();
                }
                view.pending_navigation
                    .push_back((key.to_owned(), geometry));
                return Some(expanded);
            }
            if !self.view.borrow().error.is_empty() {
                return Some(expanded);
            }
        }
        let view = self.view.borrow();
        let current = match stores.get("launcher.selection") {
            Some(Value::Text(id)) => view
                .rows
                .iter()
                .position(|row| row["id"].as_str() == Some(id.as_str())),
            _ => None,
        };
        let count = view.rows.len();
        let mut next = current.map_or(
            if direction > 0 {
                0
            } else {
                count.saturating_sub(1)
            },
            |index| {
                if grid && matches!(key, "up" | "down") {
                    (index as i64 + direction * geometry.columns.max(1) as i64)
                        .clamp(0, count.saturating_sub(1) as i64) as usize
                } else {
                    (index as i64 + direction).rem_euclid(count.max(1) as i64) as usize
                }
            },
        );
        let mut selected = None;
        for _ in 0..count {
            if view.rows[next]["disabled"] != true {
                selected = Some(next);
                break;
            }
            let candidate = next as i64 + direction;
            if grid && matches!(key, "up" | "down") && !(0..count as i64).contains(&candidate) {
                selected = current.filter(|index| view.rows[*index]["disabled"] != true);
                break;
            }
            next = candidate.rem_euclid(count as i64) as usize;
        }
        let mut scroll = match stores.get("launcher.scroll.y") {
            Some(Value::Number(value)) => *value,
            _ => 0.,
        };
        if let Some(index) = selected {
            let columns = if grid { geometry.columns.max(1) } else { 1 };
            if geometry.pitch.is_finite()
                && geometry.pitch > 0.
                && geometry.row_height > 0.
                && geometry.viewport_height > 0.
            {
                let top = (index / columns) as f64 * geometry.pitch;
                let bottom = top + geometry.row_height;
                if top < scroll {
                    scroll = top;
                } else if bottom > scroll + geometry.viewport_height {
                    scroll = bottom - geometry.viewport_height;
                }
                let rows = count.div_ceil(columns);
                let maximum = (rows.saturating_sub(1) as f64 * geometry.pitch
                    + geometry.row_height
                    - geometry.viewport_height)
                    .max(0.);
                scroll = scroll.clamp(0., maximum);
            }
        } else {
            scroll = 0.;
        }
        expanded.extend(BTreeMap::from([
            (
                "launcher.selection".into(),
                Value::Text(
                    selected
                        .and_then(|index| view.rows[index]["id"].as_str())
                        .unwrap_or("")
                        .into(),
                ),
            ),
            ("launcher.scroll.y".into(), Value::Number(scroll)),
        ]));
        Some(expanded)
    }
    pub fn activate(&self, id: &str) -> source::Result<MenuActivated> {
        let view = self.view.borrow();
        let current = self.source.as_ref().map(|source| source.snapshot());
        let current_stamp = current.as_ref().map(|snapshot| {
            (
                snapshot.epoch.clone(),
                snapshot.revision,
                snapshot.connected,
            )
        });
        if !current.as_ref().is_some_and(|snapshot| snapshot.connected)
            || view.result_stamp != current_stamp
            || view.pending.is_some()
            || !view.pending_navigation.is_empty()
            || view.result_request != view.request
        {
            return Err(source::Error::new(
                "stale_revision",
                &tr("bead.launcher.changed", &[]),
                true,
            ));
        }
        if !view.error.is_empty()
            || !view
                .rows
                .iter()
                .any(|r| r["id"].as_str() == Some(id) && r["disabled"] != true)
        {
            return Err(source::Error::new(
                "unavailable",
                &tr("bead.launcher.result-unavailable", &[]),
                true,
            ));
        }
        let request = MenuActivate {
            id: id.into(),
            activation: view.actions.get(id).cloned().ok_or_else(|| {
                source::Error::new(
                    "unavailable",
                    &tr("bead.launcher.no-activation", &[]),
                    false,
                )
            })?,
            catalog_rev: view.catalog_rev,
        };
        request.validate().map_err(|_| invalid_menu())?;
        drop(view);
        let source = self.source.as_ref().ok_or_else(|| {
            source::Error::new(
                "unavailable",
                &tr("bead.launcher.service-unavailable", &[]),
                true,
            )
        })?;
        let result = self
            .runtime
            .block_on(source.call(
                "menu.activate".into(),
                serde_json::to_value(request).map_err(|_| invalid_menu())?,
                Duration::from_secs(3),
            ))
            .and_then(|reply| {
                let result: MenuActivated =
                    serde_json::from_value(reply).map_err(|_| invalid_menu())?;
                result.validate().map_err(|_| invalid_menu())?;
                Ok(result)
            });
        match result {
            Ok(result) => {
                if result.keep_open {
                    let mut view = self.view.borrow_mut();
                    view.request = None;
                    view.grid = None;
                    view.pending_navigation.clear();
                }
                Ok(result)
            }
            Err(e) => {
                let mut view = self.view.borrow_mut();
                view.error = e.message.clone();
                if e.code == "stale_revision" {
                    // Same as QML: drop the stale rows and query again.
                    view.request = None;
                    view.grid = None;
                    view.result_stamp = None;
                }
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
        let mut mode = text("launcher.mode");
        let mut restored_route = false;
        if self.view.borrow().request.is_none()
            && mode == "apps"
            && let Some(Value::Text(route)) = stores.get("launcher.route")
            && matches!(route.as_str(), "search" | "actions")
        {
            // Only the page/route is durable. Reconstruct its transient mode
            // on first projection, without retaining or restoring a query.
            mode = route.clone();
            restored_route = true;
        }
        let request = (
            text("launcher.search.text"),
            if mode.is_empty() { "apps".into() } else { mode },
            text("launcher.engine_route"),
        );
        let restore_mode = restored_route.then(|| request.1.clone());
        let mut selection = text("launcher.selection");
        let choose_first =
            !request.0.trim().is_empty() || request.1 != "apps" || !request.2.is_empty();
        let stamp = self.source.as_ref().map(|s| {
            let snap = s.snapshot();
            (snap.epoch.clone(), snap.revision, snap.connected)
        });
        let mut view = self.view.borrow_mut();
        let prior_result_request = view.result_request.clone();
        let mut reply_applied = false;
        if let Some(actor) = &self.query_actor
            && let Ok(mut slot) = actor.completion.lock()
            && let Some(completion) = slot.take()
            && completion.task.generation == view.query_generation
            && completion.task.request == request
            && completion.task.stamp == stamp
        {
            reply_applied = true;
            view.request = Some(request.clone());
            view.stamp = stamp.clone();
            view.pending = None;
            view.pending_limited = false;
            match completion.result {
                Ok(prepared) => {
                    if bead::core::profile::enabled() {
                        println!(
                            "MENU_RESULT ok=true catalog_rev={} count={}",
                            prepared.catalog_rev,
                            prepared.rows.len()
                        );
                    }
                    if std::env::var_os("BEAD_PROFILE_MENU_IDENTITY").as_deref()
                        == Some(std::ffi::OsStr::new("1"))
                    {
                        let identity = json!({"query":request.0,"order":prepared.rows.iter().filter_map(|row|row["id"].as_str()).collect::<Vec<_>>()});
                        println!("MENU_RESULT_IDENTITY json={identity}");
                    }
                    view.total = prepared.total;
                    let favorites = self
                        .query_actor
                        .as_ref()
                        .and_then(|actor| actor.favorites.lock().ok().map(|ids| ids.clone()))
                        .unwrap_or_default();
                    let mut rows = prepared.rows;
                    for row in &mut rows {
                        let id = favorite_id(row);
                        row["favorite"] = Json::Bool(favorites.contains(&id));
                        row["favoriteId"] = Json::String(id);
                    }
                    view.rows = Arc::new(rows);
                    view.actions = prepared.actions;
                    view.catalog_rev = prepared.catalog_rev;
                    view.conversion = prepared.conversion;
                    view.message = prepared.message;
                    view.error.clear();
                    view.result_request = Some(request.clone());
                    view.result_stamp = completion.task.stamp.clone();
                }
                Err(error) => {
                    if bead::core::profile::enabled() {
                        println!(
                            "MENU_RESULT ok=false code={} retry={}",
                            error.code, error.retry
                        );
                    }
                    view.error = error.message;
                }
            }
        }
        let is_grid = request.0.trim().is_empty() && request.1 == "apps" && request.2.is_empty();
        if !reply_applied
            && is_grid
            && view.request.as_ref() != Some(&request)
            && let Some(grid) = view.grid.as_ref().filter(|grid| grid.stamp == stamp)
        {
            reply_applied = true;
            let (rows, total, actions, rev, conversion, message) = (
                grid.rows.clone(),
                grid.total,
                grid.actions.clone(),
                grid.catalog_rev,
                grid.conversion.clone(),
                grid.message.clone(),
            );
            view.rows = rows;
            view.total = total;
            view.actions = actions;
            view.catalog_rev = rev;
            view.conversion = conversion;
            view.message = message;
            view.error.clear();
            view.pending = None;
            view.pending_limited = false;
            view.request = Some(request.clone());
            view.stamp = stamp.clone();
            view.result_request = Some(request.clone());
            view.result_stamp = stamp.clone();
        }
        if let Some(actor) = &self.query_actor
            && let Ok(favorites) = actor.favorites.lock()
            && view.result_request.as_ref() == Some(&request)
            && view
                .rows
                .iter()
                .any(|row| row["favorite"].as_bool() != Some(favorites.contains(&favorite_id(row))))
        {
            for row in Arc::make_mut(&mut view.rows) {
                let id = favorite_id(row);
                row["favorite"] = Json::Bool(favorites.contains(&id));
                row["favoriteId"] = Json::String(id);
            }
        }
        let query_changed = prior_result_request
            .as_ref()
            .is_some_and(|previous| previous.0 != request.0);
        let mut scroll = match stores.get("launcher.scroll.y") {
            Some(Value::Number(n)) => *n,
            _ => 0.,
        };
        let deferred_navigation = if reply_applied && view.error.is_empty() {
            let grid = request.0.trim().is_empty() && request.1 == "apps";
            let pending = std::mem::take(&mut view.pending_navigation);
            if pending.is_empty() {
                false
            } else {
                for (key, geometry) in pending {
                    let (next_selection, next_scroll) =
                        navigation_target(&view.rows, &selection, scroll, &key, geometry, grid);
                    selection = next_selection;
                    scroll = next_scroll;
                }
                true
            }
        } else {
            if reply_applied {
                view.pending_navigation.clear();
            }
            false
        };
        let expand = view.force_full
            || (!query_changed
                && view.error.is_empty()
                && view.result_request.as_ref() == Some(&request)
                && view.total > view.rows.len()
                && scroll > 0.
                && view.pending.is_none());
        view.force_full = false;
        let pending_matches = view.pending.as_ref() == Some(&(request.clone(), stamp.clone()))
            && !(expand && view.pending_limited);
        let needs_query = !pending_matches
            && (view.request.as_ref() != Some(&request) || view.stamp != stamp || expand);
        if needs_query {
            if view
                .request
                .as_ref()
                .is_some_and(|previous| previous != &request)
            {
                view.pending_navigation.clear();
            }
            // Only edited queries use a ranked prefix. Initial/restored state,
            // catalog changes, navigation and scrolling still load the full reply.
            // Missing optional store means an older recovery bundle: stay compatible.
            let limited = query_changed
                && !request.0.trim().is_empty()
                && view.stamp == stamp
                && !expand
                && view
                    .result_request
                    .as_ref()
                    .is_some_and(|old| old.1 == request.1 && old.2 == request.2)
                && matches!(stores.get("launcher.total_count"), Some(Value::Number(_)));
            if self.source.is_some() {
                let _query = bead::core::profile::Span::new("menu_query_schedule");
                let mut params = json!({"query":request.0,"mode":request.1,
                    "route":if request.2.is_empty(){Json::Null}else{json!(request.2)}});
                if limited {
                    params["limit"] = json!(64);
                }
                if let Some(actor) = &self.query_actor {
                    view.query_generation = view.query_generation.wrapping_add(1);
                    let task = QueryTask {
                        request: request.clone(),
                        stamp: stamp.clone(),
                        generation: view.query_generation,
                        params,
                    };
                    actor.schedule(task.clone());
                    view.pending = Some((request.clone(), stamp.clone()));
                    view.pending_limited = limited;
                }
                let same_boot = match (&view.result_stamp, &stamp) {
                    (Some(previous), Some(current)) => previous.0 == current.0,
                    (None, None) => true,
                    _ => false,
                };
                let same_scope = same_boot
                    && view
                        .result_request
                        .as_ref()
                        .is_some_and(|old| old.1 == request.1 && old.2 == request.2);
                if !same_scope {
                    view.rows = Arc::new(Vec::new());
                    view.actions.clear();
                    view.total = 0;
                    view.error.clear();
                    view.conversion = None;
                    view.message.clear();
                    view.result_request = None;
                    view.result_stamp = None;
                }
            } else {
                view.error = tr("bead.launcher.service-unavailable", &[]);
            }
            view.request = Some(request.clone());
            view.stamp = stamp;
        }
        let reset_selection = reply_applied
            && prior_result_request
                .as_ref()
                .is_some_and(|previous| previous != &request)
            && view.error.is_empty();
        let pending_scope_matches = prior_result_request
            .as_ref()
            .is_none_or(|previous| previous.1 == request.1 && previous.2 == request.2);
        let keep_selection = (view.pending.is_some() && pending_scope_matches)
            || !view.error.is_empty()
            || (!reset_selection
                && view.rows.iter().any(|r| {
                    r["id"].as_str() == Some(selection.as_str()) && r["disabled"] != true
                }));
        let reset_scroll = view.error.is_empty()
            && (reset_selection || (!keep_selection && (choose_first || !selection.is_empty())));
        let selection = if keep_selection {
            selection
        } else if choose_first {
            view.rows
                .iter()
                .find(|row| row["disabled"] != true)
                .and_then(|r| r["id"].as_str())
                .unwrap_or("")
                .to_string()
        } else {
            String::new()
        };
        if view.rows.iter().any(|row| {
            row["selected"].as_bool() != Some(row["id"].as_str() == Some(selection.as_str()))
        }) {
            for row in Arc::make_mut(&mut view.rows) {
                row["selected"] = Json::Bool(row["id"].as_str() == Some(selection.as_str()));
            }
        }
        let mut delta = BTreeMap::from([
            (
                "launcher.pending".into(),
                Value::Bool(view.pending.is_some()),
            ),
            (
                "launcher.result_count".into(),
                Value::Number(view.rows.len() as f64),
            ),
            ("launcher.selection".into(), Value::Text(selection)),
            (
                "launcher.error".into(),
                Value::Text(if view.error.is_empty() {
                    self.query_actor
                        .as_ref()
                        .and_then(|actor| {
                            actor
                                .preferences_error
                                .lock()
                                .ok()
                                .and_then(|error| error.clone())
                        })
                        .unwrap_or_default()
                } else {
                    view.error.clone()
                }),
            ),
        ]);
        if stores.get("launcher.result_query").is_some() {
            delta.insert(
                "launcher.result_query".into(),
                Value::Text(
                    view.result_request
                        .as_ref()
                        .map(|request| request.0.clone())
                        .unwrap_or_default(),
                ),
            );
        }
        if let Some(mode) = restore_mode {
            delta.insert("launcher.mode".into(), Value::Text(mode));
        }
        if let Some(current) = stores.get("launcher.groups")
            && let Some(actor) = &self.query_actor
            && let Ok(groups) = actor.groups.lock()
        {
            let rows = Value::list(groups.clone());
            if *current != rows && !groups.is_empty() {
                delta.insert("launcher.groups".into(), rows);
            }
        }
        if stores.get("launcher.total_count").is_some() {
            delta.insert(
                "launcher.total_count".into(),
                Value::Number(view.total as f64),
            );
        }
        let conversion = view.conversion.as_ref();
        for (key, value) in [
            (
                "launcher.conversion.value",
                conversion.and_then(|v| v.value.clone()).unwrap_or_default(),
            ),
            (
                "launcher.conversion.description",
                conversion
                    .and_then(|v| v.description.clone())
                    .unwrap_or_default(),
            ),
            (
                "launcher.conversion.error",
                conversion.and_then(|v| v.error.clone()).unwrap_or_default(),
            ),
            ("launcher.message", view.message.clone()),
        ] {
            if stores.get(key) != Some(&Value::Text(value.clone())) {
                delta.insert(key.into(), Value::Text(value));
            }
        }
        if deferred_navigation {
            view.last_scroll = scroll;
            delta.insert("launcher.scroll.y".into(), Value::Number(scroll));
        } else if !view.error.is_empty() && scroll != view.last_scroll {
            // Failed expansion keeps the last good viewport, never an unloaded region.
            delta.insert("launcher.scroll.y".into(), Value::Number(view.last_scroll));
        } else if reset_scroll {
            view.last_scroll = 0.;
            delta.insert("launcher.scroll.y".into(), Value::Number(0.));
        } else if view.pending.is_some()
            && view.total > view.rows.len()
            && scroll > view.last_scroll
        {
            // Keep the last loaded viewport while an expansion is in flight.
        } else {
            view.last_scroll = scroll;
        }
        if stores.get("launcher.icon_placeholder") == Some(&Value::Bool(true))
            && view.pending.is_none()
        {
            delta.insert("launcher.icon_placeholder".into(), Value::Bool(false));
        }
        // A query may change while the ordered engine result stays identical.
        // Retain the store's immutable model instead of cloning it just to compare.
        match stores.get("launcher.apps") {
            Some(Value::List(current))
                if Arc::ptr_eq(current, &view.rows) || current.as_ref() == view.rows.as_ref() =>
            {
                view.rows = current.clone();
            }
            _ => {
                delta.insert("launcher.apps".into(), Value::List(view.rows.clone()));
            }
        }
        if is_grid
            && view.error.is_empty()
            && view.pending.is_none()
            && view.result_request.as_ref() == Some(&request)
            && !view.grid.as_ref().is_some_and(|grid| {
                grid.stamp == view.result_stamp && Arc::ptr_eq(&grid.rows, &view.rows)
            })
            && view
                .rows
                .iter()
                .all(|row| row["selected"].as_bool() == Some(false))
        {
            view.grid = Some(GridResult {
                stamp: view.result_stamp.clone(),
                rows: view.rows.clone(),
                total: view.total,
                actions: view.actions.clone(),
                catalog_rev: view.catalog_rev,
                conversion: view.conversion.clone(),
                message: view.message.clone(),
            });
        }
        Ok(delta)
    }

    /// Start the initial query while markup loads. Keep first-projection route
    /// restoration enabled; a different restored route supersedes this request.
    pub fn prime_initial(&self, stores: &Stores) -> Result<(), bead::ui::Diagnostic> {
        let request = self.view.borrow().request.clone();
        let result = self.project(stores);
        self.view.borrow_mut().request = request;
        result.map(|_| ())
    }

    /// Let the first launcher request settle briefly before the panel commits its first scene.
    /// Later query projections never wait here, so typing remains independent.
    pub fn project_initial(
        &self,
        stores: &Stores,
        timeout: Duration,
    ) -> Result<BTreeMap<String, Value>, bead::ui::Diagnostic> {
        let deadline = std::time::Instant::now() + timeout.min(Duration::from_millis(20));
        loop {
            let delta = self.project(stores)?;
            if delta.get("launcher.pending") != Some(&Value::Bool(true)) {
                return Ok(delta);
            }
            if std::time::Instant::now() >= deadline {
                let mut delta = delta;
                delta.insert("launcher.icon_placeholder".into(), Value::Bool(true));
                bead::core::vprintln!(
                    "LAUNCHER_FIRST_FRAME readiness=timeout_ms20 placeholder=true"
                );
                return Ok(delta);
            }
            std::thread::sleep(Duration::from_millis(1));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct PrefixSource {
        fixture: Fixture,
        calls: std::sync::Mutex<Vec<Json>>,
        fail_full: std::sync::atomic::AtomicBool,
        omit_total: bool,
    }
    impl Source for PrefixSource {
        fn snapshot(&self) -> Arc<source::Snapshot> {
            self.fixture.snapshot()
        }
        fn subscribe(&self, callback: source::Callback) -> source::Result<source::Subscription> {
            self.fixture.subscribe(callback)
        }
        fn invoke(&self, method: String, params: Json) -> source::Reply<'_> {
            Box::pin(async move {
                if method == "menu.query" {
                    self.calls.lock().unwrap().push(params.clone());
                }
                if params.get("limit").is_none()
                    && self.fail_full.load(std::sync::atomic::Ordering::SeqCst)
                {
                    return Err(source::Error::new(
                        "unavailable",
                        "fixture expansion unavailable",
                        true,
                    ));
                }
                let mut reply = self.fixture.invoke(method, params).await?;
                if self.omit_total {
                    reply.as_object_mut().unwrap().remove("total");
                }
                Ok(reply)
            })
        }
        fn reconnect(&self) -> source::Reply<'_> {
            self.fixture.reconnect()
        }
        fn stop(&self) -> source::Reply<'_> {
            self.fixture.stop()
        }
    }
    fn prefix_source(disabled_prefix: bool, omit_total: bool) -> Arc<PrefixSource> {
        let rows = (0..320).map(|i| json!({"id":format!("app{i}"),"name":format!("App {i}"),"disabled":disabled_prefix && i < 64})).collect::<Vec<_>>();
        Arc::new(PrefixSource {
            fixture: Fixture::new(json!({"queries":{
                "":fixture_reply("",rows.clone()), "a":fixture_reply("a",rows.clone()), "ap":fixture_reply("ap",rows)
            }})).unwrap(),
            calls: Default::default(), fail_full: false.into(), omit_total,
        })
    }
    fn prefix_stores() -> Stores {
        Stores::default()
            .reconcile(&BTreeMap::from([
                ("launcher.total_count".into(), Value::Number(0.)),
                ("launcher.pending".into(), Value::Bool(false)),
                ("launcher.search.text".into(), Value::Text(String::new())),
                ("launcher.selection".into(), Value::Text(String::new())),
                ("launcher.engine_route".into(), Value::Text(String::new())),
                ("launcher.scroll.y".into(), Value::Number(0.)),
            ]))
            .unwrap()
    }
    fn project_wait(
        menu: &Menu,
        stores: &Stores,
    ) -> Result<BTreeMap<String, Value>, bead::ui::Diagnostic> {
        let mut current = stores.clone();
        let mut aggregate = BTreeMap::new();
        for _ in 0..400 {
            let delta = menu.project(&current)?;
            let pending = delta.get("launcher.pending") == Some(&Value::Bool(true));
            aggregate.extend(delta.clone());
            current = current
                .reconcile(&delta)
                .expect("test projection reconciliation");
            current
                .commit(delta.clone())
                .expect("test projection delta");
            if !pending {
                return Ok(aggregate);
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        panic!("launcher query did not complete")
    }
    fn apply_projection(menu: &Menu, stores: &mut Stores) {
        let delta = project_wait(menu, stores).unwrap();
        *stores = stores.reconcile(&delta).unwrap();
        stores.commit(delta).unwrap();
    }
    #[test]
    fn initial_priming_reuses_reply_and_preserves_route_restoration() {
        let source = prefix_source(false, false);
        let menu = Menu::new(Some(source.clone())).unwrap();
        let mut stores = prefix_stores();
        menu.prime_initial(&stores).unwrap();
        assert!(menu.view.borrow().request.is_none());
        apply_projection(&menu, &mut stores);
        assert_eq!(source.calls.lock().unwrap().len(), 1);

        let menu = Menu::new(Some(prefix_source(false, false))).unwrap();
        menu.prime_initial(&prefix_stores()).unwrap();
        let restored = BTreeMap::from([
            ("launcher.mode".into(), Value::Text("apps".into())),
            ("launcher.route".into(), Value::Text("actions".into())),
        ]);
        stores = stores.reconcile(&restored).unwrap();
        stores.commit(restored).unwrap();
        let delta = menu.project(&stores).unwrap();
        assert_eq!(
            delta.get("launcher.mode"),
            Some(&Value::Text("actions".into()))
        );
        assert_eq!(menu.view.borrow().pending.as_ref().unwrap().0.1, "actions");
    }
    #[test]
    fn initial_projection_is_bounded_and_later_typing_does_not_wait() {
        let source = prefix_source(false, false);
        let menu = Menu::new(Some(source)).unwrap();
        let mut stores = prefix_stores();
        let started = std::time::Instant::now();
        let first = menu
            .project_initial(&stores, Duration::from_millis(20))
            .unwrap();
        assert!(started.elapsed() < Duration::from_millis(100));
        if first.get("launcher.pending") == Some(&Value::Bool(true)) {
            assert_eq!(
                first.get("launcher.icon_placeholder"),
                Some(&Value::Bool(true))
            );
        } else {
            assert!(first.contains_key("launcher.apps"));
        }
        stores = stores.reconcile(&first).unwrap();
        stores.commit(first).unwrap();
        stores
            .commit(BTreeMap::from([(
                "launcher.search.text".into(),
                Value::Text("typing".into()),
            )]))
            .unwrap();
        let started = std::time::Instant::now();
        let _ = menu.project(&stores).unwrap();
        assert!(started.elapsed() < Duration::from_millis(30));
    }
    fn edit_prefix(menu: &Menu, stores: &mut Stores) {
        apply_projection(menu, stores);
        stores
            .commit(BTreeMap::from([(
                "launcher.search.text".into(),
                Value::Text("a".into()),
            )]))
            .unwrap();
        apply_projection(menu, stores);
    }
    #[test]
    fn latest_same_scope_query_replaces_rows_without_an_empty_frame() {
        struct SlowSource {
            fixture: Fixture,
            calls: std::sync::Mutex<Vec<String>>,
        }
        impl Source for SlowSource {
            fn snapshot(&self) -> Arc<source::Snapshot> {
                self.fixture.snapshot()
            }
            fn subscribe(
                &self,
                callback: source::Callback,
            ) -> source::Result<source::Subscription> {
                self.fixture.subscribe(callback)
            }
            fn invoke(&self, method: String, params: Json) -> source::Reply<'_> {
                let fixture = &self.fixture;
                Box::pin(async move {
                    let query = params
                        .get("query")
                        .and_then(Json::as_str)
                        .unwrap_or_default()
                        .to_owned();
                    if method == "menu.query" {
                        if let Ok(mut calls) = self.calls.lock() {
                            calls.push(query.clone());
                        }
                        if query == "slow" {
                            tokio::time::sleep(Duration::from_millis(120)).await;
                        }
                    }
                    fixture.invoke(method, params).await
                })
            }
            fn reconnect(&self) -> source::Reply<'_> {
                self.fixture.reconnect()
            }
            fn stop(&self) -> source::Reply<'_> {
                self.fixture.stop()
            }
        }
        let fixture = Fixture::new(json!({"queries":{
            "":fixture_reply("",vec![json!({"id":"old","name":"Old"})]),
            "slow":fixture_reply("slow",vec![json!({"id":"stale","name":"Stale"})]),
            "latest":fixture_reply("latest",vec![json!({"id":"latest","name":"Latest"})])
        }}))
        .unwrap();
        let source = Arc::new(SlowSource {
            fixture,
            calls: Default::default(),
        });
        let menu = Menu::new(Some(source.clone())).unwrap();
        let mut stores = Stores::default()
            .reconcile(&BTreeMap::from([
                ("launcher.search.text".into(), Value::Text(String::new())),
                ("launcher.result_query".into(), Value::Text(String::new())),
                ("launcher.selection".into(), Value::Text(String::new())),
                ("launcher.engine_route".into(), Value::Text(String::new())),
                ("launcher.scroll.y".into(), Value::Number(0.)),
            ]))
            .unwrap();
        let initial = project_wait(&menu, &stores).unwrap();
        stores = stores.reconcile(&initial).unwrap();
        stores.commit(initial).unwrap();
        stores
            .commit(BTreeMap::from([(
                "launcher.search.text".into(),
                Value::Text("slow".into()),
            )]))
            .unwrap();
        let pending = menu.project(&stores).unwrap();
        assert_eq!(pending["launcher.pending"], Value::Bool(true));
        assert_eq!(pending["launcher.result_query"], Value::Text(String::new()));
        assert_eq!(pending["launcher.result_count"], Value::Number(1.));
        assert_eq!(menu.view.borrow().rows[0]["id"], "old");
        let deadline = std::time::Instant::now() + Duration::from_secs(1);
        while !source
            .calls
            .lock()
            .unwrap()
            .iter()
            .any(|query| query == "slow")
            && std::time::Instant::now() < deadline
        {
            std::thread::sleep(Duration::from_millis(2));
        }
        stores
            .commit(BTreeMap::from([(
                "launcher.search.text".into(),
                Value::Text("latest".into()),
            )]))
            .unwrap();
        let latest = project_wait(&menu, &stores).unwrap();
        assert_eq!(
            latest["launcher.result_query"],
            Value::Text("latest".into())
        );
        stores = stores.reconcile(&latest).unwrap();
        stores.commit(latest).unwrap();
        assert_eq!(menu.view.borrow().rows[0]["id"], "latest");
        std::thread::sleep(Duration::from_millis(150));
        let after_stale_delay = project_wait(&menu, &stores).unwrap();
        assert_eq!(menu.view.borrow().rows[0]["id"], "latest");
        assert_eq!(after_stale_delay["launcher.pending"], Value::Bool(false));
        assert_eq!(
            after_stale_delay["launcher.result_query"],
            Value::Text("latest".into())
        );
        stores
            .commit(BTreeMap::from([(
                "launcher.engine_route".into(),
                Value::Text("home".into()),
            )]))
            .unwrap();
        let switched_scope = menu.project(&stores).unwrap();
        assert_eq!(switched_scope["launcher.pending"], Value::Bool(true));
        assert_eq!(switched_scope["launcher.result_count"], Value::Number(0.));
        assert!(menu.view.borrow().rows.is_empty());
    }
    #[test]
    fn engine_groups_reach_the_rail_store_and_an_older_daemon_keeps_the_fallback() {
        let reply = |groups: Option<Json>| {
            let mut data = json!({"queries":{"":fixture_reply("",vec![
                json!({"id":"org.example.app.desktop","name":"Application","kind":"app"}),
            ])}});
            if let Some(groups) = groups {
                data["groups"] = groups;
            }
            Fixture::new(data).unwrap()
        };
        let stores = Stores::default()
            .reconcile(&BTreeMap::from([
                ("launcher.search.text".into(), Value::Text(String::new())),
                ("launcher.selection".into(), Value::Text(String::new())),
                ("launcher.groups".into(), Value::list(vec![])),
            ]))
            .unwrap();
        let project_for = |source: Fixture| {
            let menu = Menu::new(Some(Arc::new(source))).unwrap();
            let deadline = std::time::Instant::now() + Duration::from_secs(1);
            let mut seen = None;
            while seen.is_none() && std::time::Instant::now() < deadline {
                seen = project_wait(&menu, &stores)
                    .unwrap()
                    .remove("launcher.groups");
                std::thread::sleep(Duration::from_millis(5));
            }
            seen
        };
        let rows = json!([
            {"id":"home","label":"Home","icon":"home"},
            {"id":"style","label":"Style","icon":"palette"}
        ]);
        assert_eq!(
            project_for(reply(Some(rows))),
            Some(Value::list(vec![
                json!({"id":"home","name":"Home","icon":"home"}),
                json!({"id":"style","name":"Style","icon":"palette"}),
            ]))
        );
        // No `menu.groups` answer: the store stays empty and the Luau fallback rail applies.
        assert_eq!(project_for(reply(None)), None);
        assert_eq!(
            parse_groups(json!({"catalog_rev":1,"groups":[{"id":"","label":"x","icon":""}]})),
            None
        );
    }
    #[test]
    fn returning_to_the_grid_reuses_the_settled_result_in_one_projection() {
        let source = prefix_source(false, false);
        let menu = Menu::new(Some(source.clone())).unwrap();
        let mut stores = prefix_stores();
        edit_prefix(&menu, &mut stores);
        assert_eq!(
            stores.get("launcher.result_count"),
            Some(&Value::Number(64.))
        );
        let before = source.calls.lock().unwrap().len();
        stores
            .commit(BTreeMap::from([(
                "launcher.search.text".into(),
                Value::Text(String::new()),
            )]))
            .unwrap();
        let delta = menu.project(&stores).unwrap();
        assert_eq!(delta["launcher.pending"], Value::Bool(false));
        assert_eq!(delta["launcher.result_count"], Value::Number(320.));
        assert_eq!(delta["launcher.selection"], Value::Text(String::new()));
        assert_eq!(source.calls.lock().unwrap().len(), before, "no new query");
    }
    #[test]
    fn favorites_use_canonical_result_ids_and_published_preferences() {
        let fixture = Fixture::new(json!({"queries":{"":fixture_reply("",vec![
            json!({"id":"org.example.app.desktop","name":"Application","kind":"app"}),
            json!({"id":"nacre-menu:apps","name":"Applications","kind":"group"}),
        ])}}))
        .unwrap();
        let source = Arc::new(fixture);
        let menu = Menu::new(Some(source.clone())).unwrap();
        let stores = Stores::default()
            .reconcile(&BTreeMap::from([
                ("launcher.search.text".into(), Value::Text(String::new())),
                ("launcher.selection".into(), Value::Text(String::new())),
            ]))
            .unwrap();
        project_wait(&menu, &stores).unwrap();
        let deadline = std::time::Instant::now() + Duration::from_secs(1);
        while !menu
            .query_actor
            .as_ref()
            .unwrap()
            .preferences_loaded
            .load(std::sync::atomic::Ordering::Acquire)
            && std::time::Instant::now() < deadline
        {
            std::thread::sleep(Duration::from_millis(2));
        }
        menu.favorite("nacre-menu:apps").unwrap();
        let deadline = std::time::Instant::now() + Duration::from_secs(1);
        while menu
            .query_actor
            .as_ref()
            .unwrap()
            .favorite_in_flight
            .load(std::sync::atomic::Ordering::Acquire)
            && std::time::Instant::now() < deadline
        {
            std::thread::sleep(Duration::from_millis(2));
        }
        assert!(source.favorites.lock().unwrap().contains("nacre-menu:apps"));
        let delta = project_wait(&menu, &stores).unwrap();
        assert!(
            matches!(&delta["launcher.apps"], Value::List(rows) if rows.iter().any(|row| row["id"] == "nacre-menu:apps" && row["favorite"] == true))
        );
        menu.favorite("nacre-menu:apps").unwrap();
        let deadline = std::time::Instant::now() + Duration::from_secs(1);
        while menu
            .query_actor
            .as_ref()
            .unwrap()
            .favorite_in_flight
            .load(std::sync::atomic::Ordering::Acquire)
            && std::time::Instant::now() < deadline
        {
            std::thread::sleep(Duration::from_millis(2));
        }
        assert!(!source.favorites.lock().unwrap().contains("nacre-menu:apps"));
        let unfavorited = project_wait(&menu, &stores).unwrap();
        assert!(
            matches!(&unfavorited["launcher.apps"], Value::List(rows) if rows.iter().any(|row| row["id"] == "nacre-menu:apps" && row["favorite"] == false))
        );
    }
    #[test]
    fn menu_stamp_change_reloads_authoritative_favorites() {
        let source = Arc::new(
            Fixture::new(json!({"queries":{"":fixture_reply("",vec![
                json!({"id":"app","name":"Application","kind":"app"})
            ])}}))
            .unwrap(),
        );
        let menu = Menu::new(Some(source.clone())).unwrap();
        let stores = Stores::default()
            .reconcile(&BTreeMap::from([
                ("launcher.search.text".into(), Value::Text(String::new())),
                ("launcher.selection".into(), Value::Text(String::new())),
            ]))
            .unwrap();
        project_wait(&menu, &stores).unwrap();
        source.favorites.lock().unwrap().insert("app".into());
        let old_revision = source.snapshot().revision;
        let mut data = source.snapshot().data.as_ref().clone();
        data["preferences_stamp"] = json!(2);
        source.publisher.publish(data).unwrap();
        assert_ne!(source.snapshot().revision, old_revision);
        let deadline = std::time::Instant::now() + Duration::from_secs(1);
        loop {
            project_wait(&menu, &stores).unwrap();
            if menu
                .query_actor
                .as_ref()
                .unwrap()
                .favorites
                .lock()
                .unwrap()
                .contains("app")
            {
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "preference refresh did not finish"
            );
            std::thread::sleep(Duration::from_millis(2));
        }
        let refreshed = project_wait(&menu, &stores).unwrap();
        assert!(
            matches!(&refreshed["launcher.apps"], Value::List(rows) if rows.iter().any(|row| row["id"] == "app" && row["favorite"] == true))
        );
    }
    #[test]
    fn slow_favorite_write_does_not_block_query_projection() {
        struct SlowFavorite {
            fixture: Fixture,
            started: std::sync::atomic::AtomicBool,
            completed: std::sync::atomic::AtomicBool,
        }
        impl Source for SlowFavorite {
            fn snapshot(&self) -> Arc<source::Snapshot> {
                self.fixture.snapshot()
            }
            fn subscribe(
                &self,
                callback: source::Callback,
            ) -> source::Result<source::Subscription> {
                self.fixture.subscribe(callback)
            }
            fn invoke(&self, method: String, params: Json) -> source::Reply<'_> {
                Box::pin(async move {
                    let favorite = method == "menu.preferences" && params["action"] == "favorite";
                    if favorite {
                        self.started
                            .store(true, std::sync::atomic::Ordering::Release);
                        tokio::time::sleep(Duration::from_millis(180)).await;
                    }
                    let result = self.fixture.invoke(method.clone(), params).await?;
                    if favorite {
                        self.completed
                            .store(true, std::sync::atomic::Ordering::Release);
                    }
                    Ok(result)
                })
            }
            fn reconnect(&self) -> source::Reply<'_> {
                self.fixture.reconnect()
            }
            fn stop(&self) -> source::Reply<'_> {
                self.fixture.stop()
            }
        }
        let fixture = Fixture::new(json!({"queries":{
            "":fixture_reply("",vec![json!({"id":"app","name":"Application"})]),
            "next":fixture_reply("next",vec![json!({"id":"next","name":"Next"})])
        }}))
        .unwrap();
        let source = Arc::new(SlowFavorite {
            fixture,
            started: false.into(),
            completed: false.into(),
        });
        let menu = Menu::new(Some(source.clone())).unwrap();
        let mut stores = Stores::default()
            .reconcile(&BTreeMap::from([
                ("launcher.search.text".into(), Value::Text(String::new())),
                ("launcher.selection".into(), Value::Text(String::new())),
            ]))
            .unwrap();
        let initial = project_wait(&menu, &stores).unwrap();
        stores = stores.reconcile(&initial).unwrap();
        stores.commit(initial).unwrap();
        let deadline = std::time::Instant::now() + Duration::from_secs(1);
        while !menu
            .query_actor
            .as_ref()
            .unwrap()
            .preferences_loaded
            .load(std::sync::atomic::Ordering::Acquire)
            && std::time::Instant::now() < deadline
        {
            std::thread::sleep(Duration::from_millis(2));
        }
        let started = std::time::Instant::now();
        menu.favorite("app").unwrap();
        assert!(
            started.elapsed() < Duration::from_millis(100),
            "favorite scheduling blocked the UI worker"
        );
        while !source.started.load(std::sync::atomic::Ordering::Acquire)
            && std::time::Instant::now() < deadline
        {
            std::thread::sleep(Duration::from_millis(2));
        }
        stores
            .commit(BTreeMap::from([(
                "launcher.search.text".into(),
                Value::Text("next".into()),
            )]))
            .unwrap();
        let next = project_wait(&menu, &stores).unwrap();
        stores = stores.reconcile(&next).unwrap();
        stores.commit(next).unwrap();
        assert_eq!(menu.view.borrow().rows[0]["id"], "next");
        assert!(!source.completed.load(std::sync::atomic::Ordering::Acquire));
        let deadline = std::time::Instant::now() + Duration::from_secs(1);
        while !source.completed.load(std::sync::atomic::Ordering::Acquire)
            && std::time::Instant::now() < deadline
        {
            std::thread::sleep(Duration::from_millis(2));
        }
        assert!(source.completed.load(std::sync::atomic::Ordering::Acquire));
        assert!(source.fixture.favorites.lock().unwrap().contains("app"));
    }
    #[test]
    fn ranked_prefix_preserves_full_extent_and_up_navigation_reaches_last_row() {
        let source = prefix_source(false, false);
        let menu = Menu::new(Some(source.clone())).unwrap();
        let mut stores = prefix_stores();
        edit_prefix(&menu, &mut stores);
        assert_eq!(
            stores.get("launcher.result_count"),
            Some(&Value::Number(64.))
        );
        assert_eq!(
            stores.get("launcher.total_count"),
            Some(&Value::Number(320.))
        );
        assert_eq!(source.calls.lock().unwrap()[1]["limit"], 64);
        let geometry = NavigationGeometry {
            columns: 4,
            pitch: 58.,
            row_height: 55.,
            viewport_height: 400.,
        };
        assert!(
            menu.navigate(&stores, "left", geometry).is_none(),
            "caret motion must not expand results"
        );
        assert_eq!(source.calls.lock().unwrap().len(), 2);
        let down = menu.navigate(&stores, "down", geometry).unwrap();
        stores.commit(down).unwrap();
        apply_projection(&menu, &mut stores);
        assert_eq!(
            stores.get("launcher.selection"),
            Some(&Value::Text("app1".into()))
        );
        assert_eq!(
            source.calls.lock().unwrap().len(),
            2,
            "visible navigation stays local"
        );
        let up = menu.navigate(&stores, "up", geometry).unwrap();
        stores.commit(up).unwrap();
        apply_projection(&menu, &mut stores);
        assert_eq!(source.calls.lock().unwrap().len(), 2);
        let pending = menu.navigate(&stores, "up", geometry).unwrap();
        stores.commit(pending).unwrap();
        apply_projection(&menu, &mut stores);
        assert_eq!(
            stores.get("launcher.result_count"),
            Some(&Value::Number(320.))
        );
        assert_eq!(
            stores.get("launcher.selection"),
            Some(&Value::Text("app319".into()))
        );
        assert!(source.calls.lock().unwrap()[2].get("limit").is_none());
        assert_eq!(menu.view.borrow().rows[319]["id"], "app319");
    }
    #[test]
    fn disabled_prefix_tail_expands_before_wrapping_to_a_visible_row() {
        let source = prefix_source(false, false);
        let mut data = source.fixture.snapshot().data.as_ref().clone();
        for i in 1..64 {
            data["queries"]["a"]["items"][format!("app{i}")]["disabled"] = Json::Bool(true);
        }
        source.fixture.publisher.publish(data).unwrap();
        let menu = Menu::new(Some(source.clone())).unwrap();
        let mut stores = prefix_stores();
        edit_prefix(&menu, &mut stores);
        assert_eq!(
            stores.get("launcher.result_count"),
            Some(&Value::Number(64.))
        );
        let geometry = NavigationGeometry {
            columns: 4,
            pitch: 58.,
            row_height: 55.,
            viewport_height: 400.,
        };
        let pending = menu.navigate(&stores, "down", geometry).unwrap();
        stores.commit(pending).unwrap();
        apply_projection(&menu, &mut stores);
        assert_eq!(
            stores.get("launcher.selection"),
            Some(&Value::Text("app64".into()))
        );
        assert_eq!(
            stores.get("launcher.result_count"),
            Some(&Value::Number(320.))
        );
        assert_eq!(source.calls.lock().unwrap().len(), 3);
    }
    #[test]
    fn repeated_boundary_navigation_replays_after_one_delayed_full_fetch() {
        struct DelayedExpansion {
            fixture: Fixture,
            full_calls: std::sync::atomic::AtomicUsize,
        }
        impl Source for DelayedExpansion {
            fn snapshot(&self) -> Arc<source::Snapshot> {
                self.fixture.snapshot()
            }
            fn subscribe(
                &self,
                callback: source::Callback,
            ) -> source::Result<source::Subscription> {
                self.fixture.subscribe(callback)
            }
            fn invoke(&self, method: String, params: Json) -> source::Reply<'_> {
                Box::pin(async move {
                    if method == "menu.query"
                        && params["query"] == "a"
                        && params.get("limit").is_none()
                    {
                        self.full_calls
                            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                        tokio::time::sleep(Duration::from_millis(180)).await;
                    }
                    self.fixture.invoke(method, params).await
                })
            }
            fn reconnect(&self) -> source::Reply<'_> {
                self.fixture.reconnect()
            }
            fn stop(&self) -> source::Reply<'_> {
                self.fixture.stop()
            }
        }
        let rows = (0..320)
            .map(|index| json!({"id":format!("app{index}"),"name":format!("App {index}")}))
            .collect::<Vec<_>>();
        let source = Arc::new(DelayedExpansion {
            fixture: Fixture::new(
                json!({"queries":{"":fixture_reply("",rows.clone()),"a":fixture_reply("a",rows)}}),
            )
            .unwrap(),
            full_calls: Default::default(),
        });
        let menu = Menu::new(Some(source.clone())).unwrap();
        let mut stores = prefix_stores();
        edit_prefix(&menu, &mut stores);
        let geometry = NavigationGeometry {
            columns: 4,
            pitch: 58.,
            row_height: 55.,
            viewport_height: 400.,
        };
        let first = menu.navigate(&stores, "up", geometry).unwrap();
        assert_eq!(first["launcher.pending"], Value::Bool(true));
        stores.commit(first).unwrap();
        let second = menu.navigate(&stores, "up", geometry).unwrap();
        assert_eq!(second["launcher.pending"], Value::Bool(true));
        stores.commit(second).unwrap();
        assert_eq!(menu.view.borrow().pending_navigation.len(), 2);
        apply_projection(&menu, &mut stores);
        assert_eq!(
            stores.get("launcher.result_count"),
            Some(&Value::Number(320.))
        );
        assert_eq!(
            stores.get("launcher.selection"),
            Some(&Value::Text("app318".into()))
        );
        assert_eq!(
            source.full_calls.load(std::sync::atomic::Ordering::SeqCst),
            1
        );
    }
    #[test]
    fn scrolling_expands_prefix_before_materializing_an_unloaded_region() {
        let source = prefix_source(false, false);
        let menu = Menu::new(Some(source)).unwrap();
        let mut stores = prefix_stores();
        edit_prefix(&menu, &mut stores);
        stores
            .commit(BTreeMap::from([(
                "launcher.scroll.y".into(),
                Value::Number(12000.),
            )]))
            .unwrap();
        let delta = project_wait(&menu, &stores).unwrap();
        assert_eq!(delta["launcher.result_count"], Value::Number(320.));
        assert!(
            !delta.contains_key("launcher.scroll.y"),
            "valid scrolling must not reset"
        );
    }
    #[test]
    fn prefix_with_no_enabled_row_and_missing_total_both_fall_back_to_full_reply() {
        for (disabled, unknown) in [(true, false), (false, true)] {
            let source = prefix_source(disabled, unknown);
            let menu = Menu::new(Some(source.clone())).unwrap();
            let mut stores = prefix_stores();
            edit_prefix(&menu, &mut stores);
            assert_eq!(
                stores.get("launcher.result_count"),
                Some(&Value::Number(320.))
            );
            assert_eq!(
                stores.get("launcher.selection"),
                Some(&Value::Text(if disabled { "app64" } else { "app0" }.into()))
            );
            assert_eq!(source.calls.lock().unwrap().len(), 3);
        }
    }
    #[test]
    fn failed_prefix_expansion_keeps_old_rows_selection_and_last_good_viewport() {
        let source = prefix_source(false, false);
        let menu = Menu::new(Some(source.clone())).unwrap();
        let mut stores = prefix_stores();
        edit_prefix(&menu, &mut stores);
        source
            .fail_full
            .store(true, std::sync::atomic::Ordering::SeqCst);
        stores
            .commit(BTreeMap::from([(
                "launcher.scroll.y".into(),
                Value::Number(12000.),
            )]))
            .unwrap();
        let delta = project_wait(&menu, &stores).unwrap();
        assert_eq!(delta["launcher.scroll.y"], Value::Number(0.));
        assert_eq!(delta["launcher.result_count"], Value::Number(64.));
        assert_eq!(delta["launcher.selection"], Value::Text("app0".into()));
        assert!(matches!(&delta["launcher.error"],Value::Text(message)if !message.is_empty()));
        let geometry = NavigationGeometry {
            columns: 4,
            pitch: 58.,
            row_height: 55.,
            viewport_height: 400.,
        };
        let failed_navigation = menu.navigate(&stores, "up", geometry).unwrap();
        assert_eq!(
            failed_navigation["launcher.selection"],
            Value::Text("app0".into())
        );
    }
    #[test]
    fn initial_restore_old_bundle_and_catalog_wake_keep_the_full_result() {
        let source = prefix_source(false, false);
        let menu = Menu::new(Some(source.clone())).unwrap();
        let mut stores = prefix_stores();
        stores
            .commit(BTreeMap::from([
                ("launcher.search.text".into(), Value::Text("a".into())),
                ("launcher.selection".into(), Value::Text("app319".into())),
                ("launcher.scroll.y".into(), Value::Number(12000.)),
            ]))
            .unwrap();
        apply_projection(&menu, &mut stores);
        assert_eq!(
            stores.get("launcher.selection"),
            Some(&Value::Text("app319".into()))
        );
        assert!(source.calls.lock().unwrap()[0].get("limit").is_none());
        stores
            .commit(BTreeMap::from([(
                "launcher.search.text".into(),
                Value::Text("ap".into()),
            )]))
            .unwrap();
        apply_projection(&menu, &mut stores);
        assert_eq!(
            stores.get("launcher.result_count"),
            Some(&Value::Number(64.))
        );
        let mut fixture_data = source.fixture.snapshot().data.as_ref().clone();
        fixture_data["queries"]["ap"]["catalog_rev"] = json!(2);
        source.fixture.publisher.publish(fixture_data).unwrap();
        apply_projection(&menu, &mut stores);
        assert_eq!(
            stores.get("launcher.result_count"),
            Some(&Value::Number(320.))
        );
        assert!(
            source
                .calls
                .lock()
                .unwrap()
                .last()
                .unwrap()
                .get("limit")
                .is_none()
        );
        let legacy = Menu::new(Some(source.clone())).unwrap();
        let mut old = Stores::default()
            .reconcile(&BTreeMap::from([(
                "launcher.search.text".into(),
                Value::Text(String::new()),
            )]))
            .unwrap();
        apply_projection(&legacy, &mut old);
        old.commit(BTreeMap::from([(
            "launcher.search.text".into(),
            Value::Text("a".into()),
        )]))
        .unwrap();
        apply_projection(&legacy, &mut old);
        assert_eq!(old.get("launcher.result_count"), Some(&Value::Number(320.)));
        assert!(
            source
                .calls
                .lock()
                .unwrap()
                .last()
                .unwrap()
                .get("limit")
                .is_none()
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn query_busy_retry_is_bounded_and_honors_retry_flag_code_and_deadline() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        struct Busy {
            fixture: Fixture,
            attempts: AtomicUsize,
            failures: usize,
            retry: bool,
            code: &'static str,
        }
        impl Source for Busy {
            fn snapshot(&self) -> Arc<source::Snapshot> {
                self.fixture.snapshot()
            }
            fn subscribe(&self, cb: source::Callback) -> source::Result<source::Subscription> {
                self.fixture.subscribe(cb)
            }
            fn invoke(&self, method: String, params: Json) -> source::Reply<'_> {
                Box::pin(async move {
                    if self.attempts.fetch_add(1, Ordering::SeqCst) < self.failures {
                        Err(source::Error::new(
                            self.code,
                            "provider occupied",
                            self.retry,
                        ))
                    } else {
                        self.fixture.invoke(method, params).await
                    }
                })
            }
            fn reconnect(&self) -> source::Reply<'_> {
                self.fixture.reconnect()
            }
            fn stop(&self) -> source::Reply<'_> {
                self.fixture.stop()
            }
        }
        let make = |failures, retry, code| Busy {
            fixture: Fixture::new(
                json!({"queries":{"a":fixture_reply("a",vec![json!({"id":"one","name":"One"})])}}),
            )
            .unwrap(),
            attempts: AtomicUsize::new(0),
            failures,
            retry,
            code,
        };
        let params = json!({"query":"a","mode":"apps"});
        let success = make(2, true, "busy");
        let result = query(&success, params.clone(), Duration::from_secs(1))
            .await
            .unwrap();
        assert_eq!(result["order"], json!(["one"]));
        assert_eq!(success.attempts.load(Ordering::SeqCst), 3);
        let busy = make(usize::MAX, true, "busy");
        assert_eq!(
            query(&busy, params.clone(), Duration::from_millis(250))
                .await
                .unwrap_err()
                .code,
            "busy"
        );
        assert_eq!(busy.attempts.load(Ordering::SeqCst), 4);
        for (retry, code, timeout) in [
            (false, "busy", Duration::from_secs(1)),
            (true, "unavailable", Duration::from_secs(1)),
            (true, "busy", Duration::from_millis(10)),
        ] {
            let source = make(usize::MAX, retry, code);
            assert!(query(&source, params.clone(), timeout).await.is_err());
            assert_eq!(source.attempts.load(Ordering::SeqCst), 1);
        }
    }
    #[test]
    fn conversion_and_guidance_are_engine_owned_and_clear_with_the_next_reply() {
        let mut conversion = fixture_reply("= 10 km to miles", vec![]);
        conversion["conversion"] =
            json!({"ok":true,"value":"10 km = 6.213711922 mi","description":"Engine description"});
        let mut guidance = fixture_reply("= units", vec![]);
        guidance["message"] = json!("Install qalc for unit conversion");
        let mut failure = fixture_reply("= invalid", vec![]);
        failure["conversion"] = json!({"ok":false,"error":"Engine conversion error"});
        let source = Arc::new(
            Fixture::new(json!({"queries":{
                "= 10 km to miles":conversion,"= units":guidance,"= invalid":failure,
                "plain":fixture_reply("plain",vec![json!({"id":"app","name":"Application"})])
            }}))
            .unwrap(),
        );
        let menu = Menu::new(Some(source)).unwrap();
        let mut stores = Stores::default();
        for (query, value, message, error) in [
            ("= 10 km to miles", "10 km = 6.213711922 mi", "", ""),
            ("= units", "", "Install qalc for unit conversion", ""),
            ("= invalid", "", "", "Engine conversion error"),
            ("plain", "", "", ""),
        ] {
            let edit = BTreeMap::from([("launcher.search.text".into(), Value::Text(query.into()))]);
            stores = stores.reconcile(&edit).unwrap();
            stores.commit(edit).unwrap();
            let projection = project_wait(&menu, &stores).unwrap();
            stores = stores.reconcile(&projection).unwrap();
            stores.commit(projection).unwrap();
            for (key, expected) in [
                ("launcher.conversion.value", value),
                ("launcher.message", message),
                ("launcher.conversion.error", error),
            ] {
                assert_eq!(stores.get(key), Some(&Value::Text(expected.into())));
            }
            assert!(!stores.values().keys().any(|k| k.contains("activation")));
        }
    }

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
                    let prepared = prepare(ok, None).unwrap();
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
        project_wait(&menu, &Stores::default()).unwrap();
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
    fn daemon_stale_revision_drops_the_rows_and_queries_again_like_qml() {
        struct Engine {
            publisher: source::Publisher,
            queries: std::sync::atomic::AtomicUsize,
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
                        self.queries
                            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                        return Ok(fixture_reply(
                            params["query"].as_str().unwrap_or(""),
                            vec![json!({"id":"chosen","name":"Fixture"})],
                        ));
                    }
                    Err(source::Error::new(
                        "stale_revision",
                        "Catalog changed; query again",
                        true,
                    ))
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
            queries: Default::default(),
        });
        let menu = Menu::new(Some(engine.clone())).unwrap();
        project_wait(&menu, &Stores::default()).unwrap();
        let before = engine.queries.load(std::sync::atomic::Ordering::SeqCst);
        let error = menu.activate("chosen").err().unwrap();
        assert_eq!(error.code, "stale_revision");
        assert!(menu.view.borrow().result_stamp.is_none());
        project_wait(&menu, &Stores::default()).unwrap();
        assert!(engine.queries.load(std::sync::atomic::Ordering::SeqCst) > before);
        assert!(menu.view.borrow().rows.iter().any(|r| r["id"] == "chosen"));
    }
    #[test]
    fn live_memory_mirrors_only_bounded_scalar_view_state() {
        let stores = Stores::default()
            .reconcile(&BTreeMap::from([
                (
                    "launcher.search.text".into(),
                    Value::Text("x".repeat(65536)),
                ),
                ("launcher.selection".into(), Value::Text("id".into())),
                ("launcher.route".into(), Value::Text("apps".into())),
                ("launcher.mode".into(), Value::Text("apps".into())),
                ("launcher.engine_route".into(), Value::Text("home".into())),
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
        assert_eq!(remembered.len(), 6);
        assert!(!remembered.contains_key("launcher.apps"));
        assert_eq!(
            remembered["launcher.engine_route"],
            Value::Text("home".into())
        );
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
            matches!(&project_wait(&menu, &stores).unwrap()["launcher.apps"],Value::List(r) if r[0]["name"]=="Old")
        );
        source
            .publisher
            .publish(json!({"queries":{"":fixture_reply("",vec![json!({"id":"x","name":"New"})])}}))
            .unwrap();
        assert!(
            matches!(&project_wait(&menu, &stores).unwrap()["launcher.apps"],Value::List(r) if r[0]["name"]=="New")
        );
    }
    #[test]
    fn same_boot_disconnect_keeps_rows_but_restart_clears_rows_and_fences_activation() {
        struct RestartSource {
            fixture: Fixture,
            activations: std::sync::atomic::AtomicUsize,
        }
        impl Source for RestartSource {
            fn snapshot(&self) -> Arc<source::Snapshot> {
                self.fixture.snapshot()
            }
            fn subscribe(
                &self,
                callback: source::Callback,
            ) -> source::Result<source::Subscription> {
                self.fixture.subscribe(callback)
            }
            fn invoke(&self, method: String, params: Json) -> source::Reply<'_> {
                Box::pin(async move {
                    if method == "menu.activate" {
                        self.activations
                            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                        return Ok(json!({"keep_open":false}));
                    }
                    if method == "menu.query" && self.snapshot().epoch == "boot-b" {
                        tokio::time::sleep(Duration::from_millis(180)).await;
                    }
                    self.fixture.invoke(method, params).await
                })
            }
            fn reconnect(&self) -> source::Reply<'_> {
                self.fixture.reconnect()
            }
            fn stop(&self) -> source::Reply<'_> {
                self.fixture.stop()
            }
        }
        let mut reply = fixture_reply("a", vec![json!({"id":"old-app","name":"Old app"})]);
        reply["message"] = json!("Old boot guidance");
        let source = Arc::new(RestartSource {
            fixture: Fixture::new(json!({"queries":{"a":reply}})).unwrap(),
            activations: Default::default(),
        });
        let menu = Menu::new(Some(source.clone())).unwrap();
        let mut stores = Stores::default()
            .reconcile(&BTreeMap::from([
                ("launcher.search.text".into(), Value::Text("a".into())),
                ("launcher.selection".into(), Value::Text("old-app".into())),
                ("launcher.engine_route".into(), Value::Text(String::new())),
                ("launcher.scroll.y".into(), Value::Number(0.)),
                ("launcher.total_count".into(), Value::Number(0.)),
                ("launcher.pending".into(), Value::Bool(false)),
            ]))
            .unwrap();
        let initial = project_wait(&menu, &stores).unwrap();
        stores = stores.reconcile(&initial).unwrap();
        stores.commit(initial).unwrap();
        assert_eq!(menu.view.borrow().rows[0]["id"], "old-app");
        assert_eq!(menu.view.borrow().message, "Old boot guidance");

        source.fixture.publisher.disconnect().unwrap();
        let disconnected = menu.project(&stores).unwrap();
        assert_eq!(disconnected["launcher.pending"], Value::Bool(true));
        assert_eq!(disconnected["launcher.result_count"], Value::Number(1.));
        assert_eq!(menu.view.borrow().rows[0]["id"], "old-app");
        assert!(menu.activate("old-app").is_err());
        assert_eq!(
            source.activations.load(std::sync::atomic::Ordering::SeqCst),
            0
        );

        source.fixture.publisher.restart("boot-b".into()).unwrap();
        let restarted = menu.project(&stores).unwrap();
        assert_eq!(restarted["launcher.pending"], Value::Bool(true));
        assert_eq!(restarted["launcher.result_count"], Value::Number(0.));
        assert!(menu.view.borrow().rows.is_empty());
        assert_eq!(restarted["launcher.message"], Value::Text(String::new()));
        assert!(menu.activate("old-app").is_err());
        assert_eq!(
            source.activations.load(std::sync::atomic::Ordering::SeqCst),
            0
        );

        let data = source.fixture.snapshot().data.as_ref().clone();
        source.fixture.publisher.publish(data).unwrap();
        let reconnecting = menu.project(&stores).unwrap();
        assert_eq!(reconnecting["launcher.pending"], Value::Bool(true));
        assert_eq!(reconnecting["launcher.result_count"], Value::Number(0.));
        assert!(menu.activate("old-app").is_err());
        assert_eq!(
            source.activations.load(std::sync::atomic::Ordering::SeqCst),
            0
        );

        let fresh = project_wait(&menu, &stores).unwrap();
        assert!(
            matches!(&fresh["launcher.apps"], Value::List(rows) if rows.iter().any(|row| row["id"] == "old-app"))
        );
        assert!(menu.activate("old-app").is_ok());
        assert_eq!(
            source.activations.load(std::sync::atomic::Ordering::SeqCst),
            1
        );
    }
    #[test]
    fn ordered_keyboard_navigation_skips_disabled_wraps_and_contains_scroll() {
        let rows = vec![
            json!({"id":"disabled","name":"Disabled","disabled":true}),
            json!({"id":"a","name":"Alpha"}),
            json!({"id":"b","name":"Beta"}),
            json!({"id":"c","name":"Gamma"}),
        ];
        let source =
            Arc::new(Fixture::new(json!({"queries":{"a":fixture_reply("a",rows)}})).unwrap());
        let menu = Menu::new(Some(source)).unwrap();
        let mut stores = Stores::default()
            .reconcile(&BTreeMap::from([
                ("launcher.search.text".into(), Value::Text("a".into())),
                ("launcher.scroll.y".into(), Value::Number(0.)),
            ]))
            .unwrap();
        let initial = project_wait(&menu, &stores).unwrap();
        stores = stores.reconcile(&initial).unwrap();
        stores.commit(initial).unwrap();
        let geometry = NavigationGeometry {
            columns: 1,
            pitch: 58.,
            row_height: 55.,
            viewport_height: 116.,
        };
        let next = menu.navigate(&stores, "down", geometry).unwrap();
        assert_eq!(next["launcher.selection"], Value::Text("b".into()));
        assert_eq!(next["launcher.scroll.y"], Value::Number(55.));
        stores.commit(next).unwrap();
        stores
            .commit(menu.navigate(&stores, "up", geometry).unwrap())
            .unwrap();
        assert_eq!(
            stores.get("launcher.selection"),
            Some(&Value::Text("a".into()))
        );
        let wrapped = menu.navigate(&stores, "up", geometry).unwrap();
        assert_eq!(wrapped["launcher.selection"], Value::Text("c".into()));
        assert!(
            menu.navigate(&stores, "left", geometry).is_none(),
            "left must remain text editing in a query"
        );
    }
    #[test]
    fn grid_keyboard_navigation_uses_rendered_column_geometry() {
        let rows: Vec<_> = (0..20)
            .map(|id| json!({"id":id.to_string(),"name":format!("Fixture{id}"),"disabled":id==0}))
            .collect();
        let source =
            Arc::new(Fixture::new(json!({"queries":{"":fixture_reply("",rows)}})).unwrap());
        let menu = Menu::new(Some(source)).unwrap();
        let initial = project_wait(&menu, &Stores::default()).unwrap();
        let mut stores = Stores::default()
            .reconcile(&initial)
            .unwrap()
            .reconcile(&BTreeMap::from([
                ("launcher.scroll.y".into(), Value::Number(0.)),
                ("launcher.mode".into(), Value::Text("apps".into())),
            ]))
            .unwrap();
        let geometry = NavigationGeometry {
            columns: 4,
            pitch: 81.,
            row_height: 76.,
            viewport_height: 162.,
        };
        for (key, expected) in [
            ("down", "1"),
            ("right", "2"),
            ("down", "6"),
            ("up", "2"),
            ("left", "1"),
            ("up", "1"),
            ("left", "19"),
        ] {
            let next = menu.navigate(&stores, key, geometry).unwrap();
            assert_eq!(next["launcher.selection"], Value::Text(expected.into()));
            stores.commit(next).unwrap();
        }
        assert_eq!(stores.get("launcher.scroll.y"), Some(&Value::Number(238.)));
    }
    #[test]
    fn query_edit_selects_first_enabled_and_resets_scroll_but_initial_restore_survives() {
        let rows = vec![
            json!({"id":"disabled","name":"Disabled","disabled":true}),
            json!({"id":"a","name":"Alpha"}),
            json!({"id":"b","name":"Beta"}),
        ];
        let source = Arc::new(
            Fixture::new(json!({"queries":{
                "a":fixture_reply("a",rows.clone()),"ab":fixture_reply("ab",rows.clone()),
                "":fixture_reply("",rows)
            }}))
            .unwrap(),
        );
        let menu = Menu::new(Some(source)).unwrap();
        let mut stores = Stores::default()
            .reconcile(&BTreeMap::from([
                ("launcher.search.text".into(), Value::Text("a".into())),
                ("launcher.selection".into(), Value::Text("b".into())),
                ("launcher.scroll.y".into(), Value::Number(123.)),
            ]))
            .unwrap();
        let initial = project_wait(&menu, &stores).unwrap();
        assert_eq!(initial["launcher.selection"], Value::Text("b".into()));
        stores = stores.reconcile(&initial).unwrap();
        stores.commit(initial).unwrap();
        stores
            .commit(BTreeMap::from([(
                "launcher.search.text".into(),
                Value::Text("ab".into()),
            )]))
            .unwrap();
        let changed = project_wait(&menu, &stores).unwrap();
        assert_eq!(changed["launcher.selection"], Value::Text("a".into()));
        assert_eq!(changed["launcher.scroll.y"], Value::Number(0.));
        stores.commit(changed).unwrap();
        stores
            .commit(BTreeMap::from([(
                "launcher.search.text".into(),
                Value::Text(String::new()),
            )]))
            .unwrap();
        let grid = project_wait(&menu, &stores).unwrap();
        assert_eq!(grid["launcher.selection"], Value::Text(String::new()));
    }
    #[test]
    fn catalog_removal_resets_scroll_with_fallback_selection_without_query_edit() {
        let rows = (0..40)
            .map(|i| json!({"id":format!("item{i}"),"name":format!("Item {i}")}))
            .collect::<Vec<_>>();
        let source = Arc::new(
            Fixture::new(json!({"queries":{"a":fixture_reply("a",rows.clone())}})).unwrap(),
        );
        let menu = Menu::new(Some(source.clone())).unwrap();
        let mut stores = Stores::default()
            .reconcile(&BTreeMap::from([
                ("launcher.search.text".into(), Value::Text("a".into())),
                ("launcher.selection".into(), Value::Text("item30".into())),
                ("launcher.scroll.y".into(), Value::Number(1500.)),
            ]))
            .unwrap();
        let initial = project_wait(&menu, &stores).unwrap();
        assert_eq!(initial["launcher.selection"], Value::Text("item30".into()));
        assert!(
            !initial.contains_key("launcher.scroll.y"),
            "valid restored selection must keep scroll"
        );
        stores = stores.reconcile(&initial).unwrap();
        stores.commit(initial).unwrap();
        let mut reply = fixture_reply("a", rows[..10].to_vec());
        reply["catalog_rev"] = json!(2);
        source
            .publisher
            .publish(json!({"queries":{"a":reply}}))
            .unwrap();
        let changed = project_wait(&menu, &stores).unwrap();
        assert_eq!(changed["launcher.selection"], Value::Text("item0".into()));
        assert_eq!(changed["launcher.scroll.y"], Value::Number(0.));
        assert_eq!(
            stores.get("launcher.search.text"),
            Some(&Value::Text("a".into()))
        );
    }
    #[test]
    fn response_for_another_query_cannot_replace_the_view() {
        let source = Arc::new(
            Fixture::new(json!({"queries":{
                "":fixture_reply("",vec![json!({"id":"a","name":"Original"})]),
                "next":fixture_reply("wrong",vec![json!({"id":"b","name":"Wrong query"})])
            }}))
            .unwrap(),
        );
        let menu = Menu::new(Some(source)).unwrap();
        let initial = project_wait(&menu, &Stores::default()).unwrap();
        let mut stores = Stores::default()
            .reconcile(&initial)
            .unwrap()
            .reconcile(&BTreeMap::from([(
                "launcher.search.text".into(),
                Value::Text(String::new()),
            )]))
            .unwrap();
        stores
            .commit(BTreeMap::from([(
                "launcher.search.text".into(),
                Value::Text("next".into()),
            )]))
            .unwrap();
        let changes = project_wait(&menu, &stores).unwrap();
        assert!(matches!(&changes["launcher.error"],Value::Text(error)if !error.is_empty()));
        assert!(!changes.contains_key("launcher.apps"));
        assert_eq!(menu.view.borrow().rows[0]["name"], "Original");
    }
    #[test]
    fn projection_shares_rows_without_mutating_previous_selection_snapshots() {
        let source = Arc::new(
            Fixture::new(json!({"queries":{"":fixture_reply("",vec![
                json!({"id":"a","name":"Alpha"}),json!({"id":"b","name":"Beta"})
            ])}}))
            .unwrap(),
        );
        let menu = Menu::new(Some(source)).unwrap();
        let restored = Stores::default()
            .reconcile(&BTreeMap::from([(
                "launcher.selection".into(),
                Value::Text("a".into()),
            )]))
            .unwrap();
        let initial = project_wait(&menu, &restored).unwrap();
        let Value::List(before) = &initial["launcher.apps"] else {
            panic!("missing model")
        };
        assert!(Arc::ptr_eq(before, &menu.view.borrow().rows));
        let mut stores = Stores::default().reconcile(&initial).unwrap();
        assert!(
            !project_wait(&menu, &stores)
                .unwrap()
                .contains_key("launcher.apps")
        );
        assert!(Arc::ptr_eq(before, &menu.view.borrow().rows));
        stores
            .commit(BTreeMap::from([(
                "launcher.selection".into(),
                Value::Text("b".into()),
            )]))
            .unwrap();
        let next = project_wait(&menu, &stores).unwrap();
        let Value::List(after) = &next["launcher.apps"] else {
            panic!("missing changed model")
        };
        assert!(!Arc::ptr_eq(before, after));
        assert!(Arc::ptr_eq(after, &menu.view.borrow().rows));
        assert_eq!(before[0]["selected"], true);
        assert_eq!(before[1]["selected"], false);
        assert_eq!(after[0]["selected"], false);
        assert_eq!(after[1]["selected"], true);
    }
    #[test]
    fn fixture_preferences_seed_the_published_load_reply() {
        let preferences =
            json!({"favorites":["org.example.app.desktop"],"hidden":[],"recent":[],"usage":{}});
        let fixture = Arc::new(
            Fixture::new(
                json!({"queries":{"":fixture_reply("",vec![])},"preferences":preferences}),
            )
            .unwrap(),
        );
        let menu = Menu::new(Some(fixture.clone())).unwrap();
        let reply = menu
            .runtime
            .block_on(fixture.invoke("menu.preferences".into(), json!({"action":"load"})))
            .unwrap();
        assert_eq!(reply["favorites"], json!(["org.example.app.desktop"]));
        assert!(
            Fixture::new(
                json!({"queries":{"":fixture_reply("",vec![])},"preferences":{"favorites":[false]}})
            )
            .is_err()
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
            matches!(&project_wait(&menu, &stores).unwrap()["launcher.apps"],Value::List(r) if r[0]["id"]=="a")
        );
        stores
            .commit(BTreeMap::from([(
                "launcher.search.text".into(),
                Value::Text("missing".into()),
            )]))
            .unwrap();
        let v = project_wait(&menu, &stores).unwrap();
        assert!(matches!(&v["launcher.error"],Value::Text(s) if !s.is_empty()));
        assert!(
            menu.runtime
                .block_on(source.invoke("menu.activate".into(), json!({})))
                .is_err()
        );
    }
}
