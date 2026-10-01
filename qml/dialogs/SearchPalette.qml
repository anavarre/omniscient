import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import Omniscient

// Ctrl+K: open tasks whose title or notes match; ↑/↓, Enter opens.
Modal {
    id: root
    cardWidth: Theme.sp(620)
    initialFocus: q
    readonly property var hits: q.text.trim() === "" ? [] : Store.query("searchTasks", { q: q.text })
    property int current: 0
    onHitsChanged: current = 0
    onAccepted: open(current)

    function open(i) {
        const hit = hits[i]
        if (!hit)
            return
        close()
        Store.openTask(hit.id)
    }

    RowLayout {
        Layout.fillWidth: true
        spacing: Theme.sp(10)
        Glyph { text: Icons.search; color: Theme.accent; font.pixelSize: Theme.heading }
        Field {
            id: q
            bare: true
            Layout.fillWidth: true
            placeholderText: "Search open tasks…"
            font.pixelSize: Theme.heading
            onAccepted: root.open(root.current)
            Keys.onDownPressed: root.current = Math.min(root.current + 1, root.hits.length - 1)
            Keys.onUpPressed: root.current = Math.max(root.current - 1, 0)
        }
    }
    Rectangle { Layout.fillWidth: true; height: 1; color: Theme.hairline }
    Txt {
        visible: q.text.trim() !== "" && root.hits.length === 0
        text: "No open task matches “" + q.text.trim() + "”."
        color: Theme.fgDim
    }
    Txt {
        visible: q.text.trim() === ""
        text: "Type to search titles and notes."
        color: Theme.fgDim
    }
    Repeater {
        model: root.hits
        Rectangle {
            id: hit
            required property var modelData
            required property int index
            Layout.fillWidth: true
            implicitHeight: hitRow.implicitHeight + Theme.sp(14)
            radius: Theme.radius
            color: index === root.current ? Theme.menuSelectedFill : hh.hovered ? Theme.hoverFill : "transparent"
            HoverHandler { id: hh; cursorShape: Qt.PointingHandCursor; onHoveredChanged: if (hovered) root.current = hit.index }
            TapHandler { onTapped: root.open(hit.index) }
            RowLayout {
                id: hitRow
                anchors.fill: parent
                anchors.leftMargin: Theme.sp(10)
                anchors.rightMargin: Theme.sp(10)
                spacing: Theme.sp(10)
                Glyph { text: Icons.tasks; font.pixelSize: Theme.small }
                Txt { text: hit.modelData.what; color: hit.index === root.current ? Theme.menuSelectedText : Theme.fg; Layout.fillWidth: true }
                Txt { text: hit.modelData.project; color: Theme.fgDim; font.pixelSize: Theme.small }
                DueBadge { visible: !!hit.modelData.due; text: hit.modelData.due || "" }
            }
        }
    }
    footer: [
        Txt { text: "↑↓ choose · ↵ open · Esc close"; color: Theme.fgDim; font.pixelSize: Theme.caption; Layout.fillWidth: true }
    ]
}
