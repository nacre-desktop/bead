use crate::{
    item::StatusNotifierItem,
    menu::{MenuDiff, MenuItem, MenuItemUpdate, TrayMenu},
};
use std::sync::{Arc, Mutex};

#[cfg(feature = "data")]
use {crate::client::UpdateEvent, tracing::error};

#[cfg(feature = "data")]
pub type BaseMap = std::collections::HashMap<String, (StatusNotifierItem, Option<TrayMenu>)>;

#[cfg(not(feature = "data"))]
type BaseMap = std::collections::HashSet<String>;

#[derive(Debug, Clone)]
pub(crate) struct TrayItemMap {
    inner: Arc<Mutex<BaseMap>>,
}

impl TrayItemMap {
    pub(crate) fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(BaseMap::default())),
        }
    }

    #[cfg(feature = "data")]
    pub(crate) fn get_map(&self) -> Arc<Mutex<BaseMap>> {
        self.inner.clone()
    }

    pub(crate) fn new_item(&self, dest: String, item: &StatusNotifierItem) -> bool {
        let mut lock = self.inner.lock().expect("mutex lock should succeed");
        cfg_if::cfg_if! {
            if #[cfg(feature = "data")] {
                if dest.len() > 512 || lock.contains_key(&dest) || lock.len() >= 64 { return false; }
                let mut item = item.clone();
                bound_item(&mut item);
                lock.insert(dest, (item, None));
                true
            }else {
                let _ = item;
                if dest.len() > 512 || lock.contains(&dest) || lock.len() >= 64 { return false; }
                lock.insert(dest)
            }
        }
    }

    pub(crate) fn remove_item(&self, dest: &str) {
        self.inner
            .lock()
            .expect("mutex lock should succeed")
            .remove(dest);
    }

    pub(crate) fn clear_items(&self) -> Vec<String> {
        let mut lock = self.inner.lock().expect("mutex lock should succeed");
        cfg_if::cfg_if! {
            if #[cfg(feature = "data")] {
                lock.drain().map(|(k, _)| k).collect()
            }else {
                lock.drain().collect()
            }
        }
    }

    pub(crate) fn update_menu(&self, dest: &str, menu: &TrayMenu) {
        cfg_if::cfg_if! {
            if #[cfg(feature = "data")] {
                if let Some((_, menu_cache)) = self.inner
                        .lock()
                        .expect("should get lock")
                        .get_mut(dest) {
                    let mut menu = menu.clone();
                    bound_menu(&mut menu);
                    menu_cache.replace(menu);
                } else {
                    tracing::error!("could not find item in state");
                }
            }else {
                let _ = menu;
                let _ = dest;
            }
        }
    }

    #[cfg(feature = "data")]
    pub(crate) fn apply_update_event(&self, dest: &str, event: &UpdateEvent) {
        if let Some((item, menu)) = self
            .inner
            .lock()
            .expect("mutex lock should succeed")
            .get_mut(dest)
        {
            match event {
                UpdateEvent::AttentionIcon {
                    icon_name,
                    icon_pixmap,
                } => {
                    item.attention_icon_name.clone_from(icon_name);
                    item.attention_icon_pixmap.clone_from(icon_pixmap);
                }
                UpdateEvent::Icon {
                    icon_name,
                    icon_pixmap,
                } => {
                    item.icon_name.clone_from(icon_name);
                    item.icon_pixmap.clone_from(icon_pixmap);
                }
                UpdateEvent::OverlayIcon(icon_name) => item.overlay_icon_name.clone_from(icon_name),
                UpdateEvent::Status(status) => item.status = *status,
                UpdateEvent::Title(title) => item.title.clone_from(title),
                UpdateEvent::Tooltip(tooltip) => item.tool_tip.clone_from(tooltip),
                UpdateEvent::Menu(tray_menu) => *menu = Some(tray_menu.clone()),
                UpdateEvent::MenuConnect(menu) => item.menu = Some(menu.clone()),
                UpdateEvent::MenuDiff(menu_diffs) => {
                    if let Some(menu) = menu {
                        apply_menu_diffs(menu, menu_diffs);
                    }
                }
            }
            bound_item(item);
            if let Some(menu) = menu {
                bound_menu(menu);
            }
        } else {
            error!("could not find item in state");
        }
    }
}

#[cfg(feature = "data")]
fn bounded(value: &mut Option<String>, max: usize) {
    if let Some(text) = value {
        *text = text.chars().filter(|c| !c.is_control()).take(max).collect();
    }
}

#[cfg(feature = "data")]
fn bound_item(item: &mut StatusNotifierItem) {
    const ICON_BYTES: usize = 1024 * 1024;
    bounded(&mut item.title, 512);
    bounded(&mut item.icon_theme_path, 1024);
    bounded(&mut item.icon_name, 512);
    bounded(&mut item.overlay_icon_name, 512);
    bounded(&mut item.attention_icon_name, 512);
    bounded(&mut item.attention_movie_name, 512);
    let mut remaining = ICON_BYTES;
    for list in [
        &mut item.icon_pixmap,
        &mut item.overlay_icon_pixmap,
        &mut item.attention_icon_pixmap,
    ] {
        if let Some(pixmaps) = list {
            pixmaps.retain_mut(|pixmap| {
                let expected = usize::try_from(pixmap.width)
                    .ok()
                    .and_then(|w| {
                        usize::try_from(pixmap.height)
                            .ok()
                            .and_then(|h| w.checked_mul(h))
                    })
                    .and_then(|area| area.checked_mul(4));
                let Some(bytes) = expected.filter(|bytes| {
                    pixmap.width > 0
                        && pixmap.height > 0
                        && pixmap.width <= 256
                        && pixmap.height <= 256
                        && *bytes == pixmap.pixels.len()
                        && *bytes <= remaining
                }) else {
                    return false;
                };
                remaining -= bytes;
                true
            });
        }
    }
    if let Some(tip) = &mut item.tool_tip {
        tip.icon_name = tip
            .icon_name
            .chars()
            .filter(|c| !c.is_control())
            .take(512)
            .collect();
        tip.title = tip
            .title
            .chars()
            .filter(|c| !c.is_control())
            .take(512)
            .collect();
        tip.description = tip
            .description
            .chars()
            .filter(|c| !c.is_control())
            .take(2048)
            .collect();
        tip.icon_data.retain_mut(|pixmap| {
            let Some(bytes) = usize::try_from(pixmap.width)
                .ok()
                .and_then(|w| {
                    usize::try_from(pixmap.height)
                        .ok()
                        .and_then(|h| w.checked_mul(h))
                })
                .and_then(|n| n.checked_mul(4))
            else {
                return false;
            };
            if pixmap.width <= 0
                || pixmap.height <= 0
                || pixmap.width > 256
                || pixmap.height > 256
                || bytes != pixmap.pixels.len()
                || bytes > remaining
            {
                return false;
            }
            remaining -= bytes;
            true
        });
    }
}

#[cfg(feature = "data")]
fn bound_menu(menu: &mut TrayMenu) {
    fn trim(
        entries: &mut Vec<MenuItem>,
        depth: usize,
        budget: &mut usize,
        icon_budget: &mut usize,
    ) {
        if depth > 8 {
            entries.clear();
            return;
        }
        entries.truncate(*budget);
        *budget -= entries.len();
        for entry in entries {
            if let Some(label) = &mut entry.label {
                *label = label
                    .chars()
                    .filter(|c| !c.is_control())
                    .take(512)
                    .collect();
            }
            if let Some(icon) = &mut entry.icon_name {
                *icon = icon.chars().filter(|c| !c.is_control()).take(512).collect();
            }
            if let Some(display) = &mut entry.children_display {
                *display = display
                    .chars()
                    .filter(|c| !c.is_control())
                    .take(512)
                    .collect();
            }
            if let Some(shortcuts) = &mut entry.shortcut {
                shortcuts.truncate(8);
                for shortcut in shortcuts {
                    shortcut.truncate(8);
                    for key in shortcut {
                        *key = key.chars().filter(|c| !c.is_control()).take(64).collect();
                    }
                }
            }
            if let Some(data) = &mut entry.icon_data {
                let keep = data.len().min(4 * 1024).min(*icon_budget);
                if keep < data.len() {
                    *data = data[..keep].to_vec();
                }
                *icon_budget -= keep;
            }
            trim(&mut entry.submenu, depth + 1, budget, icon_budget);
        }
    }
    let mut budget = 256;
    let mut icon_budget = 1024 * 1024;
    trim(&mut menu.submenus, 0, &mut budget, &mut icon_budget);
}

pub fn apply_menu_diffs(tray_menu: &mut TrayMenu, diffs: &[MenuDiff]) {
    let mut diff_iter = diffs.iter().peekable();
    tray_menu.submenus.iter_mut().for_each(|item| {
        if let Some(diff) = diff_iter.next_if(|d| d.id == item.id) {
            apply_menu_item_diff(item, &diff.update);
        }
    });
}

fn apply_menu_item_diff(menu_item: &mut MenuItem, update: &MenuItemUpdate) {
    if let Some(label) = &update.label {
        menu_item.label.clone_from(label);
    }
    if let Some(enabled) = update.enabled {
        menu_item.enabled = enabled;
    }
    if let Some(visible) = update.visible {
        menu_item.visible = visible;
    }
    if let Some(icon_name) = &update.icon_name {
        menu_item.icon_name.clone_from(icon_name);
    }
    if let Some(icon_data) = &update.icon_data {
        menu_item.icon_data.clone_from(icon_data);
    }
    if let Some(toggle_state) = update.toggle_state {
        menu_item.toggle_state = toggle_state;
    }
    if let Some(disposition) = update.disposition {
        menu_item.disposition = disposition;
    }
}
