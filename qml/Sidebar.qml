import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import Omniscient

// Navigation: home, projects, the active project list, and who you are.
Rectangle {
    id: root
    color: Theme.bgDark
    readonly property var s: Store.s

    Rectangle { anchors.right: parent.right; width: 1; height: parent.height; color: Theme.hairline }

    component NavItem: AbstractButton {
        id: nav
        property string glyph: ""
        property string count: ""
        property bool current: false
        Layout.fillWidth: true
        hoverEnabled: true
        focusPolicy: Qt.NoFocus
        padding: Theme.sp(6)
        leftPadding: Theme.sp(8)
        rightPadding: Theme.sp(8)
        HoverHandler { cursorShape: Qt.PointingHandCursor }
        background: Rectangle {
            radius: Theme.radius
            color: nav.current ? Theme.selectedFill : nav.hovered ? Theme.hoverFill : "transparent"
            Behavior on color { ColorAnimation { duration: Theme.quick } }
        }
        contentItem: RowLayout {
            spacing: Theme.sp(8)
            Glyph {
                text: nav.glyph
                Layout.preferredWidth: Theme.sp(18)
                color: nav.current ? Theme.menuSelectedText : Theme.fgDim
                font.pixelSize: Theme.small
            }
            Txt {
                text: nav.text
                font.bold: nav.current
                color: nav.current ? Theme.menuSelectedText : Theme.fg
                Layout.fillWidth: true
            }
            Txt {
                text: nav.count
                font.pixelSize: Theme.small
                color: nav.current ? Theme.menuSelectedText : Theme.fgDim
            }
        }
    }

    ColumnLayout {
        anchors.fill: parent
        anchors.margins: Theme.sp(10)
        anchors.topMargin: Theme.sp(14)
        spacing: Theme.sp(2)

        RowLayout {
            Layout.leftMargin: Theme.sp(8)
            Layout.bottomMargin: Theme.sp(14)
            spacing: Theme.sp(8)
            Glyph { text: Icons.eye; color: Theme.accent; font.pixelSize: Theme.heading }
            Txt { text: "Omniscient"; font.bold: true; font.pixelSize: Theme.heading }
        }

        NavItem {
            text: root.s.mine && root.s.identity ? "My tasks" : "All tasks"
            glyph: Icons.tasks
            count: root.s.mine && root.s.mineTotal !== null && root.s.mineTotal !== undefined ? String(root.s.mineTotal) : String(root.s.openTotal || 0)
            current: Store.page === "tasks"
            onClicked: Store.go("tasks")
        }
        NavItem {
            text: "Projects"
            glyph: Icons.folder
            count: String((root.s.projects || []).length)
            current: Store.page === "projects"
            onClicked: Store.go("projects")
        }

        Txt {
            visible: (root.s.projects || []).length > 0
            text: "PROJECTS"
            color: Theme.fgDim
            font.pixelSize: Theme.caption
            font.bold: true
            font.letterSpacing: 1
            Layout.leftMargin: Theme.sp(8)
            Layout.topMargin: Theme.sp(14)
            Layout.bottomMargin: Theme.sp(4)
        }
        ListView {
            id: projects
            Layout.fillWidth: true
            Layout.fillHeight: true
            clip: true
            model: root.s.projects || []
            boundsBehavior: Flickable.StopAtBounds
            ScrollBar.vertical: ScrollBar { policy: ScrollBar.AsNeeded }
            delegate: NavItem {
                required property var modelData
                width: ListView.view.width
                text: modelData.name
                glyph: Icons.folderOpen
                count: modelData.open > 0 ? String(modelData.open) : ""
                current: Store.page === "project" && root.s.projectId === modelData.id
                onClicked: Store.go("project", modelData.id)
            }
        }
        Item { visible: (root.s.projects || []).length === 0; Layout.fillHeight: true }

        Rectangle { Layout.fillWidth: true; height: 1; color: Theme.hairline; Layout.bottomMargin: Theme.sp(6) }
        NavItem {
            text: root.s.identity ? root.s.identity.name : "Who are you?"
            glyph: Icons.user
            count: "s"
            onClicked: Store.openSettings(false)
        }
        RowLayout {
            Layout.fillWidth: true
            spacing: Theme.sp(2)
            Btn { kind: "icon"; glyph: Icons.search; onClicked: Store.openSearch(); ToolTip.visible: hovered; ToolTip.text: "Search  Ctrl+K"; ToolTip.delay: 500 }
            Btn { kind: "icon"; glyph: Icons.plus; onClicked: Store.newTask({}); ToolTip.visible: hovered; ToolTip.text: "New task  t"; ToolTip.delay: 500 }
            Item { Layout.fillWidth: true }
            Btn { kind: "icon"; glyph: Icons.keyboard; onClicked: Store.openShortcuts(); ToolTip.visible: hovered; ToolTip.text: "Shortcuts  ?"; ToolTip.delay: 500 }
            Btn { kind: "icon"; glyph: Icons.cog; onClicked: Store.openSettings(false); ToolTip.visible: hovered; ToolTip.text: "Settings  s"; ToolTip.delay: 500 }
        }
    }
}
