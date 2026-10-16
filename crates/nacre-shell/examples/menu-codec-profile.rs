// SPDX-License-Identifier: GPL-3.0-only
//! Headless diagnostic of the current codec, not an alternate protocol client.
use nacre_protocol::{Id, Message, Reply, framing, menu::MenuQueryResult};
use serde_json::{Value, json};
use std::{error::Error, hint::black_box, io, time::Instant};

type Result<T> = std::result::Result<T, Box<dyn Error>>;
fn protocol<T>(result: std::result::Result<T, nacre_protocol::Failure>) -> Result<T> {
    result.map_err(|error| io::Error::other(format!("Protocol {:?}", error.code)).into())
}
fn measure(mut operation: impl FnMut() -> Result<()>, iterations: usize) -> Result<Value> {
    for _ in 0..16 {
        operation()?;
    }
    let started = Instant::now();
    let cpu = bead::core::profile::process_cpu_ns();
    for _ in 0..iterations {
        operation()?;
    }
    let elapsed_cpu = bead::core::profile::process_cpu_ns().saturating_sub(cpu);
    if cpu == 0 || elapsed_cpu == 0 {
        return Err(io::Error::other("CPU clock unavailable").into());
    }
    Ok(
        json!({"cpu_ms_per_frame":elapsed_cpu as f64 / iterations as f64 / 1e6,
              "wall_ms_per_frame":started.elapsed().as_secs_f64()*1000. / iterations as f64}),
    )
}
fn main() -> Result<()> {
    let iterations = std::env::args()
        .nth(1)
        .map_or(Ok(1000), |s| s.parse::<usize>())?;
    if !(10..=100_000).contains(&iterations) {
        return Err(io::Error::other("Expected 10..100000 iterations").into());
    }
    let mut template = None;
    for line in include_str!("../tests/protocol-v1/menu-apps.jsonl").lines() {
        if let Message::Ret {
            reply: Reply::Ok { ok },
            ..
        } = protocol(framing::decode(line.as_bytes()))?
            && ok.get("items").is_some()
        {
            template = Some(serde_json::from_value::<MenuQueryResult>(ok)?);
        }
    }
    let template = template.ok_or_else(|| io::Error::other("Populated golden missing"))?;
    let mut reports = Vec::new();
    for count in [100, 320] {
        let mut result = template.clone();
        if result.order.len() < count {
            return Err(io::Error::other("Populated golden too small").into());
        }
        result.order.truncate(count);
        result.items.retain(|id, _| result.order.contains(id));
        result.query = "a".into();
        result.total = Some(count as u32);
        for item in result.items.values_mut() {
            item.activation = Some("a".repeat(64));
            item.description = Some("Synthetic application".into());
            item.breadcrumb = Some("All apps".into());
            item.icon = Some("fixture".into());
        }
        protocol(result.validate())?;
        let message = Message::Ret {
            id: Id::Integer(1),
            reply: Reply::Ok {
                ok: serde_json::to_value(result)?,
            },
        };
        let encoded = protocol(framing::encode(&message))?;
        let bytes = &encoded[..encoded.len() - 1];
        let dom: Value = serde_json::from_slice(bytes)?;
        for trial in 0..3 {
            let decode = measure(
                || {
                    black_box(protocol(framing::decode(black_box(bytes)))?);
                    Ok(())
                },
                iterations,
            )?;
            let raw_json = measure(
                || {
                    black_box(serde_json::from_slice::<Value>(black_box(bytes))?);
                    Ok(())
                },
                iterations,
            )?;
            let validation = measure(
                || {
                    protocol(black_box(&message).validate())?;
                    Ok(())
                },
                iterations,
            )?;
            let dom_clone = measure(
                || {
                    black_box(black_box(&dom).clone());
                    Ok(())
                },
                iterations,
            )?;
            let dom_to_message = measure(
                || {
                    black_box(serde_json::from_value::<Message>(black_box(&dom).clone())?);
                    Ok(())
                },
                iterations,
            )?;
            reports.push(json!({"entities":count,"bytes":bytes.len(),"trial":trial,"iterations":iterations,
                "full_decode_and_drop":decode,"raw_json_parse_and_drop":raw_json,
                "message_validation":validation,"dom_clone_and_drop":dom_clone,"dom_clone_to_message_and_drop":dom_to_message}));
        }
    }
    println!(
        "{}",
        serde_json::to_string_pretty(
            &json!({"method":"Single-thread headless process CPU; synthetic golden-derived payloads. Raw JSON and DOM clones are diagnostics, not valid substitute decoders. No duplicate-key/size/security check is removed.","reports":reports})
        )?
    );
    Ok(())
}
