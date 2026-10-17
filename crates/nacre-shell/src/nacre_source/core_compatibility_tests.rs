// SPDX-License-Identifier: GPL-3.0-only
//! Reserved schemas are consumer contracts, not advertised service capabilities.
use super::*;
use bead::core::source::{Applied, Mirror};
use nacre_protocol::core_services::CoreSnapshot;
use std::time::Instant;

fn replay(name: &str, text: &str) -> Vec<Value> {
    let generated = nacre_protocol::artifacts::generate();
    let frames = |text: &str| {
        text.lines()
            .map(|line| serde_json::from_str::<Value>(line).unwrap())
            .collect::<Vec<_>>()
    };
    assert_eq!(
        frames(text),
        frames(&generated[&format!("protocol/fixtures/core-compat-{name}.jsonl")])
    );
    let mut mirror = Mirror::new(Instant::now());
    mirror.reconnected("compatibility", Instant::now()).unwrap();
    let mut states = Vec::new();
    for line in text.lines() {
        let frame = framing::decode(line.as_bytes()).unwrap();
        assert_eq!(
            framing::decode(&framing::encode(&frame).unwrap()).unwrap(),
            frame
        );
        let topic = match frame {
            Message::Snap { topic, seq, data } => {
                mirror
                    .accept(
                        Arc::new(Snapshot {
                            epoch: "compatibility".into(),
                            revision: seq,
                            data: Arc::new(data),
                            connected: true,
                        }),
                        Instant::now(),
                    )
                    .unwrap();
                topic
            }
            Message::Patch {
                topic,
                base,
                seq,
                data,
            } => {
                assert_eq!(
                    mirror
                        .patch("compatibility", base, seq, &data, Instant::now())
                        .unwrap(),
                    Applied::Updated
                );
                topic
            }
            _ => panic!("unexpected compatibility frame"),
        };
        let state = mirror.snapshot().unwrap().data.as_ref().clone();
        let typed: CoreSnapshot =
            serde_json::from_value(serde_json::json!({ "topic": topic, "data": state })).unwrap();
        assert!(typed.valid());
        states.push(state);
    }
    states
}

#[test]
fn compatibility_snapshots_and_merge_patches_preserve_unknown_readback() {
    let audio = replay(
        "audio-readback",
        include_str!("../../tests/protocol-v1/core-compat-audio-readback.jsonl"),
    );
    let node = |index: usize| &audio[index]["nodes"]["items"]["output-1"];
    assert_eq!(node(0)["readback"], serde_json::json!({}));
    assert_eq!(node(1)["readback"]["perceptual_volume"], 0.5);
    assert_eq!(node(2)["ready"], false);
    assert!(node(2)["readback"].get("perceptual_volume").is_none());
    assert_eq!(node(2)["readback"]["linear_volume"], 0.125);
    assert!(node(3)["readback"].get("linear_volume").is_none());
    assert_eq!(node(3)["readback"]["muted"], false);
    assert_eq!(node(4)["readback"], serde_json::json!({}));
    assert_eq!(node(4)["volume"], 0.125); // Placeholder must not replace unknown readback.
    assert_eq!(node(5)["readback"], serde_json::json!({"muted":true}));
    let osd = replay(
        "osd",
        include_str!("../../tests/protocol-v1/core-compat-osd.jsonl"),
    );
    assert!(osd[0].get("current").is_none());
    assert_eq!(osd[3]["current"]["channel"], "brightness");
    assert!(osd[3]["current"].get("muted").is_none());
    assert_eq!(osd[4]["current"]["channel"], "microphone");
    assert!(osd[4]["current"].get("value").is_none());
    assert_eq!(osd[4]["current"]["muted"], true);
    assert_eq!(osd[8]["current"], osd[9]["current"]); // Same serial does not imply a new pulse.
    assert!(osd[10].get("current").is_none());
    assert_eq!(audio.len() + osd.len(), 17);
}

#[test]
fn compatibility_history_and_service_state_remain_independent() {
    let policy = replay(
        "notifications-popup-policy",
        include_str!("../../tests/protocol-v1/core-compat-notifications-popup-policy.jsonl"),
    );
    assert_eq!(policy[1]["popup_order"], serde_json::json!([]));
    assert_eq!(policy[2]["inhibited"], false);
    assert_eq!(policy[2]["popup_order"], serde_json::json!([])); // No replay after inhibition ends.
    assert_eq!(policy[5]["active"]["items"]["notice-3"]["timeout_ms"], 0);
    assert!(
        policy[5]["active"]["items"]["notice-3"]
            .get("remaining_ms")
            .is_none()
    );
    assert!(policy[6]["active"]["items"].get("notice-2").is_none());
    assert_eq!(policy[6]["history"]["items"]["notice-2"]["remaining_ms"], 0);
    assert_eq!(policy[6]["history_count"], 3);
    let history = replay(
        "notifications-history-storage",
        include_str!("../../tests/protocol-v1/core-compat-notifications-history-storage.jsonl"),
    );
    assert_eq!(history[0]["health"]["available"], true);
    assert_eq!(history[0]["history_storage"]["ready"], false);
    assert!(history[1]["history_storage"].get("error").is_none());
    assert_eq!(history[2]["history_storage"]["ready"], true);
    assert_eq!(history[2]["history_storage"]["writable"], false);
    let battery = replay(
        "battery",
        include_str!("../../tests/protocol-v1/core-compat-battery.jsonl"),
    );
    assert!(battery[3].get("percentage").is_none());
    assert!(battery[3].get("charging").is_none());
    let network = replay(
        "network-primary",
        include_str!("../../tests/protocol-v1/core-compat-network-primary.jsonl"),
    );
    assert_eq!(network[0]["primary_connection"], "connection-wired");
    assert!(network[2].get("primary_connection").is_none());
    let tray = replay(
        "tray-custom-icon",
        include_str!("../../tests/protocol-v1/core-compat-tray-custom-icon.jsonl"),
    );
    assert_eq!(
        tray[1]["items"]["items"]["tray-1"]["icon_path"],
        "/tmp/nacre-fixtures/icons/tray-attention.png"
    );
    assert_eq!(
        policy.len() + history.len() + battery.len() + network.len() + tray.len(),
        19
    );
}
