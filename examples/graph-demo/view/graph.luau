-- SPDX-License-Identifier: MIT
-- A seeded synthetic graph: 2,000 nodes in four groups, 10,000 edges biased towards a few hubs.
local seed = 721458
local function rand()
    seed = (1664525 * seed + 1013904223) % 4294967296
    return seed / 4294967296
end
local groups = { "notes", "people", "projects", "ideas" }
local nodes, edges = {}, {}
for i = 1, 2000 do
    nodes[i] = { id = "n" .. i, label = "Note " .. i, group = groups[(i - 1) % 4 + 1], weight = i % 50 == 0 and 6 or 1 }
end
for _ = 1, 10000 do
    local from = math.floor(rand() * 2000) + 1
    -- Every third edge attaches to one of the forty hubs.
    local to = rand() < 0.33 and (math.floor(rand() * 40) + 1) * 50 or math.floor(rand() * 2000) + 1
    if to == from then to = to % 2000 + 1 end
    edges[#edges + 1] = { from = "n" .. from, to = "n" .. to }
end
return { nodes = nodes, edges = edges }
