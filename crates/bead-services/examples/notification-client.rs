// SPDX-License-Identifier: MIT OR Apache-2.0
//! Private-bus protocol fixture. No discovery or desktop/session connection.
use futures_util::StreamExt;
use std::{collections::HashMap, time::Duration};
use zbus::{Proxy, zvariant::OwnedValue};
async fn notify(proxy: &Proxy<'_>, replaces: u32, timeout: i32) -> zbus::Result<u32> {
    proxy
        .call(
            "Notify",
            &(
                "Example",
                replaces,
                "dialog-information",
                "Fixture title",
                "Fixture body",
                vec!["open", "Open"],
                HashMap::<String, OwnedValue>::new(),
                timeout,
            ),
        )
        .await
}
async fn interactive(connection: &zbus::Connection) -> Result<(), Box<dyn std::error::Error>> {
    let proxy = Proxy::new(
        connection,
        "org.freedesktop.Notifications",
        "/org/freedesktop/Notifications",
        "org.freedesktop.Notifications",
    )
    .await?;
    let mut actions = proxy.receive_signal("ActionInvoked").await?;
    let mut closed = proxy.receive_signal("NotificationClosed").await?;
    let id = notify(&proxy, 0, 1500).await?;
    println!("UI_NOTICE {}", serde_json::json!({"id":id}));
    tokio::select! {
        biased;
        action=actions.next()=>{
            let action=action.ok_or("missing action signal")?;
            assert_eq!(action.body().deserialize::<(u32,String)>()?,(id,"open".into()));
        },
        _=closed.next()=>return Err("interactive notification expired before UI action".into()),
        _=tokio::time::sleep(Duration::from_secs(12))=>return Err("missing UI action".into()),
    }
    let close = tokio::time::timeout(Duration::from_secs(2), closed.next())
        .await?
        .ok_or("missing action close signal")?;
    assert_eq!(close.body().deserialize::<(u32, u32)>()?, (id, 2));
    println!("UI_ACTION_PASS {}", serde_json::json!({"id":id}));
    Ok(())
}
async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let address = std::env::args()
        .nth(1)
        .ok_or("explicit private bus address required")?;
    if std::env::args().nth(2).as_deref() == Some("interactive") {
        let connection = zbus::connection::Builder::address(address.as_str())?
            .build()
            .await?;
        return interactive(&connection).await;
    }
    parity(&address).await
}
pub async fn parity(address: &str) -> Result<(), Box<dyn std::error::Error>> {
    let connection = zbus::connection::Builder::address(address)?.build().await?;
    let proxy = Proxy::new(
        &connection,
        "org.freedesktop.Notifications",
        "/org/freedesktop/Notifications",
        "org.freedesktop.Notifications",
    )
    .await?;
    let policy = Proxy::new(
        &connection,
        "org.freedesktop.Notifications",
        "/org/freedesktop/Notifications",
        "org.nacre.Bead.Notifications",
    )
    .await?;
    let mut closed = proxy.receive_signal("NotificationClosed").await?;
    let mut invoked = proxy.receive_signal("ActionInvoked").await?;
    let caps: Vec<String> = proxy.call("GetCapabilities", &()).await?;
    assert!(caps.contains(&"actions".into()));
    assert!(!caps.contains(&"body-markup".into()));
    let info: (String, String, String, String) = proxy.call("GetServerInformation", &()).await?;
    assert_eq!(info.0, "Bead");
    let id = notify(&proxy, 0, 0).await?;
    assert!(id > 0);
    assert_eq!(notify(&proxy, id, 0).await?, id);
    policy
        .call::<_, _, ()>("InvokeAction", &(id, "open"))
        .await?;
    let action = tokio::time::timeout(Duration::from_secs(2), invoked.next())
        .await?
        .ok_or("missing action signal")?;
    assert_eq!(
        action.body().deserialize::<(u32, String)>()?,
        (id, "open".into())
    );
    let close = tokio::time::timeout(Duration::from_secs(2), closed.next())
        .await?
        .ok_or("missing close signal")?;
    assert_eq!(close.body().deserialize::<(u32, u32)>()?, (id, 2));
    let id = notify(&proxy, 0, 40).await?;
    let close = tokio::time::timeout(Duration::from_secs(2), closed.next())
        .await?
        .ok_or("missing expiry signal")?;
    assert_eq!(close.body().deserialize::<(u32, u32)>()?, (id, 1));
    let id = notify(&proxy, 0, 0).await?;
    proxy.call::<_, _, ()>("CloseNotification", &(id,)).await?;
    let close = tokio::time::timeout(Duration::from_secs(2), closed.next())
        .await?
        .ok_or("missing client close signal")?;
    assert_eq!(close.body().deserialize::<(u32, u32)>()?, (id, 3));
    assert!(
        proxy
            .call::<_, _, ()>("CloseNotification", &(id,))
            .await
            .is_err()
    );
    policy.call::<_, _, ()>("SetDoNotDisturb", &(true,)).await?;
    let inhibited: bool = policy.get_property("Inhibited").await?;
    assert!(inhibited);
    let _ = notify(&proxy, 0, 0).await?;
    policy
        .call::<_, _, ()>("SetDoNotDisturb", &(false,))
        .await?;
    let cookie: u32 = policy.call("Inhibit", &("screen sharing",)).await?;
    let other = zbus::connection::Builder::address(address)?.build().await?;
    let foreign = Proxy::new(
        &other,
        "org.freedesktop.Notifications",
        "/org/freedesktop/Notifications",
        "org.nacre.Bead.Notifications",
    )
    .await?;
    assert!(
        foreign
            .call::<_, _, ()>("Uninhibit", &(cookie,))
            .await
            .is_err()
    );
    policy.call::<_, _, ()>("Uninhibit", &(cookie,)).await?;
    // Connection-owned inhibition vanishes after disconnect.
    let _: u32 = foreign.call("Inhibit", &("temporary fixture",)).await?;
    drop(foreign);
    other.close().await?;
    tokio::time::sleep(Duration::from_millis(50)).await;
    let inhibited: bool = policy.get_property("Inhibited").await?;
    assert!(!inhibited);
    let history: String = policy.call("GetHistory", &()).await?;
    let history: serde_json::Value = serde_json::from_str(&history)?;
    assert_eq!(history.as_array().ok_or("history not array")?.len(), 4);
    for channel in ["volume", "brightness", "microphone", "capslock"] {
        policy
            .call::<_, _, ()>("ShowOsd", &(channel, 0.75f64, false))
            .await?;
        println!("OSD_CHANNEL {channel}");
        tokio::time::sleep(Duration::from_millis(350)).await;
    }
    let _: u32 = proxy
        .call(
            "Notify",
            &(
                "Example",
                0u32,
                "",
                "Visible notification",
                "Actions and replacement",
                vec!["open", "Open"],
                HashMap::<String, OwnedValue>::new(),
                0i32,
            ),
        )
        .await?;
    println!(
        "NOTIFICATION_PROTOCOL_PASS replacement=true actions=true expiry=true history=true dnd=true inhibition_disconnect=true"
    );
    Ok(())
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    rt.block_on(run())
}
