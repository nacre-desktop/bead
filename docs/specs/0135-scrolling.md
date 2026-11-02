# Scrolling contract

Scrollable views preserve an explicit offset and clamp it when content or viewport dimensions change. Nested scroll regions consume input only while they can move in the requested direction; exhausted input may continue to an eligible ancestor.

Virtualized lists bound materialized rows independently of total item count. Stable IDs preserve row identity through updates. Hosts provide content and input policy; the framework owns layout, clipping, hit testing, and scroll mechanics.
