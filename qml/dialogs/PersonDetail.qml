import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import Omniscient

Modal {
    id: root
    property int personId: -1
    readonly property var p: Store.query("person", { id: personId })
    title: p.name || ""
    glyph: Icons.user
    cardWidth: Theme.sp(560)

    Txt { text: root.p.open === 0 ? "No open tasks." : root.p.open + " open task" + (root.p.open === 1 ? "" : "s"); color: Theme.fgDim }
    Repeater {
        model: root.p.tasks || []
        Rectangle {
            id: hit
            required property var modelData
            Layout.fillWidth: true
            implicitHeight: hitRow.implicitHeight + Theme.sp(12)
            radius: Theme.radius
            color: hh.hovered ? Theme.hoverFill : "transparent"
            HoverHandler { id: hh; cursorShape: Qt.PointingHandCursor }
            TapHandler { onTapped: { root.close(); Store.openTask(hit.modelData.id) } }
            RowLayout {
                id: hitRow
                anchors.fill: parent
                anchors.leftMargin: Theme.sp(8)
                anchors.rightMargin: Theme.sp(8)
                spacing: Theme.sp(10)
                Txt { text: hit.modelData.what; Layout.fillWidth: true }
                Txt { text: hit.modelData.project; color: Theme.fgDim; font.pixelSize: Theme.small }
                DueBadge { visible: !!hit.modelData.due; text: hit.modelData.due || "" }
            }
        }
    }
    footer: [
        Item { Layout.fillWidth: true },
        Btn { text: "Close"; kind: "flat"; hint: "Esc"; onClicked: root.close() }
    ]
}
