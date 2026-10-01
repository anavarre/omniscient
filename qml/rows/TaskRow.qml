import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import Omniscient

// One task (or sub-task): checkbox, title and meta | who | when, like the
// web app's What / Who / When grid.
Item {
    id: root
    required property var d
    required property int index
    required property bool isCurrent
    property bool narrow: false
    property bool compact: false
    signal select()
    signal dragged(real sceneY, bool done)

    readonly property bool sub: d.depth > 0
    readonly property bool pending: d.pendingUntil !== null && d.pendingUntil !== undefined
    width: ListView.view ? ListView.view.width : 0
    implicitHeight: frame.implicitHeight

    HoverHandler { id: hover }

    RowFrame {
        id: frame
        anchors.fill: parent
        implicitHeight: content.implicitHeight + Theme.sp(16)
        first: root.d.first
        selected: root.isCurrent
        hovered: hover.hovered
        muted: root.d.completed
        base: root.sub ? Qt.tint(Theme.card, Theme.subtaskFill) : Theme.card
        Rectangle { visible: root.sub; anchors.top: parent.top; anchors.left: parent.left; anchors.leftMargin: Theme.sp(40); anchors.right: parent.right; height: 1; color: Theme.hairline }
    }

    TapHandler {
        onTapped: {
            root.select()
            Store.openTask(root.d.id)
        }
    }

    RowLayout {
        id: content
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.verticalCenter: parent.verticalCenter
        anchors.leftMargin: root.sub ? Theme.sp(40) : Theme.sp(12)
        anchors.rightMargin: Theme.sp(12)
        spacing: Theme.sp(10)

        // Sub-task drag handle / parent disclosure.
        Glyph {
            visible: root.sub
            text: Icons.drag
            opacity: hover.hovered || root.isCurrent ? 1 : 0
            Layout.alignment: Qt.AlignTop
            Layout.topMargin: Theme.sp(1)
            HoverHandler { cursorShape: Qt.OpenHandCursor }
            DragHandler {
                id: drag
                target: null
                yAxis.enabled: true
                xAxis.enabled: false
                onCentroidChanged: if (active) root.dragged(centroid.scenePosition.y, false)
                onActiveChanged: root.dragged(centroid.scenePosition.y, !active)
            }
        }
        Glyph {
            visible: !root.sub
            opacity: root.d.subtasks > 0 ? 1 : 0
            text: root.d.expanded ? Icons.chevronDown : Icons.chevronRight
            font.pixelSize: Theme.caption
            Layout.preferredWidth: Theme.sp(14)
            Layout.alignment: Qt.AlignTop
            Layout.topMargin: Theme.sp(2)
            color: chevHover.hovered ? Theme.fg : Theme.fgDim
            HoverHandler { id: chevHover; cursorShape: root.d.subtasks > 0 ? Qt.PointingHandCursor : Qt.ArrowCursor }
            TapHandler { enabled: root.d.subtasks > 0; onTapped: Store.act("expand", { id: root.d.id }) }
        }
        Check {
            checked: root.d.done
            pendingUntil: root.pending ? root.d.pendingUntil : 0
            blocked: !!root.d.blocked
            Layout.alignment: Qt.AlignTop
            Layout.topMargin: Theme.sp(1)
            onToggled: Store.act("toggle", { id: root.d.id })
        }

        ColumnLayout {
            Layout.fillWidth: true
            spacing: Theme.sp(3)

            RowLayout {
                Layout.fillWidth: true
                spacing: Theme.sp(8)
                Txt {
                    text: root.d.what
                    font.weight: Font.Medium
                    font.strikeout: root.d.done
                    font.italic: root.d.done
                    color: root.d.done ? Theme.doneText : Theme.fg
                    Layout.fillWidth: implicitWidth > width
                    Layout.maximumWidth: implicitWidth
                    Behavior on color { ColorAnimation { duration: Theme.quick } }
                }
                Chip {
                    visible: root.d.inProgress
                    glyph: Icons.hourglass
                    text: "In progress"
                    ink: Theme.yellow
                    border.color: Theme.yellowBorder
                }
                Glyph {
                    visible: root.d.link !== ""
                    text: Icons.external
                    font.pixelSize: Theme.small
                    color: linkHover.hovered ? Theme.accent : Theme.fgDim
                    HoverHandler { id: linkHover; cursorShape: Qt.PointingHandCursor }
                    TapHandler { onTapped: Store.openLink(root.d.link) }
                    ToolTip.visible: linkHover.hovered
                    ToolTip.text: root.d.link
                    ToolTip.delay: 400
                }
                Glyph { visible: root.d.hasNotes && root.d.notes === ""; text: Icons.note; font.pixelSize: Theme.small }
                Chip {
                    visible: !!root.d.project
                    text: root.d.project || ""
                    glyph: Icons.folder
                    ink: Theme.fgDim
                    clickable: true
                    onClicked: Store.go("project", root.d.projectId)
                }
                Item { Layout.fillWidth: true }
                Btn {
                    visible: !root.sub && !root.d.completed && (hover.hovered || root.isCurrent)
                    kind: "flat"
                    glyph: Icons.plus
                    text: "Sub-task"
                    font.pixelSize: Theme.caption
                    topPadding: Theme.sp(1)
                    bottomPadding: Theme.sp(1)
                    leftPadding: Theme.sp(6)
                    rightPadding: Theme.sp(6)
                    onClicked: Store.newTask({ parentId: root.d.id })
                }
            }
            Txt {
                visible: root.d.notes !== ""
                text: root.d.notes
                color: Theme.fgDim
                font.pixelSize: Theme.small
                Layout.fillWidth: true
            }
            Flow {
                visible: root.d.needs.length + root.d.enables.length > 0
                Layout.fillWidth: true
                spacing: Theme.sp(4)
                Txt { visible: root.d.needs.length > 0; text: "Needs"; color: Theme.fgDim; font.pixelSize: Theme.caption; height: Theme.small + Theme.sp(4) }
                Repeater {
                    model: root.d.needs
                    Chip {
                        required property var modelData
                        glyph: Icons.link
                        text: modelData.what
                        done: modelData.done
                        clickable: true
                        onClicked: Store.openTask(modelData.id)
                    }
                }
                Txt { visible: root.d.enables.length > 0; text: root.d.needs.length > 0 ? "· Enables" : "Enables"; color: Theme.fgDim; font.pixelSize: Theme.caption; height: Theme.small + Theme.sp(4) }
                Repeater {
                    model: root.d.enables
                    Chip {
                        required property var modelData
                        glyph: Icons.unlock
                        text: modelData.what
                        done: modelData.done
                        clickable: true
                        onClicked: Store.openTask(modelData.id)
                    }
                }
            }
            Txt {
                visible: !!root.d.conflict
                text: Icons.warning + "  " + (root.d.conflict || "")
                color: Theme.red
                font.pixelSize: Theme.small
                Layout.fillWidth: true
            }
            // Narrow windows stack Who and When under the title.
            Loader {
                active: root.narrow
                visible: active
                sourceComponent: RowLayout {
                    spacing: Theme.sp(8)
                    Loader { sourceComponent: whoComponent }
                    Loader { sourceComponent: whenComponent }
                }
            }
        }

        Loader {
            active: !root.narrow
            visible: active
            sourceComponent: whoComponent
            Layout.preferredWidth: root.compact ? Theme.sp(120) : Theme.sp(200)
            Layout.alignment: Qt.AlignTop
        }
        Loader {
            active: !root.narrow
            visible: active
            sourceComponent: whenComponent
            Layout.preferredWidth: Theme.sp(96)
            Layout.alignment: Qt.AlignTop | Qt.AlignRight
        }
    }

    Component {
        id: whoComponent
        RowLayout {
            spacing: Theme.sp(6)
            Repeater {
                model: root.d.people.slice(0, root.compact || root.narrow ? 3 : 2)
                RowLayout {
                    required property var modelData
                    spacing: Theme.sp(4)
                    Avatar { initials: modelData.initials; dim: root.d.done }
                    Txt {
                        visible: !root.compact && !root.narrow && root.d.people.length <= 2
                        text: modelData.name.split(" ")[0]
                        font.pixelSize: Theme.small
                        color: root.d.done ? Theme.doneText : Theme.fg
                    }
                }
            }
            Txt {
                visible: root.d.people.length > (root.compact || root.narrow ? 3 : 2)
                text: "+" + (root.d.people.length - (root.compact || root.narrow ? 3 : 2))
                color: Theme.fgDim
                font.pixelSize: Theme.small
            }
            Txt { visible: root.d.people.length === 0 && !root.narrow; text: "Unassigned"; color: Theme.placeholder; font.pixelSize: Theme.small }
        }
    }
    Component {
        id: whenComponent
        Item {
            implicitWidth: badge.visible ? badge.implicitWidth : none.implicitWidth
            implicitHeight: badge.implicitHeight
            DueBadge {
                id: badge
                anchors.right: parent.right
                visible: !!root.d.due
                text: root.d.due ? root.d.due.text : ""
                tone: root.d.due ? root.d.due.tone : ""
            }
            Txt { id: none; anchors.right: parent.right; visible: !root.d.due && !root.narrow; text: "—"; color: Theme.placeholder; font.pixelSize: Theme.small }
        }
    }
}
