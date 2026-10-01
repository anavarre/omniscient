import QtQuick
import QtQuick.Controls.Basic
import Omniscient

// "Mar 5" — red when overdue, inverted when due today, faint once done.
Rectangle {
    id: root
    property string text: ""
    property string tone: ""   // "" | "overdue" | "today" | "done"
    implicitWidth: label.implicitWidth + Theme.sp(12)
    implicitHeight: label.implicitHeight + Theme.sp(2)
    radius: Theme.smallRadius
    border.width: 1
    border.color: tone === "overdue" ? Theme.redBorder : tone === "today" ? Theme.fg : tone === "done" ? "transparent" : Theme.hairline
    color: tone === "overdue" ? Theme.redSoft : tone === "today" ? Theme.fg : tone === "done" ? "transparent" : Theme.normalFill
    Label {
        id: label
        anchors.centerIn: parent
        text: root.text
        font.pixelSize: Theme.small
        font.bold: root.tone === "today"
        color: root.tone === "overdue" ? Theme.red : root.tone === "today" ? Theme.bg : root.tone === "done" ? Theme.placeholder : Theme.fgDim
    }
}
