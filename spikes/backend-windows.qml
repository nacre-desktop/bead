// SPDX-License-Identifier: GPL-3.0-only
import QtQuick
import Quickshell
ShellRoot {
    Variants {model:["A","B"]
        FloatingWindow {required property string modelData;visible:true;width:320;height:200
            id: fixture; title:"Bead fixture "+modelData
            color:modelData==="A"?"#263238":"#37474f"
            Text {anchors.centerIn:parent;text:fixture.title;color:"white"}
        }
    }
}
