// SPDX-License-Identifier: MIT
//! Polling shared by the private-bus service tests.
use std::{fmt::Debug, time::Duration};

/// Poll `snapshot` every 10 ms until `ready` holds; fail with the last state after 3 s.
pub async fn wait_for<S: Debug>(snapshot: impl Fn() -> S, ready: impl Fn(&S) -> bool) {
    let waited = tokio::time::timeout(Duration::from_secs(3), async {
        while !ready(&snapshot()) {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await;
    assert!(waited.is_ok(), "service state timeout: {:#?}", snapshot());
}
