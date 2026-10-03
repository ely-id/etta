use halley_api::{
    Client, Event, EventTopic, NodeId, NodeInfo, NodeKind, NodeRole, NodeSelector, NodeState,
};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::io::{self, Write};
use wayland_client::globals::{GlobalListContents, registry_queue_init};
use wayland_client::protocol::{wl_output, wl_registry};
use wayland_client::{Connection, Dispatch, Proxy, QueueHandle};
use wayland_protocols::xdg::xdg_output::zv1::client::{
    zxdg_output_manager_v1::{self, ZxdgOutputManagerV1},
    zxdg_output_v1::{self, ZxdgOutputV1},
};

// Output geometry belongs to Wayland. Halley's node positions are Field
// coordinates, which cannot be used to crop a screenshot of the desktop.
#[derive(Default)]
struct Output {
    name: String,
    position: (i32, i32),
    size: (i32, i32),
    scale: i32,
    rotated: bool,
    logical_size: Option<(i32, i32)>,
}

#[derive(Default)]
struct Outputs(BTreeMap<u32, Output>);

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for Outputs {
    fn event(
        _: &mut Self,
        _: &wl_registry::WlRegistry,
        _: wl_registry::Event,
        _: &GlobalListContents,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<wl_output::WlOutput, ()> for Outputs {
    fn event(
        state: &mut Self,
        output: &wl_output::WlOutput,
        event: wl_output::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let entry = state.0.entry(output.id().protocol_id()).or_default();
        match event {
            wl_output::Event::Name { name } => entry.name = name,
            wl_output::Event::Geometry {
                x, y, transform, ..
            } => {
                entry.position = (x, y);
                entry.rotated = matches!(
                    transform,
                    wayland_client::WEnum::Value(
                        wl_output::Transform::_90
                            | wl_output::Transform::_270
                            | wl_output::Transform::Flipped90
                            | wl_output::Transform::Flipped270
                    )
                );
            }
            wl_output::Event::Mode {
                flags: wayland_client::WEnum::Value(flags),
                width,
                height,
                ..
            } if flags.contains(wl_output::Mode::Current) => entry.size = (width, height),
            wl_output::Event::Scale { factor } => entry.scale = factor,
            _ => {}
        }
    }
}

impl Dispatch<ZxdgOutputManagerV1, ()> for Outputs {
    fn event(
        _: &mut Self,
        _: &ZxdgOutputManagerV1,
        _: zxdg_output_manager_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ZxdgOutputV1, u32> for Outputs {
    fn event(
        state: &mut Self,
        _: &ZxdgOutputV1,
        event: zxdg_output_v1::Event,
        key: &u32,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let entry = state.0.entry(*key).or_default();
        match event {
            zxdg_output_v1::Event::LogicalPosition { x, y } => entry.position = (x, y),
            zxdg_output_v1::Event::LogicalSize { width, height } => {
                entry.logical_size = Some((width, height))
            }
            zxdg_output_v1::Event::Name { name } => entry.name = name,
            _ => {}
        }
    }
}

fn output_value(output: &Output, focused: &str) -> Value {
    let scale = output.scale.max(1);
    let (width, height) = output.logical_size.unwrap_or_else(|| {
        let (w, h) = output.size;
        if output.rotated {
            (h / scale, w / scale)
        } else {
            (w / scale, h / scale)
        }
    });
    json!({"name": output.name, "x": output.position.0, "y": output.position.1,
        "w": width, "h": height, "focused": output.name == focused})
}

fn outputs(client: &Client) -> Result<Value, Box<dyn std::error::Error>> {
    let focused = client
        .nodes(None)?
        .into_iter()
        .find(|n| n.focused)
        .and_then(|n| n.output)
        .unwrap_or_default();
    let connection = Connection::connect_to_env()?;
    let (globals, mut queue) = registry_queue_init::<Outputs>(&connection)?;
    let qh = queue.handle();
    let manager = globals
        .bind::<ZxdgOutputManagerV1, _, _>(&qh, 1..=3, ())
        .ok();
    let mut handles = Vec::new();
    let mut logical = Vec::new();
    for global in globals.contents().clone_list() {
        if global.interface == "wl_output" {
            let handle = globals.registry().bind::<wl_output::WlOutput, _, _>(
                global.name,
                global.version.min(4),
                &qh,
                (),
            );
            if let Some(manager) = &manager {
                logical.push(manager.get_xdg_output(&handle, &qh, handle.id().protocol_id()));
            }
            handles.push(handle);
        }
    }
    let mut state = Outputs::default();
    queue.roundtrip(&mut state)?;
    queue.roundtrip(&mut state)?;
    Ok(Value::Array(
        state
            .0
            .values()
            .filter(|o| !o.name.is_empty())
            .map(|o| output_value(o, &focused))
            .collect(),
    ))
}

fn windows(nodes: &BTreeMap<NodeId, NodeInfo>) -> Value {
    let list: Vec<Value> = nodes.values()
        .filter(|n| n.kind == NodeKind::Surface && n.role != NodeRole::Popup)
        .map(|n| json!({"id": n.id.to_string(), "title": n.title,
            "class": n.app_id.as_deref().unwrap_or(""), "monitor": n.output.as_deref().unwrap_or(""),
            "active": n.focused, "minimized": n.state == NodeState::Node})).collect();
    let active = list.iter().find(|n| n["active"] == true);
    json!({"title": active.map(|n| &n["title"]).unwrap_or(&Value::Null),
        "class": active.map(|n| &n["class"]).unwrap_or(&Value::Null),
        "monitor": active.map(|n| &n["monitor"]).unwrap_or(&Value::Null), "list": list})
}

fn print(value: &Value) -> io::Result<()> {
    println!("{value}");
    io::stdout().flush()
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let client = Client::connect()?;
    match args.as_slice() {
        [command] if command == "outputs" => print(&outputs(&client)?)?,
        [command] if command == "watch" => {
            let mut subscription = client.subscribe([EventTopic::Nodes])?;
            let mut nodes: BTreeMap<_, _> = subscription
                .initial
                .nodes
                .into_iter()
                .map(|n| (n.id, n))
                .collect();
            let mut previous = windows(&nodes);
            print(&previous)?;
            loop {
                match subscription.events.next_event()? {
                    Event::NodeAdded { node, .. } | Event::NodeChanged { node, .. } => {
                        nodes.insert(node.id, node);
                    }
                    Event::NodeRemoved { id, .. } => {
                        nodes.remove(&id);
                    }
                    _ => continue,
                }
                let current = windows(&nodes);
                if current != previous {
                    print(&current)?;
                    previous = current;
                }
            }
        }
        [command, action, id] if command == "window" => {
            let selector = Some(NodeSelector::Id(NodeId::new(id.parse()?)));
            match action.as_str() {
                "activate" => client.focus_node(selector, None)?,
                "restore" => {
                    client.restore_node(selector.clone(), None)?;
                    client.focus_node(selector, None)?;
                }
                _ => return Err(format!("unsupported window action: {action}").into()),
            }
        }
        _ => return Err("usage: marea-halley outputs | watch | window activate|restore ID".into()),
    }
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("marea-halley: {error}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn outputs_use_logical_geometry_including_negative_offsets() {
        let output = Output {
            name: "DP-1".into(),
            position: (-1280, -720),
            size: (2560, 1440),
            scale: 2,
            ..Default::default()
        };
        assert_eq!(
            output_value(&output, "DP-1"),
            json!({"name":"DP-1","x":-1280,"y":-720,"w":1280,"h":720,"focused":true})
        );
        let rotated = Output {
            rotated: true,
            ..output
        };
        assert_eq!(output_value(&rotated, "")["w"], 720);
        let logical = Output {
            logical_size: Some((853, 480)),
            ..rotated
        };
        assert_eq!(output_value(&logical, "")["w"], 853);
    }

    #[test]
    fn an_empty_desktop_has_no_focused_window() {
        let state = windows(&BTreeMap::new());
        assert_eq!(state["list"], json!([]));
        assert!(state["monitor"].is_null());
    }

    #[test]
    fn window_ids_preserve_all_64_bits_and_compositor_core_nodes_are_excluded() {
        let mut node: NodeInfo = serde_json::from_value(json!({"id":u64::MAX,"title":"test","app_id":"test-app","output":"DP-1",
            "kind":"Surface","state":"Node","visible":true,"focused":true,"latest":true,"pinned":false,
            "role":"NormalToplevel","protocol_family":"XdgToplevel","modal":false,"parent":null,"transient_for":null,
            "child_popup_count":0,"x":100.0,"y":-50.0,"width":640.0,"height":480.0})).unwrap();
        let mut nodes = BTreeMap::from([(node.id, node.clone())]);
        let state = windows(&nodes);
        assert_eq!(state["list"][0]["id"], u64::MAX.to_string());
        assert_eq!(state["monitor"], "DP-1");
        assert_eq!(state["list"][0]["minimized"], true);
        node.id = NodeId::new(1);
        node.kind = NodeKind::Core;
        nodes.insert(node.id, node);
        assert_eq!(windows(&nodes)["list"].as_array().unwrap().len(), 1);
    }
}
