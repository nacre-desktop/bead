// SPDX-License-Identifier: MIT OR Apache-2.0
//! Syntax boundary. Other frontends return the same format-independent view IR.
use crate::{
    Diagnostic,
    layout::{RawNode, Widget},
};
use bead_core::Value;
use mlua::{Function, Lua, LuaSerdeExt, Table, VmState};
use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};
pub type Sources = BTreeMap<PathBuf, Vec<u8>>;
pub type Resolve<'a> = dyn Fn(&str) -> Result<Value, Diagnostic> + 'a;
pub trait FrontEnd {
    fn build(&self, entry: &Path, resolve: &Resolve<'_>) -> Result<RawNode, Diagnostic>;
}
const INSTALL: &str = r#"
return function(ui)
    local function check(value, schema, path)
        value = ui.resolve_for_check(value)
        if type(schema) == "string" then
            if type(value) ~= schema then error(path .. " must be " .. schema, 3) end
        else
            if type(value) ~= "table" then error(path .. " must be a record", 3) end
            for key, expected in schema do check(value[key], expected, path .. "." .. key) end
        end
    end
    ui.component = function(name, schema, render)
        return function(props)
            for key in props do if schema[key] == nil then error(name .. ": unknown property " .. key, 2) end end
            for key, expected in schema do check(props[key], expected, name .. "." .. key) end
            return render(props)
        end
    end
end
"#;
/// Immutable generation-owned bytecode; evaluation always uses a fresh sandbox.
pub struct LuauFrontEnd {
    compiled: Sources,
    kit: Vec<u8>,
    install: Vec<u8>,
}
impl LuauFrontEnd {
    pub fn new(entry: &Path, sources: &Sources) -> Result<Self, Diagnostic> {
        let compiler = mlua::chunk::Compiler::new();
        let mut compiled = Sources::new();
        let mut size = 0;
        for (path, bytes) in sources {
            if path.extension().is_some_and(|ext| ext == "luau") {
                let code = compiler.compile(bytes).map_err(|e| diagnostic(path, e))?;
                size += code.len();
                if size > 8 * 1024 * 1024 {
                    return Err(Diagnostic::new(
                        path,
                        1,
                        1,
                        "compiled markup exceeds 8 MiB cache budget",
                    ));
                }
                compiled.insert(path.clone(), code);
            }
        }
        Ok(Self {
            compiled,
            kit: compiler
                .compile(include_str!("../assets/kit.luau"))
                .map_err(|e| diagnostic(entry, e))?,
            install: compiler
                .compile(INSTALL)
                .map_err(|e| diagnostic(entry, e))?,
        })
    }
}
impl FrontEnd for LuauFrontEnd {
    fn build(&self, entry: &Path, resolve: &Resolve<'_>) -> Result<RawNode, Diagnostic> {
        let sources = &self.compiled;
        let lua = Lua::new();
        let deadline = Instant::now() + Duration::from_millis(50);
        lua.set_interrupt(move |_| {
            if Instant::now() > deadline {
                Err(mlua::Error::runtime("markup exceeded 50 ms limit"))
            } else {
                Ok(VmState::Continue)
            }
        });
        let result = (|| -> mlua::Result<RawNode> {
            lua.set_memory_limit(8 * 1024 * 1024)?;
            lua.sandbox(true)?;
            lua.scope(|scope| {
                let sdk = lua.create_table()?;
                for (name, kind) in Widget::constructors() {
                    let constructor = scope
                        .create_function(move |lua, props: Table| make_node(lua, kind, props))?;
                    sdk.set(name, constructor)?;
                }
                sdk.set(
                    "alpha",
                    scope.create_function(|lua, (role, alpha): (String, f64)| {
                        let t = lua.create_table()?;
                        t.set("role", role)?;
                        t.set("alpha", alpha)?;
                        Ok(t)
                    })?,
                )?;
                sdk.set(
                    "tint",
                    scope.create_function(|lua, (base, over, alpha): (String, String, f64)| {
                        let t = lua.create_table()?;
                        t.set("base", base)?;
                        t.set("over", over)?;
                        t.set("alpha", alpha)?;
                        Ok(t)
                    })?,
                )?;
                sdk.set(
                    "resolve_for_check",
                    scope.create_function(|lua, value: mlua::Value| {
                        if let mlua::Value::Table(table) = &value
                            && let Ok(name) = table.get::<String>("__bind")
                        {
                            return lua.to_value(&resolve(&name).map_err(mlua::Error::external)?);
                        }
                        Ok(value)
                    })?,
                )?;
                let install: Function = lua
                    .load(self.install.as_slice())
                    .set_name("@bead.ui")
                    .eval()?;
                install.call::<()>(sdk.clone())?;
                sdk.set(
                    "bind",
                    scope.create_function(|lua, name: String| {
                        let marker = lua.create_table()?;
                        marker.set("__bind", name)?;
                        Ok(marker)
                    })?,
                )?;
                sdk.set(
                    "action",
                    scope.create_function(|lua, (name, arg): (String, Option<mlua::Value>)| {
                        let action = lua.create_table()?;
                        action.set("name", name)?;
                        if let Some(arg) = arg {
                            action.set("argument", arg)?;
                        }
                        Ok(action)
                    })?,
                )?;
                sdk.set(
                    "Repeat",
                    scope.create_function(|lua, props: Table| {
                        let id: String = props.get("id")?;
                        let key: String = props.get("key")?;
                        let model: Table = props.get("model")?;
                        let name: String = model.get("__bind")?;
                        let Value::List(rows) = resolve(&name).map_err(mlua::Error::external)?
                        else {
                            return Err(mlua::Error::runtime(format!(
                                "model {name} must be a list"
                            )));
                        };
                        let render: Function = props.get("render")?;
                        let children = lua.create_table()?;
                        let mut seen = BTreeSet::new();
                        for (i, item) in rows.iter().enumerate() {
                            let key = item
                                .get(&key)
                                .and_then(|v| v.as_str())
                                .filter(|key| !key.is_empty() && key.len() <= 256)
                                .ok_or_else(|| {
                                    mlua::Error::runtime(format!(
                                        "{name}: missing string key {} at item {i}",
                                        key
                                    ))
                                })?;
                            if !seen.insert(key) {
                                return Err(mlua::Error::runtime(format!(
                                    "{name}: duplicate key {key}"
                                )));
                            }
                            let row: Table = render.call(lua.to_value(item)?)?;
                            let prefix = format!(
                                "{id}.k{}.",
                                key.bytes().map(|b| format!("{b:02x}")).collect::<String>()
                            );
                            row.set("scope", prefix)?;
                            children.set(i + 1, row)?;
                        }
                        let clean = lua.create_table()?;
                        clean.set("id", id)?;
                        let node = make_node(lua, Widget::Group, clean)?;
                        node.set("children", children)?;
                        Ok(node)
                    })?,
                )?;
                let modules = RefCell::new(BTreeMap::<PathBuf, mlua::Value>::new());
                let loading = RefCell::new(BTreeSet::<PathBuf>::new());
                let entry_parent = entry.parent().unwrap_or(Path::new("")).to_path_buf();
                let sdk_copy = sdk.clone();
                let kit_factory: Function =
                    lua.load(self.kit.as_slice()).set_name("@bead.kit").eval()?;
                let tokens: serde_json::Value =
                    serde_json::from_str(include_str!("../assets/size-scale.json"))
                        .map_err(mlua::Error::external)?;
                let kit: Table = kit_factory.call((sdk.clone(), lua.to_value(&tokens)?))?;
                let require = scope.create_function(move |lua, name: String| {
                    if name == "bead.kit" {
                        return Ok(mlua::Value::Table(kit.clone()));
                    }
                    if name == "bead.ui" {
                        return Ok(mlua::Value::Table(sdk_copy.clone()));
                    }
                    let caller = source(lua, entry);
                    let base = Path::new(&caller.file).parent().unwrap_or(&entry_parent);
                    let mut path = base.join(&name);
                    if path.extension().is_none() {
                        path.set_extension("luau");
                    }
                    let path = normalize(&path).map_err(mlua::Error::runtime)?;
                    if let Some(value) = modules.borrow().get(&path) {
                        return Ok(value.clone());
                    }
                    if !loading.borrow_mut().insert(path.clone()) {
                        return Err(mlua::Error::runtime(format!(
                            "import cycle at {}",
                            path.display()
                        )));
                    }
                    let bytes = sources.get(&path).ok_or_else(|| {
                        mlua::Error::runtime(format!(
                            "import {} is outside the tracked UI bundle",
                            path.display()
                        ))
                    })?;
                    let value: mlua::Value = lua
                        .load(bytes)
                        .set_name(format!("@{}", path.display()))
                        .eval()?;
                    loading.borrow_mut().remove(&path);
                    modules.borrow_mut().insert(path, value.clone());
                    Ok(value)
                })?;
                lua.globals().set("require", require)?;
                let bytes = sources
                    .get(entry)
                    .ok_or_else(|| mlua::Error::runtime("missing entry source"))?;
                let node: mlua::Value = lua
                    .load(bytes)
                    .set_name(format!("@{}", entry.display()))
                    .eval()?;
                lua.from_value(node)
            })
        })();
        result.map_err(|error| diagnostic(entry, error))
    }
}
fn normalize(path: &Path) -> Result<PathBuf, String> {
    let mut result = PathBuf::new();
    for part in path.components() {
        match part {
            std::path::Component::Normal(s) => result.push(s),
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir if result.pop() => {}
            _ => return Err("import escapes the UI bundle".into()),
        }
    }
    Ok(result)
}
fn source(lua: &Lua, fallback: &Path) -> crate::layout::Source {
    lua.inspect_stack(1, |debug| {
        let source = debug.source();
        crate::layout::Source {
            file: source
                .source
                .as_deref()
                .unwrap_or("")
                .trim_start_matches('@')
                .to_owned(),
            line: debug.current_line().unwrap_or(1),
        }
    })
    .filter(|s| !s.file.is_empty())
    .unwrap_or(crate::layout::Source {
        file: fallback.display().to_string(),
        line: 1,
    })
}
fn make_node(lua: &Lua, kind: Widget, props: Table) -> mlua::Result<Table> {
    let source = source(lua, Path::new("ui/main.luau"));
    let result = lua.create_table()?;
    let named = lua.create_table()?;
    let children = lua.create_table()?;
    for pair in props.pairs::<mlua::Value, mlua::Value>() {
        let (key, value) = pair?;
        match key {
            mlua::Value::String(name) => {
                let name = name.to_str()?;
                if !crate::layout::allowed_property(&name) {
                    return Err(mlua::Error::runtime(format!(
                        "unknown {kind:?} property {name}"
                    )));
                }
                named.set(name, value)?;
            }
            mlua::Value::Integer(index) if index > 0 => children.set(index, value)?,
            _ => {
                return Err(mlua::Error::runtime(
                    "node properties need names or ordered children",
                ));
            }
        }
    }
    result.set("kind", lua.to_value(&kind)?)?;
    result.set("props", named)?;
    // A sequence marker preserves empty lists when crossing serde.
    let mut children_json: Vec<mlua::Value> = Vec::new();
    for child in children.sequence_values::<mlua::Value>() {
        children_json.push(child?);
    }
    result.set("children", lua.to_value(&Vec::<u8>::new())?)?;
    let array: Table = result.get("children")?;
    for (i, value) in children_json.into_iter().enumerate() {
        array.set(i + 1, value)?;
    }
    result.set("source", lua.to_value(&source)?)?;
    Ok(result)
}
fn diagnostic(entry: &Path, error: mlua::Error) -> Diagnostic {
    let message = error.to_string();
    for line in message.lines() {
        for candidate in line.split_whitespace() {
            if let Some((path, rest)) = candidate.split_once(".luau:")
                && let Some(number) = rest.split(':').next().and_then(|s| s.parse::<usize>().ok())
            {
                return Diagnostic::new(
                    Path::new(&format!("{}.luau", path.trim_matches(['[', '\"', '@']))),
                    number,
                    1,
                    &message,
                );
            }
        }
    }
    Diagnostic::new(entry, 1, 1, message)
}
