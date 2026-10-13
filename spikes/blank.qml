import QtQuick
import Quickshell
import Quickshell.Wayland

ShellRoot {
    PanelWindow {
        implicitWidth: 512
        implicitHeight: 32
        anchors.top: true
        exclusiveZone: 0
        color: "#202020"
        WlrLayershell.namespace: "bead-dev-qml-spike"
        WlrLayershell.keyboardFocus: WlrKeyboardFocus.None
        Component.onCompleted: console.log("READY_QML")
    }
}
