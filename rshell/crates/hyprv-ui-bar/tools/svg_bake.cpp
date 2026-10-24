// Offline only. QtSvg/QPainter -> immutable PNGs; never linked into Rust.
#include <QGuiApplication>
#include <QSvgRenderer>
#include <QImage>
#include <QFile>
#include <QPainter>
#include <QPainterPath>
int main(int argc,char**argv){
 QGuiApplication app(argc,argv);if(argc!=6)return 2;
 const QString source=QString::fromLocal8Bit(argv[1]);
 const int width=QString::fromLocal8Bit(argv[3]).toInt(),height=QString::fromLocal8Bit(argv[4]).toInt();
 const qreal scale=QString::fromLocal8Bit(argv[5]).toDouble();
 QImage image(qRound(width*scale),qRound(height*scale),QImage::Format_RGBA8888_Premultiplied);image.fill(Qt::transparent);
 QPainter painter(&image);painter.setRenderHints(QPainter::Antialiasing|QPainter::SmoothPixmapTransform);
 if(source.startsWith("@corner-")){
  // ScreenCornerShade.qml: Canvas's path, including its transform BEFORE rasterization.
  painter.scale(scale,scale);
  if(source.contains("right")){painter.translate(width,0);painter.scale(-1,1);}
  if(source.contains("bottom")){painter.translate(0,height);painter.scale(1,-1);}
  QPainterPath path;path.moveTo(0,0);path.lineTo(width,0);path.arcTo(0,0,width*2,height*2,90,90);path.lineTo(0,0);path.closeSubpath();
  painter.fillPath(path,Qt::black);
 }else{
  QSvgRenderer renderer;renderer.setOptions(QtSvg::DisableAnimations);if(!renderer.load(source))return 3;
  renderer.render(&painter,QRectF(0,0,image.width(),image.height()));
 }
 painter.end();
 if(qEnvironmentVariableIsSet("HYPRV_BAKE_RAW")){QFile raw(qEnvironmentVariable("HYPRV_BAKE_RAW"));if(raw.open(QIODevice::WriteOnly))raw.write(reinterpret_cast<const char*>(image.constBits()),image.sizeInBytes());}
 return image.save(QString::fromLocal8Bit(argv[2]))?0:4;
}
