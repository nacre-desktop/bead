// SPDX-License-Identifier: MIT OR Apache-2.0
//! Private proof driver; not a public shell command protocol.
use bead_services::{Action, Desktop};
use std::io::BufRead;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    if std::env::var("BEAD_PRIVATE_SERVICES_TEST").as_deref() != Ok("1") {
        return Err("requires private proof environment".into());
    }
    let desktop = Desktop::start();
    let subscription =
        desktop.subscribe(|s| println!("SNAPSHOT {}", serde_json::to_string(&*s).unwrap()));
    for line in std::io::stdin().lock().lines() {
        let line = line?;
        if line == "stop" {
            break;
        }
        let value: serde_json::Value = serde_json::from_str(&line)?;
        let action = match value["action"].as_str().unwrap_or("") {
            "volume" => Some(Action::Volume(value["value"].as_f64().ok_or("volume")?)),
            "mute" => Some(Action::Mute(value["value"].as_bool().ok_or("mute")?)),
            "activate" => Some(Action::TrayActivate {
                item: value["item"].as_str().ok_or("item")?.into(),
                x: 7,
                y: 9,
            }),
            "prepare" => Some(Action::TrayPrepare {
                item: value["item"].as_str().ok_or("item")?.into(),
            }),
            "menu" => Some(Action::TrayMenu {
                item: value["item"].as_str().ok_or("item")?.into(),
                entry: 1,
            }),
            "subscriptions" => {
                for _ in 0..100 {
                    let sub = desktop.subscribe(|_| {});
                    drop(sub);
                }
                None
            }
            _ => None,
        };
        let result = action.map(|a| desktop.action(a)).unwrap_or(Ok(()));
        let (tasks, retained) = desktop.bus_task_counts();
        println!(
            "RESULT {}",
            serde_json::json!({"ok":result.is_ok(),"error":result.err(),"subscriptions":desktop.subscription_count(),"bus_tasks":tasks,"retained_tasks":retained,"snapshot":&*desktop.snapshot()})
        );
    }
    drop(subscription);
    assert_eq!(desktop.subscription_count(), 0);
    drop(desktop);
    println!("SERVICES_STOP subscriptions=0 workers=0");
    Ok(())
}
