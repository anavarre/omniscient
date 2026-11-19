import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import Omniscient

Item {
    id: root
    required property var d
    width: ListView.view ? ListView.view.width : 0
    property bool narrow: false
    property bool compact: false
    // Header size follows urgency: Overdue > Today > Tomorrow > Later; other groupings (projects, people) sit mid-scale.
    readonly property real headerScale: ({ "Overdue": 1.8, "Today": 1.6, "Tomorrow": 1.35, "Later": 1.2 })[d.title] || 1.35
    implicitHeight: row.implicitHeight + (d.first ? Theme.sp(6) : Theme.sp(22)) + Theme.sp(8)
                    + (labels.visible ? labels.implicitHeight + Theme.sp(4) : 0)

    RowLayout {
        id: row
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.bottom: parent.bottom
        anchors.leftMargin: Theme.sp(12)   // flush with the task rows' content
        anchors.rightMargin: Theme.sp(12)
        anchors.bottomMargin: Theme.sp(8) + (labels.visible ? labels.implicitHeight + Theme.sp(4) : 0)
        spacing: Theme.sp(10)
        Txt {
            text: root.d.title
            font.pixelSize: Math.round(Theme.heading * root.headerScale)
            font.bold: true
            color: root.d.tone === "overdue" ? Theme.red : linkHover.hovered ? Theme.accent : Theme.fg
            HoverHandler { id: linkHover; enabled: root.d.projectId !== null && root.d.projectId !== undefined; cursorShape: Qt.PointingHandCursor }
            TapHandler { enabled: linkHover.enabled; onTapped: Store.go("project", root.d.projectId) }
        }
        Txt { text: root.d.count; color: Theme.fgDim; font.pixelSize: Theme.small }
        Item { Layout.fillWidth: true }
    }

    // Column labels, laid out like TaskRow's columns (stacked layouts skip them).
    RowLayout {
        id: labels
        visible: !root.narrow
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.bottom: parent.bottom
        anchors.leftMargin: Theme.sp(12)
        anchors.rightMargin: Theme.sp(12)
        anchors.bottomMargin: Theme.sp(2)
        spacing: Theme.sp(10)
        // Chevron + checkbox, then the title column.
        Item { Layout.preferredWidth: Theme.sp(14) + Theme.sp(10) + Theme.sp(16) }
        Txt { text: "Item"; color: Theme.fgDim; font.pixelSize: Theme.body; font.bold: true; Layout.fillWidth: true }
        Txt {
            text: root.d.plural ? "Collaborators" : "Collaborator"
            color: Theme.fgDim; font.pixelSize: Theme.body; font.bold: true; horizontalAlignment: Text.AlignLeft
            Layout.preferredWidth: root.compact ? Theme.sp(120) : Theme.sp(200)
        }
        Txt { text: "Date"; color: Theme.fgDim; font.pixelSize: Theme.body; font.bold: true; horizontalAlignment: Text.AlignLeft; Layout.preferredWidth: Theme.sp(96) }
    }
}
