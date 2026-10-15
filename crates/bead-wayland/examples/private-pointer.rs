// SPDX-License-Identifier: MIT OR Apache-2.0
//! A private-compositor input actuator, never an evdev/uinput desktop injector.
use smithay_client_toolkit::reexports::protocols_wlr::virtual_pointer::v1::client::{
    zwlr_virtual_pointer_manager_v1::ZwlrVirtualPointerManagerV1,
    zwlr_virtual_pointer_v1::ZwlrVirtualPointerV1,
};
use std::io::{self, BufRead};
use wayland_client::{
    Connection, Dispatch, QueueHandle, delegate_noop,
    globals::{GlobalListContents, registry_queue_init},
    protocol::{wl_pointer, wl_registry},
};
struct State;
impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for State {
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
delegate_noop!(State: ignore ZwlrVirtualPointerManagerV1);
delegate_noop!(State: ignore ZwlrVirtualPointerV1);
fn main() -> Result<(), Box<dyn std::error::Error>> {
    if std::env::var("BEAD_PRIVATE_INPUT_PROOF").as_deref() != Ok("1") {
        return Err("private nested proof environment required".into());
    }
    let connection = Connection::connect_to_env()?;
    let (globals, mut events) = registry_queue_init::<State>(&connection)?;
    let qh = events.handle();
    let manager: ZwlrVirtualPointerManagerV1 = globals.bind(&qh, 1..=2, ())?;
    let pointer = manager.create_virtual_pointer(None, &qh, ());
    let mut state = State;
    events.roundtrip(&mut state)?;
    println!("POINTER_READY {{}}");
    for line in io::stdin().lock().lines() {
        let line = line?;
        let fields: Vec<_> = line.trim_matches('"').split_whitespace().collect();
        match fields.as_slice() {
            ["move", x, y] => pointer.motion_absolute(0, x.parse()?, y.parse()?, 1920, 1200),
            ["scroll", delta] => {
                let delta: f64 = delta.parse()?;
                if !delta.is_finite() || delta.abs() > 1000. {
                    return Err("invalid scroll".into());
                }
                pointer.axis(0, wl_pointer::Axis::VerticalScroll, delta);
            }
            ["click"] => {
                pointer.button(0, 0x110, wl_pointer::ButtonState::Pressed);
                pointer.frame();
                pointer.button(0, 0x110, wl_pointer::ButtonState::Released);
            }
            ["press"] => pointer.button(0, 0x110, wl_pointer::ButtonState::Pressed),
            ["release"] => pointer.button(0, 0x110, wl_pointer::ButtonState::Released),
            ["stop"] => break,
            _ => {
                return Err(
                    "expected move X Y, scroll DELTA, click, press, release or stop".into(),
                );
            }
        }
        pointer.frame();
        events.roundtrip(&mut state)?;
        println!("POINTER_DONE {{}}");
    }
    pointer.destroy();
    connection.flush()?;
    Ok(())
}
