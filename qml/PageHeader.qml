import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import Omniscient

// Title, counts and the view controls above the list.
ColumnLayout {
    id: root
    readonly property var s: Store.s
    readonly property var project: s.project || null
    property bool narrow: false
    signal menu()
    spacing: Theme.sp(10)

    readonly property var groupLabels: ({ date: "Date", project: "Project", person: "Collaborator" })
    readonly property var groupKeys: ({ date: "g d", project: "g p", person: "g c" })

    RowLayout {
        Layout.fillWidth: true
        spacing: Theme.sp(12)
        Btn {
            visible: root.narrow
            kind: "icon"
            glyph: Icons.list
            onClicked: root.menu()
        }
        Btn {
            visible: Store.page === "project"
            kind: "icon"
            glyph: Icons.back
            onClicked: Store.go("projects")
            ToolTip.visible: hovered
            ToolTip.text: "Back to projects  Esc"
            ToolTip.delay: 500
        }
        ColumnLayout {
            Layout.fillWidth: true
            spacing: Theme.sp(2)
            Txt {
                text: root.s.title || ""
                font.pixelSize: Theme.display
                font.bold: true
                Layout.fillWidth: true
            }
            RowLayout {
                spacing: Theme.sp(10)
                Txt { text: root.s.subtitle || ""; color: Theme.fgDim; font.pixelSize: Theme.small }
                Txt {
                    visible: !!root.project && root.project.archived
                    text: "Archived " + (root.project ? root.project.archivedOn || "" : "")
                    color: Theme.fgDim
                    font.pixelSize: Theme.small
                }
                Txt {
                    visible: !!root.project && root.project.link !== ""
                    text: Icons.external + " " + (root.project ? root.project.link : "")
                    color: lh.hovered ? Theme.accent : Theme.fgDim
                    font.pixelSize: Theme.small
                    Layout.maximumWidth: Theme.sp(360)
                    HoverHandler { id: lh; cursorShape: Qt.PointingHandCursor }
                    TapHandler { onTapped: Store.openLink(root.project.link) }
                }
            }
        }
        Btn {
            visible: Store.page === "project"
            text: root.narrow ? "" : "Edit"
            glyph: Icons.edit
            hint: "e"
            onClicked: Store.openProject(root.s.projectId)
        }
        Btn {
            visible: Store.page === "projects"
            text: root.narrow ? "" : "New project"
            glyph: Icons.plus
            hint: "P"
            onClicked: Store.newProject()
        }
        Btn {
            visible: Store.page !== "projects" && !(root.project && root.project.archived)
            kind: "primary"
            text: root.narrow ? "" : "New task"
            glyph: Icons.plus
            hint: "t"
            onClicked: Store.newTask({})
        }
    }

    // Project notes, rendered.
    Rectangle {
        visible: !!root.project && root.project.notes !== ""
        Layout.fillWidth: true
        implicitHeight: notes.implicitHeight + Theme.sp(4)
        color: "transparent"
        Rectangle { width: 2; height: parent.height; color: Theme.hairline }
        Label {
            id: notes
            x: Theme.sp(12)
            width: parent.width - Theme.sp(12)
            text: root.project ? root.project.notes : ""
            textFormat: Text.MarkdownText
            wrapMode: Text.Wrap
            color: Theme.fgDim
            linkColor: Theme.accent
            font.pixelSize: Theme.small
            onLinkActivated: link => Qt.openUrlExternally(link)
        }
    }

    RowLayout {
        visible: Store.groupings.length > 0
        Layout.fillWidth: true
        Layout.topMargin: Theme.sp(4)
        spacing: Theme.sp(10)
        Segmented {
            visible: Store.page === "tasks" && !!root.s.identity
            readonly property var mineAll: [{ value: "mine", label: "Mine", hint: root.narrow ? "" : "g m" }, { value: "all", label: "All", hint: root.narrow ? "" : "g a" }]
            options: mineAll
            current: root.s.mine ? "mine" : "all"
            onPicked: value => Store.act("filter", { mine: value === "mine" })
        }
        Item { Layout.fillWidth: true }
        Txt { visible: !root.narrow; text: "Group by"; color: Theme.fgDim; font.pixelSize: Theme.small }
        Segmented {
            options: Store.groupings.map(g => ({ value: g, label: root.groupLabels[g], hint: root.narrow ? "" : root.groupKeys[g] }))
            current: root.s.grouping || "date"
            onPicked: value => Store.act("group", { by: value })
        }
    }
    Item { implicitHeight: Theme.sp(4) }
}
