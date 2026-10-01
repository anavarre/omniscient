import QtQuick
import QtQuick.Controls.Basic
import Omniscient

// A Nerd Font icon: set `text` to one of Icons.*.
Label {
    color: Theme.fgDim
    font.pixelSize: Theme.body
    verticalAlignment: Text.AlignVCenter
    horizontalAlignment: Text.AlignHCenter
    Behavior on color { ColorAnimation { duration: Theme.quick } }
}
