// Offline diagnostic only. Never linked into the Qt-free shell.
// Qt 6.12 QIcon::pixmap overload used by Quickshell's IconImage provider.
#include <QGuiApplication>
#include <QIcon>
#include <QDebug>
#include <QDir>
#include <cstdio>
int main(int argc, char **argv) {
    QGuiApplication app(argc, argv);
    QIcon::setThemeSearchPaths({QDir::homePath()+"/.local/share/icons", "/usr/local/share/icons", "/usr/share/icons"});
    QIcon::setFallbackSearchPaths({"/usr/share/pixmaps"});
    QIcon::setThemeName("hicolor");
    for (auto p : QIcon::themeSearchPaths()) fprintf(stdout, "root %s\n", qPrintable(p));
    if (argc != 2) return 2;
    QDir().mkpath(argv[1]);
    for (auto name : {"google-chrome", "discord", "keepassxc", "fcitx", "wechat"}) {
        QIcon icon = QIcon::fromTheme(name);
        for (int scale : {1, 2}) {
            auto p = icon.pixmap(QSize(36, 36), scale);
            fprintf(stdout,"%s request36 explicitDpr%d %dx%d dpr%.3f null%d\n",name,scale,p.width(),p.height(),p.devicePixelRatio(),icon.isNull());
            p.save(QString("%1/%2-dpr%3.png").arg(argv[1], name).arg(scale));
        }
        auto p = icon.pixmap(36, 36);
        fprintf(stdout,"%s defaultDpr %dx%d dpr%.3f\n",name,p.width(),p.height(),p.devicePixelRatio());
        p.save(QString("%1/%2-default.png").arg(argv[1], name));
    }
}
