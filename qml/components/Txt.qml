import QtQuick
import QtQuick.Controls.Basic
import Omniscient

// Body text in the window font (inherits family; size/colour from Theme).
Label {
    color: Theme.fg
    font.pixelSize: Theme.body
    elide: Text.ElideRight
    textFormat: Text.PlainText
    verticalAlignment: Text.AlignVCenter
}
