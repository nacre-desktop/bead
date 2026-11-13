// SPDX-License-Identifier: MIT
//! Retained GPU graph renderer (spec 0232, phase 2b). Node positions, node attributes and
//! edge index pairs live in GPU storage buffers uploaded per data/layout revision; a frame
//! rewrites one 96-byte camera/interaction uniform and records two instanced draws. Nodes
//! are SDF discs and edges anti-aliased line quads, both built in the vertex shader, so pan,
//! zoom and hover never tessellate or re-upload geometry. Labels are not drawn here; the
//! session places them as text with the same camera affine.
//!
//! wgpu is the only Bead GPU path that exposes a device for custom pipelines (the native-gl
//! path only exposes femtovg's canvas API); the same buffers and shader logic can follow
//! there once a raw GL binding is allowed.
use crate::Result;
use wgpu::util::DeviceExt;

/// Node flag: full strength while `Frame::any_highlighted` dims every other node and edge.
pub const HIGHLIGHTED: u32 = 1;

/// Retained per-node attributes (12 bytes on the GPU). Colours are straight RGBA8.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Node {
    /// World radius; drawn radius is clamped to 1–64 device px like the CPU painter.
    pub radius: f32,
    pub color: [u8; 4],
    pub flags: u32,
}
/// Retained edge (12 bytes on the GPU): node indices plus its own colour.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Edge {
    pub from: u32,
    pub to: u32,
    pub color: [u8; 4],
}
/// Per-frame state, all of it folded into one uniform write.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Frame {
    /// Canvas rectangle in logical px: x, y, w, h. Drawing is scissored to it.
    pub rect: [f32; 4],
    /// Device px per logical px.
    pub scale: f32,
    /// Target size in device px.
    pub viewport: [u32; 2],
    /// `screen = rect.center + (world - center) * zoom`, as the CPU camera.
    pub center: [f32; 2],
    pub zoom: f32,
    /// Below the LOD threshold: every fourth unfocused edge at `far_edge_alpha`.
    pub far: bool,
    pub far_edge_alpha: u8,
    /// True when a highlight set is active: unfocused nodes/edges get `dim_alpha`.
    pub any_highlighted: bool,
    /// Alpha applied to nodes and edges outside a non-empty highlight set.
    pub dim_alpha: u8,
    pub hover: Option<u32>,
    pub selected: Option<u32>,
    pub focus: Option<u32>,
    pub hover_color: [u8; 4],
    pub selected_color: [u8; 4],
    pub focus_color: [u8; 4],
    /// Edge stroke width in logical px.
    pub line_width: f32,
}
impl Default for Frame {
    fn default() -> Self {
        Self {
            rect: [0., 0., 1280., 800.],
            scale: 1.,
            viewport: [1280, 800],
            center: [0., 0.],
            zoom: 1.,
            far: false,
            far_edge_alpha: 85,
            any_highlighted: false,
            dim_alpha: 70,
            hover: None,
            selected: None,
            focus: None,
            hover_color: [255, 190, 40, 255],
            selected_color: [255, 255, 255, 255],
            focus_color: [120, 190, 255, 255],
            line_width: 1.,
        }
    }
}

const SHADER: &str = r#"
struct Camera {
    a: f32, bx: f32, by: f32, half_width: f32,
    viewport: vec2<f32>, far_alpha: f32, dim_alpha: f32,
    hover: u32, selected: u32, focus: u32, flags: u32,
    ring: f32, pad0: f32, pad1: f32, pad2: f32,
    hover_color: vec4<f32>, selected_color: vec4<f32>, focus_color: vec4<f32>,
};
struct Attr { radius: f32, color: u32, flags: u32 };
struct Edge { a: u32, b: u32, color: u32 };
@group(0) @binding(0) var<uniform> cam: Camera;
@group(0) @binding(1) var<storage, read> positions: array<vec2<f32>>;
@group(0) @binding(2) var<storage, read> attrs: array<Attr>;
@group(0) @binding(3) var<storage, read> edges: array<Edge>;

fn clip(px: vec2<f32>) -> vec4<f32> {
    return vec4<f32>(px.x / cam.viewport.x * 2.0 - 1.0, 1.0 - px.y / cam.viewport.y * 2.0, 0.0, 1.0);
}
fn device(p: vec2<f32>) -> vec2<f32> { return p * cam.a + vec2<f32>(cam.bx, cam.by); }
fn corner(v: u32) -> vec2<f32> {
    let x = select(-1.0, 1.0, v == 1u || v == 4u || v == 5u);
    let y = select(-1.0, 1.0, v >= 2u && v != 4u);
    return vec2<f32>(x, y);
}
fn focused(i: u32) -> bool {
    return i == cam.hover || i == cam.selected || i == cam.focus || (attrs[i].flags & 1u) != 0u;
}
fn dim(c: vec4<f32>, focus: bool) -> vec4<f32> {
    if ((cam.flags & 2u) != 0u && !focus) { return vec4<f32>(c.rgb, c.a * cam.dim_alpha); }
    return c;
}

struct NodeOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) local: vec2<f32>,
    @location(1) @interpolate(flat) r: f32,
    @location(2) @interpolate(flat) fill: vec4<f32>,
    @location(3) @interpolate(flat) ring: vec4<f32>,
};
@vertex fn node_vs(@builtin(vertex_index) v: u32, @builtin(instance_index) i: u32) -> NodeOut {
    var o: NodeOut;
    let at = attrs[i];
    o.r = clamp(at.radius * cam.a, 1.0, 64.0);
    o.fill = dim(unpack4x8unorm(at.color), focused(i));
    o.ring = vec4<f32>(0.0);
    if (i == cam.hover) { o.ring = cam.hover_color; } else if (i == cam.selected) { o.ring = cam.selected_color; }
    else if (i == cam.focus) { o.ring = cam.focus_color; }
    o.local = corner(v) * (o.r + cam.ring + 1.0);
    o.pos = clip(device(positions[i]) + o.local);
    return o;
}
@fragment fn node_fs(o: NodeOut) -> @location(0) vec4<f32> {
    let d = length(o.local);
    let inner = clamp(o.r + 0.5 - d, 0.0, 1.0);
    let outer = clamp(o.r + cam.ring + 0.5 - d, 0.0, 1.0);
    let fill = vec4<f32>(o.fill.rgb, 1.0) * o.fill.a * inner;
    let ring = vec4<f32>(o.ring.rgb, 1.0) * o.ring.a * outer;
    return fill + ring * (1.0 - fill.a);
}

struct EdgeOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) ts: vec2<f32>,
    @location(1) @interpolate(flat) len: f32,
    @location(2) @interpolate(flat) color: vec4<f32>,
};
@vertex fn edge_vs(@builtin(vertex_index) v: u32, @builtin(instance_index) i: u32) -> EdgeOut {
    var o: EdgeOut;
    o.pos = vec4<f32>(0.0);
    let e = edges[i];
    let n = arrayLength(&positions);
    if (e.a >= n || e.b >= n) { return o; }
    let focus = focused(e.a) || focused(e.b);
    let far = (cam.flags & 1u) != 0u;
    if (far && !focus && (i % 4u) != 0u) { return o; }
    let p = device(positions[e.a]);
    let q = device(positions[e.b]);
    o.len = distance(p, q);
    if (o.len <= 0.0) { return o; }
    var color = unpack4x8unorm(e.color);
    if (far) { color.a = cam.far_alpha; }
    o.color = dim(color, focus);
    let c = corner(v);
    let dir = (q - p) / o.len;
    o.ts = vec2<f32>(select(-1.0, o.len + 1.0, c.x > 0.0), c.y * (cam.half_width + 1.0));
    o.pos = clip(p + dir * o.ts.x + vec2<f32>(-dir.y, dir.x) * o.ts.y);
    return o;
}
@fragment fn edge_fs(o: EdgeOut) -> @location(0) vec4<f32> {
    let across = clamp(cam.half_width + 0.5 - abs(o.ts.y), 0.0, 1.0);
    let along = clamp(min(o.ts.x, o.len - o.ts.x) + 0.5, 0.0, 1.0);
    return vec4<f32>(o.color.rgb, 1.0) * o.color.a * across * along;
}
"#;

fn words(words: impl IntoIterator<Item = u32>) -> Vec<u8> {
    words.into_iter().flat_map(u32::to_le_bytes).collect()
}
fn storage(device: &wgpu::Device, label: &str, bytes: &[u8]) -> wgpu::Buffer {
    device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some(label),
        contents: if bytes.is_empty() { &[0; 16] } else { bytes },
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
    })
}

pub struct GraphGpu {
    nodes: wgpu::RenderPipeline,
    edges: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    uniform: wgpu::Buffer,
    positions: wgpu::Buffer,
    attrs: wgpu::Buffer,
    edge_buf: wgpu::Buffer,
    bind: wgpu::BindGroup,
    node_count: u32,
    edge_count: u32,
    frame: Frame,
}
impl GraphGpu {
    /// Builds the pipelines for `format` (the target's format; premultiplied source-over).
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("bead-graph"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let entry = |binding, ty| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        };
        let read = wgpu::BufferBindingType::Storage { read_only: true };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("bead-graph"),
            entries: &[
                entry(0, wgpu::BufferBindingType::Uniform),
                entry(1, read),
                entry(2, read),
                entry(3, read),
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("bead-graph"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = |vs, fs| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(vs),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &module,
                    entry_point: Some(vs),
                    compilation_options: Default::default(),
                    buffers: &[],
                },
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &module,
                    entry_point: Some(fs),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format,
                        blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        let uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("bead-graph-camera"),
            size: 112,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let positions = storage(device, "bead-graph-positions", &[]);
        let attrs = storage(device, "bead-graph-nodes", &[]);
        let edge_buf = storage(device, "bead-graph-edges", &[]);
        let bind = Self::bind(device, &layout, &uniform, &positions, &attrs, &edge_buf);
        Self {
            nodes: pipeline("node_vs", "node_fs"),
            edges: pipeline("edge_vs", "edge_fs"),
            layout,
            uniform,
            positions,
            attrs,
            edge_buf,
            bind,
            node_count: 0,
            edge_count: 0,
            frame: Frame::default(),
        }
    }
    fn bind(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        uniform: &wgpu::Buffer,
        positions: &wgpu::Buffer,
        attrs: &wgpu::Buffer,
        edges: &wgpu::Buffer,
    ) -> wgpu::BindGroup {
        let entries = [uniform, positions, attrs, edges].map(|b| b.as_entire_binding());
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("bead-graph"),
            layout,
            entries: &entries
                .into_iter()
                .enumerate()
                .map(|(i, resource)| wgpu::BindGroupEntry {
                    binding: i as u32,
                    resource,
                })
                .collect::<Vec<_>>(),
        })
    }
    /// Full upload for a new data revision; reallocates the retained buffers.
    pub fn upload(
        &mut self,
        device: &wgpu::Device,
        positions: &[[f32; 2]],
        nodes: &[Node],
        edges: &[Edge],
    ) -> Result<()> {
        if nodes.len() != positions.len() {
            return Err("graph node attribute count differs from positions".into());
        }
        if edges
            .iter()
            .any(|e| e.from as usize >= nodes.len() || e.to as usize >= nodes.len())
        {
            return Err("graph edge references a missing node".into());
        }
        self.positions = storage(device, "bead-graph-positions", &position_words(positions));
        self.attrs = storage(device, "bead-graph-nodes", &node_words(nodes));
        self.edge_buf = storage(
            device,
            "bead-graph-edges",
            &words(
                edges
                    .iter()
                    .flat_map(|e| [e.from, e.to, u32::from_le_bytes(e.color)]),
            ),
        );
        self.bind = Self::bind(
            device,
            &self.layout,
            &self.uniform,
            &self.positions,
            &self.attrs,
            &self.edge_buf,
        );
        self.node_count = nodes.len() as u32;
        self.edge_count = edges.len() as u32;
        Ok(())
    }
    /// Layout snapshot: rewrites positions `first..first + positions.len()` only.
    pub fn set_positions(&self, queue: &wgpu::Queue, first: usize, positions: &[[f32; 2]]) {
        self.write(
            queue,
            &self.positions,
            first * 8,
            &position_words(positions),
        );
    }
    /// Attribute/mask change (colour, radius, highlight flag) for a node range.
    pub fn set_nodes(&self, queue: &wgpu::Queue, first: usize, nodes: &[Node]) {
        self.write(queue, &self.attrs, first * 12, &node_words(nodes));
    }
    fn write(&self, queue: &wgpu::Queue, buffer: &wgpu::Buffer, offset: usize, bytes: &[u8]) {
        if !bytes.is_empty() && (offset + bytes.len()) as u64 <= buffer.size() {
            queue.write_buffer(buffer, offset as u64, bytes);
        }
    }
    /// Per-frame camera/interaction update: one 112-byte uniform write.
    pub fn frame(&mut self, queue: &wgpu::Queue, frame: Frame) {
        self.frame = frame;
        let (a, bx, by) = self.transform();
        let f = |c: [u8; 4]| c.map(|v| (f32::from(v) / 255.).to_bits());
        let flags = u32::from(frame.far) | (u32::from(frame.any_highlighted) << 1);
        let none = u32::MAX;
        let mut data = vec![
            a.to_bits(),
            bx.to_bits(),
            by.to_bits(),
            (frame.line_width * frame.scale * 0.5).max(0.5).to_bits(),
            (frame.viewport[0] as f32).to_bits(),
            (frame.viewport[1] as f32).to_bits(),
            (f32::from(frame.far_edge_alpha) / 255.).to_bits(),
            (f32::from(frame.dim_alpha) / 255.).to_bits(),
            frame.hover.unwrap_or(none),
            frame.selected.unwrap_or(none),
            frame.focus.unwrap_or(none),
            flags,
            (2. * frame.scale).to_bits(),
            0,
            0,
            0,
        ];
        data.extend(f(frame.hover_color));
        data.extend(f(frame.selected_color));
        data.extend(f(frame.focus_color));
        queue.write_buffer(&self.uniform, 0, &words(data));
    }
    /// Records the edge and node draws into `pass` (target format as in [`Self::new`]).
    /// Leaves the scissor set to the canvas rectangle.
    pub fn draw(&self, pass: &mut wgpu::RenderPass<'_>) {
        let (w, h) = (self.frame.viewport[0], self.frame.viewport[1]);
        let r = self.frame.rect;
        let s = self.frame.scale;
        let x0 = ((r[0] * s).floor().max(0.) as u32).min(w);
        let y0 = ((r[1] * s).floor().max(0.) as u32).min(h);
        let x1 = (((r[0] + r[2]) * s).ceil().max(0.) as u32).min(w);
        let y1 = (((r[1] + r[3]) * s).ceil().max(0.) as u32).min(h);
        if x1 <= x0 || y1 <= y0 || self.node_count == 0 || !self.frame.zoom.is_finite() {
            return;
        }
        pass.set_scissor_rect(x0, y0, x1 - x0, y1 - y0);
        pass.set_bind_group(0, &self.bind, &[]);
        pass.set_pipeline(&self.edges);
        pass.draw(0..6, 0..self.edge_count);
        pass.set_pipeline(&self.nodes);
        pass.draw(0..6, 0..self.node_count);
    }
    /// GPU bytes held by the retained buffers.
    pub fn gpu_bytes(&self) -> u64 {
        self.uniform.size() + self.positions.size() + self.attrs.size() + self.edge_buf.size()
    }
    fn transform(&self) -> (f32, f32, f32) {
        let f = &self.frame;
        let a = f.zoom * f.scale;
        (
            a,
            (f.rect[0] + f.rect[2] * 0.5) * f.scale - f.center[0] * a,
            (f.rect[1] + f.rect[3] * 0.5) * f.scale - f.center[1] * a,
        )
    }
}
fn position_words(positions: &[[f32; 2]]) -> Vec<u8> {
    words(positions.iter().flatten().map(|v| v.to_bits()))
}
fn node_words(nodes: &[Node]) -> Vec<u8> {
    words(
        nodes
            .iter()
            .flat_map(|n| [n.radius.to_bits(), u32::from_le_bytes(n.color), n.flags]),
    )
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::time::Instant;

    /// Any Vulkan adapter (`B0232_ADAPTER=discrete|integrated|cpu` narrows it), or None.
    pub(crate) fn gpu() -> Option<(wgpu::Adapter, wgpu::Device, wgpu::Queue)> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::VULKAN,
            display: None,
            flags: wgpu::InstanceFlags::from_build_config(),
            memory_budget_thresholds: Default::default(),
            backend_options: Default::default(),
        });
        let wanted = match std::env::var("B0232_ADAPTER").as_deref() {
            Ok("discrete") => Some(wgpu::DeviceType::DiscreteGpu),
            Ok("integrated") => Some(wgpu::DeviceType::IntegratedGpu),
            Ok("cpu") => Some(wgpu::DeviceType::Cpu),
            _ => None,
        };
        let mut adapters = pollster::block_on(instance.enumerate_adapters(wgpu::Backends::VULKAN));
        for a in &adapters {
            println!("GRAPH_GPU_CANDIDATE {:?}", a.get_info());
        }
        adapters.sort_by_key(|a| a.get_info().device_type == wgpu::DeviceType::Cpu);
        let adapter = adapters
            .into_iter()
            .find(|a| wanted.is_none_or(|t| a.get_info().device_type == t))?;
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            required_features: adapter.features() & wgpu::Features::TIMESTAMP_QUERY,
            ..Default::default()
        }))
        .ok()?;
        println!("GRAPH_GPU_ADAPTER {:?}", adapter.get_info());
        Some((adapter, device, queue))
    }
    pub(crate) fn target(
        device: &wgpu::Device,
        w: u32,
        h: u32,
    ) -> (wgpu::Texture, wgpu::TextureView) {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size: wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = texture.create_view(&Default::default());
        (texture, view)
    }
    fn wait(device: &wgpu::Device) {
        device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    }
    fn read(device: &wgpu::Device, buffer: &wgpu::Buffer) -> Vec<u8> {
        let (tx, rx) = std::sync::mpsc::channel();
        buffer
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |r| tx.send(r).unwrap());
        wait(device);
        rx.recv().unwrap().unwrap();
        let bytes = buffer.slice(..).get_mapped_range().unwrap().to_vec();
        buffer.unmap();
        bytes
    }
    pub(crate) fn pixels(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        tex: &wgpu::Texture,
        w: u32,
        h: u32,
    ) -> Vec<u8> {
        let row = (w * 4).div_ceil(256) * 256;
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: u64::from(row * h),
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let mut enc = device.create_command_encoder(&Default::default());
        enc.copy_texture_to_buffer(
            tex.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(row),
                    rows_per_image: None,
                },
            },
            tex.size(),
        );
        queue.submit([enc.finish()]);
        let bytes = read(device, &buffer);
        (0..h)
            .flat_map(|y| bytes[(y * row) as usize..(y * row + w * 4) as usize].to_vec())
            .collect()
    }
    fn pass<'a>(
        enc: &'a mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        timestamps: Option<wgpu::RenderPassTimestampWrites<'_>>,
    ) -> wgpu::RenderPass<'a> {
        enc.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("bead-graph-test"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: timestamps,
            occlusion_query_set: None,
            multiview_mask: None,
        })
    }
    fn seeded(n: usize, m: usize) -> (Vec<[f32; 2]>, Vec<Node>, Vec<Edge>) {
        let mut state = 0xb0232_u64;
        let mut next = || {
            state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
            ((state >> 32) as u32) as f32 / u32::MAX as f32
        };
        let positions = (0..n)
            .map(|_| [next() * 1480. - 740., next() * 1000. - 500.])
            .collect();
        let node = Node {
            radius: 3.,
            color: [95, 150, 210, 255],
            flags: 0,
        };
        let edges = (0..m)
            .map(|_| Edge {
                from: (next() * n as f32) as u32 % n as u32,
                to: (next() * n as f32) as u32 % n as u32,
                color: [105, 115, 130, 150],
            })
            .collect();
        (positions, vec![node; n], edges)
    }

    #[test]
    fn draws_discs_edges_rings_and_dims_outside_the_highlight() {
        let Some((_, device, queue)) = gpu() else {
            println!("GRAPH_GPU skipped: no Vulkan adapter");
            return;
        };
        let (w, h) = (64, 64);
        let (tex, view) = target(&device, w, h);
        let mut graph = GraphGpu::new(&device, wgpu::TextureFormat::Rgba8Unorm);
        let node = Node {
            radius: 3.,
            color: [95, 150, 210, 255],
            flags: 0,
        };
        let edge = Edge {
            from: 0,
            to: 1,
            color: [105, 115, 130, 255],
        };
        let positions = [[-20., 0.], [20., 0.]];
        graph
            .upload(&device, &positions, &[node, node], &[edge])
            .unwrap();
        assert!(graph.upload(&device, &positions, &[node], &[]).is_err());
        assert!(
            graph
                .upload(
                    &device,
                    &positions,
                    &[node, node],
                    &[Edge { to: 2, ..edge }]
                )
                .is_err()
        );
        let frame = Frame {
            rect: [0., 0., 64., 64.],
            viewport: [w, h],
            hover: Some(0),
            ..Frame::default()
        };
        let px = |p: Vec<u8>, x: f32, y: f32| {
            let i = (y.round() as usize * w as usize + x.round() as usize) * 4;
            [p[i], p[i + 1], p[i + 2], p[i + 3]]
        };
        let render = |graph: &mut GraphGpu, frame: Frame| {
            graph.frame(&queue, frame);
            let mut enc = device.create_command_encoder(&Default::default());
            graph.draw(&mut pass(&mut enc, &view, None));
            queue.submit([enc.finish()]);
            pixels(&device, &queue, &tex, w, h)
        };
        let p = render(&mut graph, frame);
        // Node 0 at world (-20, 0): 20 px left of the 64 px canvas centre.
        let (x, y) = (12., 32.);
        assert_eq!(px(p.clone(), x, y), node.color, "node disc");
        assert_eq!(px(p.clone(), x, y - 4.), frame.hover_color, "hover ring");
        // The 1 px edge lies on the row boundary: coverage splits across rows 31 and 32.
        let edge = u16::from(px(p.clone(), 32., 31.)[3]) + u16::from(px(p.clone(), 32., 32.)[3]);
        assert!((254..=256).contains(&edge), "edge midpoint coverage {edge}");
        assert_eq!(px(p.clone(), 32., 36.), [0; 4], "beside the edge");
        assert_eq!(px(p.clone(), 2., 2.), [0; 4], "background");
        let p = render(
            &mut graph,
            Frame {
                hover: None,
                focus: Some(0),
                ..frame
            },
        );
        assert_eq!(px(p, x, y - 4.), frame.focus_color, "focus ring");
        // A highlight mask is a node attribute write: node 1 keeps full strength, node 0 and
        // its colour dim, while the edge touching node 1 stays at full strength.
        graph.set_nodes(
            &queue,
            1,
            &[Node {
                flags: HIGHLIGHTED,
                ..node
            }],
        );
        graph.set_positions(&queue, 0, &[[-20., 10.]]);
        let p = render(
            &mut graph,
            Frame {
                hover: None,
                any_highlighted: true,
                ..frame
            },
        );
        assert_eq!(px(p.clone(), 52., 32.), node.color, "highlighted node");
        assert_eq!(px(p.clone(), 12., 32.), [0; 4], "old position");
        // Inside node 0's disc, behind the start of the full-strength edge from its centre.
        let dimmed = px(p.clone(), 10., 43.);
        assert!(
            (65..=75).contains(&dimmed[3]) && dimmed[3] < node.color[3],
            "dimmed node {dimmed:?}"
        );
        // At slope 1/4 a 1 px line covers 1/cos θ ≈ 1.03 rows per column (about 263).
        let edge = u16::from(px(p.clone(), 32., 36.)[3]) + u16::from(px(p, 32., 37.)[3]);
        assert!(
            (240..=280).contains(&edge),
            "edge into the highlight {edge}"
        );
        assert_eq!(graph.gpu_bytes(), 112 + 16 + 24 + 12);
    }

    /// Standalone bench: `cargo test -p bead-render --features wgpu --release graph_gpu -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn bench_2k_10k_frames() {
        let Some((adapter, device, queue)) = gpu() else {
            panic!("no Vulkan adapter");
        };
        let frames: usize = std::env::var("B0232_FRAMES")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(200);
        let stamps = device.features().contains(wgpu::Features::TIMESTAMP_QUERY);
        let queries = stamps.then(|| {
            (
                device.create_query_set(&wgpu::QuerySetDescriptor {
                    label: None,
                    ty: wgpu::QueryType::Timestamp,
                    count: 2,
                }),
                device.create_buffer(&wgpu::BufferDescriptor {
                    label: None,
                    size: 16,
                    usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC,
                    mapped_at_creation: false,
                }),
                device.create_buffer(&wgpu::BufferDescriptor {
                    label: None,
                    size: 16,
                    usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                }),
            )
        });
        let period = f64::from(queue.get_timestamp_period());
        let (positions, nodes, edges) = seeded(2000, 10000);
        let hovered = positions
            .iter()
            .position(|p| p[0].abs() < 600. && p[1].abs() < 380.)
            .unwrap() as u32;
        let info = adapter.get_info();
        let label = format!(
            "{} ({:?}, {})",
            info.name, info.device_type, info.driver_info
        );
        let mut graph = GraphGpu::new(&device, wgpu::TextureFormat::Rgba8Unorm);
        let t = Instant::now();
        graph.upload(&device, &positions, &nodes, &edges).unwrap();
        wait(&device);
        println!(
            "GRAPH_GPU {label} upload_ms={:.3} gpu_bytes={} timestamps={stamps}",
            t.elapsed().as_secs_f64() * 1000.,
            graph.gpu_bytes()
        );
        let t = Instant::now();
        graph.set_positions(&queue, 0, &positions);
        queue.submit([]);
        wait(&device);
        println!(
            "GRAPH_GPU {label} snapshot_ms={:.3}",
            t.elapsed().as_secs_f64() * 1000.
        );
        let stat = |mut v: Vec<f64>| {
            v.sort_by(f64::total_cmp);
            (v[v.len() / 2], v[(v.len() * 95).div_ceil(100) - 1])
        };
        for (w, h) in [(1280, 800), (2880, 1800)] {
            let (_tex, view) = target(&device, w, h);
            let scale = w as f32 / 1280.;
            for case in ["full", "pan", "zoom", "hover", "far-pan"] {
                let (mut cpu, mut wall, mut gpu) = (vec![], vec![], vec![]);
                let mut frame = Frame {
                    viewport: [w, h],
                    scale,
                    zoom: if case == "far-pan" { 0.4 } else { 1. },
                    far: case == "far-pan",
                    ..Frame::default()
                };
                for i in 0..frames + 10 {
                    match case {
                        "pan" | "far-pan" => {
                            frame.center = [4. * i as f32, 2. * i as f32].map(|v| v % 200.);
                        }
                        "zoom" => frame.zoom = 1.01f32.powi(if i % 2 == 0 { 1 } else { -1 }),
                        "hover" => frame.hover = (i % 2 == 0).then_some(hovered),
                        _ => {}
                    }
                    let t = Instant::now();
                    graph.frame(&queue, frame);
                    let mut enc = device.create_command_encoder(&Default::default());
                    graph.draw(&mut pass(
                        &mut enc,
                        &view,
                        queries
                            .as_ref()
                            .map(|(set, _, _)| wgpu::RenderPassTimestampWrites {
                                query_set: set,
                                beginning_of_pass_write_index: Some(0),
                                end_of_pass_write_index: Some(1),
                            }),
                    ));
                    if let Some((set, resolve, readback)) = &queries {
                        enc.resolve_query_set(set, 0..2, resolve, 0);
                        enc.copy_buffer_to_buffer(resolve, 0, readback, 0, 16);
                    }
                    queue.submit([enc.finish()]);
                    let c = t.elapsed().as_secs_f64() * 1000.;
                    wait(&device);
                    let f = t.elapsed().as_secs_f64() * 1000.;
                    let g = queries.as_ref().map_or(f64::NAN, |(_, _, readback)| {
                        let b = read(&device, readback);
                        let at = |i: usize| u64::from_le_bytes(b[i..i + 8].try_into().unwrap());
                        at(8).wrapping_sub(at(0)) as f64 * period / 1e6
                    });
                    if i >= 10 {
                        cpu.push(c);
                        wall.push(f);
                        gpu.push(g);
                    }
                }
                let (c, w95) = (stat(cpu), stat(wall));
                let g = stat(gpu);
                println!(
                    "GRAPH_GPU {label} {w}x{h} {case} frames={frames} cpu_ms={:.3}/{:.3} completed_ms={:.3}/{:.3} gpu_ms={:.3}/{:.3}",
                    c.0, c.1, w95.0, w95.1, g.0, g.1
                );
            }
        }
    }
}
