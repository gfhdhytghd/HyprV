// Mock of the Quickshell.Wayland module. It needs C++ because QML cannot
// declare attached properties (WlrLayershell.layer: ... on a PanelWindow).
#pragma once

#include <QObject>
#include <QString>
#include <QIcon>
#include <cstdio>
#include <QJSEngine>
#include <QQmlEngine>
#include <QQuickImageProvider>
#include <QtQml/qqml.h>

class WlrLayer : public QObject
{
    Q_OBJECT
public:
    enum Enum { Background = 0, Bottom = 1, Top = 2, Overlay = 3 };
    Q_ENUM(Enum)
};

class WlrKeyboardFocus : public QObject
{
    Q_OBJECT
public:
    enum Enum { None = 0, Exclusive = 1, OnDemand = 2 };
    Q_ENUM(Enum)
};

class WlrLayershellAttached : public QObject
{
    Q_OBJECT
    Q_PROPERTY(int layer MEMBER m_layer NOTIFY changed)
    Q_PROPERTY(QString namespace MEMBER m_namespace NOTIFY changed)
    Q_PROPERTY(int keyboardFocus MEMBER m_keyboardFocus NOTIFY changed)
public:
    using QObject::QObject;
signals:
    void changed();

private:
    int m_layer = WlrLayer::Top;
    QString m_namespace = QStringLiteral("quickshell");
    int m_keyboardFocus = WlrKeyboardFocus::None;
};

class WlrLayershell : public QObject
{
    Q_OBJECT
    QML_ATTACHED(WlrLayershellAttached)
public:
    using QObject::QObject;
    static WlrLayershellAttached *qmlAttachedProperties(QObject *object) { return new WlrLayershellAttached(object); }
};

// Lets the scene read a window's attached WlrLayershell object (QML cannot
// read attached properties of another object).
class LayershellInspector : public QObject
{
    Q_OBJECT
public:
    using QObject::QObject;
    Q_INVOKABLE QObject *layershellOf(QObject *object) const
    {
        return object ? qmlAttachedPropertiesObject<WlrLayershell>(object, true) : nullptr;
    }
};

// image://icon/<name>: theme icons, as Quickshell's IconImageProvider serves them:
// QIcon::fromTheme(name).pixmap(requestedSize) (default 100x100). Qt already passes
// requestedSize in device pixels (sourceSize x DPR, i.e. 36 for an 18-px tray icon at @2),
// and the live shell resolves the pixmap at device pixel ratio 1 -- measured by spec-panels
// on nested live-shell captures (chrome: hicolor 32x32, never upscaled; discord: 256 -> 36):
// docs/requests/parity-capture.md. Passing the DPR again would yield 64-72 px pixmaps.
class ThemeIconProvider : public QQuickImageProvider
{
public:
    ThemeIconProvider() : QQuickImageProvider(QQuickImageProvider::Pixmap) {}
    QPixmap requestPixmap(const QString &id, QSize *size, const QSize &requested) override
    {
        const QString name = id.section(QLatin1Char('?'), 0, 0);
        QSize want = requested.isValid() ? requested : QSize(100, 100);
        if (want.width() == 0 || want.height() == 0)
            want = QSize(2, 2);
        const QIcon icon = QIcon::fromTheme(name);
        QPixmap pixmap = icon.isNull() ? QPixmap() : icon.pixmap(want, 1.0);
        if (size)
            *size = pixmap.size();
        if (qEnvironmentVariableIsSet("PARITY_DEBUG"))
            std::fprintf(stderr, "parity-host: icon %s requested %dx%d -> pixmap %dx%d\n", qPrintable(name),
                         requested.width(), requested.height(), pixmap.width(), pixmap.height());
        return pixmap;
    }
};

inline void registerWaylandMocks()
{
    const char *uri = "Quickshell.Wayland";
    qmlRegisterUncreatableType<WlrLayershell>(uri, 1, 0, "WlrLayershell", "attached only");
    qmlRegisterUncreatableType<WlrLayer>(uri, 1, 0, "WlrLayer", "enum");
    qmlRegisterUncreatableType<WlrKeyboardFocus>(uri, 1, 0, "WlrKeyboardFocus", "enum");
    qmlRegisterSingletonType(uri, 1, 0, "ToplevelManager", [](QQmlEngine *, QJSEngine *js) -> QJSValue {
        QJSValue toplevels = js->newObject();
        toplevels.setProperty(QStringLiteral("values"), js->newArray());
        QJSValue manager = js->newObject();
        manager.setProperty(QStringLiteral("toplevels"), toplevels);
        return manager;
    });
}
