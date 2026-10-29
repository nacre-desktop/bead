// SPDX-License-Identifier: GPL-3.0-only
// Matched ported subset of NacreAppBrowser: sidebar/grid/search, not full launcher behavior.
import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Effects
import Quickshell
import Quickshell.Io
import Quickshell.Wayland
ShellRoot {
    FileView { id: palette; path: Quickshell.env("BEAD_PALETTE_PATH"); blockLoading:true }
    FileView { id: catalog; path: Quickshell.env("BEAD_CATALOG"); blockLoading:true }
    readonly property var colours: JSON.parse(palette.text()).colours
    readonly property var entries: JSON.parse(catalog.text())
    function c(name) { return "#"+colours[name]; }
    Variants { model:Quickshell.screens
        PanelWindow { id:window; required property ShellScreen modelData; screen:modelData
            implicitWidth:1164; implicitHeight:654; anchors.top:true
            color:"transparent"; exclusionMode:ExclusionMode.Ignore; mask:Region{}
            WlrLayershell.namespace:"bead-dev-launcher-qml"; WlrLayershell.keyboardFocus:WlrKeyboardFocus.None
            property real progress:1
            SequentialAnimation on progress {running:Quickshell.env("BEAD_IDLE")==="0";loops:Animation.Infinite
                NumberAnimation {from:1;to:0;duration:1000;easing.type:Easing.OutCubic}
                NumberAnimation {from:0;to:1;duration:1000;easing.type:Easing.OutCubic}
            }
            Item {id:panel;x:32;y:32-(1-window.progress)*654;width:1100;height:590;opacity:window.progress;layer.enabled:true
                property string route:"home";property string selection:""
                Rectangle {id:body;anchors.fill:parent;radius:12;color:c("surface");border.width:1;border.color:c("outlineVariant")}
                MultiEffect {source:body;anchors.fill:body;z:-1;shadowEnabled:true;shadowColor:"#52000000";shadowBlur:1;blurMax:30;autoPaddingEnabled:true}
                Item {id:navigation;x:12;y:16;width:152;height:parent.height-98
                    Row {spacing:9
                        Image {source:Quickshell.env("BEAD_SHELL_DIR")+"/ui/assets/nacre-logo.svg";width:28;height:28;sourceSize.width:42;sourceSize.height:42}
                        Text {text:"Nacre";font.family:"IBM Plex Sans";font.pixelSize:20;color:c("onSurface");anchors.verticalCenter:parent.verticalCenter}
                    }
                    ListView {id:rail;y:44;width:parent.width;height:parent.height-y-38;clip:true;spacing:3
                        model:[{id:"home",name:"Home",icon:"home"},{id:"applications",name:"Applications",icon:"apps"},{id:"system",name:"System",icon:"computer"},{id:"settings",name:"Settings",icon:"settings"},{id:"files",name:"Files",icon:"folder"},{id:"wallpaper",name:"Wallpaper",icon:"wallpaper"}]
                        delegate:Rectangle {required property var modelData;width:rail.width;height:37;radius:9
                            property bool selected:panel.route===modelData.id
                            color:selected?Qt.tint(c("surface"),Qt.alpha(c("onSurface"),.12)):"transparent"
                            border.width:selected?1:0;border.color:Qt.alpha(c("onSurface"),.16)
                            Rectangle {x:0;y:8;width:3;height:parent.height-16;radius:1.5;color:c("primary");visible:parent.selected}
                            Row {x:11;anchors.verticalCenter:parent.verticalCenter;spacing:10
                                Text {text:parent.parent.modelData.icon;font.family:"Material Symbols Rounded";font.pixelSize:20;color:c("onSurface")}
                                Text {text:parent.parent.modelData.name;font.family:"IBM Plex Sans";font.pixelSize:14;color:c("onSurface")}
                            }
                            MouseArea {anchors.fill:parent;onClicked:panel.route=parent.modelData.id}
                        }
                    }
                }
                Rectangle {x:172;y:18;width:1;height:parent.height-110;color:c("outlineVariant")}
                Text {x:182;y:17;height:32;text:"Home";font.family:"IBM Plex Sans";font.pixelSize:16;color:c("onSurface")}
                Text {x:182;y:55;width:parent.width-202;text:"Applications";font.family:"IBM Plex Sans";font.pointSize:10;color:c("onSurfaceVariant")}
                GridView {id:grid;x:182;y:84;width:parent.width-202;height:parent.height-184;clip:true
                    cellWidth:width/Math.max(1,Math.floor(width/116));cellHeight:Math.max(72,Math.min(100,height/5))
                    model:entries
                    delegate:Rectangle {required property var modelData;width:grid.cellWidth-6;height:grid.cellHeight-5;radius:11
                        property bool selected:panel.selection===modelData.id
                        color:selected?Qt.tint(c("surface"),Qt.alpha(c("onSurface"),.09)):"transparent"
                        border.width:selected?1:0;border.color:c("outlineVariant")
                        Rectangle {x:0;y:10;width:3;height:parent.height-20;radius:1.5;color:c("primary");visible:parent.selected}
                        Image {source:Quickshell.env("BEAD_ICON_PATH");width:36;height:36;x:(parent.width-width)/2;y:7;sourceSize.width:54;sourceSize.height:54}
                        Text {x:7;y:47;width:parent.width-14;height:28;text:parent.modelData.name;textFormat:Text.PlainText;font.family:"IBM Plex Sans";font.pointSize:10;wrapMode:Text.Wrap;maximumLineCount:2;horizontalAlignment:Text.AlignHCenter;color:c("onSurface")}
                        MouseArea {anchors.fill:parent;onClicked:panel.selection=parent.modelData.id}
                    }
                }
                TextField {x:16;y:parent.height-67;width:parent.width-32;height:49;padding:15;placeholderText:"Search everything…";font.family:"IBM Plex Sans";font.pixelSize:16;color:c("onSurface")
                    background:Rectangle {radius:10;color:c("surfaceContainer")}
                }
            }
        }
    }
}
