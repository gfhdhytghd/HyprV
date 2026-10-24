// Fixture oracle only: QQuickPixmap's QImageReader scaled-size path.
#include <QImageReader>
#include <QImage>
#include <QString>
int main(int argc,char**argv){if(argc!=4)return 2;QImageReader reader(QString::fromLocal8Bit(argv[1]));int size=QString::fromLocal8Bit(argv[3]).toInt();reader.setScaledSize(QSize(size,size));const auto image=reader.read();return image.save(QString::fromLocal8Bit(argv[2]))?0:3;}
