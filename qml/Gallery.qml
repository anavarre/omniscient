import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import Omniscient

// `omniscient --gallery`: every component on one page, for checking themes
// by eye (switch with `omarchy-theme-set` while it's open).
Rectangle {
    color: Theme.bg
    Flickable {
        anchors.fill: parent
        anchors.margins: Theme.sp(30)
        contentHeight: col.implicitHeight
        ColumnLayout {
            id: col
            width: parent.width
            spacing: Theme.sp(18)
            Txt { text: "Theme: " + Theme.name + (Theme.dark ? " (dark)" : " (light)") + " · " + Theme.fontFamily + " · radius " + Theme.radius; font.pixelSize: Theme.display; font.bold: true }
            Flow {
                Layout.fillWidth: true
                spacing: Theme.sp(6)
                Repeater {
                    model: ["bg", "bgDark", "card", "fg", "fgDim", "muted", "accent", "red", "orange", "yellow", "green", "hairline", "selection", "popupBorder"]
                    Rectangle {
                        required property string modelData
                        width: Theme.sp(96); height: Theme.sp(52)
                        color: Theme[modelData]
                        border.width: 1; border.color: Theme.hairline
                        Txt { anchors.bottom: parent.bottom; anchors.margins: 4; x: 4; text: parent.modelData; font.pixelSize: Theme.caption; color: Theme.fg; style: Text.Outline; styleColor: Theme.bg }
                    }
                }
            }
            RowLayout {
                spacing: Theme.sp(8)
                Btn { text: "Normal"; hint: "n" }
                Btn { text: "Primary"; kind: "primary"; glyph: Icons.plus; hint: "t" }
                Btn { text: "Danger"; kind: "danger"; glyph: Icons.trash }
                Btn { text: "Flat"; kind: "flat" }
                Btn { kind: "icon"; glyph: Icons.cog }
                Btn { text: "Disabled"; enabled: false }
                Segmented { options: [{ value: "a", label: "Date" }, { value: "b", label: "Project" }, { value: "c", label: "Collaborator" }]; current: "a" }
            }
            RowLayout {
                spacing: Theme.sp(10)
                Check { checked: false }
                Check { checked: true }
                Check { pendingUntil: Date.now() + 9000 }
                Ring { until: Date.now() + 10000 }
                Avatar { initials: "AN" }
                Avatar { initials: "JS"; dim: true }
                DueBadge { text: "Sep 28"; tone: "overdue" }
                DueBadge { text: "Today"; tone: "today" }
                DueBadge { text: "Oct 12" }
                DueBadge { text: "Oct 1"; tone: "done" }
                Chip { text: "Design review"; glyph: Icons.link; clickable: true }
                Chip { text: "Done thing"; done: true }
                Chip { text: "In progress"; glyph: Icons.hourglass; ink: Theme.yellow; border.color: Theme.yellowBorder }
                Kbd { text: "g d" }
            }
            RowLayout {
                spacing: Theme.sp(10)
                Field { placeholderText: "A field"; Layout.preferredWidth: Theme.sp(240) }
                Field { text: "Invalid"; invalid: true; Layout.preferredWidth: Theme.sp(200) }
                Picker { Layout.preferredWidth: Theme.sp(320); placeholder: "Picker"; selected: [{ id: 1, name: "Ann" }, { id: 2, name: "Bob" }] }
            }
            NotesArea { Layout.preferredWidth: Theme.sp(520); text: "**Bold**, *italic* and a [link](https://omarchy.org)\n\n- one\n- two" }
            Toast { Layout.topMargin: Theme.sp(10); Component.onCompleted: show("Saved. This is a toast.", false) }
        }
    }
}
