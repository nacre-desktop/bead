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
    fn build(
        &self,
        entry: &Path,
        resolve: &Resolve<'_>,
    ) -> Result<std::rc::Rc<RawNode>, Diagnostic>;
}
const INSTALL: &str = r#"
return function(ui, constructors, allowed, arrayMarker)
    local function string_arg(value, label)
        if type(value) == "number" then return tostring(value) end
        if type(value) ~= "string" or utf8.len(value) == nil then error(label .. " must be a UTF-8 string", 3) end
        return value
    end
    local function number_arg(value, label)
        if type(value) == "number" then return value end
        local result = type(value) == "string" and tonumber(value) or nil
        if result == nil then error(label .. " must be a number", 3) end
        return result
    end
    ui.alpha = function(role, alpha)
        -- A colour expression (a tint) keeps its own alpha under this one.
        return {role=type(role)=="table" and (role.base ~= nil or role.role ~= nil) and role or string_arg(role, "alpha role"), alpha=type(alpha)=="table" and alpha.__bind and alpha or number_arg(alpha, "alpha")}
    end
    ui.tint = function(base, over, alpha)
        return {base=type(base)=="table" and base or string_arg(base,"tint base"),
            over=type(over)=="table" and over or string_arg(over,"tint over"), alpha=type(alpha)=="table" and alpha.__bind and alpha or number_arg(alpha, "tint alpha")}
    end
    ui.bind = function(name)
        return {__bind=string_arg(name, "bind name")}
    end
    ui.action = function(name, argument)
        return {name=string_arg(name, "action name"), argument=argument}
    end
    local inspect = debug.info
    for name, kind in constructors do
        ui[name] = function(props)
            if type(props) ~= "table" then error("node properties need a table", 2) end
            local plain = getmetatable(props) == nil
            local named = plain and table.clone(props) or {}
            for key, value in next, props do
                if type(key) == "string" then
                    if not allowed[key] then error("unknown " .. name .. " property " .. key, 2) end
                    if not plain then named[key] = value end
                elseif type(key) == "number" and key > 0 and key % 1 == 0 then
                    if plain then named[key] = nil end
                else
                    error("node properties need names or ordered children", 2)
                end
            end
            if name == "Polyline" and type(named.points) == "table" and next(named.points) == nil then
                named.points = setmetatable({}, arrayMarker)
            end
            local children = setmetatable({}, arrayMarker)
            local index, child = 1, rawget(props, 1)
            while child ~= nil do
                children[index] = child
                index += 1
                child = rawget(props, index)
            end
            local file, line = inspect(2, "sl")
            file = (file or ""):gsub("^@+", "")
            return {kind=kind, props=named, children=children,
                source={file=file ~= "" and file or "ui/main.luau", line=line or 1}}
        end
    end
    local function check(value, schema)
        -- Ordinary row scalars/records need no Rust callback. Bind markers keep
        -- the same resolution and every property still receives its type check.
        if type(value) == "table" and value.__bind ~= nil then
            value = ui.resolve_for_check(value)
        end
        if type(schema) == "string" then
            if type(value) ~= schema then return " must be " .. schema end
        else
            if type(value) ~= "table" then return " must be a record" end
            for key, expected in schema do
                local suffix = check(value[key], expected)
                if suffix then return "." .. key .. suffix end
            end
        end
        return nil
    end
    ui.component = function(name, schema, render)
        return function(props)
            for key in props do if schema[key] == nil then error(name .. ": unknown property " .. key, 2) end end
            for key, expected in schema do
                local suffix = check(props[key], expected)
                if suffix then error(name .. "." .. key .. suffix, 2) end
            end
            return render(props)
        end
    end
end
"#;
/// Immutable generation-owned bytecode; evaluation always uses a fresh sandbox.
type Materialized = (
    std::rc::Rc<RawNode>,
    BTreeMap<String, Value>,
    bead_core::cache::Charge,
);
struct Fragment {
    bytes: usize,
    raw: RawNode,
    reads: BTreeMap<String, Value>,
    _charge: Option<bead_core::cache::Charge>,
}
fn expand_fragments(
    raw: &mut RawNode,
    fragments: &[std::rc::Rc<Fragment>],
    remaining: &mut usize,
) -> mlua::Result<()> {
    if let Some(index) = raw.props.get("__memo").and_then(serde_json::Value::as_u64) {
        let fragment = fragments
            .get(index as usize)
            .ok_or_else(|| mlua::Error::runtime("invalid memo fragment"))?;
        let bytes = crate::layout::raw_bytes(&fragment.raw);
        *remaining = remaining
            .checked_sub(bytes)
            .ok_or_else(|| mlua::Error::runtime("expanded markup exceeds 8 MiB"))?;
        let scope = raw.scope.take();
        *raw = fragment.raw.clone();
        if scope.is_some() {
            raw.scope = scope;
        }
    } else {
        for child in &mut raw.children {
            expand_fragments(child, fragments, remaining)?;
        }
    }
    Ok(())
}
pub struct LuauFrontEnd {
    compiled: Sources,
    fragments: RefCell<bead_core::cache::Lru<(String, String), std::rc::Rc<Fragment>>>,
    fragment_budget: bead_core::cache::Budget,
    materialized: RefCell<Option<Materialized>>,
    kit: Vec<u8>,
    install: Vec<u8>,
    _compiled_charge: bead_core::cache::Charge,
    values_budget: bead_core::cache::Budget,
    vm_build_bytes: std::cell::Cell<usize>,
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
        let kit = compiler
            .compile(include_str!("../assets/kit.luau"))
            .map_err(|e| diagnostic(entry, e))?;
        let install = compiler
            .compile(INSTALL)
            .map_err(|e| diagnostic(entry, e))?;
        let bytes = compiled
            .iter()
            .map(|(p, b)| p.as_os_str().len() + b.capacity() + 96)
            .sum::<usize>()
            + kit.capacity()
            + install.capacity();
        let budget = bead_core::cache::Budget::new("compiled-markup", 8 * bead_core::cache::MIB);
        let charge = budget
            .reserve(bytes)
            .map_err(|e| Diagnostic::new(entry, 1, 1, e))?;
        Ok(Self {
            _compiled_charge: charge,
            values_budget: bead_core::cache::Budget::new(
                "markup-values",
                8 * bead_core::cache::MIB,
            ),
            fragments: RefCell::new(bead_core::cache::Lru::new(
                "markup-fragment-index",
                32 * 1024,
            )),
            fragment_budget: bead_core::cache::Budget::new("markup-fragments", 128 * 1024),
            compiled,
            vm_build_bytes: std::cell::Cell::new(0),
            materialized: RefCell::new(None),
            kit,
            install,
        })
    }
    pub(super) fn resource_bytes(&self) -> (usize, usize, usize) {
        (
            self._compiled_charge.bytes(),
            self.values_budget.used()
                + self.fragment_budget.used()
                + self.fragments.borrow().bytes(),
            self.vm_build_bytes.get(),
        )
    }
}
impl FrontEnd for LuauFrontEnd {
    fn build(
        &self,
        entry: &Path,
        resolve: &Resolve<'_>,
    ) -> Result<std::rc::Rc<RawNode>, Diagnostic> {
        if let Some((raw, reads, _)) = &*self.materialized.borrow()
            && reads
                .iter()
                .all(|(name, value)| resolve(name).is_ok_and(|next| next == *value))
        {
            return Ok(raw.clone());
        }
        let reads = RefCell::new(BTreeMap::new());
        let scopes = RefCell::new(Vec::<BTreeMap<String, Value>>::new());
        let fragments = RefCell::new(Vec::<std::rc::Rc<Fragment>>::new());
        let fragment_remaining = std::cell::Cell::new(8 * bead_core::cache::MIB);
        let upstream = resolve;
        let record = |name: &str| {
            let value = upstream(name)?;
            reads.borrow_mut().insert(name.to_owned(), value.clone());
            for scope in scopes.borrow_mut().iter_mut() {
                scope.insert(name.to_owned(), value.clone());
            }
            Ok(value)
        };
        let resolve: &Resolve<'_> = &record;
        let setup_profile = bead_core::profile::Span::new("markup_setup");
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
                let constructors = lua.create_table()?;
                for (name, kind) in Widget::constructors() {
                    constructors.set(name, lua.to_value(&kind)?)?;
                }
                let allowed = lua.create_table_from(
                    crate::layout::PROPERTIES.iter().map(|name| (*name, true)),
                )?;
                sdk.set(
                    "preview_value",
                    scope.create_function(|_, (key, value): (String, f64)| {
                        if resolve("bead.preview.key").map_err(mlua::Error::external)?
                            == Value::Text(key)
                            && let Value::Number(preview) =
                                resolve("bead.preview.value").map_err(mlua::Error::external)?
                        {
                            return Ok(preview);
                        }
                        Ok(value)
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
                sdk.set(
                    "memo",
                    scope.create_function(
                        |lua, (name, inputs, render): (String, mlua::Value, Function)| {
                            if name.is_empty() || name.len() > 256 {
                                return Err(mlua::Error::runtime("memo name exceeds bound"));
                            }
                            let json: serde_json::Value = lua.from_value(inputs.clone())?;
                            let inputs_key =
                                serde_json::to_string(&json).map_err(mlua::Error::external)?;
                            if inputs_key.len() > 16 * 1024 {
                                return render.call::<mlua::Value>(inputs);
                            }
                            let key = (name, inputs_key);
                            let cached = self.fragments.borrow().get(&key).cloned();
                            let fragment = if let Some(cached) = cached.filter(|cached| {
                                cached.reads.iter().all(|(name, value)| {
                                    resolve(name).is_ok_and(|next| next == *value)
                                })
                            }) {
                                cached
                            } else {
                                scopes.borrow_mut().push(BTreeMap::new());
                                let result = render.call::<mlua::Value>(inputs);
                                let observed = scopes.borrow_mut().pop().expect("memo read scope");
                                let mut raw: RawNode = lua.from_value(result?)?;
                                let mut remaining = 8 * bead_core::cache::MIB;
                                expand_fragments(&mut raw, &fragments.borrow(), &mut remaining)?;
                                // Include the Fragment/Rc header as well as owned data.
                                let bytes = 128
                                    + crate::layout::raw_bytes(&raw)
                                    + observed
                                        .iter()
                                        .map(|(k, v)| {
                                            k.capacity() + 64 + bead_core::cache::value_bytes(v)
                                        })
                                        .sum::<usize>();
                                self.fragments.borrow_mut().remove(&key);
                                let charge = loop {
                                    match self.fragment_budget.reserve(bytes) {
                                        Ok(charge) => break Some(charge),
                                        Err(_) => {
                                            if self.fragments.borrow_mut().pop_lru().is_none() {
                                                break None;
                                            }
                                        }
                                    }
                                };
                                let fragment = std::rc::Rc::new(Fragment {
                                    bytes,
                                    raw,
                                    reads: observed,
                                    _charge: charge,
                                });
                                if fragment._charge.is_some() {
                                    let metadata = key.0.capacity() + key.1.capacity() + 128;
                                    let _ = self.fragments.borrow_mut().insert(
                                        key,
                                        fragment.clone(),
                                        metadata,
                                    );
                                }
                                fragment
                            };
                            fragment_remaining.set(
                                fragment_remaining
                                    .get()
                                    .checked_sub(fragment.bytes)
                                    .ok_or_else(|| {
                                        mlua::Error::runtime("memo work exceeds 8 MiB")
                                    })?,
                            );
                            let index = fragments.borrow().len();
                            fragments.borrow_mut().push(fragment);
                            let marker = lua.create_table()?;
                            marker.set("kind", "group")?;
                            let props = lua.create_table()?;
                            props.set("__memo", index)?;
                            marker.set("props", props)?;
                            let children = lua.create_table()?;
                            children.set_metatable(Some(lua.array_metatable()))?;
                            marker.set("children", children)?;
                            marker.set("source", lua.to_value(&source(lua, entry))?)?;
                            Ok(mlua::Value::Table(marker))
                        },
                    )?,
                )?;
                sdk.set("tr", crate::i18n::lua_function(&lua)?)?;
                sdk.set(
                    "model_count",
                    scope.create_function(|_, model: Table| {
                        let name: String = model.get("__bind")?;
                        match resolve(&name).map_err(mlua::Error::external)? {
                            Value::List(rows) => Ok(rows.len()),
                            _ => Err(mlua::Error::runtime(format!("model {name} must be a list"))),
                        }
                    })?,
                )?;
                let install: Function = lua
                    .load(self.install.as_slice())
                    .set_name("@bead.ui")
                    .eval()?;
                install.call::<()>((sdk.clone(), constructors, allowed, lua.array_metatable()))?;
                let group: Function = sdk.get("Group")?;
                sdk.set(
                    "Repeat",
                    scope.create_function(move |lua, props: Table| {
                        let first = props.get::<Option<usize>>("first")?.unwrap_or(0);
                        let count = props.get::<Option<usize>>("count")?.unwrap_or(2048);
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
                            if i < first || i >= first.saturating_add(count) {
                                continue;
                            }
                            let row: Table = render.call(lua.to_value(item)?)?;
                            let prefix = format!(
                                "{id}.k{}.",
                                key.bytes().map(|b| format!("{b:02x}")).collect::<String>()
                            );
                            row.set("scope", prefix)?;
                            children.set(children.raw_len() + 1, row)?;
                        }
                        let clean = lua.create_table()?;
                        clean.set("id", id)?;
                        let origin = source(lua, entry);
                        let node: Table = group.call(clean)?;
                        node.set("source", lua.to_value(&origin)?)?;
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
                let mut tokens: serde_json::Value =
                    serde_json::from_str(include_str!("../assets/size-scale.json"))
                        .map_err(mlua::Error::external)?;
                // The owner's `interfaceGradients` choice, read while the scene is built (the host reloads views on a change).
                tokens["gradients"] = crate::text_style::gradients().into();
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
                drop(setup_profile);
                let eval_profile = bead_core::profile::Span::new("markup_eval");
                let node: mlua::Value = lua
                    .load(bytes)
                    .set_name(format!("@{}", entry.display()))
                    .eval()?;
                drop(eval_profile);
                let _serde_profile = bead_core::profile::Span::new("markup_serde");
                let mut raw: RawNode = lua.from_value(node)?;
                let mut remaining = (8 * bead_core::cache::MIB)
                    .checked_sub(crate::layout::raw_bytes(&raw))
                    .ok_or_else(|| mlua::Error::runtime("markup exceeds 8 MiB"))?;
                expand_fragments(&mut raw, &fragments.borrow(), &mut remaining)?;
                Ok(raw)
            })
        })();
        // The markup sandbox is temporary; this is its end-of-build allocation,
        // not a retained VM or an allocator high-water estimate.
        self.vm_build_bytes.set(lua.used_memory());
        let result = result
            .map(std::rc::Rc::new)
            .map_err(|error| diagnostic(entry, error));
        if let Ok(raw) = &result {
            let reads = reads.into_inner();
            let bytes = crate::layout::raw_bytes(raw)
                + reads
                    .iter()
                    .map(|(k, v)| k.capacity() + 64 + bead_core::cache::value_bytes(v))
                    .sum::<usize>();
            *self.materialized.borrow_mut() = None;
            if let Ok(charge) = self.values_budget.reserve(bytes) {
                *self.materialized.borrow_mut() = Some((raw.clone(), reads, charge));
            }
        }
        result
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
