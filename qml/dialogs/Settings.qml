import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import Omniscient

// Application settings (collaborators live in their own dialog).
Modal {
    id: root
    property var location: Store.query("location", {})
    property string moveError: ""
    property string moveDone: ""
    title: "Settings"
    glyph: Icons.cog
    initialFocus: newPath

    ColumnLayout {
        Layout.fillWidth: true
        spacing: Theme.sp(6)
        Txt { text: "ZOOM"; color: Theme.fgDim; font.pixelSize: Theme.caption; font.bold: true; font.letterSpacing: 1 }
        RowLayout {
            Layout.fillWidth: true
            spacing: Theme.sp(8)
            Btn { text: "−"; enabled: Theme.zoom > 0.801; onClicked: Store.stepZoom(-0.1) }
            Txt { text: Math.round(Theme.zoom * 100) + "%"; Layout.minimumWidth: Theme.sp(48); horizontalAlignment: Text.AlignHCenter }
            Btn { text: "+"; enabled: Theme.zoom < 1.999; onClicked: Store.stepZoom(0.1) }
            Btn { text: "Reset"; enabled: Math.abs(Theme.zoom - 1) > 0.001; onClicked: Store.setZoom(1) }
            Item { Layout.fillWidth: true }
        }
        Txt { Layout.fillWidth: true; wrapMode: Text.Wrap; color: Theme.fgDim; font.pixelSize: Theme.caption; text: "Also Ctrl + = / Ctrl + − / Ctrl + 0 anywhere. Stacks on the Omarchy text size." }
        Item { Layout.preferredHeight: Theme.sp(10) }
        Txt { text: "DATABASE LOCATION"; color: Theme.fgDim; font.pixelSize: Theme.caption; font.bold: true; font.letterSpacing: 1 }
        Txt {
            Layout.fillWidth: true
            text: root.location.path || ""
            wrapMode: Text.WrapAnywhere
            font.pixelSize: Theme.small
        }
        Txt {
            visible: !root.location.movable
            Layout.fillWidth: true
            wrapMode: Text.Wrap
            color: Theme.fgDim
            font.pixelSize: Theme.caption
            text: root.location.source === "flag" ? "Set by --db for this session; restart without it to move the database."
                : root.location.source === "env" ? "Set by OMNISCIENT_DB for this session; unset it to move the database."
                : "Not available."
        }
        RowLayout {
            visible: root.location.movable === true
            Layout.fillWidth: true
            spacing: Theme.sp(8)
            Field { id: newPath; Layout.fillWidth: true; placeholderText: "New folder or file, e.g. ~/Sync/omniscient/"; onAccepted: root.move() }
            Btn { text: "Move"; enabled: newPath.text.trim() !== ""; onClicked: root.move() }
        }
        Txt { visible: root.moveError !== ""; text: root.moveError; color: Theme.red; font.pixelSize: Theme.small; wrapMode: Text.Wrap; Layout.fillWidth: true }
        Txt { visible: root.moveDone !== ""; text: root.moveDone; color: Theme.green; font.pixelSize: Theme.small; wrapMode: Text.Wrap; Layout.fillWidth: true }
        Txt {
            visible: root.location.movable === true
            Layout.fillWidth: true
            wrapMode: Text.Wrap
            color: Theme.fgDim
            font.pixelSize: Theme.caption
            text: "Copies everything, checks the copy, then switches over. The old file is left in place."
        }
    }

    function move() {
        const r = Store.tryAct("moveDb", { path: newPath.text })
        moveError = r.error !== undefined ? r.error : ""
        moveDone = r.ok !== undefined ? "Moved. The previous copy is still at " + r.ok.old + "; delete it once you're happy." : ""
        if (r.ok !== undefined) {
            newPath.text = ""
            location = Store.query("location", {})
        }
    }

    footer: [
        Txt { text: "Theme: " + (Theme.name || "default") + " · " + Theme.fontFamily + " " + Theme.body + "px"; color: Theme.fgDim; font.pixelSize: Theme.caption; Layout.fillWidth: true },
        Btn { text: "Done"; kind: "primary"; hint: "Esc"; onClicked: root.close() }
    ]
}
