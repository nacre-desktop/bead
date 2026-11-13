// SPDX-License-Identifier: MIT
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::Duration,
};
use zbus::{
    Connection, Proxy,
    zvariant::{OwnedValue, Value},
};

const NEW: &str = "org.freedesktop.UPower.PowerProfiles";
const LEGACY: &str = "net.hadess.PowerProfiles";
const PATH: &str = "/org/freedesktop/UPower/PowerProfiles";
const IFACE: &str = "org.freedesktop.UPower.PowerProfiles";
#[derive(Default)]
struct State {
    active: String,
    calls: Vec<String>,
}
struct Fake(Arc<Mutex<State>>);
#[zbus::interface(name = "org.freedesktop.UPower.PowerProfiles")]
impl Fake {
    #[zbus(property)]
    fn active_profile(&self) -> String {
        self.0.lock().unwrap().active.clone()
    }
    #[zbus(property)]
    fn set_active_profile(&self, value: String) {
        let mut state = self.0.lock().unwrap();
        state.active = value.clone();
        state.calls.push(value);
    }
    #[zbus(property)]
    fn profiles(&self) -> Vec<HashMap<String, OwnedValue>> {
        ["power-saver", "balanced", "performance"]
            .into_iter()
            .map(|name| {
                HashMap::from([(
                    "Profile".into(),
                    OwnedValue::try_from(Value::from(name)).unwrap(),
                )])
            })
            .collect()
    }
    #[zbus(property)]
    fn performance_degraded(&self) -> String {
        String::new()
    }
}
async fn fake(name: &str, active: &str) -> (Connection, Arc<Mutex<State>>) {
    let state = Arc::new(Mutex::new(State {
        active: active.into(),
        calls: Vec::new(),
    }));
    let connection = Connection::session().await.unwrap();
    connection.request_name(name).await.unwrap();
    connection
        .object_server()
        .at(PATH, Fake(state.clone()))
        .await
        .unwrap();
    (connection, state)
}
async fn wait_for(
    service: &bead_services::power_profiles::PowerProfiles,
    predicate: impl Fn(&bead_services::power_profiles::PowerProfilesSnapshot) -> bool,
) {
    let result = tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            if predicate(&service.snapshot()) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await;
    assert!(
        result.is_ok(),
        "power profiles state timeout: {:?}",
        service.snapshot()
    );
}

#[tokio::test]
#[ignore = "requires a private dbus-run-session"]
async fn power_profiles_discovery_changes_action_and_fallback() {
    assert!(std::env::var_os("DBUS_SYSTEM_BUS_ADDRESS").is_some());
    assert_eq!(
        std::env::var("BEAD_PRIVATE_SERVICES_TEST").as_deref(),
        Ok("1")
    );
    let (legacy, legacy_state) = fake(LEGACY, "balanced").await;
    let client = Connection::session().await.unwrap();
    let service = bead_services::power_profiles::PowerProfiles::start(client);
    wait_for(&service, |s| s.available && s.active == "balanced").await;
    assert_eq!(
        service.snapshot().profiles,
        vec!["power-saver", "balanced", "performance"]
            .into_iter()
            .map(str::to_owned)
            .collect::<Vec<_>>()
    );
    let props = Proxy::new(&legacy, LEGACY, PATH, IFACE).await.unwrap();
    props
        .set_property("ActiveProfile", "power-saver")
        .await
        .unwrap();
    wait_for(&service, |s| s.active == "power-saver").await;
    let service_action = service.clone();
    tokio::task::spawn_blocking(move || {
        service_action.action(
            bead_services::power_profiles::PowerProfilesAction::SetProfile("balanced".into()),
        )
    })
    .await
    .unwrap()
    .unwrap();
    assert!(
        legacy_state
            .lock()
            .unwrap()
            .calls
            .contains(&"balanced".to_owned())
    );
    assert!(
        service
            .action(
                bead_services::power_profiles::PowerProfilesAction::SetProfile("invalid".into())
            )
            .is_err()
    );
    let (primary, _) = fake(NEW, "performance").await;
    wait_for(&service, |s| s.active == "performance").await;
    assert_eq!(service.snapshot().profiles.len(), 3);
    drop(primary);
    wait_for(&service, |s| s.available && s.active == "balanced").await;
    drop(props);
    drop(legacy);
    wait_for(&service, |s| !s.available && s.profiles.is_empty()).await;
}
