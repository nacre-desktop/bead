// SPDX-License-Identifier: MIT OR Apache-2.0
//! Hit testing, focus, range controls, Unicode editing and accessibility actions.
use super::*;
pub(super) fn deferred_matches(nodes: &[Node], pending: &super::session::DeferredAction) -> bool {
    let (x, y) = pending.pointer;
    let inside =
        |rect: Rect| x >= rect.x && y >= rect.y && x < rect.x + rect.w && y < rect.y + rect.h;
    nodes
        .iter()
        .rev()
        .find(|node| {
            let rect = rect(&node.draw);
            node.focusable && inside(rect) && node.clip.is_none_or(inside)
        })
        .is_some_and(|node| {
            node.activate_on_release
                && node.id == pending.id
                && node.action.as_deref() == Some(pending.action.as_str())
                && node.action_arg.clone().unwrap_or(Value::Bool(true)) == pending.argument
        })
}
impl Session {
    /// Evaluate a declared behavior action for a host key binding. The host can
    /// consume effect requests before staging the resulting values with `update`.
    pub fn action_values(
        &self,
        name: &str,
        arg: Value,
    ) -> Result<BTreeMap<String, Value>, Diagnostic> {
        let Some(generation) = &self.generation else {
            return Ok(BTreeMap::new());
        };
        Ok(generation
            .script
            .action(name, &self.stores, arg)?
            .values()
            .clone())
    }
    fn deferred_valid(&self) -> Option<(String, String, Value)> {
        if self.stores.get("bead.pressed") != Some(&Value::Bool(true)) {
            return None;
        }
        let pending = self.deferred_action.as_ref()?;
        deferred_matches(&self.current_nodes, pending).then(|| {
            (
                pending.id.clone(),
                pending.action.clone(),
                pending.argument.clone(),
            )
        })
    }
    pub(super) fn cancel_invalid_deferred(&mut self) {
        if self.deferred_action.is_some() && self.deferred_valid().is_none() {
            self.deferred_action = None;
        }
    }
    fn track_deferred_pointer(&mut self, x: f32, y: f32) -> bool {
        let was_pending = self.deferred_action.is_some();
        if let Some(pending) = &mut self.deferred_action {
            pending.pointer = (x, y);
        }
        self.cancel_invalid_deferred();
        was_pending && self.deferred_action.is_none()
    }

    pub(super) fn hit(&self, x: f32, y: f32, scroll: bool) -> Option<&Node> {
        let generation = self.generation.as_ref()?;
        if generation.markup_entry.is_none() {
            return generation.hit(&self.stores, x, y);
        }
        self.current_nodes.iter().rev().find(|node| {
            let r = rect(&node.draw);
            let inside = |r: Rect| x >= r.x && y >= r.y && x < r.x + r.w && y < r.y + r.h;
            inside(r)
                && node.clip.is_none_or(inside)
                && if scroll {
                    node.scroll_store.is_some()
                } else {
                    node.focusable
                }
        })
    }
    /// Pointer press: the hit control's action sees `bead.pressed` false (a new gesture);
    /// every drag step that follows sees it true until [`Session::release`].
    pub fn click(&mut self, x: f32, y: f32) -> Result<Option<Scene>, Diagnostic> {
        self.click_at(x, y, true, true)
    }
    pub fn release(&mut self) -> Result<Option<Scene>, Diagnostic> {
        let deferred = self.deferred_valid();
        self.range_grab = None;
        if self.preview_deadline.is_some() {
            self.preview_deadline =
                Some(std::time::Instant::now() + std::time::Duration::from_millis(750));
        }
        self.deferred_action = None;
        self.deferred_gesture = false;
        let active = self
            .pending
            .as_ref()
            .filter(|pending| pending.generation.is_none())
            .map_or(&self.stores, |pending| &pending.stores);
        if active.get("bead.pressed") != Some(&Value::Bool(true)) && deferred.is_none() {
            return Ok(None);
        }
        let Some(generation) = &self.generation else {
            return Ok(None);
        };
        // A host may flush the last deferred range action in this same input
        // transaction. Preserve its candidate values when releasing the gesture.
        let mut stores = self
            .pending
            .as_ref()
            .filter(|pending| pending.generation.is_none())
            .map_or_else(|| self.stores.clone(), |pending| pending.stores.clone());
        // Legacy stepped feedback settles on release. A source-acknowledged
        // range keeps the exact pointer target until its final write converges.
        let released_value = stores.get("bead.focused").and_then(|focused| {
            let Value::Text(id) = focused else {
                return None;
            };
            self.current_nodes
                .iter()
                .find(|node| {
                    &node.id == id
                        && !node.preview_key.is_empty()
                        && node.preview_source.is_none()
                        && stores.get("bead.preview.key")
                            == Some(&Value::Text(node.preview_key.clone()))
                })
                .map(|node| node.preview_value)
        });
        if let Some(value) = released_value {
            stores
                .commit(BTreeMap::from([(
                    "bead.preview.value".into(),
                    Value::Number(f64::from(value)),
                )]))
                .map_err(|e| generation.script.error(e))?;
        }
        if stores.get("bead.pressed").is_some() {
            stores
                .commit(BTreeMap::from([(
                    "bead.pressed".into(),
                    Value::Bool(false),
                )]))
                .map_err(|e| generation.script.error(e))?;
        }
        if let Some((id, action, argument)) = deferred {
            let focus_on_pointer = self
                .current_nodes
                .iter()
                .find(|node| node.id == id)
                .is_none_or(|node| node.focus_on_pointer);
            if focus_on_pointer {
                stores
                    .commit(BTreeMap::from([("bead.focused".into(), Value::Text(id))]))
                    .map_err(|e| generation.script.error(e))?;
            }
            stores = generation.script.action(&action, &stores, argument)?;
        }
        self.stage_state(stores)
    }
    fn click_at(
        &mut self,
        x: f32,
        y: f32,
        press: bool,
        continuous: bool,
    ) -> Result<Option<Scene>, Diagnostic> {
        let preview = self
            .hit(x, y, false)
            .map(|node| self.preview_position(node, continuous.then_some(x)));
        let changes = preview.map_or_else(BTreeMap::new, |(key, value)| {
            self.preview_changes(key, value)
        });
        if press {
            self.capture_pointer(x, y);
            self.deferred_action = None;
            self.deferred_gesture = false;
        }
        let Some(generation) = &self.generation else {
            return Ok(None);
        };
        let hit = self.hit(x, y, false);
        let deferred = hit.is_some_and(|node| node.activate_on_release && node.action.is_some());
        let preserve_focus = press && hit.is_some_and(|node| !node.focus_on_pointer);
        let pointer_id = if continuous {
            hit.filter(|node| node.pointer_arg.is_some())
                .map(|node| node.id.clone())
        } else {
            None
        };
        let (action, focused, deferred_action, action_arg) =
            hit.map_or((None, String::new(), None, Value::Bool(true)), |node| {
                (
                    if deferred { None } else { node.action.clone() },
                    if !deferred && (node.input.is_some() || node.focusable) {
                        node.id.clone()
                    } else {
                        String::new()
                    },
                    deferred.then(|| {
                        (
                            node.id.clone(),
                            node.action.clone().unwrap(),
                            node.action_arg.clone().unwrap_or(Value::Bool(true)),
                        )
                    }),
                    node.action_arg.clone().unwrap_or(Value::Bool(true)),
                )
            });
        let mut stores = self.stores.clone();
        if (press || !continuous) && self.stores.get("bead.focus_visible").is_some() {
            stores
                .commit(BTreeMap::from([(
                    "bead.focus_visible".into(),
                    Value::Bool(!continuous),
                )]))
                .map_err(|e| generation.script.error(e))?;
        }
        stores
            .commit(changes)
            .map_err(|e| generation.script.error(e))?;
        // A press can arrive before its preceding hover frame was committed.
        if press
            && self.stores.get("bead.hovered").is_some()
            && let Some((id, _, _)) = &deferred_action
        {
            stores
                .commit(BTreeMap::from([(
                    "bead.hovered".into(),
                    Value::Text(id.clone()),
                )]))
                .map_err(|e| generation.script.error(e))?;
        }
        if !deferred && !preserve_focus {
            stores
                .commit(BTreeMap::from([(
                    "bead.focused".into(),
                    Value::Text(focused),
                )]))
                .map_err(|e| generation.script.error(e))?;
        }
        if press && let Some((id, action, argument)) = deferred_action {
            self.deferred_gesture = true;
            self.deferred_action = Some(super::session::DeferredAction {
                id,
                action,
                argument,
                pointer: (x, y),
            });
        }
        if let Some(action) = action {
            // The view owns argument formatting (including records and strings).
            // Re-evaluate it against this event's preview, never the previous frame.
            let action_arg = if let Some(id) = pointer_id {
                let (_, nodes) = generation.materialize(&stores, &self.palette, self.viewport)?;
                nodes
                    .iter()
                    .find(|node| node.id == id)
                    .and_then(|node| node.pointer_arg.clone())
                    .unwrap_or(action_arg)
            } else {
                action_arg
            };
            stores = generation.script.action(&action, &stores, action_arg)?;
        }
        if press && self.stores.get("bead.pressed").is_some() {
            stores
                .commit(BTreeMap::from([("bead.pressed".into(), Value::Bool(true))]))
                .map_err(|e| generation.script.error(e))?;
        }
        self.stage_state(stores)
    }
    /// Clipped content-scroll regions, excluding duplicate descendants of a viewport.
    pub fn scroll_regions(&self) -> Vec<bead_render::Rect> {
        let mut regions: Vec<bead_render::Rect> = Vec::new();
        for node in self
            .current_nodes
            .iter()
            .filter(|node| node.scroll_store.is_some())
        {
            let bounds = rect(&node.draw);
            let bounds = node.clip.map_or(bounds, |clip| bounds.intersect(clip));
            if bounds.w > 0.
                && bounds.h > 0.
                && !regions.iter().any(|r| {
                    bounds.x >= r.x
                        && bounds.y >= r.y
                        && bounds.x + bounds.w <= r.x + r.w
                        && bounds.y + bounds.h <= r.y + r.h
                })
            {
                regions.push(bounds);
            }
        }
        regions
    }
    pub fn scroll(&mut self, x: f32, y: f32, delta: f64) -> Result<Option<Scene>, Diagnostic> {
        let Some(generation) = &self.generation else {
            return Ok(None);
        };
        let Some(node) = self.hit(x, y, true) else {
            return Ok(None);
        };
        let stores = if let Some(id) = &node.scroll_store {
            let Some(Value::Number(value)) = self.stores.get(id) else {
                return Err(node.diagnostic("scroll store not a number"));
            };
            let mut stores = self.stores.clone();
            stores
                .commit(BTreeMap::from([(
                    id.clone(),
                    Value::Number((value + delta).clamp(0., node.scroll_limit.unwrap_or(f64::MAX))),
                )]))
                .map_err(|e| node.diagnostic(e))?;
            stores
        } else if let Some(action) = &node.scroll {
            generation
                .script
                .action(action, &self.stores, Value::Number(delta))?
        } else {
            return Ok(None);
        };
        self.stage_state(stores)
    }
    pub fn drag(&mut self, x: f32, y: f32) -> Result<Option<Scene>, Diagnostic> {
        let deferred_gesture = self.deferred_gesture;
        let canceled = self.track_deferred_pointer(x, y);
        if deferred_gesture {
            let hovered = self
                .hit(x, y, false)
                .map_or(String::new(), |node| node.id.clone());
            let mut changes = BTreeMap::from([("bead.hovered".into(), Value::Text(hovered))]);
            if canceled && self.stores.get("bead.pressed") == Some(&Value::Bool(true)) {
                changes.insert("bead.pressed".into(), Value::Bool(false));
            }
            return self.update(changes);
        }
        if canceled && self.stores.get("bead.pressed") == Some(&Value::Bool(true)) {
            return self.update(BTreeMap::from([(
                "bead.pressed".into(),
                Value::Bool(false),
            )]));
        }
        let Some(current) = self.drag_node() else {
            return Ok(None);
        };
        let range = !current.preview_key.is_empty();
        let Some(target) = self.drag_target(current, x, y) else {
            return Ok(None);
        };
        if !range {
            return self.click_at(x, y, false, true);
        }
        let r = rect(&target.draw);
        let px = if x >= r.x && x < r.x + r.w {
            x
        } else {
            r.x + r.w / 2.
        };
        self.click_at(px, r.y + r.h / 2., false, true)
    }
    pub fn step_range(&mut self, back: bool) -> Result<Option<Scene>, Diagnostic> {
        let Some(Value::Text(focused)) = self.stores.get("bead.focused") else {
            return Ok(None);
        };
        let Some(current) = self
            .current_nodes
            .iter()
            .find(|n| &n.id == focused && n.drag)
        else {
            return Ok(None);
        };
        let peers: Vec<_> = self
            .current_nodes
            .iter()
            .filter(|n| {
                n.drag
                    && n.id.rsplit_once('.').map(|(p, _)| p)
                        == current.id.rsplit_once('.').map(|(p, _)| p)
            })
            .collect();
        let index = peers
            .iter()
            .position(|n| n.id == current.id)
            .ok_or_else(|| Diagnostic::new(&self.entry, 1, 1, "missing range focus"))?;
        let next = if back {
            index.saturating_sub(1)
        } else {
            (index + 1).min(peers.len() - 1)
        };
        let r = rect(&peers[next].draw);
        self.click_at(r.x + r.w / 2., r.y + r.h / 2., false, false)
    }
    pub fn focused_range(&self) -> bool {
        matches!(self.stores.get("bead.focused"),Some(Value::Text(id)) if self.current_nodes.iter().any(|n|n.drag && &n.id==id))
    }
    pub fn hover(&mut self, x: f32, y: f32) -> Result<Option<Scene>, Diagnostic> {
        let canceled = self.track_deferred_pointer(x, y);
        if self.stores.get("bead.hovered").is_none() {
            return if canceled {
                self.update(BTreeMap::from([(
                    "bead.pressed".into(),
                    Value::Bool(false),
                )]))
            } else {
                Ok(None)
            };
        }
        let id = self
            .hit(x, y, false)
            .map_or(String::new(), |node| node.id.clone());
        let mut changes: BTreeMap<String, Value> =
            BTreeMap::from([("bead.hovered".into(), Value::Text(id))]);
        if canceled {
            changes.insert("bead.pressed".into(), Value::Bool(false));
        }
        if !canceled
            && changes
                .iter()
                .all(|(id, value)| self.stores.get(id) == Some(value))
        {
            return Ok(None);
        }
        self.update(changes)
    }
    pub fn focus_next(&mut self, backward: bool) -> Result<Option<Scene>, Diagnostic> {
        let ids: Vec<_> = self
            .current_nodes
            .iter()
            .filter(|n| n.focusable || n.input.is_some())
            .map(|n| n.id.clone())
            .collect();
        if ids.is_empty() {
            return Ok(None);
        }
        let current = self.stores.get("bead.focused");
        let index = ids
            .iter()
            .position(|id| current == Some(&Value::Text(id.clone())));
        let next = match index {
            None => {
                if backward {
                    ids.len() - 1
                } else {
                    0
                }
            }
            Some(i) => {
                if backward {
                    (i + ids.len() - 1) % ids.len()
                } else {
                    (i + 1) % ids.len()
                }
            }
        };
        self.focus_id(&ids[next])
    }
    pub fn focus_id(&mut self, id: &str) -> Result<Option<Scene>, Diagnostic> {
        self.focus_with_visibility(id, true, false, None, false)
    }
    /// Pointer focus retains keyboard routing without painting a keyboard focus ring.
    fn preview_position(&self, node: &Node, x: Option<f32>) -> (String, f32) {
        let mut value = node.preview_value;
        if !node.preview_key.is_empty()
            && let Some(x) = x
        {
            let peers = self
                .current_nodes
                .iter()
                .filter(|peer| peer.preview_key == node.preview_key);
            let mut left = f32::INFINITY;
            let mut right = f32::NEG_INFINITY;
            let mut low = f32::INFINITY;
            let mut high = f32::NEG_INFINITY;
            for peer in peers {
                let r = rect(&peer.draw);
                left = left.min(r.x);
                right = right.max(r.x + r.w);
                low = low.min(peer.preview_value);
                high = high.max(peer.preview_value);
            }
            let span = right - left - 2. * node.preview_inset;
            if span > 0. && high > low {
                value = low + ((x - left - node.preview_inset) / span).clamp(0., 1.) * (high - low);
            }
        }
        (node.preview_key.clone(), value)
    }
    pub fn has_preview(&self, id: &str) -> bool {
        self.current_nodes
            .iter()
            .any(|node| node.id == id && !node.preview_key.is_empty())
    }
    /// Argument authored by the view after staging this pointer event's feedback.
    /// Keyboard actions continue to use the node's discrete `action_arg`.
    pub fn pointer_argument(&self, id: &str) -> Option<Value> {
        self.pending
            .as_ref()
            .map_or(self.current_nodes.as_slice(), |pending| &pending.nodes)
            .iter()
            .find(|node| node.id == id)
            .and_then(|node| node.pointer_arg.clone())
    }
    pub fn pointer_focus_id(
        &mut self,
        id: &str,
        press: bool,
        x: Option<f32>,
    ) -> Result<Option<Scene>, Diagnostic> {
        self.focus_with_visibility(id, false, press, x, true)
    }
    /// Focus a keyboard-activated range step without waiting for its service echo.
    pub fn range_step_id(&mut self, id: &str) -> Result<Option<Scene>, Diagnostic> {
        self.focus_with_visibility(id, true, false, None, true)
    }
    fn focus_with_visibility(
        &mut self,
        id: &str,
        visible: bool,
        press: bool,
        x: Option<f32>,
        preview: bool,
    ) -> Result<Option<Scene>, Diagnostic> {
        if !self
            .current_nodes
            .iter()
            .any(|node| node.id == id && (node.focusable || node.input.is_some()))
        {
            return Err(Diagnostic::new(
                &self.entry,
                1,
                1,
                "focus target is not visible/focusable",
            ));
        }
        let mut changes = BTreeMap::from([("bead.focused".into(), Value::Text(id.into()))]);
        if preview && let Some(node) = self.current_nodes.iter().find(|node| node.id == id) {
            let (key, value) = self.preview_position(node, x);
            if press {
                self.range_grab = (!key.is_empty() && node.drag).then(|| key.clone());
            }
            if press && !key.is_empty() {
                changes.insert("bead.pressed".into(), Value::Bool(true));
            }
            changes.extend(self.preview_changes(key, value));
        }
        if self.stores.get("bead.focus_visible").is_some() {
            changes.insert("bead.focus_visible".into(), Value::Bool(visible));
        }
        self.update(changes)
    }
    pub fn action_at(&self, x: f32, y: f32) -> Option<(String, String, Value)> {
        let node = self.hit(x, y, false)?;
        Some((
            node.id.clone(),
            node.action.clone()?,
            node.action_arg.clone().unwrap_or(Value::Bool(true)),
        ))
    }
    /// Host action at a press; opted-in buttons defer host routing until release.
    pub fn press_action_at(&self, x: f32, y: f32) -> Option<(String, String, Value)> {
        let node = self.hit(x, y, false)?;
        if node.activate_on_release {
            None
        } else {
            self.action_at(x, y)
        }
    }
    /// Host action recorded by an opted-in press, if release remains over that action.
    pub fn release_action(&self) -> Option<(String, String, Value)> {
        self.deferred_valid()
    }
    pub fn focused_action(&self) -> Option<(String, String, Value)> {
        let Value::Text(focused) = self.stores.get("bead.focused")? else {
            return None;
        };
        let node = self.current_nodes.iter().find(|n| &n.id == focused)?;
        Some((
            node.id.clone(),
            node.action.clone()?,
            node.action_arg.clone().unwrap_or(Value::Bool(true)),
        ))
    }
    /// Resolve a host action without executing a script handler during a range gesture.
    pub fn range_action(&self, back: bool) -> Option<(String, String, Value)> {
        let Value::Text(focused) = self.stores.get("bead.focused")? else {
            return None;
        };
        let current = self
            .current_nodes
            .iter()
            .find(|n| &n.id == focused && n.drag)?;
        let peers: Vec<_> = self
            .current_nodes
            .iter()
            .filter(|n| {
                n.drag
                    && n.id.rsplit_once('.').map(|(p, _)| p)
                        == current.id.rsplit_once('.').map(|(p, _)| p)
            })
            .collect();
        let index = peers.iter().position(|n| n.id == current.id)?;
        let next = if back {
            index.saturating_sub(1)
        } else {
            (index + 1).min(peers.len() - 1)
        };
        let r = rect(&peers[next].draw);
        self.action_at(r.x + r.w / 2., r.y + r.h / 2.)
    }
    /// Record the numeric control owning a press before a host intercepts its action.
    pub fn capture_pointer(&mut self, x: f32, y: f32) {
        self.range_grab = self
            .hit(x, y, false)
            .filter(|node| node.drag && !node.preview_key.is_empty())
            .map(|node| node.preview_key.clone());
    }
    fn drag_node(&self) -> Option<&Node> {
        if let Some(group) = &self.range_grab {
            return self
                .current_nodes
                .iter()
                .find(|node| node.drag && &node.preview_key == group);
        }
        let Value::Text(focused) = self.stores.get("bead.focused")? else {
            return None;
        };
        self.current_nodes
            .iter()
            .find(|node| &node.id == focused && node.drag)
    }
    fn drag_target(&self, current: &Node, x: f32, y: f32) -> Option<&Node> {
        let group = current.id.rsplit_once('.').map(|(parent, _)| parent);
        if !current.preview_key.is_empty()
            && self.range_grab.as_deref() != Some(current.preview_key.as_str())
        {
            return None;
        }
        let peer = |node: &Node| {
            node.drag
                && if current.preview_key.is_empty() {
                    node.id.rsplit_once('.').map(|(parent, _)| parent) == group
                } else {
                    node.preview_key == current.preview_key
                }
        };
        self.hit(x, y, false).filter(|node| peer(node)).or_else(|| {
            if current.preview_key.is_empty() {
                return None;
            }
            // Numeric ranges retain their gesture outside the row and clamp at its ends.
            self.current_nodes
                .iter()
                .filter(|node| peer(node) && node.preview_key == current.preview_key)
                .min_by(|a, b| {
                    let distance = |node: &Node| {
                        let r = rect(&node.draw);
                        (x - r.x - r.w / 2.).abs()
                    };
                    distance(a).total_cmp(&distance(b))
                })
        })
    }
    pub fn drag_action(&self, x: f32, y: f32) -> Option<(String, String, Value)> {
        if self.deferred_gesture {
            return None;
        }
        let current = self.drag_node()?;
        let hit = self.drag_target(current, x, y)?;
        Some((
            hit.id.clone(),
            hit.action.clone()?,
            hit.action_arg.clone().unwrap_or(Value::Bool(true)),
        ))
    }
    pub fn activate_focused(&mut self) -> Result<Option<Scene>, Diagnostic> {
        let Some(Value::Text(focused)) = self.stores.get("bead.focused") else {
            return Ok(None);
        };
        let Some(node) = self.current_nodes.iter().find(|n| &n.id == focused) else {
            return Ok(None);
        };
        let Some(action) = &node.action else {
            return Ok(None);
        };
        let generation = self.generation.as_ref().unwrap();
        let stores = generation.script.action(
            action,
            &self.stores,
            node.action_arg.clone().unwrap_or(Value::Bool(true)),
        )?;
        self.stage_state(stores)
    }
    pub(super) fn focused_input(&self) -> Option<(String, String, String)> {
        let Value::Text(focused) = self.stores.get("bead.focused")? else {
            return None;
        };
        let node = self.current_nodes.iter().find(|n| &n.id == focused)?;
        let store = node.input.as_ref()?;
        let Value::Text(text) = self.stores.get(store)? else {
            return None;
        };
        Some((focused.clone(), store.clone(), text.clone()))
    }
    pub fn edit_key(
        &mut self,
        key: input::EditKey,
        shift: bool,
    ) -> Result<Option<Scene>, Diagnostic> {
        let Some((id, store, mut text)) = self.focused_input() else {
            return Ok(None);
        };
        let editor = self
            .editors
            .entry(id)
            .or_insert_with(|| input::Editor::new(text.len()));
        editor.clamp(&text);
        if matches!(key, input::EditKey::SelectAll) {
            editor.anchor = 0;
            editor.cursor = text.len();
        } else if matches!(key, input::EditKey::Delete) {
            let end = if editor.cursor == editor.anchor {
                input::move_cursor(&text, editor.cursor, input::EditKey::Right)
            } else {
                editor.anchor
            };
            let (new, cursor) = input::replace(&text, editor.cursor, end, "");
            text = new;
            editor.cursor = cursor;
            editor.anchor = cursor;
        } else {
            editor.cursor = input::move_cursor(&text, editor.cursor, key);
            if !shift {
                editor.anchor = editor.cursor;
            }
        }
        self.update(BTreeMap::from([(store, Value::Text(text))]))?
            .map_or_else(|| self.relayout(), |scene| Ok(Some(scene)))
    }
    /// The focused input accepts line breaks (`multiline`).
    pub fn focused_multiline(&self) -> bool {
        self.focused_input()
            .is_some_and(|(id, ..)| self.current_nodes.iter().any(|n| n.id == id && n.multiline))
    }
    /// Shift+Enter in a multi-line input.
    pub fn input_newline(&mut self) -> Result<Option<Scene>, Diagnostic> {
        if !self.focused_multiline() {
            return Ok(None);
        }
        let Some((id, store, text)) = self.focused_input() else {
            return Ok(None);
        };
        let editor = self
            .editors
            .entry(id)
            .or_insert_with(|| input::Editor::new(text.len()));
        editor.clamp(&text);
        let (value, cursor) = input::replace(&text, editor.cursor, editor.anchor, "\n");
        editor.cursor = cursor;
        editor.anchor = cursor;
        editor.preedit.clear();
        self.update(BTreeMap::from([(store, Value::Text(value))]))
    }
    /// Up/Down in a multi-line input move by drawn line, keeping the x position.
    pub fn input_vertical(&mut self, up: bool, shift: bool) -> Result<Option<Scene>, Diagnostic> {
        if !self.focused_multiline() {
            return Ok(None);
        }
        let Some((id, _, text)) = self.focused_input() else {
            return Ok(None);
        };
        let Some(node) = self.current_nodes.iter().find(|n| n.id == id) else {
            return Ok(None);
        };
        let r = rect(&node.draw);
        let found = self.current.primitives.iter().find_map(|p| match p {
            Primitive::Text {
                rect, family, size, ..
            } if rect.x >= r.x && rect.y >= r.y && rect.x < r.x + r.w && rect.y < r.y + r.h => {
                Some((rect.w, family.clone(), *size))
            }
            _ => None,
        });
        let Some((width, family, size)) = found else {
            return Ok(None);
        };
        let editor = self
            .editors
            .entry(id)
            .or_insert_with(|| input::Editor::new(text.len()));
        editor.clamp(&text);
        let to = bead_render::text_vertical(
            &self.current,
            &text,
            editor.cursor,
            up,
            &family,
            size,
            width,
        )
        .map_err(|e| Diagnostic::new(&self.entry, 1, 1, e))?;
        editor.cursor = to;
        if !shift {
            editor.anchor = to;
        }
        self.update(BTreeMap::new())?
            .map_or_else(|| self.relayout(), |scene| Ok(Some(scene)))
    }
    pub fn ime(
        &mut self,
        preedit: String,
        commit: Option<String>,
        before: u32,
        after: u32,
    ) -> Result<Option<Scene>, Diagnostic> {
        let Some((id, store, mut text)) = self.focused_input() else {
            return Ok(None);
        };
        let editor = self
            .editors
            .entry(id)
            .or_insert_with(|| input::Editor::new(text.len()));
        editor.clamp(&text);
        let start = editor.cursor.saturating_sub(before as usize);
        let end = (editor.cursor + after as usize).min(text.len());
        if !text.is_char_boundary(start) || !text.is_char_boundary(end) {
            return Err(Diagnostic::new(
                &self.entry,
                1,
                1,
                "IME deletion must use UTF-8 boundaries",
            ));
        }
        if before > 0 || after > 0 {
            let (new, cursor) = input::replace(&text, start, end, "");
            text = new;
            editor.cursor = cursor;
            editor.anchor = cursor;
        }
        if let Some(commit) = commit {
            let (new, cursor) = input::replace(&text, editor.cursor, editor.anchor, &commit);
            text = new;
            editor.cursor = cursor;
            editor.anchor = cursor;
        }
        editor.preedit = preedit;
        self.update(BTreeMap::from([(store, Value::Text(text))]))?
            .map_or_else(|| self.relayout(), |scene| Ok(Some(scene)))
    }
    pub fn decorate_inputs(&self, scene: &mut Scene) -> Result<(), Diagnostic> {
        let nodes = self
            .pending
            .as_ref()
            .map_or(&self.current_nodes, |p| &p.nodes);
        let stores = self.pending.as_ref().map_or(&self.stores, |p| &p.stores);
        let Some(Value::Text(focused)) = stores.get("bead.focused") else {
            return Ok(());
        };
        let Some(node) = nodes.iter().find(|n| &n.id == focused && n.input.is_some()) else {
            return Ok(());
        };
        let Some(Value::Text(value)) = stores.get(node.input.as_ref().unwrap()) else {
            return Ok(());
        };
        let r = rect(&node.draw);
        let editor = self.editors.get(focused);
        let cursor = input::boundaries(value)
            .into_iter()
            .rfind(|p| *p <= editor.map_or(value.len(), |e| e.cursor))
            .unwrap_or(0);
        let anchor = input::boundaries(value)
            .into_iter()
            .rfind(|p| *p <= editor.map_or(cursor, |e| e.anchor))
            .unwrap_or(0);
        let shown;
        let (value, cursor, anchor) = if node.masked {
            shown = input::mask(value);
            (
                &shown,
                input::mask_offset(value, cursor),
                input::mask_offset(value, anchor),
            )
        } else {
            (value, cursor, anchor)
        };
        let preedit = editor.map_or("", |e| e.preedit.as_str());
        let text_index=scene.primitives.iter().position(|p|matches!(p,Primitive::Text{rect,..}if rect.x>=r.x&&rect.y>=r.y&&rect.x<r.x+r.w&&rect.y<r.y+r.h));
        let Some(index) = text_index else {
            return Ok(());
        };
        let Primitive::Text {
            rect: area,
            text,
            family,
            size,
            ..
        } = &scene.primitives[index]
        else {
            return Ok(());
        };
        let area = *area;
        let family = family.clone();
        let size = *size;
        let _ = text;
        let highlight_fill = {
            let mut c = self.palette.get("primary").unwrap_or([120, 120, 120, 255]);
            c[3] = 70;
            c
        };
        let (x, y, highlights) = if node.multiline {
            let (x, y) = bead_render::text_caret(scene, value, cursor, &family, size, area.w)
                .map_err(|e| node.diagnostic(e))?;
            let rects = if cursor != anchor {
                bead_render::text_selection(scene, value, cursor, anchor, &family, size, area.w)
                    .map_err(|e| node.diagnostic(e))?
            } else {
                Vec::new()
            };
            (x, y, rects)
        } else {
            let measure = |pos| {
                bead_render::text_boundary_x(scene, value, pos, &family, size, area.w)
                    .map_err(|e| node.diagnostic(e))
            };
            let x = measure(cursor)?;
            let rects = if cursor != anchor {
                let end = measure(anchor)?;
                vec![Rect::new(
                    x.min(end),
                    0.,
                    (x - end).abs().max(1.),
                    size * 1.4,
                )]
            } else {
                Vec::new()
            };
            (x, 0., rects)
        };
        let inserted = highlights.len();
        for (offset, rect) in highlights.into_iter().enumerate() {
            let at = index + offset;
            // Insert behind the text; move clip indices along with the command.
            scene.primitives.insert(
                at,
                Primitive::Rounded {
                    rect: Rect::new(area.x + rect.x, area.y + rect.y, rect.w, rect.h),
                    radius: 1.,
                    fill: highlight_fill,
                    border: None,
                },
            );
            for layer in &mut scene.layers {
                if layer.own.contains(&at) {
                    layer.own.end += 1;
                } else if layer.own.start >= at {
                    layer.own.start += 1;
                    layer.own.end += 1;
                }
                if layer.subtree_end > at {
                    layer.subtree_end += 1;
                }
            }
            scene.clips = scene
                .clips
                .iter()
                .map(|(i, r)| (if *i >= at { i + 1 } else { *i }, *r))
                .collect();
        }
        let text_index = index + inserted;
        if !preedit.is_empty()
            && let Primitive::Text { text, .. } = &mut scene.primitives[text_index]
        {
            *text = format!("{}{}{}", &value[..cursor], preedit, &value[cursor..]);
        }
        scene.primitives.push(Primitive::Rounded {
            rect: Rect::new(area.x + x, area.y + y, 1., size * 1.4),
            radius: 0.,
            fill: self
                .palette
                .get("onSurface")
                .unwrap_or([220, 220, 220, 255]),
            border: None,
        });
        Ok(())
    }
    pub fn pointer_cursor(&self, x: f32, y: f32) -> &'static str {
        self.hit(x, y, false).map_or("default", |n| {
            if n.input.is_some() { "text" } else { "pointer" }
        })
    }
    /// Controls opt in explicitly; empty background continues to host edge hover.
    pub fn allows_edge_hover(&self, x: f32, y: f32) -> bool {
        self.hit(x, y, false).is_none_or(|n| n.edge_hover)
    }
    pub fn selected_text(&self) -> Option<String> {
        let (id, _, text) = self.focused_input()?;
        if self.focused_masked() {
            return None;
        }
        let editor = self.editors.get(&id)?;
        let points = input::boundaries(&text);
        let cursor = points
            .iter()
            .copied()
            .rfind(|p| *p <= editor.cursor)
            .unwrap_or(0);
        let anchor = points
            .into_iter()
            .rfind(|p| *p <= editor.anchor)
            .unwrap_or(0);
        (cursor != anchor).then(|| text[cursor.min(anchor)..cursor.max(anchor)].to_owned())
    }
    /// The focused input is masked: its value stays out of IME, copy and accessibility.
    pub fn focused_masked(&self) -> bool {
        self.focused_input()
            .is_some_and(|(id, ..)| self.current_nodes.iter().any(|n| n.id == id && n.masked))
    }
    pub fn surrounding_text(&self) -> Option<(String, i32, i32)> {
        let (id, _, text) = self.focused_input()?;
        if self.focused_masked() {
            return Some((String::new(), 0, 0));
        }
        let editor = self.editors.get(&id);
        Some((
            text.clone(),
            editor.map_or(text.len(), |e| e.cursor).min(text.len()) as i32,
            editor.map_or(text.len(), |e| e.anchor).min(text.len()) as i32,
        ))
    }
    pub fn key(&mut self, text: &str, backspace: bool) -> Result<Option<Scene>, Diagnostic> {
        let Some((id, store, value)) = self.focused_input() else {
            return Ok(None);
        };
        let editor = self
            .editors
            .entry(id)
            .or_insert_with(|| input::Editor::new(value.len()));
        editor.clamp(&value);
        let anchor = if backspace && editor.cursor == editor.anchor {
            input::move_cursor(&value, editor.cursor, input::EditKey::Left)
        } else {
            editor.anchor
        };
        let insert: String = text.chars().filter(|c| !c.is_control()).collect();
        let (value, cursor) = input::replace(
            &value,
            editor.cursor,
            anchor,
            if backspace { "" } else { &insert },
        );
        editor.cursor = cursor;
        editor.anchor = cursor;
        editor.preedit.clear();
        self.update(BTreeMap::from([(store, Value::Text(value))]))
    }
    pub fn accessibility_tree(&self) -> accesskit::TreeUpdate {
        accessibility::tree_with_scene(&self.current_nodes, &self.stores, &self.current)
    }
    pub fn accessibility_action(
        &mut self,
        target: u64,
        action: &str,
        value: Option<String>,
    ) -> Result<Option<Scene>, Diagnostic> {
        let node = self
            .current_nodes
            .iter()
            .find(|n| accessibility::id(&n.id).0 == target)
            .ok_or_else(|| {
                Diagnostic::new(&self.entry, 1, 1, "accessibility target unavailable")
            })?;
        let id = node.id.clone();
        let input = node.input.clone();
        match action {
            "focus" => self.focus_id(&id),
            "click" => {
                let node = self.current_nodes.iter().find(|n| n.id == id).unwrap();
                let action = node.action.clone();
                let arg = node.action_arg.clone().unwrap_or(Value::Bool(true));
                let mut stores = self.stores.clone();
                stores
                    .commit(BTreeMap::from([("bead.focused".into(), Value::Text(id))]))
                    .map_err(|e| Diagnostic::new(&self.entry, 1, 1, e))?;
                if let Some(action) = action {
                    stores = self
                        .generation
                        .as_ref()
                        .unwrap()
                        .script
                        .action(&action, &stores, arg)?;
                }
                self.stage_state(stores)
            }
            "set-value" => {
                let store = input
                    .ok_or_else(|| Diagnostic::new(&self.entry, 1, 1, "target is not editable"))?;
                self.update(BTreeMap::from([(
                    store,
                    Value::Text(value.ok_or_else(|| {
                        Diagnostic::new(&self.entry, 1, 1, "set-value requires text")
                    })?),
                )]))
            }
            _ => Err(Diagnostic::new(
                &self.entry,
                1,
                1,
                "unsupported accessibility action",
            )),
        }
    }
}
