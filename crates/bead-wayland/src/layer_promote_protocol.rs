// SPDX-License-Identifier: MIT
//! Generated client bindings for Bead's optional native layer promotion.

#![allow(
    dead_code,
    non_camel_case_types,
    unused_imports,
    unused_unsafe,
    unused_variables
)]

extern crate wayland_client;
use wayland_client::protocol::*;

pub mod __interfaces {
    use wayland_client::backend as wayland_backend;
    use wayland_client::protocol::__interfaces::*;
    wayland_scanner::generate_interfaces!("../../protocols/bead-layer-promote-v1.xml");
}

use self::__interfaces::*;
wayland_scanner::generate_client_code!("../../protocols/bead-layer-promote-v1.xml");
