// SPDX-License-Identifier: MIT OR Apache-2.0
use crate::{Audio, runtime::Shared};
use pipewire as pw;
use pw::{
    metadata::{Metadata, MetadataListener},
    node::{Node, NodeListener},
    spa::{
        self,
        pod::{Object, Pod, Property, Value, ValueArray},
    },
    types::ObjectType,
};
use std::{
    cell::RefCell,
    collections::HashMap,
    rc::Rc,
    sync::{Arc, atomic::Ordering},
    time::Duration,
};
type Reply = std::sync::mpsc::Sender<Result<(), String>>;
pub(crate) enum Command {
    Volume(f64, Reply),
    Mute(bool, Reply),
    Stop,
}
struct Sink {
    name: String,
    node: Node,
    _listener: NodeListener,
    channels: Vec<f32>,
    muted: Option<bool>,
}
#[derive(Default)]
struct Graph {
    default: Option<String>,
    sinks: HashMap<u32, Sink>,
    metadata: HashMap<u32, (Metadata, MetadataListener)>,
}
impl Graph {
    fn audio(&self) -> Audio {
        let sink = self
            .default
            .as_ref()
            .and_then(|name| self.sinks.values().find(|s| &s.name == name));
        Audio {
            connected: true,
            default_sink: self.default.clone(),
            volume: sink
                .and_then(|s| s.channels.iter().copied().reduce(f32::max))
                .map(|v| f64::from(v.max(0.).cbrt())),
            muted: sink.and_then(|s| s.muted),
        }
    }
}
fn publish(graph: &Rc<RefCell<Graph>>, shared: &Shared) {
    let audio = graph.borrow().audio();
    shared.change(|s| s.audio = audio);
}
pub(crate) fn spawn(shared: Arc<Shared>) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        super::runtime::block_worker_signals();
        pw::init();
        let mut retry = 0;
        while !shared.stop.load(Ordering::SeqCst) {
            if let Err(error) = connect(&shared) {
                eprintln!("SERVICE_AUDIO disconnected: {error}");
            }
            *shared.audio_sender.lock().unwrap() = None;
            shared.change(|s| s.audio = Audio::default());
            if shared.stop.load(Ordering::SeqCst) {
                break;
            }
            retry = (retry + 1).min(3);
            let guard = shared.audio_wait.0.lock().unwrap();
            let _ = shared
                .audio_wait
                .1
                .wait_timeout(guard, Duration::from_millis(200u64 << retry));
        }
    })
}
fn connect(shared: &Arc<Shared>) -> Result<(), Box<dyn std::error::Error>> {
    let mainloop = pw::main_loop::MainLoopRc::new(None)?;
    let context = pw::context::ContextRc::new(&mainloop, None)?;
    let core = context.connect_rc(None)?;
    let registry = core.get_registry_rc()?;
    let weak_loop = mainloop.downgrade();
    let _core = core
        .add_listener_local()
        .error(move |id, _, _, _| {
            if id == 0
                && let Some(mainloop) = weak_loop.upgrade()
            {
                mainloop.quit();
            }
        })
        .register();
    let graph = Rc::new(RefCell::new(Graph::default()));
    let selected = graph.clone();
    let registry_copy = registry.clone();
    let state = shared.clone();
    let removed = graph.clone();
    let removed_state = shared.clone();
    let _registry = registry
        .add_listener_local()
        .global(move |object| {
            if object.type_ == ObjectType::Node
                && object.props.as_ref().and_then(|p| p.get("media.class")) == Some("Audio/Sink")
            {
                let Ok(node) = registry_copy.bind::<Node, _>(object) else {
                    return;
                };
                let id = object.id;
                let name = object
                    .props
                    .as_ref()
                    .and_then(|p| p.get("node.name"))
                    .unwrap_or("")
                    .to_string();
                let weak = Rc::downgrade(&selected);
                let state = state.clone();
                let listener = node
                    .add_listener_local()
                    .param(move |_, kind, _, _, pod| {
                        if kind != spa::param::ParamType::Props {
                            return;
                        }
                        let Some(graph) = weak.upgrade() else { return };
                        let Some(pod) = pod else { return };
                        let Ok((_, Value::Object(object))) =
                            spa::pod::deserialize::PodDeserializer::deserialize_from::<Value>(
                                pod.as_bytes(),
                            )
                        else {
                            return;
                        };
                        {
                            let mut graph = graph.borrow_mut();
                            let Some(sink) = graph.sinks.get_mut(&id) else {
                                return;
                            };
                            for property in object.properties {
                                match (property.key, property.value) {
                                    (
                                        spa::sys::SPA_PROP_channelVolumes,
                                        Value::ValueArray(ValueArray::Float(volumes)),
                                    ) => sink.channels = volumes,
                                    (spa::sys::SPA_PROP_mute, Value::Bool(mute)) => {
                                        sink.muted = Some(mute)
                                    }
                                    _ => {}
                                }
                            }
                        }
                        publish(&graph, &state);
                    })
                    .register();
                node.subscribe_params(&[spa::param::ParamType::Props]);
                node.enum_params(0, Some(spa::param::ParamType::Props), 0, u32::MAX);
                selected.borrow_mut().sinks.insert(
                    id,
                    Sink {
                        name,
                        node,
                        _listener: listener,
                        channels: vec![],
                        muted: None,
                    },
                );
            } else if object.type_ == ObjectType::Metadata
                && object.props.as_ref().and_then(|p| p.get("metadata.name")) == Some("default")
            {
                let Ok(metadata) = registry_copy.bind::<Metadata, _>(object) else {
                    return;
                };
                let weak = Rc::downgrade(&selected);
                let state = state.clone();
                let listener = metadata
                    .add_listener_local()
                    .property(move |subject, key, _, value| {
                        if subject == 0
                            && (key == Some("default.audio.sink") || key.is_none())
                            && let Some(graph) = weak.upgrade()
                        {
                            graph.borrow_mut().default = value
                                .and_then(|value| {
                                    serde_json::from_str::<serde_json::Value>(value).ok()
                                })
                                .and_then(|v| v["name"].as_str().map(str::to_owned));
                            publish(&graph, &state);
                        }
                        0
                    })
                    .register();
                selected
                    .borrow_mut()
                    .metadata
                    .insert(object.id, (metadata, listener));
            }
            publish(&selected, &state);
        })
        .global_remove(move |id| {
            removed.borrow_mut().sinks.remove(&id);
            removed.borrow_mut().metadata.remove(&id);
            publish(&removed, &removed_state);
        })
        .register();
    let (sender, receiver) = pw::channel::channel();
    *shared.audio_sender.lock().unwrap() = Some(sender);
    let graph_action = graph.clone();
    let weak_loop = mainloop.downgrade();
    let _receiver = receiver.attach(mainloop.loop_(), move |command| {
        if matches!(command, Command::Stop) {
            if let Some(mainloop) = weak_loop.upgrade() {
                mainloop.quit();
            }
            return;
        }
        let (value, reply) = match command {
            Command::Volume(v, reply) => (Some(v), reply),
            Command::Mute(v, reply) => {
                let result = set(&graph_action, None, Some(v));
                let _ = reply.send(result);
                return;
            }
            Command::Stop => unreachable!(),
        };
        let _ = reply.send(set(&graph_action, value, None));
    });
    publish(&graph, shared);
    mainloop.run();
    Ok(())
}
fn set(graph: &Rc<RefCell<Graph>>, volume: Option<f64>, mute: Option<bool>) -> Result<(), String> {
    let graph = graph.borrow();
    let sink = graph
        .default
        .as_ref()
        .and_then(|name| graph.sinks.values().find(|s| &s.name == name))
        .ok_or("no PipeWire default sink")?;
    let mut properties = vec![];
    if let Some(volume) = volume {
        if sink.channels.is_empty() {
            return Err("default sink channel volumes not ready".into());
        }
        properties.push(Property::new(
            spa::sys::SPA_PROP_channelVolumes,
            Value::ValueArray(ValueArray::Float(vec![
                volume.powi(3) as f32;
                sink.channels.len()
            ])),
        ));
    }
    if let Some(mute) = mute {
        properties.push(Property::new(spa::sys::SPA_PROP_mute, Value::Bool(mute)));
    }
    let object = Value::Object(Object {
        type_: spa::utils::SpaTypes::ObjectParamProps.as_raw(),
        id: spa::param::ParamType::Props.as_raw(),
        properties,
    });
    let bytes =
        spa::pod::serialize::PodSerializer::serialize(std::io::Cursor::new(Vec::new()), &object)
            .map_err(|e| format!("{e:?}"))?
            .0
            .into_inner();
    sink.node.set_param(
        spa::param::ParamType::Props,
        0,
        Pod::from_bytes(&bytes).ok_or("invalid Props pod")?,
    );
    Ok(())
}
#[cfg(test)]
mod tests {
    #[test]
    fn connected_without_default_does_not_invent_volume_or_mute() {
        let audio = super::Graph::default().audio();
        assert!(audio.connected);
        assert!(audio.volume.is_none());
        assert!(audio.muted.is_none());
    }
}
