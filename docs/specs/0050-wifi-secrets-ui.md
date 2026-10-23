# B0050 — Wi-Fi password prompt, hidden network and saved networks

Status: **Review.** Consumes Nacre N0042 (pin `61af74be`, `nacre-protocol` additive: `network.connect {secret?, security?}`, `hidden_ssid` target, `network.forget`, saved-profile fields). Builds on B0017 (network popout) and B0042 (inline confirm).

## Behavior

- Tapping a **secured access point with no saved profile** opens a password prompt in the network popout instead of connecting. The prompt has a masked field (show/hide button), Enter or Connect to send, Cancel/back/Esc to leave. A saved profile still connects at once. A daemon refusal `validation` "Network secret required" (for example the profile vanished) opens the same prompt.
- A **wrong password** arrives asynchronously as the topic `health.error` "Network authentication required or cancelled": the prompt returns with "Wrong password. Try again." (the daemon removes the failed profile, so a retry sends the secret again). Other join errors show under the field and keep the form open.
- **Hidden network**: footer button opens a form (name, Open/WPA2/WPA3, password). Local checks (name 1..32 bytes, password present unless Open) send nothing; the call uses `target.kind=hidden_ssid`, the one Wi-Fi device, and `security`.
- **Saved networks**: footer button opens the saved Wi-Fi profiles (from `connections` rows that carry `ssid`; most recently used first, at most 6). Forget uses the B0042 inline confirm: first activation turns the row's button into "Confirm forget" (error colour), a second one within 4 s sends `network.forget`; timeout, leaving the page or any change reverts it. Both footer buttons are disabled unless the method is advertised.
- Footer/pages: the list popout grows by one 40 px row (`NETWORK_BASE_HEIGHT` 222 -> 262); prompt/hidden/saved page heights are computed in Luau from the size scale.

## Secret hygiene

The password exists only in the `network.secret` input store until the call is built. The host passes it into the call params, then clears the store (after send, cancel, Esc, or close). `*.secret` stores are never in the `--trace-state` `UI_STATE` dump and are not in the durable panel state (which only keeps `.open/.page/.route`). A masked `TextInput` draws bullets, exposes no accessibility value, gives the input method an empty context with the password content purpose and sensitive/hidden hints, and cannot be copied (cut deletes). Nothing logs params; marker lines print store names only.

## Split (framework vs Nacre edge)

- `bead-ui`: `TextInput masked`, `kit.PasswordField` (masked input + show/hide + Enter action), `kit.ConfirmButton` (inline second-activation button), `*.secret` store convention, `Runtime::sensitive_input` for IME. All general; added to ADR 0004's list.
- Luau: `shell/status/network.luau` (all pages, sizes from tokens), `shell/status/behavior.luau` (typed-field stores and local toggles).
- Nacre edge (`nacre-shell/src/status_views/join.rs`): the join/forget flow, method names, error mapping; projects `network.view|prompt_label|form_error|form_busy|can_hidden|can_forget|saved`. Host actions `network.open|cancel|join|forget_ask|forget` are registered under `network.control`.

## Checks

Fake-daemon runtime tests (`reload/join_runtime_tests.rs`): prompt without a call, typed bullets, secret sent exactly once with exact params and cleared, wrong password re-prompt, needs-secret refusal opens the prompt, hidden form validation and `hidden_ssid`+`sae` params, forget arm/timeout/late-confirm/send/readback, and the trace/summary/a11y/saved-state hygiene test. `bead-ui` test for the masked input (bullets, a11y, IME, copy, caret). Goldens `shell/tests/goldens/network-{list,prompt,reprompt,hidden,saved,saved-confirm}-{1,1.5}.png`. Existing network tests updated for the taller list.

## Not done / notes

- No nested live visual check and no memory capture (no desktop run); the relative gate is open (CPU backend). Added state is a handful of stores and at most 6 saved rows; the masked input allocates one bullet string per repaint.
- Outside-click dismissal drops the view without an Esc; the buffer then dies with the released view (memory only). Esc, Cancel and `close` clear it explicitly.
- Nacre's catalog needs `tools/i18n-sync.py NACRE --write` for the 24 new `bead.panel.network-*` ids (lead).
- Harness frames show the pre-existing list rows wider than the 310 px panel (switch/action text clipped at the frame edge); not changed here.

No temporary duplicates.
