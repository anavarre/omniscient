import QtQuick
import QtQuick.Controls.Basic
import Omniscient

// Single-line input in the shell's control chrome.
TextField {
    id: root
    property bool invalid: false
    property bool bare: false

    color: Theme.fg
    placeholderTextColor: Theme.placeholder
    selectionColor: Theme.selection
    selectedTextColor: Theme.fg
    font.pixelSize: Theme.body
    leftPadding: bare ? Theme.sp(6) : Theme.sp(10)
    rightPadding: leftPadding
    topPadding: Theme.sp(6)
    bottomPadding: Theme.sp(6)
    selectByMouse: true
    cursorDelegate: Rectangle {
        width: Math.max(1, Theme.sp(2))
        color: Theme.accent
        visible: root.cursorVisible
        SequentialAnimation on opacity {
            running: root.cursorVisible
            loops: Animation.Infinite
            NumberAnimation { to: 0; duration: 0 }
            PauseAnimation { duration: 500 }
            NumberAnimation { to: 1; duration: 0 }
            PauseAnimation { duration: 500 }
        }
    }
    background: Rectangle {
        visible: !root.bare
        radius: Theme.radius
        color: root.activeFocus ? Theme.focusFill : Theme.normalFill
        border.width: Theme.borderWidth
        border.color: root.invalid ? Theme.red
                    : root.activeFocus ? Theme.focusBorderStrong
                    : root.hovered ? Theme.hoverBorderStrong : Theme.normalBorder
        Behavior on border.color { ColorAnimation { duration: Theme.quick } }
        Behavior on color { ColorAnimation { duration: Theme.quick } }
    }
}
