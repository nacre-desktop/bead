// SPDX-License-Identifier: MIT
use crate::Diagnostic;
use bead_core::{Stores, Value};
use mlua::{Function, Lua, LuaSerdeExt, Table, VmState};
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    path::{Path, PathBuf},
    rc::Rc,
    time::{Duration, Instant},
};
// Luau numbers are doubles. Opaque JSON integers outside their exact range stay
// serializable scalars; authored arithmetic still receives ordinary safe numbers.
#[derive(serde::Serialize)]
#[serde(transparent)]
struct OpaqueInteger(serde_json::Number);
impl mlua::UserData for OpaqueInteger {}
fn needs_exact_integer(json: &serde_json::Value) -> bool {
    match json {
        serde_json::Value::Number(number) => {
            number.as_u64().is_some_and(|n| n > 9_007_199_254_740_991)
                || number.as_i64().is_some_and(|n| n < -9_007_199_254_740_991)
        }
        serde_json::Value::Array(items) => items.iter().any(needs_exact_integer),
        serde_json::Value::Object(items) => items.values().any(needs_exact_integer),
        _ => false,
    }
}
fn json_array(lua: &Lua, items: &[serde_json::Value]) -> mlua::Result<mlua::Value> {
    // mlua's bulk serializer holds the VM lock once. Keep ordinary hot lists
    // on that path; only opaque wide integers need per-value userdata.
    if !items.iter().any(needs_exact_integer) {
        return lua.to_value(items);
    }
    let table = lua.create_table_with_capacity(items.len(), 0)?;
    table.set_metatable(Some(lua.array_metatable()))?;
    for (index, item) in items.iter().enumerate() {
        table.raw_set(index + 1, json_value(lua, item)?)?;
    }
    Ok(mlua::Value::Table(table))
}
pub(crate) fn json_value(lua: &Lua, json: &serde_json::Value) -> mlua::Result<mlua::Value> {
    if !needs_exact_integer(json) {
        return lua.to_value(json);
    }
    match json {
        serde_json::Value::Array(items) => json_array(lua, items),
        serde_json::Value::Object(items) => {
            let table = lua.create_table_with_capacity(0, items.len())?;
            for (key, item) in items {
                table.raw_set(key.as_str(), json_value(lua, item)?)?;
            }
            Ok(mlua::Value::Table(table))
        }
        serde_json::Value::Number(number)
            if number.as_u64().is_some_and(|n| n > 9_007_199_254_740_991)
                || number.as_i64().is_some_and(|n| n < -9_007_199_254_740_991) =>
        {
            Ok(mlua::Value::UserData(
                lua.create_ser_userdata(OpaqueInteger(number.clone()))?,
            ))
        }
        _ => lua.to_value(json),
    }
}
pub(crate) fn value_to_lua(lua: &Lua, value: &Value) -> mlua::Result<mlua::Value> {
    match value {
        Value::List(items) => json_array(lua, items),
        _ => lua.to_value(value),
    }
}
struct Cached {
    value: Value,
    reads: BTreeMap<String, (u64, Value)>,
}
pub struct Script {
    source: Vec<u8>,
    lua: Lua,
    exports: Table,
    path: PathBuf,
    deadline: Rc<Cell<Instant>>,
    remaining: Cell<Duration>,
    builtin_require: Function,
    builtin_math: Table,
    builtin_random: Function,
    builtin_randomseed: Function,
    action_taint: Option<Rc<Cell<bool>>>,
    action_vm: RefCell<Option<Box<Script>>>,
    cache: RefCell<bead_core::cache::Lru<String, Cached>>,
}

fn behavior_exports(
    lua: &Lua,
    bytes: &[u8],
    name: &str,
    tainted: Option<Rc<Cell<bool>>>,
    builtin_require: &Function,
    builtin_math: &Table,
) -> mlua::Result<Table> {
    let globals = lua.globals();
    globals.set("tr", crate::i18n::lua_function(lua)?)?;
    let modules = lua.create_table()?;
    let builtin_require = builtin_require.clone();
    let require_taint = tainted.clone();
    globals.set(
        "require",
        lua.create_function(move |lua, module_name: String| {
            let Some(module) = module_name.strip_prefix("bead.widgets.") else {
                if let Some(tainted) = &require_taint {
                    tainted.set(true);
                }
                return builtin_require.call::<mlua::Value>(module_name);
            };
            let cached: mlua::Value = modules.get(module_name.as_str())?;
            if !cached.is_nil() {
                return Ok(cached);
            }
            let source = bead_widgets::behavior_source(module).ok_or_else(|| {
                mlua::Error::runtime(format!("{module_name} is not a behavior module"))
            })?;
            let code = crate::frontend::bytecode(source.as_bytes())?;
            let value: mlua::Value = lua
                .load(&*code)
                .set_name(format!("@{module_name}"))
                .eval()?;
            modules.set(module_name, value.clone())?;
            Ok(value)
        })?,
    )?;
    globals.set(
        "copy_value",
        lua.create_function(|lua, value: mlua::Value| {
            let json: serde_json::Value = lua.from_value_with(
                value,
                mlua::serde::DeserializeOptions::new().encode_empty_tables_as_array(false),
            )?;
            json_value(lua, &json)
        })?,
    )?;
    if let Some(tainted) = tainted {
        for key in ["random", "randomseed"] {
            let original: Function = builtin_math.get(key)?;
            let tainted = tainted.clone();
            builtin_math.set(
                key,
                lua.create_function(move |_, args: mlua::MultiValue| {
                    tainted.set(true);
                    original.call::<mlua::MultiValue>(args)
                })?,
            )?;
        }
    }
    lua.sandbox(true)?;
    match crate::frontend::bytecode(bytes) {
        Ok(code) => lua.load(&*code).set_name(format!("@{name}")).eval(),
        Err(_) => lua.load(bytes).set_name(format!("@{name}")).eval(),
    }
}
impl Script {
    pub fn validate_source(bytes: &[u8], path: &Path) -> Result<(), Diagnostic> {
        // B0202: valid source compiles; cached bytecode skips a VM per file. Errors keep the
        // loader's message (chunk name and line).
        if crate::frontend::bytecode(bytes).is_ok() {
            return Ok(());
        }
        let lua = Lua::new();
        lua.set_memory_limit(8 * 1024 * 1024)
            .map_err(|e| lua_error(path, e))?;
        lua.load(bytes)
            .set_name(format!("@{}", path.display()))
            .into_function()
            .map(|_| ())
            .map_err(|e| lua_error(path, e))
    }

    pub fn load(bytes: &[u8], path: &Path) -> Result<Self, Diagnostic> {
        Self::load_inner(bytes, path, false)
    }

    fn load_action(bytes: &[u8], path: &Path) -> Result<Self, Diagnostic> {
        Self::load_inner(bytes, path, true)
    }

    fn load_inner(bytes: &[u8], path: &Path, action: bool) -> Result<Self, Diagnostic> {
        let lua = Lua::new();
        let deadline = Rc::new(Cell::new(Instant::now() + Duration::from_millis(50)));
        let limit = deadline.clone();
        lua.set_interrupt(move |_| {
            if Instant::now() > limit.get() {
                Err(mlua::Error::runtime(
                    "script exceeded 50 ms execution limit",
                ))
            } else {
                Ok(VmState::Continue)
            }
        });
        lua.set_memory_limit(8 * 1024 * 1024)
            .map_err(|e| lua_error(path, e))?;
        let builtin_require: Function = lua
            .globals()
            .get("require")
            .map_err(|e| lua_error(path, e))?;
        let builtin_math: Table = lua.globals().get("math").map_err(|e| lua_error(path, e))?;
        let builtin_random: Function =
            builtin_math.get("random").map_err(|e| lua_error(path, e))?;
        let builtin_randomseed: Function = builtin_math
            .get("randomseed")
            .map_err(|e| lua_error(path, e))?;
        let tainted = action.then(|| Rc::new(Cell::new(false)));
        let name = path.to_string_lossy();
        let exports = behavior_exports(
            &lua,
            bytes,
            &name,
            tainted.clone(),
            &builtin_require,
            &builtin_math,
        )
        .map_err(|e| lua_error(path, e))?;
        bead_core::profile::mark("vm_ready");
        Ok(Self {
            source: bytes.to_vec(),
            lua,
            exports,
            path: path.to_path_buf(),
            deadline,
            remaining: Cell::new(Duration::from_millis(50)),
            builtin_require,
            builtin_math,
            builtin_random,
            builtin_randomseed,
            action_taint: tainted,
            action_vm: RefCell::new(None),
            cache: RefCell::new(bead_core::cache::Lru::new(
                "computed-values",
                8 * bead_core::cache::MIB,
            )),
        })
    }
    pub fn begin(&self) {
        self.remaining.set(Duration::from_millis(50));
        self.deadline.set(Instant::now() + self.remaining.get());
    }
    /// One cumulative behaviour-VM allowance per transaction. Native layout and
    /// the separate markup VM cannot spend this VM's allowance while it is idle.
    fn bounded<T>(&self, call: impl FnOnce() -> mlua::Result<T>) -> mlua::Result<T> {
        let remaining = self.remaining.get();
        if remaining.is_zero() {
            return Err(mlua::Error::runtime(
                "script exceeded 50 ms execution limit",
            ));
        }
        let began = Instant::now();
        self.deadline.set(began + remaining);
        let result = call();
        let spent = began.elapsed();
        self.remaining.set(remaining.saturating_sub(spent));
        if result.is_ok() && spent >= remaining {
            Err(mlua::Error::runtime(
                "script exceeded 50 ms execution limit",
            ))
        } else {
            result
        }
    }
    pub fn memory_bytes(&self) -> usize {
        self.lua.used_memory()
            + self
                .action_vm
                .borrow()
                .as_ref()
                .map_or(0, |action| action.lua.used_memory())
    }
    pub fn binding_count(&self) -> usize {
        self.cache.borrow().len()
    }
    pub fn error(&self, message: impl ToString) -> Diagnostic {
        Diagnostic::new(&self.path, 1, 1, message)
    }
    pub fn schema(&self) -> Result<BTreeMap<String, Value>, Diagnostic> {
        self.lua
            .from_value_with(
                self.exports
                    .get("stores")
                    .map_err(|e| lua_error(&self.path, e))?,
                mlua::serde::DeserializeOptions::new().encode_empty_tables_as_array(true),
            )
            .map_err(|e| lua_error(&self.path, e))
    }
    pub fn validate_action(&self, name: &str) -> Result<(), Diagnostic> {
        if [
            "notifications.details",
            "notifications.card",
            "notifications.dismiss",
            "notifications.forget",
            "notifications.action",
            "notifications.dnd",
            "notifications.clear",
            "notifications.clear_popups",
            "notifications.osd",
            // Host-handled dashboard control (any page may bind it once the daemon advertises it).
            "dashboard.do",
            "services.audio",
            "services.audio.volume",
            "services.tray",
            "services.menu",
            "services.menu.open",
            "services.menu.back",
            "services.tray.secondary",
            "bar.title",
            "view.open",
            "status.pill.open",
            "backend.workspace.switch",
            "backend.window.focus",
            "backend.window.close",
            "backend.windowTrash",
            "backend.scratchpad",
            "backend.windowCapture",
            "backend.monitorMirroring",
        ]
        .contains(&name)
        {
            return Ok(());
        }

        let actions: Table = self
            .exports
            .get("actions")
            .map_err(|e| lua_error(&self.path, e))?;
        self.bounded(|| actions.get::<Function>(name))
            .map(|_| ())
            .map_err(|e| lua_error(&self.path, e))
    }
    pub fn action(&self, name: &str, stores: &Stores, arg: Value) -> Result<Stores, Diagnostic> {
        // Failed actions discard their candidate VM so hidden state rolls back.
        let pooled = self.action_vm.borrow_mut().take();
        let reused = pooled.is_some();
        let mut candidate = match pooled {
            Some(candidate) => *candidate,
            None => Self::load_action(&self.source, &self.path)?,
        };
        if reused {
            candidate.prepare_action(&self.source)?;
        }
        let result = candidate.execute_action(name, stores, arg);
        let reusable = result.is_ok()
            && !candidate
                .action_taint
                .as_ref()
                .is_some_and(|tainted| tainted.get());
        if reusable && candidate.reset_action().is_ok() {
            candidate.source = Vec::new();
            *self.action_vm.borrow_mut() = Some(Box::new(candidate));
        }
        bead_core::profile::mark("luau_handler_ready");
        result
    }
    fn prepare_action(&mut self, source: &[u8]) -> Result<(), Diagnostic> {
        self.deadline
            .set(Instant::now() + Duration::from_millis(50));
        let tainted = Rc::new(Cell::new(false));
        let name = self.path.to_string_lossy();
        let exports = behavior_exports(
            &self.lua,
            source,
            &name,
            Some(tainted.clone()),
            &self.builtin_require,
            &self.builtin_math,
        )
        .map_err(|e| lua_error(&self.path, e))?;
        self.exports = exports;
        self.action_taint = Some(tainted);
        Ok(())
    }
    fn reset_action(&mut self) -> mlua::Result<()> {
        self.exports = self.lua.create_table()?;
        self.action_taint = None;
        self.lua.sandbox(false)?;
        let globals = self.lua.globals();
        globals.set("require", self.builtin_require.clone())?;
        self.builtin_math
            .set("random", self.builtin_random.clone())?;
        self.builtin_math
            .set("randomseed", self.builtin_randomseed.clone())?;
        globals.set("tr", mlua::Value::Nil)?;
        globals.set("copy_value", mlua::Value::Nil)?;
        // A second full cycle also frees objects released by callback finalizers.
        self.lua.gc_collect()?;
        self.lua.gc_collect()
    }
    fn execute_action(
        &self,
        name: &str,
        stores: &Stores,
        arg: Value,
    ) -> Result<Stores, Diagnostic> {
        self.begin();
        let actions: Table = self
            .exports
            .get("actions")
            .map_err(|e| lua_error(&self.path, e))?;
        let action: Function = actions.get(name).map_err(|e| lua_error(&self.path, e))?;
        let table = self
            .lua
            .create_table()
            .map_err(|e| lua_error(&self.path, e))?;
        for (key, value) in stores.values() {
            let value = value_to_lua(&self.lua, value).map_err(|e| lua_error(&self.path, e))?;
            table
                .set(key.as_str(), value)
                .map_err(|e| lua_error(&self.path, e))?;
        }
        self.bounded(|| action.call::<()>((table.clone(), value_to_lua(&self.lua, &arg)?)))
            .map_err(|e| lua_error(&self.path, e))?;
        // Decode the outer list using its durable schema. Inferring the entire
        // untagged Value tree as JSON would turn opaque empty objects into arrays.
        let mut changes = BTreeMap::new();
        for pair in table.pairs::<String, mlua::Value>() {
            let (id, value) = pair.map_err(|e| lua_error(&self.path, e))?;
            let parsed = if matches!(stores.get(&id), Some(Value::List(_))) {
                let items = self
                    .lua
                    .from_value_with::<Vec<serde_json::Value>>(
                        value,
                        mlua::serde::DeserializeOptions::new().encode_empty_tables_as_array(false),
                    )
                    .map_err(|e| lua_error(&self.path, e))?;
                Value::list(items)
            } else {
                self.lua
                    .from_value(value)
                    .map_err(|e| lua_error(&self.path, e))?
            };
            changes.insert(id, parsed);
        }
        if changes.keys().ne(stores.values().keys()) {
            return Err(self.error("actions cannot create or remove durable stores"));
        }
        for (id, value) in &changes {
            if id.starts_with("bead.") && stores.get(id) != Some(value) {
                return Err(self.error(format!("{id} is Rust-owned and read-only to actions")));
            }
        }
        let mut next = stores.clone();
        next.commit(changes).map_err(|e| self.error(e))?;
        Ok(next)
    }
    pub fn validate_computed(&self, stores: &Stores) -> Result<(), Diagnostic> {
        self.begin();
        let computed: Table = self
            .exports
            .get("computed")
            .map_err(|e| lua_error(&self.path, e))?;
        for pair in computed.pairs::<String, Function>() {
            let (name, _) = pair.map_err(|e| lua_error(&self.path, e))?;
            self.resolve(&name, stores)?;
        }
        Ok(())
    }
    pub(super) fn store_reads(&self, name: &str, stores: &Stores) -> Vec<String> {
        if stores.get(name).is_some() {
            return vec![name.into()];
        }
        self.cache
            .borrow()
            .get(name)
            .map(|cached| cached.reads.keys().cloned().collect())
            .unwrap_or_default()
    }
    pub fn resolve(&self, name: &str, stores: &Stores) -> Result<Value, Diagnostic> {
        if let Some(value) = stores.get(name) {
            return Ok(value.clone());
        }
        if let Some(cached) = self.cache.borrow().get(name)
            && cached.reads.iter().all(|(id, version)| {
                stores.version(id) == Some(version.0) && stores.get(id) == Some(&version.1)
            })
        {
            return Ok(cached.value.clone());
        }
        let _profile = bead_core::profile::Span::new("computed");
        let reads = Rc::new(RefCell::new(BTreeMap::new()));
        let value = self
            .bounded(|| {
                evaluate(
                    &self.lua,
                    &self.exports,
                    stores,
                    name,
                    Rc::new(RefCell::new(Vec::new())),
                    reads.clone(),
                )
            })
            .map_err(|e| lua_error(&self.path, e))?;
        let bytes = name.len()
            + 128
            + bead_core::cache::value_bytes(&value)
            + reads
                .borrow()
                .iter()
                .map(|(k, (_, v))| k.capacity() + 64 + bead_core::cache::value_bytes(v))
                .sum::<usize>();
        let _ = self.cache.borrow_mut().insert(
            name.into(),
            Cached {
                value: value.clone(),
                reads: reads.borrow().clone(),
            },
            bytes,
        );
        Ok(value)
    }
}
fn evaluate(
    lua: &Lua,
    exports: &Table,
    stores: &Stores,
    name: &str,
    stack: Rc<RefCell<Vec<String>>>,
    reads: Rc<RefCell<BTreeMap<String, (u64, Value)>>>,
) -> mlua::Result<Value> {
    if let Some(value) = stores.get(name) {
        reads.borrow_mut().insert(
            name.into(),
            (stores.version(name).unwrap_or(0), value.clone()),
        );
        return Ok(value.clone());
    }
    if stack.borrow().len() >= 32 || stack.borrow().iter().any(|id| id == name) {
        return Err(mlua::Error::runtime(format!(
            "binding cycle/depth at {name}"
        )));
    }
    let computed: Table = exports.get("computed")?;
    let function: Function = computed
        .get(name)
        .map_err(|_| mlua::Error::runtime(format!("unknown binding {name}")))?;
    stack.borrow_mut().push(name.into());
    let proxy = lua.create_table()?;
    let meta = lua.create_table()?;
    let (exports, stores, child_stack, child_reads) = (
        exports.clone(),
        stores.clone(),
        stack.clone(),
        reads.clone(),
    );
    meta.set(
        "__index",
        lua.create_function(move |lua, (_, key): (Table, String)| {
            let value = evaluate(
                lua,
                &exports,
                &stores,
                &key,
                child_stack.clone(),
                child_reads.clone(),
            )?;
            value_to_lua(lua, &value)
        })?,
    )?;
    meta.set(
        "__newindex",
        lua.create_function(
            |_, (_, _, _): (Table, mlua::Value, mlua::Value)| -> mlua::Result<()> {
                Err(mlua::Error::runtime("bindings cannot mutate state"))
            },
        )?,
    )?;
    proxy.set_metatable(Some(meta))?;
    let result = function.call::<mlua::Value>(proxy).and_then(|value| {
        lua.from_value_with(
            value,
            mlua::serde::DeserializeOptions::new().encode_empty_tables_as_array(true),
        )
    });
    stack.borrow_mut().pop();
    result
}
fn lua_error(path: &Path, error: mlua::Error) -> Diagnostic {
    let message = error.to_string();
    let marker = format!("{}:", path.to_string_lossy());
    let line = message
        .split(&marker)
        .nth(1)
        .and_then(|rest| rest.split(':').next())
        .and_then(|v| v.parse().ok())
        .unwrap_or(1);
    Diagnostic::new(path, line, 1, message)
}

#[cfg(test)]
mod budget_tests {
    use super::*;
    #[test]
    fn native_work_between_bindings_does_not_consume_luau_allowance() {
        let script = Script::load(
            b"return {stores={},actions={},computed={answer=function(_) return 42 end}}",
            Path::new("budget.luau"),
        )
        .unwrap();
        script.begin();
        std::thread::sleep(Duration::from_millis(70));
        assert_eq!(
            script.resolve("answer", &Stores::default()).unwrap(),
            Value::Number(42.)
        );
    }
    #[test]
    fn behavior_imports_share_pure_widget_modules_without_exposing_view_modules() {
        let script = Script::load(
            br#"local geometry=require("bead.widgets.drag-geometry")
assert(geometry==require("bead.widgets.drag-geometry"))
local slot=geometry.desktop_landing({},2,3,4,5,100,100,8)
return {stores={x=slot.x,y=slot.y},actions={},computed={}}"#,
            Path::new("behavior.luau"),
        )
        .unwrap();
        assert_eq!(script.schema().unwrap().get("x"), Some(&Value::Number(2.)));
        assert_eq!(script.schema().unwrap().get("y"), Some(&Value::Number(3.)));
        assert!(
            Script::load(
                br#"require("bead.widgets.measure"); return {stores={},actions={},computed={}}"#,
                Path::new("behavior.luau"),
            )
            .is_err()
        );
    }

    #[test]
    fn separate_small_evaluations_share_the_same_fifty_millisecond_allowance() {
        let script=Script::load(b"return {stores={},actions={},computed={answer=function(s) local n=s.n; for i=1,10000 do n+=i end; return n end}}",Path::new("budget.luau")).unwrap();
        let mut stores = Stores::default()
            .reconcile(&BTreeMap::from([("n".into(), Value::Number(0.))]))
            .unwrap();
        script.begin();
        let mut exhausted = false;
        for i in 1..10000 {
            stores
                .commit(BTreeMap::from([("n".into(), Value::Number(f64::from(i)))]))
                .unwrap();
            if let Err(e) = script.resolve("answer", &stores) {
                assert!(e.message.contains("50 ms"));
                exhausted = true;
                break;
            }
        }
        assert!(
            exhausted,
            "per-call deadline resets must not evade the cumulative budget"
        );
    }
}

#[cfg(test)]
mod action_isolation_tests {
    use super::*;

    #[test]
    fn actions_start_with_fresh_module_and_sandbox_globals_after_success_or_failure() {
        let script = Script::load(
            br#"
                local hidden = 0
                return {
                    stores = { value = 0 },
                    actions = {
                        mutate = function(stores)
                            hidden += 1
                            leaked = (leaked or 0) + 1
                            stores.value = hidden + leaked
                        end,
                        fail = function(stores)
                            hidden += 1
                            leaked = (leaked or 0) + 1
                            stores.value = 99
                            error("discard this action VM")
                        end,
                    },
                    computed = {},
                }
            "#,
            Path::new("action-isolation.luau"),
        )
        .unwrap();
        let stores = Stores::default()
            .reconcile(&script.schema().unwrap())
            .unwrap();

        assert_eq!(
            script
                .action("mutate", &stores, Value::Bool(true))
                .unwrap()
                .get("value"),
            Some(&Value::Number(2.))
        );
        assert!(script.action("fail", &stores, Value::Bool(true)).is_err());
        assert!(script.action_vm.borrow().is_none());
        assert_eq!(
            script
                .action("mutate", &stores, Value::Bool(true))
                .unwrap()
                .get("value"),
            Some(&Value::Number(2.))
        );
        assert!(script.action_vm.borrow().is_some());
        std::thread::sleep(Duration::from_millis(70));
        assert_eq!(
            script
                .action("mutate", &stores, Value::Bool(true))
                .unwrap()
                .get("value"),
            Some(&Value::Number(2.))
        );
    }

    #[test]
    fn separately_loaded_plugins_do_not_share_action_globals() {
        let source = br#"local hidden=0; return {stores={value=0},actions={mutate=function(s) hidden+=1; leaked=(leaked or 0)+1; s.value=hidden+leaked end},computed={}}"#;
        let first = Script::load(source, Path::new("first.luau")).unwrap();
        let second = Script::load(source, Path::new("second.luau")).unwrap();
        let stores = Stores::default()
            .reconcile(&first.schema().unwrap())
            .unwrap();

        assert_eq!(
            first
                .action("mutate", &stores, Value::Bool(true))
                .unwrap()
                .get("value"),
            Some(&Value::Number(2.))
        );
        assert_eq!(
            second
                .action("mutate", &stores, Value::Bool(true))
                .unwrap()
                .get("value"),
            Some(&Value::Number(2.))
        );
    }

    #[test]
    fn successful_actions_do_not_retain_embedded_module_tables() {
        let script = Script::load(
            br#"local module=require("bead.widgets.drag-geometry"); return {
                stores={value=0}, actions={mutate=function(s)
                    module.counter=(module.counter or 0)+1
                    module.payload=string.rep("x",5*1024*1024)
                    s.value=module.counter
                end}, computed={}
            }"#,
            Path::new("module-isolation.luau"),
        )
        .unwrap();
        let stores = Stores::default()
            .reconcile(&script.schema().unwrap())
            .unwrap();

        let initial_memory = script.memory_bytes();
        for _ in 0..2 {
            assert_eq!(
                script
                    .action("mutate", &stores, Value::Bool(true))
                    .unwrap()
                    .get("value"),
                Some(&Value::Number(1.))
            );
            assert!(script.memory_bytes() < initial_memory + bead_core::cache::MIB);
        }
    }

    #[test]
    fn fallback_require_and_caught_rng_calls_discard_the_action_vm() {
        let script = Script::load(
            br#"return {stores={value=0},actions={
                fallback=function(s) pcall(require,"b0254_missing_module") end,
                random=function(s) math.randomseed(123); math.random() end,
                random_caught=function(s) pcall(math.randomseed,123); pcall(math.random) end
            },computed={}}"#,
            Path::new("tainted-action.luau"),
        )
        .unwrap();
        let stores = Stores::default()
            .reconcile(&script.schema().unwrap())
            .unwrap();

        script
            .action("fallback", &stores, Value::Bool(true))
            .unwrap();
        assert!(script.action_vm.borrow().is_none());
        script.action("random", &stores, Value::Bool(true)).unwrap();
        assert!(script.action_vm.borrow().is_none());
        script
            .action("random_caught", &stores, Value::Bool(true))
            .unwrap();
        assert!(script.action_vm.borrow().is_none());
    }

    #[test]
    fn action_time_and_memory_failures_discard_a_warmed_vm() {
        let script = Script::load(
            br#"return {stores={value=0},actions={
                noop=function(s) end,
                memory=function(s) s.value=string.rep("x",16777216) end,
                timeout=function(s) while true do end end
            },computed={}}"#,
            Path::new("action-limits.luau"),
        )
        .unwrap();
        let stores = Stores::default()
            .reconcile(&script.schema().unwrap())
            .unwrap();

        script.action("noop", &stores, Value::Bool(true)).unwrap();
        assert!(script.action_vm.borrow().is_some());
        let memory_error = script
            .action("memory", &stores, Value::Bool(true))
            .unwrap_err();
        assert!(memory_error.message.to_lowercase().contains("memory"));
        assert!(script.action_vm.borrow().is_none());
        script.action("noop", &stores, Value::Bool(true)).unwrap();
        assert!(script.action_vm.borrow().is_some());
        let timeout_error = script
            .action("timeout", &stores, Value::Bool(true))
            .unwrap_err();
        assert!(timeout_error.message.contains("50 ms"));
        assert!(script.action_vm.borrow().is_none());
        script.action("noop", &stores, Value::Bool(true)).unwrap();
        assert!(script.action_vm.borrow().is_some());
        let warmed = script.memory_bytes();
        for _ in 0..100 {
            script.action("noop", &stores, Value::Bool(true)).unwrap();
        }
        assert!(script.memory_bytes() <= warmed + 4096);
        assert!(script.memory_bytes() <= 16 * bead_core::cache::MIB);
    }
}
