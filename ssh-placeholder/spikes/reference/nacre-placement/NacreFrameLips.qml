import QtQuick
import qs.widgets
import qs.services
import qs.config

Item {
    id: root
    required property var screen
    required property var visibilities
    required property var controller
    property real focusProgress: 0
    readonly property int hoverDelay: {
        const saved = DesktopSettings.data.lipHoverDelay;
        return Number.isInteger(saved) ? Math.max(0, Math.min(800, saved)) : 350;
    }
    readonly property var rimHandles: []
    readonly property bool available: !NacrePanelState.hidden && !controller.modal && !visibilities.launcher && !visibilities.session && !visibilities.dashboardPinned && !NacreHoverIntent.fullscreenFor(screen.name)
    readonly property rect topRect: NacreFrame.headerHeight > 0 ? Qt.rect((width - 208) / 2, 0, 208, NacreFrame.headerHeight) : Qt.rect(0, 0, 0, 0)
    readonly property rect leftRect: NacreFrame.left > 0 ? Qt.rect(0, (height - 144) / 2, NacreFrame.left, 144) : Qt.rect(0, 0, 0, 0)
    readonly property rect rightRect: NacreFrame.right > 0 ? Qt.rect(width - NacreFrame.right, (height - 144) / 2, NacreFrame.right, 144) : Qt.rect(0, 0, 0, 0)
    function publishRegions() {
        if (!screen?.name)
            return;
        NacreHoverIntent.lipRegions[screen.name] = {
            dashboard: Qt.rect((width - 208) / 2, 0, 208, NacreFrame.headerHeight),
            left: leftRect,
            osd: rightRect,
            resting: {
                left: leftRect,
                osd: rightRect
            }
        };
    }
    Component.onCompleted: publishRegions()
    onTopRectChanged: publishRegions()
    onLeftRectChanged: publishRegions()
    onRightRectChanged: publishRegions()
    readonly property real dashboardProgress: dashboardDwell.progress
    readonly property bool dashboardWaiting: dashboardDwell.waiting
    function cancelDashboard() {
        dashboardDwell.cancel();
    }
    function settleDashboard() {
        if (!NacreHoverIntent.headers[screen.name])
            cancelDashboard();
    }
    function requestDashboard(buttons) {
        if (buttons !== Qt.NoButton) {
            cancelDashboard();
            return;
        }
        if (visibilities.dashboard || DesktopSettings.data.clickEdgeMenus === true || !available || !NacreHoverIntent.canOpen("dashboard", screen, buttons) || dashboardWaiting)
            return;
        dashboardDwell.start();
    }
    function sideDwell(name) {
        return name === "left" ? leftDwell : rightDwell;
    }
    function cancelSides() {
        leftDwell.cancel();
        rightDwell.cancel();
    }
    function openSide(name, pointerOwned) {
        sideDwell(name).cancel();
        if (visibilities[name]) {
            NacreHoverIntent.dismiss(screen, pointerOwned);
            visibilities[name] = false;
            if (name === "left")
                visibilities.leftPinned = false;
            if (visibilities.edgeMenu === name)
                visibilities.edgeMenu = "";
            controller.settleHover();
        } else if (name === "left")
            NacrePanelState.openEdge(name, screen.name);
        else
            NacrePanelState.openControls("home", screen.name, false, pointerOwned);
    }
    onAvailableChanged: if (!available) {
        cancelDashboard();
        cancelSides();
    }
    NacreHoverDwell {
        id: dashboardDwell
        objectName: "nacreDashboardDwell"
        duration: root.hoverDelay
        onTriggered: {
            if (!NacreHoverIntent.headers[root.screen.name] || !root.available || DesktopSettings.data.clickEdgeMenus === true || !NacreHoverIntent.canOpen("dashboard", root.screen, Qt.NoButton)) {
                root.cancelDashboard();
                return;
            }
            NacrePanelState.hoverEdge("dashboard", root.screen.name);
        }
    }
    NacreHoverDwell {
        id: leftDwell
        objectName: "nacreLeftDwell"
        duration: root.hoverDelay
        onTriggered: if (left.hovered && root.available && DesktopSettings.data.clickEdgeMenus !== true && NacreHoverIntent.canOpen("left", root.screen, Qt.NoButton))
            NacrePanelState.hoverEdge("left", root.screen.name)
    }
    NacreHoverDwell {
        id: rightDwell
        objectName: "nacreRightDwell"
        duration: root.hoverDelay
        onTriggered: if (right.hovered && root.available && DesktopSettings.data.clickEdgeMenus !== true && NacreHoverIntent.canOpen("osd", root.screen, Qt.NoButton))
            NacrePanelState.hoverEdge("osd", root.screen.name)
    }
    Connections {
        target: root.visibilities
        function onDashboardChanged() {
            if (!root.visibilities.dashboard)
                root.cancelDashboard();
        }
    }
    function approach(name, lip, buttons) {
        const point = lip.mapToItem(root, lip.pointerX, lip.pointerY);
        NacreHoverIntent.observe(screen, point.x, point.y);
        if (name === "dashboard") {
            NacreHoverIntent.setHeaderHover(screen.name, "lip", true);
            requestDashboard(buttons);
            return;
        }
        if (DesktopSettings.data.clickEdgeMenus === true || !available || !NacreHoverIntent.canOpen(name, screen, buttons))
            return;
        if (!visibilities[name] && !sideDwell(name).waiting)
            sideDwell(name).start();
    }
    function leave(name) {
        if (name === "dashboard") {
            NacreHoverIntent.setHeaderHover(screen.name, "lip", false);
            settleDashboard();
        }
        if (name !== "dashboard")
            sideDwell(name).cancel();
        NacreHoverIntent.rearm(name, screen);
        controller.settleHover();
    }
    NacreFrameLip {
        id: top
        decorationEnabled: false
        progress: root.focusProgress
        objectName: "frameTopLip"
        active: root.visibilities.dashboard
        edge: "top"
        x: root.topRect.x
        y: root.topRect.y
        width: root.topRect.width
        height: root.topRect.height
        visible: NacrePearls.drag?.kind !== "dashboard" && !NacrePanelState.hidden && width > 0 && DesktopSettings.data.dashboardEnabled !== false
        onEntered: buttons => root.approach("dashboard", this, buttons)
        onMoved: buttons => root.approach("dashboard", this, buttons)
        onExited: root.leave("dashboard")
        onClicked: {
            root.cancelDashboard();
            NacrePanelState.openEdge("dashboard", root.screen.name);
        }
    }
    NacreFrameLip {
        id: left
        decorationEnabled: false
        objectName: "frameLeftLip"
        active: root.visibilities.left
        edge: "left"
        base: NacreFrame.left
        x: root.leftRect.x
        y: root.leftRect.y
        width: root.leftRect.width
        height: root.leftRect.height
        visible: NacrePearls.drag?.kind !== "ai" && !NacrePanelState.hidden && width > 0 && DesktopSettings.data.aiSidebarEnabled !== false
        onEntered: buttons => root.approach("left", this, buttons)
        onMoved: buttons => root.approach("left", this, buttons)
        onExited: root.leave("left")
        onClicked: root.openSide("left", pointerActivation)
        NacreEdgeTab {
            objectName: "frameLeftArrow"
            anchors.fill: parent
            edge: "left"
            base: NacreFrame.left
            progress: leftDwell.progress
            active: left.active
            keyboardFocused: left.activeFocus
        }
    }
    NacreFrameLip {
        id: right
        decorationEnabled: false
        objectName: "frameRightLip"
        active: root.visibilities.osd
        edge: "right"
        base: NacreFrame.right
        x: root.rightRect.x
        y: root.rightRect.y
        width: root.rightRect.width
        height: root.rightRect.height
        visible: NacrePearls.drag?.kind !== "controls" && !NacrePanelState.hidden && width > 0 && DesktopSettings.data.controlCenterEnabled !== false
        onEntered: buttons => root.approach("osd", this, buttons)
        onMoved: buttons => root.approach("osd", this, buttons)
        onExited: root.leave("osd")
        onClicked: root.openSide("osd", pointerActivation)
        NacreEdgeTab {
            objectName: "frameRightArrow"
            anchors.fill: parent
            edge: "right"
            base: NacreFrame.right
            progress: rightDwell.progress
            active: right.active
            keyboardFocused: right.activeFocus
        }
    }
}
