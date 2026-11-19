import QtQuick
import QtQuick.Layouts
import Omniscient

// Column labels for the projects list, laid out like ProjectRow.
Item {
    id: root
    required property var d
    width: ListView.view ? ListView.view.width : 0
    implicitHeight: row.implicitHeight + Theme.sp(10)

    RowLayout {
        id: row
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.bottom: parent.bottom
        anchors.bottomMargin: Theme.sp(2)
        anchors.leftMargin: Theme.sp(14)
        anchors.rightMargin: Theme.sp(10)
        spacing: Theme.sp(10)
        Item { Layout.preferredWidth: Theme.sp(8) }
        Txt { text: "Project"; color: Theme.fgDim; font.pixelSize: Theme.body; font.bold: true; Layout.fillWidth: true }
        Txt { text: "Open tasks"; color: Theme.fgDim; font.pixelSize: Theme.body; font.bold: true; horizontalAlignment: Text.AlignLeft; Layout.preferredWidth: Theme.projectCountWidth }
        Txt { text: "Actions"; color: Theme.fgDim; font.pixelSize: Theme.body; font.bold: true; horizontalAlignment: Text.AlignLeft; Layout.preferredWidth: Theme.projectActionsWidth }
    }
}
