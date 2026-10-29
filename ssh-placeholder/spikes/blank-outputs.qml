// Matched Feature 1 fixture: only one static strip per output, no Nacre services.
import QtQuick
import Quickshell
import Quickshell.Wayland

ShellRoot {
    Variants {
        model: Quickshell.screens
        PanelWindow {
            required property ShellScreen modelData
            screen: modelData
            implicitHeight: 32
            anchors { top: true; left: true; right: true }
            exclusiveZone: 0
            color: "#202020"
            mask: Region {}
            WlrLayershell.namespace: "bead-dev-qml"
            WlrLayershell.keyboardFocus: WlrKeyboardFocus.None
        }
    }
}
