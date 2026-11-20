#pragma once
// Sends a real key press/release to the focused window, so scripted smoke
// runs exercise exactly the paths a keyboard does.
#include <QtCore/QString>
#include <QtGui/QGuiApplication>
#include <QtGui/QKeyEvent>
#include <QtGui/QWindow>

inline void omniscientPostKey(int key, int modifiers, const QString& text)
{
    QWindow* window = QGuiApplication::focusWindow();
    if (!window && !QGuiApplication::topLevelWindows().isEmpty())
        window = QGuiApplication::topLevelWindows().first();
    if (!window)
        return;
    const auto mods = Qt::KeyboardModifiers(modifiers);
    QKeyEvent press(QEvent::KeyPress, key, mods, text);
    QGuiApplication::sendEvent(window, &press);
    QKeyEvent release(QEvent::KeyRelease, key, mods, text);
    QGuiApplication::sendEvent(window, &release);
}

#include <QtGui/QMouseEvent>
#include <QtGui/QPointingDevice>

// Sends a real mouse move (and press/release when `click`) at window
// coordinates, so replays can exercise hover and tap paths.
inline void omniscientPostMouse(double x, double y, bool click)
{
    QWindow* window = QGuiApplication::focusWindow();
    if (!window && !QGuiApplication::topLevelWindows().isEmpty())
        window = QGuiApplication::topLevelWindows().first();
    if (!window)
        return;
    const QPointF local(x, y);
    const QPointF global = window->mapToGlobal(local);
    const auto* dev = QPointingDevice::primaryPointingDevice();
    QMouseEvent move(QEvent::MouseMove, local, local, global, Qt::NoButton, Qt::NoButton, Qt::NoModifier, dev);
    QGuiApplication::sendEvent(window, &move);
    if (!click)
        return;
    QMouseEvent press(QEvent::MouseButtonPress, local, local, global, Qt::LeftButton, Qt::LeftButton, Qt::NoModifier, dev);
    QGuiApplication::sendEvent(window, &press);
    QMouseEvent release(QEvent::MouseButtonRelease, local, local, global, Qt::LeftButton, Qt::NoButton, Qt::NoModifier, dev);
    QGuiApplication::sendEvent(window, &release);
}

#include <QtQuick/QQuickWindow>

// Renders the window and saves it (screenshots for smoke runs; works on the
// offscreen platform, where Item.grabToImage never completes).
inline bool omniscientGrab(const QString& path)
{
    for (QWindow* w : QGuiApplication::topLevelWindows()) {
        if (auto* quick = qobject_cast<QQuickWindow*>(w))
            return quick->grabWindow().save(path);
    }
    return false;
}
