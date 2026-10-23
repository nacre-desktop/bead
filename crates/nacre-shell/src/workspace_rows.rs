// SPDX-License-Identifier: GPL-3.0-only
//! B0037: the bar's workspace rows, as Nacre's `NacreWorkspaceRow` lists them. Which
//! workspaces show (`workspaceCount`, named slots, occupied extras), the per-row icon
//! (`workspaceIcons`) and the active workspace's window hints (`workspaceWindowHints`)
//! are decided here from the compositor snapshot, so the Luau capsule only draws fields.
//! Window hints are Material category glyphs from the app's desktop entry (as QML), so no
//! image is decoded; the only cache is a bounded map of app id to glyph.
use bead::backends::{Snapshot, Window, Workspace};
use bead::core::Value;
use serde_json::{Value as Json, json};
use std::{cell::RefCell, collections::BTreeMap, collections::HashMap, path::PathBuf};

/// QML `NacreBar.workspaceCount || 6`, `workspaceIcons`, `workspaceWindowHints !== false`.
#[derive(Clone, Debug, PartialEq)]
pub struct View {
    pub count: u32,
    pub icons: Vec<String>,
    pub hints: bool,
}
impl Default for View {
    fn default() -> Self {
        Self {
            count: 6,
            icons: Vec::new(),
            hints: true,
        }
    }
}
impl View {
    /// Takes the `bar.workspace.*` host stores `Prefs::bar_stores` publishes; true when
    /// the view changed.
    pub fn follow(&mut self, values: &BTreeMap<String, Value>) -> bool {
        let mut next = self.clone();
        if let Some(Value::Number(count)) = values.get("bar.workspace.count") {
            next.count = *count as u32;
        }
        if let Some(Value::List(icons)) = values.get("bar.workspace.icons") {
            next.icons = icons
                .iter()
                .map(|i| i.as_str().unwrap_or_default().to_owned())
                .collect();
        }
        if let Some(Value::Bool(hints)) = values.get("bar.workspace.hints") {
            next.hints = *hints;
        }
        let changed = next != *self;
        *self = next;
        changed
    }
}

/// View plus the bounded glyph cache (the sole state; a few dozen short strings).
#[derive(Default)]
pub struct Rows {
    pub view: View,
    glyphs: RefCell<HashMap<String, &'static str>>,
}
const GLYPH_CACHE: usize = 64;
/// QML `NacreWorkspaceRow` cell and icon geometry (logical px).
const CELL: f64 = 32.;
const HINT: f64 = 18.;
const OVERFLOW: f64 = 20.;
const MAX_HINTS: usize = 3;
/// The bar's right cluster width the QML header subtracts (`root.width - right.width - 220`).
const RIGHT: f64 = 330.;

const SPECIFIC: [(&str, &str); 11] = [
    ("TerminalEmulator", "terminal"),
    ("InstantMessaging", "forum"),
    ("Email", "mail"),
    ("WebBrowser", "language"),
    ("Audio", "music_note"),
    ("Video", "movie"),
    ("Development", "code"),
    ("Game", "sports_esports"),
    ("Graphics", "palette"),
    ("Office", "description"),
    ("Settings", "tune"),
];
const GENERAL: [(&str, &str); 12] = [
    ("AudioVideo", "play_circle"),
    ("Audio", "music_note"),
    ("Video", "movie"),
    ("Development", "code"),
    ("Education", "school"),
    ("Game", "sports_esports"),
    ("Graphics", "palette"),
    ("Network", "language"),
    ("Office", "description"),
    ("Settings", "settings"),
    ("System", "computer"),
    ("Utility", "build"),
];
const OWN: [(&str, &str); 6] = [
    ("ai", "neurology"),
    ("brain", "psychology"),
    ("colors", "palette"),
    ("settings", "tune"),
    ("welcome", "waving_hand"),
    ("control", "tune"),
];

/// `NacreWorkspaceAppIcon.symbol` for a desktop entry's `Categories`.
fn glyph_for(app_id: &str, categories: &[String]) -> &'static str {
    let class = app_id.to_lowercase();
    if let Some(rest) = class.strip_prefix("nacre-") {
        let kind = rest.split('-').next().unwrap_or_default();
        if let Some((_, glyph)) = OWN.iter().find(|(name, _)| *name == kind) {
            return glyph;
        }
    }
    let has = |name: &str| categories.iter().any(|c| c == name);
    if let Some((_, glyph)) = SPECIFIC.iter().find(|(name, _)| has(name)) {
        return glyph;
    }
    categories
        .iter()
        .find_map(|c| GENERAL.iter().find(|(name, _)| name == c))
        .map_or("apps", |(_, glyph)| glyph)
}

/// `Categories=` of `<app id>.desktop` (exact, then lower case) in the XDG data dirs.
fn categories(dirs: &[PathBuf], app_id: &str) -> Vec<String> {
    if app_id.is_empty()
        || app_id.len() > 128
        || app_id.contains(['/', '\0'])
        || app_id.starts_with('.')
    {
        return Vec::new();
    }
    for name in [app_id.to_owned(), app_id.to_lowercase()] {
        for dir in dirs {
            let Ok(file) = std::fs::File::open(dir.join(format!("{name}.desktop"))) else {
                continue;
            };
            let mut text = String::new();
            std::io::Read::read_to_string(&mut std::io::Read::take(file, 64 * 1024), &mut text)
                .ok();
            let mut entry = false;
            for line in text.lines() {
                if line.starts_with('[') {
                    entry = line.trim() == "[Desktop Entry]";
                } else if entry && let Some(list) = line.strip_prefix("Categories=") {
                    return list
                        .split(';')
                        .filter(|c| !c.is_empty())
                        .map(str::to_owned)
                        .collect();
                }
            }
            return Vec::new();
        }
    }
    Vec::new()
}
fn data_dirs() -> Vec<PathBuf> {
    let home = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")));
    let system = std::env::var("XDG_DATA_DIRS")
        .ok()
        .filter(|d| !d.is_empty())
        .unwrap_or_else(|| "/usr/local/share:/usr/share".into());
    home.into_iter()
        .chain(system.split(':').map(PathBuf::from))
        .map(|d| d.join("applications"))
        .collect()
}

impl Rows {
    fn glyph(&self, app_id: &str, dirs: &dyn Fn() -> Vec<PathBuf>) -> &'static str {
        let mut cache = self.glyphs.borrow_mut();
        if let Some(glyph) = cache.get(app_id) {
            return glyph;
        }
        if cache.len() >= GLYPH_CACHE {
            cache.clear();
        }
        let glyph = glyph_for(app_id, &categories(&dirs(), app_id));
        cache.insert(app_id.to_owned(), glyph);
        glyph
    }
    /// The `bead.workspaces` model of one output (`None`: every output).
    pub fn rows(&self, snapshot: &Snapshot, output: Option<&str>) -> Vec<Json> {
        self.rows_with(snapshot, output, &data_dirs)
    }
    fn rows_with(
        &self,
        snapshot: &Snapshot,
        output: Option<&str>,
        dirs: &dyn Fn() -> Vec<PathBuf>,
    ) -> Vec<Json> {
        let view = &self.view;
        let output_id = output.and_then(|name| {
            snapshot
                .outputs
                .iter()
                .find(|o| o.name == name)
                .map(|o| &o.id)
        });
        let mine: Vec<&Workspace> = snapshot
            .workspaces
            .iter()
            .filter(|w| output.is_none() || output_id.is_some_and(|id| w.output.as_ref() == Some(id)))
            .collect();
        let windows_of = |w: &Workspace| -> Vec<&Window> {
            snapshot
                .windows
                .iter()
                .filter(|win| win.workspace.as_ref() == Some(&w.id))
                .collect()
        };
        let occupied = |w: &Workspace| {
            snapshot
                .windows
                .iter()
                .any(|win| win.workspace.as_ref() == Some(&w.id))
        };
        let mut shown: Vec<&Workspace> = if snapshot.capabilities.supports("perOutputWorkspaces") {
            let mut own: Vec<_> = mine.iter().copied().filter(|w| !w.visiting).collect();
            own.sort_by_key(|w| w.index);
            let named = |w: &Workspace| {
                w.fixed_slot
                    || (!w.name.is_empty() && !w.name.bytes().all(|b| b.is_ascii_digit()))
            };
            let mut local: Vec<_> = if own.iter().filter(|w| named(w)).count()
                >= view.count as usize
            {
                own.iter()
                    .copied()
                    .filter(|w| named(w) || occupied(w))
                    .collect()
            } else {
                own.clone()
            };
            if local.is_empty() && let Some(first) = own.first() {
                local.push(first);
            }
            let mut visitors: Vec<_> = mine
                .iter()
                .copied()
                .filter(|w| w.visiting && occupied(w))
                .collect();
            visitors.sort_by_key(|w| w.index);
            local.extend(visitors);
            local
        } else {
            let mut rows: Vec<_> = mine
                .iter()
                .copied()
                .filter(|w| {
                    w.index >= 1 && (w.index <= i64::from(view.count) || w.active || occupied(w))
                })
                .collect();
            rows.sort_by_key(|w| w.index);
            rows
        };
        let icon_mode = view.icons.iter().any(|i| !i.is_empty());
        let active = shown
            .iter()
            .find(|w| w.focused)
            .or_else(|| shown.iter().find(|w| w.active))
            .map(|w| w.id.clone());
        let width = output_id
            .and_then(|id| snapshot.outputs.iter().find(|o| &o.id == id))
            .filter(|o| o.scale > 0.)
            .map_or(1920., |o| f64::from(o.width) / o.scale);
        let available = 170f64.max(720f64.min(width / 2. - 130.).min(width - RIGHT - 220.));
        shown
            .iter()
            .map(|w| {
                let windows = windows_of(w);
                let selected = active.as_ref() == Some(&w.id);
                let hinted = selected && view.hints && !icon_mode;
                let count = windows.len();
                let base = shown.len() as f64 * CELL
                    + shown.len().saturating_sub(1) as f64 * 2.
                    + if count > 0 { 4. } else { 0. };
                let mut fit = MAX_HINTS
                    .min(count)
                    .min((((available - base) / HINT).floor()).max(0.) as usize);
                while hinted
                    && fit > 0
                    && base + fit as f64 * HINT + if fit < count { OVERFLOW } else { 0. } > available
                {
                    fit -= 1;
                }
                let fit = if hinted { fit } else { 0 };
                let mut row = serde_json::to_value(w).unwrap_or(Json::Null);
                row["label"] = json!(w.index.to_string());
                row["icon"] = json!(
                    usize::try_from(w.index - 1)
                        .ok()
                        .and_then(|i| view.icons.get(i))
                        .map_or("", String::as_str)
                );
                row["icon_mode"] = json!(icon_mode);
                row["occupied"] = json!(count > 0);
                row["urgent"] = json!(w.urgent || windows.iter().any(|win| win.urgent));
                row["shown"] = json!(fit);
                row["hinted"] = json!(hinted && count > 0);
                row["overflow"] = json!(if hinted && count > fit {
                    format!("+{}", count - fit)
                } else {
                    String::new()
                });
                for slot in 0..MAX_HINTS {
                    row[format!("app{}", slot + 1)] = json!(
                        windows
                            .get(slot)
                            .filter(|_| slot < fit)
                            .map_or("", |win| self.glyph(&win.app_id, dirs))
                    );
                }
                row
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bead::backends::{Capabilities, Id, Output};

    fn workspace(i: i64, name: &str, fixed: bool, active: bool) -> Workspace {
        Workspace {
            id: Id(format!("ws{i}")),
            name: name.into(),
            index: i,
            output: Some(Id("out".into())),
            active,
            focused: active,
            urgent: false,
            fixed_slot: fixed,
            visiting: false,
        }
    }
    fn window(i: usize, ws: i64, app: &str) -> Window {
        Window {
            id: Id(format!("w{i}")),
            title: String::new(),
            app_id: app.into(),
            workspace: Some(Id(format!("ws{ws}"))),
            output: None,
            focused: false,
            floating: false,
            fullscreen: None,
            urgent: false,
        }
    }
    fn snapshot(workspaces: Vec<Workspace>, windows: Vec<Window>, per_output: bool) -> Snapshot {
        let mut capabilities = Capabilities::empty();
        capabilities
            .features
            .insert("perOutputWorkspaces".into(), per_output);
        Snapshot {
            outputs: vec![Output {
                id: Id("out".into()),
                name: "out".into(),
                width: 1920,
                height: 1080,
                scale: 1.,
                refresh_mhz: 60_000,
                x: 0,
                y: 0,
                focused: true,
            }],
            workspaces,
            windows,
            capabilities,
            ..Snapshot::default()
        }
    }
    fn labels(rows: &[Json]) -> Vec<String> {
        rows.iter().map(|r| r["label"].as_str().unwrap().into()).collect()
    }
    fn no_dirs() -> Vec<PathBuf> {
        Vec::new()
    }

    #[test]
    fn follows_the_bar_stores_and_reports_only_real_changes() {
        let mut view = View::default();
        let stores = BTreeMap::from([
            ("bar.workspace.count".into(), Value::Number(4.)),
            ("bar.workspace.icons".into(), Value::list(vec![json!("home"), json!("")])),
            ("bar.workspace.hints".into(), Value::Bool(false)),
        ]);
        assert!(view.follow(&stores));
        assert_eq!((view.count, view.icons.len(), view.hints), (4, 2, false));
        assert!(!view.follow(&stores));
    }

    #[test]
    fn per_output_rows_keep_all_own_until_named_slots_fill_the_count() {
        let ws = |named: bool| {
            (1..=4)
                .map(|i| workspace(i, &if named { format!("nacre-{i}") } else { i.to_string() }, named, i == 1))
                .collect::<Vec<_>>()
        };
        let rows = Rows::default();
        let plain = snapshot(ws(false), vec![], true);
        assert_eq!(labels(&rows.rows_with(&plain, Some("out"), &no_dirs)), ["1", "2", "3", "4"]);
        // Four named slots reach a count of 3: only named or occupied workspaces stay,
        // so an unnamed spare is dropped.
        let mut spare = ws(true);
        spare.push(workspace(5, "5", false, false));
        let mut crowded = Rows::default();
        crowded.view.count = 3;
        let named = snapshot(spare.clone(), vec![], true);
        assert_eq!(labels(&crowded.rows_with(&named, Some("out"), &no_dirs)), ["1", "2", "3", "4"]);
        let used = snapshot(spare, vec![window(1, 5, "x")], true);
        assert_eq!(
            labels(&crowded.rows_with(&used, Some("out"), &no_dirs)),
            ["1", "2", "3", "4", "5"]
        );
    }

    #[test]
    fn visitors_follow_local_rows_only_while_occupied() {
        let mut visitor = workspace(9, "9", false, false);
        visitor.visiting = true;
        let spare = snapshot(vec![workspace(1, "1", false, true), visitor.clone()], vec![], true);
        let rows = Rows::default();
        assert_eq!(labels(&rows.rows_with(&spare, Some("out"), &no_dirs)), ["1"]);
        let used = snapshot(
            vec![workspace(1, "1", false, true), visitor],
            vec![window(1, 9, "x")],
            true,
        );
        assert_eq!(labels(&rows.rows_with(&used, Some("out"), &no_dirs)), ["1", "9"]);
    }

    #[test]
    fn global_backends_list_the_count_plus_active_and_occupied() {
        let ws: Vec<_> = [1, 2, 8, 12].map(|i| workspace(i, &i.to_string(), false, i == 8)).into();
        let mut rows = Rows::default();
        rows.view.count = 3;
        let one = snapshot(ws, vec![window(1, 12, "x")], false);
        assert_eq!(labels(&rows.rows_with(&one, Some("out"), &no_dirs)), ["1", "2", "8", "12"]);
    }

    #[test]
    fn icons_per_number_switch_the_row_to_icon_mode_and_hide_hints() {
        let mut rows = Rows::default();
        rows.view.icons = vec!["home".into(), String::new(), "code".into()];
        let snap = snapshot(
            (1..=3).map(|i| workspace(i, &i.to_string(), false, i == 1)).collect(),
            vec![window(1, 1, "x")],
            true,
        );
        let out = rows.rows_with(&snap, Some("out"), &no_dirs);
        let icons: Vec<_> = out.iter().map(|r| r["icon"].as_str().unwrap()).collect();
        assert_eq!(icons, ["home", "", "code"]);
        assert!(out.iter().all(|r| r["icon_mode"] == true && r["shown"] == 0));
        assert_eq!(out[0]["occupied"], true);
    }

    #[test]
    fn active_workspace_gets_up_to_three_hints_and_an_overflow_count() {
        let rows = Rows::default();
        let windows = (0..5).map(|i| window(i, 1, "Foo")).collect();
        let snap = snapshot(vec![workspace(1, "1", false, true), workspace(2, "2", false, false)], windows, true);
        let out = rows.rows_with(&snap, Some("out"), &no_dirs);
        assert_eq!((out[0]["shown"].as_u64(), out[0]["overflow"].as_str()), (Some(3), Some("+2")));
        assert_eq!(out[0]["app3"], "apps");
        assert_eq!((out[1]["shown"].as_u64(), out[1]["app1"].as_str()), (Some(0), Some("")));
        let mut off = Rows::default();
        off.view.hints = false;
        let none = off.rows_with(&snap, Some("out"), &no_dirs);
        assert_eq!((none[0]["shown"].as_u64(), none[0]["overflow"].as_str()), (Some(0), Some("")));
    }

    #[test]
    fn narrow_bars_allocate_fewer_hints_by_the_qml_width_rule() {
        let rows = Rows::default();
        let windows = (0..3).map(|i| window(i, 1, "Foo")).collect();
        let mut snap = snapshot((1..=6).map(|i| workspace(i, &i.to_string(), false, i == 1)).collect(), windows, true);
        // 6 cells: base 6*32+5*2+4 = 206. Width 800 leaves 250: one hint and the overflow
        // mark fit (244); width 600 leaves the 170 minimum, below the base: none.
        snap.outputs[0].width = 800;
        assert_eq!(rows.rows_with(&snap, Some("out"), &no_dirs)[0]["shown"], 1);
        snap.outputs[0].width = 600;
        assert_eq!(rows.rows_with(&snap, Some("out"), &no_dirs)[0]["shown"], 0);
    }

    #[test]
    fn glyphs_follow_desktop_entry_categories_and_nacre_names() {
        let c = |list: &[&str]| list.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(glyph_for("kitty", &c(&["System", "TerminalEmulator"])), "terminal");
        assert_eq!(glyph_for("x", &c(&["Network", "Chat", "InstantMessaging"])), "forum");
        assert_eq!(glyph_for("x", &c(&["Utility"])), "build");
        assert_eq!(glyph_for("x", &c(&["AudioVideo", "Player"])), "play_circle");
        assert_eq!(glyph_for("x", &[]), "apps");
        assert_eq!(glyph_for("Nacre-Brain-1", &[]), "psychology");
        assert_eq!(glyph_for("nacre-other", &c(&["Office"])), "description");
    }

    #[test]
    fn desktop_entries_are_read_from_the_data_dirs_and_cached_within_a_bound() {
        let dir = std::env::temp_dir().join(format!("bead-ws-glyph-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("firefox.desktop"),
            "[Desktop Entry]\nName=Firefox\nCategories=Network;WebBrowser;\n[Desktop Action x]\nCategories=Game;\n",
        )
        .unwrap();
        let dirs = vec![dir.clone()];
        assert_eq!(categories(&dirs, "firefox"), ["Network", "WebBrowser"]);
        assert_eq!(categories(&dirs, "Firefox"), ["Network", "WebBrowser"]);
        assert!(categories(&dirs, "../firefox").is_empty());
        assert!(categories(&dirs, "missing").is_empty());
        let rows = Rows::default();
        let lookup = {
            let dirs = dirs.clone();
            move || dirs.clone()
        };
        assert_eq!(rows.glyph("firefox", &lookup), "language");
        for i in 0..2 * GLYPH_CACHE {
            rows.glyph(&format!("app{i}"), &lookup);
            assert!(rows.glyphs.borrow().len() <= GLYPH_CACHE);
        }
        std::fs::remove_dir_all(dir).ok();
    }
}
