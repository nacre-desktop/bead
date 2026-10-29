// SPDX-License-Identifier: MIT OR Apache-2.0
use super::*;
use bead_core::Stores;

const VALID: &str = r#"
id = "demo"
name = "Demo"
version = "1.0.0"
api = 1
capabilities = ["topic:demo.", "action:demo.open", "network"]
[[widget]]
id = "demo"
entry = "widget.luau"
[settings.size]
type = "number"
default = 2
min = 1
max = 4
"#;

struct Temp(PathBuf);
impl Temp {
    fn new(name: &str) -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "bead-plugins-{name}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn plugin(dir: &Path, id: &str, manifest: &str, widget: &str) {
    fs::create_dir_all(dir.join(id)).unwrap();
    fs::write(
        dir.join(id).join("plugin.toml"),
        manifest.replace("demo", id),
    )
    .unwrap();
    fs::write(dir.join(id).join("widget.luau"), widget).unwrap();
}
/// A view root with a bundle that mounts plugins, a system and a user plugin dir.
fn fixture(view: &str) -> (Temp, Config) {
    let temp = Temp::new("view");
    let root = &temp.0;
    fs::create_dir_all(root.join("view")).unwrap();
    fs::write(
        root.join("palette.json"),
        r#"{"formatVersion":1,"colours":{"surface":"101010"}}"#,
    )
    .unwrap();
    fs::write(
        root.join("view/bundle.json"),
        r#"{"plugins":true,"modules":[]}"#,
    )
    .unwrap();
    fs::write(root.join("view/behavior.luau"), "return {stores={secret=\"s\",[\"demo.text\"]=\"d\"},actions={[\"demo.open\"]=function() end},computed={}}").unwrap();
    fs::write(root.join("view/main.luau"), view).unwrap();
    let config = Config {
        system: vec![key(root).join(MOUNT)],
        user: Some(root.join("user")),
        state: Some(root.join("state/plugins.json")),
    };
    configure(root, config.clone());
    (temp, config)
}
/// Builds every loaded plugin's widget and gates its output, like a host registry.
const HOST_VIEW: &str = r#"
local ui=require("bead.ui")
local ok,index=pcall(function() return require("../plugins/index") end)
local children={id="root",width=100,height=100}
for _,entry in (ok and index.plugins or {}) do
    if entry.loaded then
        for _,widget in entry.widgets do
            local package=require("../"..widget.module)({ui=ui,plugin={id=entry.id,settings=entry.values}})
            table.insert(children,ui.plugin_gate(entry.id,package.render()))
        end
    end
end
return ui.Column(children)
"#;
fn load(root: &Path) -> Result<(), crate::Diagnostic> {
    let (generation, stores) =
        crate::Generation::load(root, Path::new("view/main.luau"), &Stores::default(), 1)?;
    generation
        .materialize(
            &stores,
            &bead_render::Palette(Default::default()),
            (100., 100.),
        )
        .map(drop)
}

#[test]
fn manifests_validate_identity_api_capabilities_entries_and_settings() {
    let manifest = Manifest::parse(VALID).unwrap();
    assert_eq!(manifest.widgets[0].entry, PathBuf::from("widget.luau"));
    for (broken, expected) in [
        (
            VALID.replace("id = \"demo\"\nname", "id = \"Demo!\"\nname"),
            "invalid plugin id",
        ),
        (
            VALID.replace("api = 1", "api = 2"),
            "outside this host's range",
        ),
        (
            VALID.replace("\"network\"", "\"root\""),
            "unknown capability",
        ),
        (
            VALID.replace("\"topic:demo.\"", "\"topic:\""),
            "invalid capability",
        ),
        (
            VALID.replace("id = \"demo\"\nentry", "id = \"other\"\nentry"),
            "must be unique",
        ),
        (
            VALID.replace("widget.luau", "../escape.luau"),
            "relative .luau path",
        ),
        (
            VALID.replace("default = 2", "default = 9"),
            "invalid setting size",
        ),
        (
            VALID.replace("api = 1", "api = 1\nsurprise = true"),
            "unknown field",
        ),
        (
            VALID.replace("[[widget]]\nid = \"demo\"\nentry = \"widget.luau\"", ""),
            "at least one",
        ),
    ] {
        let error = Manifest::parse(&broken).unwrap_err();
        assert!(error.contains(expected), "{expected}: {error}");
    }
    assert_eq!(
        Capability::parse("action:demo.open"),
        Ok(Capability::Action("demo.open".into()))
    );
}

#[test]
fn user_plugins_start_disabled_and_enable_disable_persists_and_unmounts() {
    let (temp, config) = fixture(HOST_VIEW);
    let user = temp.0.join("user");
    plugin(
        &user,
        "demo",
        VALID,
        "return function(api) return {render=function() return api.ui.Text{id='t',text='x'} end} end",
    );
    let mounted = |_: &Config| {
        let mount = mount(&temp.0);
        (
            mount
                .sources
                .contains_key(Path::new("plugins/demo/widget.luau")),
            mount.grants.contains_key("demo"),
        )
    };
    assert_eq!(
        mounted(&config),
        (false, false),
        "user plugins start disabled"
    );
    set_enabled(&config, "demo", true).unwrap();
    assert_eq!(
        State::load(config.state.as_deref()).enabled.get("demo"),
        Some(&true)
    );
    assert_eq!(mounted(&config), (true, true));
    load(&temp.0).unwrap();
    set_setting(&config, "demo", "size", serde_json::json!(3)).unwrap();
    assert!(set_setting(&config, "demo", "size", serde_json::json!(7)).is_err());
    let index =
        String::from_utf8(mount(&temp.0).sources[Path::new("plugins/index.luau")].clone()).unwrap();
    assert!(index.contains("[\"size\"]=3"), "{index}");
    set_enabled(&config, "demo", false).unwrap();
    assert_eq!(
        mounted(&config),
        (false, false),
        "disabling unloads the plugin"
    );
    assert!(set_enabled(&config, "missing", true).is_err());
}

#[test]
fn host_functions_deny_undeclared_topics_actions_and_requires() {
    for (widget, expected) in [
        (
            "return function(api) return {render=function() return api.ui.Text{id='t',text=api.ui.bind('secret')} end} end",
            Some("lacks capability topic:secret"),
        ),
        (
            "return function(api) return {render=function() return api.ui.Button{id='b',on_activate=api.ui.action('shell.quit')} end} end",
            Some("lacks capability action:shell.quit"),
        ),
        (
            "return function(api) return {render=function() return api.ui.Text{id='t',text=api.ui.resolve_for_check(api.ui.bind('secret'))} end} end",
            Some("lacks capability topic:secret"),
        ),
        (
            "local stolen=require('../../view/main')\nreturn function() end",
            Some("may not require"),
        ),
        (
            "return function(api) return {render=function() return api.ui.Button{id='b',text=api.ui.bind('demo.text'),on_activate=api.ui.action('demo.open'),api.ui.Text{id='h',text=api.ui.bind('bead.hovered')}} end} end",
            None,
        ),
    ] {
        let (temp, config) = fixture(HOST_VIEW);
        plugin(&config.system[0], "demo", VALID, widget);
        match (load(&temp.0), expected) {
            (Ok(()), None) => {}
            (Err(error), Some(expected)) => {
                assert!(error.message.contains(expected), "{expected}: {error}")
            }
            (result, expected) => panic!("{widget}: {result:?} expected {expected:?}"),
        }
    }
}

#[test]
fn a_broken_plugin_is_listed_with_its_error_and_others_still_load() {
    let (temp, config) = fixture(HOST_VIEW);
    let system = &config.system[0];
    plugin(
        system,
        "demo",
        VALID,
        "return function(api) return {render=function() return api.ui.Text{id='t',text='ok'} end} end",
    );
    plugin(
        system,
        "broken",
        &VALID.replace("api = 1", "api = 99"),
        "return 1",
    );
    fs::create_dir_all(system.join("misnamed")).unwrap();
    fs::write(system.join("misnamed/plugin.toml"), VALID).unwrap();
    let mount = mount(&temp.0);
    assert!(mount.grants.contains_key("demo"));
    assert!(!mount.grants.contains_key("broken") && !mount.grants.contains_key("misnamed"));
    let index = String::from_utf8(mount.sources[Path::new("plugins/index.luau")].clone()).unwrap();
    assert!(index.contains("outside this host's range"), "{index}");
    assert!(
        index.contains("directory misnamed holds plugin demo"),
        "{index}"
    );
    load(&temp.0).unwrap();
    // A recorded fault keeps a plugin out until the user enables it again.
    fault(&temp.0, "demo", "render failed");
    assert!(!super::mount(&temp.0).grants.contains_key("demo"));
    set_enabled(&config, "demo", true).unwrap();
    assert!(super::mount(&temp.0).grants.contains_key("demo"));
}

#[test]
fn local_install_copies_a_folder_disabled_and_refuses_duplicates() {
    let (temp, config) = fixture(HOST_VIEW);
    let source = temp.0.join("source");
    plugin(&source, "hello", VALID, "return function() end");
    assert_eq!(
        install_local(&config, &source.join("hello")).unwrap(),
        "hello"
    );
    let state = State::load(config.state.as_deref());
    assert_eq!(state.enabled.get("hello"), Some(&false));
    assert!(temp.0.join("user/hello/widget.luau").is_file());
    assert!(
        install_local(&config, &source.join("hello"))
            .unwrap_err()
            .contains("already installed")
    );
    assert!(install_git(&config, "--upload-pack=x").is_err());
}
