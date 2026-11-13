-- SPDX-License-Identifier: MIT
-- A seeded synthetic graph: 2,000 nodes, 10,000 edges biased towards a few hubs.
local ui = require("bead.ui")
local seed = 721458
local function rand()
    seed = (1664525 * seed + 1013904223) % 4294967296
    return seed / 4294967296
end
local nodes, edges = {}, {}
for i = 1, 2000 do
    nodes[i] = { id = "n" .. i, label = "Note " .. i, weight = i % 50 == 0 and 6 or 1 }
end
for _ = 1, 10000 do
    local from = math.floor(rand() * 2000) + 1
    -- Every third edge attaches to one of the forty hubs.
    local to = rand() < 0.33 and (math.floor(rand() * 40) + 1) * 50 or math.floor(rand() * 2000) + 1
    if to == from then to = to % 2000 + 1 end
    edges[#edges + 1] = { from = "n" .. from, to = "n" .. to }
end
-- Handed to Rust once: this module is pure, so its handle is cached and the tables become garbage.
return ui.graph(nodes, edges)
