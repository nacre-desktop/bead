// SPDX-License-Identifier: MIT OR Apache-2.0
//! Bounded notification state. Transport and views share IDs, never view pointers.
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    time::{Duration, Instant},
};
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Notice {
    pub id: u32,
    pub app: String,
    pub icon: String,
    pub summary: String,
    pub body: String,
    pub actions: Vec<(String, String)>,
    pub urgency: u8,
    pub transient: bool,
    pub resident: bool,
    pub popup: bool,
    #[serde(default)]
    pub expanded: bool,
    #[serde(skip)]
    pub deadline: Option<Instant>,
    #[serde(skip)]
    pub remaining: Option<Duration>,
}
#[derive(Default)]
pub struct Center {
    pub active: BTreeMap<u32, Notice>,
    pub history: Vec<Notice>,
    pub exiting: BTreeMap<u32, (Notice, Instant)>,
    pub dnd: bool,
    pub inhibitors: BTreeMap<u32, (String, String)>,
    next: u32,
    cookie: u32,
}
impl Center {
    pub fn suppressed(&self) -> bool {
        self.dnd || !self.inhibitors.is_empty()
    }
    pub fn notify(
        &mut self,
        mut notice: Notice,
        replaces: u32,
        timeout: i32,
        now: Instant,
    ) -> Result<u32, String> {
        if notice.actions.len() > 16
            || notice.actions.iter().any(|(key, label)| {
                key.is_empty()
                    || key.len() > 256
                    || label.len() > 1024
                    || key.contains('\0')
                    || label.contains('\0')
            })
            || [&notice.app, &notice.icon, &notice.summary, &notice.body]
                .iter()
                .any(|s| s.len() > 8192 || s.contains('\0'))
        {
            return Err("notification exceeds text/action limits".into());
        }
        if replaces != 0 && self.active.contains_key(&replaces) {
            notice.id = replaces;
        } else {
            if self.active.len() >= 128 {
                return Err("too many active notifications".into());
            }
            self.next = self
                .next
                .checked_add(1)
                .ok_or("notification IDs exhausted")?;
            notice.id = self.next;
        }
        notice.popup = !self.suppressed();
        let timeout = if timeout < 0 {
            if notice.urgency == 2 { 0 } else { 5000 }
        } else {
            timeout
        };
        notice.deadline = (timeout > 0).then(|| now + Duration::from_millis(timeout as u64));
        notice.remaining = None;
        let id = notice.id;
        self.active.insert(id, notice);
        Ok(id)
    }
    pub fn close(&mut self, id: u32) -> Result<Notice, String> {
        let mut notice = self.active.remove(&id).ok_or("unknown notification ID")?;
        if notice.popup {
            self.exiting.insert(
                id,
                (notice.clone(), Instant::now() + Duration::from_millis(230)),
            );
        }
        while self.exiting.len() > 5 {
            self.exiting.pop_first();
        }
        notice.popup = false;
        notice.deadline = None;
        notice.remaining = None;
        if !notice.transient {
            self.history.retain(|n| n.id != id);
            self.history.push(notice.clone());
            if self.history.len() > 256 {
                self.history.remove(0);
            }
        }
        Ok(notice)
    }
    pub fn action(&mut self, id: u32, key: &str) -> Result<bool, String> {
        let notice = self.active.get(&id).ok_or("unknown notification ID")?;
        if !notice.actions.iter().any(|(k, _)| k == key) {
            return Err("unknown notification action".into());
        }
        Ok(!notice.resident)
    }
    pub fn set_dnd(&mut self, value: bool) {
        self.dnd = value;
        // Leaving suppression never replays old popups.
        if value {
            self.exiting.clear();
            for n in self.active.values_mut() {
                n.popup = false;
            }
        }
    }
    pub fn inhibit(&mut self, owner: String, reason: String) -> Result<u32, String> {
        if self.inhibitors.len() >= 64 || reason.len() > 1024 {
            return Err("inhibition limit".into());
        }
        self.cookie = self
            .cookie
            .checked_add(1)
            .ok_or("inhibition IDs exhausted")?;
        self.inhibitors.insert(self.cookie, (owner, reason));
        self.exiting.clear();
        for n in self.active.values_mut() {
            n.popup = false;
        }
        Ok(self.cookie)
    }
    pub fn uninhibit(&mut self, owner: &str, cookie: u32) -> Result<(), String> {
        if !self
            .inhibitors
            .get(&cookie)
            .is_some_and(|(o, _)| o == owner)
        {
            return Err("unknown or foreign inhibition".into());
        }
        self.inhibitors.remove(&cookie);
        Ok(())
    }
    pub fn owner_left(&mut self, owner: &str) {
        self.inhibitors.retain(|_, (o, _)| o != owner);
    }
    pub fn hover(&mut self, id: u32, hover: bool, now: Instant) -> Result<(), String> {
        let n = self.active.get_mut(&id).ok_or("unknown notification ID")?;
        if hover {
            if let Some(deadline) = n.deadline.take() {
                n.remaining = Some(deadline.saturating_duration_since(now));
            }
        } else if let Some(remaining) = n.remaining.take() {
            n.deadline = Some(now + remaining);
        }
        Ok(())
    }
    pub fn hide_popups(&mut self) {
        self.exiting.clear();
        for n in self.active.values_mut() {
            n.popup = false;
        }
    }
    pub fn deadline(&self) -> Option<Instant> {
        self.active
            .values()
            .filter_map(|n| n.deadline)
            .chain(self.exiting.values().map(|(_, deadline)| *deadline))
            .min()
    }
    pub fn expire(&mut self, now: Instant) -> Vec<u32> {
        self.exiting.retain(|_, (_, deadline)| *deadline > now);
        let ids: Vec<_> = self
            .active
            .values()
            .filter(|n| n.deadline.is_some_and(|d| d <= now))
            .map(|n| n.id)
            .collect();
        for id in &ids {
            let _ = self.close(*id);
        }
        ids
    }
    pub fn restore(&mut self, rows: Vec<Notice>) -> Result<(), String> {
        if rows.len() > 256 {
            return Err("history exceeds limit".into());
        }
        for n in &rows {
            if n.id == 0 || n.summary.len() > 8192 || n.body.len() > 8192 {
                return Err("invalid history".into());
            }
        }
        self.next = rows.iter().map(|n| n.id).max().unwrap_or(0);
        self.history = rows
            .into_iter()
            .map(|mut n| {
                n.popup = false;
                n.deadline = None;
                n.remaining = None;
                n
            })
            .collect();
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn notice() -> Notice {
        Notice {
            id: 0,
            app: "Example".into(),
            icon: String::new(),
            summary: "Title".into(),
            body: "Body".into(),
            actions: vec![("default".into(), "Open".into())],
            urgency: 1,
            transient: false,
            resident: false,
            popup: false,
            expanded: false,
            deadline: None,
            remaining: None,
        }
    }
    #[test]
    fn lifecycle_replacement_actions_hover_history_and_suppression() {
        let now = Instant::now();
        let mut c = Center::default();
        let id = c.notify(notice(), 0, 100, now).unwrap();
        assert_eq!(c.notify(notice(), id, 100, now).unwrap(), id);
        assert_eq!(c.active.len(), 1);
        c.hover(id, true, now + Duration::from_millis(40)).unwrap();
        assert!(c.expire(now + Duration::from_secs(1)).is_empty());
        c.hover(id, false, now + Duration::from_secs(1)).unwrap();
        assert_eq!(c.expire(now + Duration::from_millis(1060)), vec![id]);
        assert_eq!(c.history.len(), 1);
        let id = c.notify(notice(), 0, 0, now).unwrap();
        assert!(c.action(id, "default").unwrap());
        assert!(c.action(id, "bogus").is_err());
        c.set_dnd(true);
        assert!(!c.active[&id].popup);
        c.set_dnd(false);
        assert!(!c.active[&id].popup);
        let token = c.inhibit(":1.2".into(), "screen sharing".into()).unwrap();
        assert!(c.uninhibit(":1.3", token).is_err());
        c.owner_left(":1.2");
        assert!(!c.suppressed());
        let mut n = notice();
        n.transient = true;
        let id = c.notify(n, 0, 0, now).unwrap();
        c.close(id).unwrap();
        assert_eq!(c.history.len(), 1);
        let mut restored = Center::default();
        restored.restore(c.history.clone()).unwrap();
        assert!(restored.notify(notice(), 0, 0, now).unwrap() > c.history[0].id);
    }
}
