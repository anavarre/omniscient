import QtQuick
import QtQuick.Controls.Basic
import Omniscient

Rectangle {
    property string text: ""
    implicitWidth: Math.max(Theme.sp(18), label.implicitWidth + Theme.sp(10))
    implicitHeight: label.implicitHeight + Theme.sp(4)
    radius: Theme.smallRadius
    color: Theme.normalFill
    border.width: 1
    border.color: Theme.normalBorder
    Rectangle { anchors.bottom: parent.bottom; anchors.left: parent.left; anchors.right: parent.right; height: 1; color: Theme.normalBorder }
    Label { id: label; anchors.centerIn: parent; text: parent.text; color: Theme.fg; font.pixelSize: Theme.small }
}
