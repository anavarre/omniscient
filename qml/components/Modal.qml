import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import Omniscient

// In-window dialog: scrim + card in the shell's popup chrome. Escape
// closes; Shift+Enter (or Ctrl+Enter) runs `accepted` from anywhere inside.
FocusScope {
    id: root
    property string title: ""
    property string glyph: ""
    property int cardWidth: Theme.sp(620)
    property bool canAccept: true
    property alias footer: footerRow.data
    property alias headerExtra: headerRow.data
    default property alias content: body.data
    property Item initialFocus: null
    property bool closing: false
    // Set by editors while they hold unsaved input. Escape and clicks on the
    // backdrop then ask before throwing it away; only save() or an explicit
    // "Discard" closes such a dialog.
    property bool dirty: false
    property bool confirming: false
    signal accepted()
    signal closed()

    anchors.fill: parent
    z: 100
    focus: true

    function close() {
        if (closing)
            return
        closing = true
        outro.start()
    }

    // Every user-initiated dismissal (Escape, backdrop, Cancel) goes here.
    function requestClose() {
        if (closing)
            return
        if (!dirty) {
            close()
            return
        }
        if (confirming)
            return
        confirming = true
        Store.confirm("Discard your changes?", "What you typed here hasn't been saved.", "Discard", () => {
            confirming = false
            close()
        })
    }

    Component.onCompleted: {
        intro.start()
        if (initialFocus)
            initialFocus.forceActiveFocus()
        else
            root.forceActiveFocus()
    }

    // Escape bubbles up from fields, so a picker can close its list first.
    Keys.onEscapePressed: event => {
        root.requestClose()
        event.accepted = true
    }
    // Text fields eat Return, so submitting is a shortcut, live only while
    // focus is inside this (top-most) dialog.
    Shortcut {
        sequences: ["Shift+Return", "Shift+Enter", "Ctrl+Return", "Ctrl+Enter"]
        enabled: root.activeFocus && !root.closing
        onActivated: if (root.canAccept) root.accepted()
    }

    Rectangle {
        id: scrim
        anchors.fill: parent
        color: Theme.scrim
        opacity: 0
        // A MouseArea (not a TapHandler) so the press is accepted and never
        // reaches the page underneath.
        MouseArea { anchors.fill: parent; onClicked: root.requestClose() }
    }

    Rectangle {
        id: card
        anchors.horizontalCenter: parent.horizontalCenter
        y: Math.max(Theme.sp(24), (parent.height - height) * 0.32)
        width: Math.min(root.cardWidth, parent.width - Theme.sp(32))
        height: Math.min(column.implicitHeight, parent.height - Theme.sp(48))
        radius: Theme.radius
        color: Theme.popupBg
        border.width: Math.max(1, Math.min(2, Theme.sp(2)))
        border.color: Theme.popupBorder
        opacity: 0
        scale: 0.985
        clip: true
        // Swallow clicks so they reach neither the scrim nor the page below.
        // Handler-only items (chips) don't accept the press themselves, so
        // without this a click on one fell through and dismissed the dialog.
        MouseArea { anchors.fill: parent }

        ColumnLayout {
            id: column
            anchors.fill: parent
            spacing: 0
            RowLayout {
                id: headerRow
                visible: root.title !== ""
                Layout.fillWidth: true
                Layout.margins: Theme.sp(14)
                Layout.leftMargin: Theme.sp(18)
                Layout.rightMargin: Theme.sp(12)
                spacing: Theme.sp(10)
                Glyph { visible: root.glyph !== ""; text: root.glyph; color: Theme.accent; font.pixelSize: Theme.heading }
                Txt { text: root.title; font.pixelSize: Theme.heading; font.bold: true; Layout.fillWidth: true }
            }
            Rectangle { visible: root.title !== ""; Layout.fillWidth: true; height: 1; color: Theme.hairline }
            Flickable {
                id: flick
                Layout.fillWidth: true
                Layout.fillHeight: true
                Layout.preferredHeight: body.implicitHeight + 2 * Theme.sp(18)
                contentHeight: body.implicitHeight + 2 * Theme.sp(18)
                clip: true
                boundsBehavior: Flickable.StopAtBounds
                ScrollBar.vertical: ScrollBar { policy: flick.contentHeight > flick.height ? ScrollBar.AsNeeded : ScrollBar.AlwaysOff }
                ColumnLayout {
                    id: body
                    x: Theme.sp(18)
                    y: Theme.sp(18)
                    width: flick.width - 2 * Theme.sp(18)
                    spacing: Theme.sp(14)
                }
            }
            Rectangle { visible: footerRow.children.length > 0; Layout.fillWidth: true; height: 1; color: Theme.hairline }
            Rectangle {
                visible: footerRow.children.length > 0
                Layout.fillWidth: true
                implicitHeight: footerRow.implicitHeight + 2 * Theme.sp(12)
                color: Theme.normalFill
                RowLayout {
                    id: footerRow
                    anchors.fill: parent
                    anchors.margins: Theme.sp(12)
                    anchors.leftMargin: Theme.sp(18)
                    anchors.rightMargin: Theme.sp(18)
                    spacing: Theme.sp(8)
                }
            }
        }
    }

    ParallelAnimation {
        id: intro
        NumberAnimation { target: scrim; property: "opacity"; to: 1; duration: 140; easing.type: Easing.OutCubic }
        NumberAnimation { target: card; property: "opacity"; to: 1; duration: 160; easing.type: Easing.OutCubic }
        NumberAnimation { target: card; property: "scale"; to: 1; duration: 180; easing.type: Easing.OutBack; easing.overshoot: 0.6 }
    }
    SequentialAnimation {
        id: outro
        ParallelAnimation {
            NumberAnimation { target: scrim; property: "opacity"; to: 0; duration: 110; easing.type: Easing.InCubic }
            NumberAnimation { target: card; property: "opacity"; to: 0; duration: 110; easing.type: Easing.InCubic }
            NumberAnimation { target: card; property: "scale"; to: 0.985; duration: 110 }
        }
        ScriptAction { script: { root.closed(); root.destroy() } }
    }
}
