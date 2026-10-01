import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import Omniscient

// The shell's control chrome: [controls] normal/hover/pressed/focus
// fills and borders from shell.toml, radius from Hyprland.
//   kind: "normal" (bordered) | "primary" | "danger" | "flat" | "icon"
AbstractButton {
    id: root
    property string glyph: ""
    property string kind: "normal"
    property string hint: ""      // keyboard shortcut shown after the label
    property color tint: Theme.fg // flat/icon ink
    property bool active: false   // toggled state (segmented, nav)

    readonly property bool primary: kind === "primary"
    readonly property bool danger: kind === "danger"
    readonly property bool bordered: kind === "normal" || primary || danger

    focusPolicy: Qt.TabFocus
    hoverEnabled: true
    padding: kind === "icon" ? Theme.sp(4) : Theme.sp(6)
    leftPadding: kind === "icon" ? Theme.sp(6) : Theme.sp(12)
    rightPadding: leftPadding
    font.pixelSize: Theme.small
    opacity: enabled ? 1 : 0.45

    HoverHandler { cursorShape: Qt.PointingHandCursor }

    contentItem: RowLayout {
        spacing: Theme.sp(6)
        Label {
            visible: root.glyph !== ""
            text: root.glyph
            color: label.color
            font.pixelSize: root.font.pixelSize
            Layout.alignment: Qt.AlignVCenter
        }
        Label {
            id: label
            visible: root.text !== ""
            text: root.text
            font: root.font
            Layout.fillWidth: true
            elide: Text.ElideRight
            color: root.primary ? Theme.onAccent
                 : root.danger && root.hovered ? Theme.red
                 : root.active ? Theme.menuSelectedText
                 : (root.kind === "icon" || root.kind === "flat") && !root.hovered ? root.tint === Theme.fg ? Theme.fgDim : root.tint
                 : Theme.fg
            Behavior on color { ColorAnimation { duration: Theme.quick } }
        }
        Label {
            visible: root.hint !== ""
            text: root.hint
            color: root.primary ? Theme.onAccent : Theme.fgDim
            opacity: root.primary ? 0.7 : 1
            font.pixelSize: Theme.caption
            Layout.alignment: Qt.AlignVCenter
        }
    }

    background: Rectangle {
        radius: Theme.radius
        border.width: Theme.borderWidth
        border.color: root.primary ? (root.hovered ? Theme.accentHover : Theme.accent)
                    : root.danger && root.hovered ? Theme.red
                    : root.visualFocus ? Theme.focusBorderStrong
                    : root.bordered ? (root.hovered ? Theme.hoverBorderStrong : Theme.normalBorder)
                    : root.hovered ? Theme.hoverBorder : "transparent"
        color: root.primary ? (root.down ? Theme.accentPressed : root.hovered ? Theme.accentHover : Theme.accent)
             : root.danger && root.hovered ? Theme.redSoft
             : root.down ? Theme.pressedFill
             : root.active ? Theme.selectedFill
             : root.hovered || root.visualFocus ? Theme.hoverFill
             : root.bordered ? Theme.normalFill : "transparent"
        Behavior on color { ColorAnimation { duration: Theme.quick } }
        Behavior on border.color { ColorAnimation { duration: Theme.quick } }
    }
}
