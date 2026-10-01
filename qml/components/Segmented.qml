import QtQuick
import QtQuick.Layouts
import Omniscient

// A row of mutually exclusive options: [{value, label, hint}].
Rectangle {
    id: root
    property var options: []
    property string current: ""
    signal picked(string value)

    implicitWidth: row.implicitWidth + 2
    implicitHeight: row.implicitHeight + 2
    radius: Theme.radius
    color: Theme.normalFill
    border.width: Theme.borderWidth
    border.color: Theme.normalBorder

    RowLayout {
        id: row
        anchors.centerIn: parent
        spacing: 0
        Repeater {
            model: root.options
            Btn {
                required property var modelData
                kind: "flat"
                text: modelData.label
                hint: modelData.hint || ""
                active: root.current === modelData.value
                font.bold: active
                topPadding: Theme.sp(3)
                bottomPadding: Theme.sp(3)
                leftPadding: Theme.sp(10)
                rightPadding: Theme.sp(10)
                onClicked: root.picked(modelData.value)
            }
        }
    }
}
