# Gate E desktop validation plan — prepared, NOT RUN

No live permission is active. QML remains the daily shell. Approval must name
which phases below may run, the output, duration and stop command immediately
before starting. This document does not grant that approval. No installation,
live config/keybinding/unit change, input injection or live notification takeover.

|Phase|Duration|Method and acceptance|
| --- | ---: | --- |
|Nested reference|10 min|Existing preview, English edits/navigation and lifecycle; preserve raw timing/CPU/memory, no physical scanout claim|
|Supervised physical input/rendering|15 min maximum|One selected output, temporary Bead surfaces, manual synthetic input; exact result commit→presentation, focus/dismissal,20 opens/closes, memory after60s idle|
|Outputs and basic hardware|10–15 min|Owner manually moves/reconnects an available display; read-only output capture, scale/cropping/focus checks; no driver/config changes|
|Optional suspend/resume|5–10 min|Separate disruptive approval; owner suspends/resumes, verify recovery and no stuck layer or helper|
|IME/screen reader, later public-release phase|30–60 min per installed engine/reader plus fixes|Composition/candidates/cancel/replacement, surrounding text, Orca reading order; explicitly deferred for the owner's English-only core switch|

A physical session with QML still visible is an additional-surface test, not a
whole-shell replacement or matched whole-system memory comparison. A second bar
can temporarily reserve work area and opening the launcher takes keyboard focus;
announce those effects. Do not switch off the owner's daily shell. Reconnect mark
and provider recovery use the existing nested test, never kill the live daemon.

## Reference and preflight commands

From the isolated Bead checkout, set BEAD_BIN and NACRE_HELPER_BIN to the exact
reported, prebuilt artifacts. No builds in the physical time window. Commands
below are prepared examples for a separately approved run, not instructions to
execute now. Keep output/machine inventory only in the private test directory.

```sh
nice -n 10 python3 scripts/try-bead.py --bead "$BEAD_BIN" --daemon "$NACRE_HELPER_BIN"
# Stop the nested preview: Ctrl+C in this terminal, or close its niri window.
```

The future physical controller below deliberately gives the daemon an unavailable
compositor/Wayland endpoint and private bus with no activation directories, so it cannot resize real windows,
change audio or run the live notification service. Only the UI gets the explicitly
borrowed physical Wayland socket. The engine sees320 synthetic apps, not personal
application/history data. Do not activate system actions; only query/navigation.
General notifications still use the clearly labeled transitional private host.

## Prepared physical launch — requires a new chat approval

The owner supplies BEAD_TEST_OUTPUT from their selected output. Read-only future
inventory command: `niri msg -j outputs > "$BEAD_TEST_ROOT/outputs-before.json"`.
Create the directory first as below. Do not post this inventory in public commits.

```sh
export BEAD_TEST_ROOT="$(mktemp -d /var/tmp/bead-live-validation.XXXXXX)"
export BEAD_REPO="$(pwd)"
# Set only immediately after the owner's separate, explicit chat approval:
# export BEAD_LIVE_APPROVAL=approved-for-this-run
nice -n 10 python3 - <<'CONTROLLER'
import importlib.util, json, os, pathlib, shutil, signal, subprocess, time
os.setpriority(os.PRIO_PROCESS, 0, 10)
assert os.environ.get('BEAD_LIVE_APPROVAL') == 'approved-for-this-run', 'No live approval'
r = pathlib.Path(os.environ['BEAD_TEST_ROOT']).resolve()
repo = pathlib.Path(os.environ['BEAD_REPO']).resolve()
assert r.parent == pathlib.Path('/var/tmp') and r.name.startswith('bead-live-validation.')
bead = pathlib.Path(os.environ['BEAD_BIN']).resolve(strict=True)
helper = pathlib.Path(os.environ['NACRE_HELPER_BIN']).resolve(strict=True)
display = pathlib.Path(os.environ['WAYLAND_DISPLAY'])
if not display.is_absolute(): display = pathlib.Path(os.environ['XDG_RUNTIME_DIR']) / display
assert display.is_socket()
for name in ['home','config','state','cache','data/applications','empty','runtime','bin']:
    (r/name).mkdir(parents=True, exist_ok=True)
(r/'runtime').chmod(0o700)
shutil.copytree(repo/'shell', r/'shell')
(r/'bin/nacrectl').symlink_to(helper)
for i in range(320):
    name = 'Alpha' if i < 160 else 'Application'
    (r/f'data/applications/fixture-{i:04}.desktop').write_text(
        f'[Desktop Entry]\nType=Application\nName={name} {i:04}\nExec=bead-fixture-never-activate\nIcon=fixture\n')
e = {k:os.environ[k] for k in ['PATH','LANG','LC_ALL','LC_CTYPE','TZ'] if k in os.environ}
e.update(HOME=str(r/'home'), XDG_CONFIG_HOME=str(r/'config'),
    XDG_STATE_HOME=str(r/'state'), XDG_CACHE_HOME=str(r/'cache'),
    XDG_DATA_HOME=str(r/'data'), XDG_DATA_DIRS=str(r/'empty'), XDG_RUNTIME_DIR=str(r/'runtime'),
    NACRE_DAEMON_SOCKET=str(r/'runtime/daemon.sock'), NACRE_COMPOSITOR='niri',
    NIRI_SOCKET=str(r/'runtime/unavailable-niri.sock'), WAYLAND_DISPLAY=str(r/'runtime/no-display'),
    DBUS_SESSION_BUS_ADDRESS='unix:path='+str(r/'runtime/bus'),
    DBUS_SYSTEM_BUS_ADDRESS='unix:path='+str(r/'runtime/bus'),
    PIPEWIRE_REMOTE='bead-private-unavailable', PULSE_SERVER='unix:'+str(r/'runtime/no-pulse'),
    NACRE_NIRI_HOVER_STREAM='0')
for key in ['DISPLAY','HYPRLAND_INSTANCE_SIGNATURE','NACRE_QS_CONFIG']:
    e.pop(key, None)
spec = importlib.util.spec_from_file_location('owned', repo/'spikes/process-clock.py')
owned = importlib.util.module_from_spec(spec); spec.loader.exec_module(owned)
processes=[]
def start(args, name, env):
    p=subprocess.Popen(args,cwd=r,env=env,stdout=(r/(name+'.log')).open('w'),
        stderr=subprocess.STDOUT,start_new_session=True)
    processes.append(p); return p
def stop(p):
    if p is not None and p.poll() is None:
        p.terminate()
        try: p.wait(timeout=5)
        except subprocess.TimeoutExpired: p.kill(); p.wait(timeout=5)
def interrupted(*_): raise KeyboardInterrupt
signal.signal(signal.SIGINT, interrupted); signal.signal(signal.SIGTERM, interrupted)
(r/'controller.pid').write_text(str(os.getpid()))
daemon=None
try:
    (r/'bus.xml').write_text('<busconfig><type>session</type><listen>'+e['DBUS_SESSION_BUS_ADDRESS']+
        '</listen><auth>EXTERNAL</auth><policy context="default"><allow send_destination="*"/>'+
        '<allow receive_sender="*"/><allow own="*"/></policy></busconfig>')
    start(['dbus-daemon','--nofork','--config-file='+str(r/'bus.xml')], 'bus', e)
    deadline=time.monotonic()+5
    while not (r/'runtime/bus').exists():
        assert time.monotonic()<deadline; time.sleep(.05)
    daemon=start([str(helper),'daemon'],'daemon',e)
    deadline=time.monotonic()+15
    while not (r/'runtime/daemon.sock').exists():
        assert daemon.poll() is None and time.monotonic()<deadline; time.sleep(.05)
    output=['--output',os.environ['BEAD_TEST_OUTPUT']]
    view=dict(e,WAYLAND_DISPLAY=str(display),BEAD_PROFILE_INPUT='clocks')
    notices=start([str(bead),'--renderer','cpu','--backend','none','--notification-bus',str(r/'runtime/bus'),
        '--notification-history',str(r/'notice-history.json'),'--command-socket',str(r/'runtime/notices.sock'),
        '--namespace','bead-approved-validation-notices','--seconds','900',*output],
        'notices',dict(view,XDG_STATE_HOME=str(r/'notice-state')))
    bar=start([str(bead),'--bar','--launcher-panel','--renderer','cpu','--panel-renderer','cpu',
        '--backend','none','--daemon-socket',e['NACRE_DAEMON_SOCKET'],
        '--command-socket',str(r/'runtime/bar.sock'),'--ui-provider',
        '--ui-notification-socket',str(r/'runtime/notices.sock'),
        '--namespace','bead-approved-validation-bar','--seconds','900',*output],
        'bar',dict(view,XDG_STATE_HOME=str(r/'bar-state')))
    print('Prepared physical session running; Ctrl+C stops only its owned processes.',flush=True)
    deadline=time.monotonic()+900
    while time.monotonic()<deadline and all(p.poll() is None for p in processes):time.sleep(.1)
except KeyboardInterrupt: pass
finally:
    for p in reversed(processes[1:]):
        if p is not daemon: stop(p)
    owned.stop_owned_tree(daemon,stop)
    if processes: stop(processes[0])
    (r/'controller.pid').unlink(missing_ok=True)
    (r/'stopped.json').write_text(json.dumps({'codes':[p.returncode for p in processes]}))
CONTROLLER
```

Do not run that block during Gate E. Its syntax is checked only. Stop with Ctrl+C
in the controller terminal. Emergency stop from another terminal, while it runs:
`kill -TERM "$(cat "$BEAD_TEST_ROOT/controller.pid")"` (use the printed private
root if this terminal does not inherit it). The controller removes that PID file
on exit, stops only children it created and checks daemon-worker identities.
No `pkill`, live service stop or global compositor operation is permitted.
Private evidence remains for review; after all recorded PIDs have exited, delete
only this explicitly created test directory. No installed cleanup is required.

## Manual sequence and observations

1. Type `a`, `ap`, backspace, selection replacement, Home/End, arrows, Tab/Shift-Tab,
   repeat and Escape. Check focus returns to the daily shell; no keyboard remains
   grabbed after close. Use synthetic strings only. Compare the exact typed reply,
   UI_COMMIT update and UI_PRESENTED update; refresh0 is unknown, not120Hz proof.
2. Open/close20 times manually, then idle60s. Use the existing owned-process PSS,
   DRM and SwapPss method on this controller's UI/daemon group. Every close must
   return within its own bar+5MiB. Preserve first, third-warm and closed samples.
   Compare against the same revision's nested320-app capture; physical presentation
   and nested scheduling stay separately labeled. No run overlaps a build.
3. Private notification command (future approved session only):
   `gdbus call --address "unix:path=$BEAD_TEST_ROOT/runtime/bus" --dest
   org.freedesktop.Notifications --object-path /org/freedesktop/Notifications
   --method org.freedesktop.Notifications.Notify 'Bead fixture' 0 ''
   'Private validation' 'Synthetic content' '[]' '{}' 8000` as one shell line.
   Test replacement/actions/expiry and the four OSD fixtures only through the
   private endpoint. Do not send a notification to the default session bus.
4. Clipboard is an optional explicit subphase: owner supplies harmless synthetic
   content and approves replacing the clipboard. No automatic personal clipboard
   read/backup/logging. Verify copy/paste, selection and close without retaining
   data in reports. Otherwise record clipboard NOT TESTED.
5. Hardware subphase: owner manually reconnects the selected external output;
   repeat the private output inventory and surface/focus/scale checks. Suspend is
   separate approval; exact disruptive command `systemctl suspend`, invoked by
   the owner. Never automate connector toggles, brightness/audio changes or lock.
   Record unavailable hardware/driver variants as untested, not passing.

## IME and screen reader — deferred, no live run planned in this block

The existing headless preedit test is reproducible with the pinned Cargo setup:
`cargo test -j2 -p bead-ui --all-features
keyboard_selection_preedit_commits_and_accessibility_survive_reload` as one line,
under nice10 and the fixed CARGO_TARGET_DIR. It tests synthesized protocol-facing
edit operations, not an installed IME's candidate window or Orca.

Before the later public-release phase, inventory the owner's chosen installed
IME/reader with `command -v ibus-daemon fcitx5 orca`; do not install or replace one.
Choose its version-specific private-bus/Wayland startup contract then, and obtain
approval for those exact commands. Test non-ASCII preedit, candidate selection,
cancel, surrounding-text deletion, cursor/selection and focus/reload/reconnect.
Static AccessKit labels are headless coverage only until an approved real reader
run. Engine/reader startup commands are intentionally not invented in advance.
That missing engine-specific procedure is a public-release prerequisite, not a
claimed ready-to-run IME validation or a core-switch completion requirement.
