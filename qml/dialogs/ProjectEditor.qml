import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import Omniscient

Modal {
    id: root
    property int projectId: -1
    readonly property bool editing: projectId > 0
    property var p: ({})
    property string errors: ""
    property string pristine: ""
    function snapshot() { return JSON.stringify([name.text, link.text, notes.text]) }
    dirty: pristine !== "" && !closing && snapshot() !== pristine

    title: editing ? "Edit project" : "New project"
    glyph: editing ? Icons.edit : Icons.folder
    initialFocus: name
    onAccepted: save()

    Component.onCompleted: {
        if (editing) {
            p = Store.query("project", { id: projectId })
            name.text = p.name || ""
            link.text = p.link || ""
            notes.text = p.notes || ""
        }
        name.selectAll()
        pristine = snapshot()
    }

    function save() {
        if (closing)
            return // Shift+Enter reaches both the field and the dialog
        const r = Store.tryAct("saveProject", { id: editing ? projectId : undefined, name: name.text, link: link.text, notes: notes.text })
        if (r.error !== undefined) {
            errors = r.error
            return
        }
        close()
        if (!editing)
            Store.go("project", r.ok.id)
    }

    Txt {
        visible: root.errors !== ""
        text: root.errors
        color: Theme.red
        wrapMode: Text.Wrap
        elide: Text.ElideNone
        Layout.fillWidth: true
    }
    ColumnLayout {
        Layout.fillWidth: true
        spacing: Theme.sp(6)
        Txt { text: "NAME"; color: Theme.fgDim; font.pixelSize: Theme.caption; font.bold: true; font.letterSpacing: 1 }
        Field { id: name; Layout.fillWidth: true; placeholderText: "Project name"; font.pixelSize: Theme.heading; onAccepted: root.save() }
    }
    ColumnLayout {
        Layout.fillWidth: true
        spacing: Theme.sp(6)
        Txt { text: "LINK"; color: Theme.fgDim; font.pixelSize: Theme.caption; font.bold: true; font.letterSpacing: 1 }
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
        spacing: Theme.sp(6)
        Txt { text: "NOTES"; color: Theme.fgDim; font.pixelSize: Theme.caption; font.bold: true; font.letterSpacing: 1 }
        NotesArea { id: notes; Layout.fillWidth: true; onSubmit: root.save() }
    }

    footer: [
        Btn {
            visible: root.editing && !root.p.archived
            glyph: Icons.archive
            text: "Archive"
            enabled: root.p.open === 0
            onClicked: { Store.go("projects"); Store.act("archive", { id: root.projectId }); root.close() }
            ToolTip.visible: hovered && !enabled
            ToolTip.text: "All tasks should be completed before a project can be archived."
        },
        Btn {
            visible: root.editing
            kind: "danger"
            glyph: Icons.trash
            text: "Delete"
            onClicked: Store.confirm("Delete “" + root.p.name + "”?", "The project and its completed tasks are removed for good.", "Delete",
                                     () => { if (Store.act("deleteProject", { id: root.projectId }) !== null) root.close() })
        },
        Item { Layout.fillWidth: true },
        Btn { text: "Cancel"; kind: "flat"; hint: "Esc"; onClicked: root.requestClose() },
        Btn { text: root.editing ? "Save" : "Create project"; kind: "primary"; hint: "⇧↵"; onClicked: root.save() }
    ]
}
