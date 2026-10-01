import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import Omniscient

// Generated from Main.qml's key map, so it can't drift from the keys.
Modal {
    id: root
    property var keymap: []
    title: "Keyboard shortcuts"
    glyph: Icons.keyboard
    cardWidth: Theme.sp(760)
    readonly property var groups: {
        const out = []
        for (const b of keymap) {
            let g = out.find(x => x.name === b.group)
            if (!g) {
                g = { name: b.group, items: [] }
                out.push(g)
            }
            g.items.push(b)
        }
        return out
    }

    GridLayout {
        Layout.fillWidth: true
        columns: root.width < Theme.sp(700) ? 1 : 2
        columnSpacing: Theme.sp(28)
        rowSpacing: Theme.sp(14)
        Repeater {
            model: root.groups
            ColumnLayout {
                required property var modelData
                Layout.fillWidth: true
                Layout.alignment: Qt.AlignTop
                spacing: Theme.sp(6)
                Txt { text: modelData.name.toUpperCase(); color: Theme.accent; font.bold: true; font.pixelSize: Theme.small; font.letterSpacing: 1 }
                Repeater {
                    model: modelData.items
                    RowLayout {
                        required property var modelData
                        Layout.fillWidth: true
                        spacing: Theme.sp(6)
                        Txt { text: modelData.label; Layout.fillWidth: true; font.pixelSize: Theme.small }
                        Repeater {
                            model: modelData.keys
                            Kbd { required property var modelData; text: modelData }
                        }
                    }
                }
            }
        }
    }
}
