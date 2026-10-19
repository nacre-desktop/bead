// SPDX-License-Identifier: GPL-3.0-only
use super::*;

#[test]
fn call_ticket_reply_and_timeout_wake_once() {
    for expected in [Ok(serde_json::json!(true)), Err(timeout())] {
        let (sender, receiver) = oneshot::channel();
        let receiver = Arc::new(Mutex::new(Some(receiver)));
        let observed = receiver.clone();
        let wakes = Arc::new(AtomicU64::new(0));
        let wake_count = wakes.clone();
        let wake: CallWake = Arc::new(move || {
            assert!(
                observed
                    .lock()
                    .unwrap()
                    .as_mut()
                    .unwrap()
                    .try_recv()
                    .is_ok()
            );
            wake_count.fetch_add(1, Ordering::Relaxed);
        });
        TicketCompletion {
            sender: Some(sender),
            wake: Some(wake),
        }
        .finish(expected);
        assert_eq!(wakes.load(Ordering::Relaxed), 1);
    }
}

#[test]
fn call_ticket_drop_before_dispatch_sends_nothing() {
    let state = Arc::new(std::sync::atomic::AtomicU8::new(TICKET_QUEUED));
    let ticket = CallTicket {
        receiver: oneshot::channel().1,
        state: state.clone(),
        done: false,
    };
    drop(ticket);
    assert_eq!(state.load(Ordering::Acquire), TICKET_CANCELLED);
    assert!(
        state
            .compare_exchange(
                TICKET_QUEUED,
                TICKET_DISPATCHED,
                Ordering::AcqRel,
                Ordering::Acquire
            )
            .is_err()
    );
}

#[test]
fn call_ticket_drop_after_dispatch_does_not_cancel_or_replay() {
    let state = Arc::new(std::sync::atomic::AtomicU8::new(TICKET_DISPATCHED));
    let ticket = CallTicket {
        receiver: oneshot::channel().1,
        state: state.clone(),
        done: false,
    };
    drop(ticket);
    assert_eq!(state.load(Ordering::Acquire), TICKET_DISPATCHED);
}

#[test]
fn call_ticket_guard_rejects_changed_boot_or_method() {
    let ticket = TicketDispatch {
        state: Arc::new(std::sync::atomic::AtomicU8::new(TICKET_QUEUED)),
        generation: 9,
        guard: CallGuard {
            boot: "daemon-a".into(),
            version: 1,
        },
    };
    let method = |v| nacre_protocol::Method {
        v,
        class: nacre_protocol::Capability::System,
        timeout: 10_000,
        params: serde_json::json!({"type":"object"}),
        result: serde_json::json!({}),
    };
    let methods = BTreeMap::from([("network.set_wifi".into(), method(1))]);
    assert!(ticket_guard_matches(
        &ticket,
        9,
        "daemon-a",
        &methods,
        "network.set_wifi"
    ));
    assert!(
        !ticket_guard_matches(&ticket, 10, "daemon-a", &methods, "network.set_wifi"),
        "same-boot reconnect has a new transport generation"
    );
    assert!(!ticket_guard_matches(
        &ticket,
        9,
        "daemon-b",
        &methods,
        "network.set_wifi"
    ));
    assert!(!ticket_guard_matches(
        &ticket,
        9,
        "daemon-a",
        &BTreeMap::from([("network.set_wifi".into(), method(2))]),
        "network.set_wifi"
    ));
    assert!(!ticket_guard_matches(
        &ticket,
        9,
        "daemon-a",
        &methods,
        "network.set_wifi.extra"
    ));
}

#[test]
fn call_ticket_disconnect_shutdown_and_drop_wake_once() {
    let (sender, mut receiver) = oneshot::channel::<source::Result<Value>>();
    let wakes = Arc::new(AtomicU64::new(0));
    let wake_count = wakes.clone();
    drop(TicketCompletion {
        sender: Some(sender),
        wake: Some(Arc::new(move || {
            wake_count.fetch_add(1, Ordering::Relaxed);
        })),
    });
    assert!(matches!(
        receiver.try_recv(),
        Err(oneshot::error::TryRecvError::Closed)
    ));
    assert_eq!(wakes.load(Ordering::Relaxed), 1);
    assert!(claim_ticket_dispatch(&std::sync::atomic::AtomicU8::new(
        TICKET_QUEUED
    )));
}

#[test]
fn call_ticket_drop_closes_and_wakes_once_even_if_callback_panics() {
    let (sender, mut receiver) = oneshot::channel::<source::Result<Value>>();
    let wakes = Arc::new(AtomicU64::new(0));
    let wake_count = wakes.clone();
    let completion = TicketCompletion {
        sender: Some(sender),
        wake: Some(Arc::new(move || {
            wake_count.fetch_add(1, Ordering::Relaxed);
            panic!("test callback panic");
        })),
    };
    drop(completion);
    assert!(matches!(
        receiver.try_recv(),
        Err(oneshot::error::TryRecvError::Closed)
    ));
    assert_eq!(wakes.load(Ordering::Relaxed), 1);
}
