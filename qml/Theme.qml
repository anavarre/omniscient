pragma Singleton
import QtQuick
import Omniscient

// The live Omarchy theme as animated properties. Everything on screen is
// painted from these; nothing is hard-coded. Values come from the Rust
// `Omarchy` singleton (colors.toml + shell.toml + fontconfig + Hyprland),
// which re-publishes on every `omarchy-theme-set` / font / text-size change.
// (No Behaviors here: Main.qml cross-fades a snapshot instead, like the
// Omarchy shell's theme transition.)
QtObject {
    id: root

    readonly property var raw: {
        try {
            return JSON.parse(Omarchy.tokens)
        } catch (e) {
            return ({})
        }
    }
    function c(key, fallback) {
        return raw[key] !== undefined ? raw[key] : fallback
    }
    function n(key, fallback) {
        return raw[key] !== undefined ? raw[key] : fallback
    }
    function f(key, fallback) {
        return raw.fonts && raw.fonts[key] !== undefined ? raw.fonts[key] : fallback
    }

    // Theme switches cross-fade like the Omarchy shell's.
    readonly property int transition: 260
    // Interaction feedback.
    readonly property int quick: 120

    readonly property string name: n("name", "")
    readonly property bool dark: n("dark", true)
    readonly property string fontFamily: n("fontFamily", "monospace")
    readonly property real spacingScale: n("spacingScale", 1)
    readonly property real fontBase: n("fontBase", 12)
    readonly property int radius: n("radius", 0)
    readonly property int smallRadius: Math.min(radius, 4)
    readonly property int borderWidth: n("borderWidth", 1)

    // The shell's type scale. App body sits a step above the bar's 12px:
    // task titles are read, not glanced at.
    readonly property int caption: f("bodySmall", 11)
    readonly property int small: f("body", 12)
    readonly property int body: f("title", 14)
    readonly property int heading: f("heading", 16)
    readonly property int display: f("display", 24)
    readonly property int iconLarge: f("iconLarge", 18)

    // The shell's Style.space(px).
    function sp(px) {
        return Math.max(1, Math.round(px * spacingScale))
    }

    property color bg: c("bg", "#101315")
    property color bgDark: c("bgDark", "#0c0e10")
    property color fg: c("fg", "#cacccc")
    property color fgDim: c("fgDim", "#8a8f94")
    property color muted: c("muted", "#707880")
    property color accent: c("accent", "#cacccc")
    property color onAccent: c("onAccent", "#101315")
    property color red: c("red", "#e06c75")
    property color orange: c("orange", "#d19a66")
    property color yellow: c("yellow", "#e5c07b")
    property color green: c("green", "#98c379")
    property color hairline: c("hairline", "#262a2d")
    property color card: c("card", "#121518")
    property color normalFill: c("normalFill", "#0acacccc")
    property color hoverFill: c("hoverFill", "#14cacccc")
    property color focusFill: c("focusFill", "#14cacccc")
    property color selectedFill: c("selectedFill", "#2ecacccc")
    property color pressedFill: c("pressedFill", "#38cacccc")
    property color selection: c("selection", "#59cacccc")
    property color normalBorder: c("normalBorder", "#66cacccc")
    property color hoverBorder: c("hoverBorder", "#40cacccc")
    property color hoverBorderStrong: c("hoverBorderStrong", "#80cacccc")
    property color focusBorder: c("focusBorder", "#40cacccc")
    property color focusBorderStrong: c("focusBorderStrong", "#cccacccc")
    property color popupBg: c("popupBg", "#101315")
    property color popupBorder: c("popupBorder", "#cacccc")
    property color tooltipBg: c("tooltipBg", "#f7101315")
    property color menuSelectedText: c("menuSelectedText", "#cacccc")
    property color menuSelectedFill: c("menuSelectedFill", "#1fcacccc")
    property color scrim: c("scrim", "#80101315")
    property color shadow: c("shadow", "#73000000")
    property color placeholder: c("placeholder", "#6b6e70")
    property color doneText: c("doneText", "#767879")
    property color redSoft: c("redSoft", "#1ae06c75")
    property color redBorder: c("redBorder", "#80e06c75")
    property color yellowBorder: c("yellowBorder", "#73e5c07b")
    property color accentHover: c("accentHover", "#cacccc")
    property color accentPressed: c("accentPressed", "#aaaaaa")
    property color subtaskFill: c("subtaskFill", "#05cacccc")
    property color scrollbar: c("scrollbar", "#38cacccc")
    property color scrollbarHover: c("scrollbarHover", "#66cacccc")
}
