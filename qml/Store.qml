pragma Singleton
import QtQuick
import Omniscient

// App-wide state and the one place QML talks to Rust. Pages and dialogs
// call act()/query(); Main.qml listens to the open* signals to show
// dialogs.
QtObject {
    id: store

    readonly property var s: parse(App.state)
    readonly property string page: s.page || "tasks"

    function parse(json) {
        try {
            return JSON.parse(json)
        } catch (e) {
            return ({})
        }
    }

    signal toast(string text, bool error)
    signal openTask(int id)
    signal newTask(var defaults)
    signal openProject(int id)
    signal newProject()
    signal openSettings(bool addPerson)
    signal openPerson(int id)
    signal openSearch()
    signal openShortcuts()
    signal confirm(string title, string text, string action, var onYes)

    // Runs an action; errors become a toast. Returns the result or null.
    function act(name, args) {
        const r = parse(App.act(name, JSON.stringify(args || {})))
        if (r.error !== undefined) {
            toast(r.error, true)
            return null
        }
        return r.ok
    }

    // Runs an action and hands back {ok} or {error} (forms show errors inline).
    function tryAct(name, args) {
        return parse(App.act(name, JSON.stringify(args || {})))
    }

    function query(name, args) {
        return parse(App.query(name, JSON.stringify(args || {})))
    }

    function go(page, id) {
        act("go", id === undefined ? { page: page } : { page: page, id: id })
    }

    function openLink(url) {
        if (url)
            Qt.openUrlExternally(url)
    }
}
