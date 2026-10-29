pragma Singleton
import QtQuick
import qs.services
import qs.widgets

QtObject {
    readonly property color colour: NacreTokens.body
    readonly property int thickness: DesktopSettings.data.frameWidth ?? 10
    readonly property int headerHeight: (DesktopSettings.data.topBarEnabled === false ? 0 : 40) + (DesktopSettings.data.frameTopEnabled === false ? 0 : thickness)
    readonly property int left: DesktopSettings.data.frameLeftEnabled === false ? 0 : thickness
    readonly property int right: DesktopSettings.data.frameRightEnabled === false ? 0 : thickness
    readonly property int bottom: DesktopSettings.data.frameBottomEnabled === false ? 0 : thickness
    readonly property int rounding: DesktopSettings.data.frameRounding ?? 25
}
