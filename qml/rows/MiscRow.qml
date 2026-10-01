import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import Omniscient

// "Show N more", "Completed (N)", "Archived (N)" and empty states.
Item {
    id: root
    required property var d
    required property bool isCurrent
    signal select()
    readonly property bool toggle: d.kind === "completedToggle" || d.kind === "archivedToggle"
    width: ListView.view ? ListView.view.width : 0
    implicitHeight: d.kind === "empty" ? empty.implicitHeight + Theme.sp(80)
                  : toggle ? label.implicitHeight + Theme.sp(30) : label.implicitHeight + Theme.sp(14)

    function activate() {
        if (d.kind === "more")
            Store.act("reveal", { section: d.section })
        else if (d.kind === "completedToggle")
            Store.act("toggleCompleted")
        else if (d.kind === "archivedToggle")
            Store.act("toggleArchived")
    }

    HoverHandler { id: hover; enabled: root.d.kind !== "empty"; cursorShape: Qt.PointingHandCursor }
    TapHandler { enabled: root.d.kind !== "empty"; onTapped: { root.select(); root.activate() } }

    RowFrame {
        visible: root.d.kind === "more"
        anchors.fill: parent
        selected: root.isCurrent
        hovered: hover.hovered
    }

    RowLayout {
        visible: root.d.kind !== "empty"
        anchors.left: parent.left
        anchors.leftMargin: root.toggle ? Theme.sp(4) : Theme.sp(12)
        anchors.bottom: parent.bottom
        anchors.bottomMargin: root.toggle ? Theme.sp(6) : Theme.sp(7)
        spacing: Theme.sp(8)
        Glyph {
            visible: root.toggle
            text: root.d.open ? Icons.chevronDown : Icons.chevronRight
            font.pixelSize: Theme.caption
            color: label.color
        }
        Txt {
            id: label
            text: root.d.kind === "more" ? "Show " + root.d.count + " more"
                : root.d.kind === "completedToggle" ? "Completed (" + root.d.count + ")"
                : root.d.kind === "archivedToggle" ? "Archived (" + root.d.count + ")" : ""
            font.bold: root.toggle
            font.pixelSize: root.toggle ? Theme.body : Theme.small
            color: hover.hovered || root.isCurrent ? Theme.fg : Theme.fgDim
        }
    }

    ColumnLayout {
        id: empty
        visible: root.d.kind === "empty"
        anchors.centerIn: parent
        spacing: Theme.sp(8)
        Glyph { text: Icons.check; color: Theme.accent; font.pixelSize: Theme.display; Layout.alignment: Qt.AlignHCenter }
        Txt { text: root.d.title || ""; font.pixelSize: Theme.heading; font.bold: true; Layout.alignment: Qt.AlignHCenter }
        Txt { text: root.d.text || ""; color: Theme.fgDim; Layout.alignment: Qt.AlignHCenter }
    }
}
