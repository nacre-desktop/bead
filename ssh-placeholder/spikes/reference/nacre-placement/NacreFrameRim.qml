import QtQuick
import QtQuick.Shapes
import qs.widgets
import qs.services

Shape {
    id: root
    required property string contour
    property bool engaged: false
    preferredRendererType: Shape.CurveRenderer
    enabled: DesktopSettings.data.frameSheen !== false
    opacity: enabled ? (engaged ? .25 : .16) : 0
    Behavior on opacity {
        NumberAnimation {
            id: fade
            duration: root.visible && NacreTokens.motionEnabled ? 180 : 0
        }
    }
    onVisibleChanged: if (!visible)
        fade.complete()
    Connections {
        target: NacreTokens
        function onMotionEnabledChanged() {
            if (!NacreTokens.motionEnabled)
                fade.complete();
        }
    }
    ShapePath {
        fillColor: "transparent"
        strokeWidth: 1
        capStyle: ShapePath.FlatCap
        strokeGradient: LinearGradient {
            x1: 0
            y1: 0
            x2: root.width
            y2: root.height
            GradientStop {
                position: 0
                color: NacreTokens.orientHighlight
            }
            GradientStop {
                position: .38
                color: NacreTokens.gradientColour(NacreTokens.orientHighlight, NacreTokens.accent)
            }
            GradientStop {
                position: 1
                color: NacreTokens.gradientColour(NacreTokens.orientHighlight, NacreTokens.orientSecondary)
            }
        }
        PathSvg {
            path: root.contour
        }
    }
}
