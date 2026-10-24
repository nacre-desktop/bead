// SPDX-License-Identifier: MIT OR Apache-2.0
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
    cache: RefCell<bead_core::cache::Lru<String, Cached>>,
}
impl Script {
    pub fn validate_source(bytes: &[u8], path: &Path) -> Result<(), Diagnostic> {
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
        let name = path.to_string_lossy();
        let exports = (|| -> mlua::Result<Table> {
            lua.set_memory_limit(8 * 1024 * 1024)?;
            lua.globals().set("tr", crate::i18n::lua_function(&lua)?)?;
            lua.sandbox(true)?;
            lua.load(bytes).set_name(format!("@{name}")).eval()
        })()
        .map_err(|e| lua_error(path, e))?;
        bead_core::profile::mark("vm_ready");
        Ok(Self {
            source: bytes.to_vec(),
            lua,
            exports,
            path: path.to_path_buf(),
            deadline,
            remaining: Cell::new(Duration::from_millis(50)),
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
        // Failed actions cannot mutate even lexical state in the current script VM.
        let candidate = Self::load(&self.source, &self.path)?;
        let result = candidate.execute_action(name, stores, arg);
        bead_core::profile::mark("luau_handler_ready");
        result
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
            .to_value(stores.values())
            .map_err(|e| lua_error(&self.path, e))?;
        let table = match table {
            mlua::Value::Table(table) => table,
            _ => return Err(self.error("invalid state table")),
        };
        self.bounded(|| action.call::<()>((table.clone(), self.lua.to_value(&arg)?)))
            .map_err(|e| lua_error(&self.path, e))?;
        let changes: BTreeMap<String, Value> = self
            .lua
            .from_value_with(
                mlua::Value::Table(table),
                mlua::serde::DeserializeOptions::new().encode_empty_tables_as_array(true),
            )
            .map_err(|e| lua_error(&self.path, e))?;
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
            lua.to_value(&value)
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
