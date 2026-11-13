// SPDX-License-Identifier: MIT
//! Graph data handed from Luau to Rust once (spec 0232). `ui.graph(nodes, edges)` walks the
//! Luau tables straight into one compact [`GraphResource`] and returns a handle; the handle
//! serializes into the view tree as a small token, so no JSON copy of the graph exists, and
//! the `Graph` node resolves the token back to the shared resource while the build is current.
use mlua::{AnyUserData, Lua, Table, Value as LuaValue};
use std::{
    cell::RefCell,
    sync::{Arc, Weak},
};

const MAX_NODES: usize = 20_000;
const MAX_EDGES: usize = 100_000;

#[derive(Debug, PartialEq)]
pub struct GraphNode {
    pub id: Box<str>,
    pub label: Box<str>,
    pub weight: f32,
}
/// One graph's topology and node attributes; shared by the view node and its canvas.
#[derive(Debug, Default, PartialEq)]
pub struct GraphResource {
    pub nodes: Vec<GraphNode>,
    pub edges: Arc<[[u32; 2]]>,
    /// Node indices by descending weight: the label priority.
    pub priority: Vec<u32>,
    /// Node indices sorted by id.
    by_id: Vec<u32>,
}
fn id_text(value: &LuaValue) -> mlua::Result<Option<Box<str>>> {
    Ok(match value {
        LuaValue::String(s) => {
            let s = s.to_str()?;
            (!s.is_empty() && s.len() <= 256).then(|| s.as_ref().into())
        }
        LuaValue::Integer(i) => Some(i.to_string().into()),
        LuaValue::Number(n) => Some(n.to_string().into()),
        _ => None,
    })
}
fn records(table: &Table, what: &str, max: usize) -> mlua::Result<Vec<Table>> {
    if table.raw_len() > max {
        return Err(mlua::Error::runtime(format!(
            "GraphCanvas supports at most {max} {what}"
        )));
    }
    table
        .sequence_values::<Table>()
        .map(|item| item.map_err(|_| mlua::Error::runtime(format!("graph {what} must be records"))))
        .collect()
}
impl GraphResource {
    pub fn from_lua(nodes: &Table, edges: &Table) -> mlua::Result<Self> {
        let mut out = Self::default();
        for item in records(nodes, "nodes", MAX_NODES)? {
            let id = id_text(&item.raw_get::<LuaValue>("id")?)?
                .ok_or_else(|| mlua::Error::runtime("GraphCanvas node needs an id"))?;
            let label = match item.raw_get::<LuaValue>("label")? {
                LuaValue::Nil => id.clone(),
                LuaValue::String(s) => s.to_str()?.as_ref().into(),
                _ => "".into(),
            };
            let weight = match item.raw_get::<LuaValue>("weight")? {
                LuaValue::Nil => 1.,
                LuaValue::Integer(i) => i as f64,
                LuaValue::Number(n) => n,
                _ => f64::NAN,
            };
            if !(weight >= 0. && (weight as f32).is_finite()) {
                return Err(mlua::Error::runtime(format!(
                    "GraphCanvas node {id} needs a finite weight >= 0"
                )));
            }
            out.nodes.push(GraphNode {
                id,
                label,
                weight: weight as f32,
            });
        }
        let nodes = &out.nodes;
        let mut by_id: Vec<u32> = (0..nodes.len() as u32).collect();
        by_id.sort_by(|a, b| nodes[*a as usize].id.cmp(&nodes[*b as usize].id));
        if let Some(pair) = by_id
            .windows(2)
            .find(|w| nodes[w[0] as usize].id == nodes[w[1] as usize].id)
        {
            return Err(mlua::Error::runtime(format!(
                "duplicate GraphCanvas node id {}",
                nodes[pair[0] as usize].id
            )));
        }
        let mut priority = by_id.clone();
        priority.sort_by(|a, b| {
            nodes[*b as usize]
                .weight
                .total_cmp(&nodes[*a as usize].weight)
        });
        out.by_id = by_id;
        out.priority = priority;
        let mut pairs = Vec::new();
        for item in records(edges, "edges", MAX_EDGES)? {
            let end = |key: &str| -> mlua::Result<u32> {
                id_text(&item.raw_get::<LuaValue>(key)?)?
                    .and_then(|id| out.index(&id))
                    .ok_or_else(|| {
                        mlua::Error::runtime(format!("GraphCanvas edge {key} must name a node"))
                    })
            };
            pairs.push([end("from")?, end("to")?]);
        }
        out.edges = pairs.into();
        Ok(out)
    }
    pub fn index(&self, id: &str) -> Option<u32> {
        self.by_id
            .binary_search_by(|i| self.nodes[*i as usize].id.as_ref().cmp(id))
            .ok()
            .map(|k| self.by_id[k])
    }
}

/// The Luau-side handle; it serializes as its token so the view tree stays small.
pub struct GraphHandle {
    token: u64,
    _data: Arc<GraphResource>,
}
impl serde::Serialize for GraphHandle {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_u64(self.token)
    }
}
impl mlua::UserData for GraphHandle {}

#[derive(Default)]
struct Registry {
    next: u64,
    live: Vec<(u64, Weak<GraphResource>)>,
    /// Resources handed over during the current build, alive until the node holds them.
    fresh: Vec<Arc<GraphResource>>,
}
thread_local! {
    static REGISTRY: RefCell<Registry> = RefCell::default();
}
/// `ui.graph(nodes, edges)`: parse once and hand back a handle. Equal content keeps its
/// token, so an unchanged view tree still patches in place without re-parsing the tree.
/// The caller excludes this native time from the markup budget, as for text measurement.
pub(crate) fn hand_over(lua: &Lua, nodes: &Table, edges: &Table) -> mlua::Result<AnyUserData> {
    let data = GraphResource::from_lua(nodes, edges)?;
    let (token, data) = REGISTRY.with_borrow_mut(|r| {
        r.live.retain(|(_, weak)| weak.strong_count() > 0);
        let same = r.live.iter().find_map(|(token, weak)| {
            weak.upgrade()
                .filter(|live| **live == data)
                .map(|live| (*token, live))
        });
        same.unwrap_or_else(|| {
            let data = Arc::new(data);
            r.next += 1;
            r.live.push((r.next, Arc::downgrade(&data)));
            r.fresh.push(data.clone());
            (r.next, data)
        })
    });
    lua.create_ser_userdata(GraphHandle { token, _data: data })
}
#[cfg(test)]
fn lua_function(lua: &Lua) -> mlua::Result<mlua::Function> {
    lua.create_function(|lua, (nodes, edges): (Table, Table)| hand_over(lua, &nodes, &edges))
}
/// The resource behind a view node's `graph` token, while something still holds it.
pub(crate) fn resolve(token: u64) -> Option<Arc<GraphResource>> {
    REGISTRY.with_borrow(|r| {
        r.live
            .iter()
            .find(|(t, _)| *t == token)
            .and_then(|(_, weak)| weak.upgrade())
    })
}
/// A new build starts: resources only the previous build handed over may go.
pub(crate) fn begin_build() {
    REGISTRY.with_borrow_mut(|r| r.fresh.clear());
}
/// True when the current build handed over new graph data.
pub(crate) fn handed_over() -> bool {
    REGISTRY.with_borrow(|r| !r.fresh.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use mlua::LuaSerdeExt;

    fn parse(lua: &Lua, source: &str) -> mlua::Result<GraphResource> {
        let (nodes, edges): (Table, Table) = lua.load(source).eval()?;
        GraphResource::from_lua(&nodes, &edges)
    }

    #[test]
    fn parses_orders_priority_and_resolves_edges() {
        let lua = Lua::new();
        let g = parse(
            &lua,
            r#"return {{id="a",label="Alpha",weight=5},{id=7},{id="c",weight=2}},
                      {{from="a",to=7},{from="c",to="a"}}"#,
        )
        .unwrap();
        assert_eq!(g.nodes[1].id.as_ref(), "7");
        assert_eq!(g.nodes[1].label.as_ref(), "7");
        assert_eq!(g.priority, [0, 2, 1]);
        assert_eq!(&*g.edges, &[[0, 1], [2, 0]]);
        assert_eq!(g.index("c"), Some(2));
        assert_eq!(g.index("x"), None);
        assert!(parse(&lua, "return {}, {}").unwrap().nodes.is_empty());
    }

    #[test]
    fn rejects_bad_ids_weights_and_edges() {
        let lua = Lua::new();
        for source in [
            r#"return {{label="no id"}}, {}"#,
            r#"return {{id="a"},{id="a"}}, {}"#,
            r#"return {{id="a",weight=-1}}, {}"#,
            r#"return {{id="a",weight=0/0}}, {}"#,
            r#"return {{id="a"}}, {{from="a",to="b"}}"#,
            r#"return {1}, {}"#,
            r#"return {{id=""}}, {}"#,
        ] {
            assert!(parse(&lua, source).is_err(), "{source}");
        }
    }

    #[test]
    fn handles_serialize_as_stable_tokens_and_expire_with_their_holders() {
        let lua = Lua::new();
        lua.globals()
            .set("graph", lua_function(&lua).unwrap())
            .unwrap();
        let first: mlua::Value = lua.load(r#"return graph({{id="a"}}, {})"#).eval().unwrap();
        let token: serde_json::Value = lua.from_value(first.clone()).unwrap();
        let token = token.as_u64().unwrap();
        let data = resolve(token).unwrap();
        // Equal content from a later call shares the token and the resource.
        let again: mlua::Value = lua.load(r#"return graph({{id="a"}}, {})"#).eval().unwrap();
        assert_eq!(lua.from_value::<serde_json::Value>(again).unwrap(), token);
        let other: mlua::Value = lua.load(r#"return graph({{id="b"}}, {})"#).eval().unwrap();
        assert_ne!(lua.from_value::<serde_json::Value>(other).unwrap(), token);
        // Once the Luau handles are collected and the next build begins, only the
        // resolved resource keeps the token alive.
        drop(first);
        lua.gc_collect().unwrap();
        begin_build();
        assert!(resolve(token).is_some(), "the caller still holds it");
        drop(data);
        assert!(resolve(token).is_none());
    }
}
