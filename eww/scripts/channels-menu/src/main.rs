use std::{cell::RefCell, collections::{BTreeMap, HashMap}, rc::Rc};

use eww_ipc::EwwClient;
use pipewire::{context::ContextRc, main_loop::MainLoopRc, metadata::{Metadata, MetadataListener}, node::{Node, NodeListener}, registry::{self, GlobalObject, Registry}, spa::{param::ParamType, pod::{Value, ValueArray, deserialize::PodDeserializer}, sys::{SPA_PROP_channelVolumes, SPA_PROP_mute}, utils::dict::DictRef}, types::ObjectType};

type NodeHash = Rc<RefCell<HashMap<u32, NodeEntry>>>;
struct NodeHashes {
    streams: NodeHash,
    inputs: NodeHash,
    outputs: NodeHash,

    default_input_name: RefCell<String>,
    default_output_name: RefCell<String>,
    
    meta: RefCell<Option<(Metadata, MetadataListener)>>
}
impl NodeHashes {
    fn get(&self, node_type: NodeType) -> &NodeHash {
        match node_type {
            NodeType::Stream => &self.streams,
            NodeType::Input => &self.inputs,
            NodeType::Output => &self.outputs,
        }
    }
}
struct NodeEntry {
    full_name: String,
    display_name: String,
    volume: f32,
    mute: bool,
    _proxy: Node,
    _listener: NodeListener,
}
#[derive(Clone, Copy)]
enum NodeType {
    Stream,
    Input,
    Output
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mainloop = MainLoopRc::new(None)?;
    let context = ContextRc::new(&mainloop, None)?;
    let core = context.connect_rc(None)?;
    let registry = core.get_registry_rc()?;
    let registry_to_move: registry::RegistryRc = registry.clone();

    let eww_ipc = EwwClient::new()?;
    let eww_add = eww_ipc.clone();
    let eww_remove = eww_ipc.clone();

    let maps = Rc::new(NodeHashes {
        streams: Rc::new(RefCell::new(HashMap::new())),
        inputs: Rc::new(RefCell::new(HashMap::new())),
        outputs: Rc::new(RefCell::new(HashMap::new())),

        default_input_name: RefCell::new("none".to_string()),
        default_output_name: RefCell::new("none".to_string()),

        meta: RefCell::new(None),
    });
    let maps_add = maps.clone();
    let maps_remove = maps.clone();

    let _listener = registry
        .add_listener_local()
        .global(move |global| {
            match global.type_ {
                ObjectType::Node => {
                    let Some(props) = global.props else { return; };

                    match props.get("media.class") {
                        Some("Stream/Output/Audio") => {
                            monitor_node(&registry_to_move, &global, &maps_add, props.get("application.name").unwrap_or("unknown").to_string(), NodeType::Stream, eww_add.clone());
                        }
                        Some("Audio/Sink") => {
                            monitor_node(&registry_to_move, &global, &maps_add, props.get("node.nick").unwrap_or("unknown").to_string(), NodeType::Output, eww_add.clone());
                        }
                        Some("Audio/Source") => {
                            monitor_node(&registry_to_move, &global, &maps_add, props.get("node.nick").unwrap_or("unknown").to_string(), NodeType::Input, eww_add.clone());
                        }
                        _ => {}
                    }
                }
                ObjectType::Metadata => {
                    let Some(meta) = global.props else { return; };
                    if meta.get("metadata.name") != Some("default") { return; };

                    let Ok(metadata) = registry_to_move.bind::<Metadata, _>(global) else { return; };
                    let weak = Rc::downgrade(&maps_add);
                    let eww_to_move = eww_add.clone();

                    let listener = metadata.add_listener_local()
                        .property(move |_subject, key, _type, value| {
                            let Some(maps) = weak.upgrade() else { return 0; };

                            let name = value
                                .and_then(|val| serde_json::from_str::<serde_json::Value>(val).ok())
                                .and_then(|json| json["name"].as_str().map(str::to_string));

                            let Some(name) = name else { return 0; };

                            match key {
                                Some("default.audio.sink") => {
                                    *maps.default_output_name.borrow_mut() = name;
                                    output_hard(&maps, NodeType::Output, eww_to_move.clone());
                                }
                                Some("default.audio.source") => {
                                    *maps.default_input_name.borrow_mut() = name;
                                    output_hard(&maps, NodeType::Input, eww_to_move.clone());
                                }
                                _ => {}
                            }
                            return 0
                        })
                        .register();

                    *maps_add.meta.borrow_mut() = Some((metadata, listener));
                }
                _ => {}
            }
            
        })
        .global_remove(move |id| {
            let removed = maps_remove.streams.borrow_mut().remove(&id).is_some();
            if removed {
                output_hard(&maps_remove, NodeType::Stream, eww_remove.clone());
            }

            let removed = maps_remove.outputs.borrow_mut().remove(&id).is_some();
            if removed {
                output_hard(&maps_remove, NodeType::Output, eww_remove.clone());
            }

            let removed = maps_remove.inputs.borrow_mut().remove(&id).is_some();
            if removed {
                output_hard(&maps_remove, NodeType::Input, eww_remove.clone());
            }
        })
        .register();

    mainloop.run();

    Ok(())
}

#[allow(non_upper_case_globals)]
fn monitor_node(registry: &Registry, global: &GlobalObject<&DictRef>, maps: &Rc<NodeHashes>, name: String, node_type: NodeType, eww_ipc: EwwClient) {
    let Ok(node) = registry.bind::<Node, _>(global) else { return; };
    let id = global.id;
    let weak = Rc::downgrade(maps);

    let listener = node.add_listener_local()
        .param(move |_, ptype, _, _, pod| {
            let Some(maps) = weak.upgrade() else { return; };
            if ptype != ParamType::Props { return; }

            let Some(pod) = pod else { return; };
            let Ok((_, Value::Object(obj)))
                = PodDeserializer::deserialize_any_from(pod.as_bytes())
                else { return; };

            {
                let mut nodes = maps.get(node_type).borrow_mut();
                let Some(entry) = nodes.get_mut(&id) else { return; };

                for props in obj.properties {
                    match props.key {
                        SPA_PROP_channelVolumes => {
                            if let Value::ValueArray(ValueArray::Float(volumes)) = props.value {
                                let linear = volumes.iter().copied().fold(0.0, f32::max);
                                entry.volume = linear.cbrt();
                            }
                        }
                        SPA_PROP_mute => {
                            if let Value::Bool(mute) = props.value {
                                entry.mute = mute;
                            }
                        }
                        _ => {}
                    }
                }
            }

            output_soft(&maps);
        })
        .register();

    node.subscribe_params(&[ParamType::Props]);

    let props = global.props;
    let full_name = match props {
        Some(pr) => {
            pr.get("node.name").unwrap_or("unknown")
        }
        _ => "unknown"
    };
    maps.get(node_type).borrow_mut().insert(id, NodeEntry { full_name: full_name.to_string(), display_name: name, volume: 0.0, mute: false, _proxy: node, _listener: listener });
    
    output_hard(&maps, node_type, eww_ipc);
}

fn output_soft(maps: &NodeHashes) {
    let mut obj = serde_json::Map::new();
    for map in [&maps.streams, &maps.inputs, &maps.outputs] {
        for (id, entry) in map.borrow().iter() {
            obj.insert(id.to_string(), serde_json::json!({"volume": (entry.volume * 100.0).round() / 100.0, "muted": entry.mute}));
        }
    }

    let json = serde_json::Value::Object(obj).to_string();

    println!("{}", json);
}

fn output_hard(maps: &NodeHashes, node_type: NodeType, eww_ipc: EwwClient) {
    let nodes = maps.get(node_type).borrow();

    let mut list: Vec<serde_json::Value> = match node_type {
        NodeType::Stream => {
            let mut groups: BTreeMap<String, Vec<u32>> = BTreeMap::new();
            for (id, entry) in nodes.iter() {
                groups.entry(entry.display_name.clone()).or_default().push(*id);
            }
            groups
                .into_iter()
                .map(|(name, mut ids)| {
                    ids.sort_unstable();
                    serde_json::json!({"name": name, "ids": ids})
                })
                .collect()
        }
        _ => {
            let default_name = match node_type {
                NodeType::Input => maps.default_input_name.borrow().clone(),
                _ => maps.default_output_name.borrow().clone(),
            };
            nodes
                .iter()
                .map(|(id, entry)| {
                    let is_default = default_name == entry.full_name;
                    let name = capitilize_first_letter(entry.display_name.as_str());
                    serde_json::json!({"id": id, "name": name, "full_name": entry.full_name, "default": is_default})
                })
                .collect()
            
        }
    };

    list.sort_by_key(|value| value["name"].as_str().unwrap_or("").to_string());
    let json: String = serde_json::to_string(&list).unwrap_or_else(|_| "[]".into());

    let update_var = match node_type {
        NodeType::Stream => "audio_streams",
        NodeType::Input => "audio_inputs",
        NodeType::Output => "audio_outputs"
    };

    eww_ipc.update(&[(update_var, json)]).unwrap_or_default();
}

fn capitilize_first_letter(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        None => String::new(),
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
    }
}