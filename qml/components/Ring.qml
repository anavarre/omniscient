import QtQuick
import QtQuick.Shapes
import Omniscient

// Countdown ring for the 10-second undo window. Animates on the render
// thread from the time left until `until` (ms since epoch) down to zero.
Item {
    id: root
    property double until: 0
    property int total: 10000
    property color color: Theme.accent
    property real progress: 0   // 1 → 0
    implicitWidth: Theme.sp(16)
    implicitHeight: Theme.sp(16)

    function restart() {
        anim.stop()
        const left = Math.max(0, root.until - Date.now())
        root.progress = Math.min(1, left / root.total)
        anim.duration = left
        anim.start()
    }
    onUntilChanged: if (until > 0) restart()
    Component.onCompleted: if (until > 0) restart()

    NumberAnimation { id: anim; target: root; property: "progress"; to: 0 }

    Shape {
        anchors.fill: parent
        preferredRendererType: Shape.CurveRenderer
        ShapePath {
            fillColor: "transparent"
            strokeColor: Theme.hairline
            strokeWidth: Math.max(1.5, Theme.sp(2))
            PathAngleArc {
                centerX: root.width / 2; centerY: root.height / 2
                radiusX: root.width / 2 - 1.5; radiusY: root.height / 2 - 1.5
                startAngle: 0; sweepAngle: 360
            }
        }
        ShapePath {
            fillColor: "transparent"
            strokeColor: root.color
            strokeWidth: Math.max(1.5, Theme.sp(2))
            capStyle: ShapePath.RoundCap
            PathAngleArc {
                centerX: root.width / 2; centerY: root.height / 2
                radiusX: root.width / 2 - 1.5; radiusY: root.height / 2 - 1.5
                startAngle: -90; sweepAngle: -360 * root.progress
            }
        }
    }
}
