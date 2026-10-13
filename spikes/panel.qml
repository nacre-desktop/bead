// SPDX-License-Identifier: GPL-3.0-only
// Matched Feature 2 primitive workload, not the complete launcher application.
import QtQuick
import QtQuick.Effects
import Quickshell
import Quickshell.Io
import Quickshell.Wayland

ShellRoot {
    Component.onCompleted: console.info("READY")
    FileView { id: palette; path: Quickshell.env("BEAD_PALETTE_PATH"); blockLoading: true }
    readonly property var colors: JSON.parse(palette.text()).colours
    function c(name) { return "#" + colors[name]; }
    Variants {
        model: Quickshell.screens
        PanelWindow {
            id: window
            required property ShellScreen modelData
            screen: modelData
            anchors { top: true; bottom: true; left: true; right: true }
            color: "transparent"
            exclusionMode: ExclusionMode.Ignore
            mask: Region {}
            WlrLayershell.namespace: "bead-dev-f2-qml"
            WlrLayershell.keyboardFocus: WlrKeyboardFocus.None
            property real progress: Quickshell.env("BEAD_IDLE") === "1" ? 1 : 0
            readonly property real eased: progress * progress * (3 - 2 * progress)
            SequentialAnimation on progress {
                running: Quickshell.env("BEAD_IDLE") !== "1"
                loops: Animation.Infinite
                NumberAnimation { from: 0; to: 1; duration: 1000 }
                NumberAnimation { from: 1; to: 0; duration: 1000 }
            }
            Item {
                id: panel
                x: Math.max(0, (window.width - 1100) / 2)
                y: Math.round((-650 + window.eased * 680) * window.screen.devicePixelRatio) / window.screen.devicePixelRatio
                width: 1100; height: 590
                Rectangle {
                    id: body
                    anchors.fill: parent
                    radius: 12
                    color: c("surface")
                    border.width: 1; border.color: c("outlineVariant")
                }
                MultiEffect {
                    source: body; anchors.fill: body; z: -1
                    shadowEnabled: true; shadowColor: "#52000000"
                    shadowBlur: 1; blurMax: 30; shadowVerticalOffset: 0
                    autoPaddingEnabled: true
                }
                Rectangle { x: 182; y: 20; width: 898; height: 48; radius: 13; color: c("surfaceContainer") }
                Rectangle { x: 12; y: 84; width: 150; height: 40; radius: 10; color: c("surfaceContainer") }
                Text { x: 202; y: 30; width: 860; height: 32; text: "Search applications…"; font.family: "IBM Plex Sans"; font.pixelSize: 18; color: c("onSurfaceVariant") }
                Repeater {
                    model: ["Home", "Applications", "System", "Settings", "Files", "Wallpaper"]
                    Text { required property int index; required property string modelData; x: 28; y: 93 + index * 48; width: 120; height: 26; text: modelData; font.family: "IBM Plex Sans"; font.pixelSize: 16; color: c(index === 0 ? "primary" : "onSurface") }
                }
                Text { x: 202; y: 83; width: 860; height: 34; text: "Applications"; font.family: "IBM Plex Sans"; font.pixelSize: 22; color: c("onSurface") }
                Repeater {
                    model: 27
                    Item {
                        required property int index
                        x: 202 + (index % 3) * 290; y: 128 + Math.floor(index / 3) * 43
                        Text { width: 270; height: 23; text: "Application " + String(parent.index + 1).padStart(2, "0") + " — Open"; font.family: "IBM Plex Sans"; font.pixelSize: 16; color: c("onSurface") }
                        Text { y: 21; width: 270; height: 20; text: "System tools and everyday tasks"; font.family: "IBM Plex Sans"; font.pixelSize: 12; color: c("onSurfaceVariant") }
                    }
                }
                Text { x: 202; y: 535; width: 820; height: 30; text: "Font fallback: Ελληνικά · العربية · 日本語 · 😀 🌈 🚀"; font.family: "IBM Plex Sans"; font.pixelSize: 16; color: c("onSurface") }
                Image { x: 28; y: 422; width: 48; height: 48; source: "file://" + Quickshell.env("BEAD_SHELL_DIR") + "/sample.svg"; sourceSize: Qt.size(72,72) }
                Image { x: 96; y: 422; width: 48; height: 48; source: "file://" + Quickshell.env("BEAD_SHELL_DIR") + "/sample.png"; sourceSize: Qt.size(72,72) }
                Image { x: 62; y: 490; width: 48; height: 48; source: Quickshell.env("BEAD_ICON_PATH") || Quickshell.iconPath("folder"); sourceSize: Qt.size(72,72) }
            }
        }
    }
}
