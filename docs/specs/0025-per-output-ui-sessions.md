# Per-output UI sessions

A host may create one UI session per selected Wayland output. Each session owns its output identity, window specification, view generation, and input state; the host decides which outputs receive sessions.

Output removal retires the matching session and its resources. Re-adding an output creates a fresh session using current host configuration. A framework session must not infer product-specific output selection or persistence policy.
