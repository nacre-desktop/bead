#!/usr/bin/env python3
# SPDX-License-Identifier: MIT OR Apache-2.0
"""Load only Nacre's actual QML compositor backend in a disposable proof shell.

NACRE_REFERENCE selects a read-only checkout. Paths/i18n are small lab shims;
state reduction, event transport and actions use the unchanged reference files.
"""
import json
import os
import pathlib
import shutil
import subprocess
import tempfile
import threading
import time


def sample(env, raw, compositor, memory, drm_memory, ticks, stop):
    reference = pathlib.Path(os.environ['NACRE_REFERENCE'])
    with tempfile.TemporaryDirectory(prefix='bead-qml-backend-') as directory:
        root = pathlib.Path(directory)
        services = root / 'services'
        backend = services / 'compositor'
        backend.mkdir(parents=True)
        (root / 'utils').mkdir()
        (root / 'utils' / 'Lab.qml').write_text('import QtQuick\nQtObject {}\n')
        name = 'NiriBackend' if compositor == 'niri' else 'HyprlandBackend'
        for path in (reference / 'nacre/shell/services/compositor').glob('*'):
            if path.is_file():
                shutil.copy2(path, backend)
        tools = str(reference / 'nacre/shell-tools')
        (services / 'NacrePaths.qml').write_text('pragma Singleton\nimport Quickshell\nSingleton { readonly property string compositorTools: ' + json.dumps(tools) + ' }\n')
        (services / 'NacreI18n.qml').write_text('pragma Singleton\nimport Quickshell\nSingleton { function tr(key, fallback, args) { return fallback; } }\n')
        (root / 'shell.qml').write_text('''import QtQuick
import Quickshell
import Quickshell.Wayland
import "services/compositor" as Reference
ShellRoot {
    id: root
    property var backend: Reference.BACKEND
    property string requested: ""
    property double begun: 0
    property int samples: 0
    property var latencies: []
    Connections {
        target: root.backend
        function onActiveClientChanged() {
            if (!root.requested || root.backend.activeClient?.title !== root.requested) return;
            root.latencies.push(Date.now() - root.begun);
            root.requested = "";
            root.samples++;
            if (root.samples === 20) {
                driver.stop();
                console.log("REFERENCE_RESULT " + JSON.stringify(root.latencies));
            }
        }
    }
    Timer {
        id: driver; interval: 200; running: true; repeat: true
        onTriggered: {
            if (root.requested) {
                if (Date.now() - root.begun > 2000) console.log("REFERENCE_WAIT " + root.requested + " active=" + root.backend.activeClient?.title);
                return;
            }
            const owned = root.backend.clients.filter(w => w.title.startsWith("Bead fixture"));
            if (owned.length !== 2) return;
            const next = owned.find(w => w.title !== root.backend.activeClient?.title);
            if (root.samples === 0) console.log("REFERENCE_START " + next.windowId + " active=" + root.backend.activeClient?.title);
            root.requested = next.title;
            root.begun = Date.now();
            if (!root.backend.focusWindow(next.windowId)) throw new Error("reference action rejected");
        }
    }
    PanelWindow {
        anchors { top: true; left: true }
        WlrLayershell.keyboardFocus: WlrKeyboardFocus.None
        implicitWidth: 760; implicitHeight: 112; exclusiveZone: 0; color: "#172126"
        Column {
            x: 12; y: 12; spacing: 8
            Row { spacing: 6
                Repeater {
                    model: root.backend.workspaces.values
                    delegate: Rectangle {
                        required property var modelData; width: 44; height: 32
                        color: modelData.active ? "#49677b" : "#263238"
                        Text { anchors.centerIn: parent; color: "white"; text: modelData.name || modelData.index || modelData.id }
                    }
                }
            }
            Text { text: root.backend.activeClient?.title || ""; color: "white"; font.pixelSize: 16 }
        }
    }
}
'''.replace('Reference.BACKEND', 'Reference.' + name))
        lines = []
        condition = threading.Condition()
        client = subprocess.Popen(['quickshell', '--log-rules', 'qml.debug=true', '--path', str(root / 'shell.qml')], env={**env, 'NACRE_COMPOSITOR': compositor}, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True, bufsize=1, start_new_session=True)

        def drain():
            with (raw / f'{compositor}-qml-reference.log').open('w') as log:
                for line in client.stdout:
                    log.write(line)
                    log.flush()
                    with condition:
                        lines.append(line)
                        condition.notify_all()

        reader = threading.Thread(target=drain)
        reader.start()
        try:
            deadline = time.monotonic() + 30
            with condition:
                while not any('REFERENCE_RESULT ' in l for l in lines):
                    if client.poll() is not None or time.monotonic() > deadline:
                        raise RuntimeError('QML reference timeout; inspect private reference log')
                    condition.wait(.05)
            line = next(l for l in lines if 'REFERENCE_RESULT ' in l)
            latencies = sorted(json.loads(line.split('REFERENCE_RESULT ', 1)[1]))
            # Count the persistent Python event helper as well as the QML client.
            time.sleep(1)  # Let the last short-lived action helper exit.
            pids = [client.pid]
            for pid in pids:
                children = pathlib.Path(f'/proc/{pid}/task/{pid}/children')
                if children.exists():
                    pids.extend(int(p) for p in children.read_text().split())
            before = sum(ticks(p) for p in pids)
            begun = time.monotonic()
            time.sleep(2 if os.environ.get('BEAD_SKIP_IDLE') else 60)
            elapsed = time.monotonic() - begun
            cpu = 100 * (sum(ticks(p) for p in pids) - before) / os.sysconf('SC_CLK_TCK') / elapsed
            pss = sum(memory(p)['pss_mib'] for p in pids)
            drm = sum(sum((drm_memory(p) or {}).values()) for p in pids)
            print(f'BACKEND_REFERENCE {compositor} QML_state_p50_p99={latencies[9]}/{latencies[-1]}ms samples=20 clock=Date.now_ms CPU={cpu:.3f}% seconds={elapsed:.3f} PSS_plus_DRM={pss+drm:.3f}MiB processes={len(pids)}', flush=True)
        finally:
            stop(client)
            reader.join(timeout=5)
