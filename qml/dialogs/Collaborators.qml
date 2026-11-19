import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import Omniscient

// Who you are (drives "Mine") and the collaborator list.
Modal {
    id: root
    property bool addPerson: false
    property var people: Store.query("people", {})
    property string errors: ""
    property int editingId: -1
    title: "Collaborators"
    glyph: Icons.users
    initialFocus: addPerson ? newPerson : identity
    function reload() {
        people = Store.query("people", {})
        identity.selected = Store.s.identity ? [{ id: Store.s.identity.id, name: Store.s.identity.name }] : []
    }
    Component.onCompleted: reload()

    function add() {
        if (newPerson.text.trim() === "")
            return
        const r = Store.tryAct("createPerson", { name: newPerson.text })
        errors = r.error !== undefined ? r.error : r.ok.existed ? "“" + r.ok.name + "” is already a collaborator." : ""
        newPerson.text = ""
        reload()
    }

    ColumnLayout {
        Layout.fillWidth: true
        spacing: Theme.sp(6)
        Txt { text: "WHO ARE YOU?"; color: Theme.fgDim; font.pixelSize: Theme.caption; font.bold: true; font.letterSpacing: 1 }
        Picker {
            id: identity
            Layout.fillWidth: true
            multi: false
            placeholder: "Pick yourself to see your own tasks…"
            createNoun: "person"
            chipGlyph: Icons.user
            source: q => Store.query("people", { q: q }).map(p => ({ id: p.id, name: p.name }))
            onCreate: name => {
                const r = Store.act("createPerson", { name: name })
                if (r)
                    add({ id: r.id, name: r.name })
            }
            onChanged: {
                Store.act("identity", { id: selected.length ? selected[0].id : null })
                root.reload()
            }
        }
        Txt { text: "Your tasks show under Mine (g m); new tasks are assigned to you."; color: Theme.fgDim; font.pixelSize: Theme.caption }
    }

    ColumnLayout {
        Layout.fillWidth: true
        spacing: Theme.sp(4)
        Txt { text: "COLLABORATORS"; color: Theme.fgDim; font.pixelSize: Theme.caption; font.bold: true; font.letterSpacing: 1 }
        Txt { visible: root.errors !== ""; text: root.errors; color: Theme.red; font.pixelSize: Theme.small; wrapMode: Text.Wrap; Layout.fillWidth: true }
        Repeater {
            model: root.people
            Rectangle {
                id: person
                required property var modelData
                Layout.fillWidth: true
                implicitHeight: personRow.implicitHeight + Theme.sp(10)
                radius: Theme.radius
                color: ph.hovered ? Theme.hoverFill : "transparent"
                HoverHandler { id: ph }
                RowLayout {
                    id: personRow
                    anchors.fill: parent
                    anchors.leftMargin: Theme.sp(8)
                    anchors.rightMargin: Theme.sp(4)
                    spacing: Theme.sp(10)
                    Avatar { initials: person.modelData.initials }
                    Txt {
                        visible: root.editingId !== person.modelData.id
                        text: person.modelData.name + (person.modelData.me ? "  (you)" : "")
                        font.bold: person.modelData.me
                        color: person.modelData.me ? Theme.menuSelectedText : Theme.fg
                        Layout.fillWidth: true
                        HoverHandler { cursorShape: Qt.PointingHandCursor }
                        TapHandler { onTapped: Store.openPerson(person.modelData.id) }
                    }
                    Field {
                        id: rename
                        visible: root.editingId === person.modelData.id
                        Layout.fillWidth: true
                        text: person.modelData.name
                        onVisibleChanged: if (visible) { forceActiveFocus(); selectAll() }
                        onAccepted: {
                            const r = Store.tryAct("renamePerson", { id: person.modelData.id, name: text })
                            root.errors = r.error !== undefined ? r.error : ""
                            root.editingId = -1
                            root.reload()
                        }
                        Keys.onEscapePressed: event => { root.editingId = -1; event.accepted = true }
                    }
                    Txt {
                        text: person.modelData.open === 0 ? "no open tasks" : person.modelData.open + " open"
                        color: person.modelData.open === 0 ? Theme.green : Theme.fgDim
                        font.pixelSize: Theme.small
                    }
                    Btn {
                        kind: "icon"
                        glyph: Icons.edit
                        opacity: ph.hovered ? 1 : 0.0
                        onClicked: root.editingId = person.modelData.id
                    }
                    Btn {
                        kind: "icon"
                        glyph: Icons.trash
                        tint: Theme.red
                        opacity: ph.hovered ? 1 : 0.0
                        onClicked: {
                            const r = Store.tryAct("deletePerson", { id: person.modelData.id })
                            root.errors = r.error !== undefined ? person.modelData.name + ": " + r.error : ""
                            root.reload()
                        }
                    }
                }
            }
        }
        RowLayout {
            Layout.fillWidth: true
            Layout.topMargin: Theme.sp(6)
            spacing: Theme.sp(8)
            Field { id: newPerson; Layout.fillWidth: true; placeholderText: "Add a collaborator…"; onAccepted: root.add() }
            Btn { glyph: Icons.plus; text: "Add"; onClicked: root.add() }
        }
    }

    footer: [
        Item { Layout.fillWidth: true },
        Btn { text: "Done"; kind: "primary"; hint: "Esc"; onClicked: root.close() }
    ]
}
