// SPDX-License-Identifier: MIT
//! Transient viewport decoration; never participates in content layout.
use crate::{Diagnostic, Node, Session, rect, scrollbar};
use bead_core::{Stores, Value};
use bead_render::{Palette, Primitive, Scene, SceneLayer};
use std::{
    collections::BTreeMap,
    time::{Duration, Instant},
};

#[derive(Default)]
pub(super) struct Feedback {
    pub pointer: Option<(f32, f32)>,
    pub capture: Option<(String, f32)>,
    activity: BTreeMap<String, Activity>,
    pub dirty: bool,
    pub deadline: Option<Instant>,
}
struct Activity {
    shown: Instant,
    last: Instant,
}
pub(super) struct KeyboardMotion {
    pub store: String,
    pub started: Instant,
    pub from: f64,
    pub target: f64,
    pub next: Instant,
}
impl KeyboardMotion {
    pub fn value(&self, now: Instant) -> (f64, bool) {
        let t = (now.saturating_duration_since(self.started).as_secs_f64() / 0.125).min(1.);
        (
            self.from + (self.target - self.from) * (1. - (1. - t).powi(3)),
            t >= 1.,
        )
    }
}
impl Feedback {
    pub fn leave(&mut self, id: &str, now: Instant, config: &scrollbar::Config) {
        self.reveal(id, now, config);
        if let Some(activity) = self.activity.get_mut(id) {
            activity.shown = now - Duration::from_secs_f32(config.fade_ms / 1000.);
        }
    }

    pub fn reveal(&mut self, id: &str, now: Instant, config: &scrollbar::Config) {
        if !self.activity.contains_key(id) {
            self.activity.insert(
                id.into(),
                Activity {
                    shown: now,
                    last: now,
                },
            );
        }
        let entry = self.activity.get_mut(id).expect("activity inserted");
        if now.saturating_duration_since(entry.last).as_secs_f32() * 1000.
            > (config.auto_hide_ms + config.fade_ms)
        {
            entry.shown = now;
        }
        entry.last = now;
        self.dirty = true;
    }
    pub fn paint(
        &mut self,
        scene: &mut Scene,
        nodes: &[Node],
        stores: &Stores,
        palette: &Palette,
        config: &scrollbar::Config,
        (now, interval): (Instant, Duration),
    ) {
        // Only framework decorations occupy the trailing ranges.
        if let Some(first) = scene
            .layers
            .iter()
            .position(|l| l.id.starts_with("@scrollbar:"))
        {
            scene.prepared.clear();
            let start = scene.layers[first].own.start;
            scene.primitives.truncate(start);
            scene.clips.retain(|i, _| *i < start);
            scene.layers.truncate(first);
        }
        self.deadline = None;
        self.dirty = false;
        let fade_ms = if bead_core::animation::reduced_motion() {
            0.
        } else {
            config.fade_ms
        };
        for node in nodes
            .iter()
            .filter(|n| n.scrollbar && n.scroll_store.is_some())
        {
            let Some(Value::Number(offset)) =
                node.scroll_store.as_ref().and_then(|s| stores.get(s))
            else {
                continue;
            };
            let viewport = rect(&node.draw);
            let Some(mut g) =
                scrollbar::geometry(viewport, *offset, node.scroll_limit.unwrap_or(0.), config)
            else {
                continue;
            };
            let hovered = self
                .pointer
                .is_some_and(|(x, y)| node.contains(x, y) && g.hit.hit_test(x, y, None, None));
            let captured = self.capture.as_ref().is_some_and(|(id, _)| id == &node.id);
            let held = hovered || captured;
            let activity = self.activity.get(&node.id);
            let entrance = activity.map_or(1., |a| {
                if fade_ms <= 0. {
                    1.
                } else {
                    (now.saturating_duration_since(a.shown).as_secs_f32() * 1000. / fade_ms).min(1.)
                }
            });
            let opacity = if held {
                if captured { 1. } else { entrance }
            } else if let Some(a) = activity {
                let age = now.saturating_duration_since(a.last).as_secs_f32() * 1000.;
                if fade_ms <= 0. {
                    if age < config.auto_hide_ms { 1. } else { 0. }
                } else {
                    entrance * (1. - ((age - config.auto_hide_ms) / fade_ms).clamp(0., 1.))
                }
            } else {
                0.
            };
            if held && entrance < 1. && !captured {
                let due = now + interval;
                self.deadline = Some(self.deadline.map_or(due, |old| old.min(due)));
            }
            if let Some(a) = activity.filter(|_| !held) {
                let end = a.last + Duration::from_secs_f32((config.auto_hide_ms + fade_ms) / 1000.);
                if now < end {
                    let entering =
                        now.saturating_duration_since(a.shown).as_secs_f32() * 1000. < fade_ms;
                    let due = if entering
                        || now >= a.last + Duration::from_secs_f32(config.auto_hide_ms / 1000.)
                    {
                        now + interval
                    } else {
                        a.last + Duration::from_secs_f32(config.auto_hide_ms / 1000.)
                    };
                    self.deadline = Some(self.deadline.map_or(due, |old| old.min(due)));
                }
            }
            if opacity <= 0. {
                continue;
            }
            if held {
                g.thumb.x -= config.hover_width - config.width;
                g.thumb.w = config.hover_width;
            }
            let mut color = config.paint_color(held, palette);
            color[3] = (f32::from(color[3]) * opacity).round() as u8;
            scene.prepared.clear();
            let index = scene.primitives.len();
            scene.primitives.push(Primitive::Rounded {
                rect: g.thumb,
                radius: config.radius,
                fill: color,
                border: None,
            });
            scene
                .clips
                .insert(index, node.clip.map_or(viewport, |c| c.intersect(viewport)));
            scene.layers.push(SceneLayer {
                id: format!("@scrollbar:{}", node.id),
                parent: None,
                own: index..index + 1,
                subtree_end: index + 1,
                bounds: g.thumb,
            });
        }
        self.activity.retain(|id, a| {
            nodes.iter().any(|n| {
                n.scrollbar && n.scroll_limit.is_some_and(|limit| limit > 0.) && &n.id == id
            }) && now.saturating_duration_since(a.last).as_secs_f32() * 1000.
                < (config.auto_hide_ms + fade_ms)
        });
        if self.activity.is_empty() {
            self.activity = BTreeMap::new();
        }
    }
}

impl Session {
    /// Scroll keys target the focused viewport, or the pointer viewport with no focus.
    /// Editing inputs retain their own cursor navigation.
    pub fn scroll_key(
        &mut self,
        command: &str,
        shift: bool,
        pointer: Option<(f32, f32)>,
    ) -> Result<Option<Scene>, Diagnostic> {
        if matches!(command, "home" | "end" | "up" | "down") && self.focused_input().is_some() {
            return match command {
                "home" => self.edit_key(crate::input::EditKey::Home, shift),
                "end" => self.edit_key(crate::input::EditKey::End, shift),
                "up" | "down" => self.input_vertical(command == "up", shift),
                _ => Ok(None),
            };
        }
        let focused = match self.stores.get("bead.focused") {
            Some(Value::Text(id)) if !id.is_empty() => Some(id.as_str()),
            _ => None,
        };
        let mut ancestor = focused;
        let mut focused_region = None;
        while let Some(id) = ancestor {
            if let Some(node) = self
                .current_nodes
                .iter()
                .find(|n| n.scrollable && n.id == id)
            {
                focused_region = Some(node);
                break;
            }
            ancestor = self
                .current
                .layers
                .iter()
                .find(|l| l.id == id)
                .and_then(|l| l.parent.as_deref());
        }
        let node = focused_region.or_else(|| {
            pointer.and_then(|(x, y)| {
                self.current_nodes
                    .iter()
                    .rev()
                    .find(|n| n.scrollable && n.contains(x, y))
            })
        });
        let Some(node) = node else { return Ok(None) };
        let Some(store) = &node.scroll_store else {
            return Ok(None);
        };
        let Some(Value::Number(offset)) = self.stores.get(store) else {
            return Ok(None);
        };
        let limit = node.scroll_limit.unwrap_or(0.);
        let line = f64::from(self.scrollbar_config.line_step.max(1.));
        let page = (f64::from(rect(&node.draw).h) - f64::from(self.scrollbar_config.page_overlap))
            .max(line);
        let base = self
            .keyboard_scroll
            .as_ref()
            .filter(|m| &m.store == store)
            .map_or(*offset, |m| m.target);
        let target = match command {
            "home" => 0.,
            "end" => limit,
            "up" => base - line,
            "down" => base + line,
            "page-up" => base - page,
            "page-down" => base + page,
            _ => return Ok(None),
        }
        .clamp(0., limit);
        let now = Instant::now();
        if bead_core::animation::reduced_motion() {
            let store = store.clone();
            self.keyboard_scroll = None;
            return self.update(BTreeMap::from([(store, Value::Number(target))]));
        }
        self.keyboard_scroll = Some(KeyboardMotion {
            store: store.clone(),
            started: now,
            from: *offset,
            target,
            next: now,
        });
        self.update(BTreeMap::new())
    }
    pub(super) fn scrollbar_hit(&self, x: f32, y: f32) -> Option<(String, scrollbar::Geometry)> {
        self.current_nodes
            .iter()
            .rev()
            .filter(|n| n.scrollbar && n.contains(x, y))
            .find_map(|n| {
                let Value::Number(offset) = self.stores.get(n.scroll_store.as_ref()?)? else {
                    return None;
                };
                let g = scrollbar::geometry(
                    rect(&n.draw),
                    *offset,
                    n.scroll_limit?,
                    &self.scrollbar_config,
                )?;
                g.hit.hit_test(x, y, None, None).then(|| (n.id.clone(), g))
            })
    }
    pub(super) fn scrollbar_press(
        &mut self,
        x: f32,
        y: f32,
    ) -> Result<Option<Option<Scene>>, Diagnostic> {
        self.keyboard_scroll = None;
        self.scroll_feedback.pointer = Some((x, y));
        let Some((id, g)) = self.scrollbar_hit(x, y) else {
            return Ok(None);
        };
        let node = self.current_nodes.iter().find(|n| n.id == id).unwrap();
        let store = node.scroll_store.clone().unwrap();
        let Value::Number(offset) = self.stores.get(&store).unwrap() else {
            return Ok(None);
        };
        let target = scrollbar::page_target(
            *offset,
            node.scroll_limit.unwrap_or(0.),
            rect(&node.draw).h,
            &g,
            y,
        );
        if y >= g.thumb.y && y <= g.thumb.y + g.thumb.h {
            self.scroll_feedback.capture = Some((id.clone(), y - g.thumb.y));
        }
        self.scroll_feedback
            .reveal(&id, Instant::now(), &self.scrollbar_config);
        self.update(BTreeMap::from([(store, Value::Number(target))]))
            .map(Some)
    }
    pub(super) fn scrollbar_drag(
        &mut self,
        x: f32,
        y: f32,
    ) -> Result<Option<Option<Scene>>, Diagnostic> {
        self.scroll_feedback.pointer = Some((x, y));
        let Some((id, grab)) = self.scroll_feedback.capture.clone() else {
            return Ok(None);
        };
        let Some(node) = self.current_nodes.iter().find(|n| n.id == id) else {
            self.scroll_feedback.capture = None;
            return Ok(None);
        };
        let store = node.scroll_store.clone().unwrap();
        let limit = node.scroll_limit.unwrap_or(0.);
        let viewport = rect(&node.draw);
        let Some(g) = scrollbar::geometry(viewport, 0., limit, &self.scrollbar_config) else {
            return Ok(None);
        };
        let target = scrollbar::offset_at_thumb_start(viewport, limit, &g, y - grab);
        self.scroll_feedback
            .reveal(&id, Instant::now(), &self.scrollbar_config);
        self.update(BTreeMap::from([(store, Value::Number(target))]))
            .map(Some)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::Fixture;

    fn session() -> (Fixture, Session) {
        let f = Fixture::new();
        std::fs::create_dir_all(f.0.join("ui")).unwrap();
        std::fs::write(
            f.0.join("ui/behavior.luau"),
            "return {stores={offset=0,text='hello'},actions={noop=function()end},computed={}}",
        )
        .unwrap();
        std::fs::write(f.0.join("ui/main.luau"),r#"local ui=require('bead.ui')
return ui.Panel{id='panel',width=200,height=120,ui.Scroll{id='list',width=200,height=100,scroll=ui.bind('offset'),ui.Column{height=1000,shrink=0,ui.TextInput{id='edit',height=30,text=ui.bind('text')},ui.Button{id='button',height=30,text='row'}}},ui.Button{id='outside',position='absolute',left=0,top=100,width=200,height=20,text='Outside',on_activate=ui.action('noop')}}"#).unwrap();
        let mut s = f.session();
        s.entry = "ui/main.luau".into();
        let scene = s.reload().unwrap();
        s.commit(scene);
        (f, s)
    }

    #[test]
    fn controlled_clock_fades_then_hides_but_hover_and_capture_hold() {
        let (_f, s) = session();
        let node = s.current_nodes.iter().find(|n| n.scrollable).unwrap();
        let mut feedback = Feedback::default();
        let now = Instant::now();
        let config = &s.scrollbar_config;
        feedback.reveal(&node.id, now, config);
        let mut scene = s.current.clone();
        let mut paint = |feedback: &mut Feedback, time| {
            feedback.paint(
                &mut scene,
                &s.current_nodes,
                &s.stores,
                &s.palette,
                config,
                (time, Duration::from_nanos(8_333_333)),
            );
            scene
                .layers
                .iter()
                .filter(|l| l.id.starts_with("@scrollbar:"))
                .count()
        };
        assert_eq!(paint(&mut feedback, now), 0);
        assert_eq!(paint(&mut feedback, now + Duration::from_millis(200)), 1);
        assert_eq!(paint(&mut feedback, now + Duration::from_millis(1100)), 1);
        assert_eq!(paint(&mut feedback, now + Duration::from_millis(1200)), 0);
        assert!(feedback.deadline.is_none());
        feedback.pointer = Some((195., 40.));
        assert_eq!(paint(&mut feedback, now + Duration::from_secs(10)), 1);
        feedback.pointer = None;
        feedback.capture = Some((node.id.clone(), 0.));
        assert_eq!(paint(&mut feedback, now + Duration::from_secs(20)), 1);
    }

    #[test]
    fn near_deadline_model_frame_reaches_feedback_before_the_next_refresh() {
        let (_f, mut s) = session();
        let now = Instant::now();
        let id = s
            .current_nodes
            .iter()
            .find(|n| n.scrollable)
            .unwrap()
            .id
            .clone();
        // A wide synthetic interval keeps the eligibility assertion independent
        // of thread scheduling; no clock sleeps are involved.
        s.set_frame_interval(Duration::from_secs(1));
        s.scroll_feedback.reveal(&id, now, &s.scrollbar_config);
        s.scroll_feedback.dirty = false;
        s.scroll_feedback.deadline = Some(now + Duration::from_millis(200));
        assert!(s.frame_due(now + Duration::from_millis(200), now));
        assert!(s.update_model(BTreeMap::new()).unwrap().is_some());
    }

    #[test]
    fn thumb_drag_captures_outside_and_track_click_pages_without_content_activation() {
        let (_f, mut s) = session();
        let node = s.current_nodes.iter().find(|n| n.scrollable).unwrap();
        let id = node.id.clone();
        let viewport = rect(&node.draw);
        let limit = node.scroll_limit.unwrap();
        let g = scrollbar::geometry(viewport, 0., limit, &s.scrollbar_config).unwrap();
        let scene = s.click(g.thumb.x + 1., g.thumb.y + 1.).unwrap().unwrap();
        s.commit(scene);
        assert_eq!(s.scroll_feedback.capture.as_ref().unwrap().0, id);
        let scene = s.drag(-100., g.track.y + g.track.h + 1.).unwrap().unwrap();
        s.commit(scene);
        assert_eq!(s.stores.get("offset"), Some(&Value::Number(limit)));
        let scene = s.release().unwrap().unwrap();
        s.commit(scene);
        assert!(s.scroll_feedback.capture.is_none());
        let scene = s.click(g.track.x + 1., g.track.y + 1.).unwrap().unwrap();
        s.commit(scene);
        assert!(matches!(s.stores.get("offset"),Some(Value::Number(v)) if *v<limit));
    }

    #[test]
    fn zero_fade_is_visible_immediately_and_hides_at_the_idle_deadline() {
        let (_f, s) = session();
        let node = s.current_nodes.iter().find(|n| n.scrollable).unwrap();
        let config = scrollbar::Config {
            fade_ms: 0.,
            ..s.scrollbar_config.clone()
        };
        let mut feedback = Feedback::default();
        let now = Instant::now();
        feedback.reveal(&node.id, now, &config);
        let mut scene = s.current.clone();
        feedback.paint(
            &mut scene,
            &s.current_nodes,
            &s.stores,
            &s.palette,
            &config,
            (now, Duration::from_millis(8)),
        );
        assert!(scene.layers.iter().any(|l| l.id.starts_with("@scrollbar:")));
        feedback.paint(
            &mut scene,
            &s.current_nodes,
            &s.stores,
            &s.palette,
            &config,
            (now + Duration::from_secs(1), Duration::from_millis(8)),
        );
        assert!(!scene.layers.iter().any(|l| l.id.starts_with("@scrollbar:")));
        assert!(feedback.activity.is_empty() && feedback.deadline.is_none());
    }

    #[test]
    fn programmatic_scroll_reveals_the_bar_and_opt_out_has_no_activity_entry() {
        let (f, mut s) = session();
        let scene = s
            .update(BTreeMap::from([("offset".into(), Value::Number(100.))]))
            .unwrap()
            .unwrap();
        s.commit(scene);
        assert_eq!(s.scroll_feedback.activity.len(), 1);
        let path = f.0.join("ui/main.luau");
        let view = std::fs::read_to_string(&path).unwrap().replacen(
            "ui.Scroll{",
            "ui.Scroll{scrollbar=false,",
            1,
        );
        std::fs::write(path, view).unwrap();
        let scene = s.reload().unwrap();
        s.commit(scene);
        let scene = s.scroll(10., 50., 100.).unwrap().unwrap();
        assert!(!scene.layers.iter().any(|l| l.id.starts_with("@scrollbar:")));
        assert!(s.scroll_feedback.activity.is_empty());
    }

    #[test]
    fn virtual_list_thumb_uses_all_thousand_items_without_materializing_them() {
        for loaded in [1000, 16] {
            let (f, mut s) = session();
            std::fs::write(f.0.join("ui/behavior.luau"),"local rows={};for i=1,COUNT do rows[i]={id=tostring(i)} end; return {stores={offset=0,rows=rows},actions={},computed={}}".replace("COUNT",&loaded.to_string())).unwrap();
            std::fs::write(f.0.join("ui/main.luau"),r#"local ui=require('bead.ui');local list=require('bead.widgets.kit.virtual-list')
return list{id='list',width=200,height=100,row_height=32,gap=0,total_count=1000,scroll=ui.bind('offset'),model=ui.bind('rows'),key='id',render=function(item)return ui.Text{id='row',height=32,text=item.id}end}"#).unwrap();
            let scene = s.reload().unwrap();
            s.commit(scene);
            assert!(s.current.primitives.len() < 10);
            let config = scrollbar::Config {
                minimum_thumb: 0.,
                ..s.scrollbar_config.clone()
            };
            for offset in [0., 15950., 31900.] {
                if let Some(scene) = s
                    .update(BTreeMap::from([("offset".into(), Value::Number(offset))]))
                    .unwrap()
                {
                    s.commit(scene);
                }
                let node = s.current_nodes.iter().find(|n| n.scrollable).unwrap();
                assert_eq!(node.scroll_limit, Some(31900.));
                let g = scrollbar::geometry(rect(&node.draw), offset, 31900., &config).unwrap();
                assert!((g.thumb.h - g.track.h * 100. / 32000.).abs() < 0.001);
                let fraction = (g.thumb.y - g.track.y) / (g.track.h - g.thumb.h);
                assert!((f64::from(fraction) - offset / 31900.).abs() < 0.0001);
                assert!(s.current.primitives.len() < 11);
            }
        }
    }

    fn finish_key(s: &mut Session) {
        s.keyboard_scroll.as_mut().unwrap().started = Instant::now() - Duration::from_secs(1);
        let scene = s.update(BTreeMap::new()).unwrap().unwrap();
        s.commit(scene);
    }

    #[test]
    fn keyboard_pages_home_end_and_focused_input_keeps_cursor_keys() {
        let (_f, mut s) = session();
        let scene = s
            .scroll_key("page-down", false, Some((10., 60.)))
            .unwrap()
            .unwrap();
        s.commit(scene);
        finish_key(&mut s);
        assert_eq!(s.stores.get("offset"), Some(&Value::Number(76.)));
        let scene = s
            .scroll_key("end", false, Some((10., 60.)))
            .unwrap()
            .unwrap();
        s.commit(scene);
        finish_key(&mut s);
        assert_eq!(s.stores.get("offset"), Some(&Value::Number(900.)));
        let scene = s
            .scroll_key("home", false, Some((10., 60.)))
            .unwrap()
            .unwrap();
        s.commit(scene);
        finish_key(&mut s);
        assert_eq!(s.stores.get("offset"), Some(&Value::Number(0.)));
        let id = s
            .current_nodes
            .iter()
            .find(|n| n.input.is_some())
            .unwrap()
            .id
            .clone();
        let scene = s.focus_id(&id).unwrap().unwrap();
        s.commit(scene);
        let scene = s
            .scroll_key("home", false, Some((10., 60.)))
            .unwrap()
            .unwrap();
        s.commit(scene);
        assert_eq!(s.editors[&id].cursor, 0);
        assert!(s.keyboard_scroll.is_none());
        let scene = s
            .scroll_key("end", false, Some((10., 60.)))
            .unwrap()
            .unwrap();
        s.commit(scene);
        assert_eq!(s.editors[&id].cursor, 5);
        assert_eq!(s.stores.get("offset"), Some(&Value::Number(0.)));
        let scene = s.scroll_key("page-down", false, None).unwrap().unwrap();
        s.commit(scene);
        finish_key(&mut s);
        assert_eq!(s.stores.get("offset"), Some(&Value::Number(76.)));
        assert_eq!(s.editors[&id].cursor, 5);
    }
    #[test]
    fn focus_outside_viewport_falls_back_to_pointer_and_line_keys_retarget() {
        let (_f, mut s) = session();
        let outside = s
            .current_nodes
            .iter()
            .find(|n| n.id.ends_with("outside"))
            .unwrap()
            .id
            .clone();
        let scene = s.focus_id(&outside).unwrap().unwrap();
        s.commit(scene);
        for (command, expected) in [
            ("down", 24.),
            ("down", 48.),
            ("up", 24.),
            ("page-down", 100.),
            ("page-up", 24.),
        ] {
            let scene = s
                .scroll_key(command, false, Some((10., 60.)))
                .unwrap()
                .unwrap();
            s.commit(scene);
            finish_key(&mut s);
            assert_eq!(s.stores.get("offset"), Some(&Value::Number(expected)));
        }
    }
}
