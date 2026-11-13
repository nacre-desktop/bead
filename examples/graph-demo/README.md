# Graph demo

A `kit.GraphCanvas` showing a synthetic seeded graph of 2,000 nodes and 10,000 edges (`view/graph.luau`). Force-directed layout runs on a worker thread and parks when settled; hovering shows the node label, a click selects, a double click activates, dragging the background pans, dragging a node pins it and the wheel zooms. The status line reports each event. Run `bead check examples/graph-demo` to validate and CPU-render it, or `bead run examples/graph-demo` inside a Wayland session.

This is a provisional CPU-path demo. Phase 2a exceeds its retained-memory budget; see spec 0232 for measurements and the acceptance blocker.
