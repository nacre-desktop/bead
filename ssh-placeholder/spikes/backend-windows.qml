// SPDX-License-Identifier: GPL-3.0-only
import QtQuick
import Quickshell
ShellRoot {
    Variants {model:["A","B"]
        FloatingWindow {required property string modelData;visible:true;width:320;height:200
            id: fixture
            property int ticks: 0
            property bool churn: modelData === "B" && Quickshell.env("BEAD_BACKGROUND_TITLE_CHURN") === "1"
            title:"Bead fixture "+modelData+(churn ? " "+ticks : "")
            Timer {interval:100;repeat:true;running:fixture.churn;onTriggered:fixture.ticks++}
            color:modelData==="A"?"#263238":"#37474f"
            Text {anchors.centerIn:parent;text:fixture.title;color:"white"}
        }
    }
}
