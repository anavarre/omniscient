import QtQuick
import QtQuick.Controls.Basic
import Omniscient

Rectangle {
    property string initials: ""
    property bool dim: false
    implicitWidth: Theme.sp(20)
    implicitHeight: Theme.sp(20)
    radius: Theme.smallRadius
    color: dim ? Theme.hoverFill : Theme.fg
    Label {
        anchors.centerIn: parent
        text: parent.initials
        color: parent.dim ? Theme.fgDim : Theme.bg
        font.pixelSize: Theme.caption
        font.bold: true
    }
}
