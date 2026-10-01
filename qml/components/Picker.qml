import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import Omniscient

// Keyboard-first chip picker: type to filter, ↑/↓ to choose, Enter to
// pick (or create what you typed), Backspace on empty removes the last
// chip. `source(query)` returns [{id, name, detail?, done?}].
FocusScope {
    id: root
    property var selected: []
    property bool multi: true
    property string placeholder: ""
    property var source: function (q) { return [] }
    property string createNoun: ""        // "person" → offers Add person "x"
    property var exclude: []              // ids never offered
    property string chipGlyph: ""
    property bool chipsClickable: false
    signal create(string name)
    signal chipClicked(var item)
    signal changed()

    implicitHeight: box.implicitHeight
    activeFocusOnTab: true

    function add(item) {
        if (selected.some(s => s.id === item.id)) {
            field.text = ""
            return
        }
        selected = multi ? selected.concat([item]) : [item]
        browsing = false
        field.text = ""
        list.close()
        changed()
    }
    function remove(id) {
        selected = selected.filter(s => s.id !== id)
        changed()
    }
    function ids() {
        return selected.map(s => s.id)
    }

    // Suggestions open when you type, or on ↓ / Enter in an empty field.
    property bool browsing: false
    readonly property var results: {
        if (!field.activeFocus || (field.text === "" && !browsing))
            return []
        const taken = ids().concat(exclude)
        const hits = source(field.text).filter(r => taken.indexOf(r.id) < 0)
        const q = field.text.trim()
        const exact = q !== "" && hits.some(r => r.name.toLowerCase() === q.toLowerCase())
        if (createNoun !== "" && q !== "" && !exact)
            hits.push({ id: -1, name: q, create: true })
        return hits.slice(0, 8)
    }
    onResultsChanged: {
        suggestions.currentIndex = 0
        if (results.length > 0 && field.activeFocus) list.open(); else list.close()
    }

    Rectangle {
        id: box
        width: parent.width
        implicitHeight: flow.implicitHeight + Theme.sp(6)
        radius: Theme.radius
        color: field.activeFocus ? Theme.focusFill : Theme.normalFill
        border.width: Theme.borderWidth
        border.color: field.activeFocus ? Theme.focusBorderStrong : hover.hovered ? Theme.hoverBorderStrong : Theme.normalBorder
        Behavior on border.color { ColorAnimation { duration: Theme.quick } }
        HoverHandler { id: hover; cursorShape: Qt.IBeamCursor }
        TapHandler { onTapped: { field.forceActiveFocus(); root.browsing = true } }

        Flow {
            id: flow
            anchors.left: parent.left
            anchors.right: parent.right
            anchors.verticalCenter: parent.verticalCenter
            anchors.margins: Theme.sp(4)
            spacing: Theme.sp(4)
            Repeater {
                id: chips
                model: root.selected
                Chip {
                    required property var modelData
                    text: modelData.name
                    glyph: root.chipGlyph
                    done: !!modelData.done
                    removable: true
                    clickable: root.chipsClickable
                    height: field.implicitHeight - Theme.sp(4)
                    y: Theme.sp(2)
                    onRemoved: root.remove(modelData.id)
                    onClicked: root.chipClicked(modelData)
                }
            }
            Field {
                id: field
                bare: true
                focus: true
                // Takes what's left of the chips' last line (or a new line).
                readonly property Item lastChip: chips.count > 0 ? chips.itemAt(chips.count - 1) : null
                readonly property real rest: lastChip ? flow.width - (lastChip.x + lastChip.width + flow.spacing) : flow.width
                width: rest >= Theme.sp(40) ? rest : flow.width
                onActiveFocusChanged: if (!activeFocus) root.browsing = false
                placeholderText: root.selected.length === 0 ? root.placeholder : ""
                topPadding: Theme.sp(4)
                bottomPadding: Theme.sp(4)
                Keys.onPressed: event => {
                    const n = root.results.length
                    if ((event.key === Qt.Key_Down || event.key === Qt.Key_Return || event.key === Qt.Key_Enter)
                            && !(event.modifiers & (Qt.ShiftModifier | Qt.ControlModifier)) && !list.opened && field.text === "") {
                        root.browsing = true
                        event.accepted = true
                    } else if (event.key === Qt.Key_Down && n > 0) {
                        suggestions.currentIndex = (suggestions.currentIndex + 1) % n
                        event.accepted = true
                    } else if (event.key === Qt.Key_Up && n > 0) {
                        suggestions.currentIndex = (suggestions.currentIndex + n - 1) % n
                        event.accepted = true
                    } else if ((event.key === Qt.Key_Return || event.key === Qt.Key_Enter)
                               && !(event.modifiers & (Qt.ShiftModifier | Qt.ControlModifier)) && n > 0 && list.opened) {
                        root.choose(root.results[suggestions.currentIndex])
                        event.accepted = true
                    } else if (event.key === Qt.Key_Backspace && field.text === "" && root.selected.length > 0) {
                        root.remove(root.selected[root.selected.length - 1].id)
                        event.accepted = true
                    } else if (event.key === Qt.Key_Escape && list.opened) {
                        list.close()
                        root.browsing = false
                        field.text = ""
                        event.accepted = true
                    }
                }
            }
        }
    }

    function choose(item) {
        if (!item)
            return
        if (item.create)
            root.create(item.name)
        else
            root.add(item)
    }

    Popup {
        id: list
        y: box.height + Theme.sp(4)
        width: box.width
        padding: Theme.sp(4)
        closePolicy: Popup.CloseOnPressOutsideParent
        focus: false
        background: Rectangle {
            radius: Theme.radius
            color: Theme.popupBg
            border.width: Math.max(1, Math.min(2, Theme.sp(2)))
            border.color: Theme.popupBorder
        }
        enter: Transition { NumberAnimation { property: "opacity"; from: 0; to: 1; duration: 100 } }
        exit: Transition { NumberAnimation { property: "opacity"; to: 0; duration: 80 } }
        contentItem: ListView {
            id: suggestions
            implicitHeight: contentHeight
            model: root.results
            interactive: false
            delegate: Rectangle {
                id: hit
                required property var modelData
                required property int index
                width: ListView.view.width
                height: hitRow.implicitHeight + Theme.sp(12)
                radius: Theme.radius
                color: ListView.isCurrentItem ? Theme.menuSelectedFill : hitHover.hovered ? Theme.hoverFill : "transparent"
                HoverHandler { id: hitHover; cursorShape: Qt.PointingHandCursor; onHoveredChanged: if (hovered) suggestions.currentIndex = hit.index }
                TapHandler { onTapped: root.choose(hit.modelData) }
                RowLayout {
                    id: hitRow
                    anchors.fill: parent
                    anchors.leftMargin: Theme.sp(10)
                    anchors.rightMargin: Theme.sp(10)
                    spacing: Theme.sp(8)
                    Glyph { visible: !!hit.modelData.create; text: Icons.plus; color: Theme.accent; font.pixelSize: Theme.small }
                    Txt {
                        text: hit.modelData.create ? "Add " + root.createNoun + " “" + hit.modelData.name + "”" : hit.modelData.name
                        color: hit.ListView.isCurrentItem ? Theme.menuSelectedText : Theme.fg
                        font.pixelSize: Theme.small
                        Layout.fillWidth: true
                    }
                    Txt {
                        visible: !!hit.modelData.detail
                        text: hit.modelData.detail || ""
                        color: hit.ListView.isCurrentItem ? Theme.menuSelectedText : Theme.fgDim
                        font.pixelSize: Theme.caption
                    }
                }
            }
        }
    }
}
