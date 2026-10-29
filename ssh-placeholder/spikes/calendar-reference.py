#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""Unchanged Nacre calendar/header with private, deterministic data transports."""
import datetime
import importlib.util
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import time

ROOT = Path(__file__).resolve().parent.parent
spec = importlib.util.spec_from_file_location('matrix', ROOT / 'spikes/bar-matrix.py')
matrix = importlib.util.module_from_spec(spec)
spec.loader.exec_module(matrix)


def sample(env, raw, compositor, stop):
    runtime = Path(env['XDG_RUNTIME_DIR']).resolve()
    assert runtime != Path(f'/run/user/{os.getuid()}'), 'live runtime refused'
    assert Path(env['WAYLAND_DISPLAY']).resolve().is_relative_to(runtime), 'live display refused'
    private = dict(env, DBUS_SYSTEM_BUS_ADDRESS=env['DBUS_SESSION_BUS_ADDRESS'],
                   PIPEWIRE_REMOTE='bead-private-unavailable')
    today = datetime.datetime.now().astimezone()
    with tempfile.TemporaryDirectory(prefix='bead-calendar-reference-') as directory:
        root = Path(directory)
        reference = matrix.reference.fixture(root)
        compositor_fixture = root / 'services/NacreCompositor.qml'
        text = compositor_fixture.read_text()
        compositor_fixture.write_text(text[:text.rfind('}')] + ' property var monitors:({values:[]}); property var workspaces:({values:[]}) }\n')
        popup = root / 'modules/bar/popouts'
        popup.mkdir(parents=True)
        shutil.copyfile(reference / 'modules/bar/popouts/NacreCalendarPopup.qml', popup / 'NacreCalendarPopup.qml')
        overview = root / 'modules/dashboard/overview'
        overview.mkdir(parents=True)
        shutil.copyfile(reference / 'modules/dashboard/overview/overview.js', overview / 'overview.js')
        # These are transport/formatting fixtures; calendar geometry, controls,
        # date calculation and Nacre widgets remain the actual reference files.
        i18n = root / 'services/NacreI18n.qml'
        text = i18n.read_text()
        text = text[:text.rfind('}')] + '''
            property var locale:Qt.locale("en_GB")
            function date(value, pattern) {return new Date(value).toLocaleDateString(locale, pattern)}
            function number(value) {return String(value)}
            function dayName(day, format) {return locale.dayName(day, format)}
        }\n'''
        i18n.write_text(text)
        (root / 'services/NacreTime.qml').write_text(
            'pragma Singleton\nimport QtQuick\nQtObject {property date date:new Date('
            + f'{today.year},{today.month-1},{today.day},12); '
            + 'function format(pattern) {return ' + json.dumps(today.strftime('%H:%M')) + '}}\n')
        path = root / 'shell.qml'
        text = path.read_text().replace('import "modules/topbar"',
            'import "modules/topbar"\nimport "modules/bar/popouts"\nimport "widgets"')
        text = text[:text.rfind('}')] + '''
            PanelWindow {
                anchors {top:true;left:true}
                margins.top:70
                implicitWidth:374;implicitHeight:376
                exclusiveZone:-1;color:"transparent"
                WlrLayershell.namespace:"bead-reference-calendar"
                NacreSurface {
                    anchors.fill:parent;anchors.margins:32
                    radius:12;color:NacreTokens.body
                    NacreCalendarPopup {x:15;y:15;width:280;height:282;firstWeekday:1}
                }
            }
        }\n'''
        path.write_text(text)
        log = raw / 'qml-calendar.log'
        with log.open('w') as output:
            client = subprocess.Popen(['quickshell', '--path', str(path)], env=private,
                stdout=output, stderr=subprocess.STDOUT, start_new_session=True)
            try:
                time.sleep(4)
                assert client.poll() is None, 'QML calendar exited; inspect private log'
                text = log.read_text()
                assert not any(error in text for error in ['TypeError:', 'ReferenceError:', 'is not a type']), 'QML fixture has a runtime/type error'
                subprocess.run(['grim', str(raw / 'qml-calendar.png')], env=private,
                    capture_output=True, check=True, timeout=8)
                result = matrix.idle(client.pid, float(os.environ.get('BEAD_REFERENCE_SECONDS', '60')))
                result.update(compositor=compositor, source_revision=os.environ.get('NACRE_REFERENCE_REVISION') or subprocess.check_output(
                    ['git', '-C', str(reference), 'rev-parse', 'HEAD'], text=True).strip(),
                    date=today.strftime('%Y-%m-%d'), scope='unchanged populated NacreCalendarPopup + NacreHeader; formatting/transport shims; static date fixture; no native service-cost or physical timing claim')
                (raw / 'qml-calendar.json').write_text(json.dumps(result, indent=2) + '\n')
                print('QML_CALENDAR_REFERENCE compositor=' + compositor + ' pss_plus_drm_mib=' + str(result['pss_plus_drm_mib']), flush=True)
            finally:
                stop(client)
