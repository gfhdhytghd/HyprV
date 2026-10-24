// Offline independent installed-QPainter oracle; never linked into Rust.
#include <QFile>
#include <QJsonDocument>
#include <QJsonObject>
#include <QJsonArray>
#include <QImage>
#include <QPainter>
#include <QPainterPath>
#include <QPainterPathStroker>
#include <cstdio>
QColor color(QJsonValue v){auto a=v.toArray();return QColor::fromRgbF(a[0].toDouble(),a[1].toDouble(),a[2].toDouble(),a[3].toDouble());}
int main(int argc,char**argv){
 QFile f(argv[1]);if(!f.open(QFile::ReadOnly))return 1;
 auto j=QJsonDocument::fromJson(f.readAll()).object();double scale=atof(argv[3]);
 QImage image(qRound(j["width"].toDouble()*scale),qRound(j["height"].toDouble()*scale),QImage::Format_ARGB32_Premultiplied);image.fill(Qt::transparent);
 QPainter painter(&image);painter.setRenderHint(QPainter::Antialiasing);painter.scale(scale,scale);
 int index=0;
 for(auto value:j["operations"].toArray()){
  auto op=value.toObject();auto e=op["path"].toArray();QPainterPath path;
  for(int i=0;i<e.size();i++){auto elem=e[i].toArray();int type=elem[0].toInt();auto pt=elem[1].toArray();double x=pt[0].toDouble(),y=pt[1].toDouble();if(type==0)path.moveTo(x,y);else if(type==1)path.lineTo(x,y);else if(type==2){auto b=e[++i].toArray()[1].toArray(),c=e[++i].toArray()[1].toArray();path.cubicTo(x,y,b[0].toDouble(),b[1].toDouble(),c[0].toDouble(),c[1].toDouble());}}
  if(!op["fill"].isNull())painter.fillPath(path,color(op["fill"]));
  if(!op["gradient"].isNull()){auto g=op["gradient"].toObject();QLinearGradient gradient(0,g["y0"].toDouble(),0,g["y1"].toDouble());gradient.setColorAt(0,color(g["top"]));gradient.setColorAt(1,color(g["bottom"]));painter.fillPath(path,gradient);}
  if(!op["stroke"].isNull()){auto s=op["stroke"].toObject();QPen pen(color(s["color"]),s["width"].toDouble());pen.setCapStyle(s["cap"].toString()=="Round"?Qt::RoundCap:Qt::FlatCap);pen.setJoinStyle(s["join"].toString()=="Round"?Qt::RoundJoin:Qt::MiterJoin);painter.strokePath(path,pen);
   if(index==5){QPainterPathStroker stroker(pen);auto outline=stroker.createStroke(path);QJsonArray points;for(int n=0;n<outline.elementCount();n++){auto e=outline.elementAt(n);points.append(QJsonArray{int(e.type),e.x,e.y});}QFile out(QString::fromLocal8Bit(argv[2])+".outline.json");out.open(QFile::WriteOnly);out.write(QJsonDocument(points).toJson());}
  }index++;
 }
 painter.end();return image.save(argv[2])?0:2;
}
