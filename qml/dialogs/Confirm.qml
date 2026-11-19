import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import Omniscient

Modal {
    id: root
    property string text: ""
    property string action: "OK"
    property var callback: null
    glyph: Icons.warning
    cardWidth: Theme.sp(440)
    onAccepted: yes()
    initialFocus: yesButton

    function yes() {
        if (closing)
            return
        if (callback)
            callback()
        close()
    }
    Keys.onReturnPressed: yes()
    Keys.onEnterPressed: yes()

    Txt { text: root.text; wrapMode: Text.Wrap; elide: Text.ElideNone; color: Theme.fgDim; Layout.fillWidth: true }

    footer: [
        Item { Layout.fillWidth: true },
        Btn { text: "Cancel"; kind: "flat"; hint: "Esc"; onClicked: root.close() },
        Btn { id: yesButton; text: root.action; kind: "danger"; hint: "↵"; focus: true; onClicked: root.yes() }
    ]
}
