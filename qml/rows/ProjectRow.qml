import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import Omniscient

Item {
    id: root
    required property var d
    required property int index
    required property bool isCurrent
    signal select()
    readonly property bool pending: d.pendingUntil !== null && d.pendingUntil !== undefined
    width: ListView.view ? ListView.view.width : 0
    implicitHeight: row.implicitHeight + Theme.sp(20)

    HoverHandler { id: hover }
    RowFrame {
        anchors.fill: parent
        first: root.d.first
        selected: root.isCurrent
        hovered: hover.hovered
        muted: root.d.archived
    }
    TapHandler {
        onTapped: {
            root.select()
            Store.go("project", root.d.id)
        }
    }

    RowLayout {
        id: row
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.verticalCenter: parent.verticalCenter
        anchors.leftMargin: Theme.sp(14)
        anchors.rightMargin: Theme.sp(10)
        spacing: Theme.sp(10)

        Rectangle {
            implicitWidth: Theme.sp(8)
            implicitHeight: Theme.sp(8)
            radius: Theme.smallRadius
            color: root.d.archived ? Theme.muted : Theme.accent
        }
        ColumnLayout {
            Layout.fillWidth: true
            spacing: Theme.sp(2)
            RowLayout {
                spacing: Theme.sp(8)
                Txt {
                    text: root.d.name
                    font.bold: true
                    font.strikeout: root.pending
                    color: root.pending ? Theme.doneText : root.d.archived ? Theme.fgDim : Theme.fg
                }
                Glyph { visible: root.d.hasNotes; text: Icons.note; font.pixelSize: Theme.small }
                Glyph {
                    visible: root.d.link !== ""
                    text: Icons.external
                    font.pixelSize: Theme.small
                    color: lh.hovered ? Theme.accent : Theme.fgDim
                    HoverHandler { id: lh; cursorShape: Qt.PointingHandCursor }
                    TapHandler { onTapped: Store.openLink(root.d.link) }
                }
            }
            Txt {
                visible: root.d.notes !== "" || root.d.archived
                text: root.d.archived ? "Archived " + (root.d.archivedOn || "") : root.d.notes
                color: Theme.fgDim
                font.pixelSize: Theme.small
                Layout.fillWidth: true
            }
        }
        Txt {
            visible: !root.d.archived
            text: root.d.open === 0 ? "No open tasks" : root.d.open === 1 ? "1 open task" : root.d.open + " open tasks"
            color: root.d.open === 0 ? Theme.green : Theme.fgDim
            font.pixelSize: Theme.small
        }
        Ring { visible: root.pending; until: root.pending ? root.d.pendingUntil : 0 }
        RowLayout {
            spacing: Theme.sp(4)
            opacity: hover.hovered || root.isCurrent || root.pending ? 1 : 0
            Btn {
                visible: !root.d.archived
                kind: root.pending ? "normal" : "flat"
                glyph: root.pending ? Icons.undo : Icons.archive
                text: root.pending ? "Undo" : "Archive"
                hint: "a"
                font.pixelSize: Theme.caption
                onClicked: Store.act("archive", { id: root.d.id })
            }
            Btn {
                visible: root.d.archived
                kind: "flat"
                glyph: Icons.undo
                text: "Unarchive"
                font.pixelSize: Theme.caption
                onClicked: Store.act("unarchive", { id: root.d.id })
            }
            Btn {
                visible: root.d.archived
                kind: "danger"
                glyph: Icons.trash
                text: "Delete"
                font.pixelSize: Theme.caption
                onClicked: Store.confirm("Delete “" + root.d.name + "”?", "The project and its completed tasks are removed for good.", "Delete",
                                         () => Store.act("deleteProject", { id: root.d.id }))
            }
        }
    }
}
