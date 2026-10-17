// SPDX-License-Identifier: GPL-3.0-only
//! View projection of ordered engine results. No catalog, providers or ranking.
use bead::core::{
    Stores, Value,
    source::{self, Source},
};
use nacre_protocol::menu::{
    MAX_ITEMS, MenuActivate, MenuActivated, MenuConversion, MenuQuery, MenuQueryResult,
};
use serde_json::{Value as Json, json};
use std::{cell::RefCell, collections::BTreeMap, path::Path, sync::Arc, time::Duration};

pub type Memory = Arc<std::sync::Mutex<BTreeMap<String, Value>>>;
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
}
impl SourceChoice {
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

#[derive(Default)]
struct View {
    request: Option<(String, String, String)>,
    actions: BTreeMap<String, String>,
    catalog_rev: u64,
    stamp: Option<(String, u64, bool)>,
    rows: Arc<Vec<Json>>,
    total: usize,
    force_full: bool,
    last_scroll: f64,
    error: String,
    conversion: Option<MenuConversion>,
    message: String,
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
        let selection = text("launcher.selection");
        let choose_first =
            !request.0.trim().is_empty() || request.1 != "apps" || !request.2.is_empty();
        let stamp = self.source.as_ref().map(|s| {
            let snap = s.snapshot();
            (snap.epoch.clone(), snap.revision, snap.connected)
        });
        let mut view = self.view.borrow_mut();
        let query_changed = view
            .request
            .as_ref()
            .is_some_and(|previous| previous.0 != request.0);
        let scroll = match stores.get("launcher.scroll.y") {
            Some(Value::Number(n)) => *n,
            _ => 0.,
        };
        let expand = view.force_full
            || (view.request.as_ref() == Some(&request)
                && view.total > view.rows.len()
                && scroll > 0.);
        view.force_full = false;
        if view.request.as_ref() != Some(&request) || view.stamp != stamp || expand {
            // Only edited queries use a ranked prefix. Initial/restored state,
            // catalog changes, navigation and scrolling still load the full reply.
            // Missing optional store means an older recovery bundle: stay compatible.
            let limited = query_changed
                && !request.0.trim().is_empty()
                && view.stamp == stamp
                && !expand
                && view
                    .request
                    .as_ref()
                    .is_some_and(|old| old.1 == request.1 && old.2 == request.2)
                && matches!(stores.get("launcher.total_count"), Some(Value::Number(_)));
            let result = if let Some(source) = &self.source {
                let _query = bead::core::profile::Span::new("menu_query");
                let mut params = json!({"query":request.0,"mode":request.1,
                    "route":if request.2.is_empty(){Json::Null}else{json!(request.2)}});
                if limited {
                    params["limit"] = json!(64);
                }
                self.runtime
                    .block_on(query(
                        source.as_ref(),
                        params,
                        Duration::from_secs(3),
                    ))
                    .and_then(|reply| prepare(reply, Some(request.0.as_str())))
                    .and_then(|prepared| {
                        if limited && (!prepared.total_known || (prepared.total > prepared.rows.len()
                            && !prepared.rows.iter().any(|row| row["disabled"] != true))) {
                            self.runtime.block_on(query(source.as_ref(),
                                json!({"query":request.0,"mode":request.1,"route":if request.2.is_empty(){Json::Null}else{json!(request.2)}}),
                                Duration::from_secs(3)))
                                .and_then(|reply| prepare(reply, Some(request.0.as_str())))
                        } else { Ok(prepared) }
                    })
            } else {
                Err(source::Error::new(
                    "unavailable",
                    "Launcher service unavailable",
                    true,
                ))
            };
            match result {
                Ok(prepared) => {
                    if bead::core::profile::enabled() {
                        println!(
                            "MENU_RESULT ok=true catalog_rev={} count={}",
                            prepared.catalog_rev,
                            prepared.rows.len()
                        );
                    }
                    view.total = prepared.total;
                    view.rows = Arc::new(prepared.rows);
                    view.actions = prepared.actions;
                    view.catalog_rev = prepared.catalog_rev;
                    view.conversion = prepared.conversion;
                    view.message = prepared.message;
                    view.error.clear();
                }
                Err(e) => {
                    if bead::core::profile::enabled() {
                        println!("MENU_RESULT ok=false code={} retry={}", e.code, e.retry);
                    }
                    view.error = e.message;
                }
            }
            view.request = Some(request);
            view.stamp = stamp;
        }
        let reset_selection = query_changed && view.error.is_empty();
        let keep_selection = !view.error.is_empty()
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
                "launcher.result_count".into(),
                Value::Number(view.rows.len() as f64),
            ),
            ("launcher.selection".into(), Value::Text(selection)),
            ("launcher.error".into(), Value::Text(view.error.clone())),
        ]);
        if let Some(mode) = restore_mode {
            delta.insert("launcher.mode".into(), Value::Text(mode));
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
        if !view.error.is_empty() && scroll != view.last_scroll {
            // Failed expansion keeps the last good viewport, never an unloaded region.
            delta.insert("launcher.scroll.y".into(), Value::Number(view.last_scroll));
        } else if reset_scroll {
            view.last_scroll = 0.;
            delta.insert("launcher.scroll.y".into(), Value::Number(0.));
        } else {
            view.last_scroll = scroll;
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
        Ok(delta)
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
                self.calls.lock().unwrap().push(params.clone());
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
                ("launcher.search.text".into(), Value::Text(String::new())),
                ("launcher.selection".into(), Value::Text(String::new())),
                ("launcher.scroll.y".into(), Value::Number(0.)),
            ]))
            .unwrap()
    }
    fn apply_projection(menu: &Menu, stores: &mut Stores) {
        let delta = menu.project(stores).unwrap();
        *stores = stores.reconcile(&delta).unwrap();
        stores.commit(delta).unwrap();
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
        let delta = menu.navigate(&stores, "up", geometry).unwrap();
        assert_eq!(delta["launcher.result_count"], Value::Number(320.));
        assert_eq!(delta["launcher.selection"], Value::Text("app319".into()));
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
        let down = menu.navigate(&stores, "down", geometry).unwrap();
        assert_eq!(down["launcher.selection"], Value::Text("app64".into()));
        assert_eq!(down["launcher.result_count"], Value::Number(320.));
        assert_eq!(source.calls.lock().unwrap().len(), 3);
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
        let delta = menu.project(&stores).unwrap();
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
        let delta = menu.project(&stores).unwrap();
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
            let projection = menu.project(&stores).unwrap();
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
        let initial = menu.project(&stores).unwrap();
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
        let initial = menu.project(&Stores::default()).unwrap();
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
        let initial = menu.project(&stores).unwrap();
        assert_eq!(initial["launcher.selection"], Value::Text("b".into()));
        stores = stores.reconcile(&initial).unwrap();
        stores.commit(initial).unwrap();
        stores
            .commit(BTreeMap::from([(
                "launcher.search.text".into(),
                Value::Text("ab".into()),
            )]))
            .unwrap();
        let changed = menu.project(&stores).unwrap();
        assert_eq!(changed["launcher.selection"], Value::Text("a".into()));
        assert_eq!(changed["launcher.scroll.y"], Value::Number(0.));
        stores.commit(changed).unwrap();
        stores
            .commit(BTreeMap::from([(
                "launcher.search.text".into(),
                Value::Text(String::new()),
            )]))
            .unwrap();
        let grid = menu.project(&stores).unwrap();
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
        let initial = menu.project(&stores).unwrap();
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
        let changed = menu.project(&stores).unwrap();
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
        let initial = menu.project(&Stores::default()).unwrap();
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
        let changes = menu.project(&stores).unwrap();
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
        let initial = menu.project(&restored).unwrap();
        let Value::List(before) = &initial["launcher.apps"] else {
            panic!("missing model")
        };
        assert!(Arc::ptr_eq(before, &menu.view.borrow().rows));
        let mut stores = Stores::default().reconcile(&initial).unwrap();
        assert!(!menu.project(&stores).unwrap().contains_key("launcher.apps"));
        assert!(Arc::ptr_eq(before, &menu.view.borrow().rows));
        stores
            .commit(BTreeMap::from([(
                "launcher.selection".into(),
                Value::Text("b".into()),
            )]))
            .unwrap();
        let next = menu.project(&stores).unwrap();
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
