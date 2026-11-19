import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import Omniscient

// Plain-text notes with a Markdown preview (Ctrl+E toggles).
ColumnLayout {
    id: root
    property alias text: area.text
    property bool preview: false
    property alias area: area
    // Shift/Ctrl+Enter inside the notes saves the dialog (the text area
    // would otherwise take it as a newline).
    signal submit()
    spacing: Theme.sp(4)

    Rectangle {
        Layout.fillWidth: true
        Layout.preferredHeight: Theme.sp(140)
        radius: Theme.radius
        color: area.activeFocus ? Theme.focusFill : Theme.normalFill
        border.width: Theme.borderWidth
        border.color: area.activeFocus ? Theme.focusBorderStrong : Theme.normalBorder
        Behavior on border.color { ColorAnimation { duration: Theme.quick } }

        ScrollView {
            id: scroller
            anchors.fill: parent
            anchors.margins: Theme.borderWidth
            visible: !root.preview
            TextArea {
                id: area
                color: Theme.fg
                placeholderText: "Markdown: **bold**, *italic*, - lists, [links](https://…)"
                placeholderTextColor: Theme.placeholder
                selectionColor: Theme.selection
                selectedTextColor: Theme.fg
                font.pixelSize: Theme.body
                wrapMode: TextEdit.Wrap
                selectByMouse: true
                padding: Theme.sp(8)
                background: null
                Keys.onPressed: event => {
                    if (event.key === Qt.Key_E && (event.modifiers & Qt.ControlModifier)) {
                        root.preview = true
                        event.accepted = true
                    } else if ((event.key === Qt.Key_Return || event.key === Qt.Key_Enter)
                               && (event.modifiers & (Qt.ShiftModifier | Qt.ControlModifier))) {
                        root.submit()
                        event.accepted = true
                    }
                }
            }
        }
        ScrollView {
            anchors.fill: parent
            anchors.margins: Theme.sp(8)
            visible: root.preview
            Label {
                width: root.width - Theme.sp(16)
                text: area.text.trim() === "" ? "*Nothing yet.*" : area.text
                textFormat: Text.MarkdownText
                wrapMode: Text.Wrap
                color: Theme.fg
                linkColor: Theme.accent
                font.pixelSize: Theme.body
                onLinkActivated: link => Qt.openUrlExternally(link)
                HoverHandler { cursorShape: parent.hoveredLink ? Qt.PointingHandCursor : Qt.ArrowCursor }
            }
            TapHandler { onDoubleTapped: { root.preview = false; area.forceActiveFocus() } }
        }
    }
    RowLayout {
        Layout.fillWidth: true
        Label {
            text: root.preview ? "Preview — double-click to edit" : "Markdown supported"
            color: Theme.fgDim
            font.pixelSize: Theme.caption
            Layout.fillWidth: true
        }
        Btn {
            kind: "flat"
            text: root.preview ? "Edit" : "Preview"
            glyph: root.preview ? Icons.edit : Icons.eye
            font.pixelSize: Theme.caption
            topPadding: Theme.sp(2)
            bottomPadding: Theme.sp(2)
            onClicked: {
                root.preview = !root.preview
                if (!root.preview)
                    area.forceActiveFocus()
            }
        }
    }
    Shortcut {
        enabled: root.preview && root.visible
        sequence: "Ctrl+E"
        onActivated: { root.preview = false; area.forceActiveFocus() }
    }
}
