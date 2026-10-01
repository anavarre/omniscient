import QtQuick
import QtQuick.Controls.Basic
import Omniscient

// Task checkbox. While a completion is pending it shows the undo ring;
// clicking again cancels.
Item {
    id: root
    property bool checked: false
    property double pendingUntil: 0
    property bool blocked: false
    signal toggled()
    implicitWidth: Theme.sp(16)
    implicitHeight: Theme.sp(16)

    Rectangle {
        anchors.fill: parent
        visible: root.pendingUntil <= 0
        radius: Theme.smallRadius
        color: root.checked ? (hover.hovered ? Theme.fgDim : Theme.fg) : hover.hovered ? Theme.hoverFill : "transparent"
        border.width: Theme.fontBase >= 14 ? 2 : Math.max(1, Theme.borderWidth)
        border.color: root.checked ? color : hover.hovered && !root.blocked ? Theme.fg : Theme.fgDim
        Behavior on color { ColorAnimation { duration: Theme.quick } }
        Label {
            anchors.centerIn: parent
            text: Icons.check
            font.pixelSize: Theme.caption
            color: root.checked ? Theme.bg : hover.hovered && !root.blocked ? Theme.fgDim : "transparent"
        }
    }
    Ring {
        anchors.fill: parent
        visible: root.pendingUntil > 0
        until: root.pendingUntil
        Label {
            anchors.centerIn: parent
            text: hover.hovered ? Icons.undo : Icons.check
            font.pixelSize: Theme.caption - 2
            color: Theme.accent
        }
    }
    HoverHandler { id: hover; cursorShape: Qt.PointingHandCursor }
    TapHandler { onTapped: root.toggled() }
}
