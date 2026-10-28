// SPDX-License-Identifier: MIT OR Apache-2.0
use super::*;
use std::sync::atomic::{AtomicU64, Ordering};
static NEXT: AtomicU64 = AtomicU64::new(0);
pub(super) struct Fixture(pub(super) PathBuf);
impl Fixture {
    pub(super) fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "bead-reload-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&root).unwrap();
        fs::write(
            root.join("palette.json"),
            r#"{"formatVersion":1,"colours":{"surface":"101010"}}"#,
        )
        .unwrap();
        fs::write(
            root.join("main.json"),
            r#"{
 "version":1,"script":"behavior.luau","width":100,
 "bounds":{"x":0,"y":0,"w":100,"h":100},
 "includes":["content.json"],"nodes":[]
}"#,
        )
        .unwrap();
        fs::write(root.join("content.json"),r#"{"nodes":[
 {"id":"search","input":"search.text","scroll":"scroll","bind":{"text":"label"},"draw":{"type":"text","rect":{"x":0,"y":0,"w":100,"h":20},"text":"placeholder","family":"sans-serif","size":12,"color":[255,255,255,255]}},
 {"id":"toggle","action":"toggle","draw":{"type":"rounded","rect":{"x":0,"y":25,"w":50,"h":20},"radius":5,"fill":[30,30,30,255],"border":null}}
]}"#).unwrap();
        let fixture = Self(root);
        fixture.behavior("default");
        fixture
    }
    fn behavior(&self, default: &str) {
        fs::write(
            self.0.join("behavior.luau"),
            format!(
                r#"return {{
 stores={{["search.text"]="{default}",["panel.open"]=true,["scroll.y"]=0}},
 actions={{toggle=function(s) s["panel.open"]=not s["panel.open"] end,
 scroll=function(s,d) s["scroll.y"]+=d end,
 fail=function(s) s["search.text"]="lost"; error("deliberate action failure") end}},
 computed={{label=function(s) return s["search.text"] end}}
}}"#
            ),
        )
        .unwrap();
    }
    pub(super) fn session(&self) -> Session {
        let fallback = Scene {
            opacity: 1.,
            motion: None,
            resource_revision: 0,
            asset_data: Default::default(),
            clips: Default::default(),
            layers: Vec::new(),
            prepared: Default::default(),
            font_files: vec![],
            width: 100.,
            slide: None,
            primitives: vec![],
            offset: (0., 0.),
            bounds: Rect::new(0., 0., 100., 100.),
        };
        let mut session = Session::new(
            self.0.clone(),
            "main.json".into(),
            self.0.join("palette.json"),
            Palette::read(&self.0.join("palette.json")).unwrap(),
            fallback,
        );
        let scene = session.reload().unwrap();
        session.commit(scene);
        session
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
#[test]
fn external_projector_refreshes_without_local_edits_and_keeps_atomic_failure() {
    let fixture = Fixture::new();
    let mut session = fixture.session();
    let external = std::rc::Rc::new(std::cell::RefCell::new("default".to_owned()));
    let current = external.clone();
    session.set_projector(Box::new(move |_| {
        let value = current.borrow().clone();
        if value == "broken" {
            return Err(Diagnostic::new(
                Path::new("external"),
                1,
                1,
                "service failure",
            ));
        }
        Ok(BTreeMap::from([("search.text".into(), Value::Text(value))]))
    }));
    assert!(session.update(BTreeMap::new()).unwrap().is_none());
    *external.borrow_mut() = "fresh".into();
    let scene = session.update(BTreeMap::new()).unwrap().unwrap();
    assert_eq!(
        session.stores.get("search.text"),
        Some(&Value::Text("default".into()))
    );
    assert!(
        scene
            .primitives
            .iter()
            .any(|p| matches!(p, Primitive::Text { text, .. } if text == "fresh"))
    );
    session.commit(scene);
    assert_eq!(
        session.stores.get("search.text"),
        Some(&Value::Text("fresh".into()))
    );
    assert!(session.update(BTreeMap::new()).unwrap().is_none());
    *external.borrow_mut() = "broken".into();
    assert!(session.update(BTreeMap::new()).is_err());
    assert_eq!(
        session.stores.get("search.text"),
        Some(&Value::Text("fresh".into()))
    );
    assert!(session.pending.is_none());
}

#[test]
fn forty_unchanged_projection_wakeups_do_not_materialize_or_build_luau() {
    let fixture = Fixture::new();
    generation::reset_materialize_calls();
    frontend::reset_build_calls();
    let mut session = fixture.session();
    assert_eq!(
        generation::materialize_calls(),
        1,
        "first frame materializes once"
    );
    assert_eq!(frontend::build_calls(), 1, "first frame builds Luau once");
    let calls = std::rc::Rc::new(std::cell::Cell::new(0));
    let projected = calls.clone();
    session.set_projector(Box::new(move |_| {
        projected.set(projected.get() + 1);
        Ok(BTreeMap::new())
    }));
    for _ in 0..40 {
        assert!(session.update(BTreeMap::new()).unwrap().is_none());
    }

    assert_eq!(calls.get(), 40);
    assert_eq!(generation::materialize_calls(), 1);
    assert_eq!(frontend::build_calls(), 1);
}
#[test]
fn coalesced_projection_is_bounded_and_rejects_a_failed_final_reply_atomically() {
    for fail in [false, true] {
        let fixture = Fixture::new();
        let mut session = fixture.session();
        let calls = std::rc::Rc::new(std::cell::Cell::new(0));
        let count = calls.clone();
        session.set_projector(Box::new(move |_| {
            let n = count.get() + 1;
            count.set(n);
            if n == 2 && fail {
                return Err(Diagnostic::new(Path::new("external"), 1, 1, "late failure"));
            }
            Ok(BTreeMap::from([(
                "search.text".into(),
                Value::Text(if n == 1 { "pending" } else { "ready" }.into()),
            )]))
        }));
        session.set_projection_coalescing(true);
        let result = session.update(BTreeMap::new());
        assert_eq!(
            calls.get(),
            2,
            "one recheck, with no waiting or unbounded loop"
        );
        assert_eq!(
            session.stores.get("search.text"),
            Some(&Value::Text("default".into()))
        );
        if fail {
            assert!(result.is_err());
            assert!(session.pending.is_none());
        } else {
            let scene = result.unwrap().unwrap();
            assert!(
                scene
                    .primitives
                    .iter()
                    .any(|p| matches!(p, Primitive::Text { text, .. } if text == "ready"))
            );
            session.commit(scene);
            assert_eq!(
                session.stores.get("search.text"),
                Some(&Value::Text("ready".into()))
            );
        }
    }
}
#[test]
fn normalized_entry_and_final_palette_and_watch_race_checks() {
    let fixture = Fixture::new();
    let (generation, stores) =
        Generation::load(&fixture.0, Path::new("./main.json"), &Stores::default(), 1).unwrap();
    assert!(
        !generation
            .scene(
                &stores,
                &Palette::read(&fixture.0.join("palette.json")).unwrap(),
                true
            )
            .unwrap()
            .primitives
            .is_empty()
    );
    let mut session = fixture.session();
    session.reload().unwrap();
    session.watch_revision.fetch_add(1, Ordering::SeqCst);
    assert!(
        session
            .check_sources()
            .unwrap_err()
            .message
            .contains("superseded")
    );
    session.reload().unwrap();
    fs::write(
        fixture.0.join("palette.json"),
        r#"{"formatVersion":1,"colours":{"surface":"202020"}}"#,
    )
    .unwrap();
    assert!(
        session
            .check_sources()
            .unwrap_err()
            .message
            .contains("palette changed")
    );
}
#[test]
fn external_palette_watch_survives_atomic_replacements() {
    let fixture = Fixture::new();
    let palette = fixture.0.with_extension("palette.json");
    let temporary = fixture.0.with_extension("palette.tmp");
    fs::copy(fixture.0.join("palette.json"), &palette).unwrap();
    let mut session = fixture.session();
    session.palette_path = palette.clone();
    let (sender, receiver) = std::sync::mpsc::channel();
    session
        .watch(Box::new(move |event| {
            let _ = sender.send(event);
        }))
        .unwrap();
    for color in ["202020", "303030"] {
        while receiver.try_recv().is_ok() {}
        fs::write(
            &temporary,
            format!(r#"{{"formatVersion":1,"colours":{{"surface":"{color}"}}}}"#),
        )
        .unwrap();
        fs::rename(&temporary, &palette).unwrap();
        receiver
            .recv_timeout(std::time::Duration::from_secs(3))
            .unwrap()
            .unwrap();
        // A rename can produce several events; simulate the runtime's debounce.
        while receiver
            .recv_timeout(std::time::Duration::from_millis(50))
            .is_ok()
        {}
        let scene = session.reload().unwrap();
        session.check_sources().unwrap();
        session.commit(scene);
    }
    drop(session);
    fs::remove_file(palette).unwrap();
}

#[test]
fn runtime_palette_before_a_generation_commits_in_memory_without_writing_palette_file() {
    let fixture = Fixture::new();
    let mut session = fixture.session();
    session.generation = None;
    let path = fixture.0.join("palette.json");
    let original = fs::read(&path).unwrap();
    let overlay =
        Palette(std::collections::HashMap::from([("primary".into(), [20, 40, 60, 255])]).into());
    assert!(
        session
            .set_runtime_palette(overlay.clone())
            .unwrap()
            .is_none()
    );
    assert_eq!(session.palette.get("primary").unwrap(), [20, 40, 60, 255]);
    assert_eq!(
        session.runtime_palette.unwrap().get("primary").unwrap(),
        [20, 40, 60, 255]
    );
    assert_eq!(fs::read(path).unwrap(), original);
}
#[test]
fn focused_input_open_panel_and_scroll_survive_recreated_views() {
    let fixture = Fixture::new();
    let mut session = fixture.session();
    let scene = session.click(5., 5.).unwrap().unwrap();
    session.commit(scene);
    let scene = session.key("saved 😀", false).unwrap().unwrap();
    session.commit(scene);
    let scene = session.scroll(5., 5., 42.).unwrap().unwrap();
    session.commit(scene);
    let focused = session.stores.get("bead.focused").cloned();
    fixture.behavior("new default must not reset the input");
    let scene = session.reload().unwrap();
    session.commit(scene);
    assert_eq!(
        session.stores.get("search.text"),
        Some(&Value::Text("defaultsaved 😀".into()))
    );
    assert_eq!(session.stores.get("scroll.y"), Some(&Value::Number(42.)));
    assert_eq!(session.stores.get("panel.open"), Some(&Value::Bool(true)));
    assert_eq!(session.stores.get("bead.focused"), focused.as_ref());
    let scene = session.key("!", false).unwrap().unwrap();
    session.commit(scene);
    assert_eq!(
        session.stores.get("search.text"),
        Some(&Value::Text("defaultsaved 😀!".into()))
    );
    let scene = session.click(5., 30.).unwrap().unwrap();
    session.commit(scene);
    let scene = session.reload().unwrap();
    session.commit(scene);
    assert_eq!(session.stores.get("panel.open"), Some(&Value::Bool(false)));
}
#[test]
fn a_broken_dependency_keeps_the_entire_old_generation_and_fix_clears_error() {
    let fixture = Fixture::new();
    let mut session = fixture.session();
    let old = fs::read(fixture.0.join("content.json")).unwrap();
    let revision = session.generation.as_ref().unwrap().revision;
    fixture.behavior("should remain default");
    fs::write(fixture.0.join("content.json"), "{\n broken").unwrap();
    let error = session.reload().err().unwrap();
    assert_eq!(error.file, "content.json");
    assert_eq!(error.line, 2);
    let overlay = session.reject(error.to_string());
    assert!(overlay.primitives.len() > session.current.primitives.len());
    assert_eq!(session.generation.as_ref().unwrap().revision, revision);
    assert_eq!(
        session.stores.get("search.text"),
        Some(&Value::Text("default".into()))
    );
    fs::write(fixture.0.join("content.json"), old).unwrap();
    let scene = session.reload().unwrap();
    session.commit(scene);
    assert!(session.error.is_none());
    assert_eq!(session.current.primitives.len(), 2);
    assert_eq!(
        session.stores.get("search.text"),
        Some(&Value::Text("default".into()))
    );
}
#[test]
fn failed_actions_and_schema_migrations_roll_back() {
    let fixture = Fixture::new();
    let mut session = fixture.session();
    let before = session.stores.values().clone();
    assert!(
        session
            .generation
            .as_ref()
            .unwrap()
            .script
            .action("fail", &session.stores, Value::Bool(true))
            .is_err()
    );
    assert_eq!(session.stores.values(), &before);
    let source = fs::read_to_string(fixture.0.join("behavior.luau"))
        .unwrap()
        .replace(r#"["search.text"]="default""#, r#"["search.text"]=1"#);
    fs::write(fixture.0.join("behavior.luau"), source).unwrap();
    assert!(
        session
            .reload()
            .err()
            .unwrap()
            .message
            .contains("changes type")
    );
    assert_eq!(session.stores.values(), &before);
}
#[test]
fn cycles_script_errors_and_changes_during_staging_are_rejected() {
    let fixture = Fixture::new();
    let mut session = fixture.session();
    let script=script::Script::load(b"return { stores={},actions={},computed={a=function(s) return s.b end,b=function(s) return s.a end}}",Path::new("cycle.luau")).unwrap();
    assert!(
        script
            .validate_computed(&Stores::default())
            .unwrap_err()
            .message
            .contains("cycle")
    );
    let old = session.stores.values().clone();
    let _scene = session.reload().unwrap();
    fs::write(fixture.0.join("content.json"), "{}").unwrap();
    assert!(session.check_sources().is_err());
    session.reject("changed during staging".into());
    assert_eq!(session.stores.values(), &old);
    let error = script::Script::load(b"return {\n stores = ???\n}", Path::new("syntax.luau"))
        .err()
        .unwrap();
    assert_eq!(error.file, "syntax.luau");
    assert_eq!(error.line, 2);
}
#[test]
fn dirty_bindings_ignore_unrelated_state_and_speculative_cache_values_do_not_leak() {
    let script=script::Script::load(br#"local calls=0; return {stores={a=1,b=2},actions={},computed={label=function(s) calls+=1; return tostring(s.a)..":"..tostring(calls) end}}"#,Path::new("binding.luau")).unwrap();
    let stores = Stores::default()
        .reconcile(&script.schema().unwrap())
        .unwrap();
    assert_eq!(
        script.resolve("label", &stores).unwrap(),
        Value::Text("1:1".into())
    );
    let mut unrelated = stores.clone();
    unrelated
        .commit(BTreeMap::from([("b".into(), Value::Number(3.))]))
        .unwrap();
    assert_eq!(
        script.resolve("label", &unrelated).unwrap(),
        Value::Text("1:1".into())
    );
    let mut abandoned = stores.clone();
    abandoned
        .commit(BTreeMap::from([("a".into(), Value::Number(4.))]))
        .unwrap();
    assert_eq!(
        script.resolve("label", &abandoned).unwrap(),
        Value::Text("4:2".into())
    );
    let mut valid = stores.clone();
    valid
        .commit(BTreeMap::from([("a".into(), Value::Number(5.))]))
        .unwrap();
    assert_eq!(
        script.resolve("label", &valid).unwrap(),
        Value::Text("5:3".into())
    );
}

#[test]
fn bad_asset_and_unused_broken_ui_files_do_not_replace_good_views() {
    let fixture = Fixture::new();
    let mut session = fixture.session();
    fs::write(fixture.0.join("unused.json"), "{\n broken").unwrap();
    let error = session.reload().err().unwrap();
    assert_eq!(error.file, "unused.json");
    assert_eq!(error.line, 2);
    fs::remove_file(fixture.0.join("unused.json")).unwrap();
    let old = session.current.primitives.len();
    fs::write(fixture.0.join("picture.svg"), "<svg broken").unwrap();
    fs::write(fixture.0.join("content.json"),r#"{"nodes":[{"id":"image","draw":{"type":"image","rect":{"x":0,"y":0,"w":20,"h":20},"path":"picture.svg"}}]}"#).unwrap();
    let error = session.reload().err().unwrap();
    assert_eq!(error.file, "picture.svg");
    session.reject(error.to_string());
    assert_eq!(session.current.primitives.len(), old);
}

#[test]
fn shared_markup_modules_are_snapshotted_and_reload_is_atomic() {
    let fixture = Fixture::new();
    let mut session = fixture.session();
    fs::create_dir(fixture.0.join("view")).unwrap();
    fs::write(
        fixture.0.join("view/behavior.luau"),
        "return {stores={},actions={},computed={}}",
    )
    .unwrap();
    fs::write(
        fixture.0.join("view/main.luau"),
        "return require(\"../shared\")()",
    )
    .unwrap();
    fs::write(fixture.0.join("shared.luau"), "local ui=require(\"bead.ui\"); return function() return ui.Box{width=20,height=20,fill=\"surface\"} end").unwrap();
    fs::write(
        fixture.0.join("view/bundle.json"),
        r#"{"modules":["shared.luau"]}"#,
    )
    .unwrap();
    session.entry = "view/main.luau".into();
    let scene = session.reload().unwrap();
    session.commit(scene);
    let old = session.current.primitives.len();
    assert!(old > 0);
    fs::write(fixture.0.join("shared.luau"), "return { broken").unwrap();
    let error = session.reload().err().unwrap();
    assert_eq!(error.file, "shared.luau");
    assert_eq!(session.current.primitives.len(), old);
    fs::write(
        fixture.0.join("view/bundle.json"),
        r#"{"modules":["../outside.luau"]}"#,
    )
    .unwrap();
    let error = session.reload().err().unwrap();
    assert_eq!(error.file, "view/bundle.json");
    assert!(error.message.contains("root-relative"));
}
