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
    sync::{Arc, Mutex, Weak},
    time::{Duration, Instant},
};
#[cfg(test)]
thread_local! {
    static BUILD_CALLS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

#[cfg(test)]
pub(super) fn reset_build_calls() {
    BUILD_CALLS.set(0);
}

#[cfg(test)]
pub(super) fn build_calls() -> usize {
    BUILD_CALLS.get()
}
pub type Sources = BTreeMap<PathBuf, Vec<u8>>;
pub type Resolve<'a> = dyn Fn(&str) -> Result<Value, Diagnostic> + 'a;

/// Fixed embedded compiler inputs shared while at least one frontend is alive.
struct CompiledPrelude {
    kit: Vec<u8>,
    install: Vec<u8>,
    host: Vec<u8>,
    _charge: bead_core::cache::Charge,
}
static COMPILED_PRELUDE: Mutex<Weak<CompiledPrelude>> = Mutex::new(Weak::new());

fn compiled_prelude(
    compiler: &mlua::chunk::Compiler,
    entry: &Path,
) -> Result<Arc<CompiledPrelude>, Diagnostic> {
    let mut slot = COMPILED_PRELUDE.lock().unwrap_or_else(|p| p.into_inner());
    if let Some(prelude) = slot.upgrade() {
        return Ok(prelude);
    }
    let kit = compiler
        .compile(include_str!("../assets/kit.luau"))
        .map_err(|e| diagnostic(entry, e))?;
    let install = compiler
        .compile(INSTALL)
        .map_err(|e| diagnostic(entry, e))?;
    let host = compiler
        .compile(include_str!("../assets/host.luau"))
        .map_err(|e| diagnostic(entry, e))?;
    let bytes = kit.capacity() + install.capacity() + host.capacity();
    let budget =
        bead_core::cache::Budget::new("compiled-markup-prelude", 8 * bead_core::cache::MIB);
    let charge = budget
        .reserve(bytes)
        .map_err(|e| Diagnostic::new(entry, 1, 1, e))?;
    let prelude = Arc::new(CompiledPrelude {
        kit,
        install,
        host,
        _charge: charge,
    });
    *slot = Arc::downgrade(&prelude);
    Ok(prelude)
}

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
    ui.animate = function(key, target, duration, transition)
        return ui.__animate(string_arg(key, "animation key"), number_arg(target, "animation target"),
            number_arg(duration, "animation duration"), string_arg(transition, "animation transition"))
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
struct WidthDependency {
    binding: String,
    family: String,
    size: f32,
    width: f32,
}
struct Materialized {
    raw: std::rc::Rc<RawNode>,
    reads: BTreeMap<String, Value>,
    widths: Vec<WidthDependency>,
    _charge: bead_core::cache::Charge,
}
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
    text_meters: RefCell<BTreeMap<String, crate::transcript::Meter>>,
    /// Generation-owned selector, so sessions never depend on process-global init order.
    font_picker: Option<crate::transcript::Picker>,
    compiled: Sources,
    fragments: RefCell<bead_core::cache::Lru<(String, String), std::rc::Rc<Fragment>>>,
    fragment_budget: bead_core::cache::Budget,
    materialized: RefCell<Option<Materialized>>,
    prelude: Arc<CompiledPrelude>,
    _compiled_charge: bead_core::cache::Charge,
    values_budget: bead_core::cache::Budget,
    vm_build_bytes: std::cell::Cell<usize>,
    animations: std::rc::Rc<RefCell<Animations>>,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum AnimationCurve {
    Linear,
    OutCubic,
    OutBack,
    InOutCubic,
}
#[derive(Clone, Copy)]
struct ScalarMotion {
    from: f64,
    to: f64,
    started: Instant,
    duration: Duration,
    curve: AnimationCurve,
}
#[derive(Clone, Default)]
struct Animations {
    states: BTreeMap<String, ScalarMotion>,
    seen: BTreeSet<String>,
    next_frame: Option<Instant>,
    frame_interval: Duration,
}
#[derive(Clone)]
pub(super) struct AnimationSnapshot(Animations);
fn animation_frame_deadline(
    now: Instant,
    started: Instant,
    interval: Duration,
    end: Instant,
) -> Instant {
    let interval_ns = interval.as_nanos();
    if interval_ns == 0 {
        return end;
    }
    let elapsed_ns = now.saturating_duration_since(started).as_nanos();
    let Some(offset_ns) = (elapsed_ns / interval_ns + 1).checked_mul(interval_ns) else {
        return end;
    };
    if offset_ns >= end.saturating_duration_since(started).as_nanos() {
        return end;
    }
    let Ok(seconds) = u64::try_from(offset_ns / 1_000_000_000) else {
        return end;
    };
    let offset = Duration::new(seconds, (offset_ns % 1_000_000_000) as u32);
    started.checked_add(offset).unwrap_or(end).min(end)
}
impl Animations {
    fn frame_interval(&self) -> Duration {
        if self.frame_interval.is_zero() {
            Duration::from_millis(16)
        } else {
            self.frame_interval
        }
    }
    fn set_frame_interval(&mut self, interval: Duration) {
        let interval = if interval.is_zero() {
            Duration::from_millis(16)
        } else {
            interval
        };
        if self.frame_interval == interval {
            return;
        }
        self.frame_interval = interval;
        let now = Instant::now();
        self.next_frame = self
            .states
            .values()
            .filter(|motion| motion.from != motion.to && now < motion.started + motion.duration)
            .map(|motion| {
                animation_frame_deadline(
                    now,
                    motion.started,
                    interval,
                    motion.started + motion.duration,
                )
            })
            .min();
    }
    fn restore(&mut self, snapshot: AnimationSnapshot) {
        let frame_interval = self.frame_interval();
        let mut restored = snapshot.0;
        restored.set_frame_interval(frame_interval);
        *self = restored;
    }
    fn begin_build(&mut self) {
        self.seen.clear();
        self.next_frame = None;
    }
    fn animate(
        &mut self,
        key: String,
        target: f64,
        duration: f64,
        transition: &str,
        now: Instant,
    ) -> Result<f64, &'static str> {
        self.animate_as(
            key,
            target,
            duration,
            transition,
            now,
            bead_core::animation::reduced_motion(),
        )
    }
    fn animate_as(
        &mut self,
        key: String,
        target: f64,
        duration: f64,
        transition: &str,
        now: Instant,
        reduced: bool,
    ) -> Result<f64, &'static str> {
        if key.is_empty() || key.len() > 256 || !target.is_finite() {
            return Err("animation key or target is invalid");
        }
        if !duration.is_finite() || !(0. ..=60.).contains(&duration) {
            return Err("animation duration must be between 0 and 60 seconds");
        }
        let curve = match transition {
            "linear" => AnimationCurve::Linear,
            "out-cubic" => AnimationCurve::OutCubic,
            "out-back" => AnimationCurve::OutBack,
            "in-out-cubic" => AnimationCurve::InOutCubic,
            _ => {
                return Err(
                    "animation transition must be linear, out-cubic, out-back, or in-out-cubic",
                );
            }
        };
        if !self.states.contains_key(&key) && self.states.len() >= 1024 {
            return Err("a view may animate at most 1024 scalar values");
        }
        self.seen.insert(key.clone());
        let current = self.states.get(&key).map(|motion| motion.value(now));
        if reduced
            || duration == 0.
            || self
                .states
                .get(&key)
                .is_none_or(|motion| motion.to != target)
        {
            let from = if reduced || duration == 0. {
                target
            } else {
                current.unwrap_or(target)
            };
            self.states.insert(
                key.clone(),
                ScalarMotion {
                    from,
                    to: target,
                    started: now,
                    duration: Duration::from_secs_f64(duration.max(0.001)),
                    curve,
                },
            );
        }
        let motion = self.states.get(&key).expect("inserted animation");
        let value = if reduced { target } else { motion.value(now) };
        if !reduced && now < motion.started + motion.duration && motion.from != motion.to {
            let deadline = animation_frame_deadline(
                now,
                motion.started,
                self.frame_interval(),
                motion.started + motion.duration,
            );
            self.next_frame = Some(self.next_frame.map_or(deadline, |old| old.min(deadline)));
        }
        Ok(value)
    }
    fn finish_build(&mut self) {
        self.states.retain(|key, _| self.seen.contains(key));
    }
}
impl ScalarMotion {
    fn value(self, now: Instant) -> f64 {
        let t = (now.saturating_duration_since(self.started).as_secs_f64()
            / self.duration.as_secs_f64())
        .clamp(0., 1.);
        let eased = match self.curve {
            AnimationCurve::Linear => t,
            AnimationCurve::OutCubic => 1. - (1. - t).powi(3),
            AnimationCurve::OutBack => {
                let shifted = t - 1.;
                1. + 2.70158 * shifted.powi(3) + 1.70158 * shifted.powi(2)
            }
            AnimationCurve::InOutCubic if t < 0.5 => 4. * t.powi(3),
            AnimationCurve::InOutCubic => 1. - (-2. * t + 2.).powi(3) / 2.,
        };
        self.from + (self.to - self.from) * eased
    }
}
impl LuauFrontEnd {
    pub fn new(entry: &Path, sources: &Sources) -> Result<Self, Diagnostic> {
        Self::new_with_picker(entry, sources, None)
    }
    pub(super) fn new_with_picker(
        entry: &Path,
        sources: &Sources,
        font_picker: Option<crate::transcript::Picker>,
    ) -> Result<Self, Diagnostic> {
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
        let prelude = compiled_prelude(&compiler, entry)?;
        let source_bytes = compiled
            .iter()
            .map(|(p, b)| p.as_os_str().len() + b.capacity() + 96)
            .sum::<usize>();
        if source_bytes.saturating_add(prelude._charge.bytes()) > 8 * 1024 * 1024 {
            return Err(Diagnostic::new(
                entry,
                1,
                1,
                "compiled markup exceeds 8 MiB cache budget",
            ));
        }
        let budget = bead_core::cache::Budget::new("compiled-markup", 8 * bead_core::cache::MIB);
        let charge = budget
            .reserve(source_bytes)
            .map_err(|e| Diagnostic::new(entry, 1, 1, e))?;
        Ok(Self {
            text_meters: Default::default(),
            font_picker,
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
            prelude,
            animations: Default::default(),
        })
    }
    fn measured_width(&self, text: &str, family: &str, size: f32) -> f32 {
        let mut meters = self.text_meters.borrow_mut();
        if meters.len() >= 8 && !meters.contains_key(family) {
            meters.clear();
        }
        let meter = meters.entry(family.to_owned()).or_insert_with(|| {
            self.font_picker.as_ref().map_or_else(
                || crate::transcript::Meter::fixed(vec![]),
                |picker| crate::transcript::Meter::new(picker.clone()),
            )
        });
        meter.width(text, family, size)
    }
    pub(super) fn resource_bytes(&self) -> (usize, usize, usize) {
        (
            self._compiled_charge.bytes() + self.prelude._charge.bytes(),
            self.values_budget.used()
                + self.fragment_budget.used()
                + self.fragments.borrow().bytes(),
            self.vm_build_bytes.get(),
        )
    }
    #[cfg(test)]
    pub(super) fn shares_compiled_prelude_with(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.prelude, &other.prelude)
    }
    pub(super) fn animation_deadline(&self) -> Option<Instant> {
        self.animations.borrow().next_frame
    }
    pub(super) fn set_frame_interval(&self, interval: Duration) {
        self.animations.borrow_mut().set_frame_interval(interval);
    }
    pub(super) fn animation_snapshot(&self) -> AnimationSnapshot {
        AnimationSnapshot(self.animations.borrow().clone())
    }
    pub(super) fn restore_animations(&self, snapshot: AnimationSnapshot) {
        self.animations.borrow_mut().restore(snapshot);
        // A rejected candidate may have populated the raw-view cache with its
        // interpolated pose. Rebuild it against the restored animation state.
        *self.materialized.borrow_mut() = None;
    }
}
impl FrontEnd for LuauFrontEnd {
    fn build(
        &self,
        entry: &Path,
        resolve: &Resolve<'_>,
    ) -> Result<std::rc::Rc<RawNode>, Diagnostic> {
        #[cfg(test)]
        BUILD_CALLS.set(BUILD_CALLS.get() + 1);
        if self.animation_deadline().is_none()
            && let Some(cached) = &*self.materialized.borrow()
            && cached
                .reads
                .iter()
                .all(|(name, value)| resolve(name).is_ok_and(|next| next == *value))
            && cached.widths.iter().all(|dependency| {
                matches!(resolve(&dependency.binding), Ok(Value::Text(text))
                    if text.len() <= 16 * 1024
                        && dependency.binding.len() <= 2048
                        && dependency.family.len() <= 128
                        && (1. ..=256.).contains(&dependency.size)
                        && self.measured_width(&text, &dependency.family, dependency.size).to_bits()
                        == dependency.width.to_bits())
            })
        {
            return Ok(cached.raw.clone());
        }
        let reads = RefCell::new(BTreeMap::new());
        let widths = RefCell::new(Vec::<WidthDependency>::new());
        let width_overflow = std::cell::Cell::new(false);
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
        let native_time = std::rc::Rc::new(std::cell::Cell::new(Duration::ZERO));
        let spent = native_time.clone();
        lua.set_interrupt(move |_| {
            if Instant::now() > deadline + spent.get() {
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
                sdk.set(
                    "text_width",
                    scope.create_function(
                        |_, (input, family, size): (mlua::Value, String, f32)| {
                            if family.len() > 128 || !(1. ..=256.).contains(&size) {
                                return Err(mlua::Error::runtime("invalid text measurement"));
                            }
                            let (text, binding) = match input {
                                mlua::Value::String(text) => (text.to_str()?.to_owned(), None),
                                mlua::Value::Table(table) => {
                                    let binding = table.get::<String>("__bind")?;
                                    if binding.is_empty() || binding.len() > 2048 {
                                        return Err(mlua::Error::runtime(
                                            "invalid text measurement",
                                        ));
                                    }
                                    // Memo fragments cache their own read set. Record a bound
                                    // measurement there so title-driven geometry invalidates
                                    // the fragment; outside memo it stays a width-only dependency.
                                    let in_memo = !scopes.borrow().is_empty();
                                    let measured = if in_memo {
                                        resolve(&binding)
                                    } else {
                                        upstream(&binding)
                                    };
                                    let Value::Text(text) =
                                        measured.map_err(mlua::Error::external)?
                                    else {
                                        return Err(mlua::Error::runtime(
                                            "bound text measurement needs a text store",
                                        ));
                                    };
                                    (text, Some(binding))
                                }
                                _ => return Err(mlua::Error::runtime("invalid text measurement")),
                            };
                            if text.len() > 16 * 1024 {
                                return Err(mlua::Error::runtime("invalid text measurement"));
                            }
                            let started = Instant::now();
                            let width = self.measured_width(&text, &family, size);
                            native_time.set(native_time.get() + started.elapsed());
                            if let Some(binding) = binding {
                                let mut dependencies = widths.borrow_mut();
                                if !dependencies.iter().any(|dependency| {
                                    dependency.binding == binding
                                        && dependency.family == family
                                        && dependency.size == size
                                }) {
                                    if dependencies.len() == 4096 {
                                        width_overflow.set(true);
                                    } else {
                                        dependencies.push(WidthDependency {
                                            binding,
                                            family,
                                            size,
                                            width,
                                        });
                                    }
                                }
                            }
                            Ok(width)
                        },
                    )?,
                )?;
                let constructors = lua.create_table()?;
                for (name, kind) in Widget::constructors() {
                    constructors.set(name, lua.to_value(&kind)?)?;
                }
                let allowed = lua.create_table_from(
                    crate::layout::PROPERTIES.iter().map(|name| (*name, true)),
                )?;
                self.animations.borrow_mut().begin_build();
                let animations = self.animations.clone();
                sdk.set(
                    "__animate",
                    scope.create_function(
                        move |_, (key, target, duration, transition): (String, f64, f64, String)| {
                            animations
                                .borrow_mut()
                                .animate(key, target, duration, &transition, Instant::now())
                                .map_err(mlua::Error::runtime)
                        },
                    )?,
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
                            return crate::script::value_to_lua(
                                lua,
                                &resolve(&name).map_err(mlua::Error::external)?,
                            );
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
                    .load(self.prelude.install.as_slice())
                    .set_name("@bead.ui")
                    .eval()?;
                install.call::<()>((sdk.clone(), constructors, allowed, lua.array_metatable()))?;
                let host_factory: Function = lua
                    .load(self.prelude.host.as_slice())
                    .set_name("@bead.host")
                    .eval()?;
                sdk.set("Host", host_factory.call::<Function>(sdk.clone())?)?;
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
                            let row: Table = render.call(crate::script::json_value(lua, item)?)?;
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
                let kit_factory: Function = lua
                    .load(self.prelude.kit.as_slice())
                    .set_name("@bead.kit")
                    .eval()?;
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
            .map(|raw| {
                // Windowed lists read the offset to choose rows. Between row
                // boundaries the authored tree is identical: keep its identity
                // so the materialized cache can update the scroll pose alone.
                self.materialized
                    .borrow()
                    .as_ref()
                    .filter(|cached| *cached.raw == raw)
                    .map_or_else(|| std::rc::Rc::new(raw), |cached| cached.raw.clone())
            })
            .map_err(|error| diagnostic(entry, error));
        self.animations.borrow_mut().finish_build();
        if let Ok(raw) = &result {
            let reads = reads.into_inner();
            let widths = widths.into_inner();
            let bytes = crate::layout::raw_bytes(raw)
                + reads
                    .iter()
                    .map(|(k, v)| k.capacity() + 64 + bead_core::cache::value_bytes(v))
                    .sum::<usize>()
                + widths
                    .iter()
                    .map(|dependency| {
                        dependency.binding.capacity()
                            + dependency.family.capacity()
                            + std::mem::size_of::<WidthDependency>()
                            + 64
                    })
                    .sum::<usize>();
            *self.materialized.borrow_mut() = None;
            if !width_overflow.get()
                && let Ok(charge) = self.values_budget.reserve(bytes)
            {
                *self.materialized.borrow_mut() = Some(Materialized {
                    raw: raw.clone(),
                    reads,
                    widths,
                    _charge: charge,
                });
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

#[cfg(test)]
mod animation_tests {
    use super::*;

    #[test]
    fn scalar_animation_curves_reach_the_target_with_expected_easing() {
        let start = Instant::now();
        let out = ScalarMotion {
            from: 0.,
            to: 1.,
            started: start,
            duration: Duration::from_secs(1),
            curve: AnimationCurve::OutCubic,
        };
        assert_eq!(out.value(start), 0.);
        assert!((out.value(start + Duration::from_millis(250)) - 0.578125).abs() < 1e-9);
        assert_eq!(out.value(start + Duration::from_secs(1)), 1.);

        let in_out = ScalarMotion {
            curve: AnimationCurve::InOutCubic,
            ..out
        };
        assert!((in_out.value(start + Duration::from_millis(250)) - 0.0625).abs() < 1e-9);
        assert_eq!(in_out.value(start + Duration::from_millis(500)), 0.5);

        let out_back = ScalarMotion {
            curve: AnimationCurve::OutBack,
            ..out
        };
        let settle = out_back.value(start + Duration::from_millis(600));
        assert!(settle > 1.09 && settle < 1.11, "soft overshoot is {settle}");
        assert_eq!(out_back.value(start + Duration::from_secs(1)), 1.);
    }

    #[test]
    fn animation_retargets_continuously_and_retires_missing_keys() {
        let start = Instant::now();
        let mut animations = Animations::default();
        animations.begin_build();
        assert_eq!(
            animations.animate_as("x".into(), 0., 0.3, "out-cubic", start, false),
            Ok(0.)
        );
        animations.finish_build();

        animations.begin_build();
        assert_eq!(
            animations.animate_as("x".into(), 100., 0.3, "out-cubic", start, false),
            Ok(0.)
        );
        animations.finish_build();
        let midpoint = start + Duration::from_millis(150);
        animations.begin_build();
        let before_retarget = animations
            .animate_as("x".into(), 100., 0.3, "out-cubic", midpoint, false)
            .unwrap();
        animations.finish_build();
        assert!((before_retarget - 87.5).abs() < 1e-9);

        animations.begin_build();
        let after_retarget = animations
            .animate_as("x".into(), 200., 0.3, "out-cubic", midpoint, false)
            .unwrap();
        animations.finish_build();
        assert_eq!(after_retarget, before_retarget);
        assert!(animations.next_frame.is_some());

        animations.begin_build();
        animations.finish_build();
        assert!(animations.states.is_empty());
        assert!(animations.next_frame.is_none());
    }

    #[test]
    fn reduced_motion_snaps_and_does_not_arm_a_frame() {
        let mut animations = Animations::default();
        animations.begin_build();
        assert_eq!(
            animations.animate_as("x".into(), 42., 0.3, "out-cubic", Instant::now(), true),
            Ok(42.)
        );
        animations.finish_build();
        assert!(animations.next_frame.is_none());
        animations.begin_build();
        assert_eq!(
            animations.animate_as("x".into(), 42., 0.3, "out-cubic", Instant::now(), false),
            Ok(42.)
        );
        animations.finish_build();
        assert!(animations.next_frame.is_none());
    }

    #[test]
    fn animation_deadlines_keep_motion_phase_and_skip_missed_slots() {
        let start = Instant::now();
        let end = start + Duration::from_secs(1);
        let interval = Duration::from_nanos(16_667_000);
        for (elapsed, expected) in [
            (Duration::ZERO, Duration::from_nanos(16_667_000)),
            (
                Duration::from_micros(1_500),
                Duration::from_nanos(16_667_000),
            ),
            (Duration::from_millis(35), Duration::from_nanos(50_001_000)),
        ] {
            assert_eq!(
                animation_frame_deadline(start + elapsed, start, interval, end),
                start + expected
            );
        }

        let refreshed = animation_frame_deadline(
            start + Duration::from_millis(35),
            start,
            Duration::from_millis(20),
            end,
        );
        assert_eq!(refreshed, start + Duration::from_millis(40));

        let motion_end = start + Duration::from_millis(55);
        assert_eq!(
            animation_frame_deadline(
                start + Duration::from_nanos(50_001_000),
                start,
                interval,
                motion_end,
            ),
            motion_end,
            "the final animation frame is scheduled exactly at its end"
        );
        assert_eq!(
            animation_frame_deadline(start, start, interval, start + Duration::from_millis(10),),
            start + Duration::from_millis(10),
            "a short motion ends before its first frame slot"
        );
    }

    #[test]
    fn animation_deadlines_follow_independent_display_intervals() {
        let start = Instant::now();
        let intervals = [
            Duration::from_nanos(1_000_000_000_000 / 60_000),
            Duration::from_nanos(1_000_000_000_000 / 120_000),
        ];
        let mut deadlines = Vec::new();
        for interval in intervals {
            let mut animations = Animations::default();
            animations.set_frame_interval(interval);
            animations.states.insert(
                "x".into(),
                ScalarMotion {
                    from: 0.,
                    to: 0.,
                    started: start,
                    duration: Duration::from_secs(1),
                    curve: AnimationCurve::Linear,
                },
            );
            animations.begin_build();
            animations
                .animate_as("x".into(), 1., 0.3, "linear", start, false)
                .unwrap();
            deadlines.push(animations.next_frame.unwrap());
        }
        assert_eq!(deadlines[0] - start, intervals[0]);
        assert_eq!(deadlines[1] - start, intervals[1]);
        assert!(deadlines[1] < deadlines[0]);
    }

    #[test]
    fn rejected_animation_candidate_restores_motion_and_current_interval() {
        let start = Instant::now();
        let interval = Duration::from_nanos(1_000_000_000_000 / 120_000);
        let mut animations = Animations::default();
        animations.set_frame_interval(interval);
        animations.states.insert(
            "x".into(),
            ScalarMotion {
                from: 0.,
                to: 0.,
                started: start,
                duration: Duration::from_secs(1),
                curve: AnimationCurve::Linear,
            },
        );
        animations.begin_build();
        animations
            .animate_as("x".into(), 1., 0.3, "linear", start, false)
            .unwrap();
        let snapshot = AnimationSnapshot(animations.clone());
        let snapshot_deadline = snapshot.0.next_frame.unwrap();
        let rejected = start + Duration::from_millis(5);
        animations.set_frame_interval(Duration::from_millis(16));
        animations.begin_build();
        animations
            .animate_as("x".into(), 9., 0.3, "linear", rejected, false)
            .unwrap();

        animations.restore(snapshot);
        assert_eq!(animations.frame_interval(), Duration::from_millis(16));
        assert_eq!(animations.states["x"].to, 1.);
        assert!(animations.next_frame.unwrap() > snapshot_deadline);
    }
}
