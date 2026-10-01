import QtQuick
import Omniscient

// The card look shared by list rows: hairline borders that join rows of a
// section into one card, hover and selection (accent bar) states.
Rectangle {
    id: frame
    property bool first: false
    property bool selected: false
    property bool hovered: false
    property bool muted: false
    property color base: Theme.card

    color: selected ? Theme.focusFill : hovered ? Theme.hoverFill : muted ? Theme.normalFill : base
    Behavior on color { ColorAnimation { duration: Theme.quick } }

    Rectangle { visible: frame.first; anchors.top: parent.top; width: parent.width; height: 1; color: Theme.hairline }
    Rectangle { anchors.bottom: parent.bottom; width: parent.width; height: 1; color: Theme.hairline }
    Rectangle { anchors.left: parent.left; height: parent.height; width: 1; color: Theme.hairline }
    Rectangle { anchors.right: parent.right; height: parent.height; width: 1; color: Theme.hairline }
    Rectangle {
        anchors.left: parent.left
        height: parent.height
        width: Theme.sp(3)
        color: Theme.accent
        opacity: frame.selected ? 1 : 0
        Behavior on opacity { NumberAnimation { duration: Theme.quick } }
    }
}
