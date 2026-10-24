// Offline diagnostic only; inspect the installed Qt shader precision contract.
#include <QGuiApplication>
#include <QQuickWindow>
#include <QFile>
#include <QDir>
int main(int argc,char **argv) {
 QGuiApplication app(argc,argv);QQuickWindow window;
 if(argc!=2)return 2;QDir().mkpath(argv[1]);
 for(auto name:{"textmask.frag","texture.frag","opaquetexture.frag","smoothcolor.vert","smoothcolor.frag"}) {
  const auto target=QString::fromLocal8Bit(argv[1])+"/"+name+".qsb";QFile::remove(target);
  if(!QFile::copy(":/qt-project.org/scenegraph/shaders_ng/"+QString(name)+".qsb",target))return 3;
 }
}
