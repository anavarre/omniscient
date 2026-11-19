import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import Omniscient

// A small tag: a person, project or task reference.
Rectangle {
    id: root
    property string text: ""
    property string glyph: ""
    property bool done: false
    property bool removable: false
    property bool clickable: false
    property color ink: Theme.fg
    signal clicked()
    signal removed()

    implicitHeight: row.implicitHeight + Theme.sp(2)
    implicitWidth: row.implicitWidth + Theme.sp(12)
    radius: Theme.smallRadius
    color: hover.hovered && clickable ? Theme.hoverFill : Theme.normalFill
    border.width: 1
    border.color: hover.hovered && clickable ? Theme.hoverBorderStrong : Theme.hairline
    Behavior on color { ColorAnimation { duration: Theme.quick } }

    HoverHandler { id: hover; cursorShape: root.clickable ? Qt.PointingHandCursor : Qt.ArrowCursor }
    // MouseAreas, not TapHandlers: a handler-only item doesn't accept the
    // press, so the click also reached whatever lies beneath (the scrim, or
    // a task row) and closed or replaced the dialog the chip sits in.
    MouseArea {
        anchors.fill: parent
        enabled: root.clickable
        onClicked: root.clicked()
    }

    RowLayout {
        id: row
        anchors.centerIn: parent
        spacing: Theme.sp(4)
        Label {
            visible: root.glyph !== ""
            text: root.glyph
            color: label.color
            font.pixelSize: Theme.caption
        }
        Label {
            id: label
            text: root.text
            color: root.done ? Theme.doneText : hover.hovered && root.clickable ? Theme.accent : root.ink
            font.pixelSize: Theme.small
            font.strikeout: root.done
            font.italic: root.done
            elide: Text.ElideRight
            Layout.maximumWidth: Theme.sp(260)
        }
        Label {
            visible: root.removable
            text: Icons.close
            color: x.hovered ? Theme.red : Theme.fgDim
            font.pixelSize: Theme.caption
            HoverHandler { id: x; cursorShape: Qt.PointingHandCursor }
            MouseArea { anchors.fill: parent; anchors.margins: -Theme.sp(3); onClicked: root.removed() }
        }
    }
}
