import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import Omniscient

Item {
    id: root
    required property var d
    width: ListView.view ? ListView.view.width : 0
    implicitHeight: row.implicitHeight + (d.first ? Theme.sp(6) : Theme.sp(22)) + Theme.sp(8)

    RowLayout {
        id: row
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.bottom: parent.bottom
        anchors.bottomMargin: Theme.sp(8)
        spacing: Theme.sp(10)
        Txt {
            text: root.d.title
            font.pixelSize: Theme.heading
            font.bold: true
            color: root.d.tone === "overdue" ? Theme.red : linkHover.hovered ? Theme.accent : Theme.fg
            HoverHandler { id: linkHover; enabled: root.d.projectId !== null && root.d.projectId !== undefined; cursorShape: Qt.PointingHandCursor }
            TapHandler { enabled: linkHover.enabled; onTapped: Store.go("project", root.d.projectId) }
        }
        Txt { text: root.d.count; color: Theme.fgDim; font.pixelSize: Theme.small }
        Item { Layout.fillWidth: true }
    }
}
