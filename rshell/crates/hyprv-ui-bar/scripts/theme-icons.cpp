// Test fixture host only: use the same QIcon provider as parity/host/wayland_mocks.h.
#include <QApplication>
#include <QIcon>
#include <QDir>
#include <QStringList>
int main(int argc,char**argv){QApplication app(argc,argv);if(argc!=5)return 2;const QString destination=QString::fromLocal8Bit(argv[1]);const int size=QString::fromLocal8Bit(argv[2]).toInt();QIcon::setThemeSearchPaths(QStringList{QDir::homePath()+"/.local/share/icons",QDir::homePath()+"/.icons","/usr/local/share/icons","/usr/share/icons"});QIcon::setFallbackSearchPaths(QStringList{"/usr/share/pixmaps"});QIcon::setThemeName(QString::fromLocal8Bit(argv[3]));QDir().mkpath(destination);for(const QString&name:QStringList{"google-chrome","discord","keepassxc","fcitx","wechat","telegram","applications-system","steam","thunderbird","spotify","firefox","code","vlc","obs","gimp"}){const auto pixmap=QIcon::fromTheme(name).pixmap(QSize(size,size),QString::fromLocal8Bit(argv[4]).toDouble());if(!pixmap.isNull()&&!pixmap.save(destination+"/"+name+".png"))return 3;}return 0;}
