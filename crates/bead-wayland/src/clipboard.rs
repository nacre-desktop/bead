// SPDX-License-Identifier: MIT OR Apache-2.0
//! Native selection transports; payloads never pass through an external command.
use super::*;
use std::{
    io::{Read, Write},
    os::fd::{AsFd, AsRawFd, FromRawFd},
    sync::Arc,
};
use wayland_client::protocol::{
    wl_data_device as device, wl_data_device_manager as manager, wl_data_offer as offer,
    wl_data_source as source,
};
use wayland_protocols::ext::data_control::v1::client::{
    ext_data_control_device_v1 as ed, ext_data_control_manager_v1 as em,
    ext_data_control_offer_v1 as eo, ext_data_control_source_v1 as es,
};
const MIME: &str = "text/plain;charset=utf-8";
pub(super) struct Clipboard {
    manager: Option<manager::WlDataDeviceManager>,
    device: Option<device::WlDataDevice>,
    selected: Option<offer::WlDataOffer>,
    sources: Vec<source::WlDataSource>,
    ext: Option<em::ExtDataControlManagerV1>,
    ext_device: Option<ed::ExtDataControlDeviceV1>,
    ext_selected: Option<eo::ExtDataControlOfferV1>,
    ext_sources: Vec<es::ExtDataControlSourceV1>,
}
impl Clipboard {
    pub fn new(globals: &wayland_client::globals::GlobalList, qh: &QueueHandle<App>) -> Self {
        Self {
            manager: globals.bind(qh, 1..=3, ()).ok(),
            device: None,
            selected: None,
            sources: vec![],
            ext: globals.bind(qh, 1..=1, ()).ok(),
            ext_device: None,
            ext_selected: None,
            ext_sources: vec![],
        }
    }
    pub fn seat(&mut self, seat: &wl_seat::WlSeat, qh: &QueueHandle<App>) {
        if self.device.is_none()
            && let Some(m) = &self.manager
        {
            self.device = Some(m.get_data_device(seat, qh, ()));
        }
        if self.ext_device.is_none()
            && let Some(m) = &self.ext
        {
            self.ext_device = Some(m.get_data_device(seat, qh, ()));
        }
    }
    pub fn set(&mut self, text: String, serial: u32, qh: &QueueHandle<App>) -> Result<(), String> {
        if text.len() > 65536 {
            return Err("clipboard text exceeds64KiB".into());
        }
        let text = Arc::new(text);
        if let (Some(manager), Some(device)) = (&self.ext, &self.ext_device) {
            let source = manager.create_data_source(qh, text);
            source.offer(MIME.into());
            source.offer("text/plain".into());
            device.set_selection(Some(&source));
            self.ext_sources.push(source);
        } else if let (Some(manager), Some(device)) = (&self.manager, &self.device) {
            if serial == 0 {
                return Err("core clipboard needs a valid input serial".into());
            }
            let source = manager.create_data_source(qh, text);
            source.offer(MIME.into());
            source.offer("text/plain".into());
            device.set_selection(Some(&source), serial);
            self.sources.push(source);
        } else {
            return Err("native clipboard protocol unavailable".into());
        }
        Ok(())
    }
    pub fn get(
        &self,
        done: impl FnOnce(Result<String, String>) + Send + 'static,
    ) -> Result<(), String> {
        let mut fds = [0; 2];
        if unsafe { libc::pipe2(fds.as_mut_ptr(), libc::O_CLOEXEC | libc::O_NONBLOCK) } != 0 {
            return Err(std::io::Error::last_os_error().to_string());
        }
        let reader = unsafe { std::fs::File::from_raw_fd(fds[0]) };
        let writer = unsafe { std::fs::File::from_raw_fd(fds[1]) };
        if let Some(offer) = &self.ext_selected {
            offer.receive(MIME.into(), writer.as_fd());
        } else if let Some(offer) = &self.selected {
            offer.receive(MIME.into(), writer.as_fd());
        } else {
            return Err("clipboard text unavailable".into());
        }
        drop(writer);
        std::thread::spawn(move || {
            done(read_pipe(reader));
        });
        Ok(())
    }
}
fn read_pipe(mut file: std::fs::File) -> Result<String, String> {
    let deadline = Instant::now() + Duration::from_secs(2);
    let mut bytes = Vec::new();
    let mut buf = [0; 4096];
    loop {
        match file.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                bytes.extend_from_slice(&buf[..n]);
                if bytes.len() > 65536 {
                    return Err("clipboard text exceeds64KiB".into());
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                if Instant::now() >= deadline {
                    return Err("clipboard read deadline".into());
                }
                let mut p = libc::pollfd {
                    fd: file.as_raw_fd(),
                    events: libc::POLLIN,
                    revents: 0,
                };
                unsafe {
                    libc::poll(&mut p, 1, 20);
                }
            }
            Err(e) => return Err(e.to_string()),
        }
    }
    String::from_utf8(bytes).map_err(|e| e.to_string())
}
fn write_pipe(fd: std::os::fd::OwnedFd, text: Arc<String>) {
    std::thread::spawn(move || {
        let mut file = std::fs::File::from(fd);
        unsafe {
            libc::fcntl(
                file.as_raw_fd(),
                libc::F_SETFL,
                libc::fcntl(file.as_raw_fd(), libc::F_GETFL) | libc::O_NONBLOCK,
            );
        }
        let deadline = Instant::now() + Duration::from_secs(2);
        let mut bytes = text.as_bytes();
        while !bytes.is_empty() {
            match file.write(bytes) {
                Ok(0) => break,
                Ok(n) => bytes = &bytes[n..],
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    if Instant::now() >= deadline {
                        break;
                    }
                    let mut p = libc::pollfd {
                        fd: file.as_raw_fd(),
                        events: libc::POLLOUT,
                        revents: 0,
                    };
                    unsafe {
                        libc::poll(&mut p, 1, 20);
                    }
                }
                Err(_) => break,
            }
        }
    });
}
impl Dispatch<manager::WlDataDeviceManager, ()> for App {
    fn event(
        _: &mut Self,
        _: &manager::WlDataDeviceManager,
        _: manager::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}
impl Dispatch<em::ExtDataControlManagerV1, ()> for App {
    fn event(
        _: &mut Self,
        _: &em::ExtDataControlManagerV1,
        _: em::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}
impl Dispatch<device::WlDataDevice, ()> for App {
    fn event(
        state: &mut Self,
        _: &device::WlDataDevice,
        event: device::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let device::Event::Selection { id } = event {
            if let Some(old) = state.clipboard.selected.take() {
                old.destroy();
            }
            state.clipboard.selected = id;
        }
    }
    wayland_client::event_created_child!(App,device::WlDataDevice,[0=>(offer::WlDataOffer,())]);
}
impl Dispatch<ed::ExtDataControlDeviceV1, ()> for App {
    fn event(
        state: &mut Self,
        _: &ed::ExtDataControlDeviceV1,
        event: ed::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let ed::Event::Selection { id } = event {
            if let Some(old) = state.clipboard.ext_selected.take() {
                old.destroy();
            }
            state.clipboard.ext_selected = id;
        }
    }
    wayland_client::event_created_child!(App,ed::ExtDataControlDeviceV1,[0=>(eo::ExtDataControlOfferV1,())]);
}
impl Dispatch<offer::WlDataOffer, ()> for App {
    fn event(
        _: &mut Self,
        _: &offer::WlDataOffer,
        _: offer::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}
impl Dispatch<eo::ExtDataControlOfferV1, ()> for App {
    fn event(
        _: &mut Self,
        _: &eo::ExtDataControlOfferV1,
        _: eo::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}
impl Dispatch<source::WlDataSource, Arc<String>> for App {
    fn event(
        state: &mut Self,
        proxy: &source::WlDataSource,
        event: source::Event,
        text: &Arc<String>,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            source::Event::Send { fd, .. } => write_pipe(fd, text.clone()),
            source::Event::Cancelled => {
                state.clipboard.sources.retain(|s| s != proxy);
                proxy.destroy();
            }
            _ => {}
        }
    }
}
impl Dispatch<es::ExtDataControlSourceV1, Arc<String>> for App {
    fn event(
        state: &mut Self,
        proxy: &es::ExtDataControlSourceV1,
        event: es::Event,
        text: &Arc<String>,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            es::Event::Send { fd, .. } => write_pipe(fd, text.clone()),
            es::Event::Cancelled => {
                state.clipboard.ext_sources.retain(|s| s != proxy);
                proxy.destroy();
            }
            _ => {}
        }
    }
}
