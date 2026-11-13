# Graph demo

A `kit.GraphCanvas` showing a synthetic seeded graph of 2,000 nodes and 10,000 edges (`view/graph.luau`, handed to Rust once with `ui.graph`). Force-directed layout runs on a worker thread and parks when settled; hovering shows the node label, a click selects, dragging pans and the wheel zooms. The status line reports each event. Run `bead check examples/graph-demo` to validate and CPU-render it, or `bead run examples/graph-demo` inside a Wayland session.
