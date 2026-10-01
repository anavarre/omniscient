import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import Omniscient

Rectangle {
    id: root
    property string text: ""
    property bool error: false
    radius: Theme.radius
    color: Theme.popupBg
    border.width: 1
    border.color: error ? Theme.red : Theme.popupBorder
    implicitWidth: Math.min(row.implicitWidth + Theme.sp(28), Theme.sp(560))
    implicitHeight: row.implicitHeight + Theme.sp(16)
    opacity: 0
    y: Theme.sp(8)

    RowLayout {
        id: row
        anchors.centerIn: parent
        width: parent.width - Theme.sp(28)
        spacing: Theme.sp(8)
        Glyph { text: root.error ? Icons.warning : Icons.check; color: root.error ? Theme.red : Theme.accent }
        Txt { text: root.text; color: root.error ? Theme.red : Theme.fg; wrapMode: Text.Wrap; elide: Text.ElideNone; Layout.fillWidth: true }
    }

    function show(text, error) {
        root.text = text
        root.error = error
        life.restart()
    }

    SequentialAnimation {
        id: life
        ParallelAnimation {
            NumberAnimation { target: root; property: "opacity"; to: 1; duration: 160; easing.type: Easing.OutCubic }
            NumberAnimation { target: root; property: "y"; to: 0; duration: 160; easing.type: Easing.OutCubic }
        }
        PauseAnimation { duration: root.error ? 5000 : 2600 }
        ParallelAnimation {
            NumberAnimation { target: root; property: "opacity"; to: 0; duration: 220 }
            NumberAnimation { target: root; property: "y"; to: Theme.sp(8); duration: 220 }
        }
    }
}
