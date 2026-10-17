#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""Freeze real Qt layout/contour outputs. Offscreen/private; no live shell imports."""
import argparse
import json
import os
import signal
from pathlib import Path
import subprocess
import tempfile
import time

ROOT = Path(__file__).resolve().parent.parent


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--screenshots", type=Path)
    parser.add_argument("--scale", type=float, default=1.5, choices=[1.0, 1.5])
    args = parser.parse_args()
    cases = []
    for name, width, height, edge, center in [
        ("calendar", 310, 312, "popout", 1810),
        ("network", 310, 222, "popout", 1670),
        ("bluetooth", 310, 222, "popout", 1701),
        ("battery", 310, 222, "popout", 1732),
        ("sound", 400, 286, "right", None),
        ("sound-left", 400, 286, "left", None),
        ("launcher", 1100, 590, "top", None),
        ("launcher-bottom-reference", 1100, 590, "bottom", None),
        ("corner-before", 310, 222, "popout", 1729),
        ("corner-at", 310, 222, "popout", 1730),
        ("left-clamp", 310, 222, "popout", 0),
    ]:
        cases.append(dict(name=name, width=width, height=height, edge=edge, center=center))
    with tempfile.TemporaryDirectory(prefix="bead-placement-qt-") as directory:
        root = Path(directory)
        for script in ["layout.js", "contour.js"]:
            (root / script).write_bytes((ROOT / "spikes/reference/nacre-placement" / script).read_bytes())
        qml = '''import QtQuick
import Quickshell
import "layout.js" as Layout
import "contour.js" as Contour
ShellRoot {
 Component.onCompleted: {
  const cases = CASES;
  const output=[];
  for (const c of cases) {
   const p=c.edge==="popout" ? Layout.popout(10,1900,c.center,c.width,c.width,25) : Layout.attached(c.edge,1900,1140,c.width,c.height,0);
   const rect={x:10+p.x,y:50+p.y,width:c.width,height:c.height};
   const boxes=[{x:0,y:0,width:1920,height:50},{x:0,y:0,width:10,height:1200},{x:1910,y:0,width:10,height:1200},{x:0,y:1190,width:1920,height:10},rect];
   const shape=Contour.geometry(1920,1200,boxes,25,[]);
   output.push(Object.assign({},c,{x:rect.x,y:rect.y,joinsRight:p.joinsRight===true,body:shape.body,rim:shape.rim}));
  }
  console.log("BEAD_QT_PLACEMENT="+JSON.stringify(output));
  Qt.quit();
 }
}
'''.replace("CASES", json.dumps(cases))
        (root / "shell.qml").write_text(qml)
        for directory in ["home", "runtime", "config", "data", "cache"]:
            (root / directory).mkdir(mode=0o700)
        env = dict(os.environ, HOME=str(root / "home"), XDG_RUNTIME_DIR=str(root / "runtime"),
                   XDG_CONFIG_HOME=str(root / "config"), XDG_DATA_HOME=str(root / "data"),
                   XDG_CACHE_HOME=str(root / "cache"), QT_QPA_PLATFORM="offscreen",
                   QSG_RHI_BACKEND="software", DBUS_SYSTEM_BUS_ADDRESS="unix:path=" + str(root / "missing"))
        marker = "BEAD_QT_PLACEMENT="
        log = root / "qt.log"
        with log.open("w") as stream:
            process = subprocess.Popen(["dbus-run-session", "--", "quickshell", "--path", str(root / "shell.qml")],
                                       env=env, stdout=stream, stderr=subprocess.STDOUT, start_new_session=True)
            try:
                deadline = time.monotonic() + 8
                while marker not in log.read_text() and process.poll() is None and time.monotonic() < deadline:
                    time.sleep(0.05)
            finally:
                if process.poll() is None:
                    os.killpg(process.pid, signal.SIGTERM)
                try:
                    process.wait(timeout=3)
                except subprocess.TimeoutExpired:
                    os.killpg(process.pid, signal.SIGKILL)
                    process.wait(timeout=3)
        output = log.read_text()
        rows = [json.loads(line.split(marker, 1)[1]) for line in output.splitlines() if marker in line]
        if len(rows) != 1:
            raise RuntimeError("Private Qt reference failed: " + output[-2500:])
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(json.dumps(rows[0], indent=2) + "\n")
        print("QT_PLACEMENT_REFERENCE", len(rows[0]), "cases", args.output.name)
        if args.screenshots:
            screenshots(root, rows[0], env, args.screenshots.resolve(), args.scale)


def screenshots(root, cases, env, destination, scale):
    """Placement masks: actual QML contour and FrameRim, no content-parity claim."""
    destination.mkdir(parents=True, exist_ok=True)
    palette = json.loads((ROOT / "shell/palette.json").read_text())["colours"]
    for module in ["widgets", "services"]:
        (root / module).mkdir()
    (root / "widgets/qmldir").write_text("singleton NacreTokens 1.0 NacreTokens.qml\n")
    (root / "widgets/NacreTokens.qml").write_text('''pragma Singleton
import QtQuick
QtObject {
 property bool motionEnabled:false
 property color accent:PRIMARY
 property color orientSecondary:SECONDARY
 property color orientHighlight:Qt.tint(accent,Qt.alpha("#fff9f1",.22))
 function gradientColour(base,shaded) { return shaded; }
}
'''.replace("PRIMARY", json.dumps("#" + palette["primary"])).replace("SECONDARY", json.dumps("#" + palette["secondary"])))
    (root / "services/qmldir").write_text("singleton DesktopSettings 1.0 DesktopSettings.qml\n")
    (root / "services/DesktopSettings.qml").write_text('pragma Singleton\nimport QtQuick\nQtObject { property var data:({frameSheen:true}) }\n')
    rim = (ROOT / "spikes/reference/nacre-placement/NacreFrameRim.qml").read_text()
    (root / "NacreFrameRim.qml").write_text(rim.replace("import qs.widgets", 'import "widgets"').replace("import qs.services", 'import "services"'))
    qml = '''import QtQuick
import QtQuick.Window
import QtQuick.Shapes
import Quickshell
ShellRoot {
 id:root
 property var cases:CASES
 property int current:0
 Window {
  visible:true;width:1920;height:1200
  Rectangle {
   id:canvas;anchors.fill:parent;color:"#34373f"
   Shape {
    anchors.fill:parent;preferredRendererType:Shape.CurveRenderer
    ShapePath {fillColor:SURFACE;strokeWidth:0;fillRule:ShapePath.OddEvenFill
     PathSvg {path:root.cases[root.current].body}
    }
   }
   NacreFrameRim {
    anchors.fill:parent;contour:root.cases[root.current].rim
    engaged:!root.cases[root.current].name.startsWith("launcher")
   }
  }
 }
 Timer {
  id:timer;interval:120;running:true
  onTriggered:canvas.grabToImage(function(image) {
   const name=root.cases[root.current].name;
   if(!image.saveToFile(DEST+"/qml-"+name+"-"+SCALE+".png")) throw new Error("Cannot save placement screenshot");
   if(root.current+1<root.cases.length) {root.current++;timer.restart();}
   else {console.log("BEAD_QT_SCREENSHOTS_DONE="+root.cases.length);Qt.quit();}
  },Qt.size(1920*SCALE,1200*SCALE))
 }
}
'''.replace("CASES", json.dumps(cases)).replace("SURFACE", json.dumps("#" + palette["surface"])).replace("DEST", json.dumps(str(destination))).replace("SCALE", str(scale))
    (root / "screenshots.qml").write_text(qml)
    log = root / "screenshots.log"
    with log.open("w") as stream:
        process = subprocess.Popen(["dbus-run-session", "--", "quickshell", "--path", str(root / "screenshots.qml")],
                                   env=dict(env, QT_QUICK_BACKEND="software"), stdout=stream, stderr=subprocess.STDOUT, start_new_session=True)
        try:
            deadline = time.monotonic() + 15
            while "BEAD_QT_SCREENSHOTS_DONE=" not in log.read_text() and process.poll() is None and time.monotonic() < deadline:
                time.sleep(0.05)
        finally:
            if process.poll() is None:
                os.killpg(process.pid, signal.SIGTERM)
            try:
                process.wait(timeout=3)
            except subprocess.TimeoutExpired:
                os.killpg(process.pid, signal.SIGKILL)
                process.wait(timeout=3)
    text = log.read_text()
    if "BEAD_QT_SCREENSHOTS_DONE=" not in text or any(e in text for e in ["TypeError:", "ReferenceError:"]):
        raise RuntimeError("Private Qt screenshots failed: " + text[-2500:])
    print("QT_PLACEMENT_SCREENSHOTS", len(cases), "scale", scale)


if __name__ == "__main__":
    main()
