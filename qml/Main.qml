import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import Qt.labs.qmlmodels
import Omniscient

ApplicationWindow {
    id: win
    width: 1180
    height: 780
    minimumWidth: 420
    minimumHeight: 360
    visible: true
    title: (Store.s.title ? Store.s.title + " — " : "") + "Omniscient"
    color: Theme.bg
    font.family: Theme.fontFamily
    font.pixelSize: Theme.body

    palette {
        window: Theme.bg
        windowText: Theme.fg
        base: Theme.bg
        alternateBase: Theme.card
        text: Theme.fg
        button: Theme.normalFill
        buttonText: Theme.fg
        highlight: Theme.selection
        highlightedText: Theme.fg
        placeholderText: Theme.placeholder
        toolTipBase: Theme.tooltipBg
        toolTipText: Theme.fg
        mid: Theme.hairline
        dark: Theme.normalBorder
        light: Theme.hoverFill
    }

    readonly property bool compact: width < Theme.sp(1100)
    readonly property bool narrow: width < Theme.sp(760)
    property bool sidebarOpen: true
    readonly property bool modalOpen: modals.children.length > 0
    readonly property var s: Store.s

    // ---------------------------------------------------------------- keys

    // One table drives both the key handling and the shortcuts help.
    readonly property var keymap: [
        { group: "Navigate", keys: ["h"], label: "Tasks (home)", run: () => Store.go("tasks") },
        { group: "Navigate", keys: ["p"], label: "Projects", run: () => Store.go("projects") },
        { group: "Navigate", keys: ["j", "↓"], label: "Next row", run: () => moveSelection(1) },
        { group: "Navigate", keys: ["k", "↑"], label: "Previous row", run: () => moveSelection(-1) },
        { group: "Navigate", keys: ["g g", "Home"], label: "Top", run: () => jump(false) },
        { group: "Navigate", keys: ["G", "End"], label: "Bottom", run: () => jump(true) },
        { group: "Navigate", keys: ["Enter"], label: "Open selected", run: () => activateSelected() },
        { group: "Navigate", keys: ["Esc"], label: "Close dialog / back to projects", run: () => back() },
        { group: "Navigate", keys: ["b"], label: "Toggle sidebar", run: () => win.narrow ? drawer.open() : win.sidebarOpen = !win.sidebarOpen },
        { group: "Tasks", keys: ["t", "Ctrl+N"], label: "New task", run: () => Store.newTask({}) },
        { group: "Tasks", keys: ["x"], label: "Complete / undo selected", run: () => onSelected("task", d => Store.act("toggle", { id: d.id })) },
        { group: "Tasks", keys: ["i"], label: "Mark in progress", run: () => onSelected("task", d => Store.act("progress", { id: d.id })) },
        { group: "Tasks", keys: ["o"], label: "Open link", run: () => onSelected("", d => Store.openLink(d.link)) },
        { group: "Tasks", keys: ["l", "→"], label: "Show sub-tasks", run: () => onSelected("task", d => expand(d, true)) },
        { group: "Tasks", keys: ["←"], label: "Hide sub-tasks", run: () => onSelected("task", d => expand(d, false)) },
        { group: "Tasks", keys: ["Space"], label: "Toggle sub-tasks", run: () => onSelected("task", d => expand(d, undefined)) },
        { group: "Tasks", keys: ["J", "K"], label: "Move sub-task down / up", run: null },
        { group: "Views", keys: ["g d"], label: "Group by date", run: () => Store.act("group", { by: "date" }) },
        { group: "Views", keys: ["g p"], label: "Group by project", run: () => Store.act("group", { by: "project" }) },
        { group: "Views", keys: ["g c"], label: "Group by collaborator", run: () => Store.act("group", { by: "person" }) },
        { group: "Views", keys: ["g m"], label: "Mine", run: () => Store.act("filter", { mine: true }) },
        { group: "Views", keys: ["g a"], label: "All", run: () => Store.act("filter", { mine: false }) },
        { group: "Projects", keys: ["P"], label: "New project", run: () => { Store.go("projects"); Store.newProject() } },
        { group: "Projects", keys: ["e"], label: "Edit project", run: () => editContextProject() },
        { group: "Projects", keys: ["a"], label: "Archive selected project", run: () => onSelected("project", d => Store.act("archive", { id: d.id })) },
        { group: "App", keys: ["f", "/", "Ctrl+K"], label: "Search", run: () => Store.openSearch() },
        { group: "App", keys: ["s"], label: "Settings & collaborators", run: () => Store.openSettings(false) },
        { group: "App", keys: ["C"], label: "Add collaborator", run: () => Store.openSettings(true) },
        { group: "App", keys: ["?"], label: "Shortcuts", run: () => Store.openShortcuts() },
        { group: "Dialogs", keys: ["Shift+Enter"], label: "Save", run: null },
        { group: "Dialogs", keys: ["Ctrl+E"], label: "Preview notes", run: null }
    ]

    function binding(keys) {
        return keymap.find(b => b.keys.indexOf(keys) >= 0 && b.run)
    }

    property bool gPending: false
    Timer { id: gTimer; interval: 1200; onTriggered: win.gPending = false }

    function handleKey(event) {
        if (event.modifiers & (Qt.AltModifier | Qt.MetaModifier))
            return false
        if (event.modifiers & Qt.ControlModifier)
            return false // Ctrl shortcuts are Shortcut items below.
        const t = event.text
        if (gPending) {
            gPending = false
            gTimer.stop()
            const b = binding("g " + t)
            if (b)
                b.run()
            return true
        }
        let name = t
        switch (event.key) {
        case Qt.Key_Down: name = "↓"; break
        case Qt.Key_Up: name = "↑"; break
        case Qt.Key_Right: name = "→"; break
        case Qt.Key_Left: name = "←"; break
        case Qt.Key_Home: name = "Home"; break
        case Qt.Key_End: name = "End"; break
        case Qt.Key_Space: name = "Space"; break
        case Qt.Key_Return:
        case Qt.Key_Enter: name = "Enter"; break
        case Qt.Key_Escape: name = "Esc"; break
        }
        if (name === "g") {
            gPending = true
            gTimer.restart()
            return true
        }
        if (name === "J" || name === "K") {
            moveSubtask(name === "J" ? 1 : -1)
            return true
        }
        const b = binding(name)
        if (!b)
            return false
        b.run()
        return true
    }

    Shortcut { sequences: ["Ctrl+K", "Ctrl+F"]; enabled: !win.modalOpen; onActivated: Store.openSearch() }
    Shortcut { sequence: "Ctrl+N"; enabled: !win.modalOpen; onActivated: Store.newTask({}) }
    Shortcut { sequence: "Ctrl+Q"; onActivated: win.close() }

    // ----------------------------------------------------------- selection

    function rowAt(i) {
        const j = App.rowAt(i)
        return j === "" ? null : JSON.parse(j)
    }
    function selectable(d) {
        return d && d.kind !== "section" && d.kind !== "empty"
    }
    function moveSelection(step) {
        const n = list.count
        let i = list.currentIndex
        for (let k = 0; k < n; k++) {
            const next = i + step
            if (next < 0 || next >= n)
                return
            i = next
            if (selectable(rowAt(i))) {
                list.currentIndex = i
                return
            }
        }
    }
    function selectFirst() {
        for (let i = 0; i < list.count; i++) {
            if (selectable(rowAt(i))) {
                list.currentIndex = i
                return
            }
        }
        list.currentIndex = -1
    }
    function jump(toEnd) {
        if (toEnd) {
            for (let i = list.count - 1; i >= 0; i--)
                if (selectable(rowAt(i))) { list.currentIndex = i; break }
            list.positionViewAtEnd()
        } else {
            selectFirst()
            list.positionViewAtBeginning()
        }
    }
    function current() {
        return list.currentIndex >= 0 ? rowAt(list.currentIndex) : null
    }
    function onSelected(kind, fn) {
        const d = current()
        if (d && (kind === "" || d.kind === kind))
            fn(d)
    }
    function expand(d, open) {
        if (d.subtasks > 0)
            Store.act("expand", open === undefined ? { id: d.id } : { id: d.id, open: open })
        else if (d.parentId !== null && open === false) {
            // ← on a sub-task folds its parent and selects it.
            Store.act("expand", { id: d.parentId, open: false })
            for (let i = list.currentIndex; i >= 0; i--) {
                const r = rowAt(i)
                if (r && r.kind === "task" && r.id === d.parentId) { list.currentIndex = i; break }
            }
        }
    }
    function activateSelected() {
        const d = current()
        if (!d)
            return
        if (d.kind === "task")
            Store.openTask(d.id)
        else if (d.kind === "project")
            Store.go("project", d.id)
        else if (d.kind === "more")
            Store.act("reveal", { section: d.section })
        else if (d.kind === "completedToggle")
            Store.act("toggleCompleted")
        else if (d.kind === "archivedToggle")
            Store.act("toggleArchived")
    }
    function moveSubtask(step) {
        const d = current()
        if (!d || d.kind !== "task" || !d.siblings)
            return
        const ids = d.siblings.slice()
        const at = ids.indexOf(d.id)
        const to = at + step
        if (to < 0 || to >= ids.length)
            return
        ids.splice(at, 1)
        ids.splice(to, 0, d.id)
        Store.act("reorder", { ids: ids })
        list.currentIndex += step // the row moved with the data
    }
    // Mouse drag of a sub-task handle: drop onto a sibling to take its place.
    function dragSubtask(index, sceneY, done) {
        const d = rowAt(index)
        if (!d)
            return
        const p = list.mapFromItem(null, 0, sceneY)
        if (!done) {
            dragGhost.taskId = d.id
            dragGhost.label = d.what
            dragGhost.y = listArea.y + p.y - dragGhost.height / 2
            return
        }
        dragGhost.taskId = -1
        const target = rowAt(list.indexAt(list.width / 2 + list.contentX, p.y + list.contentY))
        if (!target || target.kind !== "task" || !d.siblings || target.parentId !== d.parentId || target.id === d.id)
            return
        const after = d.siblings.indexOf(target.id) > d.siblings.indexOf(d.id)
        const ids = d.siblings.filter(x => x !== d.id)
        ids.splice(ids.indexOf(target.id) + (after ? 1 : 0), 0, d.id)
        Store.act("reorder", { ids: ids })
    }
    function editContextProject() {
        if (Store.page === "project")
            Store.openProject(s.projectId)
        else
            onSelected("project", d => Store.openProject(d.id))
    }
    function back() {
        if (Store.page === "project")
            Store.go("projects")
    }

    // Page switches start at the top with the first row selected.
    property string lastPage: ""
    onSChanged: {
        const key = (s.page || "") + ":" + (s.projectId || "")
        if (key !== lastPage) {
            lastPage = key
            Qt.callLater(() => { selectFirst(); list.positionViewAtBeginning() })
        }
        if (s.toast && s.toast.seq !== lastToast) {
            lastToast = s.toast.seq
            toast.show(s.toast.text, s.toast.error)
        }
    }
    property int lastToast: 0

    // --------------------------------------------------------------- layout

    // Everything visible lives in `stage`, so a theme switch can snapshot it.
    Item {
    id: stage
    anchors.fill: parent

    FocusScope {
        id: keyRoot
        anchors.fill: parent
        focus: !win.modalOpen
        visible: !App.gallery && !Store.s.fatal
        Keys.onPressed: event => { event.accepted = win.handleKey(event) }

        RowLayout {
            anchors.fill: parent
            spacing: 0

            Sidebar {
                id: sidebar
                Layout.fillHeight: true
                Layout.preferredWidth: win.sidebarOpen && !win.narrow ? Theme.sp(win.compact ? 200 : 236) : 0
                visible: Layout.preferredWidth > 0
                clip: true
                Behavior on Layout.preferredWidth { NumberAnimation { duration: 160; easing.type: Easing.OutCubic } }
            }

            Item {
                id: listArea
                Layout.fillWidth: true
                Layout.fillHeight: true
                clip: true

            ListView {
                id: list
                anchors.fill: parent
                anchors.leftMargin: gutter
                anchors.rightMargin: gutter
                clip: false
                model: App
                currentIndex: -1
                reuseItems: true
                cacheBuffer: 600
                boundsBehavior: Flickable.StopAtBounds
                flickDeceleration: 4000
                maximumFlickVelocity: 6000
                highlightFollowsCurrentItem: false
                keyNavigationEnabled: false
                readonly property int gutter: Theme.sp(win.narrow ? 14 : win.compact ? 22 : 34)
                bottomMargin: Theme.sp(48)
                topMargin: Theme.sp(24)

                // Keep the selection in view, smoothly.
                onCurrentIndexChanged: Qt.callLater(ensureVisible)
                function ensureVisible() {
                    const item = itemAtIndex(currentIndex)
                    if (!item)
                        return positionViewAtIndex(currentIndex, ListView.Contain)
                    const margin = Theme.sp(48)
                    const top = item.y - margin
                    const bottom = item.y + item.height + margin - height
                    if (contentY > top)
                        scrollTo(Math.max(top, originY - topMargin))
                    else if (contentY < bottom)
                        scrollTo(bottom)
                }
                function scrollTo(y) {
                    scroll.to = y
                    scroll.restart()
                }
                NumberAnimation { id: scroll; target: list; property: "contentY"; duration: 140; easing.type: Easing.OutCubic }

                header: PageHeader {
                    width: list.width
                    narrow: win.narrow
                    onMenu: drawer.open()
                }

                delegate: DelegateChooser {
                    role: "kind"
                    DelegateChoice {
                        roleValue: "task"
                        TaskRow {
                            required property string row
                            d: JSON.parse(row)
                            width: list.width
                            isCurrent: ListView.isCurrentItem
                            narrow: win.narrow
                            compact: win.compact
                            onSelect: list.currentIndex = index
                            onDragged: (sceneY, done) => { list.currentIndex = index; win.dragSubtask(index, sceneY, done) }
                        }
                    }
                    DelegateChoice {
                        roleValue: "section"
                        SectionRow {
                            required property string row
                            d: JSON.parse(row)
                            width: list.width
                        }
                    }
                    DelegateChoice {
                        roleValue: "project"
                        ProjectRow {
                            required property string row
                            d: JSON.parse(row)
                            width: list.width
                            isCurrent: ListView.isCurrentItem
                            onSelect: list.currentIndex = index
                        }
                    }
                    DelegateChoice {
                        MiscRow {
                            required property string row
                            required property int index
                            d: JSON.parse(row)
                            width: list.width
                            isCurrent: ListView.isCurrentItem
                            onSelect: list.currentIndex = index
                        }
                    }
                }

                // Rows glide into place as the model changes.
                add: Transition {
                    NumberAnimation { property: "opacity"; from: 0; to: 1; duration: 160; easing.type: Easing.OutCubic }
                }
                remove: Transition {
                    NumberAnimation { property: "opacity"; to: 0; duration: 120 }
                }
                displaced: Transition {
                    NumberAnimation { properties: "y"; duration: 180; easing.type: Easing.OutCubic }
                    NumberAnimation { property: "opacity"; to: 1; duration: 120 }
                }
                move: Transition {
                    NumberAnimation { properties: "y"; duration: 200; easing.type: Easing.OutCubic }
                }

                ScrollBar.vertical: ScrollBar {
                    id: vbar
                    parent: listArea
                    y: 0
                    height: listArea.height
                    x: listArea.width - (list.gutter + width) / 2
                    policy: ScrollBar.AsNeeded
                    contentItem: Rectangle {
                        implicitWidth: Theme.sp(4)
                        radius: Theme.smallRadius
                        color: vbar.pressed || vbar.hovered ? Theme.scrollbarHover : Theme.scrollbar
                        opacity: vbar.active ? 1 : 0
                        Behavior on opacity { NumberAnimation { duration: 300 } }
                    }
                    background: null
                }
            }
            } // listArea
        }

        // Sub-task drag: a ghost follows the pointer; dropping reorders.
        Rectangle {
            id: dragGhost
            property int taskId: -1
            property int fromIndex: -1
            property string label: ""
            visible: taskId >= 0
            x: listArea.x + list.x + Theme.sp(30)
            width: list.width - Theme.sp(30)
            height: ghostText.implicitHeight + Theme.sp(14)
            radius: Theme.radius
            color: Theme.popupBg
            border.width: 1
            border.color: Theme.accent
            opacity: 0.92
            z: 50
            Txt { id: ghostText; anchors.verticalCenter: parent.verticalCenter; x: Theme.sp(12); text: Icons.drag + "  " + dragGhost.label }
        }
    }

    // Narrow windows (a third of a screen, tiled) slide the sidebar in.
    Drawer {
        id: drawer
        edge: Qt.LeftEdge
        width: Math.min(Theme.sp(260), win.width * 0.8)
        height: win.height
        interactive: win.narrow
        modal: true
        background: Rectangle { color: Theme.bgDark }
        Overlay.modal: Rectangle { color: Theme.scrim }
        Sidebar { anchors.fill: parent }
        Connections {
            target: Store
            function onPageChanged() { drawer.close() }
        }
    }
    Connections {
        target: win
        function onNarrowChanged() { if (!win.narrow) drawer.close() }
    }

    // ------------------------------------------------------------- overlays

    Item {
        id: modals
        anchors.fill: parent
        z: 10
        // Modal.qml instances are created here and destroy themselves.
    }
    } // stage

    function openModal(component, props) {
        const m = component.createObject(modals, props || {})
        if (!m)
            console.warn("cannot open dialog:", component.errorString())
        m.closed.connect(() => Qt.callLater(() => { if (!win.modalOpen) keyRoot.forceActiveFocus() }))
        return m
    }

    Component { id: taskEditor; TaskEditor {} }
    Component { id: projectEditor; ProjectEditor {} }
    Component { id: settings; Settings {} }
    Component { id: personDetail; PersonDetail {} }
    Component { id: search; SearchPalette {} }
    Component { id: shortcuts; Shortcuts {} }
    Component { id: confirm; Confirm {} }

    Connections {
        target: Store
        function onOpenTask(id) { win.openModal(taskEditor, { taskId: id }) }
        function onNewTask(defaults) { win.openModal(taskEditor, { defaults: defaults }) }
        function onOpenProject(id) { win.openModal(projectEditor, { projectId: id }) }
        function onNewProject() { win.openModal(projectEditor, {}) }
        function onOpenSettings(addPerson) { win.openModal(settings, { addPerson: addPerson }) }
        function onOpenPerson(id) { win.openModal(personDetail, { personId: id }) }
        function onOpenSearch() { win.openModal(search, {}) }
        function onOpenShortcuts() { win.openModal(shortcuts, { keymap: win.keymap }) }
        function onConfirm(title, text, action, onYes) { win.openModal(confirm, { title: title, text: text, action: action, onYes: onYes }) }
        function onToast(text, error) { toast.show(text, error) }
    }

    Toast {
        id: toast
        anchors.horizontalCenter: parent.horizontalCenter
        anchors.bottom: parent.bottom
        anchors.bottomMargin: Theme.sp(18) + y
        z: 20
    }

    Loader {
        anchors.fill: parent
        active: App.gallery
        source: "Gallery.qml"
    }

    Label {
        visible: !!Store.s.fatal
        anchors.centerIn: parent
        width: parent.width * 0.7
        wrapMode: Text.Wrap
        horizontalAlignment: Text.AlignHCenter
        text: Store.s.fatal || ""
        color: Theme.red
    }

    // ------------------------------------------------------ theme switch

    // Omarchy switched theme/font/size: freeze the current frame on top,
    // apply the new tokens beneath it, and dissolve the old frame away.
    Connections {
        target: Omarchy
        function onPendingChanged() {
            const next = Omarchy.pending
            if (next === "" || next === Omarchy.tokens)
                return
            if (!win.visible || win.width <= 0) {
                Omarchy.tokens = next
                return
            }
            // Platforms that never deliver the grab (offscreen) just switch.
            themeFallback.next = next
            themeFallback.restart()
            const asked = stage.grabToImage(result => {
                if (!themeFallback.running)
                    return
                themeFallback.stop()
                snapshot.source = result.url
                snapshot.opacity = 1
                Omarchy.tokens = next
                dissolve.restart()
            })
            if (!asked)
                themeFallback.triggered()
        }
    }
    Timer {
        id: themeFallback
        property string next: ""
        interval: 120
        onTriggered: { stop(); Omarchy.tokens = next }
    }
    Image {
        id: snapshot
        anchors.fill: parent
        z: 1000
        opacity: 0
        visible: opacity > 0
        cache: false
        NumberAnimation on opacity { id: dissolve; running: false; to: 0; duration: Theme.transition; easing.type: Easing.InOutQuad }
    }

    // ------------------------------------------------------- housekeeping

    // Commits undo countdowns when their window closes.
    Timer {
        readonly property var deadline: win.s.nextDeadline
        running: deadline !== null && deadline !== undefined
        interval: running ? Math.max(16, Math.min(deadline + 10, 1000)) : 1000
        repeat: true
        onTriggered: App.act("tick", "{}")
    }
    // Coming back to the window picks up a new day.
    onActiveChanged: if (active && s.today && s.today !== new Date().toISOString().slice(0, 10)) Store.act("refresh")
    onClosing: App.act("flush", "{}")
    Connections {
        target: App
        function onQuitRequested() { Qt.quit() }
    }

    // ------------------------------------------------------------ replay

    // OMNISCIENT_DEBUG_KEYS="j k x type:hello S-Return Escape …" sends real
    // key events one at a time, then commits and quits (smoke tests).
    property var replayTokens: []
    Timer {
        id: replayTimer
        interval: 140
        repeat: true
        onTriggered: {
            if (win.replayTokens.length === 0) {
                stop()
                if (App.screenshot !== "" && !App.grab(App.screenshot))
                    console.warn("screenshot failed:", App.screenshot)
                App.act("flush", "{}")
                Qt.callLater(Qt.quit)
                return
            }
            win.sendToken(win.replayTokens.shift())
        }
    }
    Component.onCompleted: {
        const g = App.geometry.split("x").map(Number)
        if (g.length === 2 && g[0] > 0 && g[1] > 0) {
            width = g[0]
            height = g[1]
        }
        requestActivate()
        if (App.replay !== "") {
            replayTokens = App.replay.trim().split(/\s+/)
            replayTimer.start()
        }
    }
    function keyFor(ch) {
        const code = ch.toUpperCase().charCodeAt(0)
        if (ch >= "a" && ch <= "z") return { key: Qt.Key_A + (code - 65), mods: 0, text: ch }
        if (ch >= "A" && ch <= "Z") return { key: Qt.Key_A + (code - 65), mods: Qt.ShiftModifier, text: ch }
        if (ch >= "0" && ch <= "9") return { key: Qt.Key_0 + (ch.charCodeAt(0) - 48), mods: 0, text: ch }
        const named = { " ": Qt.Key_Space, "?": Qt.Key_Question, "/": Qt.Key_Slash, "-": Qt.Key_Minus, "+": Qt.Key_Plus, ".": Qt.Key_Period, ",": Qt.Key_Comma }
        return { key: named[ch] || Qt.Key_unknown, mods: ch === "?" || ch === "+" ? Qt.ShiftModifier : 0, text: ch }
    }
    function sendToken(tok) {
        if (tok === "wait")
            return
        if (tok.startsWith("type:")) {
            for (const ch of tok.slice(5).replace(/_/g, " ")) {
                const k = keyFor(ch)
                App.postKey(k.key, k.mods, k.text)
            }
            return
        }
        let mods = 0
        while (/^[SCA]-./.test(tok)) {
            mods |= tok[0] === "S" ? Qt.ShiftModifier : tok[0] === "C" ? Qt.ControlModifier : Qt.AltModifier
            tok = tok.slice(2)
        }
        const named = { Return: Qt.Key_Return, Escape: Qt.Key_Escape, Down: Qt.Key_Down, Up: Qt.Key_Up, Left: Qt.Key_Left,
                        Right: Qt.Key_Right, Tab: Qt.Key_Tab, space: Qt.Key_Space, Home: Qt.Key_Home, End: Qt.Key_End,
                        BackSpace: Qt.Key_Backspace, question: Qt.Key_Question, slash: Qt.Key_Slash }
        if (named[tok] !== undefined) {
            const text = tok === "space" ? " " : tok === "question" ? "?" : tok === "slash" ? "/" : tok === "Return" ? "\r" : tok === "Escape" ? "\u001b" : ""
            App.postKey(named[tok], mods | (tok === "question" ? Qt.ShiftModifier : 0), text)
            return
        }
        const k = keyFor(tok)
        App.postKey(k.key, k.mods | mods, mods & Qt.ControlModifier ? "" : k.text)
    }
}
