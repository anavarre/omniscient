import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import Omniscient

// New / edit task. Enter in a text field saves; Shift+Enter saves from
// anywhere; Esc closes.
Modal {
    id: root
    property int taskId: -1
    property var defaults: ({})
    readonly property bool editing: taskId > 0
    property var t: ({})
    property string errors: ""

    title: editing ? "Edit task" : t.parentId ? "New sub-task" : "New task"
    glyph: editing ? Icons.edit : Icons.plus
    cardWidth: Theme.sp(680)
    initialFocus: what
    onAccepted: save()

    Component.onCompleted: {
        t = editing ? Store.query("task", { id: taskId }) : Store.query("newTask", defaults)
        what.text = t.what || ""
        when.text = t.dueText || ""
        link.text = t.link || ""
        notes.text = t.notes || ""
        people.selected = (t.people || []).map(p => ({ id: p.id, name: p.name }))
        project.selected = t.projectId ? [{ id: t.projectId, name: t.projectName }] : []
        needs.selected = (t.needs || []).map(r => ({ id: r.id, name: r.what, done: r.done }))
        enables.selected = (t.enables || []).map(r => ({ id: r.id, name: r.what, done: r.done }))
        what.selectAll()
    }

    function save() {
        const r = Store.tryAct("saveTask", {
            id: editing ? taskId : undefined,
            what: what.text,
            projectId: project.selected.length ? project.selected[0].id : null,
            parentId: t.parentId || null,
            due: when.text,
            link: link.text,
            notes: notes.text,
            people: people.ids(),
            needs: needs.ids(),
            enables: enables.ids()
        })
        if (r.error !== undefined) {
            errors = r.error
            return
        }
        close()
    }

    function taskSource(q) {
        if (q.trim() === "")
            return []
        return Store.query("searchTasks", { q: q, exclude: editing ? [taskId] : [] })
            .map(r => ({ id: r.id, name: r.what, detail: r.project, done: r.done }))
    }
    function createTask(name, picker) {
        // Inline Needs/Enables task, in this task's project.
        const pid = project.selected.length ? project.selected[0].id : null
        const r = Store.tryAct("saveTask", { what: name, projectId: pid })
        if (r.error !== undefined) {
            errors = r.error
            return
        }
        picker.add({ id: r.ok.id, name: name })
    }

    component FieldLabel: Txt {
        color: Theme.fgDim
        font.pixelSize: Theme.caption
        font.bold: true
        font.letterSpacing: 1
    }

    Rectangle {
        visible: root.errors !== ""
        Layout.fillWidth: true
        implicitHeight: errorText.implicitHeight + Theme.sp(16)
        radius: Theme.radius
        color: Theme.redSoft
        border.width: 1
        border.color: Theme.redBorder
        Txt {
            id: errorText
            anchors.fill: parent
            anchors.margins: Theme.sp(8)
            anchors.leftMargin: Theme.sp(12)
            text: root.errors
            color: Theme.red
            wrapMode: Text.Wrap
            elide: Text.ElideNone
        }
    }

    Txt {
        visible: !!root.t.parentWhat
        text: Icons.list + "  Sub-task of “" + (root.t.parentWhat || "") + "” · " + (root.t.projectName || "")
        color: Theme.fgDim
        font.pixelSize: Theme.small
    }

    ColumnLayout {
        Layout.fillWidth: true
        spacing: Theme.sp(6)
        FieldLabel { text: "WHAT?" }
        Field {
            id: what
            Layout.fillWidth: true
            placeholderText: "What needs doing?"
            font.pixelSize: Theme.heading
            onAccepted: root.save()
        }
    }

    GridLayout {
        Layout.fillWidth: true
        columns: Math.min(root.cardWidth, root.width - Theme.sp(32)) < Theme.sp(560) ? 1 : 2
        columnSpacing: Theme.sp(14)
        rowSpacing: Theme.sp(14)

        ColumnLayout {
            Layout.fillWidth: true
            Layout.preferredWidth: 1
            spacing: Theme.sp(6)
            FieldLabel { text: "WHO?" }
            Picker {
                id: people
                Layout.fillWidth: true
                placeholder: "Assign people…"
                createNoun: "person"
                source: q => Store.query("people", { q: q }).map(p => ({ id: p.id, name: p.name, detail: p.open + " open" }))
                onCreate: name => {
                    const r = Store.act("createPerson", { name: name })
                    if (r)
                        add({ id: r.id, name: r.name })
                }
            }
        }
        ColumnLayout {
            Layout.fillWidth: true
            Layout.preferredWidth: 1
            spacing: Theme.sp(6)
            FieldLabel { text: "WHEN?" }
            Field {
                id: when
                Layout.fillWidth: true
                placeholderText: "tom, fri, +3, oct 12…"
                readonly property var parsed: Store.query("parseDate", { text: text })
                invalid: !!parsed.invalid
                onAccepted: root.save()
            }
            RowLayout {
                spacing: Theme.sp(4)
                Txt {
                    text: when.parsed.invalid ? "Not a date I understand" : when.parsed.text || "No date"
                    color: when.parsed.invalid ? Theme.red : Theme.fgDim
                    font.pixelSize: Theme.caption
                    Layout.fillWidth: true
                }
                Repeater {
                    model: [["Today", "today"], ["Tom", "tom"], ["Mon", "next week"], ["Clear", ""]]
                    Chip {
                        required property var modelData
                        text: modelData[0]
                        clickable: true
                        onClicked: when.text = modelData[1]
                    }
                }
            }
        }
        ColumnLayout {
            visible: !root.t.parentId
            Layout.fillWidth: true
            Layout.preferredWidth: 1
            spacing: Theme.sp(6)
            FieldLabel { text: "PROJECT" }
            Picker {
                id: project
                Layout.fillWidth: true
                multi: false
                placeholder: "Choose a project…"
                createNoun: "project"
                chipGlyph: Icons.folder
                source: q => Store.query("projects", { q: q }).map(p => ({ id: p.id, name: p.name, detail: p.open + " open" }))
                onCreate: name => {
                    const r = Store.act("createProject", { name: name })
                    if (r)
                        add({ id: r.id, name: r.name })
                }
            }
        }
        ColumnLayout {
            Layout.fillWidth: true
            Layout.preferredWidth: 1
            spacing: Theme.sp(6)
            FieldLabel { text: "LINK" }
            Field {
                id: link
                Layout.fillWidth: true
                placeholderText: "https://…"
                invalid: text.trim() !== "" && !/^https?:\/\/\S+$/.test(text.trim())
                onAccepted: root.save()
            }
        }
        ColumnLayout {
            Layout.fillWidth: true
            Layout.preferredWidth: 1
            spacing: Theme.sp(6)
            FieldLabel { text: "NEEDS" }
            Picker {
                id: needs
                Layout.fillWidth: true
                placeholder: "Tasks this waits on…"
                createNoun: "task"
                chipGlyph: Icons.link
                chipsClickable: true
                exclude: enables.ids()
                source: q => root.taskSource(q)
                onCreate: name => root.createTask(name, needs)
                onChipClicked: item => Store.openTask(item.id)
            }
        }
        ColumnLayout {
            Layout.fillWidth: true
            Layout.preferredWidth: 1
            spacing: Theme.sp(6)
            FieldLabel { text: "ENABLES" }
            Picker {
                id: enables
                Layout.fillWidth: true
                placeholder: "Tasks waiting on this…"
                createNoun: "task"
                chipGlyph: Icons.unlock
                chipsClickable: true
                exclude: needs.ids()
                source: q => root.taskSource(q)
                onCreate: name => root.createTask(name, enables)
                onChipClicked: item => Store.openTask(item.id)
            }
        }
    }

    ColumnLayout {
        Layout.fillWidth: true
        spacing: Theme.sp(6)
        FieldLabel { text: "NOTES" }
        NotesArea { id: notes; Layout.fillWidth: true }
    }

    footer: [
        Btn {
            visible: root.editing
            glyph: root.t.completed ? Icons.undo : Icons.check
            text: root.t.completed ? "Uncomplete" : root.t.pending ? "Undo complete" : "Complete"
            enabled: !root.t.blocked || root.t.completed
            onClicked: {
                if (Store.act("toggle", { id: root.taskId }) !== null)
                    root.close()
            }
            ToolTip.visible: hovered && !!root.t.blocked
            ToolTip.text: root.t.blocked || ""
        },
        Btn {
            visible: root.editing && !root.t.completed
            glyph: root.t.inProgress ? Icons.stop : Icons.play
            text: root.t.inProgress ? "Not in progress" : "In progress"
            onClicked: {
                Store.act("progress", { id: root.taskId })
                root.t = Store.query("task", { id: root.taskId })
            }
        },
        Btn {
            visible: root.editing
            kind: "danger"
            glyph: Icons.trash
            text: "Delete"
            onClicked: Store.confirm("Delete “" + root.t.what + "”?",
                                     root.t.subtasks > 0 ? "Its " + root.t.subtasks + " sub-task(s) are deleted too." : "This can't be undone.",
                                     "Delete", () => { Store.act("deleteTask", { id: root.taskId }); root.close() })
        },
        Item { Layout.fillWidth: true },
        Btn { text: "Cancel"; kind: "flat"; hint: "Esc"; onClicked: root.close() },
        Btn { text: root.editing ? "Save" : "Create task"; kind: "primary"; hint: "⇧↵"; onClicked: root.save() }
    ]
}
